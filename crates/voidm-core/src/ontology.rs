use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

// ─── Models ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyEdge {
    pub id: i64,
    pub from_id: String,
    pub from_type: NodeKind,
    pub rel_type: String,
    pub to_id: String,
    pub to_type: NodeKind,
    pub note: Option<String>,
    pub created_at: String,
}

/// Discriminates whether an endpoint in an ontology edge is a Concept or a Memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Concept,
    Memory,
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeKind::Concept => write!(f, "concept"),
            NodeKind::Memory => write!(f, "memory"),
        }
    }
}

impl std::str::FromStr for NodeKind {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "concept" => Ok(NodeKind::Concept),
            "memory" => Ok(NodeKind::Memory),
            other => bail!("Unknown node kind: '{}'", other),
        }
    }
}

/// Ontology-specific edge types (IS_A, INSTANCE_OF, HAS_PROPERTY).
/// Regular EdgeTypes (SUPPORTS, CONTRADICTS, etc.) are also valid in ontology_edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OntologyRelType {
    IsA,
    InstanceOf,
    HasProperty,
    /// Pass-through for any string rel_type (covers existing EdgeType variants too)
    Other(String),
}

impl OntologyRelType {
    pub fn as_str(&self) -> &str {
        match self {
            OntologyRelType::IsA => "IS_A",
            OntologyRelType::InstanceOf => "INSTANCE_OF",
            OntologyRelType::HasProperty => "HAS_PROPERTY",
            OntologyRelType::Other(s) => s.as_str(),
        }
    }

    pub fn all_ontology_types() -> &'static [&'static str] {
        &["IS_A", "INSTANCE_OF", "HAS_PROPERTY"]
    }
}

impl std::fmt::Display for OntologyRelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for OntologyRelType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().replace('-', "_").as_str() {
            "IS_A" | "ISA" => Ok(OntologyRelType::IsA),
            "INSTANCE_OF" => Ok(OntologyRelType::InstanceOf),
            "HAS_PROPERTY" => Ok(OntologyRelType::HasProperty),
            other => Ok(OntologyRelType::Other(other.to_string())),
        }
    }
}

// ─── Concept CRUD ─────────────────────────────────────────────────────────────

/// Add a new concept. Returns the created Concept.
pub async fn add_concept(
    pool: &SqlitePool,
    name: &str,
    description: Option<&str>,
    scope: Option<&str>,
) -> Result<ConceptWithSimilarityWarning> {
    // Check for exact name+scope duplicate
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM ontology_concepts WHERE lower(name) = lower(?) AND (scope IS ? OR (scope IS NULL AND ? IS NULL))"
    )
    .bind(name)
    .bind(scope)
    .bind(scope)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        bail!("Concept '{}' already exists (id: {}). Use 'voidm ontology concept get {}' to inspect it.", name, &id[..8], &id[..8]);
    }

    // Check for similar concepts (similarity >= 0.8)
    let similar = find_similar_concepts(pool, name, 0.8).await?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO ontology_concepts (id, name, description, scope, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(scope)
    .bind(&now)
    .execute(pool)
    .await
    .context("Failed to insert concept")?;

    // FTS insert
    sqlx::query("INSERT INTO ontology_concept_fts (id, name, description) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(description.unwrap_or(""))
        .execute(pool)
        .await?;

    Ok(ConceptWithSimilarityWarning {
        id,
        name: name.to_string(),
        description: description.map(str::to_string),
        scope: scope.map(str::to_string),
        created_at: now,
        similar_concepts: similar,
    })
}

/// Get a concept by full or short (prefix) ID.
pub async fn get_concept(pool: &SqlitePool, id: &str) -> Result<Concept> {
    let full_id = resolve_concept_id(pool, id).await?;
    let row: (String, String, Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT id, name, description, scope, created_at FROM ontology_concepts WHERE id = ?",
    )
    .bind(&full_id)
    .fetch_one(pool)
    .await
    .with_context(|| format!("Concept '{}' not found", id))?;

    Ok(Concept {
        id: row.0,
        name: row.1,
        description: row.2,
        scope: row.3,
        created_at: row.4,
    })
}

/// List concepts, optionally filtered by scope prefix.
pub async fn list_concepts(
    pool: &SqlitePool,
    scope_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<Concept>> {
    let rows: Vec<(String, String, Option<String>, Option<String>, String)> =
        if let Some(scope) = scope_filter {
            let prefix = format!("{}%", scope);
            sqlx::query_as(
                "SELECT id, name, description, scope, created_at
             FROM ontology_concepts WHERE scope LIKE ?
             ORDER BY name ASC LIMIT ?",
            )
            .bind(&prefix)
            .bind(limit as i64)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as(
                "SELECT id, name, description, scope, created_at
             FROM ontology_concepts ORDER BY name ASC LIMIT ?",
            )
            .bind(limit as i64)
            .fetch_all(pool)
            .await?
        };

    Ok(rows
        .into_iter()
        .map(|(id, name, description, scope, created_at)| Concept {
            id,
            name,
            description,
            scope,
            created_at,
        })
        .collect())
}

/// Delete a concept (and its ontology edges via CASCADE).
pub async fn delete_concept(pool: &SqlitePool, id: &str) -> Result<bool> {
    let full_id = resolve_concept_id(pool, id).await?;

    // FTS: manual delete
    sqlx::query("DELETE FROM ontology_concept_fts WHERE id = ?")
        .bind(&full_id)
        .execute(pool)
        .await?;

    let result = sqlx::query("DELETE FROM ontology_concepts WHERE id = ?")
        .bind(&full_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ─── Ontology Edge CRUD ───────────────────────────────────────────────────────

/// Add an edge in the ontology graph. Both endpoints can be concepts or memories.
pub async fn add_ontology_edge(
    pool: &SqlitePool,
    from_id: &str,
    from_kind: NodeKind,
    rel_type: &OntologyRelType,
    to_id: &str,
    to_kind: NodeKind,
    note: Option<&str>,
) -> Result<OntologyEdge> {
    // Validate endpoints exist
    validate_node(pool, from_id, &from_kind).await?;
    validate_node(pool, to_id, &to_kind).await?;

    // IS_A and HAS_PROPERTY only make sense between concepts
    match rel_type {
        OntologyRelType::IsA | OntologyRelType::HasProperty => {
            if from_kind != NodeKind::Concept || to_kind != NodeKind::Concept {
                bail!("{} edges must connect two concepts", rel_type.as_str());
            }
        }
        OntologyRelType::InstanceOf => {
            if to_kind != NodeKind::Concept {
                bail!("INSTANCE_OF target must be a concept");
            }
        }
        _ => {}
    }

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO ontology_edges
         (from_id, from_type, rel_type, to_id, to_type, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(from_id)
    .bind(from_kind.to_string())
    .bind(rel_type.as_str())
    .bind(to_id)
    .bind(to_kind.to_string())
    .bind(note)
    .bind(&now)
    .execute(pool)
    .await
    .context("Failed to insert ontology edge")?;

    let id: i64 = sqlx::query_scalar(
        "SELECT id FROM ontology_edges WHERE from_id = ? AND to_id = ? AND rel_type = ?",
    )
    .bind(from_id)
    .bind(to_id)
    .bind(rel_type.as_str())
    .fetch_one(pool)
    .await?;

    Ok(OntologyEdge {
        id,
        from_id: from_id.to_string(),
        from_type: from_kind,
        rel_type: rel_type.as_str().to_string(),
        to_id: to_id.to_string(),
        to_type: to_kind,
        note: note.map(str::to_string),
        created_at: now,
    })
}

/// Remove an ontology edge by id.
pub async fn delete_ontology_edge(pool: &SqlitePool, edge_id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM ontology_edges WHERE id = ?")
        .bind(edge_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List ontology edges for a node (concept or memory), both directions.
pub async fn list_ontology_edges(pool: &SqlitePool, node_id: &str) -> Result<Vec<OntologyEdge>> {
    let rows: Vec<(
        i64,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT id, from_id, from_type, rel_type, to_id, to_type, note, created_at
             FROM ontology_edges
             WHERE from_id = ? OR to_id = ?
             ORDER BY created_at ASC",
    )
    .bind(node_id)
    .bind(node_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(
            |(id, from_id, from_type, rel_type, to_id, to_type, note, created_at)| {
                Ok(OntologyEdge {
                    id,
                    from_id,
                    from_type: from_type.parse()?,
                    rel_type,
                    to_id,
                    to_type: to_type.parse()?,
                    note,
                    created_at,
                })
            },
        )
        .collect()
}

// ─── Hierarchy & Subsumption ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyNode {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub depth: i64,
    pub direction: HierarchyDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HierarchyDirection {
    Ancestor,
    Descendant,
}

/// Return all ancestors (IS_A chain upward) and descendants (IS_A chain downward) of a concept.
pub async fn concept_hierarchy(pool: &SqlitePool, concept_id: &str) -> Result<Vec<HierarchyNode>> {
    let full_id = resolve_concept_id(pool, concept_id).await?;
    let mut results = Vec::new();

    // Ancestors: follow IS_A outgoing edges upward (from → to means "from IS_A to", so go to_id)
    let ancestors: Vec<(String, String, Option<String>, i64)> = sqlx::query_as(
        "WITH RECURSIVE ancestors(id, depth) AS (
           SELECT to_id, 1
           FROM ontology_edges
           WHERE from_id = ? AND rel_type = 'IS_A' AND from_type = 'concept' AND to_type = 'concept'
           UNION ALL
           SELECT e.to_id, a.depth + 1
           FROM ontology_edges e
           JOIN ancestors a ON e.from_id = a.id
           WHERE e.rel_type = 'IS_A' AND e.from_type = 'concept' AND e.to_type = 'concept'
             AND a.depth < 20
         )
         SELECT c.id, c.name, c.description, a.depth
         FROM ancestors a
         JOIN ontology_concepts c ON c.id = a.id
         ORDER BY a.depth ASC",
    )
    .bind(&full_id)
    .fetch_all(pool)
    .await?;

    for (id, name, description, depth) in ancestors {
        results.push(HierarchyNode {
            id,
            name,
            description,
            depth,
            direction: HierarchyDirection::Ancestor,
        });
    }

    // Descendants: follow IS_A incoming edges downward (find all x where x IS_A ... root)
    let descendants: Vec<(String, String, Option<String>, i64)> = sqlx::query_as(
        "WITH RECURSIVE descendants(id, depth) AS (
           SELECT from_id, 1
           FROM ontology_edges
           WHERE to_id = ? AND rel_type = 'IS_A' AND from_type = 'concept' AND to_type = 'concept'
           UNION ALL
           SELECT e.from_id, d.depth + 1
           FROM ontology_edges e
           JOIN descendants d ON e.to_id = d.id
           WHERE e.rel_type = 'IS_A' AND e.from_type = 'concept' AND e.to_type = 'concept'
             AND d.depth < 20
         )
         SELECT c.id, c.name, c.description, d.depth
         FROM descendants d
         JOIN ontology_concepts c ON c.id = d.id
         ORDER BY d.depth ASC",
    )
    .bind(&full_id)
    .fetch_all(pool)
    .await?;

    for (id, name, description, depth) in descendants {
        results.push(HierarchyNode {
            id,
            name,
            description,
            depth,
            direction: HierarchyDirection::Descendant,
        });
    }

    Ok(results)
}

/// Return all instances of a concept, including instances of all subclasses (full subsumption).
/// Instances are memories or concepts linked via INSTANCE_OF to X or any subclass of X.
pub async fn concept_instances(
    pool: &SqlitePool,
    concept_id: &str,
) -> Result<Vec<ConceptInstance>> {
    let full_id = resolve_concept_id(pool, concept_id).await?;

    // First collect all subclass IDs (including self)
    let subclass_ids: Vec<(String,)> = sqlx::query_as(
        "WITH RECURSIVE subclasses(id) AS (
           SELECT ?
           UNION ALL
           SELECT e.from_id
           FROM ontology_edges e
           JOIN subclasses s ON e.to_id = s.id
           WHERE e.rel_type = 'IS_A' AND e.from_type = 'concept' AND e.to_type = 'concept'
         )
         SELECT id FROM subclasses",
    )
    .bind(&full_id)
    .fetch_all(pool)
    .await?;

    let ids: Vec<String> = subclass_ids.into_iter().map(|(id,)| id).collect();

    // Collect all INSTANCE_OF edges pointing to any of those concept IDs
    let mut instances = Vec::new();
    for cid in &ids {
        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT from_id, from_type, to_id, note
             FROM ontology_edges
             WHERE to_id = ? AND rel_type = 'INSTANCE_OF'",
        )
        .bind(cid)
        .fetch_all(pool)
        .await?;

        for (from_id, from_type, to_id, note) in rows {
            instances.push(ConceptInstance {
                instance_id: from_id,
                instance_kind: from_type.parse().unwrap_or(NodeKind::Memory),
                concept_id: to_id,
                note,
            });
        }
    }

    Ok(instances)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptInstance {
    pub instance_id: String,
    pub instance_kind: NodeKind,
    pub concept_id: String,
    pub note: Option<String>,
}

// ─── ID resolution ────────────────────────────────────────────────────────────

/// Resolve full or short (prefix, min 4 chars) concept ID.
pub async fn resolve_concept_id(pool: &SqlitePool, id: &str) -> Result<String> {
    let exact: Option<String> = sqlx::query_scalar("SELECT id FROM ontology_concepts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some(full) = exact {
        return Ok(full);
    }

    if id.len() < 4 {
        bail!(
            "Concept ID prefix '{}' is too short (minimum 4 characters)",
            id
        );
    }

    let pattern = format!("{}%", id);
    let matches: Vec<String> =
        sqlx::query_scalar("SELECT id FROM ontology_concepts WHERE id LIKE ?")
            .bind(&pattern)
            .fetch_all(pool)
            .await?;

    match matches.len() {
        0 => bail!("Concept '{}' not found", id),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => bail!(
            "Ambiguous concept ID '{}' matches {} concepts. Use more characters:\n{}",
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

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn validate_node(pool: &SqlitePool, id: &str, kind: &NodeKind) -> Result<()> {
    match kind {
        NodeKind::Concept => {
            let exists: Option<String> =
                sqlx::query_scalar("SELECT id FROM ontology_concepts WHERE id = ?")
                    .bind(id)
                    .fetch_optional(pool)
                    .await?;
            if exists.is_none() {
                bail!("Concept '{}' not found", id);
            }
        }
        NodeKind::Memory => {
            let exists: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;
            if exists.is_none() {
                bail!("Memory '{}' not found", id);
            }
        }
    }
    Ok(())
}

// ─── Batch NER enrichment ─────────────────────────────────────────────────────

/// Result for one memory processed by enrich_memories.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnrichMemoryResult {
    pub memory_id: String,
    /// First ~80 chars of content for display
    pub preview: String,
    /// Entities extracted above threshold
    pub entities_found: usize,
    /// INSTANCE_OF edges created (concept existed or was created)
    pub links_created: usize,
    /// Concept names that were newly created (--add only)
    pub concepts_created: Vec<String>,
    /// Concept names that were linked to existing concepts
    pub concepts_linked: Vec<String>,
    /// Similar concepts for newly created ones (potential merge candidates)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub similar_concepts: Vec<(String, SimilarConcept)>,
    /// Whether this memory was skipped (already processed, no --force)
    pub skipped: bool,
}

/// Options for batch memory enrichment.
pub struct EnrichMemoriesOpts<'a> {
    pub scope: Option<&'a str>,
    pub min_score: f32,
    /// Create missing concepts automatically (like extract --add)
    pub add: bool,
    /// Re-process memories already in ontology_ner_processed
    pub force: bool,
    /// Don't write anything — just report what would be done
    pub dry_run: bool,
    /// Max memories to process (0 = all)
    pub limit: usize,
}

/// Batch-enrich all (or scoped) memories with NER entity extraction.
/// For each memory:
///   1. Extract named entities above min_score
///   2. For each entity: if concept exists → INSTANCE_OF edge
///                       if not + add=true → create concept + INSTANCE_OF edge
///   3. Record in ontology_ner_processed (skip on re-run unless force=true)
///
/// Progress is reported via the returned Vec (caller prints it).
pub async fn enrich_memories(
    pool: &SqlitePool,
    opts: &EnrichMemoriesOpts<'_>,
) -> Result<Vec<EnrichMemoryResult>> {
    use crate::ner;

    // Fetch memories to process
    let rows: Vec<(String, String)> = if let Some(scope) = opts.scope {
        let prefix = format!("{}%", scope);
        sqlx::query_as(
            "SELECT DISTINCT m.id, m.content
             FROM memories m
             JOIN memory_scopes ms ON ms.memory_id = m.id
             WHERE ms.scope LIKE ?
             ORDER BY m.created_at DESC",
        )
        .bind(&prefix)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as("SELECT id, content FROM memories ORDER BY created_at DESC")
            .fetch_all(pool)
            .await?
    };

    let limit = if opts.limit == 0 {
        rows.len()
    } else {
        opts.limit.min(rows.len())
    };
    let rows = &rows[..limit];

    let mut results = Vec::with_capacity(rows.len());
    let now = chrono::Utc::now().to_rfc3339();

    for (memory_id, content) in rows {
        // Check if already processed
        if !opts.force {
            let already: Option<String> = sqlx::query_scalar(
                "SELECT memory_id FROM ontology_ner_processed WHERE memory_id = ?",
            )
            .bind(memory_id)
            .fetch_optional(pool)
            .await?;
            if already.is_some() {
                results.push(EnrichMemoryResult {
                    memory_id: memory_id.clone(),
                    preview: preview(content),
                    entities_found: 0,
                    links_created: 0,
                    concepts_created: vec![],
                    concepts_linked: vec![],
                    similar_concepts: vec![],
                    skipped: true,
                });
                continue;
            }
        }

        // Run NER
        let entities = match ner::extract_entities(content) {
            Ok(e) => e,
            Err(_) => {
                // Record as processed with 0 entities to avoid re-running broken content
                if !opts.dry_run {
                    record_processed(pool, memory_id, 0, 0, &now).await?;
                }
                results.push(EnrichMemoryResult {
                    memory_id: memory_id.clone(),
                    preview: preview(content),
                    entities_found: 0,
                    links_created: 0,
                    concepts_created: vec![],
                    concepts_linked: vec![],
                    similar_concepts: vec![],
                    skipped: false,
                });
                continue;
            }
        };

        let above_threshold: Vec<_> = entities
            .iter()
            .filter(|e| e.score >= opts.min_score)
            .collect();

        let mut links_created = 0usize;
        let mut concepts_created = Vec::new();
        let mut concepts_linked = Vec::new();
        let mut similar_concepts = Vec::new();

        for entity in &above_threshold {
            // Check for existing concept (case-insensitive)
            let existing: Option<(String, String)> = sqlx::query_as(
                "SELECT id, name FROM ontology_concepts WHERE lower(name) = lower(?)",
            )
            .bind(&entity.text)
            .fetch_optional(pool)
            .await?;

            let concept_id = if let Some((id, name)) = existing {
                concepts_linked.push(name);
                Some(id)
            } else if opts.add {
                // Create new concept from entity
                if opts.dry_run {
                    concepts_created.push(entity.text.clone());
                    None
                } else {
                    match add_concept(pool, &entity.text, None, None).await {
                        Ok(c) => {
                            concepts_created.push(c.name.clone());
                            // Check for similar concepts (dedup detection)
                            if let Ok(similars) =
                                find_similar_concepts(pool, &entity.text, 0.85).await
                            {
                                for sim in similars {
                                    similar_concepts.push((entity.text.clone(), sim));
                                }
                            }
                            Some(c.id)
                        }
                        Err(_) => None, // duplicate race — skip
                    }
                }
            } else {
                None
            };

            // Create INSTANCE_OF edge if we have a concept
            if let Some(cid) = concept_id {
                if !opts.dry_run {
                    // Ignore duplicate edge errors (unique index)
                    let _ = add_ontology_edge(
                        pool,
                        memory_id,
                        NodeKind::Memory,
                        &OntologyRelType::InstanceOf,
                        &cid,
                        NodeKind::Concept,
                        None,
                    )
                    .await;
                }
                links_created += 1;
            }
        }

        // Record as processed
        if !opts.dry_run {
            record_processed(pool, memory_id, above_threshold.len(), links_created, &now).await?;
        }

        results.push(EnrichMemoryResult {
            memory_id: memory_id.clone(),
            preview: preview(content),
            entities_found: above_threshold.len(),
            links_created,
            concepts_created,
            concepts_linked,
            similar_concepts,
            skipped: false,
        });
    }

    Ok(results)
}

async fn record_processed(
    pool: &SqlitePool,
    memory_id: &str,
    entity_count: usize,
    link_count: usize,
    now: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO ontology_ner_processed (memory_id, processed_at, entity_count, link_count)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(memory_id) DO UPDATE SET
           processed_at = excluded.processed_at,
           entity_count = excluded.entity_count,
           link_count   = excluded.link_count",
    )
    .bind(memory_id)
    .bind(now)
    .bind(entity_count as i64)
    .bind(link_count as i64)
    .execute(pool)
    .await?;
    Ok(())
}

fn preview(content: &str) -> String {
    let s = content.trim();
    if s.chars().count() <= 80 {
        s.to_string()
    } else {
        let cut: String = s.chars().take(77).collect();
        format!("{}...", cut)
    }
}

// ─── Conflict management ──────────────────────────────────────────────────────

/// A CONTRADICTS edge with context about both endpoints.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Conflict {
    pub edge_id: i64,
    pub from_id: String,
    pub from_kind: String,
    pub from_name: Option<String>,
    pub from_description: Option<String>,
    pub to_id: String,
    pub to_kind: String,
    pub to_name: Option<String>,
    pub to_description: Option<String>,
    pub created_at: String,
}

/// List all CONTRADICTS edges that have not been resolved (no INVALIDATES edge from either endpoint).
pub async fn list_conflicts(pool: &SqlitePool, scope: Option<&str>) -> Result<Vec<Conflict>> {
    // Get all CONTRADICTS edges not yet invalidated
    // "Resolved" = a subsequent INVALIDATES edge exists from winner→loser or loser→winner
    let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT e.id, e.from_id, e.from_type, e.to_id, e.to_type, e.created_at
         FROM ontology_edges e
         WHERE e.rel_type = 'CONTRADICTS'
           AND NOT EXISTS (
             SELECT 1 FROM ontology_edges inv
             WHERE inv.rel_type = 'INVALIDATES'
               AND (
                 (inv.from_id = e.from_id AND inv.to_id = e.to_id)
                 OR (inv.from_id = e.to_id AND inv.to_id = e.from_id)
               )
           )
         ORDER BY e.created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    let mut conflicts = Vec::new();
    for (edge_id, from_id, from_type, to_id, to_type, created_at) in rows {
        // Fetch name/description for concept endpoints
        let from_info = if from_type == "concept" {
            fetch_concept_info(pool, &from_id).await?
        } else {
            fetch_memory_preview(pool, &from_id).await?
        };
        let to_info = if to_type == "concept" {
            fetch_concept_info(pool, &to_id).await?
        } else {
            fetch_memory_preview(pool, &to_id).await?
        };

        // Optional scope filter
        if let Some(s) = scope {
            let from_scope = get_scope(pool, &from_id, &from_type).await?;
            let to_scope = get_scope(pool, &to_id, &to_type).await?;
            if !from_scope.as_deref().unwrap_or("").starts_with(s)
                && !to_scope.as_deref().unwrap_or("").starts_with(s)
            {
                continue;
            }
        }

        conflicts.push(Conflict {
            edge_id,
            from_id,
            from_kind: from_type,
            from_name: from_info.0,
            from_description: from_info.1,
            to_id,
            to_kind: to_type,
            to_name: to_info.0,
            to_description: to_info.1,
            created_at,
        });
    }

    Ok(conflicts)
}

/// Get a single CONTRADICTS edge by id.
pub async fn get_conflict(pool: &SqlitePool, edge_id: i64) -> Result<Conflict> {
    let row: Option<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, from_id, from_type, to_id, to_type, created_at
         FROM ontology_edges WHERE id = ? AND rel_type = 'CONTRADICTS'",
    )
    .bind(edge_id)
    .fetch_optional(pool)
    .await?;

    let (eid, from_id, from_type, to_id, to_type, created_at) =
        row.ok_or_else(|| anyhow::anyhow!("CONTRADICTS edge #{} not found", edge_id))?;

    let from_info = if from_type == "concept" {
        fetch_concept_info(pool, &from_id).await?
    } else {
        fetch_memory_preview(pool, &from_id).await?
    };
    let to_info = if to_type == "concept" {
        fetch_concept_info(pool, &to_id).await?
    } else {
        fetch_memory_preview(pool, &to_id).await?
    };

    Ok(Conflict {
        edge_id: eid,
        from_id,
        from_kind: from_type,
        from_name: from_info.0,
        from_description: from_info.1,
        to_id,
        to_kind: to_type,
        to_name: to_info.0,
        to_description: to_info.1,
        created_at,
    })
}

/// Resolve a CONTRADICTS conflict:
///   1. Remove the CONTRADICTS edge
///   2. Add an INVALIDATES edge from winner → loser
///   3. Mark the loser concept as superseded (metadata field)
pub async fn resolve_conflict(
    pool: &SqlitePool,
    edge_id: i64,
    winner_id: &str,
    loser_id: &str,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Remove the CONTRADICTS edge
    sqlx::query("DELETE FROM ontology_edges WHERE id = ?")
        .bind(edge_id)
        .execute(pool)
        .await?;

    // 2. Determine kinds
    let winner_kind = node_kind_str(pool, winner_id).await?;
    let loser_kind = node_kind_str(pool, loser_id).await?;

    // 3. Insert INVALIDATES edge winner → loser
    sqlx::query(
        "INSERT OR IGNORE INTO ontology_edges
         (from_id, from_type, rel_type, to_id, to_type, note, created_at)
         VALUES (?, ?, 'INVALIDATES', ?, ?, 'conflict resolution', ?)",
    )
    .bind(winner_id)
    .bind(&winner_kind)
    .bind(loser_id)
    .bind(&loser_kind)
    .bind(&now)
    .execute(pool)
    .await?;

    // 4. Mark loser as superseded if it's a concept
    if loser_kind == "concept" {
        sqlx::query(
            "UPDATE ontology_concepts SET description =
               COALESCE(description, '') || ' [SUPERSEDED]'
             WHERE id = ? AND (description IS NULL OR description NOT LIKE '%[SUPERSEDED]%')",
        )
        .bind(loser_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

// ─── Conflict helpers ─────────────────────────────────────────────────────────

async fn fetch_concept_info(
    pool: &SqlitePool,
    id: &str,
) -> Result<(Option<String>, Option<String>)> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT name, description FROM ontology_concepts WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(n, d)| (Some(n), d)).unwrap_or((None, None)))
}

async fn fetch_memory_preview(
    pool: &SqlitePool,
    id: &str,
) -> Result<(Option<String>, Option<String>)> {
    let content: Option<String> = sqlx::query_scalar("SELECT content FROM memories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok((content.map(|c| preview(&c)), None))
}

async fn get_scope(pool: &SqlitePool, id: &str, kind: &str) -> Result<Option<String>> {
    if kind == "concept" {
        let s: Option<String> =
            sqlx::query_scalar("SELECT scope FROM ontology_concepts WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;
        Ok(s)
    } else {
        let s: Option<String> =
            sqlx::query_scalar("SELECT scope FROM memory_scopes WHERE memory_id = ? LIMIT 1")
                .bind(id)
                .fetch_optional(pool)
                .await?;
        Ok(s)
    }
}

async fn node_kind_str(pool: &SqlitePool, id: &str) -> Result<String> {
    let is_concept: Option<String> =
        sqlx::query_scalar("SELECT id FROM ontology_concepts WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    if is_concept.is_some() {
        Ok("concept".into())
    } else {
        Ok("memory".into())
    }
}

// ─── Concept search ───────────────────────────────────────────────────────────

/// A concept search result with a relevance score.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConceptSearchResult {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    /// BM25 relevance score from FTS5
    pub score: f32,
}

/// Search concepts by name/description using FTS5.
/// Returns up to `limit` results ordered by relevance.
pub async fn search_concepts(
    pool: &SqlitePool,
    query: &str,
    scope_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<ConceptSearchResult>> {
    // FTS5 MATCH — escape special chars to avoid parse errors
    let fts_query = query.replace('"', "\"\"");

    let rows: Vec<(String, String, Option<String>, Option<String>, f64)> =
        if let Some(scope) = scope_filter {
            let prefix = format!("{}%", scope);
            sqlx::query_as(
                "SELECT c.id, c.name, c.description, c.scope, bm25(ontology_concept_fts) AS score
             FROM ontology_concept_fts f
             JOIN ontology_concepts c ON c.id = f.id
             WHERE ontology_concept_fts MATCH ?
               AND c.scope LIKE ?
             ORDER BY score LIMIT ?",
            )
            .bind(&fts_query)
            .bind(&prefix)
            .bind(limit as i64)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as(
                "SELECT c.id, c.name, c.description, c.scope, bm25(ontology_concept_fts) AS score
             FROM ontology_concept_fts f
             JOIN ontology_concepts c ON c.id = f.id
             WHERE ontology_concept_fts MATCH ?
             ORDER BY score LIMIT ?",
            )
            .bind(&fts_query)
            .bind(limit as i64)
            .fetch_all(pool)
            .await?
        };

    Ok(rows
        .into_iter()
        .map(
            |(id, name, description, scope, score)| ConceptSearchResult {
                id,
                name,
                description,
                scope,
                score: score.abs() as f32,
            },
        )
        .collect())
}

// ─── Concept with instances ───────────────────────────────────────────────────

/// A concept enriched with its direct INSTANCE_OF memories.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConceptWithInstances {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub created_at: String,
    /// Direct INSTANCE_OF edges from memories to this concept
    pub instances: Vec<InstanceRef>,
    /// Subclasses (IS_A children)
    pub subclasses: Vec<ConceptRef>,
    /// Superclasses (IS_A parents)  
    pub superclasses: Vec<ConceptRef>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstanceRef {
    pub memory_id: String,
    /// First 120 chars of content
    pub preview: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConceptRef {
    pub id: String,
    pub name: String,
}

pub async fn get_concept_with_instances(
    pool: &SqlitePool,
    id: &str,
) -> Result<ConceptWithInstances> {
    let concept = get_concept(pool, id).await?;

    // Direct INSTANCE_OF edges (memory → this concept)
    let instance_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT e.from_id, m.content
         FROM ontology_edges e
         JOIN memories m ON m.id = e.from_id
         WHERE e.to_id = ? AND e.rel_type = 'INSTANCE_OF' AND e.from_type = 'memory'
         ORDER BY m.created_at DESC",
    )
    .bind(&concept.id)
    .fetch_all(pool)
    .await?;

    let instances = instance_rows
        .into_iter()
        .map(|(memory_id, content)| InstanceRef {
            memory_id,
            preview: preview(&content),
        })
        .collect();

    // Subclasses (IS_A where this concept is the parent/to_id)
    let sub_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.id, c.name FROM ontology_edges e
         JOIN ontology_concepts c ON c.id = e.from_id
         WHERE e.to_id = ? AND e.rel_type = 'IS_A'",
    )
    .bind(&concept.id)
    .fetch_all(pool)
    .await?;
    let subclasses = sub_rows
        .into_iter()
        .map(|(id, name)| ConceptRef { id, name })
        .collect();

    // Superclasses (IS_A where this concept is the child/from_id)
    let super_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.id, c.name FROM ontology_edges e
         JOIN ontology_concepts c ON c.id = e.to_id
         WHERE e.from_id = ? AND e.rel_type = 'IS_A'",
    )
    .bind(&concept.id)
    .fetch_all(pool)
    .await?;
    let superclasses = super_rows
        .into_iter()
        .map(|(id, name)| ConceptRef { id, name })
        .collect();

    Ok(ConceptWithInstances {
        id: concept.id,
        name: concept.name,
        description: concept.description,
        scope: concept.scope,
        created_at: concept.created_at,
        instances,
        subclasses,
        superclasses,
    })
}

/// Find similar concepts by name (returns matching concepts with similarity >= threshold)
async fn find_similar_concepts(
    pool: &SqlitePool,
    name: &str,
    threshold: f32,
) -> Result<Vec<SimilarConcept>> {
    let all_concepts: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM ontology_concepts ORDER BY name")
            .fetch_all(pool)
            .await?;

    let mut similar = Vec::new();
    let name_lower = name.to_lowercase();

    for (id, concept_name) in all_concepts {
        let similarity = strsim::jaro_winkler(&name_lower, &concept_name.to_lowercase()) as f32;
        if similarity >= threshold && similarity < 1.0 {
            // Get edge count
            let edge_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM ontology_edges WHERE to_id = ?")
                    .bind(&id)
                    .fetch_one(pool)
                    .await?;

            similar.push(SimilarConcept {
                id,
                name: concept_name,
                similarity,
                edge_count,
            });
        }
    }

    similar.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(similar)
}

/// Find similar concepts that are candidates for merging (Jaro-Winkler similarity >= threshold).
pub async fn find_merge_candidates(
    pool: &SqlitePool,
    similarity_threshold: f32,
) -> Result<Vec<MergeCandidate>> {
    let all_concepts: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM ontology_concepts ORDER BY name")
            .fetch_all(pool)
            .await?;

    let mut candidates = Vec::new();
    let mut seen_pairs = std::collections::HashSet::new();

    for i in 0..all_concepts.len() {
        for j in (i + 1)..all_concepts.len() {
            let (id1, name1) = &all_concepts[i];
            let (id2, name2) = &all_concepts[j];

            // Avoid duplicate pairs (sorted)
            let pair_key = if id1 < id2 {
                format!("{}|{}", id1, id2)
            } else {
                format!("{}|{}", id2, id1)
            };

            if seen_pairs.contains(&pair_key) {
                continue;
            }
            seen_pairs.insert(pair_key);

            // Jaro-Winkler similarity
            let similarity =
                strsim::jaro_winkler(&name1.to_lowercase(), &name2.to_lowercase()) as f32;

            if similarity >= similarity_threshold as f64 as f32 {
                // Get edge counts to decide which is "larger" (more connections)
                let count1: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM ontology_edges WHERE to_id = ? AND to_type = 'concept'",
                )
                .bind(id1)
                .fetch_one(pool)
                .await?;

                let count2: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM ontology_edges WHERE to_id = ? AND to_type = 'concept'",
                )
                .bind(id2)
                .fetch_one(pool)
                .await?;

                // Recommend merging smaller into larger (fewer retargets)
                let (source_id, source_name, target_id, target_name) = if count1 <= count2 {
                    (id1.clone(), name1.clone(), id2.clone(), name2.clone())
                } else {
                    (id2.clone(), name2.clone(), id1.clone(), name1.clone())
                };

                candidates.push(MergeCandidate {
                    source_id,
                    source_name,
                    target_id,
                    target_name,
                    similarity,
                    source_edges: count1.min(count2),
                    target_edges: count1.max(count2),
                });
            }
        }
    }

    // Sort by similarity (highest first)
    candidates.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(candidates)
}

/// Find merge candidates using both fuzzy matching and optional semantic similarity.
///
/// When semantic_dedup config is enabled:
/// 1. Uses fuzzy Jaro-Winkler as first filter (fast)
/// 2. For fuzzy matches, computes semantic similarity
/// 3. Filters based on semantic threshold if configured
///
/// Fallback to fuzzy-only if semantic model fails to load.
pub async fn find_merge_candidates_with_semantic(
    pool: &SqlitePool,
    fuzzy_threshold: f32,
    config: &crate::config::Config,
) -> Result<Vec<MergeCandidate>> {
    // Get semantic config if enabled
    let semantic_config = config
        .enrichment
        .semantic_dedup
        .as_ref()
        .filter(|c| c.enabled);

    let candidates = find_merge_candidates(pool, fuzzy_threshold).await?;

    // If semantic dedup is disabled, return fuzzy-based candidates
    let Some(sem_cfg) = semantic_config else {
        return Ok(candidates);
    };

    // Try to compute semantic similarities for fuzzy matches
    let embeddings_model = &config.embeddings.model;

    // Get all concept names for batch embedding
    let concept_ids: Vec<String> = candidates
        .iter()
        .flat_map(|c| vec![c.source_id.clone(), c.target_id.clone()])
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut id_to_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for id in &concept_ids {
        if let Ok(Some(name)) =
            sqlx::query_scalar::<_, String>("SELECT name FROM ontology_concepts WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await
        {
            id_to_name.insert(id.clone(), name);
        }
    }

    // Try to compute semantic similarities
    // Non-blocking: if this fails, fall back to fuzzy scores
    let mut enhanced_candidates = Vec::new();

    for mut candidate in candidates {
        let source_name = id_to_name
            .get(&candidate.source_id)
            .map(|n| n.as_str())
            .unwrap_or(&candidate.source_name);
        let target_name = id_to_name
            .get(&candidate.target_id)
            .map(|n| n.as_str())
            .unwrap_or(&candidate.target_name);

        match crate::semantic_dedup::similarity(source_name, target_name, embeddings_model) {
            Ok(semantic_sim) => {
                // Filter by semantic threshold if configured
                if semantic_sim >= sem_cfg.threshold {
                    // Optionally blend fuzzy and semantic scores
                    // For now: use semantic as the primary score
                    candidate.similarity = semantic_sim;
                    enhanced_candidates.push(candidate);
                }
            }
            Err(e) => {
                // Log but don't fail - semantic similarity is optional
                tracing::warn!(
                    "Semantic similarity computation failed: {}, falling back to fuzzy",
                    e
                );
                // Keep the candidate with fuzzy score
                if candidate.similarity >= sem_cfg.threshold {
                    enhanced_candidates.push(candidate);
                }
            }
        }
    }

    // Re-sort by similarity (now potentially semantic-based)
    enhanced_candidates.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(enhanced_candidates)
}

/// Merge source_concept into target_concept: retarget all edges, preserve all memories, delete source.
/// Transaction: all-or-nothing.
pub async fn merge_concepts(
    pool: &SqlitePool,
    source_id: &str,
    target_id: &str,
) -> Result<MergeResult> {
    let source_id_resolved = resolve_concept_id(pool, source_id).await?;
    let target_id_resolved = resolve_concept_id(pool, target_id).await?;

    if source_id_resolved == target_id_resolved {
        anyhow::bail!("Cannot merge concept with itself");
    }

    let mut tx = pool.begin().await?;

    // Get source concept details for response
    let source: (String, String) =
        sqlx::query_as("SELECT id, name FROM ontology_concepts WHERE id = ?")
            .bind(&source_id_resolved)
            .fetch_one(&mut *tx)
            .await?;

    let target: (String, String) =
        sqlx::query_as("SELECT id, name FROM ontology_concepts WHERE id = ?")
            .bind(&target_id_resolved)
            .fetch_one(&mut *tx)
            .await?;

    // Delete any edges that would become duplicates after merge
    sqlx::query(
        "DELETE FROM ontology_edges 
         WHERE from_id IN (SELECT from_id FROM ontology_edges WHERE to_id = ? AND rel_type = 'INSTANCE_OF')
         AND to_id = ? AND rel_type = 'INSTANCE_OF'"
    )
    .bind(&source_id_resolved)
    .bind(&target_id_resolved)
    .execute(&mut *tx)
    .await?;

    // Retarget all INSTANCE_OF edges (memory → source) to point to target
    sqlx::query("UPDATE ontology_edges SET to_id = ? WHERE to_id = ? AND rel_type = 'INSTANCE_OF'")
        .bind(&target_id_resolved)
        .bind(&source_id_resolved)
        .execute(&mut *tx)
        .await?;

    // Count for response (all INSTANCE_OF edges now point to target)
    let memory_edges: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ontology_edges WHERE to_id = ? AND rel_type = 'INSTANCE_OF'",
    )
    .bind(&target_id_resolved)
    .fetch_one(&mut *tx)
    .await?;

    // Retarget IS_A edges where source is child (source IS_A parent) → target IS_A parent
    sqlx::query("UPDATE ontology_edges SET from_id = ? WHERE from_id = ? AND rel_type = 'IS_A'")
        .bind(&target_id_resolved)
        .bind(&source_id_resolved)
        .execute(&mut *tx)
        .await?;

    // Retarget IS_A edges where source is parent (child IS_A source) → child IS_A target
    sqlx::query("UPDATE ontology_edges SET to_id = ? WHERE to_id = ? AND rel_type = 'IS_A'")
        .bind(&target_id_resolved)
        .bind(&source_id_resolved)
        .execute(&mut *tx)
        .await?;

    // Delete source concept
    sqlx::query("DELETE FROM ontology_concepts WHERE id = ?")
        .bind(&source_id_resolved)
        .execute(&mut *tx)
        .await?;

    tx.commit().await.context("Merge transaction failed")?;

    Ok(MergeResult {
        source_name: source.1,
        target_name: target.1,
        memory_edges_merged: memory_edges,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SimilarConcept {
    pub id: String,
    pub name: String,
    pub similarity: f32,
    pub edge_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConceptWithSimilarityWarning {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub similar_concepts: Vec<SimilarConcept>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MergeCandidate {
    pub source_id: String,
    pub source_name: String,
    pub target_id: String,
    pub target_name: String,
    pub similarity: f32,
    pub source_edges: i64,
    pub target_edges: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct MergeResult {
    pub source_name: String,
    pub target_name: String,
    pub memory_edges_merged: i64,
}

// ── Batch merge operations (Phase 5) ────────────────────────────────────────

use crate::models::{MergeLogEntry, MergePlan};

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchMergeResult {
    pub batch_id: String,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub conflicts: usize,
    pub edges_retargeted: usize,
    pub errors: Vec<(String, String, String)>, // (source, target, reason)
}

/// Analyze merge plan and return preview with impact stats (dry-run)
pub async fn analyze_merge_plan(pool: &SqlitePool, plan: &MergePlan) -> Result<BatchMergeResult> {
    let batch_id = uuid::Uuid::new_v4().to_string();
    let mut succeeded = 0;
    let mut failed = 0;
    let mut conflicts = 0;
    let mut edges_retargeted = 0;
    let mut errors = Vec::new();

    for pair in &plan.merges {
        // Verify both concepts exist
        let source_exists: (i64,) =
            match sqlx::query_as("SELECT COUNT(*) FROM ontology_concepts WHERE id = ?")
                .bind(&pair.source)
                .fetch_one(pool)
                .await
            {
                Ok(r) => r,
                Err(_) => {
                    failed += 1;
                    errors.push((
                        pair.source.clone(),
                        pair.target.clone(),
                        "Source not found".to_string(),
                    ));
                    continue;
                }
            };

        if source_exists.0 == 0 {
            failed += 1;
            errors.push((
                pair.source.clone(),
                pair.target.clone(),
                "Source not found".to_string(),
            ));
            continue;
        }

        let target_exists: (i64,) =
            match sqlx::query_as("SELECT COUNT(*) FROM ontology_concepts WHERE id = ?")
                .bind(&pair.target)
                .fetch_one(pool)
                .await
            {
                Ok(r) => r,
                Err(_) => {
                    failed += 1;
                    errors.push((
                        pair.source.clone(),
                        pair.target.clone(),
                        "Target not found".to_string(),
                    ));
                    continue;
                }
            };

        if target_exists.0 == 0 {
            failed += 1;
            errors.push((
                pair.source.clone(),
                pair.target.clone(),
                "Target not found".to_string(),
            ));
            continue;
        }

        // Count edges for both concepts
        let source_edge_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ontology_edges WHERE from_id = ? OR to_id = ?")
                .bind(&pair.source)
                .bind(&pair.source)
                .fetch_one(pool)
                .await?;

        let _target_edge_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ontology_edges WHERE from_id = ? OR to_id = ?")
                .bind(&pair.target)
                .bind(&pair.target)
                .fetch_one(pool)
                .await?;

        edges_retargeted += source_edge_count.0 as usize;

        // Check for conflicts (both have CONTRADICTS edges)
        let source_contradicts: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM ontology_edges WHERE (from_id = ? OR to_id = ?) AND rel_type = 'CONTRADICTS'"
        )
        .bind(&pair.source)
        .bind(&pair.source)
        .fetch_one(pool)
        .await?;

        let target_contradicts: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM ontology_edges WHERE (from_id = ? OR to_id = ?) AND rel_type = 'CONTRADICTS'"
        )
        .bind(&pair.target)
        .bind(&pair.target)
        .fetch_one(pool)
        .await?;

        if source_contradicts.0 > 0 && target_contradicts.0 > 0 {
            conflicts += 1;
        }

        succeeded += 1;
    }

    Ok(BatchMergeResult {
        batch_id,
        total: plan.merges.len(),
        succeeded,
        failed,
        conflicts,
        edges_retargeted,
        errors,
    })
}

/// Execute merge batch operation with single transaction
pub async fn execute_merge_batch(
    pool: &SqlitePool,
    batch_id: &str,
    plan: &MergePlan,
) -> Result<BatchMergeResult> {
    let mut tx = pool.begin().await?;
    let now = chrono::Local::now().to_rfc3339();
    let mut succeeded = 0;
    let mut failed = 0;
    let mut conflicts = 0;
    let mut edges_retargeted: usize = 0;
    let mut errors = Vec::new();

    for pair in &plan.merges {
        let merge_id = uuid::Uuid::new_v4().to_string();

        // Determine direction: smaller concept (fewer edges) merges into larger
        let source_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ontology_edges WHERE from_id = ? OR to_id = ?")
                .bind(&pair.source)
                .bind(&pair.source)
                .fetch_one(&mut *tx)
                .await
                .unwrap_or((0,));

        let target_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM ontology_edges WHERE from_id = ? OR to_id = ?")
                .bind(&pair.target)
                .bind(&pair.target)
                .fetch_one(&mut *tx)
                .await
                .unwrap_or((0,));

        let (source, target) = if source_count.0 < target_count.0 {
            // Source is smaller - merge it into target (use as-is)
            (&pair.source[..], &pair.target[..])
        } else if target_count.0 < source_count.0 {
            // Target is smaller - merge it into source (swap)
            (&pair.target[..], &pair.source[..])
        } else {
            // Equal edges - respect user input order
            (&pair.source[..], &pair.target[..])
        };

        // Check for conflicts
        let conflict_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM (
                SELECT from_id FROM ontology_edges WHERE (from_id = ? OR to_id = ?) AND rel_type = 'CONTRADICTS'
                INTERSECT
                SELECT from_id FROM ontology_edges WHERE (from_id = ? OR to_id = ?) AND rel_type = 'CONTRADICTS'
            )"
        )
        .bind(source)
        .bind(source)
        .bind(target)
        .bind(target)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or((0,));

        let conflicts_on_target = conflict_count.0 as i32;
        if conflict_count.0 > 0 {
            conflicts += 1;
        }

        // Retarget edges from source to target
        if let Err(_) = sqlx::query("UPDATE ontology_edges SET from_id = ? WHERE from_id = ?")
            .bind(target)
            .bind(source)
            .execute(&mut *tx)
            .await
        {
            failed += 1;
            errors.push((
                source.to_string(),
                target.to_string(),
                "Failed to retarget edges".to_string(),
            ));
            continue;
        }

        // Retarget incoming edges
        let _ = sqlx::query("UPDATE ontology_edges SET to_id = ? WHERE to_id = ?")
            .bind(target)
            .bind(source)
            .execute(&mut *tx)
            .await;

        let edges_count = source_count.0 as usize;
        edges_retargeted += edges_count;

        // Delete source concept FIRST (before logging)
        if let Err(_) = sqlx::query("DELETE FROM ontology_concepts WHERE id = ?")
            .bind(source)
            .execute(&mut *tx)
            .await
        {
            failed += 1;
            errors.push((
                source.to_string(),
                target.to_string(),
                "Failed to delete source concept".to_string(),
            ));
            continue;
        }

        // Log merge operation AFTER successful deletion
        let _ = sqlx::query(
            "INSERT INTO ontology_merge_log (id, batch_id, source_id, target_id, edges_retargeted, conflicts_kept, status, created_at, completed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&merge_id)
        .bind(batch_id)
        .bind(source)
        .bind(target)
        .bind(edges_count as i32)
        .bind(conflicts_on_target)
        .bind("completed")
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await;

        succeeded += 1;
    }

    // Commit transaction
    tx.commit().await?;

    // Update batch record
    let _ = sqlx::query(
        "INSERT OR REPLACE INTO ontology_merge_batch (id, total_merges, failed_merges, conflicts, created_at, executed_at)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(batch_id)
    .bind(plan.merges.len() as i32)
    .bind(failed as i32)
    .bind(conflicts as i32)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await;

    Ok(BatchMergeResult {
        batch_id: batch_id.to_string(),
        total: plan.merges.len(),
        succeeded,
        failed,
        conflicts,
        edges_retargeted,
        errors,
    })
}

/// Rollback single merge operation
pub async fn rollback_merge(pool: &SqlitePool, merge_id: &str) -> Result<()> {
    // Fetch merge log entry
    let entry: Option<(String, String, i32)> = sqlx::query_as(
        "SELECT source_id, target_id, edges_retargeted FROM ontology_merge_log WHERE id = ?",
    )
    .bind(merge_id)
    .fetch_optional(pool)
    .await?;

    let (source_id, target_id, _edge_count) =
        entry.ok_or_else(|| anyhow::anyhow!("Merge not found: {}", merge_id))?;
    let now = chrono::Local::now().to_rfc3339();

    let mut tx = pool.begin().await?;

    // Restore source concept (recreate with same ID)
    sqlx::query("INSERT INTO ontology_concepts (id, name, created_at) VALUES (?, ?, ?)")
        .bind(&source_id)
        .bind(&source_id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

    // Retarget edges back from target to source
    sqlx::query("UPDATE ontology_edges SET from_id = ? WHERE from_id = ?")
        .bind(&source_id)
        .bind(&target_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE ontology_edges SET to_id = ? WHERE to_id = ?")
        .bind(&source_id)
        .bind(&target_id)
        .execute(&mut *tx)
        .await?;

    // Update merge log status
    sqlx::query("UPDATE ontology_merge_log SET status = ?, completed_at = ? WHERE id = ?")
        .bind("rolled_back")
        .bind(&now)
        .bind(merge_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(())
}

/// List merge history
pub async fn list_merge_history(
    pool: &SqlitePool,
    batch_id: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<MergeLogEntry>> {
    let mut query = "SELECT id, batch_id, source_id, target_id, edges_retargeted, conflicts_kept, status, reason, created_at, completed_at FROM ontology_merge_log WHERE 1=1".to_string();

    if let Some(bid) = batch_id {
        query.push_str(&format!(" AND batch_id = '{}'", bid));
    }
    if let Some(s) = status {
        query.push_str(&format!(" AND status = '{}'", s));
    }
    query.push_str(" ORDER BY created_at DESC LIMIT 1000");

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            i32,
            i32,
            String,
            Option<String>,
            String,
            Option<String>,
        ),
    >(&query)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                batch_id,
                source_id,
                target_id,
                edges_retargeted,
                conflicts_kept,
                status,
                reason,
                created_at,
                completed_at,
            )| {
                MergeLogEntry {
                    id,
                    batch_id,
                    source_id,
                    target_id,
                    edges_retargeted,
                    conflicts_kept,
                    status,
                    reason,
                    created_at,
                    completed_at,
                }
            },
        )
        .collect())
}
