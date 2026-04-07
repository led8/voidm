use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::{crud, db::Database, models::EdgeType, resolve_id};

#[derive(Args)]
pub struct UnlinkArgs {
    /// Source memory ID or short prefix
    pub from: String,
    /// Edge type: RELATES_TO, SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES
    pub rel: String,
    /// Target memory ID or short prefix
    pub to: String,
}

pub async fn run(args: UnlinkArgs, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    let edge_type: EdgeType = args.rel.parse()?;
    let from = resolve_id(pool, &args.from).await?;
    let to = resolve_id(pool, &args.to).await?;
    let removed = crud::unlink_memories(pool, &from, &edge_type, &to).await?;

    if removed {
        if json {
            println!(
                "{}",
                serde_json::json!({ "removed": true, "from": from, "rel": args.rel, "to": to })
            );
        } else {
            println!("Unlinked: {} {} {}", from, args.rel, to);
        }
    } else {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "error": format!("Edge not found: {} --[{}]--> {}", from, args.rel, to),
                    "from": from, "rel": args.rel, "to": to
                })
            );
        } else {
            eprintln!("Error: Edge not found: {} --[{}]--> {}", from, args.rel, to);
            eprintln!(
                "Hint: Use 'voidm graph neighbors {}' to see existing edges.",
                from
            );
        }
        std::process::exit(1);
    }
    Ok(())
}
