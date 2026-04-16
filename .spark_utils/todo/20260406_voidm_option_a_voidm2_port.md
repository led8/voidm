# Task: Option A — voidm-2 Architecture Port

## Checklist

### F1 — Memory model enrichment (`title` + `context` fields)
- [x] Add idempotent migration for `title` and `context` columns
- [x] Add `title` and `context` to `Memory` struct
- [x] Update `crud::add_memory` to persist both fields
- [x] Update `crud::get_memory` and `crud::update_memory`
- [x] Add `--title` and `--context` flags to `voidm add` and `voidm update`
- [x] Update `voidm get` and `voidm list` output
- [x] Add title-based score boost in search (post-RRF tiebreaker)
- [x] Update Neo4j backend
- [x] cargo check passes

### F2 — Graph analytics (`graph path` + `graph pagerank`)
- [x] Add `GraphCommands::Path` and `GraphCommands::Pagerank` variants
- [x] Implement BFS path-finding in `commands/graph.rs`
- [x] Implement iterative PageRank in `commands/graph.rs`
- [x] Wire Neo4j backend (native Cypher)
- [x] JSON and human output for both
- [x] cargo check passes

### F3 — Text chunking layer
- [x] Add `chunks` table and `vec_chunks` virtual table in `migrate.rs`
- [x] Add `ChunkingConfig` to config
- [x] Implement `chunk_text()` in `voidm-core/src/chunking.rs`
- [x] Update `crud::add_memory` to chunk + embed at chunk level
- [x] Update `crud::delete_memory` to cascade delete chunks
- [x] Update `crud::update_memory` to re-chunk + re-embed
- [x] Add `collect_chunk_hits()` helper and integrate into search pipeline
- [x] Update RRF fusion to include chunk ANN signal
- [x] Add `context_chunks` and `content_source` to `SearchResult`
- [x] Update `vector::reembed_all` to rebuild chunk embeddings
- [x] Update Neo4j backend (best-effort, `context_chunks: vec![]`)
- [x] cargo check passes

### F4 — Unified RRF pipeline
- [x] Add `[search.signals]` to config (`vector`, `bm25`, `fuzzy`) via `SignalsConfig`
- [x] Refactor `search::search()` to single RRF path with dynamic signal set
- [x] Map legacy mode names to signal presets (semantic/bm25/keyword/fuzzy/hybrid/hybrid-rrf)
- [x] Remove `search_with_rrf()` — logic inlined into unified `search()`
- [x] All `--mode` values still work (backward compatible aliases preserved)
- [x] cargo check passes

### F5 — `voidm-db` abstraction trait
- [x] Add 3 new methods to `Database` trait in `db/mod.rs` (`sqlite_pool`, `update_memory_full`, `list_memories_filtered`)
- [x] Implement all 3 in `SqliteDatabase` (`db/sqlite.rs`)
- [x] Add stubs for all 3 in `Neo4jDatabase` (`db/neo4j.rs`)
- [x] Update `main.rs`: replace `open_pool` with `DbPool::open()`, thread `Arc<dyn Database>`
- [x] Update all CLI command `run()` signatures: `pool: &SqlitePool` → `db: &Arc<dyn Database>`
- [x] Use `db.sqlite_pool()` escape hatch in commands with SQLite-only internals
- [x] Use `db.update_memory_full()` in `mem_update.rs` (trait method instead of direct crud call)
- [x] cargo check passes

## Notes
- F3 is the highest-impact change — do not skip
- F5 is deferred; do not start until explicitly approved
- Each phase must pass `cargo check` before starting the next
