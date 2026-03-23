use anyhow::Result;
use clap::Subcommand;
use voidm_core::{config::save_config, Config};

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current config
    Show,
    /// Set a config value (key=value dot-notation)
    Set {
        /// Config key (e.g. embeddings.model)
        key: String,
        /// New value
        value: String,
    },
}

pub async fn run(cmd: &ConfigCommands, json: bool) -> Result<()> {
    match cmd {
        ConfigCommands::Show => {
            let config = Config::load();
            if json {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else {
                println!("{}", toml::to_string_pretty(&config)?);
            }
        }
        ConfigCommands::Set { key, value } => {
            let mut config = Config::load();
            apply_config_key(&mut config, key, value)?;
            save_config(&config)?;
            eprintln!("Set {} = {}", key, value);
        }
    }
    Ok(())
}

fn apply_config_key(config: &mut Config, key: &str, value: &str) -> Result<()> {
    match key {
        "embeddings.model" => config.embeddings.model = value.to_string(),
        "embeddings.enabled" => config.embeddings.enabled = value.parse()?,
        "search.mode" => config.search.mode = value.to_string(),
        "search.default_limit" => config.search.default_limit = value.parse()?,
        "search.min_score" => config.search.min_score = value.parse()?,
        "insert.auto_link_threshold" => config.insert.auto_link_threshold = value.parse()?,
        "insert.duplicate_threshold" => config.insert.duplicate_threshold = value.parse()?,
        "insert.auto_link_limit" => config.insert.auto_link_limit = value.parse()?,
        "database.backend" => config.database.backend = value.to_string(),
        "database.sqlite_path" => config.database.sqlite_path = value.to_string(),
        "database.path" => config.database.path = Some(value.to_string()), // legacy
        other => anyhow::bail!("Unknown config key: '{}'. Valid keys: embeddings.model, embeddings.enabled, search.mode, search.default_limit, search.min_score, insert.auto_link_threshold, insert.duplicate_threshold, insert.auto_link_limit, database.backend, database.sqlite_path, database.path", other),
    }
    Ok(())
}
