# Neo4j as a voidm Backend — Ideas & Assumptions

**Date:** 2026-04-06
**Status:** Idea — not validated, not in backlog

---

## What is Neo4j

Neo4j is a native graph database. Instead of rows and tables, the primary data structures are **nodes** (entities) and **relationships** (typed, directional edges between nodes). Both nodes and relationships can carry properties.

It ships with Cypher — a declarative graph query language designed to express graph patterns naturally (`MATCH (a)-[:SUPPORTS*1..3]->(b) RETURN a, b`).

voidm already has a Neo4j backend in `crates/voidm-core/src/db/neo4j.rs`. It works today. This document is about whether and when investing in it further makes sense.

---

## What Neo4j changes compared to SQLite

### 1. Graph traversal is native

In SQLite, multi-hop traversal requires recursive CTEs and JOINs across `graph_edges` + `graph_nodes`. It works but the cost grows non-linearly with hop depth and graph size. Depth-3 queries on a large graph are noticeably slow.

In Neo4j, traversal is the primary operation the engine is optimised for. Depth-10 traversals on millions of nodes are routine.

For voidm, this matters when:
- Running `voidm graph path` (shortest path between two memories)
- Running `voidm graph pagerank` (influence scoring across all memories)
- Using `--include-neighbors` at depth > 2
- Exploring ontology hierarchies with many levels

### 2. Native graph algorithms

Neo4j Graph Data Science (GDS) library provides:
- PageRank, Betweenness Centrality, Closeness Centrality
- Community detection (Louvain, Label Propagation)
- Similarity algorithms (Jaccard, Cosine, Overlap)
- Link prediction
- Node embeddings (FastRP, Node2Vec, GraphSAGE)

In SQLite, these require custom Rust implementations. Neo4j makes them one Cypher call.

### 3. Visual graph exploration

Neo4j Browser (web UI on port 7474) renders the memory graph visually. You can:
- See clusters of related memories
- Explore SUPPORTS / CONTRADICTS / DERIVED_FROM chains
- Identify isolated memories (no edges)
- Spot over-connected hub memories

This is useful for periodic memory health reviews — seeing the graph visually reveals structure that text queries do not.

### 4. Concurrent access

SQLite serialises writes (WAL mode helps reads but writes still queue). Neo4j handles multiple concurrent readers and writers via MVCC. Relevant if:
- Multiple agent processes share the same memory store simultaneously
- A team shares one voidm instance across machines

### 5. Schema flexibility for future ontology work

Neo4j's property graph model makes adding new node types and relationship types trivial — no migrations, no schema changes. If voidm's ontology layer (concepts, IS_A, INSTANCE_OF hierarchies) grows significantly, Neo4j handles complexity better than a flat relational schema.

---

## What does NOT change

- All voidm CLI commands work identically (`voidm add`, `voidm search`, `voidm recall` etc.)
- Same memory model, same edge types, same scopes, same search quality
- RRF, reranking, graph retrieval all work on both backends
- Migration is bidirectional: `voidm migrate sqlite-to-neo4j` / `voidm migrate neo4j-to-sqlite`

---

## When Neo4j makes sense for voidm

| Use case | SQLite | Neo4j |
|---|---|---|
| Single local coding agent | Best fit | Overkill |
| Shared team memory store | Possible but single-writer | Better fit |
| Large memory corpus (10k+ memories) | Acceptable | Better traversal |
| Visual graph exploration | Not available | Native (Neo4j Browser) |
| Complex graph analytics (PageRank, paths) | Custom Rust code | Native Cypher |
| Offline / no-server usage | Native | Not possible |
| Zero-config setup | Native | Requires Docker or install |

---

## Operational cost

Neo4j requires a running server. Minimum viable setup:

```bash
# Docker (simplest)
docker run -d \
  --name voidm-neo4j \
  -p 7474:7474 -p 7687:7687 \
  -e NEO4J_AUTH=neo4j/voidmpassword \
  neo4j:5

# voidm config
[database]
backend = "neo4j"
[database.neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "voidmpassword"
```

Alternatively: Neo4j Desktop (free, GUI) or Neo4j Aura (cloud, free tier available).

---

## Known gaps in current Neo4j backend

The current `db/neo4j.rs` is functional but has some known gaps vs the SQLite backend:

1. **No chunk-level vector search** — if chunking (Phase 3 of Option A) is implemented, Neo4j will need a vector search strategy. Neo4j 5.x+ supports native vector indexes but this requires GDS or Neo4j Enterprise for full ANN support.
2. **Limited test coverage** — SQLite backend has more integration test coverage.
3. **voidm-2 architecture** — the voidm-2 `voidm-neo4j` crate is a cleaner, fully-isolated implementation of the Database trait. If the abstraction trait (Phase 5 of Option A) is ever pursued, the voidm-2 Neo4j implementation is a strong reference.

---

## Potential improvements worth exploring

1. **Auto-suggest Neo4j when corpus exceeds N memories** — warn user that graph traversal may benefit from a server backend at scale.
2. **Neo4j Aura integration** — cloud-hosted option for teams without local Docker.
3. **Graph export to Neo4j** — one-way "push" from SQLite to a read-only Neo4j instance for visualization only, without switching backends.
4. **PageRank via Neo4j GDS** — when the GDS library is available, delegate `graph pagerank` to Neo4j natively instead of custom Rust.
5. **Community detection** — use Louvain or Label Propagation to cluster memories automatically, then surface cluster labels as suggested scopes or tags.

---

## Assumptions (not validated)

- Neo4j 5.x vector index is sufficient for chunk-level ANN search without GDS Enterprise
- Teams sharing voidm memory would want a hosted Neo4j instance, not a local Docker container
- The visual graph exploration use case is valuable enough to document and surface in `voidm instructions`
- `graph path` and `graph pagerank` (Phase 2 of Option A) are worth implementing in SQLite first; Neo4j delegation can come later

---

## Related

- Backlog: `20260406_voidm_option_a_voidm2_port.md` (Phase 2 adds graph path + pagerank in SQLite)
- Code: `crates/voidm-core/src/db/neo4j.rs`
- Config: `[database.neo4j]` section in `~/.config/voidm/config.toml`
- Reference: voidm-2 `crates/voidm-neo4j/` for a clean Neo4j implementation to learn from
