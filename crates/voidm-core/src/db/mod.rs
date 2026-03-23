use anyhow::{Context, Result};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::models::{AddMemoryRequest, AddMemoryResponse, EdgeType, LinkResponse, Memory};
use crate::ontology::{
    Concept, ConceptSearchResult, ConceptWithInstances, ConceptWithSimilarityWarning, OntologyEdge,
};
use crate::search::{SearchOptions, SearchResponse};

/// Database abstraction trait for supporting multiple backends (SQLite, Neo4j, etc.)
///
/// All methods are async and return `Result<T>`. Implementations must be Send + Sync
/// to work with async runtime.
///
/// Note: Methods return trait objects (Pin<Box<dyn Future>>) to allow dynamic dispatch
/// without requiring async_trait dependency.
pub trait Database: Send + Sync {
    // ===== Lifecycle =====

    /// Check if the database connection is healthy
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Close the database connection cleanly
    fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Ensure the database schema is initialized
    fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    // ===== Memory CRUD =====

    /// Add a new memory
    fn add_memory(
        &self,
        req: AddMemoryRequest,
        config: &crate::Config,
    ) -> Pin<Box<dyn Future<Output = Result<AddMemoryResponse>> + Send + '_>>;

    /// Get a memory by ID (full ID or short prefix)
    fn get_memory(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Memory>>> + Send + '_>>;

    /// List memories with optional limit
    fn list_memories(
        &self,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Memory>>> + Send + '_>>;

    /// Delete a memory by ID
    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// Update memory content (for re-embedding, etc)
    fn update_memory(
        &self,
        id: &str,
        content: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Resolve a memory ID (from short prefix or full UUID)
    fn resolve_memory_id(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>;

    /// List all scopes used in memories
    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;

    // ===== Memory Edges/Links =====

    /// Create a link between two memories
    fn link_memories(
        &self,
        from_id: &str,
        rel: &EdgeType,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<LinkResponse>> + Send + '_>>;

    /// Remove a link between two memories
    fn unlink_memories(
        &self,
        from_id: &str,
        rel: &EdgeType,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// List all memory-to-memory edges (for migration)
    fn list_edges(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::models::MemoryEdge>>> + Send + '_>>;

    /// List all ontology edges (concept-to-concept, concept-to-memory, etc.)
    fn list_ontology_edges(
        &self,
    ) -> Pin<
        Box<dyn Future<Output = Result<Vec<crate::models::OntologyEdgeForMigration>>> + Send + '_>,
    >;

    /// Create an ontology edge (for migration)
    fn create_ontology_edge(
        &self,
        from_id: &str,
        from_type: &str,
        rel_type: &str,
        to_id: &str,
        to_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    // ===== Search =====

    /// Hybrid search (vector + BM25 + fuzzy)
    fn search_hybrid(
        &self,
        opts: &SearchOptions,
        model_name: &str,
        embeddings_enabled: bool,
        config_min_score: f32,
        config_search: &crate::config::SearchConfig,
    ) -> Pin<Box<dyn Future<Output = Result<SearchResponse>> + Send + '_>>;

    // ===== Ontology Concepts =====

    /// Create a new concept
    fn add_concept(
        &self,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
        id: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithSimilarityWarning>> + Send + '_>>;

    /// Get a concept by ID
    fn get_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Concept>> + Send + '_>>;

    /// Get a concept with its instances, subclasses, and superclasses
    fn get_concept_with_instances(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithInstances>> + Send + '_>>;

    /// List concepts with optional scope filter
    fn list_concepts(
        &self,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Concept>>> + Send + '_>>;

    /// Delete a concept
    fn delete_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// Resolve a concept ID (from short prefix or full UUID)
    fn resolve_concept_id(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>;

    /// Search for concepts by name and description
    fn search_concepts(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ConceptSearchResult>>> + Send + '_>>;

    // ===== Ontology Edges =====

    /// Create an ontology edge (IS_A, INSTANCE_OF, etc)
    fn add_ontology_edge(
        &self,
        from_id: &str,
        from_kind: crate::ontology::NodeKind,
        rel: &crate::ontology::OntologyRelType,
        to_id: &str,
        to_kind: crate::ontology::NodeKind,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<OntologyEdge>> + Send + '_>>;

    /// Delete an ontology edge by ID
    fn delete_ontology_edge(
        &self,
        edge_id: i64,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    // ===== Graph Operations =====

    /// Execute a Cypher query (read-only)
    fn query_cypher(
        &self,
        query: &str,
        params: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Get neighbors of a node at specified depth
    fn get_neighbors(
        &self,
        id: &str,
        depth: usize,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    // ===== Utility =====

    /// Check if embedding model in database matches configured model
    fn check_model_mismatch(
        &self,
        configured_model: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>>;
}

/// Runtime-selected database backend
pub enum DbPool {
    Sqlite(sqlx::SqlitePool),
    Neo4j(Option<()>), // Placeholder; will be neo4j::driver::Driver in Phase 2
}

impl DbPool {
    /// Construct a database instance from configuration
    pub async fn open(config: &crate::config::DatabaseConfig) -> Result<Arc<dyn Database>> {
        match config.backend.to_lowercase().as_str() {
            "sqlite" => {
                let pool = crate::db::sqlite::open_sqlite_pool(&config.sqlite_path).await?;
                Ok(Arc::new(crate::db::sqlite::SqliteDatabase { pool }))
            }
            "neo4j" => {
                let neo4j_config = config
                    .neo4j
                    .as_ref()
                    .context("Neo4j backend selected but no [database.neo4j] config provided")?;
                let db = crate::db::neo4j::Neo4jDatabase::connect(
                    &neo4j_config.uri,
                    &neo4j_config.username,
                    &neo4j_config.password,
                )
                .await?;
                Ok(Arc::new(db))
            }
            other => {
                anyhow::bail!(
                    "Unknown database backend: '{}'. Use 'sqlite' or 'neo4j'",
                    other
                )
            }
        }
    }
}

pub mod neo4j;
pub mod sqlite;

#[cfg(test)]
mod tests;
