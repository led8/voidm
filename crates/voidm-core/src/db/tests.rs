#[cfg(test)]
mod tests {
    use crate::config::SearchConfig;
    use crate::db::Database;
    use crate::models::{AddMemoryRequest, AddMemoryResponse, EdgeType, LinkResponse, Memory};
    use crate::ontology::{
        Concept, ConceptSearchResult, ConceptWithInstances, ConceptWithSimilarityWarning,
        OntologyEdge,
    };
    use crate::search::{SearchOptions, SearchResponse};
    use anyhow::Result;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    /// Dummy implementation to verify the Database trait is object-safe
    struct DummyDatabase;

    impl Database for DummyDatabase {
        fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn add_memory(
            &self,
            _req: AddMemoryRequest,
            _config: &crate::Config,
        ) -> Pin<Box<dyn Future<Output = Result<AddMemoryResponse>> + Send + '_>> {
            Box::pin(async { Err(anyhow::anyhow!("dummy")) })
        }

        fn get_memory(
            &self,
            _id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<Memory>>> + Send + '_>> {
            Box::pin(async { Ok(None) })
        }

        fn list_memories(
            &self,
            _limit: Option<usize>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<Memory>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn delete_memory(
            &self,
            _id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(false) })
        }

        fn update_memory(
            &self,
            _id: &str,
            _content: &str,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn resolve_memory_id(
            &self,
            id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
            let id = id.to_string();
            Box::pin(async move { Ok(id) })
        }

        fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn link_memories(
            &self,
            from_id: &str,
            rel: &EdgeType,
            to_id: &str,
            _note: Option<&str>,
        ) -> Pin<Box<dyn Future<Output = Result<LinkResponse>> + Send + '_>> {
            let from_id = from_id.to_string();
            let rel = rel.clone();
            let to_id = to_id.to_string();
            Box::pin(async move {
                Ok(LinkResponse {
                    created: true,
                    from: from_id,
                    rel: format!("{:?}", rel),
                    to: to_id,
                    conflict_warning: None,
                })
            })
        }

        fn unlink_memories(
            &self,
            _from_id: &str,
            _rel: &EdgeType,
            _to_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(true) })
        }

        fn list_edges(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::models::MemoryEdge>>> + Send + '_>>
        {
            Box::pin(async { Ok(vec![]) })
        }

        fn list_ontology_edges(
            &self,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<Vec<crate::models::OntologyEdgeForMigration>>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(vec![]) })
        }

        fn create_ontology_edge(
            &self,
            _from_id: &str,
            _from_type: &str,
            _rel_type: &str,
            _to_id: &str,
            _to_type: &str,
        ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(false) })
        }

        fn search_hybrid(
            &self,
            _opts: &SearchOptions,
            _model_name: &str,
            _embeddings_enabled: bool,
            _config_min_score: f32,
            _config_search: &SearchConfig,
        ) -> Pin<Box<dyn Future<Output = Result<SearchResponse>> + Send + '_>> {
            Box::pin(async {
                Ok(SearchResponse {
                    results: vec![],
                    threshold_applied: None,
                    best_score: None,
                })
            })
        }

        fn add_concept(
            &self,
            name: &str,
            _description: Option<&str>,
            _scope: Option<&str>,
            _id: Option<&str>,
        ) -> Pin<Box<dyn Future<Output = Result<ConceptWithSimilarityWarning>> + Send + '_>>
        {
            let name = name.to_string();
            Box::pin(async move {
                Ok(ConceptWithSimilarityWarning {
                    id: "c1".to_string(),
                    name,
                    description: None,
                    scope: None,
                    created_at: "2025-01-01".to_string(),
                    similar_concepts: vec![],
                })
            })
        }

        fn get_concept(
            &self,
            id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Concept>> + Send + '_>> {
            let id = id.to_string();
            Box::pin(async move {
                Ok(Concept {
                    id,
                    name: "Test".to_string(),
                    description: None,
                    scope: None,
                    created_at: "2025-01-01".to_string(),
                })
            })
        }

        fn get_concept_with_instances(
            &self,
            id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<ConceptWithInstances>> + Send + '_>> {
            let id = id.to_string();
            Box::pin(async move {
                Ok(ConceptWithInstances {
                    id,
                    name: "Test".to_string(),
                    description: None,
                    scope: None,
                    created_at: "2025-01-01".to_string(),
                    instances: vec![],
                    subclasses: vec![],
                    superclasses: vec![],
                })
            })
        }

        fn list_concepts(
            &self,
            _scope: Option<&str>,
            _limit: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<Concept>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn delete_concept(
            &self,
            _id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(true) })
        }

        fn resolve_concept_id(
            &self,
            id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
            let id = id.to_string();
            Box::pin(async move { Ok(id) })
        }

        fn search_concepts(
            &self,
            _query: &str,
            _scope: Option<&str>,
            _limit: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<ConceptSearchResult>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn add_ontology_edge(
            &self,
            from_id: &str,
            _from_type: crate::ontology::NodeKind,
            _rel: &crate::ontology::OntologyRelType,
            to_id: &str,
            _to_type: crate::ontology::NodeKind,
            _note: Option<&str>,
        ) -> Pin<Box<dyn Future<Output = Result<OntologyEdge>> + Send + '_>> {
            let from_id = from_id.to_string();
            let to_id = to_id.to_string();
            Box::pin(async move {
                Ok(OntologyEdge {
                    id: 1,
                    from_id,
                    from_type: crate::ontology::NodeKind::Memory,
                    rel_type: "IS_A".to_string(),
                    to_id,
                    to_type: crate::ontology::NodeKind::Concept,
                    note: None,
                    created_at: "2025-01-01".to_string(),
                })
            })
        }

        fn delete_ontology_edge(
            &self,
            _edge_id: i64,
        ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(true) })
        }

        fn query_cypher(
            &self,
            _query: &str,
            _params: &serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
            Box::pin(async { Ok(serde_json::json!({})) })
        }

        fn get_neighbors(
            &self,
            _id: &str,
            _depth: usize,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
            Box::pin(async { Ok(serde_json::json!({})) })
        }

        fn check_model_mismatch(
            &self,
            _configured_model: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> {
            Box::pin(async { Ok(None) })
        }
    }

    #[test]
    fn test_database_trait_is_object_safe() {
        // Verify that we can create a trait object
        let db: Arc<dyn Database> = Arc::new(DummyDatabase);
        let _ = db; // Use the variable
    }
}
