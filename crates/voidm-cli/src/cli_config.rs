//! CLI configuration override helper
//!
//! Provides utilities to merge CLI args with Config struct
//! Implements the top layer of config hierarchy: file → env → CLI

use voidm_core::Config;

/// Configuration overrides from CLI arguments
#[derive(Debug, Clone, Default)]
pub struct CliConfigOverrides {
    // Database
    pub database_backend: Option<String>,
    pub database_sqlite_path: Option<String>,

    // Embeddings
    pub embeddings_enabled: Option<bool>,
    pub embeddings_model: Option<String>,

    // Search
    pub search_mode: Option<String>,
    pub search_default_limit: Option<usize>,
    pub search_min_score: Option<f32>,

    // Reranker
    pub reranker_enabled: Option<bool>,
    pub reranker_model: Option<String>,
    pub reranker_top_k: Option<usize>,

    // Query Expansion
    pub qe_enabled: Option<bool>,
    pub qe_timeout_ms: Option<usize>,

    // Graph Retrieval
    pub gr_enabled: Option<bool>,
    pub gr_max_hops: Option<usize>,

    // Insert
    pub insert_auto_link_threshold: Option<f32>,
    pub insert_duplicate_threshold: Option<f32>,
}

impl CliConfigOverrides {
    /// Apply CLI overrides to a Config (CLI takes highest priority)
    pub fn apply_to_config(self, mut config: Config) -> Config {
        // Database
        if let Some(backend) = self.database_backend {
            config.database.backend = backend;
        }
        if let Some(path) = self.database_sqlite_path {
            config.database.sqlite_path = path.clone();
            if let Some(sqlite) = &mut config.database.sqlite {
                sqlite.path = Some(path);
            } else {
                config.database.sqlite =
                    Some(voidm_core::config::SqliteConfig { path: Some(path) });
            }
        }

        // Embeddings
        if let Some(enabled) = self.embeddings_enabled {
            config.embeddings.enabled = enabled;
        }
        if let Some(model) = self.embeddings_model {
            config.embeddings.model = model;
        }

        // Search
        if let Some(mode) = self.search_mode {
            config.search.mode = mode;
        }
        if let Some(limit) = self.search_default_limit {
            config.search.default_limit = limit;
        }
        if let Some(min_score) = self.search_min_score {
            config.search.min_score = min_score;
        }

        // Reranker
        if let Some(enabled) = self.reranker_enabled {
            if let Some(mut r) = config.search.reranker.take() {
                r.enabled = enabled;
                config.search.reranker = Some(r);
            } else {
                let mut r = voidm_core::config::RerankerConfig::default();
                r.enabled = enabled;
                config.search.reranker = Some(r);
            }
        }
        if let Some(model) = self.reranker_model {
            if let Some(mut r) = config.search.reranker.take() {
                r.model = model;
                config.search.reranker = Some(r);
            } else {
                let mut r = voidm_core::config::RerankerConfig::default();
                r.model = model;
                config.search.reranker = Some(r);
            }
        }
        if let Some(top_k) = self.reranker_top_k {
            if let Some(mut r) = config.search.reranker.take() {
                r.apply_to_top_k = top_k;
                config.search.reranker = Some(r);
            }
        }

        // Query Expansion
        if let Some(enabled) = self.qe_enabled {
            if let Some(mut qe) = config.search.query_expansion.take() {
                qe.enabled = enabled;
                config.search.query_expansion = Some(qe);
            }
        }
        if let Some(timeout) = self.qe_timeout_ms {
            if let Some(mut qe) = config.search.query_expansion.take() {
                qe.timeout_ms = timeout as u64;
                config.search.query_expansion = Some(qe);
            }
        }

        // Graph Retrieval
        if let Some(enabled) = self.gr_enabled {
            if let Some(mut gr) = config.search.graph_retrieval.take() {
                gr.enabled = enabled;
                config.search.graph_retrieval = Some(gr);
            }
        }
        if let Some(hops) = self.gr_max_hops {
            if let Some(mut gr) = config.search.graph_retrieval.take() {
                gr.max_concept_hops = hops as u8;
                config.search.graph_retrieval = Some(gr);
            }
        }

        // Insert
        if let Some(threshold) = self.insert_auto_link_threshold {
            config.insert.auto_link_threshold = threshold;
        }
        if let Some(threshold) = self.insert_duplicate_threshold {
            config.insert.duplicate_threshold = threshold;
        }

        config
    }
}
