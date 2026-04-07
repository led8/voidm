use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use std::sync::Arc;
use voidm_core::{db::Database, ontology};

// ─── CLI types ────────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum ConflictsCommands {
    /// List all unresolved CONTRADICTS edges with context
    List(ConflictsListArgs),
    /// Resolve a conflict: keep one side, mark the other as superseded
    Resolve(ConflictsResolveArgs),
}

#[derive(Args)]
pub struct ConflictsListArgs {
    /// Only show conflicts involving this scope
    #[arg(long, short)]
    pub scope: Option<String>,
}

#[derive(Args)]
pub struct ConflictsResolveArgs {
    /// Ontology edge ID of the CONTRADICTS edge to resolve
    pub edge_id: i64,
    /// ID of the node to keep (the winner); the other is marked superseded
    #[arg(long)]
    pub keep: String,
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

pub async fn run(cmd: ConflictsCommands, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    match cmd {
        ConflictsCommands::List(args) => run_list(args, pool, json).await,
        ConflictsCommands::Resolve(args) => run_resolve(args, pool, json).await,
    }
}

// ─── list ─────────────────────────────────────────────────────────────────────

async fn run_list(args: ConflictsListArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let conflicts = ontology::list_conflicts(pool, args.scope.as_deref()).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&conflicts)?);
        return Ok(());
    }

    if conflicts.is_empty() {
        println!("No unresolved conflicts.");
        println!("Conflicts are created when 'voidm ontology concept add --enrich' detects a CONTRADICTS relation.");
        return Ok(());
    }

    println!("{} conflict(s):\n", conflicts.len());
    for c in &conflicts {
        println!("  Edge #{}: CONTRADICTS", c.edge_id);
        println!(
            "    [{}] {} {}",
            &c.from_id[..8],
            c.from_kind,
            c.from_name.as_deref().unwrap_or("(memory)")
        );
        if let Some(ref desc) = c.from_description {
            println!("        {}", desc);
        }
        println!("    ↕ CONTRADICTS");
        println!(
            "    [{}] {} {}",
            &c.to_id[..8],
            c.to_kind,
            c.to_name.as_deref().unwrap_or("(memory)")
        );
        if let Some(ref desc) = c.to_description {
            println!("        {}", desc);
        }
        println!();
        println!(
            "    Resolve: voidm conflicts resolve {} --keep <id>",
            c.edge_id
        );
        println!();
    }

    Ok(())
}

// ─── resolve ──────────────────────────────────────────────────────────────────

async fn run_resolve(args: ConflictsResolveArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    // Load the CONTRADICTS edge
    let conflict = ontology::get_conflict(pool, args.edge_id).await?;

    // Validate --keep is one of the two endpoints
    let winner_id = ontology::resolve_concept_id(pool, &args.keep)
        .await
        .or_else(|_| -> Result<String> { Ok(args.keep.clone()) })?;

    let loser_id = if winner_id == conflict.from_id || conflict.from_id.starts_with(&winner_id) {
        conflict.to_id.clone()
    } else if winner_id == conflict.to_id || conflict.to_id.starts_with(&winner_id) {
        conflict.from_id.clone()
    } else {
        bail!(
            "--keep '{}' is not one of the conflict's endpoints.\n\
             Endpoints: {} and {}",
            args.keep,
            &conflict.from_id[..8],
            &conflict.to_id[..8]
        );
    };

    ontology::resolve_conflict(pool, args.edge_id, &winner_id, &loser_id).await?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "resolved": true,
                "edge_id": args.edge_id,
                "winner": &winner_id[..8.min(winner_id.len())],
                "loser": &loser_id[..8.min(loser_id.len())],
            })
        );
    } else {
        println!("Conflict #{} resolved.", args.edge_id);
        println!(
            "  Winner (kept):      [{}]",
            &winner_id[..8.min(winner_id.len())]
        );
        println!(
            "  Loser (superseded): [{}]",
            &loser_id[..8.min(loser_id.len())]
        );
        println!("  CONTRADICTS edge removed. INVALIDATES edge created.");
    }

    Ok(())
}
