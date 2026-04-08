use anyhow::{bail, Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::config::Config;
use crate::models::{
    AddMemoryRequest, AddMemoryResponse, ConflictWarning, DuplicateWarning, EdgeType, LinkResponse,
    Memory,
};
use crate::{auto_tagger, chunking, embeddings, quality, redactor, search, vector};

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

    // Truncate title to 200 chars if provided
    let title = req.title.as_deref().map(|t| {
        if t.len() > 200 {
            t[..200].to_string()
        } else {
            t.to_string()
        }
    });

    // Insert memory with persistent quality_score
    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, quality_score, created_at, updated_at, title, context)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
    .bind(&title)
    .bind(&req.context)
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

    // Post-insert: chunk the content and store chunk-level embeddings
    if config.embeddings.enabled && config.chunking.enabled {
        let chunks = chunking::chunk_text(&req.content, &config.chunking);
        if chunks.len() > 1 {
            // Only worth chunking if we got more than one chunk
            if let Err(e) = store_chunks(pool, &id, &chunks, &config.embeddings.model, &now).await {
                tracing::warn!("Failed to store chunk embeddings: {}. Continuing.", e);
            }
        }
    }

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
        title,
        context: req.context,
    })
}

/// Store chunk rows and their embeddings for a memory.
async fn store_chunks(
    pool: &SqlitePool,
    memory_id: &str,
    chunks: &[String],
    model_name: &str,
    now: &str,
) -> Result<()> {
    // Embed all chunks in one batch
    let embeddings = embeddings::embed_batch(model_name, chunks)?;
    let dim = embeddings.first().map(|e| e.len()).unwrap_or(0);
    if dim == 0 {
        return Ok(());
    }
    vector::ensure_chunk_vector_table(pool, dim).await?;

    for (i, (content, emb)) in chunks.iter().zip(embeddings.iter()).enumerate() {
        let chunk_id = format!("{}_{}", memory_id, i);
        sqlx::query(
            "INSERT OR REPLACE INTO chunks (id, memory_id, chunk_index, content, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&chunk_id)
        .bind(memory_id)
        .bind(i as i64)
        .bind(content)
        .bind(now)
        .execute(pool)
        .await?;

        vector::store_chunk_embedding(pool, &chunk_id, emb).await?;
    }
    Ok(())
}

/// Delete all chunks (rows + embeddings) for a memory.
async fn delete_chunks(pool: &SqlitePool, memory_id: &str) -> Result<()> {
    // ON DELETE CASCADE handles chunks table; still clean vec_chunks
    vector::delete_chunk_embeddings(pool, memory_id).await?;
    // Also delete rows explicitly (cascade should handle it, but be safe)
    let _ = sqlx::query("DELETE FROM chunks WHERE memory_id = ?")
        .bind(memory_id)
        .execute(pool)
        .await;
    Ok(())
}

/// Get a single memory by ID.
pub async fn get_memory(pool: &SqlitePool, id: &str) -> Result<Option<Memory>> {
    // Touch last_accessed_at (best-effort, non-blocking on failure)
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query("UPDATE memories SET last_accessed_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await;

    let row: Option<(String, String, String, i64, String, String, Option<f32>, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at, title, context
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
        title,
        context,
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
            title,
            context,
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
        Option<String>,
        Option<String>,
    )> = if let Some(scope) = scope_filter {
        let scope_prefix = format!("{}%", scope);
        sqlx::query_as(
            "SELECT DISTINCT m.id, m.type, m.content, m.importance, m.tags, m.metadata, m.quality_score, m.created_at, m.updated_at, m.title, m.context
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
            "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at, title, context
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
        title,
        context,
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
            title,
            context,
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

    // vec_chunks: delete chunk embeddings (chunks table rows cascade from memories delete)
    let _ = delete_chunks(pool, id).await;

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
/// Find CONTRADICTS edges among a given set of memory IDs.
/// Returns Vec of (edge_id, from_memory_id, to_memory_id, note).
pub async fn find_contradicts_among(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<(i64, String, String, Option<String>)>> {
    if ids.len() < 2 {
        return Ok(vec![]);
    }

    // Build placeholders: (?, ?, ...)
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT e.id, n_from.memory_id, n_to.memory_id, e.note
         FROM graph_edges e
         JOIN graph_nodes n_from ON n_from.id = e.source_id
         JOIN graph_nodes n_to   ON n_to.id   = e.target_id
         WHERE e.rel_type = 'CONTRADICTS'
           AND n_from.memory_id IN ({placeholders})
           AND n_to.memory_id   IN ({placeholders})"
    );

    let mut q = sqlx::query_as::<_, (i64, String, String, Option<String>)>(&sql);
    // bind IDs twice (once for each IN clause)
    for id in ids {
        q = q.bind(id);
    }
    for id in ids {
        q = q.bind(id);
    }

    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

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

/// Get all graph edges (outgoing and incoming) for a specific memory.
/// Returns Vec of (direction, other_memory_id, rel_type, note)
/// where direction is "outgoing" or "incoming".
pub async fn get_edges_for_memory(
    pool: &SqlitePool,
    memory_id: &str,
) -> Result<Vec<(String, String, String, Option<String>)>> {
    let outgoing: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT n_to.memory_id, ge.rel_type, ge.note
         FROM graph_edges ge
         JOIN graph_nodes n_from ON n_from.id = ge.source_id
         JOIN graph_nodes n_to   ON n_to.id   = ge.target_id
         WHERE n_from.memory_id = ?
         ORDER BY ge.rel_type, n_to.memory_id",
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await?;

    let incoming: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT n_from.memory_id, ge.rel_type, ge.note
         FROM graph_edges ge
         JOIN graph_nodes n_from ON n_from.id = ge.source_id
         JOIN graph_nodes n_to   ON n_to.id   = ge.target_id
         WHERE n_to.memory_id = ?
         ORDER BY ge.rel_type, n_from.memory_id",
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for (other_id, rel_type, note) in outgoing {
        result.push(("outgoing".to_string(), other_id, rel_type, note));
    }
    for (other_id, rel_type, note) in incoming {
        result.push(("incoming".to_string(), other_id, rel_type, note));
    }
    Ok(result)
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

// ── Update memory ─────────────────────────────────────────────────────────────

/// Partial patch for updating a memory in-place.
pub struct UpdateMemoryPatch {
    pub content: Option<String>,
    pub memory_type: Option<crate::models::MemoryType>,
    pub tags: Option<Vec<String>>,
    pub importance: Option<i64>,
    pub title: Option<String>,
    pub context: Option<String>,
}

/// Update a memory in-place, preserving its ID and all graph edges.
/// Returns the updated Memory record.
pub async fn update_memory(
    pool: &SqlitePool,
    id: &str,
    patch: UpdateMemoryPatch,
    config: &Config,
) -> Result<Memory> {
    let full_id = resolve_id(pool, id).await?;
    let current = get_memory(pool, &full_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Memory '{}' not found", full_id))?;

    if patch.content.is_none()
        && patch.memory_type.is_none()
        && patch.tags.is_none()
        && patch.importance.is_none()
        && patch.title.is_none()
        && patch.context.is_none()
    {
        anyhow::bail!("No fields to update. Provide at least one of: --content, --type, --tags, --importance, --title, --context");
    }

    let new_content = patch.content.unwrap_or(current.content.clone());
    let new_type_str = patch
        .memory_type
        .as_ref()
        .map(|t| t.to_string())
        .unwrap_or(current.memory_type.clone());
    let new_tags = patch.tags.unwrap_or(current.tags.clone());
    let new_importance = patch.importance.unwrap_or(current.importance);
    // For title/context: None in patch means "keep existing"; Some("") means "clear"
    let new_title = patch.title.or(current.title.clone());
    let new_context = patch.context.or(current.context.clone());

    if !(1..=10).contains(&new_importance) {
        anyhow::bail!("importance must be 1–10, got {}", new_importance);
    }

    let content_changed = new_content != current.content;
    let type_changed = new_type_str != current.memory_type;

    // Re-embed if content changed
    let new_embedding = if content_changed && config.embeddings.enabled {
        match embeddings::embed_text(&config.embeddings.model, &new_content) {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!("Failed to re-embed updated content: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Recompute quality if content or type changed
    let new_quality = if content_changed || type_changed {
        let mt: crate::models::MemoryType = new_type_str
            .parse()
            .unwrap_or(crate::models::MemoryType::Semantic);
        quality::compute_quality_score(&new_content, &mt).score
    } else {
        current.quality_score.unwrap_or(0.5)
    };

    let tags_json = serde_json::to_string(&new_tags)?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE memories SET type=?, content=?, importance=?, tags=?, quality_score=?, updated_at=?, title=?, context=? WHERE id=?"
    )
    .bind(&new_type_str)
    .bind(&new_content)
    .bind(new_importance)
    .bind(&tags_json)
    .bind(new_quality)
    .bind(&now)
    .bind(&new_title)
    .bind(&new_context)
    .bind(&full_id)
    .execute(&mut *tx)
    .await?;

    // Update FTS if content changed
    if content_changed {
        sqlx::query("UPDATE memories_fts SET content=? WHERE id=?")
            .bind(&new_content)
            .bind(&full_id)
            .execute(&mut *tx)
            .await?;
    }

    // Update embedding if content changed
    if let Some(ref emb) = new_embedding {
        let bytes: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query("INSERT OR REPLACE INTO vec_memories (memory_id, embedding) VALUES (?, ?)")
            .bind(&full_id)
            .bind(&bytes)
            .execute(&mut *tx)
            .await?;
    }

    // Update graph node property for memory_type if type changed
    if type_changed {
        let node_id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(&full_id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(nid) = node_id {
            let key_id = intern_property_key(&mut tx, "memory_type").await?;
            sqlx::query(
                "INSERT OR REPLACE INTO graph_node_props_text (node_id, key_id, value) VALUES (?, ?, ?)",
            )
            .bind(nid)
            .bind(key_id)
            .bind(&new_type_str)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    // Re-chunk if content changed
    if content_changed && config.embeddings.enabled && config.chunking.enabled {
        let _ = delete_chunks(pool, &full_id).await;
        let chunks = chunking::chunk_text(&new_content, &config.chunking);
        if chunks.len() > 1 {
            if let Err(e) =
                store_chunks(pool, &full_id, &chunks, &config.embeddings.model, &now).await
            {
                tracing::warn!(
                    "Failed to re-store chunk embeddings on update: {}. Continuing.",
                    e
                );
            }
        }
    }

    Ok(Memory {
        id: full_id,
        memory_type: new_type_str,
        content: new_content,
        importance: new_importance,
        tags: new_tags,
        metadata: current.metadata,
        scopes: current.scopes,
        created_at: current.created_at,
        updated_at: now,
        quality_score: Some(new_quality),
        title: new_title,
        context: new_context,
    })
}
