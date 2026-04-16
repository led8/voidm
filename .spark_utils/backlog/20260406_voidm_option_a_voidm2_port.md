# Backlog: Option A — voidm-2 Architecture Port

**Date:** 2026-04-06
**Repo:** voidm
**Scope:** Port the meaningful architectural improvements from voidm-2 into the current voidm codebase, without the MCP crate. Preserve all existing agent UX features (recall, update, stale, why, batch-add, scope detect, --agent, learn layer).

---

## Step 1 — Implementation Plan

Four phases ordered by impact vs risk. Each phase is independently shippable and cargo-check verified before the next begins.

---

### Phase 1 — F1: Memory model enrichment (`title` + `context` fields)

**What:** Add two optional fields to the Memory model and all CRUD surfaces.
- `title: Option<String>` — max 200 chars, for faster lexical retrieval and display
- `context: Option<String>` — semantic label: `gotcha | decision | procedure | reference`

**Steps:**
1. Add `title` and `context` columns to SQLite schema via idempotent migration (`ALTER TABLE memories ADD COLUMN IF NOT EXISTS`).
2. Add fields to `Memory` struct in `voidm-core/src/models.rs`.
3. Update `crud::add_memory` to accept and persist both fields.
4. Update `crud::get_memory` and `crud::update_memory` to return/patch both fields.
5. Add `--title <text>` and `--context <gotcha|decision|procedure|reference>` flags to `voidm add` and `voidm update`.
6. Update `voidm get` and `voidm list` human output to show title/context when present.
7. Add title-based score boost in search: exact match +2.0, prefix +1.5, substring +1.0 (applied post-RRF as a tiebreaker, not a main signal).
8. Update Neo4j backend to persist title/context as node properties.

**Inputs:** Memory model, crud.rs, add.rs, get.rs, list.rs, update.rs, search.rs, neo4j.rs, migrate.rs
**Outputs:** Both fields available on all CRUD paths, searchable, shown in output
**Success criteria:** `voidm add "..." --title "Auth flow constraint" --context decision` stores and retrieves both fields; `voidm search` returns title in results; `cargo check` passes clean

**Checkpoint:** `voidm add` + `voidm get` smoke test with both new flags.

---

### Phase 2 — F2: Graph analytics (`graph path` + `graph pagerank`)

**What:** Add two new graph subcommands backed by SQLite (no new dependencies).

**`voidm graph path <from-id> <to-id>`**
- Find shortest path between two memories via BFS on graph_edges
- Output: ordered list of memory IDs + edge types along the path
- Max depth: 10 hops (configurable)

**`voidm graph pagerank [--scope <s>] [--limit <n>] [--damping <f>]`**
- Compute PageRank over memory graph using iterative power method
- Default: damping=0.85, iterations=100, limit=20
- Output: ranked list of (memory_id, score, content_preview)
- Use: identify the most-referenced/influential memories in a scope

**Steps:**
1. Add `GraphCommands::Path(PathArgs)` and `GraphCommands::Pagerank(PagerankArgs)` variants.
2. Implement BFS path-finding in `commands/graph.rs` using existing `graph_edges` table.
3. Implement iterative PageRank in `commands/graph.rs` (pure Rust, no new dep).
4. Wire both to the Neo4j backend using native Cypher equivalents (`shortestPath`, `gds.pageRank` or manual).
5. JSON and human output for both.

**Inputs:** commands/graph.rs, main.rs, graph_edges table
**Outputs:** Two new graph subcommands
**Success criteria:** `voidm graph path <id1> <id2>` returns a path or "no path found"; `voidm graph pagerank --scope voidm` ranks memories; `cargo check` passes

**Checkpoint:** Run both commands against a DB with at least 3 linked memories.

---

### Phase 3 — F3: Text chunking layer (highest impact)

**What:** Embed at chunk level instead of memory level. Long memories are currently partially invisible in vector search because the whole text is one vector. Chunking makes every paragraph/sentence retrievable.

**Architecture:**
- New `chunks` table: `(id TEXT PK, memory_id TEXT FK, content TEXT, chunk_index INTEGER, total_chunks INTEGER, created_at TEXT)`
- New `vec_chunks` virtual table (sqlite-vec): stores chunk-level embeddings
- Memory-level embedding = average of all chunk embeddings (for backward-compat ANN)
- Search queries both `vec_chunks` (chunk ANN) and existing signals; chunk hits are aggregated to memory level before RRF
- `SearchResult` gets a new `context_chunks: Vec<String>` field — the top matching chunks that caused this result to surface

**Steps:**
1. Add `chunks` table and `vec_chunks` virtual table in `migrate.rs` (idempotent).
2. Add `ChunkingConfig` to config: `chunk_size = 600`, `chunk_min = 150`, `chunk_max = 900`, `chunk_overlap = 100`.
3. Implement `chunk_text(content: &str, config: &ChunkingConfig) -> Vec<String>` in a new `voidm-core/src/chunking.rs` module:
   - Split strategy: paragraph → sentence → word → character (fallback)
   - Respect min/max bounds and overlap
4. Update `crud::add_memory`: after embedding, chunk content → embed each chunk → store in `vec_chunks`.
5. Update `crud::delete_memory`: cascade delete chunks (FK or explicit DELETE).
6. Update `crud::update_memory`: delete old chunks → re-chunk + re-embed.
7. Add `search_chunk_ann(embedding, limit, scope, type) -> Vec<(memory_id, chunk_content, score)>` to search pipeline.
8. Update RRF fusion: include chunk ANN signal, aggregate by memory_id, attach top chunks to result.
9. Add `context_chunks: Vec<String>` to `SearchResult` (`#[serde(skip_serializing_if = "Vec::is_empty")]`).
10. Add `content_source: String` field to `SearchResult` (`"context_chunks"` or `"memory_truncate"`).
11. Add re-embed migration path: `voidm models reembed` should also rebuild chunk embeddings.
12. Update Neo4j backend to store/query chunk embeddings if Neo4j vector support is configured.

**Inputs:** crud.rs, search.rs, migrate.rs, models.rs, vector.rs, voidm-core/src/
**Outputs:** Chunk table populated on insert, search results include context_chunks
**Success criteria:**
- A 2000-char memory returns `context_chunks` showing which paragraph matched
- Short memories (< chunk_min) produce one chunk
- `voidm search "..." --json` output includes `context_chunks` array
- Existing memories without chunks still searchable (graceful fallback to memory-level ANN)
- `cargo check` passes clean

**Checkpoint:** Insert one long memory (3+ paragraphs), search for a term that only appears in the second paragraph — confirm it surfaces correctly with the right `context_chunks`.

---

### Phase 4 — F4: Unified RRF pipeline

**What:** Simplify the search pipeline. All modes (`hybrid`, `hybrid-rrf`, `semantic`, `bm25`, `fuzzy`) are routed through a single RRF function with per-signal enable/disable config. Remove the parallel code paths.

**Steps:**
1. Add `[search.signals]` to config: `vector = true`, `bm25 = true`, `fuzzy = true` (all default on).
2. Refactor `search::search()` to always call `rrf_fusion()` with a dynamic signal set derived from config + mode flag.
3. Map legacy mode names to signal presets:
   - `semantic` → vector only
   - `bm25` / `keyword` → bm25 only
   - `fuzzy` → fuzzy only
   - `hybrid` / `hybrid-rrf` → all signals enabled
4. Remove the old `if mode == "semantic" { ... } else if mode == "bm25" { ... }` branching.
5. Preserve all existing `--mode` flag values as backward-compatible aliases.

**Inputs:** search.rs, config.rs
**Outputs:** Single RRF path, all modes still work, config signals respected
**Success criteria:** All existing mode values produce identical results to before; no new errors; `cargo check` passes; search regression test across all modes

**Checkpoint:** Run `voidm search "test" --mode semantic`, `--mode bm25`, `--mode fuzzy`, `--mode hybrid` — all return results.

---

### Phase 5 (optional) — F5: `voidm-db` abstraction trait

**What:** Extract a formal `Database` trait into `voidm-core`, implement it for both backends, decouple business logic from backend-specific code.

**Status:** This is a large architectural refactor. Scope and approach should be validated in a separate session before starting. Not blocking Phases 1–4.

**Rough scope:**
- Define `trait Database: Send + Sync` with ~30 async methods covering CRUD, search, graph, stats
- Implement for `SqliteDb` and `Neo4jDb`
- Replace direct pool/connection references in `voidm-core` with `dyn Database` or `impl Database`
- Eliminates the `match config.backend` dispatch currently in multiple places

**Risk:** High — touches almost every file in voidm-core. Should be done in a dedicated branch with a full `cargo check` + smoke test plan.

---

## Step 2 — Dependencies

### Mandatory (runtime)
| Dependency | Already present | Reason |
|---|---|---|
| `sqlx` | Yes | Chunk table schema and queries |
| `sqlite-vec` | Yes | `vec_chunks` virtual table |
| `fastembed` | Yes | Chunk-level embedding generation |
| `chrono` | Yes | Chunk created_at timestamps |
| `serde` / `serde_json` | Yes | SearchResult new fields |

### No new runtime dependencies required for Phases 1–4.

### Optional (Phase 5 only)
| Dependency | Reason |
|---|---|
| `async-trait` | If not using RPITIT (Rust 1.75+); check existing usage first |

---

## Step 3 — Skills

- `voidm-memory` [HAVE] — recall prior decisions about voidm architecture before starting each phase

---

## Step 4 — MCP Tools

None required.

---

## Validation Checklist (pre-GO)

- [ ] Phase ordering agreed (1 → 2 → 3 → 4, Phase 5 deferred)
- [ ] Chunking config defaults agreed (600/150/900/100)
- [ ] `context` field enum values agreed (`gotcha | decision | procedure | reference`)
- [ ] `graph pagerank` output format agreed
- [ ] Phase 5 (abstraction trait) explicitly deferred to separate session
