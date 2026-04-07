pub mod auto_tagger;
pub mod chunking;
pub mod config;
pub mod config_loader;
pub mod crud;
pub mod db;
pub mod embeddings;
pub mod gguf_query_expander;
pub mod graph_retrieval;
pub mod learning;
pub mod migrate;
pub mod migration;
pub mod models;
pub mod ner;
pub mod nli;
pub mod ontology;
pub mod passage;
pub mod quality;
pub mod query;
pub mod query_expansion;
pub mod redactor;
pub mod reranker;
pub mod rrf_fusion;
pub mod search;
pub mod semantic_dedup;
pub mod tag_linker;
pub mod vector;

pub use config::{
    codex_sandbox_db_path, config_path_display, is_codex_sandbox_active, Config, DbPathResolution,
    DbPathSource,
};
pub use crud::{find_contradicts_among, get_edges_for_memory, resolve_id, update_memory, UpdateMemoryPatch};
pub use search::compute_age_days;
pub use db::sqlite::open_pool; // Re-export for backward compatibility
pub use models::{
    AddMemoryRequest, AddMemoryResponse, DuplicateWarning, Memory, MemoryEdge, MemoryType,
    OntologyEdgeForMigration, SuggestedLink,
};
