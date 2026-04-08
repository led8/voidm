use crate::models::{edge_hint, Memory, SuggestedLink};
use anyhow::Result;
use sqlx::SqlitePool;

const NEIGHBOR_MAX_DEPTH: u8 = 3;
const NEVER_TRAVERSE: &[&str] = &["CONTRADICTS", "INVALIDATES"];

/// Search result with all signals merged.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub created_at: String,
    /// "search" for direct hits, "graph" for neighbor-expanded results.
    pub source: String,
    /// Only set for source="graph".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel_type: Option<String>,
    /// Only set for source="graph": "outgoing" | "incoming" | "undirected".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// Only set for source="graph".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop_depth: Option<u8>,
    /// Only set for source="graph": ID of the direct search result this was reached from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Quality score (0.0-1.0) based on content genericity, abstraction, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,
    /// Age of the memory in days (days since created_at).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_days: Option<u32>,
    /// Short title if set on the memory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Semantic context label if set (gotcha | decision | procedure | reference).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Top matching chunk texts from chunk-level ANN (empty if no chunks).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context_chunks: Vec<String>,
    /// "memory" when matched at memory level, "chunk" when chunk ANN contributed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchMode {
    Hybrid,
    Semantic,
    Keyword,
    Fuzzy,
    Bm25,
    /// Hybrid search with Reciprocal Rank Fusion (RRF)
    /// Combines vector, BM25, fuzzy signals using RRF instead of weighted averaging
    HybridRRF,
}

impl std::str::FromStr for SearchMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hybrid" => Ok(SearchMode::Hybrid),
            "semantic" => Ok(SearchMode::Semantic),
            "keyword" => Ok(SearchMode::Keyword),
            "fuzzy" => Ok(SearchMode::Fuzzy),
            "bm25" => Ok(SearchMode::Bm25),
            "hybrid-rrf" => Ok(SearchMode::HybridRRF),
            other => Err(anyhow::anyhow!("Unknown search mode: '{}'. Valid: hybrid, semantic, keyword, fuzzy, bm25, hybrid-rrf", other)),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchOptions {
    pub query: String,
    pub mode: SearchMode,
    pub limit: usize,
    pub scope_filter: Option<String>,
    pub type_filter: Option<String>,
    /// Only applied in hybrid mode. None = use config default.
    pub min_score: Option<f32>,
    /// Minimum quality score (0.0-1.0) for results. None = no filter.
    pub min_quality: Option<f32>,
    /// If true, expand results with graph neighbors.
    pub include_neighbors: bool,
    /// Max hops for neighbor expansion (hard cap: NEIGHBOR_MAX_DEPTH).
    pub neighbor_depth: Option<u8>,
    /// Score decay per hop: neighbor_score = parent_score * decay^depth.
    pub neighbor_decay: Option<f32>,
    /// Min score for neighbors to be included.
    pub neighbor_min_score: Option<f32>,
    /// Max total neighbors to append (prevents hub explosion). None = same as limit.
    pub neighbor_limit: Option<usize>,
    /// Edge types to traverse. None = use config defaults.
    pub edge_types: Option<Vec<String>>,
    /// Optional intent/context for query expansion guidance.
    pub intent: Option<String>,
    /// Only return memories created within the last N days. None = no filter.
    pub max_age_days: Option<u32>,
}

/// Result of a search, including threshold metadata for empty-result hints.
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    /// Set when threshold was applied and filtered some results out.
    pub threshold_applied: Option<f32>,
    /// Best score seen before threshold filtering (None if no results at all).
    pub best_score: Option<f32>,
}

/// Full hybrid search pipeline — unified RRF path for all modes.
///
/// Mode → signal preset mapping:
/// - `semantic`          → vector only
/// - `bm25` / `keyword`  → bm25 only
/// - `fuzzy`             → fuzzy only
/// - `hybrid` / `hybrid-rrf` → all signals (per `config_search.signals`)
pub async fn search(
    pool: &SqlitePool,
    opts: &SearchOptions,
    model_name: &str,
    embeddings_enabled: bool,
    config_min_score: f32,
    config_search: &crate::config::SearchConfig,
) -> Result<SearchResponse> {
    tracing::info!("Search: Starting search request (mode={:?})", opts.mode);

    let fetch_limit = opts.limit * 3;

    // Determine active signals for this mode
    let sig = &config_search.signals;
    let use_vector = embeddings_enabled
        && sig.vector
        && matches!(
            opts.mode,
            SearchMode::Hybrid | SearchMode::HybridRRF | SearchMode::Semantic
        )
        && crate::vector::vec_table_exists(pool).await.unwrap_or(false);
    let use_bm25 = sig.bm25
        && matches!(
            opts.mode,
            SearchMode::Hybrid | SearchMode::HybridRRF | SearchMode::Bm25 | SearchMode::Keyword
        );
    let use_fuzzy = sig.fuzzy
        && matches!(
            opts.mode,
            SearchMode::Hybrid | SearchMode::HybridRRF | SearchMode::Fuzzy
        );

    let mut vector_results: Vec<(String, f32)> = Vec::new();
    let mut bm25_results: Vec<(String, f32)> = Vec::new();
    let mut fuzzy_results: Vec<(String, f32)> = Vec::new();
    let mut chunk_hits: std::collections::HashMap<String, (f32, Vec<String>)> =
        std::collections::HashMap::new();

    // --- Vector ANN ---
    if use_vector {
        if let Ok(embedding) = crate::embeddings::embed_text(model_name, &opts.query) {
            if let Ok(hits) = crate::vector::ann_search(pool, &embedding, fetch_limit).await {
                for (id, dist) in hits {
                    let sim = 1.0 - (dist / 2.0).clamp(0.0, 1.0);
                    vector_results.push((id, sim));
                }
                vector_results
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
            // Chunk ANN: inject as extra vector-level hits + populate chunk_hits
            if let Ok(chunk_results) =
                crate::vector::chunk_ann_search(pool, &embedding, fetch_limit).await
            {
                let mut dummy: std::collections::HashMap<String, f32> =
                    std::collections::HashMap::new();
                collect_chunk_hits(pool, chunk_results, &mut dummy, &mut chunk_hits, 0.15).await;
                for (mem_id, (sim, _)) in &chunk_hits {
                    if !vector_results.iter().any(|(id, _)| id == mem_id) {
                        vector_results.push((mem_id.clone(), sim * 0.8));
                    }
                }
                vector_results
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
    }

    // --- BM25 via FTS5 ---
    if use_bm25 {
        let fts_query = sanitize_fts_query(&opts.query);
        if let Ok(rows) = sqlx::query_as::<_, (String, f32)>(
            "SELECT id, bm25(memories_fts) AS score FROM memories_fts WHERE content MATCH ? ORDER BY score LIMIT ?"
        )
        .bind(&fts_query)
        .bind(fetch_limit as i64)
        .fetch_all(pool)
        .await {
            let min_bm25 = rows.iter().map(|(_, s)| *s).fold(f32::MAX, f32::min);
            let max_bm25 = rows.iter().map(|(_, s)| *s).fold(f32::MIN, f32::max);
            let range = (max_bm25 - min_bm25).abs().max(0.001);
            for (id, raw) in rows {
                let norm = 1.0 - ((raw - min_bm25) / range).clamp(0.0, 1.0);
                bm25_results.push((id, norm));
            }
            bm25_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    // --- Fuzzy (Jaro-Winkler) ---
    if use_fuzzy {
        if let Ok(all) = sqlx::query_as::<_, (String, String)>(
            "SELECT id, content FROM memories ORDER BY created_at DESC LIMIT 500",
        )
        .fetch_all(pool)
        .await
        {
            let q = opts.query.to_lowercase();
            for (id, content) in all {
                let sim = strsim::jaro_winkler(&q, &content.to_lowercase()) as f32;
                if sim > 0.6 {
                    fuzzy_results.push((id, sim));
                }
            }
            fuzzy_results
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    // Assemble signals for RRF
    let mut signals: Vec<(&str, Vec<(String, f32)>)> = Vec::new();
    if !vector_results.is_empty() {
        signals.push(("vector", vector_results));
    }
    if !bm25_results.is_empty() {
        signals.push(("bm25", bm25_results));
    }
    if !fuzzy_results.is_empty() {
        signals.push(("fuzzy", fuzzy_results));
    }

    if signals.is_empty() {
        let memories = fetch_memories_newest(pool, opts).await?;
        return Ok(SearchResponse {
            results: memories,
            threshold_applied: None,
            best_score: None,
        });
    }

    // RRF fusion
    let fused = crate::rrf_fusion::RRFFusion::default().fuse(signals);

    // Build results
    let mut results = Vec::new();
    let mut best_score: Option<f32> = None;

    for rrf_result in fused.iter().take(opts.limit * 2) {
        if let Some(m) = fetch_memory_by_id(pool, &rrf_result.id).await? {
            if let Some(ref scope) = opts.scope_filter {
                if !m.scopes.iter().any(|s| s.starts_with(scope.as_str())) {
                    continue;
                }
            }
            if let Some(ref t) = opts.type_filter {
                if m.memory_type != *t {
                    continue;
                }
            }
            let age = compute_age_days(&m.created_at);
            if let (Some(max), Some(age_val)) = (opts.max_age_days, age) {
                if age_val > max {
                    continue;
                }
            }

            let importance_boost = (m.importance as f32 - 5.0) * 0.02;
            let tb = title_boost(&opts.query, m.title.as_deref());
            let final_score = rrf_result.rrf_score + importance_boost + tb;
            best_score = Some(best_score.unwrap_or(final_score).max(final_score));

            let (ctx_chunks, content_src) = chunk_hits
                .remove(&rrf_result.id)
                .map(|(_, chunks)| (chunks, Some("chunk".to_string())))
                .unwrap_or_default();

            results.push(SearchResult {
                id: rrf_result.id.clone(),
                score: final_score,
                memory_type: m.memory_type,
                content: m.content,
                scopes: m.scopes,
                tags: m.tags,
                importance: m.importance,
                created_at: m.created_at,
                source: "search".into(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: m.quality_score,
                age_days: age,
                title: m.title,
                context: m.context,
                context_chunks: ctx_chunks,
                content_source: content_src,
            });

            if results.len() >= opts.limit {
                break;
            }
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply reranker if enabled (before quality/threshold filters)
    if let Some(reranker_config) = &config_search.reranker {
        if reranker_config.enabled {
            if !results.is_empty() {
                tracing::info!(
                    "Search: Reranker enabled, applying to {} results",
                    results.len()
                );
                if let Err(e) = apply_reranker(reranker_config, &opts.query, &mut results).await {
                    tracing::warn!("Search: Reranking failed, using original scores: {}", e);
                }
            } else {
                tracing::debug!("Search: Reranker enabled but no results to rerank");
            }
        } else {
            tracing::debug!("Search: Reranker disabled");
        }
    } else {
        tracing::debug!("Search: No reranker config found");
    }

    // Apply graph-aware retrieval if enabled (before quality/threshold filters)
    if let Some(graph_config) = &config_search.graph_retrieval {
        if graph_config.enabled {
            if !results.is_empty() {
                tracing::info!(
                    "Search: Graph-aware retrieval enabled, applying to {} results",
                    results.len()
                );
                if let Err(e) =
                    crate::graph_retrieval::expand_graph_results(pool, &mut results, graph_config)
                        .await
                {
                    tracing::warn!("Search: Graph-aware retrieval failed, continuing with original results: {}", e);
                }
            } else {
                tracing::debug!("Search: Graph-aware retrieval enabled but no results to expand");
            }
        } else {
            tracing::debug!("Search: Graph-aware retrieval disabled");
        }
    } else {
        tracing::debug!("Search: No graph-retrieval config found");
    }

    // Apply quality filter if specified
    if let Some(min_quality) = opts.min_quality {
        results.retain(|r| r.quality_score.unwrap_or(0.0) >= min_quality);
    }

    let threshold_applied = apply_score_threshold(
        &mut results,
        &opts.mode,
        opts.min_score,
        config_min_score,
        best_score,
    );

    if opts.include_neighbors {
        expand_neighbors(pool, &mut results, opts, config_search).await?;
    }

    tracing::info!("Search: Returning {} results", results.len());
    Ok(SearchResponse {
        results,
        threshold_applied,
        best_score,
    })
}

fn apply_score_threshold(
    results: &mut Vec<SearchResult>,
    mode: &SearchMode,
    explicit_min_score: Option<f32>,
    config_min_score: f32,
    best_score: Option<f32>,
) -> Option<f32> {
    if !matches!(mode, SearchMode::Hybrid | SearchMode::HybridRRF) {
        return None;
    }

    let threshold = explicit_min_score.unwrap_or(config_min_score);
    if threshold <= 0.0 {
        return None;
    }

    // Older configs commonly carry a 0.3 threshold from the pre-RRF weighted-score model.
    // When the best RRF score does not reach that bar, returning no results is worse than
    // returning the ranked candidates the engine already found.
    if explicit_min_score.is_none() && best_score.is_some_and(|score| score < threshold) {
        return None;
    }

    let before_count = results.len();
    results.retain(|r| r.score >= threshold);
    if results.len() < before_count {
        Some(threshold)
    } else {
        None
    }
}

/// Expand search results with graph neighbors in-place.
async fn expand_neighbors(
    pool: &SqlitePool,
    results: &mut Vec<SearchResult>,
    opts: &SearchOptions,
    config: &crate::config::SearchConfig,
) -> Result<()> {
    use voidm_graph::traverse::neighbors as graph_neighbors;

    let depth = opts
        .neighbor_depth
        .unwrap_or(config.default_neighbor_depth)
        .min(NEIGHBOR_MAX_DEPTH);
    let decay = opts.neighbor_decay.unwrap_or(config.neighbor_decay);
    let min_score = opts.neighbor_min_score.unwrap_or(config.neighbor_min_score);
    let limit = opts.neighbor_limit.unwrap_or(opts.limit);
    let allowed_types: Vec<&str> = opts
        .edge_types
        .as_deref()
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| {
            config
                .default_edge_types
                .iter()
                .map(|s| s.as_str())
                .collect()
        });

    // Build set of IDs already in results
    let mut seen: std::collections::HashSet<String> =
        results.iter().map(|r| r.id.clone()).collect();

    let direct_results: Vec<(String, f32)> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();

    let mut neighbors_to_add: Vec<SearchResult> = Vec::new();

    'outer: for (parent_id, parent_score) in &direct_results {
        let hops = graph_neighbors(pool, parent_id, depth, None).await?;
        for hop in hops {
            // Skip disallowed edge types
            if NEVER_TRAVERSE.contains(&hop.rel_type.as_str()) {
                continue;
            }
            if !allowed_types.contains(&hop.rel_type.as_str()) {
                continue;
            }
            if seen.contains(&hop.memory_id) {
                continue;
            }
            let nscore = parent_score * decay.powi(hop.depth as i32);
            if nscore < min_score {
                continue;
            }
            if let Some(m) = fetch_memory_by_id(pool, &hop.memory_id).await? {
                seen.insert(hop.memory_id.clone());

                // Use persisted quality_score from DB (already fetched via get_memory)
                let quality_score = m.quality_score;

                let age = compute_age_days(&m.created_at);
                neighbors_to_add.push(SearchResult {
                    id: hop.memory_id,
                    score: nscore,
                    memory_type: m.memory_type,
                    content: m.content,
                    scopes: m.scopes,
                    tags: m.tags,
                    importance: m.importance,
                    created_at: m.created_at,
                    source: "graph".into(),
                    rel_type: Some(hop.rel_type),
                    direction: Some(hop.direction),
                    hop_depth: Some(hop.depth),
                    parent_id: Some(parent_id.clone()),
                    title: m.title,
                    context: m.context,
                    quality_score,
                    age_days: age,
                    context_chunks: vec![],
                    content_source: None,
                });
                if neighbors_to_add.len() >= limit {
                    break 'outer;
                }
            }
        }
    }

    // Sort neighbors by score desc, then append
    neighbors_to_add.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.extend(neighbors_to_add);
    Ok(())
}

async fn fetch_memories_newest(
    pool: &SqlitePool,
    opts: &SearchOptions,
) -> Result<Vec<SearchResult>> {
    let memories = crate::crud::list_memories(
        pool,
        opts.scope_filter.as_deref(),
        opts.type_filter.as_deref(),
        opts.limit,
    )
    .await?;
    Ok(memories
        .into_iter()
        .map(|m| {
            let age = compute_age_days(&m.created_at);
            SearchResult {
                id: m.id,
                score: 0.0,
                memory_type: m.memory_type,
                content: m.content,
                scopes: m.scopes,
                tags: m.tags,
                importance: m.importance,
                created_at: m.created_at,
                source: "search".into(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: m.quality_score,
                age_days: age,
                title: m.title,
                context: m.context,
                context_chunks: vec![],
                content_source: None,
            }
        })
        .collect())
}

async fn fetch_memory_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Memory>> {
    crate::crud::get_memory(pool, id).await
}

/// Process chunk ANN results into score contributions and chunk_hits map.
/// chunk_id format: "{memory_id}_{index}"
/// Adds `weight` * sim to scores[memory_id], keeps top-2 chunk texts per memory.
async fn collect_chunk_hits(
    pool: &SqlitePool,
    chunk_results: Vec<(String, f32)>,
    scores: &mut std::collections::HashMap<String, f32>,
    chunk_hits: &mut std::collections::HashMap<String, (f32, Vec<String>)>,
    weight: f32,
) {
    for (chunk_id, dist) in chunk_results {
        let sim = 1.0 - (dist / 2.0).clamp(0.0, 1.0);
        // Extract memory_id as everything before the last '_'
        let memory_id = match chunk_id.rfind('_') {
            Some(pos) => chunk_id[..pos].to_string(),
            None => continue,
        };

        *scores.entry(memory_id.clone()).or_default() += sim * weight;

        let entry = chunk_hits.entry(memory_id.clone()).or_insert((0.0, vec![]));
        if sim > entry.0 {
            entry.0 = sim;
        }
        if entry.1.len() < 2 {
            // Fetch chunk content
            if let Ok(Some(content)) =
                sqlx::query_scalar::<_, String>("SELECT content FROM chunks WHERE id = ?")
                    .bind(&chunk_id)
                    .fetch_optional(pool)
                    .await
            {
                entry.1.push(content);
            }
        }
    }
}

/// Compute age in days from an RFC3339 `created_at` string.
pub fn compute_age_days(created_at: &str) -> Option<u32> {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Parse the RFC3339 timestamp by finding the seconds since epoch
    // Simple approach: parse up to second precision manually
    let ts = chrono::DateTime::parse_from_rfc3339(created_at).ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let created_secs = ts.timestamp();
    let diff = now.saturating_sub(created_secs);
    Some((diff / 86400) as u32)
}

/// Post-RRF title-based score boost (tiebreaker, not main signal).
/// exact match +2.0, prefix +1.5, substring +1.0
fn title_boost(query: &str, title: Option<&str>) -> f32 {
    let Some(t) = title else { return 0.0 };
    let q = query.to_lowercase();
    let t = t.to_lowercase();
    if t == q {
        2.0
    } else if t.starts_with(&q) {
        1.5
    } else if t.contains(&q) {
        1.0
    } else {
        0.0
    }
}

fn sanitize_fts_query(q: &str) -> String {
    // FTS5 requires quoting special chars; simple approach: wrap in quotes
    let cleaned: String = q.chars().map(|c| if c == '"' { ' ' } else { c }).collect();
    format!("\"{}\"", cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(score: f32) -> SearchResult {
        SearchResult {
            id: format!("id-{score}"),
            score,
            memory_type: "semantic".to_string(),
            content: "content".to_string(),
            scopes: vec!["scope".to_string()],
            tags: vec![],
            importance: 5,
            created_at: "2026-04-08T00:00:00+00:00".to_string(),
            source: "search".to_string(),
            rel_type: None,
            direction: None,
            hop_depth: None,
            parent_id: None,
            quality_score: Some(1.0),
            age_days: Some(0),
            title: None,
            context: None,
            context_chunks: vec![],
            content_source: Some("memory".to_string()),
        }
    }

    #[test]
    fn hybrid_ignores_legacy_config_threshold_when_all_scores_are_below_it() {
        let mut results = vec![result(0.12), result(0.08)];

        let applied =
            apply_score_threshold(&mut results, &SearchMode::Hybrid, None, 0.3, Some(0.12));

        assert_eq!(applied, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn hybrid_keeps_explicit_threshold_behavior() {
        let mut results = vec![result(0.12), result(0.08)];

        let applied = apply_score_threshold(
            &mut results,
            &SearchMode::Hybrid,
            Some(0.3),
            0.0,
            Some(0.12),
        );

        assert_eq!(applied, Some(0.3));
        assert!(results.is_empty());
    }

    #[test]
    fn hybrid_filters_when_results_clear_the_config_threshold() {
        let mut results = vec![result(0.35), result(0.08)];

        let applied =
            apply_score_threshold(&mut results, &SearchMode::Hybrid, None, 0.3, Some(0.35));

        assert_eq!(applied, Some(0.3));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 0.35);
    }

    #[test]
    fn semantic_mode_never_applies_hybrid_threshold() {
        let mut results = vec![result(0.12)];

        let applied =
            apply_score_threshold(&mut results, &SearchMode::Semantic, None, 0.3, Some(0.12));

        assert_eq!(applied, None);
        assert_eq!(results.len(), 1);
    }
}

/// Find similar memories for suggested_links and duplicate detection.
pub async fn find_similar(
    pool: &SqlitePool,
    embedding: &[f32],
    exclude_id: &str,
    limit: usize,
    threshold: f32,
) -> Result<Vec<(String, f32)>> {
    if !crate::vector::vec_table_exists(pool).await? {
        return Ok(vec![]);
    }
    let all = crate::vector::ann_search(pool, embedding, limit + 1).await?;
    let results = all
        .into_iter()
        .filter(|(id, _)| id != exclude_id)
        .map(|(id, dist)| {
            let sim = 1.0 - (dist / 2.0).clamp(0.0, 1.0);
            (id, sim)
        })
        .filter(|(_, sim)| *sim >= threshold)
        .take(limit)
        .collect();
    Ok(results)
}

/// Build SuggestedLink entries from similar memories.
pub async fn build_suggested_links(
    pool: &SqlitePool,
    new_memory_type: &str,
    similar: Vec<(String, f32)>,
) -> Result<Vec<SuggestedLink>> {
    let mut links = Vec::new();
    for (id, score) in similar {
        if let Some(m) = crate::crud::get_memory(pool, &id).await? {
            let content_truncated = if m.content.len() > 120 {
                format!("{}...", safe_truncate(&m.content, 120))
            } else {
                m.content.clone()
            };
            let hint = format!(
                "High similarity ({:.2}) — consider: {}",
                score,
                edge_hint(new_memory_type, &m.memory_type)
            );
            links.push(SuggestedLink {
                id,
                score,
                memory_type: m.memory_type,
                content: content_truncated,
                hint,
            });
        }
    }
    Ok(links)
}

/// Truncate a string at a safe Unicode char boundary.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

/// Apply reranker to top-k results using pure reranker-guided scoring.
/// Uses only reranker scores for reranked results (no blending with original scores).
/// This aligns with the reranker's intent as an expert ranking override.
async fn apply_reranker(
    config: &crate::config::RerankerConfig,
    query: &str,
    results: &mut Vec<SearchResult>,
) -> anyhow::Result<()> {
    let apply_to_k = config.apply_to_top_k.min(results.len());
    if apply_to_k == 0 {
        tracing::info!("Reranker: apply_to_top_k=0, skipping reranking");
        return Ok(());
    }

    tracing::info!(
        "Reranker: Initializing reranking with model: {}",
        config.model
    );
    tracing::debug!("Reranker config: apply_to_top_k={}", config.apply_to_top_k);

    let reranker = crate::reranker::CrossEncoderReranker::load(&config.model).await?;
    tracing::info!("Reranker: Model '{}' loaded successfully", config.model);

    // Extract passages using intelligent passage extraction
    let docs_to_rerank: Vec<String> = results[..apply_to_k]
        .iter()
        .map(|r| {
            crate::passage::extract_best_passage(&r.content, query, &config.passage_extraction)
        })
        .collect();

    let docs_to_rerank_refs: Vec<&str> = docs_to_rerank.iter().map(|s| s.as_str()).collect();

    tracing::debug!(
        "Reranker: Starting reranking of top-{} results (from {} total)",
        apply_to_k,
        results.len()
    );

    let reranked = reranker.rerank(query, &docs_to_rerank_refs)?;
    tracing::info!(
        "Reranker: Successfully reranked {} documents",
        reranked.len()
    );

    // Create a mapping of original_index -> reranker_score
    let mut rerank_scores: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
    let mut score_changes = Vec::new();

    for rerank_result in reranked {
        rerank_scores.insert(rerank_result.index, rerank_result.score);
    }

    // Update scores with pure reranker scores (no blending)
    for (idx, result) in results[..apply_to_k].iter_mut().enumerate() {
        if let Some(rerank_score) = rerank_scores.get(&idx) {
            let original_score = result.score;
            let score_delta = rerank_score - original_score;
            result.score = *rerank_score; // Use pure reranker score

            tracing::debug!(
                "Reranked [{}]: {:.4} → {:.4} (Δ {:.4}, {:.1}%) | {}",
                idx,
                original_score,
                rerank_score,
                score_delta,
                if original_score > 0.0 {
                    score_delta / original_score * 100.0
                } else {
                    0.0
                },
                &result.id[..std::cmp::min(12, result.id.len())]
            );

            score_changes.push((original_score, *rerank_score));
        }
    }

    // Calculate statistics
    if !score_changes.is_empty() {
        let original_mean =
            score_changes.iter().map(|(o, _)| o).sum::<f32>() / score_changes.len() as f32;
        let reranked_mean =
            score_changes.iter().map(|(_, r)| r).sum::<f32>() / score_changes.len() as f32;
        let min_original = score_changes
            .iter()
            .map(|(o, _)| o)
            .copied()
            .fold(f32::INFINITY, f32::min);
        let max_reranked = score_changes
            .iter()
            .map(|(_, r)| r)
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);

        tracing::info!(
            "Reranker: Score statistics - Original (mean={:.4}, min={:.4}) → Reranked (mean={:.4}, max={:.4})",
            original_mean, min_original, reranked_mean, max_reranked
        );
    }

    // Re-sort by reranker scores
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    tracing::info!("Reranker: Results re-sorted by reranker scores");

    Ok(())
}
