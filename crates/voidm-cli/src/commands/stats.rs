use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::Config;

#[derive(Args)]
pub struct StatsArgs {}

pub async fn run(_args: StatsArgs, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    // Memory counts total + by type
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await?;

    let by_type: Vec<(String, i64)> =
        sqlx::query_as("SELECT type, COUNT(*) FROM memories GROUP BY type ORDER BY COUNT(*) DESC")
            .fetch_all(pool)
            .await?;

    // Scope count
    let scope_count: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT scope) FROM memory_scopes")
        .fetch_one(pool)
        .await?;

    // Tag counts (top 10)
    let all_tags: Vec<(String,)> = sqlx::query_as("SELECT tags FROM memories WHERE tags != '[]'")
        .fetch_all(pool)
        .await?;

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

    // Graph counts
    let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let edge_by_type: Vec<(String, i64)> = sqlx::query_as(
        "SELECT rel_type, COUNT(*) FROM graph_edges GROUP BY rel_type ORDER BY COUNT(*) DESC",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Embedding coverage
    let vec_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vec_memories")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

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
                "memories": {
                    "total": total,
                    "by_type": type_map,
                    "embedded": vec_count,
                    "embedding_coverage_pct": if total > 0 {
                        (vec_count as f64 / total as f64 * 100.0).round() as i64
                    } else { 0 }
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
        println!("Memories:  {} total", total);
        for (t, c) in &by_type {
            println!("  {:12} {}", t, c);
        }
        if vec_count < total {
            println!(
                "  Embedded:  {}/{} ({:.0}%)",
                vec_count,
                total,
                vec_count as f64 / total.max(1) as f64 * 100.0
            );
        } else if total > 0 {
            println!("  Embedded:  {}/{} (100%)", vec_count, total);
        }
        println!("Scopes:    {}", scope_count);
        if !top_tags.is_empty() {
            let tag_str: Vec<String> = top_tags
                .iter()
                .map(|(t, c)| format!("{}({})", t, c))
                .collect();
            println!("Top tags:  {}", tag_str.join(", "));
        }
        println!("Graph:     {} nodes, {} edges", node_count, edge_count);
        for (rel, cnt) in &edge_by_type {
            println!("  {:20} {}", rel, cnt);
        }
        println!("DB size:   {}", human_size(db_size));
    }
    Ok(())
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
