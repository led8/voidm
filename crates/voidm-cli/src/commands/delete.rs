use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_core::{crud, db::Database, resolve_id};

#[derive(Args)]
pub struct DeleteArgs {
    /// Memory ID or short prefix (min 4 chars)
    pub id: String,
    /// Skip confirmation prompt
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(args: DeleteArgs, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    let id = match resolve_id(pool, &args.id).await {
        Ok(id) => id,
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "error": e.to_string(), "id": args.id })
                );
            } else {
                eprintln!("Error: {}", e);
            }
            std::process::exit(1);
        }
    };

    if !args.yes && !json {
        eprint!("Delete memory '{}'? [y/N] ", id);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let deleted = crud::delete_memory(pool, &id).await?;
    if deleted {
        if json {
            println!("{}", serde_json::json!({ "deleted": true, "id": id }));
        } else {
            println!("Deleted memory '{}'", id);
        }
    } else {
        if json {
            println!(
                "{}",
                serde_json::json!({ "error": format!("Memory '{}' not found", id), "id": id })
            );
        } else {
            eprintln!("Error: Memory '{}' not found", id);
        }
        std::process::exit(1);
    }
    Ok(())
}
