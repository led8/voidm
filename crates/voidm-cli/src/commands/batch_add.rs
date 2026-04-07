use anyhow::{Context, Result};
use clap::Args;
use std::sync::Arc;
use voidm_core::{
    crud,
    db::Database,
    models::{AddMemoryRequest, MemoryType},
    Config,
};

/// Schema for a single memory in the batch JSON array.
#[derive(Debug, serde::Deserialize)]
struct BatchEntry {
    content: String,
    #[serde(rename = "type")]
    memory_type: String,
    #[serde(default)]
    scope: Vec<String>,
    #[serde(default)]
    tags: Option<String>,
    #[serde(default = "default_importance")]
    importance: i64,
}

fn default_importance() -> i64 {
    5
}

#[derive(Args)]
pub struct BatchAddArgs {
    /// Path to a JSON file containing an array of memory objects.
    /// Schema: [{"content":"...","type":"semantic","scope":["repo"],"tags":"a,b","importance":5}]
    #[arg(long = "from")]
    pub from: String,
}

pub async fn run(args: BatchAddArgs, db: &Arc<dyn Database>, config: &Config, json: bool) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    let raw = std::fs::read_to_string(&args.from)
        .with_context(|| format!("Failed to read batch file '{}'", args.from))?;

    let entries: Vec<BatchEntry> = serde_json::from_str(&raw)
        .context("Failed to parse batch file. Expected a JSON array of memory objects.")?;

    if entries.is_empty() {
        if json {
            println!("{}", serde_json::json!({"inserted": 0, "results": []}));
        } else {
            println!("Nothing to insert — batch file is empty.");
        }
        return Ok(());
    }

    // Validate all entries before inserting any
    for (i, entry) in entries.iter().enumerate() {
        entry
            .memory_type
            .parse::<MemoryType>()
            .with_context(|| format!("Entry {}: invalid type '{}'", i, entry.memory_type))?;
        if !(1..=10).contains(&entry.importance) {
            anyhow::bail!("Entry {}: importance {} is out of range (1–10)", i, entry.importance);
        }
        if entry.content.trim().is_empty() {
            anyhow::bail!("Entry {}: content must not be empty", i);
        }
    }

    // Insert sequentially, collecting results
    let mut results = Vec::new();
    let mut inserted = 0usize;

    for (i, entry) in entries.into_iter().enumerate() {
        let memory_type: MemoryType = entry.memory_type.parse()?;
        let tags: Vec<String> = entry
            .tags
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let req = AddMemoryRequest {
            id: None,
            content: entry.content,
            memory_type,
            scopes: entry.scope,
            tags,
            importance: entry.importance,
            metadata: serde_json::Value::Object(Default::default()),
            links: vec![],
            title: None,
            context: None,
        };

        match crud::add_memory(pool, req, config).await {
            Ok(resp) => {
                inserted += 1;
                results.push(serde_json::json!({
                    "index": i,
                    "id": resp.id,
                    "duplicate_warning": resp.duplicate_warning.map(|d| d.id),
                }));
            }
            Err(e) => {
                results.push(serde_json::json!({
                    "index": i,
                    "error": e.to_string(),
                }));
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "inserted": inserted,
                "results": results,
            }))?
        );
    } else {
        println!("Inserted {}/{} memories.", inserted, results.len());
        for r in &results {
            if let Some(id) = r.get("id") {
                let dup = r.get("duplicate_warning").and_then(|v| v.as_str());
                if let Some(d) = dup {
                    println!("  {} (near-duplicate of {})", id, d);
                } else {
                    println!("  {}", id);
                }
            } else if let Some(err) = r.get("error") {
                println!("  [{}] ERROR: {}", r["index"], err);
            }
        }
    }

    Ok(())
}
