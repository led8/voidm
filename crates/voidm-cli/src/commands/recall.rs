use anyhow::Result;
use clap::Args;
use std::collections::HashMap;
use std::sync::Arc;
use voidm_core::{
    crud,
    db::Database,
    models::Memory,
    search::{search, SearchMode, SearchOptions},
    Config,
};

#[derive(Clone, Copy)]
struct RecallBucketSpec {
    key: &'static str,
    label: &'static str,
    query: &'static str,
    search_type_filter: Option<&'static str>,
    fallback_type_filter: Option<&'static str>,
    allow_type_match: bool,
    content_prefixes: &'static [&'static str],
    contexts: &'static [&'static str],
}

const STARTUP_BUCKETS: &[RecallBucketSpec] = &[
    RecallBucketSpec {
        key: "architecture",
        label: "architecture",
        query: "architecture",
        search_type_filter: Some("conceptual"),
        fallback_type_filter: Some("conceptual"),
        allow_type_match: true,
        content_prefixes: &["Architecture:"],
        contexts: &[],
    },
    RecallBucketSpec {
        key: "constraints",
        label: "constraints",
        query: "constraints",
        search_type_filter: None,
        fallback_type_filter: None,
        allow_type_match: false,
        content_prefixes: &["Constraint:"],
        contexts: &["gotcha", "reference"],
    },
    RecallBucketSpec {
        key: "decisions",
        label: "decisions",
        query: "decisions",
        search_type_filter: None,
        fallback_type_filter: None,
        allow_type_match: false,
        content_prefixes: &["Decision:"],
        contexts: &["decision"],
    },
    RecallBucketSpec {
        key: "procedures",
        label: "procedures",
        query: "procedures",
        search_type_filter: Some("procedural"),
        fallback_type_filter: Some("procedural"),
        allow_type_match: true,
        content_prefixes: &["Procedure:"],
        contexts: &["procedure"],
    },
    RecallBucketSpec {
        key: "preferences",
        label: "user preferences",
        query: "user preferences",
        search_type_filter: None,
        fallback_type_filter: None,
        allow_type_match: false,
        content_prefixes: &["Preference:"],
        contexts: &[],
    },
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
    let mode: SearchMode = config.search.mode.parse().unwrap_or(SearchMode::Hybrid);

    // Track seen IDs to deduplicate across categories
    let mut seen: HashMap<String, bool> = HashMap::new();

    // category_key → Vec of compact result entries
    let mut buckets: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut total = 0usize;

    for bucket in STARTUP_BUCKETS {
        let query = compose_query(bucket.query, args.task.as_deref());
        let opts = SearchOptions {
            query,
            mode: mode.clone(),
            limit: args.limit,
            scope_filter: args.scope.clone(),
            type_filter: bucket.search_type_filter.map(str::to_string),
            min_score: Some(0.0),
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
                tracing::warn!("recall search '{}' failed: {}", bucket.query, e);
                continue;
            }
        };

        let entries = buckets.entry(bucket.key.to_string()).or_default();

        for r in resp.results {
            if entries.len() >= args.limit {
                break;
            }
            if !search_result_matches_bucket(
                &r.memory_type,
                &r.content,
                r.context.as_deref(),
                bucket,
            ) {
                continue;
            }
            if seen.contains_key(&r.id) {
                continue;
            }
            seen.insert(r.id.clone(), true);
            total += 1;
            entries.push(recall_entry(&r.id, r.score, &r.memory_type, &r.content));
        }

        fill_bucket_from_recent_memories(
            pool,
            entries,
            &mut seen,
            &mut total,
            bucket,
            args.scope.as_deref(),
            args.limit,
        )
        .await?;
    }

    for extra in &args.also {
        let opts = SearchOptions {
            query: extra.clone(),
            mode: mode.clone(),
            limit: args.limit,
            scope_filter: args.scope.clone(),
            type_filter: None,
            min_score: Some(0.0),
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
                tracing::warn!("recall search '{}' failed: {}", extra, e);
                continue;
            }
        };

        let entries = buckets.entry("extra".to_string()).or_default();
        for r in resp.results {
            if entries.len() >= args.limit {
                break;
            }
            if seen.contains_key(&r.id) {
                continue;
            }
            seen.insert(r.id.clone(), true);
            total += 1;
            entries.push(recall_entry(&r.id, r.score, &r.memory_type, &r.content));
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
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(out))?
        );
    } else {
        if total == 0 {
            println!("No memories found.");
            if args.scope.is_some() {
                println!("Hint: use `voidm scopes list` to see available scopes.");
            }
            return Ok(());
        }

        println!("Recall — {} unique memories\n", total);

        for bucket in STARTUP_BUCKETS {
            let entries = match buckets.get(bucket.key) {
                Some(e) if !e.is_empty() => e,
                _ => continue,
            };
            println!("## {}", bucket.label.to_uppercase());
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

        if let Some(entries) = buckets.get("extra").filter(|entries| !entries.is_empty()) {
            println!("## EXTRA");
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

async fn fill_bucket_from_recent_memories(
    pool: &sqlx::SqlitePool,
    entries: &mut Vec<serde_json::Value>,
    seen: &mut HashMap<String, bool>,
    total: &mut usize,
    bucket: &RecallBucketSpec,
    scope_filter: Option<&str>,
    limit: usize,
) -> Result<()> {
    if entries.len() >= limit {
        return Ok(());
    }

    let fetch_limit = limit.saturating_mul(6).max(limit);
    let memories =
        crud::list_memories(pool, scope_filter, bucket.fallback_type_filter, fetch_limit).await?;

    for memory in memories {
        if entries.len() >= limit {
            break;
        }
        if seen.contains_key(&memory.id) || !memory_matches_bucket(&memory, bucket) {
            continue;
        }
        seen.insert(memory.id.clone(), true);
        *total += 1;
        entries.push(recall_entry(
            &memory.id,
            0.0,
            &memory.memory_type,
            &memory.content,
        ));
    }

    Ok(())
}

fn compose_query(base: &str, task: Option<&str>) -> String {
    match task {
        Some(task) if !task.trim().is_empty() => format!("{base} {task}"),
        _ => base.to_string(),
    }
}

fn recall_entry(id: &str, score: f32, memory_type: &str, content: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "score": (score * 100.0).round() / 100.0,
        "type": memory_type,
        "content": preview_content(content),
    })
}

fn preview_content(content: &str) -> String {
    if content.len() > 300 {
        format!("{}…", voidm_core::search::safe_truncate(content, 300))
    } else {
        content.to_string()
    }
}

fn memory_matches_bucket(memory: &Memory, bucket: &RecallBucketSpec) -> bool {
    search_result_matches_bucket(
        &memory.memory_type,
        &memory.content,
        memory.context.as_deref(),
        bucket,
    )
}

fn search_result_matches_bucket(
    memory_type: &str,
    content: &str,
    context: Option<&str>,
    bucket: &RecallBucketSpec,
) -> bool {
    let type_match = bucket.allow_type_match
        && bucket
            .fallback_type_filter
            .is_some_and(|expected| memory_type.eq_ignore_ascii_case(expected));

    type_match
        || has_content_prefix(content, bucket.content_prefixes)
        || has_context(context, bucket.contexts)
}

fn has_content_prefix(content: &str, prefixes: &[&str]) -> bool {
    let normalized = content.trim_start().to_ascii_lowercase();
    prefixes
        .iter()
        .any(|prefix| normalized.starts_with(&prefix.to_ascii_lowercase()))
}

fn has_context(context: Option<&str>, expected_contexts: &[&str]) -> bool {
    context.is_some_and(|context| {
        expected_contexts
            .iter()
            .any(|expected| context.eq_ignore_ascii_case(expected))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(memory_type: &str, content: &str, context: Option<&str>) -> Memory {
        Memory {
            id: "id".to_string(),
            memory_type: memory_type.to_string(),
            content: content.to_string(),
            importance: 5,
            tags: vec![],
            metadata: serde_json::json!({}),
            scopes: vec!["scope".to_string()],
            created_at: "2026-04-08T00:00:00+00:00".to_string(),
            updated_at: "2026-04-08T00:00:00+00:00".to_string(),
            quality_score: Some(1.0),
            title: None,
            context: context.map(str::to_string),
        }
    }

    #[test]
    fn architecture_bucket_accepts_conceptual_memories() {
        let bucket = STARTUP_BUCKETS[0];
        assert!(memory_matches_bucket(
            &memory("conceptual", "A general architecture note", None),
            &bucket
        ));
    }

    #[test]
    fn decisions_bucket_uses_prefix_or_context_not_type_only() {
        let bucket = STARTUP_BUCKETS[2];
        assert!(memory_matches_bucket(
            &memory("semantic", "Decision: use RRF for hybrid ranking", None),
            &bucket
        ));
        assert!(memory_matches_bucket(
            &memory("semantic", "Use RRF for hybrid ranking", Some("decision")),
            &bucket
        ));
        assert!(!memory_matches_bucket(
            &memory("semantic", "A plain semantic note", None),
            &bucket
        ));
    }

    #[test]
    fn procedures_bucket_accepts_procedural_memories_by_type() {
        let bucket = STARTUP_BUCKETS[3];
        assert!(memory_matches_bucket(
            &memory("procedural", "Run cargo check before release", None),
            &bucket
        ));
    }

    #[test]
    fn constraints_bucket_recognizes_constraint_prefix() {
        let bucket = STARTUP_BUCKETS[1];
        assert!(memory_matches_bucket(
            &memory(
                "semantic",
                "Constraint: hybrid threshold must not hide all matches",
                None
            ),
            &bucket
        ));
    }
}
