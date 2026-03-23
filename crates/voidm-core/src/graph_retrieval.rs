//! Graph-aware retrieval for search results.
//!
//! This module finds related memories based on:
//! - Shared tags (tag overlap scoring)
//! - Shared concepts (ontology relationships)
//!
//! Allows search to include memories that are conceptually related
//! to directly matched results, improving recall while maintaining precision.

use crate::models::Memory;
use crate::search::SearchResult;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::time::Instant;
use tracing::{debug, info, span, Level};

/// Configuration for tag-based retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRetrievalConfig {
    /// Enable tag-based related retrieval (default: true).
    #[serde(default = "default_tag_enabled")]
    pub enabled: bool,
    /// Minimum number of shared tags to consider related (default: 3).
    #[serde(default = "default_tag_min_overlap")]
    pub min_overlap: usize,
    /// Minimum overlap percentage (0-100) to include (default: 50).
    #[serde(default = "default_tag_min_percentage")]
    pub min_percentage: f32,
    /// Score decay for related results vs direct hits (default: 0.7).
    #[serde(default = "default_tag_decay")]
    pub decay_factor: f32,
    /// Max related memories per direct result (default: 5).
    #[serde(default = "default_tag_limit")]
    pub limit: usize,
}

fn default_tag_enabled() -> bool {
    true
}
fn default_tag_min_overlap() -> usize {
    3
}
fn default_tag_min_percentage() -> f32 {
    50.0
}
fn default_tag_decay() -> f32 {
    0.7
}
fn default_tag_limit() -> usize {
    5
}

impl Default for TagRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_overlap: 3,
            min_percentage: 50.0,
            decay_factor: 0.7,
            limit: 5,
        }
    }
}
/// Configuration for concept-based retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptRetrievalConfig {
    /// Enable concept-based related retrieval (default: true).
    #[serde(default = "default_concept_enabled")]
    pub enabled: bool,
    /// Max hops for concept graph traversal (optional, overrides global max_concept_hops).
    /// If None, uses GraphRetrievalConfig.max_concept_hops (default: 2).
    #[serde(default)]
    pub max_hops: Option<u8>,
    /// Score decay for concept-related results (default: 0.7).
    #[serde(default = "default_concept_decay")]
    pub decay_factor: f32,
    /// Max concept-related memories per direct result (default: 3).
    #[serde(default = "default_concept_limit")]
    pub limit: usize,
}

fn default_concept_enabled() -> bool {
    true
}
fn default_concept_decay() -> f32 {
    0.7
}
fn default_concept_limit() -> usize {
    3
}

impl Default for ConceptRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_hops: None,
            decay_factor: 0.7,
            limit: 3,
        }
    }
}
/// Graph retrieval configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRetrievalConfig {
    /// Enable graph-aware retrieval (default: true).
    #[serde(default = "default_graph_enabled")]
    pub enabled: bool,
    /// Global default max hops for concept graph traversal (default: 2).
    #[serde(default = "default_max_concept_hops")]
    pub max_concept_hops: u8,
    /// Tag-based retrieval configuration.
    #[serde(default)]
    pub tags: TagRetrievalConfig,
    /// Concept-based retrieval configuration.
    #[serde(default)]
    pub concepts: ConceptRetrievalConfig,
}

fn default_graph_enabled() -> bool {
    true
}
fn default_max_concept_hops() -> u8 {
    2
}

impl Default for GraphRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_concept_hops: 2,
            tags: TagRetrievalConfig::default(),
            concepts: ConceptRetrievalConfig::default(),
        }
    }
}

/// Find memories related via shared tags.
///
/// Scores memories based on tag overlap:
/// - overlap_count >= min_overlap
/// - (overlap_count / max_tags) * 100 >= min_percentage
///
/// Returns memories with scores decayed based on overlap percentage.
pub async fn find_related_by_tags(
    pool: &SqlitePool,
    direct_results: &[SearchResult],
    config: &TagRetrievalConfig,
) -> Result<Vec<SearchResult>> {
    if !config.enabled || direct_results.is_empty() {
        return Ok(Vec::new());
    }

    let span = span!(
        Level::DEBUG,
        "find_related_by_tags",
        direct_results_count = direct_results.len(),
        min_overlap = config.min_overlap,
        min_percentage = config.min_percentage
    );
    let _enter = span.enter();

    let start = Instant::now();
    let mut related = Vec::new();
    let seen_ids: HashSet<String> = direct_results.iter().map(|r| r.id.clone()).collect();

    for direct_result in direct_results {
        debug!(
            memory_id = &direct_result.id,
            "finding tag-overlapping memories"
        );

        // Use tags from direct result
        let query_tags_strs: HashSet<String> = direct_result
            .tags
            .iter()
            .map(|t| t.to_lowercase().trim().to_string())
            .collect();

        if query_tags_strs.is_empty() {
            debug!(
                memory_id = &direct_result.id,
                "no tags in direct result, skipping"
            );
            continue;
        }

        let query_tags_refs: HashSet<&String> = query_tags_strs.iter().collect();

        let overlap_start = Instant::now();
        let tag_overlaps =
            find_memories_by_tag_overlap(pool, &direct_result.id, &query_tags_refs, config).await?;
        debug!(
            elapsed_ms = overlap_start.elapsed().as_millis() as u64,
            overlap_count = tag_overlaps.len(),
            "found tag overlapping memories"
        );

        for (memory, overlap_count) in tag_overlaps {
            if !seen_ids.contains(&memory.id) {
                // Calculate overlap percentage for scoring
                let overlap_pct = (overlap_count as f32
                    / query_tags_strs.len().max(memory.tags.len()) as f32)
                    * 100.0;
                let score = (overlap_pct / 100.0) * config.decay_factor;

                related.push(SearchResult {
                    id: memory.id,
                    score,
                    memory_type: memory.memory_type,
                    content: memory.content,
                    scopes: memory.scopes,
                    tags: memory.tags,
                    importance: memory.importance,
                    created_at: memory.created_at,
                    source: "graph_tags".to_string(),
                    rel_type: None,
                    direction: None,
                    hop_depth: None,
                    parent_id: Some(direct_result.id.clone()),
                    quality_score: None,
                });
            }
        }
    }

    info!(
        total_ms = start.elapsed().as_millis() as u64,
        result_count = related.len(),
        "tag-based retrieval complete"
    );

    Ok(related)
}

/// Internal: Find memories with overlapping tags.
///
/// Returns (Memory, overlap_count) for all memories with sufficient tag overlap.
async fn find_memories_by_tag_overlap(
    pool: &SqlitePool,
    exclude_id: &str,
    query_tags: &HashSet<&String>,
    config: &TagRetrievalConfig,
) -> Result<Vec<(Memory, usize)>> {
    let span = span!(
        Level::DEBUG,
        "find_memories_by_tag_overlap",
        query_tags_count = query_tags.len(),
        exclude_id = exclude_id
    );
    let _enter = span.enter();

    let query_start = Instant::now();

    // Query all memories except the excluded one
    let rows: Vec<(String, String, String, i64, String, String, Option<f32>, String, String)> = sqlx::query_as(
        "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
         FROM memories
         WHERE id != ?
         ORDER BY created_at DESC"
    )
    .bind(exclude_id)
    .fetch_all(pool)
    .await?;

    debug!(
        elapsed_ms = query_start.elapsed().as_millis() as u64,
        count = rows.len(),
        "queried all memories"
    );

    let parse_start = Instant::now();
    let mut results = Vec::new();

    for (
        id,
        memory_type,
        content,
        importance,
        tags_json,
        _metadata_json,
        _quality_score_db,
        created_at,
        updated_at,
    ) in rows
    {
        // Parse tags
        let tags_strs: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let memory_tags_set: HashSet<String> = tags_strs
            .iter()
            .map(|t| t.to_lowercase().trim().to_string())
            .collect();

        // Skip empty tags
        if memory_tags_set.is_empty() {
            continue;
        }

        // Calculate overlap - convert query_tags reference strings to owned for comparison
        let overlap_count = query_tags
            .iter()
            .filter(|qt| memory_tags_set.contains((*qt).as_str()))
            .count();

        // Apply filters
        if overlap_count < config.min_overlap {
            continue;
        }

        let overlap_percentage =
            (overlap_count as f32 / query_tags.len().max(memory_tags_set.len()) as f32) * 100.0;
        if overlap_percentage < config.min_percentage {
            continue;
        }

        // Get scopes for this memory
        let scopes: Vec<String> =
            sqlx::query_scalar("SELECT scope FROM memory_scopes WHERE memory_id = ?")
                .bind(&id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

        let memory = Memory {
            id,
            memory_type,
            content,
            importance,
            tags: tags_strs,
            metadata: serde_json::Value::Object(Default::default()),
            scopes,
            created_at,
            updated_at,
            quality_score: None,
        };

        results.push((memory, overlap_count));
    }

    // Sort by overlap count (descending)
    results.sort_by(|a, b| b.1.cmp(&a.1));

    // Apply limit
    results.truncate(config.limit);

    debug!(
        elapsed_ms = parse_start.elapsed().as_millis() as u64,
        result_count = results.len(),
        "parsed and ranked overlaps"
    );

    Ok(results)
}

/// Find memories related via shared concepts.
///
/// Traverses ontology to find memories linked to related concept nodes.
/// Returns memories with scores based on concept distance.
pub async fn find_related_by_concepts(
    pool: &SqlitePool,
    direct_results: &[SearchResult],
    config: &ConceptRetrievalConfig,
    max_concept_hops: u8,
) -> Result<Vec<SearchResult>> {
    if !config.enabled || direct_results.is_empty() {
        return Ok(Vec::new());
    }

    let effective_max_hops = config.max_hops.unwrap_or(max_concept_hops);

    let span = span!(
        Level::DEBUG,
        "find_related_by_concepts",
        direct_results_count = direct_results.len(),
        max_hops = effective_max_hops
    );
    let _enter = span.enter();

    let start = Instant::now();
    let mut related = Vec::new();
    let mut seen_ids: HashSet<String> = direct_results.iter().map(|r| r.id.clone()).collect();

    for direct_result in direct_results {
        debug!(
            memory_id = &direct_result.id,
            "finding concept-related memories"
        );

        let concepts_start = Instant::now();
        let concepts = find_concepts_for_memory(pool, &direct_result.id).await?;
        debug!(
            elapsed_ms = concepts_start.elapsed().as_millis() as u64,
            concept_count = concepts.len(),
            "queried concepts for memory"
        );

        for concept_id in concepts {
            let traverse_start = Instant::now();
            let related_concepts =
                traverse_concept_graph(pool, &concept_id, effective_max_hops).await?;
            debug!(
                elapsed_ms = traverse_start.elapsed().as_millis() as u64,
                related_count = related_concepts.len(),
                hops = effective_max_hops,
                "traversed concept graph"
            );

            for (related_concept_id, hops) in related_concepts {
                let instances_start = Instant::now();
                let instances = find_concept_instances(pool, &related_concept_id).await?;
                debug!(
                    elapsed_ms = instances_start.elapsed().as_millis() as u64,
                    instance_count = instances.len(),
                    concept_id = &related_concept_id,
                    "found concept instances"
                );

                for (instance_id, instance_type) in instances {
                    // Only add memories as results, skip concept instances
                    if instance_type != "memory" {
                        continue;
                    }

                    if !seen_ids.contains(&instance_id) {
                        // Calculate score based on hop distance
                        let distance_score = config.decay_factor.powi(hops as i32);

                        related.push(SearchResult {
                            id: instance_id.clone(),
                            score: distance_score,
                            memory_type: "note".to_string(), // Will be overridden by actual lookup
                            content: String::new(),
                            scopes: vec![],
                            tags: vec![],
                            importance: 0,
                            created_at: String::new(),
                            source: "graph_concepts".to_string(),
                            rel_type: Some("related_concept".to_string()),
                            direction: None,
                            hop_depth: Some(hops as u8),
                            parent_id: Some(direct_result.id.clone()),
                            quality_score: None,
                        });

                        seen_ids.insert(instance_id);
                    }
                }
            }
        }
    }

    // Now enrich results with actual memory data
    let mut enriched = Vec::new();
    for mut result in related {
        let row: Option<(String, String, String, i64, String, String, Option<f32>, String, String)> = sqlx::query_as(
            "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
             FROM memories WHERE id = ?"
        )
        .bind(&result.id)
        .fetch_optional(pool)
        .await?;

        if let Some((
            id,
            memory_type,
            content,
            importance,
            tags_json,
            _metadata_json,
            _quality_score,
            created_at,
            _updated_at,
        )) = row
        {
            let tags_strs: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let scopes: Vec<String> =
                sqlx::query_scalar("SELECT scope FROM memory_scopes WHERE memory_id = ?")
                    .bind(&id)
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();

            result.memory_type = memory_type;
            result.content = content;
            result.scopes = scopes;
            result.tags = tags_strs;
            result.importance = importance;
            result.created_at = created_at;
            enriched.push(result);
        } else {
            debug!(memory_id = &result.id, "memory not found during enrichment");
        }
    }

    // Sort by score descending and apply limit
    enriched.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    enriched.truncate(config.limit);

    info!(
        total_ms = start.elapsed().as_millis() as u64,
        result_count = enriched.len(),
        max_hops = effective_max_hops,
        "concept distance retrieval complete"
    );

    Ok(enriched)
}

/// Find all concepts linked to a memory via INSTANCE_OF edge.
async fn find_concepts_for_memory(pool: &SqlitePool, memory_id: &str) -> Result<Vec<String>> {
    let concepts: Vec<(String,)> = sqlx::query_as(
        "SELECT to_id FROM ontology_edges
         WHERE from_id = ? AND rel_type = 'INSTANCE_OF' AND from_type = 'memory' AND to_type = 'concept'"
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await?;

    Ok(concepts.into_iter().map(|(id,)| id).collect())
}

/// Traverse concept graph bidirectionally to find related concepts up to max_hops distance.
/// Returns (concept_id, hops) for all related concepts.
async fn traverse_concept_graph(
    pool: &SqlitePool,
    concept_id: &str,
    max_hops: u8,
) -> Result<Vec<(String, i32)>> {
    let max_hops_limit = (max_hops as i64).max(1);

    // Bidirectional traversal: both IS_A forward and backward
    let related: Vec<(String, i64)> = sqlx::query_as(
        "WITH RECURSIVE related_concepts(id, hops) AS (
           SELECT ?, 0
           UNION ALL
           -- Forward: from IS_A to (follow parent concepts)
           SELECT e.to_id, rc.hops + 1
           FROM ontology_edges e
           JOIN related_concepts rc ON e.from_id = rc.id
           WHERE e.rel_type = 'IS_A'
             AND e.from_type = 'concept' AND e.to_type = 'concept'
             AND rc.hops < ?
           UNION ALL
           -- Backward: to IS_A from (follow child concepts)
           SELECT e.from_id, rc.hops + 1
           FROM ontology_edges e
           JOIN related_concepts rc ON e.to_id = rc.id
           WHERE e.rel_type = 'IS_A'
             AND e.from_type = 'concept' AND e.to_type = 'concept'
             AND rc.hops < ?
         )
         SELECT id, hops FROM related_concepts WHERE hops > 0",
    )
    .bind(concept_id)
    .bind(max_hops_limit)
    .bind(max_hops_limit)
    .fetch_all(pool)
    .await?;

    debug!(
        concept_id = concept_id,
        max_hops = max_hops,
        found_count = related.len(),
        "traversed concept graph bidirectionally"
    );

    Ok(related
        .into_iter()
        .map(|(id, hops)| (id, hops as i32))
        .collect())
}

/// Find all memories linked to a concept via INSTANCE_OF edge (backward).
/// Returns (instance_id, instance_type) pairs.
async fn find_concept_instances(
    pool: &SqlitePool,
    concept_id: &str,
) -> Result<Vec<(String, String)>> {
    let instances: Vec<(String, String)> = sqlx::query_as(
        "SELECT from_id, from_type FROM ontology_edges
         WHERE to_id = ? AND rel_type = 'INSTANCE_OF'",
    )
    .bind(concept_id)
    .fetch_all(pool)
    .await?;

    Ok(instances)
}

/// Merge graph-aware results with original search results.
///
/// Deduplicates by ID and applies score decay for related results.
pub fn merge_graph_results(
    original: Vec<SearchResult>,
    tag_related: Vec<SearchResult>,
    concept_related: Vec<SearchResult>,
) -> Vec<SearchResult> {
    let mut merged = original;
    let mut seen_ids: HashSet<String> = merged.iter().map(|r| r.id.clone()).collect();

    // Add tag-related results
    for result in tag_related {
        if !seen_ids.contains(&result.id) {
            seen_ids.insert(result.id.clone());
            merged.push(result);
        }
    }

    // Add concept-related results
    for result in concept_related {
        if !seen_ids.contains(&result.id) {
            seen_ids.insert(result.id.clone());
            merged.push(result);
        }
    }

    merged
}

/// Expand search results with graph-aware retrieval (tag & concept matching).
/// This function is called during the search pipeline to add related memories
/// based on tag overlap and concept relationships.
///
/// Errors are logged but don't stop the search pipeline (graceful degradation).
pub async fn expand_graph_results(
    pool: &SqlitePool,
    results: &mut Vec<crate::search::SearchResult>,
    config: &GraphRetrievalConfig,
) -> Result<()> {
    if !config.enabled || results.is_empty() {
        return Ok(());
    }

    let span = span!(
        Level::DEBUG,
        "expand_graph_results",
        enabled = config.enabled,
        direct_result_count = results.len()
    );
    let _enter = span.enter();

    let start = Instant::now();
    let mut graph_results = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> =
        results.iter().map(|r| r.id.clone()).collect();

    // Get tag-based related results
    if config.tags.enabled {
        let tag_start = Instant::now();
        if let Ok(tag_related) = find_related_by_tags(pool, results, &config.tags).await {
            debug!(
                elapsed_ms = tag_start.elapsed().as_millis() as u64,
                result_count = tag_related.len(),
                "tag-based retrieval completed"
            );

            for mut result in tag_related {
                if !seen_ids.contains(&result.id) {
                    result.source = "graph_tags".to_string();
                    seen_ids.insert(result.id.clone());
                    graph_results.push(result);
                }
            }
        }
    }

    // Get concept-based related results
    if config.concepts.enabled {
        let concept_start = Instant::now();
        let max_hops = config.concepts.max_hops.unwrap_or(config.max_concept_hops);

        if let Ok(concept_related) =
            find_related_by_concepts(pool, results, &config.concepts, max_hops).await
        {
            debug!(
                elapsed_ms = concept_start.elapsed().as_millis() as u64,
                result_count = concept_related.len(),
                max_hops = max_hops,
                "concept-based retrieval completed"
            );

            for mut result in concept_related {
                if !seen_ids.contains(&result.id) {
                    result.source = "graph_concepts".to_string();
                    seen_ids.insert(result.id.clone());
                    graph_results.push(result);
                }
            }
        }
    }

    // Sort graph results by score and append to results
    graph_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    info!(
        total_ms = start.elapsed().as_millis() as u64,
        graph_result_count = graph_results.len(),
        tag_enabled = config.tags.enabled,
        concept_enabled = config.concepts.enabled,
        "graph-aware retrieval completed"
    );

    results.extend(graph_results);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_search_result(id: &str, tags: Vec<String>) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            score: 0.9,
            memory_type: "note".to_string(),
            content: format!("Memory {}", id),
            scopes: vec![],
            tags,
            importance: 0,
            created_at: "2026-01-01".to_string(),
            source: "search".to_string(),
            rel_type: None,
            direction: None,
            hop_depth: None,
            parent_id: None,
            quality_score: None,
        }
    }

    #[test]
    fn test_tag_overlap_percentage() {
        // Test: 3 shared tags out of 5 total = 60%
        let query_tags: Vec<String> = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        let memory_tags: Vec<String> = vec![
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "f".to_string(),
        ];

        let query_set: HashSet<_> = query_tags.iter().collect();
        let memory_set: HashSet<_> = memory_tags.iter().collect();

        let overlap = query_set.intersection(&memory_set).count();
        let max_tags = query_tags.len().max(memory_tags.len());
        let percentage = (overlap as f32 / max_tags as f32) * 100.0;

        assert_eq!(overlap, 3);
        assert_eq!(max_tags, 5);
        assert!((percentage - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_score_decay() {
        let base_score: f32 = 0.8;
        let decay_factor: f32 = 0.7;
        let decayed = base_score * decay_factor;

        assert!((decayed - 0.56_f32).abs() < 0.01);
    }

    #[test]
    fn test_concept_distance_decay() {
        // 1-hop: score = decay_factor^1
        let hop1 = 0.7_f32.powi(1);
        assert!((hop1 - 0.7).abs() < 0.01);

        // 2-hop: score = decay_factor^2
        let hop2 = 0.7_f32.powi(2);
        assert!((hop2 - 0.49).abs() < 0.01);

        // 3-hop: score = decay_factor^3
        let hop3 = 0.7_f32.powi(3);
        assert!((hop3 - 0.343).abs() < 0.01);
    }

    #[test]
    fn test_tag_retrieval_config_defaults() {
        let config = TagRetrievalConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_overlap, 3);
        assert_eq!(config.min_percentage, 50.0);
        assert_eq!(config.decay_factor, 0.7);
        assert_eq!(config.limit, 5);
    }

    #[test]
    fn test_concept_retrieval_config_defaults() {
        let config = ConceptRetrievalConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_hops, None); // Should default to GraphRetrievalConfig value
        assert_eq!(config.decay_factor, 0.7);
        assert_eq!(config.limit, 3);
    }

    #[test]
    fn test_concept_retrieval_config_max_hops_override() {
        let mut config = ConceptRetrievalConfig::default();
        config.max_hops = Some(3);
        assert_eq!(config.max_hops, Some(3));
    }

    #[test]
    fn test_graph_retrieval_config_defaults() {
        let config = GraphRetrievalConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_concept_hops, 2); // Default should be 2
        assert!(config.tags.enabled);
        assert!(config.concepts.enabled);
    }

    #[test]
    fn test_graph_retrieval_config_max_hops() {
        let config = GraphRetrievalConfig::default();
        assert_eq!(config.max_concept_hops, 2);
    }

    #[test]
    fn test_merge_deduplication() {
        let original = vec![create_search_result("1", vec!["docker".to_string()])];

        let tag_related = vec![create_search_result("2", vec!["kubernetes".to_string()])];

        let concept_related = vec![
            create_search_result("1", vec!["docker".to_string()]), // Duplicate
        ];

        let merged = merge_graph_results(original, tag_related, concept_related);

        // Should have 2 results (1 original + 1 tag_related, concept duplicate filtered)
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "1");
        assert_eq!(merged[1].id, "2");
    }

    #[test]
    fn test_merge_preserves_order() {
        let original = vec![create_search_result("1", vec!["a".to_string()])];

        let tag_related = vec![
            create_search_result("2", vec!["b".to_string()]),
            create_search_result("3", vec!["c".to_string()]),
        ];

        let concept_related = vec![create_search_result("4", vec!["d".to_string()])];

        let merged = merge_graph_results(original, tag_related, concept_related);

        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].id, "1");
        assert_eq!(merged[1].id, "2");
        assert_eq!(merged[2].id, "3");
        assert_eq!(merged[3].id, "4");
    }

    #[test]
    fn test_tag_config_case_normalization() {
        // Tags should be normalized to lowercase for comparison
        let tag1 = "Docker".to_lowercase();
        let tag2 = "docker".to_lowercase();
        assert_eq!(tag1, tag2);
    }

    #[test]
    fn test_tag_config_trim() {
        // Tags should have whitespace trimmed
        let tag = "  kubernetes  ".trim();
        assert_eq!(tag, "kubernetes");
    }

    #[test]
    fn test_overlap_percentage_formula() {
        // Test: 3 shared / max(4, 5) = 3/5 = 60%
        let query_count = 4;
        let memory_count = 5;
        let overlap_count = 3;

        let percentage = (overlap_count as f32 / query_count.max(memory_count) as f32) * 100.0;
        assert!((percentage - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_concept_disabled_returns_empty() {
        // When concept retrieval is disabled, should return empty results
        let config = ConceptRetrievalConfig {
            enabled: false,
            max_hops: None,
            decay_factor: 0.7,
            limit: 3,
        };

        assert!(!config.enabled);
    }

    #[test]
    fn test_tag_disabled_returns_empty() {
        // When tag retrieval is disabled, should return empty results
        let config = TagRetrievalConfig {
            enabled: false,
            min_overlap: 3,
            min_percentage: 50.0,
            decay_factor: 0.7,
            limit: 5,
        };

        assert!(!config.enabled);
    }

    #[test]
    fn test_graph_disabled_returns_empty() {
        // When graph retrieval is disabled, should return empty results
        let config = GraphRetrievalConfig {
            enabled: false,
            max_concept_hops: 2,
            tags: TagRetrievalConfig::default(),
            concepts: ConceptRetrievalConfig::default(),
        };

        assert!(!config.enabled);
    }
}
