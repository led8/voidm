use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::db::Database;

#[derive(Args)]
pub struct StaleArgs {
    /// Only show memories older than this many days (default: 90)
    #[arg(long, default_value = "90")]
    pub older_than: u32,

    /// Filter by scope prefix
    #[arg(long, short = 's')]
    pub scope: Option<String>,

    /// Maximum results to return (default: 20)
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

pub async fn run(args: StaleArgs, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    let like = args.scope.as_deref().map(|s| format!("{}%", s));

    // Query memories older than N days, ordered by age desc
    let rows: Vec<(String, String, String, i64, String)> = if let Some(ref pattern) = like {
        sqlx::query_as(
            "SELECT DISTINCT m.id, m.type, m.content, m.importance, m.created_at
             FROM memories m
             JOIN memory_scopes ms ON ms.memory_id = m.id
             WHERE ms.scope LIKE ?
               AND julianday('now') - julianday(m.created_at) > ?
             ORDER BY m.created_at ASC
             LIMIT ?",
        )
        .bind(pattern)
        .bind(args.older_than as f64)
        .bind(args.limit as i64)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, type, content, importance, created_at
             FROM memories
             WHERE julianday('now') - julianday(created_at) > ?
             ORDER BY created_at ASC
             LIMIT ?",
        )
        .bind(args.older_than as f64)
        .bind(args.limit as i64)
        .fetch_all(pool)
        .await?
    };

    if json {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|(id, typ, content, importance, created_at)| {
                let age_days = voidm_core::search::compute_age_days(created_at).unwrap_or(0);
                let preview = if content.len() > 120 {
                    format!("{}…", &content[..120])
                } else {
                    content.clone()
                };
                serde_json::json!({
                    "id": id,
                    "type": typ,
                    "importance": importance,
                    "age_days": age_days,
                    "created_at": created_at,
                    "content": preview,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        if rows.is_empty() {
            println!(
                "No memories older than {} days{}.",
                args.older_than,
                args.scope
                    .as_deref()
                    .map(|s| format!(" in scope '{}'", s))
                    .unwrap_or_default()
            );
            return Ok(());
        }

        println!(
            "{} memories older than {} days (oldest first):\n",
            rows.len(),
            args.older_than
        );
        for (id, typ, content, importance, created_at) in &rows {
            let age_days = voidm_core::search::compute_age_days(created_at).unwrap_or(0);
            let preview = if content.len() > 100 {
                format!("{}…", &content[..100])
            } else {
                content.clone()
            };
            println!(
                "[{}d] {} ({}) importance={}",
                age_days,
                &id[..8.min(id.len())],
                typ,
                importance
            );
            println!("  {}", preview);
            println!();
        }
        println!("Hint: use `voidm update <id>` to refresh or `voidm delete <id>` to remove stale memories.");
    }

    Ok(())
}
