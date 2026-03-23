use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use voidm_core::{crud, models::EdgeType, resolve_id};

#[derive(Args)]
pub struct LinkArgs {
    /// Source memory ID or short prefix
    pub from: String,
    /// Edge type (SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES, RELATES_TO)
    pub rel: String,
    /// Target memory ID or short prefix
    pub to: String,
    /// Note (required for RELATES_TO)
    #[arg(long)]
    pub note: Option<String>,
}

pub async fn run(args: LinkArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let edge_type: EdgeType = args.rel.parse()?;
    let from = resolve_id(pool, &args.from).await?;
    let to = resolve_id(pool, &args.to).await?;
    let resp = crud::link_memories(pool, &from, &edge_type, &to, args.note.as_deref()).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Linked: {} {} {}", resp.from, resp.rel, resp.to);
        if let Some(ref w) = resp.conflict_warning {
            eprintln!("Warning: {}", w.message);
        }
    }
    Ok(())
}
