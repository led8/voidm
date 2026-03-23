use anyhow::{bail, Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::config::Config;
use crate::models::{
    AddMemoryRequest, AddMemoryResponse, ConflictWarning, DuplicateWarning, EdgeType, LinkResponse,
    Memory,
};
use crate::{auto_tagger, embeddings, quality, redactor, search, vector};

/// Resolve a full or short (prefix) ID to a full memory ID.
/// - If `id` is already a full UUID that exists → return it as-is.
/// - If `id` is a prefix → find all matches; error if 0 or >1.
/// - Minimum prefix length: 4 characters.
pub async fn resolve_id(pool: &SqlitePool, id: &str) -> Result<String> {
    // Exact match first (fast path)
    let exact: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some(full) = exact {
        return Ok(full);
    }

    if id.len() < 4 {
        bail!("ID prefix '{}' is too short (minimum 4 characters)", id);
    }

    // Prefix search — LIKE 'prefix%'
    let pattern = format!("{}%", id);
    let matches: Vec<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id LIKE ?")
        .bind(&pattern)
        .fetch_all(pool)
        .await?;

    match matches.len() {
        0 => bail!("Memory '{}' not found", id),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => bail!(
            "Ambiguous short ID '{}' matches {} memories. Use more characters:\n{}",
            id,
            n,
            matches
                .iter()
                .map(|m| format!("  {}", m))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

/// Add a memory — full workflow:
/// 1. Compute embedding + quality_score (outside tx)
/// 2. BEGIN tx
/// 3. Insert memory + scopes + FTS + vec + graph node + links
/// 4. COMMIT
/// Returns AddMemoryResponse with suggested_links and duplicate_warning.
pub async fn add_memory(
    pool: &SqlitePool,
    mut req: AddMemoryRequest,
    config: &Config,
) -> Result<AddMemoryResponse> {
    // Auto-enrich tags BEFORE creating tags_json (moved to beginning)
    if let Err(e) = auto_tagger::enrich_memory_tags(&mut req, config) {
        tracing::warn!(
            "Failed to auto-enrich tags: {}. Using user-provided tags only.",
            e
        );
    }

    // Redact secrets from memory content and metadata BEFORE insertion
    let mut redaction_warnings = Vec::new();
    if let Err(e) = redact_memory(&mut req, config, &mut redaction_warnings) {
        tracing::warn!(
            "Failed to redact secrets: {}. Continuing without redaction.",
            e
        );
    }

    // Log any redacted secrets to inform user
    for warning in &redaction_warnings {
        tracing::warn!(
            "Redacted {} {}(s) in memory.{}: {}",
            warning.count,
            warning.pattern_type,
            warning.field,
            warning.count
        );
    }

    let id = req.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = Utc::now().to_rfc3339();

    let tags_json = serde_json::to_string(&req.tags)?;
    let metadata_json = serde_json::to_string(&req.metadata)?;
    let memory_type_str = req.memory_type.to_string();

    // 1. Compute embedding OUTSIDE transaction
    let embedding_result = if config.embeddings.enabled {
        match embeddings::embed_text(&config.embeddings.model, &req.content) {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!(
                    "Failed to compute embedding: {}. Skipping vector storage.",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    // Compute quality score OUTSIDE transaction (will persist to DB)
    let memory_type_enum = req.memory_type.clone();
    let quality = quality::compute_quality_score(&req.content, &memory_type_enum);

    // Ensure vec_memories table exists with correct dimension
    if let Some(ref emb) = embedding_result {
        let dim = emb.len();
        vector::ensure_vector_table(pool, dim).await?;
        // Record in db_meta
        sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_model', ?)")
            .bind(&config.embeddings.model)
            .execute(pool)
            .await?;
        sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_dim', ?)")
            .bind(dim.to_string())
            .execute(pool)
            .await?;
    }

    // Validate --link targets exist before opening transaction
    for link in &req.links {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
            .bind(&link.target_id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            anyhow::bail!("Link target '{}' not found", link.target_id);
        }
        if link.edge_type.requires_note() && link.note.is_none() {
            anyhow::bail!(
                "RELATES_TO requires --note explaining why no stronger relationship applies."
            );
        }
    }

    // 2–8: Atomic transaction
    let mut tx = pool.begin().await?;

    // Insert memory with persistent quality_score
    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, quality_score, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&memory_type_str)
    .bind(&req.content)
    .bind(req.importance)
    .bind(&tags_json)
    .bind(&metadata_json)
    .bind(quality.score)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .context("Failed to insert memory")?;

    // Insert scopes
    for scope in &req.scopes {
        sqlx::query("INSERT OR IGNORE INTO memory_scopes (memory_id, scope) VALUES (?, ?)")
            .bind(&id)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
    }

    // Insert FTS
    sqlx::query("INSERT INTO memories_fts (id, content) VALUES (?, ?)")
        .bind(&id)
        .bind(&req.content)
        .execute(&mut *tx)
        .await?;

    // Insert embedding
    if let Some(ref emb) = embedding_result {
        let bytes: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query("INSERT INTO vec_memories (memory_id, embedding) VALUES (?, ?)")
            .bind(&id)
            .bind(&bytes)
            .execute(&mut *tx)
            .await
            .context("Failed to insert into vec_memories")?;
    }

    // Graph node upsert
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let node_id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO graph_node_labels (node_id, label) VALUES (?, 'Memory')")
        .bind(node_id)
        .execute(&mut *tx)
        .await?;

    // Store memory_type as a text property on the graph node
    let key_id = intern_property_key(&mut tx, "memory_type").await?;
    sqlx::query(
        "INSERT OR REPLACE INTO graph_node_props_text (node_id, key_id, value) VALUES (?, ?, ?)",
    )
    .bind(node_id)
    .bind(key_id)
    .bind(&memory_type_str)
    .execute(&mut *tx)
    .await?;

    // Create --link edges
    for link in &req.links {
        let target_node: i64 =
            sqlx::query_scalar("SELECT n.id FROM graph_nodes n WHERE n.memory_id = ?")
                .bind(&link.target_id)
                .fetch_one(&mut *tx)
                .await
                .with_context(|| format!("Graph node not found for target '{}'", link.target_id))?;

        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(node_id)
        .bind(target_node)
        .bind(link.edge_type.as_str())
        .bind(&link.note)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await.context("Transaction commit failed")?;

    // Post-insert: Auto-link memories with shared tags
    if !req.tags.is_empty() {
        let tag_limit = config.insert.auto_link_limit;
        if let Err(e) = crate::tag_linker::auto_link_by_tags(pool, &id, &req.tags, tag_limit).await
        {
            tracing::warn!(
                "Failed to auto-link by tags: {}. Continuing with memory creation.",
                e
            );
        }
    }

    // Post-insert: compute suggested_links and duplicate_warning (outside tx)
    let (suggested_links, duplicate_warning) = if let Some(ref emb) = embedding_result {
        let dup_candidates =
            search::find_similar(pool, emb, &id, 1, config.insert.duplicate_threshold)
                .await
                .unwrap_or_default();

        let dup_warning = if let Some((dup_id, dup_score)) = dup_candidates.first() {
            if let Ok(Some(dup_mem)) = get_memory(pool, dup_id).await {
                let content_trunc = if dup_mem.content.len() > 120 {
                    format!("{}...", crate::search::safe_truncate(&dup_mem.content, 120))
                } else {
                    dup_mem.content.clone()
                };
                Some(DuplicateWarning {
                    id: dup_id.clone(),
                    score: *dup_score,
                    content: content_trunc,
                    message: "Near-duplicate detected. Consider linking instead of inserting."
                        .into(),
                })
            } else {
                None
            }
        } else {
            None
        };

        let link_candidates = search::find_similar(
            pool,
            emb,
            &id,
            config.insert.auto_link_limit,
            config.insert.auto_link_threshold,
        )
        .await
        .unwrap_or_default();

        let suggested = search::build_suggested_links(pool, &memory_type_str, link_candidates)
            .await
            .unwrap_or_default();

        (suggested, dup_warning)
    } else {
        (vec![], None)
    };

    Ok(AddMemoryResponse {
        id,
        memory_type: memory_type_str,
        content: req.content,
        scopes: req.scopes,
        tags: req.tags,
        importance: req.importance,
        created_at: now,
        quality_score: Some(quality.score),
        suggested_links,
        duplicate_warning,
    })
}

/// Get a single memory by ID.
pub async fn get_memory(pool: &SqlitePool, id: &str) -> Result<Option<Memory>> {
    let row: Option<(String, String, String, i64, String, String, Option<f32>, String, String)> = sqlx::query_as(
        "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
         FROM memories WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    if let Some((
        id,
        memory_type,
        content,
        importance,
        tags_json,
        metadata_json,
        quality_score_db,
        created_at,
        updated_at,
    )) = row
    {
        let scopes = get_scopes(pool, &id).await?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        // Use persisted quality_score if available, otherwise compute and return
        let quality_score = if let Some(score) = quality_score_db {
            Some(score)
        } else {
            let memory_type_enum: crate::models::MemoryType = memory_type
                .parse()
                .unwrap_or(crate::models::MemoryType::Semantic);
            let quality_score_val = quality::compute_quality_score(&content, &memory_type_enum);
            Some(quality_score_val.score)
        };

        Ok(Some(Memory {
            id,
            memory_type,
            content,
            importance,
            tags,
            metadata,
            scopes,
            created_at,
            updated_at,
            quality_score,
        }))
    } else {
        Ok(None)
    }
}

/// List memories newest-first, with optional scope prefix and type filter.
pub async fn list_memories(
    pool: &SqlitePool,
    scope_filter: Option<&str>,
    type_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<Memory>> {
    // Build query dynamically
    let rows: Vec<(
        String,
        String,
        String,
        i64,
        String,
        String,
        Option<f32>,
        String,
        String,
    )> = if let Some(scope) = scope_filter {
        let scope_prefix = format!("{}%", scope);
        sqlx::query_as(
            "SELECT DISTINCT m.id, m.type, m.content, m.importance, m.tags, m.metadata, m.quality_score, m.created_at, m.updated_at
             FROM memories m
             JOIN memory_scopes ms ON ms.memory_id = m.id
             WHERE ms.scope LIKE ?
             ORDER BY m.created_at DESC LIMIT ?"
        )
        .bind(&scope_prefix)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
             FROM memories ORDER BY created_at DESC LIMIT ?"
        )
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    };

    let mut memories = Vec::new();
    for (
        id,
        memory_type,
        content,
        importance,
        tags_json,
        metadata_json,
        quality_score_db,
        created_at,
        updated_at,
    ) in rows
    {
        if let Some(t) = type_filter {
            if memory_type != t {
                continue;
            }
        }
        let scopes = get_scopes(pool, &id).await?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap_or_default();

        // Use persisted quality_score if available, otherwise compute
        let quality_score = if let Some(score) = quality_score_db {
            Some(score)
        } else {
            let memory_type_enum: crate::models::MemoryType = memory_type
                .parse()
                .unwrap_or(crate::models::MemoryType::Semantic);
            let quality_score_val = quality::compute_quality_score(&content, &memory_type_enum);
            Some(quality_score_val.score)
        };

        memories.push(Memory {
            id,
            memory_type,
            content,
            importance,
            tags,
            metadata,
            scopes,
            created_at,
            updated_at,
            quality_score,
        });
    }
    Ok(memories)
}

/// Delete a memory and all its graph edges (cascade via FK).
pub async fn delete_memory(pool: &SqlitePool, id: &str) -> Result<bool> {
    // FTS5: manual delete required
    sqlx::query("DELETE FROM memories_fts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    // vec_memories: best-effort delete
    let _ = sqlx::query("DELETE FROM vec_memories WHERE memory_id = ?")
        .bind(id)
        .execute(pool)
        .await;

    let result = sqlx::query("DELETE FROM memories WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Get all scopes for a memory.
async fn get_scopes(pool: &SqlitePool, memory_id: &str) -> Result<Vec<String>> {
    let scopes: Vec<String> =
        sqlx::query_scalar("SELECT scope FROM memory_scopes WHERE memory_id = ? ORDER BY scope")
            .bind(memory_id)
            .fetch_all(pool)
            .await?;
    Ok(scopes)
}

/// List all known scope strings.
pub async fn list_scopes(pool: &SqlitePool) -> Result<Vec<String>> {
    let scopes: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT scope FROM memory_scopes ORDER BY scope")
            .fetch_all(pool)
            .await?;
    Ok(scopes)
}

/// Create a graph edge between two memories.
pub async fn link_memories(
    pool: &SqlitePool,
    from_id: &str,
    edge_type: &EdgeType,
    to_id: &str,
    note: Option<&str>,
) -> Result<LinkResponse> {
    // Validate RELATES_TO requires note
    if edge_type.requires_note() && note.is_none() {
        anyhow::bail!(
            "RELATES_TO requires --note explaining why no stronger relationship applies."
        );
    }

    // Check both memories exist
    for id in &[from_id, to_id] {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            anyhow::bail!("Memory '{}' not found", id);
        }
    }

    // Get or create graph nodes
    let from_node = get_or_create_node(pool, from_id).await?;
    let to_node = get_or_create_node(pool, to_id).await?;

    // Check for conflicting edge
    let conflict_rel = edge_type.conflict();
    let conflict_warning = if let Some(opposing) = conflict_rel {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?",
        )
        .bind(from_node)
        .bind(to_node)
        .bind(opposing)
        .fetch_optional(pool)
        .await?;

        if exists.is_some() {
            Some(ConflictWarning {
                existing_rel: opposing.to_string(),
                message: format!(
                    "Conflict: {} {} {} already exists. Both edges are now present. Use 'voidm unlink {} {} {}' to resolve.",
                    from_id, opposing, to_id, from_id, opposing, to_id
                ),
            })
        } else {
            None
        }
    } else {
        None
    };

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(from_node)
    .bind(to_node)
    .bind(edge_type.as_str())
    .bind(note)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(LinkResponse {
        created: true,
        from: from_id.to_string(),
        rel: edge_type.as_str().to_string(),
        to: to_id.to_string(),
        conflict_warning,
    })
}

/// Remove a graph edge.
pub async fn unlink_memories(
    pool: &SqlitePool,
    from_id: &str,
    edge_type: &EdgeType,
    to_id: &str,
) -> Result<bool> {
    let from_node: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
            .bind(from_id)
            .fetch_optional(pool)
            .await?;
    let to_node: Option<i64> = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(to_id)
        .fetch_optional(pool)
        .await?;

    match (from_node, to_node) {
        (Some(f), Some(t)) => {
            let result = sqlx::query(
                "DELETE FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?",
            )
            .bind(f)
            .bind(t)
            .bind(edge_type.as_str())
            .execute(pool)
            .await?;
            Ok(result.rows_affected() > 0)
        }
        _ => Ok(false),
    }
}

async fn get_or_create_node(pool: &SqlitePool, memory_id: &str) -> Result<i64> {
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(memory_id)
        .execute(pool)
        .await?;
    let node_id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;
    Ok(node_id)
}

async fn intern_property_key(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &str,
) -> Result<i64> {
    sqlx::query("INSERT OR IGNORE INTO graph_property_keys (key) VALUES (?)")
        .bind(key)
        .execute(&mut **tx)
        .await?;
    let id: i64 = sqlx::query_scalar("SELECT id FROM graph_property_keys WHERE key = ?")
        .bind(key)
        .fetch_one(&mut **tx)
        .await?;
    Ok(id)
}

/// Check model mismatch against db_meta.
pub async fn check_model_mismatch(
    pool: &SqlitePool,
    configured_model: &str,
) -> Result<Option<(String, String)>> {
    let db_model: Option<String> =
        sqlx::query_scalar("SELECT value FROM db_meta WHERE key = 'embedding_model'")
            .fetch_optional(pool)
            .await?;
    let db_dim: Option<String> =
        sqlx::query_scalar("SELECT value FROM db_meta WHERE key = 'embedding_dim'")
            .fetch_optional(pool)
            .await?;

    if let Some(db_m) = db_model {
        if db_m != configured_model {
            return Ok(Some((db_m, db_dim.unwrap_or_else(|| "?".into()))));
        }
    }
    Ok(None)
}

/// List all memory-to-memory edges for migration purposes
pub async fn list_edges(pool: &SqlitePool) -> Result<Vec<crate::models::MemoryEdge>> {
    // Get all edges with their source and target memory IDs
    let edges_data: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT gn1.memory_id, gn2.memory_id, ge.rel_type, ge.note
        FROM graph_edges ge
        JOIN graph_nodes gn1 ON ge.source_id = gn1.id
        JOIN graph_nodes gn2 ON ge.target_id = gn2.id
        ORDER BY ge.created_at
        "#,
    )
    .fetch_all(pool)
    .await?;

    let edges = edges_data
        .into_iter()
        .map(
            |(from_id, to_id, rel_type, note)| crate::models::MemoryEdge {
                from_id,
                to_id,
                rel_type,
                note,
            },
        )
        .collect();

    Ok(edges)
}

/// List all ontology edges for migration purposes
pub async fn list_ontology_edges(
    pool: &SqlitePool,
) -> Result<Vec<crate::models::OntologyEdgeForMigration>> {
    let edges_data: Vec<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT from_id, from_type, to_id, to_type, rel_type, note
        FROM ontology_edges
        ORDER BY from_id
        "#,
    )
    .fetch_all(pool)
    .await?;

    let edges = edges_data
        .into_iter()
        .map(|(from_id, from_type, to_id, to_type, rel_type, note)| {
            crate::models::OntologyEdgeForMigration {
                from_id,
                from_type,
                to_id,
                to_type,
                rel_type,
                note,
            }
        })
        .collect();

    Ok(edges)
}

/// Redact secrets from memory content, tags, and metadata in-place.
/// Returns list of redaction warnings.
fn redact_memory(
    req: &mut AddMemoryRequest,
    config: &Config,
    warnings: &mut Vec<redactor::RedactionWarning>,
) -> Result<()> {
    // Redact content
    let (redacted_content, content_warnings) =
        redactor::redact_text(&req.content, &config.redaction);
    for mut w in content_warnings {
        w.field = "content".to_string();
        warnings.push(w);
    }
    req.content = redacted_content;

    // Redact tags
    let mut redacted_tags = Vec::new();
    for tag in &req.tags {
        let (redacted_tag, tag_warnings) = redactor::redact_text(tag, &config.redaction);
        for mut w in tag_warnings {
            w.field = "tags".to_string();
            warnings.push(w);
        }
        redacted_tags.push(redacted_tag);
    }
    req.tags = redacted_tags;

    // Redact metadata
    if let Ok(metadata_str) = serde_json::to_string(&req.metadata) {
        let (redacted_metadata_str, metadata_warnings) =
            redactor::redact_text(&metadata_str, &config.redaction);
        for mut w in metadata_warnings {
            w.field = "metadata".to_string();
            warnings.push(w);
        }
        if let Ok(redacted_metadata) = serde_json::from_str(&redacted_metadata_str) {
            req.metadata = redacted_metadata;
        }
    }

    Ok(())
}
