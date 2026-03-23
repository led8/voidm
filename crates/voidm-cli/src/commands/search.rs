use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{
    search::{search, SearchMode, SearchOptions},
    Config,
};

#[derive(Args)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Filter by memory type
    #[arg(long, short = 't')]
    pub r#type: Option<String>,

    /// Search mode: hybrid, semantic, keyword, fuzzy, bm25
    #[arg(long, default_value = "hybrid")]
    pub mode: String,

    /// Maximum results
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Minimum score threshold (hybrid mode only). Overrides config search.min_score.
    /// Use --min-score 0 to disable filtering.
    #[arg(long)]
    pub min_score: Option<f32>,

    /// Minimum quality score (0.0-1.0) for results. Filters by quality_score.
    /// Use --min-quality 0.7 to exclude low-quality memories.
    #[arg(long)]
    pub min_quality: Option<f32>,

    /// Expand results with graph neighbors
    #[arg(long, default_value_t = false)]
    pub include_neighbors: bool,

    /// Max hops for neighbor expansion (default: config, hard cap: 3)
    #[arg(long)]
    pub neighbor_depth: Option<u8>,

    /// Score decay per hop (default: config neighbor_decay)
    #[arg(long)]
    pub neighbor_decay: Option<f32>,

    /// Min score for neighbors to be included (default: config neighbor_min_score)
    #[arg(long)]
    pub neighbor_min_score: Option<f32>,

    /// Max total neighbors to append (default: same as --limit)
    #[arg(long)]
    pub neighbor_limit: Option<usize>,

    /// Comma-separated edge types to traverse (default: PART_OF,SUPPORTS,DERIVED_FROM,EXEMPLIFIES)
    #[arg(long, value_delimiter = ',')]
    pub edge_types: Option<Vec<String>>,

    /// Enable/disable graph-aware retrieval (tag & concept matching) (overrides config)
    #[arg(long)]
    pub graph_retrieval: Option<bool>,

    /// Enable/disable reranker (overrides config)
    #[arg(long)]
    pub reranker: Option<bool>,

    /// Reranker model: ms-marco-TinyBERT or bge-reranker-base (overrides config)
    #[arg(long)]
    pub reranker_model: Option<String>,

    /// Apply reranker only to top-k results (overrides config)
    #[arg(long)]
    pub reranker_top_k: Option<usize>,

    /// Enable/disable query expansion (overrides config)
    #[arg(long)]
    pub query_expand: Option<bool>,

    /// Query expansion model: tinyllama (ONNX, default) or tobil/qmd-query-expansion-1.7B (GGUF, opt-in, better quality).
    /// App auto-detects backend. (overrides config)
    #[arg(long)]
    pub query_expand_model: Option<String>,

    /// Intent/context for query expansion (e.g., "oauth2", "database-design")
    /// When provided, guides expansion toward this context. Falls back to scope if not set.
    #[arg(long)]
    pub intent: Option<String>,

    /// Clear query expansion cache
    #[arg(long)]
    pub clear_expansion_cache: bool,

    /// Verbose output: show query expansion details
    #[arg(short, long)]
    pub verbose: bool,
}

pub async fn run(args: SearchArgs, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    let mode: SearchMode = args.mode.parse()?;

    // Apply CLI reranker overrides to config
    let mut config = config.clone();
    if args.reranker.is_some() || args.reranker_model.is_some() || args.reranker_top_k.is_some() {
        let mut reranker_config = config.search.reranker.take().unwrap_or_default();
        tracing::info!("CLI: Applying reranker CLI overrides");
        if let Some(enabled) = args.reranker {
            reranker_config.enabled = enabled;
            tracing::info!("CLI: Reranker override enabled={}", enabled);
        }
        if let Some(model) = args.reranker_model {
            tracing::info!(
                "CLI: Reranker model override: {} → {}",
                reranker_config.model,
                model
            );
            reranker_config.model = model;
        }
        if let Some(k) = args.reranker_top_k {
            tracing::info!(
                "CLI: Reranker apply_to_top_k override: {} → {}",
                reranker_config.apply_to_top_k,
                k
            );
            reranker_config.apply_to_top_k = k;
        }
        config.search.reranker = Some(reranker_config);
    }

    // Apply CLI query expansion overrides to config
    if args.query_expand.is_some() || args.query_expand_model.is_some() {
        let mut expansion_config = config.search.query_expansion.take().unwrap_or_default();
        tracing::info!("CLI: Applying query expansion CLI overrides");
        if let Some(enabled) = args.query_expand {
            expansion_config.enabled = enabled;
            tracing::info!("CLI: Query expansion override enabled={}", enabled);
        }
        if let Some(model) = args.query_expand_model {
            tracing::info!(
                "CLI: Query expansion model override: {} → {}",
                expansion_config.model,
                model
            );
            expansion_config.model = model;
        }
        config.search.query_expansion = Some(expansion_config);
    }

    // Handle cache clearing
    if args.clear_expansion_cache {
        tracing::warn!("CLI: Query expansion cache clearing requested (feature in development)");
        eprintln!("Query expansion cache clearing requested (feature in development)");
        return Ok(());
    }

    // Handle query expansion if enabled
    let mut expanded_query = args.query.clone();
    if let Some(expansion_config) = &config.search.query_expansion {
        if expansion_config.enabled {
            tracing::debug!("CLI: Query expansion is enabled in config");
            let expander =
                voidm_core::query_expansion::QueryExpander::new(expansion_config.clone());

            // Use intent-aware expansion if intent is provided, otherwise use standard expansion
            let expansion_result = if let Some(ref intent) = args.intent {
                tracing::info!(
                    "CLI: Requesting intent-aware query expansion with intent '{}'",
                    intent
                );
                expander
                    .expand_with_intent(&args.query, Some(intent.as_str()))
                    .await
            } else {
                tracing::info!("CLI: Requesting standard query expansion");
                expander.expand(&args.query).await
            };

            match expansion_result {
                Ok(expanded) => {
                    expanded_query = expanded;
                    tracing::info!("CLI: Query expansion succeeded");
                    if args.verbose {
                        tracing::info!("CLI (verbose): Original: '{}' | Expanded: '{}' | Model: {} | Intent: {:?}", 
                                       args.query, expanded_query, expansion_config.model, args.intent);
                        eprintln!("[query-expansion] Original: {}", args.query);
                        eprintln!("[query-expansion] Expanded: {}", expanded_query);
                        eprintln!("[query-expansion] Model: {}", expansion_config.model);
                        if let Some(ref intent) = args.intent {
                            eprintln!("[query-expansion] Intent: {}", intent);
                        }
                    }
                }
                Err(e) => {
                    // Query expansion failed - no fallback, use original query
                    tracing::warn!("CLI: Query expansion failed, using original query: {}", e);
                    if args.verbose {
                        eprintln!("[query-expansion] Failed: {} (using original query)", e);
                    }
                }
            }
        }
    }

    // Create search options with expanded query (or original if expansion failed)
    let opts = SearchOptions {
        query: expanded_query,
        mode,
        limit: args.limit,
        scope_filter: args.scope.clone(),
        type_filter: args.r#type,
        min_score: args.min_score,
        min_quality: args.min_quality,
        include_neighbors: args.include_neighbors,
        neighbor_depth: args.neighbor_depth,
        neighbor_decay: args.neighbor_decay,
        neighbor_min_score: args.neighbor_min_score,
        neighbor_limit: args.neighbor_limit,
        edge_types: args.edge_types,
        intent: args.intent.clone(),
    };

    let resp = search(
        pool,
        &opts,
        &config.embeddings.model,
        config.embeddings.enabled,
        config.search.min_score,
        &config.search,
    )
    .await?;

    if json {
        if resp.results.is_empty() {
            // Return best result even if below threshold, so agent can decide
            if let Some(best_score) = resp.best_score {
                let threshold = resp.threshold_applied.unwrap_or(config.search.min_score);
                let threshold_rounded = (threshold as f64 * 100.0).round() / 100.0;
                let best_rounded = (best_score as f64 * 100.0).round() / 100.0;
                println!(
                    "{}",
                    serde_json::json!({
                        "results": [],
                        "threshold": threshold_rounded,
                        "best_score": best_rounded,
                        "hint": format!(
                            "No results above score {:.2}. Best match scored {:.2}. \
                             Try --min-score {:.1} or --mode semantic.",
                            threshold,
                            best_score,
                            (best_score * 0.9).max(0.0)
                        )
                    })
                );
            } else {
                println!(
                    "{}",
                    serde_json::json!({
                        "results": [],
                        "threshold": null,
                        "best_score": null,
                        "hint": "No memories found. Use 'voidm add' to create memories."
                    })
                );
            }
        } else {
            println!("{}", serde_json::to_string_pretty(&resp.results)?);
        }
    } else {
        if resp.results.is_empty() {
            if let Some(threshold) = resp.threshold_applied {
                let best = resp.best_score.unwrap_or(0.0);
                eprintln!(
                    "No results above score {:.2} (best match: {:.2}).",
                    threshold, best
                );
                eprintln!(
                    "Try: --min-score {:.1}  or  --mode semantic  or  --min-score 0 to disable filtering.",
                    (best * 0.9).max(0.0)
                );
            } else {
                println!("No results found. Use 'voidm add' to create memories.");
            }
            return Ok(());
        }

        for r in &resp.results {
            if r.source == "graph" {
                let rel = r.rel_type.as_deref().unwrap_or("?");
                let dir = r.direction.as_deref().unwrap_or("?");
                let depth = r.hop_depth.unwrap_or(0);
                let parent = r.parent_id.as_deref().unwrap_or("?");
                println!(
                    "  ↳ [{:.3}] {} ({}) [graph: {} {} depth={}  parent={}]",
                    r.score,
                    r.id,
                    r.memory_type,
                    rel,
                    dir,
                    depth,
                    &parent[..8.min(parent.len())]
                );
            } else {
                println!("[{:.3}] {} ({})", r.score, r.id, r.memory_type);
            }
            let preview = if r.content.len() > 100 {
                format!("{}...", voidm_core::search::safe_truncate(&r.content, 100))
            } else {
                r.content.clone()
            };
            println!("  {}", preview);
            if let Some(qs) = r.quality_score {
                println!("  Quality: {:.2}", qs);
            }
            if !r.scopes.is_empty() {
                println!("  Scopes: {}", r.scopes.join(", "));
            }
            println!();
        }
    }
    Ok(())
}
