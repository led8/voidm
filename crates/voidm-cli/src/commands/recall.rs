use anyhow::Result;
use clap::Args;
use std::collections::HashMap;
use std::sync::Arc;
use voidm_core::{
    db::Database,
    search::{search, SearchMode, SearchOptions},
    Config,
};

const STARTUP_CATEGORIES: &[(&str, &str)] = &[
    ("architecture", "architecture"),
    ("constraints", "constraints"),
    ("decisions", "decisions"),
    ("procedures", "procedures"),
    ("preferences", "user preferences"),
];

#[derive(Args)]
pub struct RecallArgs {
    /// Scope to recall from (e.g. my-repo)
    #[arg(long, short = 's')]
    pub scope: Option<String>,

    /// Task hint — guides recall toward a specific topic (e.g. "auth", "deployment")
    #[arg(long)]
    pub task: Option<String>,

    /// Additional query terms to include in recall (may be repeated)
    #[arg(long = "also")]
    pub also: Vec<String>,

    /// Maximum results per category (default: 5)
    #[arg(long, default_value = "5")]
    pub limit: usize,
}

pub async fn run(
    args: RecallArgs,
    db: &Arc<dyn Database>,
    config: &Config,
    json: bool,
    agent: bool,
) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    // Build the list of (category_key, query) pairs
    let mut queries: Vec<(&str, String)> = STARTUP_CATEGORIES
        .iter()
        .map(|(key, query)| {
            let q = if let Some(ref task) = args.task {
                format!("{} {}", query, task)
            } else {
                query.to_string()
            };
            (*key, q)
        })
        .collect();

    for extra in &args.also {
        queries.push(("extra", extra.clone()));
    }

    let mode: SearchMode = config.search.mode.parse().unwrap_or(SearchMode::HybridRRF);

    // Track seen IDs to deduplicate across categories
    let mut seen: HashMap<String, bool> = HashMap::new();

    // category_key → Vec of compact result entries
    let mut buckets: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut total = 0usize;

    for (cat_key, query) in &queries {
        let opts = SearchOptions {
            query: query.clone(),
            mode: mode.clone(),
            limit: args.limit,
            scope_filter: args.scope.clone(),
            type_filter: None,
            min_score: None,
            min_quality: None,
            include_neighbors: false,
            neighbor_depth: None,
            neighbor_decay: None,
            neighbor_min_score: None,
            neighbor_limit: None,
            edge_types: None,
            intent: args.task.clone(),
            max_age_days: None,
        };

        let resp = match search(
            pool,
            &opts,
            &config.embeddings.model,
            config.embeddings.enabled,
            config.search.min_score,
            &config.search,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("recall search '{}' failed: {}", query, e);
                continue;
            }
        };

        let bucket = buckets.entry(cat_key.to_string()).or_default();

        for r in resp.results {
            if seen.contains_key(&r.id) {
                continue;
            }
            seen.insert(r.id.clone(), true);
            total += 1;

            let content_preview = if r.content.len() > 300 {
                format!("{}…", voidm_core::search::safe_truncate(&r.content, 300))
            } else {
                r.content.clone()
            };

            bucket.push(serde_json::json!({
                "id": r.id,
                "score": (r.score * 100.0).round() / 100.0,
                "type": r.memory_type,
                "content": content_preview,
            }));
        }
    }

    // Output
    if agent || json {
        let mut out = serde_json::Map::new();
        out.insert("scope".into(), serde_json::json!(args.scope));
        out.insert("total".into(), serde_json::json!(total));
        let mut cats = serde_json::Map::new();
        for (key, entries) in &buckets {
            cats.insert(key.clone(), serde_json::json!(entries));
        }
        out.insert("categories".into(), serde_json::Value::Object(cats));
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out))?);
    } else {
        if total == 0 {
            println!("No memories found.");
            if args.scope.is_some() {
                println!("Hint: use `voidm scopes list` to see available scopes.");
            }
            return Ok(());
        }

        println!("Recall — {} unique memories\n", total);

        for (cat_key, _query) in &queries {
            let entries = match buckets.get(*cat_key) {
                Some(e) if !e.is_empty() => e,
                _ => continue,
            };
            let label = STARTUP_CATEGORIES
                .iter()
                .find(|(k, _)| k == cat_key)
                .map(|(_, q)| *q)
                .unwrap_or(cat_key);
            println!("## {}", label.to_uppercase());
            for entry in entries {
                let id = entry["id"].as_str().unwrap_or("");
                let score = entry["score"].as_f64().unwrap_or(0.0);
                let typ = entry["type"].as_str().unwrap_or("");
                let content = entry["content"].as_str().unwrap_or("");
                println!("[{:.2}] {} ({})", score, &id[..8.min(id.len())], typ);
                println!("  {}", content);
                println!();
            }
        }
    }

    Ok(())
}
