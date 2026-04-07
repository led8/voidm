use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::str::FromStr;

use crate::models::{AddMemoryRequest, AddMemoryResponse, EdgeType, LinkResponse, Memory};
use crate::ontology::{
    Concept, ConceptSearchResult, ConceptWithInstances, ConceptWithSimilarityWarning, OntologyEdge,
};
use crate::search::{SearchOptions, SearchResponse};

/// Load sqlite-vec at process level via sqlite3_auto_extension.
/// Must be called once before creating any connections.
fn ensure_sqlite_vec_loaded() {
    use once_cell::sync::OnceCell;
    static LOADED: OnceCell<()> = OnceCell::new();
    LOADED.get_or_init(|| unsafe {
        libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

/// Open (or create) the SQLite pool. Enables WAL mode, foreign keys, and sqlite-vec.
pub async fn open_pool(db_path: &Path) -> Result<SqlitePool> {
    ensure_sqlite_vec_loaded();

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create directory {}", parent.display()))?;
    }

    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1) // SQLite: single writer
        .connect_with(opts)
        .await
        .with_context(|| format!("Cannot open database at {}", db_path.display()))?;

    Ok(pool)
}

/// Open an SQLite connection pool from a path string
pub async fn open_sqlite_pool(db_path: &str) -> Result<SqlitePool> {
    let path = Path::new(db_path);
    open_pool(path).await
}

/// SQLite implementation of the Database trait.
/// Wraps all existing CRUD, ontology, and search functions without changing their logic.
pub struct SqliteDatabase {
    pub pool: SqlitePool,
}

impl crate::db::Database for SqliteDatabase {
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            // Simple query to check connection
            sqlx::query("SELECT 1").execute(&self.pool).await?;
            Ok(())
        })
    }

    fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.pool.close().await;
            Ok(())
        })
    }

    fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move { crate::migrate::run(&pool).await })
    }

    fn add_memory(
        &self,
        req: AddMemoryRequest,
        config: &crate::Config,
    ) -> Pin<Box<dyn Future<Output = Result<AddMemoryResponse>> + Send + '_>> {
        let pool = self.pool.clone();
        let config = config.clone();
        Box::pin(async move { crate::crud::add_memory(&pool, req, &config).await })
    }

    fn get_memory(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Memory>>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move { crate::crud::get_memory(&pool, &id).await })
    }

    fn list_memories(
        &self,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Memory>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            crate::crud::list_memories(&pool, None, None, limit.unwrap_or(100)).await
        })
    }

    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move { crate::crud::delete_memory(&pool, &id).await })
    }

    fn update_memory(
        &self,
        id: &str,
        content: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let content = content.to_string();
        Box::pin(async move {
            // Update memory content in DB
            sqlx::query("UPDATE memories SET content = ? WHERE id = ?")
                .bind(&content)
                .bind(&id)
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    fn resolve_memory_id(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move { crate::crud::resolve_id(&pool, &id).await })
    }

    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move { crate::crud::list_scopes(&pool).await })
    }

    fn link_memories(
        &self,
        from_id: &str,
        rel: &EdgeType,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<LinkResponse>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel = rel.clone();
        let note = note.map(|s| s.to_string());
        Box::pin(async move {
            crate::crud::link_memories(&pool, &from_id, &rel, &to_id, note.as_deref()).await
        })
    }

    fn unlink_memories(
        &self,
        from_id: &str,
        rel: &EdgeType,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel = rel.clone();
        Box::pin(async move { crate::crud::unlink_memories(&pool, &from_id, &rel, &to_id).await })
    }

    fn list_edges(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::models::MemoryEdge>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move { crate::crud::list_edges(&pool).await })
    }

    fn list_ontology_edges(
        &self,
    ) -> Pin<
        Box<dyn Future<Output = Result<Vec<crate::models::OntologyEdgeForMigration>>> + Send + '_>,
    > {
        let pool = self.pool.clone();
        Box::pin(async move { crate::crud::list_ontology_edges(&pool).await })
    }

    fn create_ontology_edge(
        &self,
        _from_id: &str,
        _from_type: &str,
        _rel_type: &str,
        _to_id: &str,
        _to_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        // SQLite doesn't support ontology edge creation via this trait
        // It would need to be added to the crud module
        Box::pin(async { Ok(false) })
    }

    fn search_hybrid(
        &self,
        opts: &SearchOptions,
        model_name: &str,
        embeddings_enabled: bool,
        config_min_score: f32,
        config_search: &crate::config::SearchConfig,
    ) -> Pin<Box<dyn Future<Output = Result<SearchResponse>> + Send + '_>> {
        let pool = self.pool.clone();
        let opts_owned: SearchOptions = opts.clone();
        let model_name_owned: String = model_name.to_string();
        let search_config_owned: crate::config::SearchConfig = config_search.clone();

        let future = async move {
            crate::search::search(
                &pool,
                &opts_owned,
                &model_name_owned,
                embeddings_enabled,
                config_min_score,
                &search_config_owned,
            )
            .await
        };

        Box::pin(future)
    }

    fn add_concept(
        &self,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
        _id: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithSimilarityWarning>> + Send + '_>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let description = description.map(|s| s.to_string());
        let scope = scope.map(|s| s.to_string());
        Box::pin(async move {
            crate::ontology::add_concept(&pool, &name, description.as_deref(), scope.as_deref())
                .await
        })
    }

    fn get_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Concept>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move { crate::ontology::get_concept(&pool, &id).await })
    }

    fn get_concept_with_instances(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<ConceptWithInstances>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move { crate::ontology::get_concept_with_instances(&pool, &id).await })
    }

    fn list_concepts(
        &self,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Concept>>> + Send + '_>> {
        let pool = self.pool.clone();
        let scope = scope.map(|s| s.to_string());
        Box::pin(
            async move { crate::ontology::list_concepts(&pool, scope.as_deref(), limit).await },
        )
    }

    fn delete_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move { crate::ontology::delete_concept(&pool, &id).await })
    }

    fn resolve_concept_id(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move { crate::ontology::resolve_concept_id(&pool, &id).await })
    }

    fn search_concepts(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ConceptSearchResult>>> + Send + '_>> {
        let pool = self.pool.clone();
        let query = query.to_string();
        let scope = scope.map(|s| s.to_string());
        Box::pin(async move {
            crate::ontology::search_concepts(&pool, &query, scope.as_deref(), limit).await
        })
    }

    fn add_ontology_edge(
        &self,
        from_id: &str,
        from_kind: crate::ontology::NodeKind,
        rel: &crate::ontology::OntologyRelType,
        to_id: &str,
        to_kind: crate::ontology::NodeKind,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<OntologyEdge>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let to_id = to_id.to_string();
        let rel = rel.clone();
        let note = note.map(|s| s.to_string());
        Box::pin(async move {
            crate::ontology::add_ontology_edge(
                &pool,
                &from_id,
                from_kind,
                &rel,
                &to_id,
                to_kind,
                note.as_deref(),
            )
            .await
        })
    }

    fn delete_ontology_edge(
        &self,
        edge_id: i64,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move { crate::ontology::delete_ontology_edge(&pool, edge_id).await })
    }

    fn query_cypher(
        &self,
        _query: &str,
        _params: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        Box::pin(async move {
            // For now, return error - Cypher translation layer not implemented yet
            anyhow::bail!("Cypher queries not yet supported on SQLite backend")
        })
    }

    fn get_neighbors(
        &self,
        _id: &str,
        _depth: usize,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        Box::pin(async move {
            // Placeholder for graph traversal
            anyhow::bail!("Graph traversal not yet implemented on SQLite backend")
        })
    }

    fn check_model_mismatch(
        &self,
        configured_model: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let configured_model = configured_model.to_string();
        Box::pin(async move { crate::crud::check_model_mismatch(&pool, &configured_model).await })
    }

    fn sqlite_pool(&self) -> Option<&sqlx::SqlitePool> {
        Some(&self.pool)
    }

    fn update_memory_full(
        &self,
        id: &str,
        patch: crate::crud::UpdateMemoryPatch,
        config: &crate::Config,
    ) -> Pin<Box<dyn Future<Output = Result<crate::models::Memory>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let config = config.clone();
        Box::pin(async move { crate::crud::update_memory(&pool, &id, patch, &config).await })
    }

    fn list_memories_filtered(
        &self,
        scope_filter: Option<String>,
        type_filter: Option<String>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::models::Memory>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            crate::crud::list_memories(
                &pool,
                scope_filter.as_deref(),
                type_filter.as_deref(),
                limit,
            )
            .await
        })
    }
}
