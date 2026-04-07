use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::{compute_age_days, crud, db::Database};

#[derive(Args)]
pub struct WhyArgs {
    /// Memory ID (full or short prefix)
    pub id: String,
}

pub async fn run(args: WhyArgs, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    let full_id = crud::resolve_id(pool, &args.id).await?;

    let memory = crud::get_memory(pool, &full_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Memory '{}' not found", full_id))?;

    let edges = voidm_core::get_edges_for_memory(pool, &full_id).await?;

    let age_days = compute_age_days(&memory.created_at);

    if json {
        let edges_json: Vec<serde_json::Value> = edges
            .iter()
            .map(|(dir, other_id, rel_type, note)| {
                serde_json::json!({
                    "direction": dir,
                    "other_id": other_id,
                    "rel_type": rel_type,
                    "note": note,
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": memory.id,
                "type": memory.memory_type,
                "importance": memory.importance,
                "age_days": age_days,
                "created_at": memory.created_at,
                "updated_at": memory.updated_at,
                "scopes": memory.scopes,
                "tags": memory.tags,
                "content": memory.content,
                "edges": edges_json,
            }))?
        );
    } else {
        println!("Memory: {}", memory.id);
        println!("  Type:       {}", memory.memory_type);
        println!("  Importance: {}/10", memory.importance);
        if let Some(days) = age_days {
            println!("  Age:        {} days (created {})", days, memory.created_at);
        } else {
            println!("  Created:    {}", memory.created_at);
        }
        if !memory.scopes.is_empty() {
            println!("  Scopes:     {}", memory.scopes.join(", "));
        }
        if !memory.tags.is_empty() {
            println!("  Tags:       {}", memory.tags.join(", "));
        }
        println!();
        println!("Content:");
        println!("  {}", memory.content);

        if !edges.is_empty() {
            println!();
            println!("Graph edges ({}):", edges.len());
            for (dir, other_id, rel_type, note) in &edges {
                let short = &other_id[..8.min(other_id.len())];
                let arrow = if dir == "outgoing" {
                    format!("→ {} {}", rel_type, short)
                } else {
                    format!("← {} {}", rel_type, short)
                };
                if let Some(n) = note {
                    println!("  {} ({})", arrow, n);
                } else {
                    println!("  {}", arrow);
                }
            }
        } else {
            println!();
            println!("No graph edges.");
        }
    }

    Ok(())
}
