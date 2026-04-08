use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use cli_config::CliConfigOverrides;
use std::sync::Arc;
use voidm_cli::{cli_config, commands};
use voidm_core::{db::DbPool, Config};

#[derive(Parser)]
#[command(
    name = "voidm",
    about = "Local-first memory tool for LLM agents",
    version
)]
pub struct Cli {
    /// Override database path [env: VOIDM_DB]
    #[arg(long, global = true, env = "VOIDM_DB")]
    pub db: Option<String>,

    /// Output JSON (machine-readable)
    #[arg(long, global = true)]
    pub json: bool,

    /// Agent mode: compact token-minimal JSON output optimised for LLM consumption [env: VOIDM_AGENT_MODE]
    #[arg(long, global = true, env = "VOIDM_AGENT_MODE")]
    pub agent: bool,

    /// Suppress decorative output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    // ─── Global Config Overrides ───
    /// Database backend: sqlite, neo4j [env: VOIDM_DATABASE_BACKEND]
    #[arg(long, global = true, env = "VOIDM_DATABASE_BACKEND")]
    pub database_backend: Option<String>,

    /// SQLite database path [env: VOIDM_DATABASE_SQLITE_PATH]
    #[arg(long, global = true, env = "VOIDM_DATABASE_SQLITE_PATH")]
    pub database_sqlite_path: Option<String>,

    /// Embeddings enabled [env: VOIDM_EMBEDDINGS_ENABLED]
    #[arg(long, global = true, env = "VOIDM_EMBEDDINGS_ENABLED")]
    pub embeddings_enabled: Option<bool>,

    /// Embeddings model name [env: VOIDM_EMBEDDINGS_MODEL]
    #[arg(long, global = true, env = "VOIDM_EMBEDDINGS_MODEL")]
    pub embeddings_model: Option<String>,

    /// Search mode: hybrid-rrf, hybrid, semantic, keyword, fuzzy, bm25 [env: VOIDM_SEARCH_MODE]
    #[arg(long, global = true, env = "VOIDM_SEARCH_MODE")]
    pub search_mode: Option<String>,

    /// Search default limit [env: VOIDM_SEARCH_DEFAULT_LIMIT]
    #[arg(long, global = true, env = "VOIDM_SEARCH_DEFAULT_LIMIT")]
    pub search_default_limit: Option<usize>,

    /// Search minimum score (0.0-1.0) [env: VOIDM_SEARCH_MIN_SCORE]
    #[arg(long, global = true, env = "VOIDM_SEARCH_MIN_SCORE")]
    pub search_min_score: Option<f32>,

    /// Reranker enabled [env: VOIDM_SEARCH_RERANKER_ENABLED]
    #[arg(long, global = true, env = "VOIDM_SEARCH_RERANKER_ENABLED")]
    pub reranker_enabled: Option<bool>,

    /// Reranker model name [env: VOIDM_SEARCH_RERANKER_MODEL]
    #[arg(long, global = true, env = "VOIDM_SEARCH_RERANKER_MODEL")]
    pub reranker_model: Option<String>,

    /// Reranker apply to top K results [env: VOIDM_SEARCH_RERANKER_TOP_K]
    #[arg(long, global = true, env = "VOIDM_SEARCH_RERANKER_TOP_K")]
    pub reranker_top_k: Option<usize>,

    /// Query expansion enabled [env: VOIDM_SEARCH_QE_ENABLED]
    #[arg(long, global = true, env = "VOIDM_SEARCH_QE_ENABLED")]
    pub qe_enabled: Option<bool>,

    /// Query expansion timeout milliseconds [env: VOIDM_SEARCH_QE_TIMEOUT_MS]
    #[arg(long, global = true, env = "VOIDM_SEARCH_QE_TIMEOUT_MS")]
    pub qe_timeout_ms: Option<usize>,

    /// Graph retrieval enabled [env: VOIDM_SEARCH_GR_ENABLED]
    #[arg(long, global = true, env = "VOIDM_SEARCH_GR_ENABLED")]
    pub gr_enabled: Option<bool>,

    /// Graph retrieval max concept hops [env: VOIDM_SEARCH_GR_MAX_HOPS]
    #[arg(long, global = true, env = "VOIDM_SEARCH_GR_MAX_HOPS")]
    pub gr_max_hops: Option<usize>,

    /// Insert auto-link threshold [env: VOIDM_INSERT_AUTO_LINK_THRESHOLD]
    #[arg(long, global = true, env = "VOIDM_INSERT_AUTO_LINK_THRESHOLD")]
    pub insert_auto_link_threshold: Option<f32>,

    /// Insert duplicate threshold [env: VOIDM_INSERT_DUPLICATE_THRESHOLD]
    #[arg(long, global = true, env = "VOIDM_INSERT_DUPLICATE_THRESHOLD")]
    pub insert_duplicate_threshold: Option<f32>,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    /// Extract CLI config overrides
    pub fn cli_config_overrides(&self) -> CliConfigOverrides {
        CliConfigOverrides {
            database_backend: self.database_backend.clone(),
            database_sqlite_path: self.database_sqlite_path.clone(),
            embeddings_enabled: self.embeddings_enabled,
            embeddings_model: self.embeddings_model.clone(),
            search_mode: self.search_mode.clone(),
            search_default_limit: self.search_default_limit,
            search_min_score: self.search_min_score,
            reranker_enabled: self.reranker_enabled,
            reranker_model: self.reranker_model.clone(),
            reranker_top_k: self.reranker_top_k,
            qe_enabled: self.qe_enabled,
            qe_timeout_ms: self.qe_timeout_ms,
            gr_enabled: self.gr_enabled,
            gr_max_hops: self.gr_max_hops,
            insert_auto_link_threshold: self.insert_auto_link_threshold,
            insert_duplicate_threshold: self.insert_duplicate_threshold,
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a memory
    Add(commands::add::AddArgs),
    /// Get a memory by ID
    Get(commands::get::GetArgs),
    /// Hybrid search
    Search(commands::search::SearchArgs),
    /// Trajectory-informed learning tips
    #[command(subcommand)]
    Learn(commands::learn::LearnCommands),
    /// List memories (newest first)
    List(commands::list::ListArgs),
    /// Delete a memory (cascades graph edges)
    Delete(commands::delete::DeleteArgs),
    /// Create a graph edge between two memories
    Link(commands::link::LinkArgs),
    /// Remove a graph edge
    Unlink(commands::unlink::UnlinkArgs),
    /// Initialize voidm: download and cache all models
    Init(commands::init::InitArgs),
    /// Graph operations
    #[command(subcommand)]
    Graph(commands::graph::GraphCommands),
    /// Ontology operations (concepts, hierarchy, instances)
    #[command(subcommand)]
    Ontology(commands::ontology::OntologyCommands),
    /// Review and resolve ontology conflicts (CONTRADICTS edges)
    #[command(subcommand)]
    Conflicts(commands::conflicts::ConflictsCommands),
    /// List all known scope strings
    #[command(subcommand)]
    Scopes(commands::scopes::ScopesCommands),
    /// Export memories
    Export(commands::export::ExportArgs),
    /// Show or edit config
    #[command(subcommand)]
    Config(commands::config::ConfigCommands),
    /// Model management
    #[command(subcommand)]
    Models(commands::models::ModelsCommands),
    /// Print usage guide for LLM agents
    Instructions(commands::instructions::InstructionsArgs),
    /// Show paths, config and runtime settings
    Info(commands::info::InfoArgs),
    /// Show memory and graph statistics
    Stats(commands::stats::StatsArgs),
    /// Migrate data between backends (sqlite ↔ neo4j)
    Migrate(commands::migrate::MigrateArgs),
    /// Check for new releases on GitHub
    CheckUpdate(commands::update::CheckUpdateArgs),
    /// Update a memory in-place (preserves ID and graph edges)
    Update(commands::mem_update::UpdateMemoryArgs),
    /// Recall startup context in one call (architecture, constraints, decisions, procedures, preferences)
    Recall(commands::recall::RecallArgs),
    /// List memories older than N days for staleness review
    Stale(commands::stale::StaleArgs),
    /// Add multiple memories from a JSON file in one call
    BatchAdd(commands::batch_add::BatchAddArgs),
    /// Show provenance summary for a memory (graph edges, tags, age)
    Why(commands::why::WhyArgs),
}

#[tokio::main]
async fn main() {
    // Intercept clap parse errors to inject helpful hints for known args.
    // We parse manually so we can customise the error before clap exits.
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            // Augment missing --type with the list of valid types
            let msg = if msg.contains("--type") && msg.contains("required arguments") {
                format!(
                    "{msg}\nValid memory types: episodic, semantic, procedural, conceptual, contextual\n\
                     Example: voidm add \"content\" --type semantic"
                )
            } else {
                msg
            };
            // Print to stderr and exit with clap's own code (1 for usage, 2 for error)
            eprintln!("{msg}");
            std::process::exit(e.exit_code());
        }
    };

    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let json = cli.json;
    let result = run(cli).await;

    match result {
        Ok(()) => {}
        Err(e) => {
            emit_error(&e.to_string(), json);
            std::process::exit(2);
        }
    }
}

/// Emit an error. In JSON mode: `{"error": "..."}` on stdout. Otherwise: `Error: ...` on stderr.
pub fn emit_error(msg: &str, json: bool) {
    let msg = augment_error_message(msg);
    if json {
        println!("{}", serde_json::json!({ "error": msg }));
    } else {
        eprintln!("Error: {msg}");
    }
}

fn augment_error_message(msg: &str) -> String {
    let lowercase = msg.to_ascii_lowercase();
    if voidm_core::is_codex_sandbox_active() && lowercase.contains("readonly database") {
        return format!(
            "{msg}\nHint: sandboxed runs need a writable DB path. Use VOIDM_DB={} or set database.sqlite_path to the same location.",
            voidm_core::codex_sandbox_db_path().display()
        );
    }

    msg.to_string()
}

async fn run(cli: Cli) -> Result<()> {
    let cli_sqlite_path_override = cli.database_sqlite_path.clone();
    let agent = cli.agent;

    // Commands that don't need DB
    match &cli.command {
        Commands::Scopes(commands::scopes::ScopesCommands::Detect(ref args)) => {
            return commands::scopes::run_detect(args.clone(), cli.json);
        }
        Commands::Instructions(args) => {
            return commands::instructions::run(args, cli.json);
        }
        Commands::Config(cmd) => {
            return commands::config::run(cmd, cli.json).await;
        }
        Commands::Info(args) => {
            let mut config = Config::load();
            // Apply CLI overrides (file → env → CLI hierarchy)
            config = config.merge_from_env();
            config = cli.cli_config_overrides().apply_to_config(config);
            return commands::info::run(
                args.clone(),
                &config,
                cli.db.as_deref(),
                cli.database_sqlite_path.as_deref(),
                cli.json,
            );
        }
        Commands::Init(args) => {
            return commands::init::run(args.clone()).await;
        }
        Commands::Migrate(args) => {
            let mut config = Config::load();
            // Apply CLI overrides (file → env → CLI hierarchy)
            config = config.merge_from_env();
            config = cli.cli_config_overrides().apply_to_config(config);
            return commands::migrate::run(
                args.clone(),
                &config,
                cli.db.as_deref(),
                cli.database_sqlite_path.as_deref(),
                cli.json,
            )
            .await;
        }
        Commands::CheckUpdate(args) => {
            return commands::update::check_update(args.clone()).await;
        }
        Commands::Models(cmd) => {
            if let commands::models::ModelsCommands::List = cmd {
                return commands::models::run_list(cli.json);
            }
        }
        _ => {}
    }

    // Load config + open DB
    let mut config = Config::load();
    // Apply environment variables and CLI overrides (file → env → CLI hierarchy)
    use voidm_core::config_loader::MergeFromEnv;
    config = config.merge_from_env();
    config = cli.cli_config_overrides().apply_to_config(config);

    // Resolve DB path (handles --db, VOIDM_DB, --database-sqlite-path overrides)
    let resolved = config.resolve_db_path(cli.db.as_deref(), cli_sqlite_path_override.as_deref());
    config.database.sqlite_path = resolved.path.to_string_lossy().into_owned();

    // Open backend-agnostic database
    let db: Arc<dyn voidm_core::db::Database> = DbPool::open(&config.database).await?;

    // Run migrations (SQLite only)
    if let Some(pool) = db.sqlite_pool() {
        voidm_core::migrate::run(pool).await?;
        let _ = voidm_core::vector::cleanup_stale_temp_table(pool).await;
    }

    // Check model mismatch
    if config.embeddings.enabled {
        if let Ok(Some((db_model, db_dim))) =
            db.check_model_mismatch(&config.embeddings.model).await
        {
            eprintln!(
                "Warning: configured model '{}' differs from DB model '{}' (dim {}). \
                 Vector search disabled. Run 'voidm models reembed' to re-embed all memories.",
                config.embeddings.model, db_model, db_dim
            );
        }
    }

    match cli.command {
        Commands::Add(args) => commands::add::run(args, &db, &config, cli.json).await,
        Commands::Get(args) => commands::get::run(args, &db, cli.json).await,
        Commands::Search(args) => commands::search::run(args, &db, &config, cli.json).await,
        Commands::Learn(cmd) => commands::learn::run(cmd, &db, &config, cli.json).await,
        Commands::List(args) => commands::list::run(args, &db, &config, cli.json).await,
        Commands::Delete(args) => commands::delete::run(args, &db, cli.json).await,
        Commands::Link(args) => commands::link::run(args, &db, cli.json).await,
        Commands::Unlink(args) => commands::unlink::run(args, &db, cli.json).await,
        Commands::Graph(cmd) => commands::graph::run(cmd, &db, cli.json).await,
        Commands::Ontology(cmd) => commands::ontology::run(cmd, &db, &config, cli.json).await,
        Commands::Conflicts(cmd) => commands::conflicts::run(cmd, &db, cli.json).await,
        Commands::Scopes(cmd) => commands::scopes::run(cmd, &db, cli.json).await,
        Commands::Export(args) => commands::export::run(args, &db, &config, cli.json).await,
        Commands::Config(_) => unreachable!(),
        Commands::Models(cmd) => commands::models::run(cmd, &db, &config, cli.json).await,
        Commands::Instructions(_) => unreachable!(),
        Commands::Info(_) => unreachable!(),
        Commands::Init(_) => unreachable!(),
        Commands::Migrate(_) => unreachable!(),
        Commands::CheckUpdate(_) => unreachable!(),
        Commands::Stats(args) => commands::stats::run(args, &db, &config, cli.json).await,
        Commands::Update(args) => {
            commands::mem_update::run(args, &db, &config, cli.json, agent).await
        }
        Commands::Recall(args) => commands::recall::run(args, &db, &config, cli.json, agent).await,
        Commands::Stale(args) => commands::stale::run(args, &db, cli.json).await,
        Commands::BatchAdd(args) => commands::batch_add::run(args, &db, &config, cli.json).await,
        Commands::Why(args) => commands::why::run(args, &db, cli.json).await,
    }
}
