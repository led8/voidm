use anyhow::Result;
use clap::Subcommand;
use std::sync::Arc;
use voidm_core::{db::Database, embeddings, vector, Config};

#[derive(Subcommand)]
pub enum ModelsCommands {
    /// List available embedding models
    List,
    /// Download a model
    Download {
        /// Model name
        model: String,
    },
    /// Re-embed all memories with the current (or specified) model
    Reembed {
        /// Model to use (default: configured model)
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value = "32")]
        batch_size: usize,
    },
}

pub fn run_list(json: bool) -> Result<()> {
    let models = embeddings::list_models();
    if json {
        println!("{}", serde_json::to_string_pretty(&models)?);
    } else {
        println!("{:<35} {:>6}  {}", "Model", "Dims", "Description");
        println!("{}", "-".repeat(70));
        for m in &models {
            println!("{:<35} {:>6}  {}", m.name, m.dims, m.description);
        }
    }
    Ok(())
}

pub async fn run(
    cmd: ModelsCommands,
    db: &Arc<dyn Database>,
    config: &Config,
    json: bool,
) -> Result<()> {
    let pool = db.sqlite_pool().expect("SQLite backend required");
    match cmd {
        ModelsCommands::List => run_list(json),
        ModelsCommands::Download { model } => {
            eprintln!("Downloading model '{}'...", model);
            // fastembed downloads automatically on first use
            let _ = embeddings::embed_text(&model, "warmup")?;
            eprintln!("Model '{}' ready.", model);
            Ok(())
        }
        ModelsCommands::Reembed { model, batch_size } => {
            let model_name = model.as_deref().unwrap_or(&config.embeddings.model);
            eprintln!("Re-embedding all memories with '{}'...", model_name);

            // Get dimension from a test embed
            let test = embeddings::embed_text(model_name, "test")?;
            let dim = test.len();

            vector::reembed_all(pool, model_name, dim, batch_size).await?;
            eprintln!("Done.");
            Ok(())
        }
    }
}
