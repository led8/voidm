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
}

/// Result of a search, including threshold metadata for empty-result hints.
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    /// Set when threshold was applied and filtered some results out.
    pub threshold_applied: Option<f32>,
    /// Best score seen before threshold filtering (None if no results at all).
    pub best_score: Option<f32>,
}

/// Full hybrid search pipeline.
pub async fn search(
    pool: &SqlitePool,
    opts: &SearchOptions,
    model_name: &str,
    embeddings_enabled: bool,
    config_min_score: f32,
    config_search: &crate::config::SearchConfig,
) -> Result<SearchResponse> {
    // Dispatch to RRF-enhanced search if mode is HybridRRF
    if opts.mode == SearchMode::HybridRRF {
        return search_with_rrf(
            pool,
            opts,
            model_name,
            embeddings_enabled,
            config_min_score,
            config_search,
        )
        .await;
    }

    use std::collections::HashMap;

    tracing::info!("Search: Starting search request");
    tracing::debug!(
        "Search: query='{}', mode={:?}, limit={}, min_quality={:?}",
        opts.query,
        opts.mode,
        opts.limit,
        opts.min_quality
    );
    tracing::debug!(
        "Search: embeddings_enabled={}, config_min_score={}",
        embeddings_enabled,
        config_min_score
    );

    let fetch_limit = opts.limit * 3; // over-fetch for merging
    let mut scores: HashMap<String, f32> = HashMap::new();

    // --- Vector ANN ---
    let use_vector = embeddings_enabled
        && matches!(opts.mode, SearchMode::Hybrid | SearchMode::Semantic)
        && crate::vector::vec_table_exists(pool).await.unwrap_or(false);

    if use_vector {
        tracing::debug!("Search: Attempting vector-based search");
        match crate::embeddings::embed_text(model_name, &opts.query) {
            Ok(embedding) => {
                match crate::vector::ann_search(pool, &embedding, fetch_limit).await {
                    Ok(hits) => {
                        for (id, dist) in hits {
                            // Convert cosine distance [0,2] to similarity [0,1]
                            let sim = 1.0 - (dist / 2.0).clamp(0.0, 1.0);
                            *scores.entry(id).or_default() += sim * 0.5;
                        }
                    }
                    Err(e) => tracing::warn!("Vector search failed: {}", e),
                }
            }
            Err(e) => tracing::warn!("Embedding failed: {}", e),
        }
    }

    // --- BM25 via FTS5 ---
    let use_bm25 = matches!(
        opts.mode,
        SearchMode::Hybrid | SearchMode::Bm25 | SearchMode::Keyword
    );
    if use_bm25 {
        let fts_query = sanitize_fts_query(&opts.query);
        let rows: Vec<(String, f32)> = sqlx::query_as(
            "SELECT id, bm25(memories_fts) AS score FROM memories_fts WHERE content MATCH ? ORDER BY score LIMIT ?"
        )
        .bind(&fts_query)
        .bind(fetch_limit as i64)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        // BM25 scores are negative in FTS5 (more negative = more relevant)
        let min_bm25 = rows.iter().map(|(_, s)| *s).fold(f32::MAX, f32::min);
        let max_bm25 = rows.iter().map(|(_, s)| *s).fold(f32::MIN, f32::max);
        let range = (max_bm25 - min_bm25).abs().max(0.001);

        for (id, raw_score) in rows {
            // Normalize to [0, 1] where higher = more relevant (invert because BM25 is negative)
            let norm = 1.0 - ((raw_score - min_bm25) / range).clamp(0.0, 1.0);
            *scores.entry(id).or_default() += norm * 0.3;
        }
    }

    // --- Fuzzy (Jaro-Winkler) ---
    let use_fuzzy = matches!(opts.mode, SearchMode::Hybrid | SearchMode::Fuzzy);
    if use_fuzzy {
        let all: Vec<(String, String)> =
            sqlx::query_as("SELECT id, content FROM memories ORDER BY created_at DESC LIMIT 500")
                .fetch_all(pool)
                .await
                .unwrap_or_default();

        let query_lower = opts.query.to_lowercase();
        for (id, content) in all {
            let sim = strsim::jaro_winkler(&query_lower, &content.to_lowercase()) as f32;
            if sim > 0.6 {
                *scores.entry(id).or_default() += sim * 0.2;
            }
        }
    }

    if scores.is_empty() {
        // Fallback: return newest memories (no threshold applied — no scores to compare)
        let memories = fetch_memories_newest(pool, opts).await?;
        return Ok(SearchResponse {
            results: memories,
            threshold_applied: None,
            best_score: None,
        });
    }

    // Collect IDs sorted by score
    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(opts.limit);

    // Fetch full memory records for top results
    let mut results = Vec::new();
    for (id, score) in ranked {
        if let Some(m) = fetch_memory_by_id(pool, &id).await? {
            // Apply scope/type filters
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
            // Boost by importance
            let importance_boost = (m.importance as f32 - 5.0) * 0.02;

            // Use persisted quality_score from DB (already fetched via get_memory)
            let quality_score = m.quality_score;

            results.push(SearchResult {
                id,
                score: score + importance_boost,
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
                quality_score,
            });
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

    // Apply threshold — only in hybrid mode
    if opts.mode == SearchMode::Hybrid {
        let threshold = opts.min_score.unwrap_or(config_min_score);
        let best_score = results.first().map(|r| r.score);
        let before_count = results.len();
        results.retain(|r| r.score >= threshold);

        let threshold_applied = if results.len() < before_count {
            Some(threshold)
        } else {
            None
        };

        if opts.include_neighbors {
            expand_neighbors(pool, &mut results, opts, config_search).await?;
        }

        return Ok(SearchResponse {
            results,
            threshold_applied,
            best_score,
        });
    }

    if opts.include_neighbors {
        expand_neighbors(pool, &mut results, opts, config_search).await?;
    }

    Ok(SearchResponse {
        results,
        threshold_applied: None,
        best_score: None,
    })
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
                    quality_score,
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
        .map(|m| SearchResult {
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
        })
        .collect())
}

async fn fetch_memory_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Memory>> {
    crate::crud::get_memory(pool, id).await
}

fn sanitize_fts_query(q: &str) -> String {
    // FTS5 requires quoting special chars; simple approach: wrap in quotes
    let cleaned: String = q.chars().map(|c| if c == '"' { ' ' } else { c }).collect();
    format!("\"{}\"", cleaned)
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
                    (score_delta / original_score * 100.0)
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

/// Enhanced hybrid search with Reciprocal Rank Fusion (RRF).
///
/// Combines vector, BM25, and fuzzy signals using RRF instead of weighted averaging.
/// Benefits:
/// - Better ranking by combining signals without manual weights
/// - Preserves high-confidence matches (rank 1-3 bonuses)
/// - Prevents any single signal from dominating
///
/// Usage: Enable via SearchMode or config option
pub async fn search_with_rrf(
    pool: &SqlitePool,
    opts: &SearchOptions,
    model_name: &str,
    embeddings_enabled: bool,
    config_min_score: f32,
    config_search: &crate::config::SearchConfig,
) -> Result<SearchResponse> {
    use std::collections::HashMap;

    tracing::info!("Search (RRF): Starting RRF-enhanced search request");
    tracing::debug!(
        "Search (RRF): query='{}', mode={:?}, limit={}",
        opts.query,
        opts.mode,
        opts.limit
    );

    let fetch_limit = opts.limit * 3; // over-fetch for merging

    // Collect signal results separately for RRF
    let mut vector_results: Vec<(String, f32)> = Vec::new();
    let mut bm25_results: Vec<(String, f32)> = Vec::new();
    let mut fuzzy_results: Vec<(String, f32)> = Vec::new();

    // --- Vector ANN Signal ---
    let use_vector = embeddings_enabled
        && matches!(opts.mode, SearchMode::Hybrid | SearchMode::Semantic)
        && crate::vector::vec_table_exists(pool).await.unwrap_or(false);

    if use_vector {
        tracing::debug!("Search (RRF): Vector signal");
        if let Ok(embedding) = crate::embeddings::embed_text(model_name, &opts.query) {
            if let Ok(hits) = crate::vector::ann_search(pool, &embedding, fetch_limit).await {
                for (id, dist) in hits {
                    let sim = 1.0 - (dist / 2.0).clamp(0.0, 1.0);
                    vector_results.push((id, sim));
                }
                // Sort by score descending for RRF
                vector_results
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
    }

    // --- BM25 Signal ---
    let use_bm25 = matches!(
        opts.mode,
        SearchMode::Hybrid | SearchMode::Bm25 | SearchMode::Keyword
    );
    if use_bm25 {
        tracing::debug!("Search (RRF): BM25 signal");
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

            for (id, raw_score) in rows {
                let norm = 1.0 - ((raw_score - min_bm25) / range).clamp(0.0, 1.0);
                bm25_results.push((id, norm));
            }
            bm25_results.sort_by(|a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    // --- Fuzzy Signal ---
    let use_fuzzy = matches!(opts.mode, SearchMode::Hybrid | SearchMode::Fuzzy);
    if use_fuzzy {
        tracing::debug!("Search (RRF): Fuzzy signal");
        if let Ok(all) = sqlx::query_as::<_, (String, String)>(
            "SELECT id, content FROM memories ORDER BY created_at DESC LIMIT 500",
        )
        .fetch_all(pool)
        .await
        {
            let query_lower = opts.query.to_lowercase();
            for (id, content) in all {
                let sim = strsim::jaro_winkler(&query_lower, &content.to_lowercase()) as f32;
                if sim > 0.6 {
                    fuzzy_results.push((id, sim));
                }
            }
            fuzzy_results
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    // Prepare signals for RRF
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
        // Fallback: return newest memories
        let memories = fetch_memories_newest(pool, opts).await?;
        return Ok(SearchResponse {
            results: memories,
            threshold_applied: None,
            best_score: None,
        });
    }

    // Apply RRF fusion
    let rrf = crate::rrf_fusion::RRFFusion::default();
    let fused = rrf.fuse(signals);

    tracing::debug!("Search (RRF): RRF fusion complete, {} results", fused.len());

    // Fetch full memory records
    let mut results = Vec::new();
    let mut best_score = None;

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

            let importance_boost = (m.importance as f32 - 5.0) * 0.02;
            let final_score = rrf_result.rrf_score + importance_boost;

            best_score = Some(best_score.unwrap_or(final_score).max(final_score));

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
            });

            if results.len() >= opts.limit {
                break;
            }
        }
    }

    tracing::info!("Search (RRF): Returning {} results", results.len());

    Ok(SearchResponse {
        results,
        threshold_applied: None,
        best_score,
    })
}
