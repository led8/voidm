use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub insert: InsertConfig,
    #[serde(default)]
    pub enrichment: EnrichmentConfig,
    #[serde(default)]
    pub redaction: crate::redactor::RedactionConfig,
    #[serde(default)]
    pub chunking: ChunkingConfig,
}

/// Text chunking configuration for chunk-level embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    /// Whether chunk-level embeddings are enabled (default: true).
    #[serde(default = "default_chunking_enabled")]
    pub enabled: bool,
    /// Target chunk size in characters (default: 600).
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    /// Minimum chunk size in characters — smaller chunks are merged or dropped (default: 150).
    #[serde(default = "default_chunk_min")]
    pub chunk_min: usize,
    /// Maximum chunk size in characters — oversized chunks are split further (default: 900).
    #[serde(default = "default_chunk_max")]
    pub chunk_max: usize,
    /// Overlap between adjacent chunks in characters (default: 100).
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,
}

fn default_chunking_enabled() -> bool {
    true
}
fn default_chunk_size() -> usize {
    600
}
fn default_chunk_min() -> usize {
    150
}
fn default_chunk_max() -> usize {
    900
}
fn default_chunk_overlap() -> usize {
    100
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            enabled: default_chunking_enabled(),
            chunk_size: default_chunk_size(),
            chunk_min: default_chunk_min(),
            chunk_max: default_chunk_max(),
            chunk_overlap: default_chunk_overlap(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database backend: "sqlite" or "neo4j" (default: "sqlite")
    #[serde(default = "default_backend")]
    pub backend: String,

    /// SQLite configuration (used when backend = "sqlite")
    #[serde(default)]
    pub sqlite: Option<SqliteConfig>,

    /// Path to SQLite database file - DEPRECATED, use [database.sqlite].path instead
    /// This is kept for backward compatibility
    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,

    /// Neo4j connection parameters (used when backend = "neo4j")
    #[serde(default)]
    pub neo4j: Option<Neo4jConfig>,

    /// Legacy field for backward compatibility
    pub path: Option<String>,
}

/// SQLite configuration section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// Path to SQLite database file (optional, defaults to ~/.local/share/voidm/memories.db)
    /// Supports ~ for home directory
    #[serde(default)]
    pub path: Option<String>,
}

fn default_backend() -> String {
    "sqlite".to_string()
}

fn default_sqlite_path() -> String {
    platform_default_db_path().to_string_lossy().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbPathSource {
    CliDbFlag,
    EnvDb,
    CliSqlitePath,
    EnvSqlitePath,
    ConfigFile,
    ConfigObject,
    CodexSandboxDefault,
    PlatformDefault,
}

impl DbPathSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CliDbFlag => "--db flag",
            Self::EnvDb => "$VOIDM_DB",
            Self::CliSqlitePath => "--database-sqlite-path",
            Self::EnvSqlitePath => "$VOIDM_DATABASE_SQLITE_PATH",
            Self::ConfigFile => "config file",
            Self::ConfigObject => "resolved config override",
            Self::CodexSandboxDefault => "default (Codex sandbox writable path)",
            Self::PlatformDefault => "default (platform data dir)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbPathResolution {
    pub path: PathBuf,
    pub source: DbPathSource,
}

pub fn platform_default_db_path() -> PathBuf {
    if let Some(mut path) = dirs::data_local_dir() {
        path.push("voidm");
        path.push("memories.db");
        return path;
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(".local/share/voidm/memories.db");
    }

    PathBuf::from(".voidm/memories.db")
}

pub fn is_codex_sandbox_active() -> bool {
    std::env::var("CODEX_SANDBOX")
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

pub fn codex_sandbox_db_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        return home.join(".codex/memories/voidm/memories.db");
    }

    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        if !tmpdir.is_empty() {
            return PathBuf::from(tmpdir).join("voidm/memories.db");
        }
    }

    PathBuf::from("/tmp/voidm/memories.db")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neo4jConfig {
    /// Neo4j Bolt URI (default: bolt://localhost:7687)
    #[serde(default = "default_neo4j_uri")]
    pub uri: String,

    /// Neo4j username (default: "neo4j")
    #[serde(default = "default_neo4j_user")]
    pub username: String,

    /// Neo4j password (default: "password")
    #[serde(default = "default_neo4j_password")]
    pub password: String,
}

fn default_neo4j_uri() -> String {
    "bolt://localhost:7687".to_string()
}

fn default_neo4j_user() -> String {
    "neo4j".to_string()
}

fn default_neo4j_password() -> String {
    "password".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    pub enabled: bool,
    pub model: String,
}

/// Per-signal on/off toggles for the unified RRF pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalsConfig {
    /// Enable vector ANN signal (default: true).
    #[serde(default = "default_true")]
    pub vector: bool,
    /// Enable BM25 FTS signal (default: true).
    #[serde(default = "default_true")]
    pub bm25: bool,
    /// Enable fuzzy (Jaro-Winkler) signal (default: true).
    #[serde(default = "default_true")]
    pub fuzzy: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SignalsConfig {
    fn default() -> Self {
        Self {
            vector: true,
            bm25: true,
            fuzzy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    pub mode: String,
    pub default_limit: usize,
    /// Per-signal enable/disable for the unified RRF pipeline.
    #[serde(default)]
    pub signals: SignalsConfig,
    /// Minimum score threshold for hybrid mode results (0.0–1.0). Default: 0.0.
    pub min_score: f32,
    /// Score decay per hop for graph-expanded neighbors. neighbor_score = parent_score * decay^depth.
    pub neighbor_decay: f32,
    /// Minimum score for graph-expanded neighbors to be included. Default: 0.2.
    pub neighbor_min_score: f32,
    /// Default traversal depth for --include-neighbors. Hard cap: 3.
    pub default_neighbor_depth: u8,
    /// Edge types to traverse by default for neighbor expansion.
    pub default_edge_types: Vec<String>,
    /// Reranker configuration (optional).
    #[serde(default)]
    pub reranker: Option<RerankerConfig>,
    /// Query expansion configuration (optional).
    #[serde(default)]
    pub query_expansion: Option<QueryExpansionConfig>,
    /// Graph-aware retrieval configuration (optional).
    #[serde(default)]
    pub graph_retrieval: Option<crate::graph_retrieval::GraphRetrievalConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassageExtractionConfig {
    /// Enable passage extraction for reranking (default: true)
    #[serde(default = "default_passage_extraction_enabled")]
    pub enabled: bool,
    /// Number of context sentences before/after match (default: 1)
    #[serde(default = "default_context_sentences")]
    pub context_sentences: usize,
    /// Fallback length if no match found (default: 400)
    #[serde(default = "default_fallback_length")]
    pub fallback_length: usize,
    /// Minimum passage length to return (default: 50)
    #[serde(default = "default_min_passage_length")]
    pub min_passage_length: usize,
}

impl Default for PassageExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            context_sentences: 1,
            fallback_length: 400,
            min_passage_length: 50,
        }
    }
}

fn default_passage_extraction_enabled() -> bool {
    true
}

fn default_context_sentences() -> usize {
    1
}

fn default_fallback_length() -> usize {
    400
}

fn default_min_passage_length() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerConfig {
    /// Enable reranking (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Model name: "ms-marco-TinyBERT-L-2" (default)
    #[serde(default = "default_reranker_model")]
    pub model: String,
    /// Apply reranker to top-k results only (default: 15).
    #[serde(default = "default_reranker_top_k")]
    pub apply_to_top_k: usize,
    /// Passage extraction configuration for better reranking on long documents
    #[serde(default)]
    pub passage_extraction: PassageExtractionConfig,
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_reranker_model(),
            apply_to_top_k: default_reranker_top_k(),
            passage_extraction: PassageExtractionConfig::default(),
        }
    }
}

fn default_reranker_model() -> String {
    "ms-marco-MiniLM-L-6-v2".into()
}

fn default_reranker_top_k() -> usize {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExpansionConfig {
    /// Enable query expansion (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Model name: "tinyllama" (ONNX, default) or "tobil/qmd-query-expansion-1.7B" (GGUF, opt-in).
    /// App auto-detects backend based on model name (models with "tobil" or "qmd" use GGUF).
    #[serde(default = "default_query_expansion_model")]
    pub model: String,
    /// Maximum time to wait for expansion in milliseconds (default: 300).
    #[serde(default = "default_query_expansion_timeout_ms")]
    pub timeout_ms: u64,
    /// Intent-aware expansion configuration.
    #[serde(default)]
    pub intent: IntentConfig,
}

impl Default for QueryExpansionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_query_expansion_model(),
            timeout_ms: default_query_expansion_timeout_ms(),
            intent: IntentConfig::default(),
        }
    }
}

fn default_query_expansion_model() -> String {
    "tinyllama".into()
}

fn default_query_expansion_timeout_ms() -> u64 {
    300
}

/// Intent-aware query expansion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    /// Enable intent-aware expansion (default: true).
    #[serde(default = "default_intent_enabled")]
    pub enabled: bool,
    /// Use scope as fallback intent if intent not explicitly provided (default: true).
    #[serde(default = "default_intent_use_scope_as_fallback")]
    pub use_scope_as_fallback: bool,
    /// Optional default intent for all queries (default: null).
    #[serde(default)]
    pub default_intent: Option<String>,
}

fn default_intent_enabled() -> bool {
    true
}

fn default_intent_use_scope_as_fallback() -> bool {
    true
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_scope_as_fallback: true,
            default_intent: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertConfig {
    pub auto_link_threshold: f32,
    pub duplicate_threshold: f32,
    pub auto_link_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentConfig {
    #[serde(default)]
    pub semantic_dedup: Option<crate::semantic_dedup::SemanticDedupConfig>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: "sqlite".to_string(),
            sqlite: None,
            sqlite_path: default_sqlite_path(),
            neo4j: None,
            path: None,
        }
    }
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model: "Xenova/all-MiniLM-L6-v2".into(),
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: "hybrid".into(),
            default_limit: 10,
            min_score: 0.0,
            neighbor_decay: 0.7,
            neighbor_min_score: 0.2,
            default_neighbor_depth: 1,
            default_edge_types: vec![
                "PART_OF".into(),
                "SUPPORTS".into(),
                "DERIVED_FROM".into(),
                "EXEMPLIFIES".into(),
            ],
            reranker: None,
            query_expansion: None,
            graph_retrieval: None,
            signals: SignalsConfig::default(),
        }
    }
}

impl Default for InsertConfig {
    fn default() -> Self {
        Self {
            auto_link_threshold: 0.7,
            duplicate_threshold: 0.95,
            auto_link_limit: 5,
        }
    }
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            semantic_dedup: Some(crate::semantic_dedup::SemanticDedupConfig::default()),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: Default::default(),
            embeddings: Default::default(),
            search: Default::default(),
            insert: Default::default(),
            enrichment: Default::default(),
            redaction: Default::default(),
            chunking: Default::default(),
        }
    }
}

impl Config {
    /// Load config from disk, merging with defaults. Never fails — missing file = all defaults.
    pub fn load() -> Self {
        let path = config_path();
        if let Some(p) = &path {
            if p.exists() {
                match std::fs::read_to_string(p) {
                    Ok(s) => match toml::from_str::<Config>(&s) {
                        Ok(c) => return c,
                        Err(e) => tracing::warn!("Failed to parse config {}: {}", p.display(), e),
                    },
                    Err(e) => tracing::warn!("Failed to read config {}: {}", p.display(), e),
                }
            }
        }
        Config::default()
    }

    /// Resolve the DB path with source tracking.
    /// Precedence: --db > $VOIDM_DB > explicit sqlite-path override > $VOIDM_DATABASE_SQLITE_PATH
    /// > explicit config file path > explicit in-memory config path > sandbox default > platform default
    pub fn resolve_db_path(
        &self,
        override_path: Option<&str>,
        sqlite_path_override: Option<&str>,
    ) -> DbPathResolution {
        if let Some(path) = non_empty_arg(override_path) {
            return DbPathResolution {
                path: PathBuf::from(path),
                source: DbPathSource::CliDbFlag,
            };
        }

        if let Some(path) = non_empty_env("VOIDM_DB") {
            return DbPathResolution {
                path: PathBuf::from(path),
                source: DbPathSource::EnvDb,
            };
        }

        if let Some(path) = non_empty_arg(sqlite_path_override) {
            return DbPathResolution {
                path: PathBuf::from(shellexpand(path)),
                source: DbPathSource::CliSqlitePath,
            };
        }

        if let Some(path) = non_empty_env("VOIDM_DATABASE_SQLITE_PATH") {
            return DbPathResolution {
                path: PathBuf::from(shellexpand(&path)),
                source: DbPathSource::EnvSqlitePath,
            };
        }

        if let Some(path) = explicit_config_db_path_from_file() {
            return DbPathResolution {
                path,
                source: DbPathSource::ConfigFile,
            };
        }

        if let Some(path) = explicit_config_db_path_from_object(self) {
            return DbPathResolution {
                path,
                source: DbPathSource::ConfigObject,
            };
        }

        if is_codex_sandbox_active() {
            return DbPathResolution {
                path: codex_sandbox_db_path(),
                source: DbPathSource::CodexSandboxDefault,
            };
        }

        DbPathResolution {
            path: platform_default_db_path(),
            source: DbPathSource::PlatformDefault,
        }
    }

    /// Resolve the DB path: --db flag > $VOIDM_DB > config > defaults
    pub fn db_path(&self, override_path: Option<&str>) -> PathBuf {
        self.resolve_db_path(override_path, None).path
    }
}

pub fn config_path_display() -> String {
    config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".into())
}

fn config_path() -> Option<PathBuf> {
    // XDG_CONFIG_HOME/voidm/config.toml
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("voidm/config.toml"));
        }
    }
    // ~/.config/voidm/config.toml
    dirs::home_dir().map(|h| h.join(".config/voidm/config.toml"))
}

fn shellexpand(s: &str) -> String {
    if s.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &s[1..]);
        }
    }
    s.to_string()
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path().context("Cannot determine config path")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(config)?;
    std::fs::write(&path, s)?;
    Ok(())
}

fn non_empty_arg(value: Option<&str>) -> Option<&str> {
    value.and_then(|item| if item.is_empty() { None } else { Some(item) })
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn explicit_config_db_path_from_file() -> Option<PathBuf> {
    let path = config_path()?;
    let contents = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = toml::from_str(&contents).ok()?;
    let database = value.get("database")?;

    if let Some(path) = database
        .get("sqlite")
        .and_then(|sqlite| sqlite.get("path"))
        .and_then(non_empty_toml_str)
    {
        return Some(PathBuf::from(shellexpand(path)));
    }

    if let Some(path) = database.get("sqlite_path").and_then(non_empty_toml_str) {
        return Some(PathBuf::from(shellexpand(path)));
    }

    if let Some(path) = database.get("path").and_then(non_empty_toml_str) {
        return Some(PathBuf::from(shellexpand(path)));
    }

    None
}

fn explicit_config_db_path_from_object(config: &Config) -> Option<PathBuf> {
    let platform_default = platform_default_db_path();

    if let Some(path) = config
        .database
        .sqlite
        .as_ref()
        .and_then(|sqlite| sqlite.path.as_deref())
        .filter(|path| !path.is_empty())
    {
        let expanded = PathBuf::from(shellexpand(path));
        if expanded != platform_default {
            return Some(expanded);
        }
    }

    if !config.database.sqlite_path.is_empty() {
        let expanded = PathBuf::from(shellexpand(&config.database.sqlite_path));
        if expanded != platform_default {
            return Some(expanded);
        }
    }

    if let Some(path) = config
        .database
        .path
        .as_deref()
        .filter(|path| !path.is_empty())
    {
        return Some(PathBuf::from(shellexpand(path)));
    }

    None
}

fn non_empty_toml_str(value: &toml::Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

// ─── Configuration Merging from Environment Variables ───

impl crate::config_loader::MergeFromEnv for Config {
    fn merge_from_env(mut self) -> Self {
        use crate::config_loader::EnvHelper;

        // Database config
        if let Some(backend) = EnvHelper::get("DATABASE_BACKEND") {
            self.database.backend = backend;
        }
        if let Some(path) = EnvHelper::get("DATABASE_SQLITE_PATH") {
            self.database.sqlite_path = path.clone();
            if let Some(sqlite) = &mut self.database.sqlite {
                sqlite.path = Some(path);
            } else {
                self.database.sqlite = Some(SqliteConfig { path: Some(path) });
            }
        }
        if let Some(uri) = EnvHelper::get("DATABASE_NEO4J_URI") {
            if let Some(neo4j) = &mut self.database.neo4j {
                neo4j.uri = uri;
            }
        }
        if let Some(user) = EnvHelper::get("DATABASE_NEO4J_USERNAME") {
            if let Some(neo4j) = &mut self.database.neo4j {
                neo4j.username = user;
            }
        }
        if let Some(pass) = EnvHelper::get("DATABASE_NEO4J_PASSWORD") {
            if let Some(neo4j) = &mut self.database.neo4j {
                neo4j.password = pass;
            }
        }

        // Embeddings config
        if let Some(enabled) = EnvHelper::get_bool("EMBEDDINGS_ENABLED") {
            self.embeddings.enabled = enabled;
        }
        if let Some(model) = EnvHelper::get("EMBEDDINGS_MODEL") {
            self.embeddings.model = model;
        }

        // Search config
        if let Some(mode) = EnvHelper::get("SEARCH_MODE") {
            self.search.mode = mode;
        }
        if let Some(limit) = EnvHelper::get_usize("SEARCH_DEFAULT_LIMIT") {
            self.search.default_limit = limit;
        }
        if let Some(min_score) = EnvHelper::get_f32("SEARCH_MIN_SCORE") {
            self.search.min_score = min_score;
        }
        if let Some(decay) = EnvHelper::get_f32("SEARCH_NEIGHBOR_DECAY") {
            self.search.neighbor_decay = decay;
        }
        if let Some(min) = EnvHelper::get_f32("SEARCH_NEIGHBOR_MIN_SCORE") {
            self.search.neighbor_min_score = min;
        }
        if let Some(depth) = EnvHelper::get_usize("SEARCH_NEIGHBOR_DEPTH") {
            self.search.default_neighbor_depth = depth as u8;
        }
        if let Some(edges) = EnvHelper::get_vec_string("SEARCH_DEFAULT_EDGE_TYPES") {
            self.search.default_edge_types = edges;
        }

        // Reranker config
        if let Some(mut reranker) = self.search.reranker.take() {
            if let Some(enabled) = EnvHelper::get_bool("SEARCH_RERANKER_ENABLED") {
                reranker.enabled = enabled;
            }
            if let Some(model) = EnvHelper::get("SEARCH_RERANKER_MODEL") {
                reranker.model = model;
            }
            if let Some(top_k) = EnvHelper::get_usize("SEARCH_RERANKER_TOP_K") {
                reranker.apply_to_top_k = top_k;
            }
            self.search.reranker = Some(reranker);
        } else if EnvHelper::get("SEARCH_RERANKER_ENABLED").is_some()
            || EnvHelper::get("SEARCH_RERANKER_MODEL").is_some()
        {
            // Create reranker config if env vars exist
            let mut reranker = RerankerConfig::default();
            if let Some(enabled) = EnvHelper::get_bool("SEARCH_RERANKER_ENABLED") {
                reranker.enabled = enabled;
            }
            if let Some(model) = EnvHelper::get("SEARCH_RERANKER_MODEL") {
                reranker.model = model;
            }
            if let Some(top_k) = EnvHelper::get_usize("SEARCH_RERANKER_TOP_K") {
                reranker.apply_to_top_k = top_k;
            }
            self.search.reranker = Some(reranker);
        }

        // Query expansion config
        if let Some(mut qe) = self.search.query_expansion.take() {
            if let Some(enabled) = EnvHelper::get_bool("SEARCH_QE_ENABLED") {
                qe.enabled = enabled;
            }
            if let Some(model) = EnvHelper::get("SEARCH_QE_MODEL") {
                qe.model = model;
            }
            if let Some(timeout) = EnvHelper::get_usize("SEARCH_QE_TIMEOUT_MS") {
                qe.timeout_ms = timeout as u64;
            }
            self.search.query_expansion = Some(qe);
        }

        // Graph retrieval config
        if let Some(mut gr) = self.search.graph_retrieval.take() {
            if let Some(enabled) = EnvHelper::get_bool("SEARCH_GR_ENABLED") {
                gr.enabled = enabled;
            }
            if let Some(hops) = EnvHelper::get_usize("SEARCH_GR_MAX_HOPS") {
                gr.max_concept_hops = hops as u8;
            }
            self.search.graph_retrieval = Some(gr);
        }

        // Insert config
        if let Some(threshold) = EnvHelper::get_f32("INSERT_AUTO_LINK_THRESHOLD") {
            self.insert.auto_link_threshold = threshold;
        }
        if let Some(threshold) = EnvHelper::get_f32("INSERT_DUPLICATE_THRESHOLD") {
            self.insert.duplicate_threshold = threshold;
        }
        if let Some(limit) = EnvHelper::get_usize("INSERT_AUTO_LINK_LIMIT") {
            self.insert.auto_link_limit = limit;
        }

        // Enrichment config
        if let Some(mut sem_dedup) = self.enrichment.semantic_dedup.take() {
            if let Some(enabled) = EnvHelper::get_bool("ENRICHMENT_SEMANTIC_DEDUP_ENABLED") {
                sem_dedup.enabled = enabled;
            }
            if let Some(threshold) = EnvHelper::get_f32("ENRICHMENT_SEMANTIC_DEDUP_THRESHOLD") {
                sem_dedup.threshold = threshold;
            }
            self.enrichment.semantic_dedup = Some(sem_dedup);
        }

        // Redaction config
        if let Some(enabled) = EnvHelper::get_bool("REDACTION_ENABLED") {
            self.redaction.enabled = enabled;
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        saved: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn set(pairs: &[(&str, Option<&str>)]) -> Self {
            let lock = env_lock().lock().expect("env lock");
            let mut saved = Vec::with_capacity(pairs.len());

            for (key, value) in pairs {
                saved.push(((*key).to_string(), std::env::var(key).ok()));
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }

            Self { _lock: lock, saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("voidm-{label}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn test_parse_neo4j_config() {
        let toml_str = r#"
[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "neo4jneo4j"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        assert!(
            config.database.neo4j.is_some(),
            "neo4j config should be parsed"
        );
        if let Some(nc) = &config.database.neo4j {
            assert_eq!(nc.uri, "bolt://localhost:7687");
            assert_eq!(nc.username, "neo4j");
            assert_eq!(nc.password, "neo4jneo4j");
        }
    }

    #[test]
    fn test_config_with_both_backends() {
        let toml_str = r#"
[database]
backend = "sqlite"

[database.sqlite]
path = "~/.codex/memories/voidm/memories.db"

[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "neo4jneo4j"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to parse");

        // Verify backend selection
        assert_eq!(config.database.backend, "sqlite");

        // Verify SQLite config is present
        assert!(config.database.sqlite.is_some());
        if let Some(sqlite) = &config.database.sqlite {
            assert_eq!(
                sqlite.path,
                Some("~/.codex/memories/voidm/memories.db".to_string())
            );
        }

        // Verify Neo4j config is present
        assert!(config.database.neo4j.is_some());
        if let Some(neo4j) = &config.database.neo4j {
            assert_eq!(neo4j.uri, "bolt://localhost:7687");
            assert_eq!(neo4j.username, "neo4j");
        }
    }

    #[test]
    fn test_switch_to_neo4j_backend() {
        let toml_str = r#"
[database]
backend = "neo4j"

[database.sqlite]
path = "~/.codex/memories/voidm/memories.db"

[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "neo4jneo4j"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to parse");

        // Verify backend is switched to neo4j
        assert_eq!(config.database.backend, "neo4j");

        // Both are still configured
        assert!(config.database.sqlite.is_some());
        assert!(config.database.neo4j.is_some());
    }

    #[test]
    fn test_reranker_config_defaults() {
        let toml_str = r#"
[search]
mode = "hybrid"
default_limit = 10
min_score = 0.3
neighbor_decay = 0.7
neighbor_min_score = 0.2
default_neighbor_depth = 1
default_edge_types = ["PART_OF", "SUPPORTS"]
"#;
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        assert!(
            config.search.reranker.is_none(),
            "reranker should be absent by default"
        );
    }

    #[test]
    fn test_reranker_config_enabled() {
        let toml_str = r#"
[search]
mode = "hybrid"
default_limit = 10
min_score = 0.3
neighbor_decay = 0.7
neighbor_min_score = 0.2
default_neighbor_depth = 1
default_edge_types = ["PART_OF"]

[search.reranker]
enabled = true
model = "bge-reranker-base"
apply_to_top_k = 15
"#;
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        assert!(
            config.search.reranker.is_some(),
            "reranker config should be parsed"
        );
        if let Some(r) = &config.search.reranker {
            assert_eq!(r.enabled, true);
            assert_eq!(r.model, "bge-reranker-base");
            assert_eq!(r.apply_to_top_k, 15);
        }
    }

    #[test]
    fn test_reranker_config_partial() {
        let toml_str = r#"
[search]
mode = "hybrid"
default_limit = 10
min_score = 0.3
neighbor_decay = 0.7
neighbor_min_score = 0.2
default_neighbor_depth = 1
default_edge_types = ["PART_OF"]

[search.reranker]
enabled = true
"#;
        let config: Config = toml::from_str(toml_str).expect("Failed to parse");
        if let Some(r) = &config.search.reranker {
            assert_eq!(r.enabled, true);
            assert_eq!(
                r.model, "ms-marco-MiniLM-L-6-v2",
                "should use default model"
            );
            assert_eq!(r.apply_to_top_k, 15, "should use default top_k");
        }
    }

    #[test]
    fn test_config_env_merge_search_mode() {
        use crate::config_loader::MergeFromEnv;
        // This test verifies that MergeFromEnv trait is implemented
        // Note: Can't easily test actual env var behavior in tests without env::set_var
        let config = Config::default();
        let _merged = config.merge_from_env();
        // If this compiles, the trait is properly implemented
    }

    #[test]
    fn test_resolve_db_path_uses_codex_sandbox_default_for_implicit_path() {
        let temp_config_home = unique_test_dir("sandbox-default");
        std::fs::create_dir_all(&temp_config_home).expect("create temp config dir");

        let _env = EnvGuard::set(&[
            ("CODEX_SANDBOX", Some("seatbelt")),
            ("VOIDM_DB", None),
            ("VOIDM_DATABASE_SQLITE_PATH", None),
            (
                "XDG_CONFIG_HOME",
                Some(temp_config_home.to_string_lossy().as_ref()),
            ),
        ]);

        let resolution = Config::default().resolve_db_path(None, None);

        assert_eq!(resolution.source, DbPathSource::CodexSandboxDefault);
        assert!(resolution
            .path
            .ends_with(Path::new(".codex/memories/voidm/memories.db")));

        let _ = std::fs::remove_dir_all(&temp_config_home);
    }

    #[test]
    fn test_resolve_db_path_honors_env_sqlite_path_before_sandbox_default() {
        let temp_config_home = unique_test_dir("sandbox-env");
        std::fs::create_dir_all(&temp_config_home).expect("create temp config dir");

        let _env = EnvGuard::set(&[
            ("CODEX_SANDBOX", Some("seatbelt")),
            ("VOIDM_DB", None),
            (
                "VOIDM_DATABASE_SQLITE_PATH",
                Some("/tmp/voidm-env-override.db"),
            ),
            (
                "XDG_CONFIG_HOME",
                Some(temp_config_home.to_string_lossy().as_ref()),
            ),
        ]);

        let resolution = Config::default().resolve_db_path(None, None);

        assert_eq!(resolution.source, DbPathSource::EnvSqlitePath);
        assert_eq!(resolution.path, PathBuf::from("/tmp/voidm-env-override.db"));

        let _ = std::fs::remove_dir_all(&temp_config_home);
    }

    #[test]
    fn test_resolve_db_path_honors_explicit_config_file_even_if_it_matches_platform_default() {
        let temp_config_home = unique_test_dir("sandbox-config");
        let voidm_config_dir = temp_config_home.join("voidm");
        std::fs::create_dir_all(&voidm_config_dir).expect("create config dir");

        let platform_default = platform_default_db_path();
        let config_contents = format!(
            "[database.sqlite]\npath = {:?}\n",
            platform_default.to_string_lossy()
        );
        std::fs::write(voidm_config_dir.join("config.toml"), config_contents)
            .expect("write config");

        let _env = EnvGuard::set(&[
            ("CODEX_SANDBOX", Some("seatbelt")),
            ("VOIDM_DB", None),
            ("VOIDM_DATABASE_SQLITE_PATH", None),
            (
                "XDG_CONFIG_HOME",
                Some(temp_config_home.to_string_lossy().as_ref()),
            ),
        ]);

        let config = Config::load();
        let resolution = config.resolve_db_path(None, None);

        assert_eq!(resolution.source, DbPathSource::ConfigFile);
        assert_eq!(resolution.path, platform_default);

        let _ = std::fs::remove_dir_all(&temp_config_home);
    }
}
