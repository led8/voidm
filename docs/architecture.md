# Architecture

This page maps the repository and the runtime subsystems behind the CLI.

## Workspace Layout

```text
voidm/
├── crates/
│   ├── voidm-cli/
│   ├── voidm-core/
│   └── voidm-graph/
├── docs/
├── config.example.toml
└── README.md
```

## Crate Responsibilities

### `voidm-cli`

Operator-facing layer:

- Clap command definitions
- JSON and human-readable output
- command orchestration
- config override wiring

Important entry points:

- `crates/voidm-cli/src/main.rs`
- `crates/voidm-cli/src/commands/*.rs`

### `voidm-core`

Application core:

- CRUD and list/update flows
- search and recall support
- embeddings, reranker, query expansion
- ontology, NER, NLI, quality, redaction
- config loading and database-path resolution
- learning-tip storage and consolidation

Important entry points:

- `crates/voidm-core/src/crud.rs`
- `crates/voidm-core/src/search.rs`
- `crates/voidm-core/src/config.rs`
- `crates/voidm-core/src/learning.rs`
- `crates/voidm-core/src/ontology.rs`

### `voidm-graph`

Graph subsystem:

- graph traversal helpers
- Cypher parsing and translation
- graph operations shared by the CLI and core

Important entry points:

- `crates/voidm-graph/src/lib.rs`
- `crates/voidm-graph/src/cypher/*`
- `crates/voidm-graph/src/traverse.rs`

## Storage Model

### Primary backend

SQLite is the default operational backend.

It combines:

- relational rows for memories and metadata
- FTS5 for keyword search
- `sqlite-vec` for vector search
- graph and ontology tables for relationships

### Graph model

The graph layer is local and transactional. It does not require an external graph database for normal operation.

Memory edges capture relationships such as:

- `SUPPORTS`
- `DERIVED_FROM`
- `PART_OF`
- `EXEMPLIFIES`
- `CONTRADICTS`
- `INVALIDATES`

Ontology concepts and ontology edges sit alongside the memory graph rather than replacing it.

### Learning-tip model

Trajectory-informed learning tips reuse the core memory store.

- no new top-level memory table
- no extra memory type
- structured fields live under metadata

That choice keeps learning tips compatible with export, graph traversal, and normal memory retrieval.

## Runtime Pipelines

### Add and update path

The write path is intentionally layered:

1. parse and validate CLI input
2. resolve config and database path
3. redact secrets where applicable
4. score and enrich the memory
5. store the memory
6. compute duplicate and link suggestions

Auto-tagging, link suggestions, and quality scoring are part of this path.

### Search path

The read path is composable:

1. optional query expansion
2. retrieve candidates from vector, BM25, and fuzzy signals
3. fuse the hybrid candidates
4. optionally rerank top results
5. optionally expand via graph retrieval
6. optionally append graph neighbors

Important consequences:

- hybrid scores are fusion scores, not raw cosine similarities
- reranking changes ordering after candidate retrieval
- graph retrieval and neighbor expansion are different stages

### Recall path

`voidm recall` is not just `search` with canned strings.

It:

1. queries startup buckets such as architecture and decisions
2. applies structured matching by type, content prefix, and context
3. deduplicates across buckets
4. falls back to recent scoped memories when a bucket is sparse

That makes recall more stable than query-word-only retrieval.

## Models

Local models are used for:

- embeddings
- NER
- NLI
- optional reranking
- optional query expansion

Model download and cache management are runtime concerns, not build-time dependencies.

## Secondary Backend Support

The codebase also contains Neo4j-related support and migration flows. SQLite remains the primary local-first path and the one most commands are built around.

## Documentation Boundaries

When you add or change features:

- keep `README.md` high-level
- put operational detail in `docs/`
- put learning-specific design notes in [TRAJECTORY_LEARNING_LAYER.md](TRAJECTORY_LEARNING_LAYER.md)
