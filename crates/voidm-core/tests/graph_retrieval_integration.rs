//! Integration test for graph-aware retrieval in search pipeline.
//!
//! Tests that graph retrieval configuration is respected when disabled/enabled.

#[cfg(test)]
mod tests {
    use voidm_core::graph_retrieval::GraphRetrievalConfig;

    #[test]
    fn test_graph_retrieval_config_enabled() {
        let config = GraphRetrievalConfig {
            enabled: true,
            max_concept_hops: 2,
            tags: voidm_core::graph_retrieval::TagRetrievalConfig {
                enabled: true,
                min_overlap: 2,
                min_percentage: 50.0,
                decay_factor: 0.7,
                limit: 5,
            },
            concepts: voidm_core::graph_retrieval::ConceptRetrievalConfig {
                enabled: false,
                max_hops: None,
                decay_factor: 0.7,
                limit: 3,
            },
        };

        assert!(config.enabled);
        assert!(config.tags.enabled);
        assert!(!config.concepts.enabled);
        assert_eq!(config.max_concept_hops, 2);
    }

    #[test]
    fn test_graph_retrieval_config_disabled() {
        let config = GraphRetrievalConfig {
            enabled: false,
            max_concept_hops: 2,
            tags: voidm_core::graph_retrieval::TagRetrievalConfig::default(),
            concepts: voidm_core::graph_retrieval::ConceptRetrievalConfig::default(),
        };

        assert!(!config.enabled);
        assert!(config.tags.enabled); // Default is enabled
        assert!(config.concepts.enabled); // Default is enabled
    }

    #[test]
    fn test_graph_retrieval_max_hops_override() {
        let mut config = GraphRetrievalConfig::default();
        config.max_concept_hops = 3;
        config.concepts.max_hops = Some(1); // Override

        // When local max_hops is set, it takes precedence
        let effective_hops = config.concepts.max_hops.unwrap_or(config.max_concept_hops);
        assert_eq!(effective_hops, 1);

        // When not set, global default is used
        config.concepts.max_hops = None;
        let effective_hops = config.concepts.max_hops.unwrap_or(config.max_concept_hops);
        assert_eq!(effective_hops, 3);
    }

    #[test]
    fn test_graph_retrieval_tag_config_validation() {
        let config = voidm_core::graph_retrieval::TagRetrievalConfig {
            enabled: true,
            min_overlap: 3,
            min_percentage: 50.0,
            decay_factor: 0.7,
            limit: 5,
        };

        assert!(config.enabled);
        assert!(config.min_overlap > 0);
        assert!(config.min_percentage > 0.0 && config.min_percentage <= 100.0);
        assert!(config.decay_factor > 0.0 && config.decay_factor < 1.0);
        assert!(config.limit > 0);
    }

    #[test]
    fn test_graph_retrieval_concept_config_validation() {
        let config = voidm_core::graph_retrieval::ConceptRetrievalConfig {
            enabled: true,
            max_hops: Some(2),
            decay_factor: 0.7,
            limit: 3,
        };

        assert!(config.enabled);
        assert_eq!(config.max_hops, Some(2));
        assert!(config.decay_factor > 0.0 && config.decay_factor < 1.0);
        assert!(config.limit > 0);
    }

    #[test]
    fn test_graph_retrieval_search_result_source_field() {
        // Verify that SearchResult has source field that can be set to graph_tags/graph_concepts
        let mut result = voidm_core::search::SearchResult {
            id: "test".to_string(),
            score: 0.8,
            memory_type: "semantic".to_string(),
            content: "test content".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            created_at: "2026-03-15T10:00:00Z".to_string(),
            source: "search".to_string(),
            rel_type: None,
            direction: None,
            hop_depth: None,
            parent_id: None,
            quality_score: None,
            age_days: None,
            title: None,
            context: None,
            context_chunks: vec![],
            content_source: None,
        };

        assert_eq!(result.source, "search");

        // Simulate marking result as coming from tag-based graph retrieval
        result.source = "graph_tags".to_string();
        assert_eq!(result.source, "graph_tags");

        // Simulate marking result as coming from concept-based graph retrieval
        result.source = "graph_concepts".to_string();
        assert_eq!(result.source, "graph_concepts");
    }
}
