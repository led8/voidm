use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::{db::Database, Config};

#[derive(Args)]
pub struct StatsArgs {
    /// Filter statistics to a specific scope prefix (e.g. my-repo)
    #[arg(long, short = 's')]
    pub scope: Option<String>,
}

pub async fn run(args: StatsArgs, db: &Arc<dyn Database>, config: &Config, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    let scope = args.scope.as_deref();

    // Memory counts total + by type
    let (total, by_type, scope_count, all_tags, vec_count, oldest_age_days, newest_age_days) =
        if let Some(s) = scope {
            let like = format!("{}%", s);
            let total: i64 = sqlx::query_scalar(
                "SELECT COUNT(DISTINCT m.id) FROM memories m
                 JOIN memory_scopes ms ON ms.memory_id = m.id WHERE ms.scope LIKE ?",
            )
            .bind(&like)
            .fetch_one(pool)
            .await?;

            let by_type: Vec<(String, i64)> = sqlx::query_as(
                "SELECT m.type, COUNT(DISTINCT m.id) FROM memories m
                 JOIN memory_scopes ms ON ms.memory_id = m.id
                 WHERE ms.scope LIKE ?
                 GROUP BY m.type ORDER BY COUNT(DISTINCT m.id) DESC",
            )
            .bind(&like)
            .fetch_all(pool)
            .await?;

            let scope_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(DISTINCT scope) FROM memory_scopes WHERE scope LIKE ?",
            )
            .bind(&like)
            .fetch_one(pool)
            .await?;

            let all_tags: Vec<(String,)> = sqlx::query_as(
                "SELECT DISTINCT m.tags FROM memories m
                 JOIN memory_scopes ms ON ms.memory_id = m.id
                 WHERE ms.scope LIKE ? AND m.tags != '[]'",
            )
            .bind(&like)
            .fetch_all(pool)
            .await?;

            let vec_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(DISTINCT v.memory_id) FROM vec_memories v
                 JOIN memory_scopes ms ON ms.memory_id = v.memory_id
                 WHERE ms.scope LIKE ?",
            )
            .bind(&like)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

            let ages: Option<(Option<f64>, Option<f64>)> = sqlx::query_as(
                "SELECT MAX(julianday('now') - julianday(m.created_at)),
                        MIN(julianday('now') - julianday(m.created_at))
                 FROM memories m JOIN memory_scopes ms ON ms.memory_id = m.id
                 WHERE ms.scope LIKE ?",
            )
            .bind(&like)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let (oldest, newest) = ages.flatten_tuple();

            (total, by_type, scope_count, all_tags, vec_count, oldest, newest)
        } else {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
                .fetch_one(pool)
                .await?;

            let by_type: Vec<(String, i64)> = sqlx::query_as(
                "SELECT type, COUNT(*) FROM memories GROUP BY type ORDER BY COUNT(*) DESC",
            )
            .fetch_all(pool)
            .await?;

            let scope_count: i64 =
                sqlx::query_scalar("SELECT COUNT(DISTINCT scope) FROM memory_scopes")
                    .fetch_one(pool)
                    .await?;

            let all_tags: Vec<(String,)> =
                sqlx::query_as("SELECT tags FROM memories WHERE tags != '[]'")
                    .fetch_all(pool)
                    .await?;

            let vec_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vec_memories")
                .fetch_one(pool)
                .await
                .unwrap_or(0);

            let ages: Option<(Option<f64>, Option<f64>)> = sqlx::query_as(
                "SELECT MAX(julianday('now') - julianday(created_at)),
                        MIN(julianday('now') - julianday(created_at))
                 FROM memories",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let (oldest, newest) = ages.flatten_tuple();

            (total, by_type, scope_count, all_tags, vec_count, oldest, newest)
        };

    // Tag counts (top 10)
    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (tags_json,) in &all_tags {
        let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
        for tag in tags {
            *tag_counts.entry(tag).or_default() += 1;
        }
    }
    let mut top_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
    top_tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top_tags.truncate(10);

    // Graph counts (scoped if scope provided)
    let (node_count, edge_count, edge_by_type) = if scope.is_none() {
        let nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        let by_rel: Vec<(String, i64)> = sqlx::query_as(
            "SELECT rel_type, COUNT(*) FROM graph_edges GROUP BY rel_type ORDER BY COUNT(*) DESC",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        (nodes, edges, by_rel)
    } else {
        (0i64, 0i64, vec![]) // graph stats are global only
    };

    // DB file size
    let db_path = config.db_path(None);
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    if json {
        let mut type_map = serde_json::Map::new();
        for (t, c) in &by_type {
            type_map.insert(t.clone(), serde_json::json!(c));
        }
        let edge_map: serde_json::Map<String, serde_json::Value> = edge_by_type
            .iter()
            .map(|(t, c)| (t.clone(), serde_json::json!(c)))
            .collect();

        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "scope": scope,
                "memories": {
                    "total": total,
                    "by_type": type_map,
                    "embedded": vec_count,
                    "embedding_coverage_pct": if total > 0 {
                        (vec_count as f64 / total as f64 * 100.0).round() as i64
                    } else { 0 },
                    "oldest_age_days": oldest_age_days.map(|d| d.round() as i64),
                    "newest_age_days": newest_age_days.map(|d| d.round() as i64),
                },
                "scopes": scope_count,
                "tags": top_tags.iter().map(|(t, c)| serde_json::json!({"tag": t, "count": c})).collect::<Vec<_>>(),
                "graph": {
                    "nodes": node_count,
                    "edges": edge_count,
                    "by_rel_type": edge_map
                },
                "db_size_bytes": db_size,
            }))?
        );
    } else {
        if let Some(s) = scope {
            println!("Scope:     {} (prefix match)", s);
        }
        println!("Memories:  {} total", total);
        for (t, c) in &by_type {
            println!("  {:12} {}", t, c);
        }
        if total > 0 {
            if vec_count < total {
                println!(
                    "  Embedded:  {}/{} ({:.0}%)",
                    vec_count,
                    total,
                    vec_count as f64 / total as f64 * 100.0
                );
            } else {
                println!("  Embedded:  {}/{} (100%)", vec_count, total);
            }
            if let Some(oldest) = oldest_age_days {
                println!(
                    "  Age range: {}d – {}d",
                    newest_age_days.unwrap_or(0.0).round() as i64,
                    oldest.round() as i64
                );
            }
        }
        println!("Scopes:    {}", scope_count);
        if !top_tags.is_empty() {
            let tag_str: Vec<String> = top_tags
                .iter()
                .map(|(t, c)| format!("{}({})", t, c))
                .collect();
            println!("Top tags:  {}", tag_str.join(", "));
        }
        if scope.is_none() {
            println!("Graph:     {} nodes, {} edges", node_count, edge_count);
            for (rel, cnt) in &edge_by_type {
                println!("  {:20} {}", rel, cnt);
            }
            println!("DB size:   {}", human_size(db_size));
        }
    }
    Ok(())
}

trait FlattenTuple {
    fn flatten_tuple(self) -> (Option<f64>, Option<f64>);
}

impl FlattenTuple for Option<(Option<f64>, Option<f64>)> {
    fn flatten_tuple(self) -> (Option<f64>, Option<f64>) {
        match self {
            Some((a, b)) => (a, b),
            None => (None, None),
        }
    }
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}
