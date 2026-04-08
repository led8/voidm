use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::{crud::UpdateMemoryPatch, db::Database, models::MemoryType, Config};

#[derive(Args)]
pub struct UpdateMemoryArgs {
    /// Memory ID (or short prefix, min 4 chars)
    pub id: String,

    /// New content (replaces existing)
    #[arg(long)]
    pub content: Option<String>,

    /// New memory type: episodic, semantic, procedural, conceptual, contextual
    #[arg(long, short = 't')]
    pub r#type: Option<String>,

    /// New tags (comma-separated, replaces existing tags)
    #[arg(long)]
    pub tags: Option<String>,

    /// New importance (1–10)
    #[arg(long)]
    pub importance: Option<i64>,

    /// New title (max 200 chars)
    #[arg(long)]
    pub title: Option<String>,

    /// New context label: gotcha, decision, procedure, reference
    #[arg(long)]
    pub context: Option<String>,
}

pub async fn run(
    args: UpdateMemoryArgs,
    db: &Arc<dyn Database>,
    config: &Config,
    json: bool,
    agent: bool,
) -> Result<()> {
    let memory_type: Option<MemoryType> = args.r#type.as_deref().map(|t| t.parse()).transpose()?;

    let tags: Option<Vec<String>> = args.tags.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let patch = UpdateMemoryPatch {
        content: args.content,
        memory_type,
        tags,
        importance: args.importance,
        title: args.title,
        context: args.context,
    };

    let mem = db.update_memory_full(&args.id, patch, config).await?;

    if agent {
        println!("{}", serde_json::json!({ "id": mem.id }));
    } else if json {
        println!("{}", serde_json::to_string_pretty(&mem)?);
    } else {
        println!("Updated: {}", mem.id);
        println!("Type: {}  Importance: {}", mem.memory_type, mem.importance);
        if let Some(qs) = mem.quality_score {
            println!("Quality: {:.2}", qs);
        }
        println!("Updated at: {}", mem.updated_at);
    }

    Ok(())
}
