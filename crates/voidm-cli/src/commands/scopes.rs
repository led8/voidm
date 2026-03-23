use anyhow::Result;
use clap::Subcommand;
use sqlx::SqlitePool;
use voidm_core::crud;

#[derive(Subcommand)]
pub enum ScopesCommands {
    /// List all known scopes
    List,
}

pub async fn run(cmd: ScopesCommands, pool: &SqlitePool, json: bool) -> Result<()> {
    match cmd {
        ScopesCommands::List => {
            let scopes = crud::list_scopes(pool).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&scopes)?);
            } else {
                if scopes.is_empty() {
                    println!("No scopes found.");
                } else {
                    for s in &scopes {
                        println!("{}", s);
                    }
                }
            }
        }
    }
    Ok(())
}
