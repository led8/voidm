# voidm Neo4j Backend Support - Phase 1 Refinement Plan

## Current Codebase Structure Analysis

**Core Modules (voidm-core):**
- `db.rs` (39 lines) - SQLite pool initialization
- `crud.rs` (562 lines) - Memory CRUD operations
- `ontology.rs` (1753 lines) - Concept management & hierarchy
- `search.rs` (443 lines) - Hybrid search (vector + BM25 + fuzzy)
- `migrate.rs` - Schema migrations
- `models.rs` - Data structures

**Current Flow:**
```
CLI → SqlitePool → CRUD/Ontology/Search → SQL queries
                ↓
              Migrations apply schema
```

## Phase 1: Foundation Architecture

### 1.1 Database Trait Definition

**File:** `crates/voidm-core/src/db/mod.rs` (NEW)

Core database operations contract:

```rust
pub trait Database: Send + Sync {
    // Lifecycle
    async fn health_check(&self) -> Result<()>;
    async fn close(&self) -> Result<()>;
    
    // Schema
    async fn ensure_schema(&self) -> Result<()>;
    
    // Memory CRUD
    async fn add_memory(&self, req: AddMemoryRequest) -> Result<AddMemoryResponse>;
    async fn get_memory(&self, id: &str) -> Result<Option<Memory>>;
    async fn list_memories(&self, limit: Option<usize>) -> Result<Vec<Memory>>;
    async fn delete_memory(&self, id: &str) -> Result<bool>;
    async fn update_memory(&self, id: &str, content: &str) -> Result<()>;
    
    // Memory Edges
    async fn link_memories(&self, from: &str, rel: &EdgeType, to: &str, note: Option<&str>) -> Result<Edge>;
    async fn unlink_memories(&self, from: &str, rel: &EdgeType, to: &str) -> Result<bool>;
    
    // Search
    async fn search_hybrid(&self, opts: &SearchOptions) -> Result<SearchResults>;
    
    // Ontology
    async fn add_concept(&self, name: &str, description: Option<&str>, scope: Option<&str>) -> Result<Concept>;
    async fn get_concept(&self, id: &str) -> Result<Option<ConceptFull>>;
    async fn list_concepts(&self, scope: Option<&str>, limit: usize) -> Result<Vec<Concept>>;
    // ... more ontology methods
    
    // Graph
    async fn query_cypher(&self, query: &str, params: &str) -> Result<CypherResult>;
    async fn get_neighbors(&self, id: &str, depth: usize) -> Result<GraphNeighbors>;
}
```

### 1.2 Database Enum Wrapper

**File:** `crates/voidm-core/src/db/pool.rs` (NEW)

```rust
pub enum DbPool {
    Sqlite(SqlitePool),
    Neo4j(Neo4jDriver),
}

impl DbPool {
    pub async fn open(config: &DatabaseConfig) -> Result<Self> {
        match config.backend.as_str() {
            "sqlite" => { ... }
            "neo4j" => { ... }
            _ => bail!("Unknown backend: {}", config.backend),
        }
    }
}
```

### 1.3 SqliteDatabase Wrapper

**File:** `crates/voidm-core/src/db/sqlite.rs` (NEW)

- Wraps existing functions from `crud.rs`, `ontology.rs`, etc
- Implements `Database` trait
- **Zero logic changes** - just call existing functions through trait

Example:
```rust
pub struct SqliteDatabase {
    pool: SqlitePool,
}

impl Database for SqliteDatabase {
    async fn add_memory(&self, req: AddMemoryRequest) -> Result<AddMemoryResponse> {
        crud::add_memory(&self.pool, req, ...).await
    }
    // ... delegate to existing functions
}
```

### 1.4 Configuration Extension

**File:** `crates/voidm-core/src/config.rs` (MODIFY)

Add to existing config:
```toml
[database]
backend = "sqlite"  # "sqlite" or "neo4j"
sqlite_path = "~/.local/share/voidm/memories.db"

[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "password"
```

### 1.5 Neo4j Driver Integration

**File:** `crates/voidm-core/src/db/neo4j.rs` (NEW - SKELETON)

```rust
pub struct Neo4jDatabase {
    driver: Driver,  // neo4j::driver::Driver
}

impl Neo4jDatabase {
    pub async fn connect(uri: &str, username: &str, password: &str) -> Result<Self> {
        // neo4j-rs integration
    }
}

impl Database for Neo4jDatabase {
    // Implement all methods (start with stub/error responses)
}
```

### 1.6 Dependency Updates

**File:** `crates/voidm-core/Cargo.toml`

```toml
[dependencies]
neo4j = { version = "0.5", optional = true }

[features]
neo4j = ["dep:neo4j"]
default = ["neo4j"]
```

### 1.7 Module Exports

**File:** `crates/voidm-core/src/lib.rs` (MODIFY)

```rust
pub mod db;

// Re-export for convenience
pub use db::{Database, DbPool, open_pool};
```

### 1.8 CLI Refactoring

**File:** `crates/voidm-cli/src/main.rs` (MODIFY)

```rust
// Instead of:
let pool = open_pool(&db_path).await?;

// Use:
let db_pool = DbPool::open(&config.database).await?;
let db = db_pool.as_database();

// Pass `db` (impl Database) instead of `SqlitePool`
```

### 1.9 Existing Command Refactoring Strategy

**Minimal Changes:**
- Add trait object parameter: `db: Arc<dyn Database>`
- Keep logic unchanged - trait delegates to existing functions
- CRUD functions remain private to `db` module
- Commands call through trait instead of directly

Example:
```rust
// Old
pub async fn add(args: AddArgs, pool: SqlitePool, config: Config) -> Result<()> {
    let resp = crud::add_memory(&pool, req, &config).await?;
}

// New (zero logic change)
pub async fn add(args: AddArgs, db: Arc<dyn Database>, config: Config) -> Result<()> {
    let resp = db.add_memory(req).await?;
}
```

## Phase 1 Checklist

- [ ] Create `crates/voidm-core/src/db/mod.rs` with `Database` trait
- [ ] Create `crates/voidm-core/src/db/pool.rs` with `DbPool` enum
- [ ] Create `crates/voidm-core/src/db/sqlite.rs` - wrapper around existing code
- [ ] Create `crates/voidm-core/src/db/neo4j.rs` - skeleton (all methods → error/stub)
- [ ] Add `neo4j` dependency to `Cargo.toml`
- [ ] Extend `config.rs` with `[database]` section
- [ ] Update `lib.rs` exports
- [ ] Refactor `main.rs` to use `DbPool` and trait objects
- [ ] Update all CLI commands to accept `db: Arc<dyn Database>` instead of `SqlitePool`
- [ ] Update `migrate.rs` to work through trait
- [ ] Add unit tests for trait contract (dummy impl)
- [ ] Verify all existing tests pass with new abstraction
- [ ] Document trait expectations

## Key Design Decisions for Phase 1

**1. Trait vs. Enum Approach:**
   - Use `Arc<dyn Database>` trait objects at call sites (flexible for future backends)
   - DbPool enum handles construction logic
   - ✅ Cleaner for commands

**2. Error Handling:**
   - Both backends use `anyhow::Result`
   - Standardize error messages for compatibility

**3. No Neo4j Logic Yet:**
   - Neo4jDatabase skeleton returns errors/unimplemented
   - Phase 2 implements actual Neo4j operations
   - Allows us to validate trait design first

**4. Backwards Compatibility:**
   - Default config: `backend = "sqlite"`
   - Existing installations unaffected
   - New installations can opt-in to Neo4j

**5. Testing Approach:**
   - Unit tests: mock impl of `Database` trait
   - Integration tests: run with both backends (when Phase 2 done)
   - Existing tests: run against SqliteDatabase wrapper

## File Changes Summary

**New Files (6):**
- `crates/voidm-core/src/db/mod.rs` (trait definition)
- `crates/voidm-core/src/db/pool.rs` (pool enum)
- `crates/voidm-core/src/db/sqlite.rs` (wrapper ~150 lines)
- `crates/voidm-core/src/db/neo4j.rs` (skeleton ~100 lines)
- `crates/voidm-core/src/db/tests.rs` (unit tests ~200 lines)
- `.github/workflows/neo4j-ci.yml` (Docker Neo4j for CI)

**Modified Files (6):**
- `crates/voidm-core/src/lib.rs` (+5 lines)
- `crates/voidm-core/src/config.rs` (+30 lines)
- `crates/voidm-core/Cargo.toml` (+2 lines)
- `crates/voidm-cli/src/main.rs` (~30 line changes)
- `crates/voidm-cli/src/commands/*.rs` (trait objects instead of SqlitePool)
- `crates/voidm-core/src/migrate.rs` (trait instead of SqlitePool)

**Total Estimated Changes:** ~600 lines (mostly new, minimal refactoring)

## Commit Strategy

Separate commits:
1. "feat(db): add Database trait abstraction"
2. "feat(db): add DbPool enum and pool factory"
3. "feat(db): implement SqliteDatabase wrapper"
4. "feat(db): add Neo4j driver skeleton"
5. "refactor(core,cli): migrate to trait-based database"
6. "test(db): add database trait contract tests"
7. "config: add database backend configuration"

## Next Steps

1. Review and approve architecture
2. Start implementing Phase 1 (Database trait)
3. Create individual commits as listed above
4. Ensure all tests pass
5. Plan Phase 2 (Neo4j implementation) based on Phase 1 learnings
