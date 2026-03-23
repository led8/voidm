use anyhow::Result;
use clap::Args;
use voidm_core::{Config, DbPathSource};

#[derive(Args, Clone)]
pub struct InfoArgs {}

pub fn run(
    _args: InfoArgs,
    config: &Config,
    db_override: Option<&str>,
    sqlite_path_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let resolution = config.resolve_db_path(db_override, sqlite_path_override);
    let db_path = resolution.path;
    let db_exists = db_path.exists();
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).ok();

    let config_path = voidm_core::config_path_display();

    let embedding_model = &config.embeddings.model;
    let embeddings_enabled = config.embeddings.enabled;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "db": {
                    "path": db_path.display().to_string(),
                    "exists": db_exists,
                    "size_bytes": db_size,
                    "source": resolution.source.as_str(),
                },
                "config": {
                    "path": config_path,
                },
                "embeddings": {
                    "enabled": embeddings_enabled,
                    "model": embedding_model,
                },
                "search": {
                    "default_mode": config.search.mode,
                    "min_score": (config.search.min_score as f64 * 100.0).round() / 100.0,
                    "default_limit": config.search.default_limit,
                }
            }))?
        );
    } else {
        println!("Database");
        println!("  Path:    {}", db_path.display());
        println!(
            "  Exists:  {}",
            if db_exists {
                "yes"
            } else {
                "no (will be created on first write)"
            }
        );
        if let Some(sz) = db_size {
            println!("  Size:    {}", human_size(sz));
        }
        println!("  Source:  {}", resolution.source.as_str());
        if matches!(resolution.source, DbPathSource::CodexSandboxDefault) {
            println!("  Note:    using a writable fallback path because the Codex sandbox restricts writes outside approved roots");
        }
        println!();
        println!("Config");
        println!("  Path:    {}", config_path);
        println!();
        println!("Embeddings");
        println!("  Enabled: {}", embeddings_enabled);
        println!("  Model:   {}", embedding_model);
        println!();
        println!("Search defaults");
        println!("  Mode:      {}", config.search.mode);
        println!("  Min score: {} (hybrid only)", config.search.min_score);
        println!("  Limit:     {}", config.search.default_limit);
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}
