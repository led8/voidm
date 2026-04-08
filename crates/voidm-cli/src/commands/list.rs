use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::{crud, db::Database, Config};

#[derive(Args)]
pub struct ListArgs {
    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Filter by memory type
    #[arg(long, short = 't')]
    pub r#type: Option<String>,

    /// Maximum results
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

pub async fn run(
    args: ListArgs,
    db: &Arc<dyn Database>,
    _config: &Config,
    json: bool,
) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    let memories = crud::list_memories(
        pool,
        args.scope.as_deref(),
        args.r#type.as_deref(),
        args.limit,
    )
    .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&memories)?);
    } else {
        if memories.is_empty() {
            println!("No memories found.");
            return Ok(());
        }
        for m in &memories {
            let preview = if m.content.len() > 80 {
                format!("{}...", &m.content[..80])
            } else {
                m.content.clone()
            };
            println!(
                "{} [{}] imp:{} {}",
                m.id, m.memory_type, m.importance, m.created_at
            );
            if let Some(ref t) = m.title {
                println!("  Title: {}", t);
            }
            println!("  {}", preview);
            if !m.scopes.is_empty() {
                println!("  Scopes: {}", m.scopes.join(", "));
            }
            println!();
        }
    }
    Ok(())
}
