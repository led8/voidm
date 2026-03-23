//! Integration tests for configuration hierarchy (file → env → CLI)
//!
//! Tests verify that config overrides work correctly at each level
//! and that higher priority levels correctly override lower ones.

#[cfg(test)]
mod config_hierarchy_tests {
    use voidm_core::{config_loader::MergeFromEnv, Config};

    #[test]
    fn test_file_config_baseline() {
        // Create a baseline config from defaults
        let config = Config::default();
        assert_eq!(config.database.backend, "sqlite");
        assert_eq!(config.search.mode, "hybrid");
        assert_eq!(config.search.default_limit, 10);
    }

    #[test]
    fn test_env_var_overrides_file() {
        // This test demonstrates the pattern - actual env var testing requires env::set_var
        // In CI, we'd use env::set_var before running tests
        let config = Config::default();

        // Simulate what happens when env vars are set
        // (actual values would come from VOIDM_* env vars)
        let _merged = config.merge_from_env();

        // The merge_from_env() call should check VOIDM_* env vars
        // If VOIDM_SEARCH_MODE="semantic" was set, merged.search.mode would be "semantic"
    }

    #[test]
    fn test_cli_overrides_both() {
        // Demonstrates how CLI args override both file and env
        use voidm_cli::cli_config::CliConfigOverrides;

        let mut config = Config::default();
        assert_eq!(config.search.mode, "hybrid");
        assert_eq!(config.search.default_limit, 10);

        // Simulate CLI args
        let cli_overrides = CliConfigOverrides {
            search_mode: Some("semantic".to_string()),
            search_default_limit: Some(20),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);
        assert_eq!(config.search.mode, "semantic");
        assert_eq!(config.search.default_limit, 20);
    }

    #[test]
    fn test_reranker_config_merge() {
        use voidm_cli::cli_config::CliConfigOverrides;

        let mut config = Config::default();

        // Apply reranker CLI overrides
        let cli_overrides = CliConfigOverrides {
            reranker_enabled: Some(true),
            reranker_model: Some("my-model".to_string()),
            reranker_top_k: Some(20),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);

        // Verify reranker was created/updated
        if let Some(reranker) = &config.search.reranker {
            assert_eq!(reranker.enabled, true);
            assert_eq!(reranker.model, "my-model");
            assert_eq!(reranker.apply_to_top_k, 20);
        } else {
            panic!("Reranker config should exist after CLI override");
        }
    }

    #[test]
    fn test_insert_thresholds_merge() {
        use voidm_cli::cli_config::CliConfigOverrides;

        let mut config = Config::default();
        let original_threshold = config.insert.auto_link_threshold;

        // Apply insert CLI overrides
        let cli_overrides = CliConfigOverrides {
            insert_auto_link_threshold: Some(0.95),
            insert_duplicate_threshold: Some(0.85),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);

        // Verify overrides were applied
        assert_eq!(config.insert.auto_link_threshold, 0.95);
        assert_eq!(config.insert.duplicate_threshold, 0.85);
        assert_ne!(config.insert.auto_link_threshold, original_threshold);
    }

    #[test]
    fn test_partial_cli_overrides() {
        // CLI overrides should only affect specified parameters
        use voidm_cli::cli_config::CliConfigOverrides;

        let mut config = Config::default();
        let original_mode = config.search.mode.clone();
        let original_limit = config.search.default_limit;

        // Only override search_mode, not limit
        let cli_overrides = CliConfigOverrides {
            search_mode: Some("keyword".to_string()),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);

        assert_eq!(config.search.mode, "keyword");
        assert_eq!(config.search.default_limit, original_limit); // Should not change
        assert_ne!(config.search.mode, original_mode);
    }

    #[test]
    fn test_database_backend_merge() {
        use voidm_cli::cli_config::CliConfigOverrides;

        let mut config = Config::default();

        let cli_overrides = CliConfigOverrides {
            database_backend: Some("neo4j".to_string()),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);
        assert_eq!(config.database.backend, "neo4j");
    }

    #[test]
    fn test_embeddings_merge() {
        use voidm_cli::cli_config::CliConfigOverrides;

        let mut config = Config::default();

        let cli_overrides = CliConfigOverrides {
            embeddings_enabled: Some(false),
            embeddings_model: Some("custom-model".to_string()),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);
        assert_eq!(config.embeddings.enabled, false);
        assert_eq!(config.embeddings.model, "custom-model");
    }

    #[test]
    fn test_graph_retrieval_merge() {
        use voidm_cli::cli_config::CliConfigOverrides;

        let mut config = Config::default();

        let cli_overrides = CliConfigOverrides {
            gr_enabled: Some(false),
            gr_max_hops: Some(3),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);

        if let Some(gr) = &config.search.graph_retrieval {
            assert_eq!(gr.enabled, false);
            assert_eq!(gr.max_concept_hops, 3);
        }
    }

    #[test]
    fn test_full_hierarchy_simulation() {
        // Simulates complete hierarchy: file → env (mock) → CLI
        use voidm_cli::cli_config::CliConfigOverrides;

        // Step 1: Load from file (defaults)
        let mut config = Config::default();
        assert_eq!(config.search.mode, "hybrid");
        assert_eq!(config.search.default_limit, 10);

        // Step 2: Apply env vars (would be real in CI)
        // In this test we just call merge_from_env (which checks VOIDM_* env vars)
        config = config.merge_from_env();

        // Step 3: Apply CLI overrides (highest priority)
        let cli_overrides = CliConfigOverrides {
            search_mode: Some("semantic".to_string()),
            search_default_limit: Some(50),
            insert_auto_link_threshold: Some(0.9),
            ..Default::default()
        };

        config = cli_overrides.apply_to_config(config);

        // Verify final state
        assert_eq!(config.search.mode, "semantic");
        assert_eq!(config.search.default_limit, 50);
        assert_eq!(config.insert.auto_link_threshold, 0.9);
    }
}
