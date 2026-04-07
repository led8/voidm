# voidm

Local-first persistent memory for LLM agents.

`voidm` is a single-binary CLI that gives AI agents a durable memory store: add typed memories, search them with hybrid vector+BM25+fuzzy retrieval, connect them in a knowledge graph, define ontology concepts with IS-A hierarchies, extract entities with a local NER model, detect contradictions with a local NLI model, and query with Cypher — all offline, no API keys required.

This repository comes from the upstream project [autonomous-toaster/voidm](https://github.com/autonomous-toaster/voidm).

---

## Features

- **Hybrid search** — vector (ANN), BM25, fuzzy, keyword, or combined with RRF scoring
- **Query expansion** — automatically expand queries with synonyms and related terms using local LLMs (tinyllama, phi-2); improves search recall
- **Trajectory-informed learning tips** — store reusable learnings with trigger, context, task category, priority, source outcome, and trajectory provenance
- **Trajectory ingestion** — preview or persist candidate learning tips extracted from structured coding-agent run traces
- **Learning consolidation** — cluster overlapping tips into canonical records and supersede raw variants
- **Auto-tagging** — automatically generate tags from memory content using NER + TF + type-specific rules; ~60-65% quality for suggestions
- **Auto-linking** — automatically link memories that share tags; creates RELATES_TO edges in the knowledge graph
- **Secrets redaction** — automatically detect and mask sensitive secrets (API keys, DB credentials, JWT tokens); prevents leakage into vector DB
- **Quality scoring** — automatic scoring (0.0-1.0) for all memories; filtering by quality threshold
- **Knowledge graph** — link memories with typed directed edges (SUPPORTS, DERIVED_FROM, PART_OF, …)
- **Ontology layer** — first-class concept nodes, IS-A hierarchies, INSTANCE_OF links, subsumption queries
- **Concept deduplication** — manual merge, auto-detection, prevention at creation time; batch merge operations
- **Graph visualization** — export as interactive HTML, DOT (Graphviz), JSON, CSV; force-directed layout
- **Local NER** — entity extraction via `Xenova/bert-base-NER` (ONNX, ~103 MB, downloaded once)
- **Local NLI** — relation classification + contradiction detection via `cross-encoder/nli-deberta-v3-small`
- **Conflict management** — surface CONTRADICTS edges, resolve with INVALIDATES
- **Cypher queries** — read-only graph traversal; `:Memory` and `:Concept` node labels both supported
- **Local embeddings** — [fastembed](https://github.com/Anush008/fastembed-rs) + ONNX, 7 models available
- **Model initialization** — `voidm init` pre-downloads all models for offline use (CI-friendly)
- **Auto-init** — DB created on first write, no setup step
- **Short IDs** — use any 4+ char UUID prefix instead of full IDs
- **JSON output** — every command supports `--json` for agent consumption

Implementation notes for the learning-tip layer live in [docs/TRAJECTORY_LEARNING_LAYER.md](docs/TRAJECTORY_LEARNING_LAYER.md).

---

## Installation

```bash
cd voidm
cargo install --path crates/voidm-cli
```

Or build manually:

```bash
cargo build --release
cp target/release/voidm ~/.local/bin/
```

> Requires Rust 1.94.0+. SQLite is bundled — no system dependencies.  
> ML models are downloaded on first use to `~/.cache/voidm/`.

### Model Initialization (Optional)

To pre-download models for offline use (useful for CI):

```bash
voidm init
```

This downloads the configured embedding model (default: `Xenova/all-MiniLM-L6-v2`), NER, and NLI models to `~/.cache/voidm/models/`. Total: ~300-400 MB. Idempotent—skips already-cached models.

If you change the embedding model later via `voidm config set embeddings.model <name>`, the new model will be automatically downloaded on first use.

---

## Configuration

voidm supports a **three-level configuration hierarchy** (lowest to highest priority):

1. **File** (`~/.config/voidm/config.toml`) - Persistent configuration
2. **Environment variables** (VOIDM_*) - Override file settings
3. **CLI arguments** (--flags) - Override both file and env vars

### Configuration Priority

Any configuration parameter can be set at all three levels. Higher priority levels override lower ones:

```bash
# Example: Three ways to set search mode
# 1. File (lowest priority)
# ~/.config/voidm/config.toml: search.mode = "hybrid"

# 2. Environment variable (middle priority)
$ VOIDM_SEARCH_MODE=semantic voidm search "query"

# 3. CLI argument (highest priority - wins over all)
$ voidm search "query" --search-mode keyword
```

### Environment Variables

All configuration parameters support environment variable overrides with the `VOIDM_` prefix.

**Naming pattern**: `VOIDM_SECTION_SUBSECTION_PARAM` (uppercase, underscores)

**Common env vars**:

```bash
# Database
VOIDM_DATABASE_BACKEND=neo4j            # sqlite (default) or neo4j
VOIDM_DATABASE_SQLITE_PATH=/path/to/db

# Embeddings
VOIDM_EMBEDDINGS_ENABLED=true
VOIDM_EMBEDDINGS_MODEL=Xenova/all-MiniLM-L6-v2

# Search
VOIDM_SEARCH_MODE=hybrid-rrf            # hybrid-rrf, hybrid, semantic, keyword, fuzzy, bm25
VOIDM_SEARCH_DEFAULT_LIMIT=20
VOIDM_SEARCH_MIN_SCORE=0.5

# Reranker
VOIDM_SEARCH_RERANKER_ENABLED=true
VOIDM_SEARCH_RERANKER_MODEL=bge-reranker-base
VOIDM_SEARCH_RERANKER_TOP_K=15

# Query Expansion
VOIDM_SEARCH_QE_ENABLED=true
VOIDM_SEARCH_QE_TIMEOUT_MS=5000

# Graph Retrieval
VOIDM_SEARCH_GR_ENABLED=true
VOIDM_SEARCH_GR_MAX_HOPS=2

# Insert
VOIDM_INSERT_AUTO_LINK_THRESHOLD=0.85
VOIDM_INSERT_DUPLICATE_THRESHOLD=0.95

# Redaction
VOIDM_REDACTION_ENABLED=false
```

### CLI Arguments

Most configuration settings can also be overridden via CLI flags:

```bash
# Global flags (apply to all commands)
voidm --search-mode semantic search "query"
voidm --embeddings-model my-model search "query"
voidm --reranker-enabled true search "query"

# View all available flags
voidm --help
```

### Configuration File

Edit `~/.config/voidm/config.toml` for persistent configuration:

```toml
[database]
backend = "sqlite"

[embeddings]
enabled = true
model = "Xenova/all-MiniLM-L6-v2"

[search]
mode = "hybrid-rrf"
default_limit = 10
min_score = 0.3

[search.reranker]
enabled = true
model = "ms-marco-MiniLM-L-6-v2"
apply_to_top_k = 15

[search.query_expansion]
enabled = false
model = "Xenova/gte-small"
timeout_ms = 300

[search.graph_retrieval]
enabled = true
max_concept_hops = 2

[insert]
auto_link_threshold = 0.80
duplicate_threshold = 0.95
auto_link_limit = 10

[redaction]
enabled = false
```

---

## Usage

### Add memories

```bash
voidm add "Postgres chosen for ACID guarantees" --type conceptual --scope work/acme
voidm add "DB migration takes ~5 min on production" --type semantic --scope work/acme
voidm add "Run rake db:migrate then restart puma" --type procedural --scope work/acme
```

When you add a memory, `voidm` automatically:

1. **Generates tags** from your content using NER, keyword frequency, and type-specific rules
2. **Links related memories** by finding others that share tags

#### Auto-Tagging

Every memory gets automatic tags extracted from its content — no manual tagging needed. The system uses three strategies for comprehensive coverage:

- **NER** (Named Entity Recognition) — extracts people, organizations, locations from text (~50ms)
- **TF** (Term Frequency) — finds frequent keywords filtered through English stopwords (~10ms)
- **Type-specific rules** — extracts relevant patterns based on memory type (~10ms)

Auto-tags appear alongside user-provided tags:

```bash
$ voidm add "Attended Docker conference in San Francisco" --type episodic --tags "conference"

# Output shows both user and auto-generated tags
Tags:       conference, attended, docker, san, francisco, 2024
Auto-Tags:  attended, docker, san, francisco

$ voidm get <id> --json | jq .metadata.auto_generated_tags
["attended", "docker", "san", "francisco"]
```

Quality: ~60-65% accuracy (good for suggestions, not perfect). Entity tags are 70-80% accurate; keyword tags 50-60%. All tags are deduplicated and case-insensitive.

Performance: ~75ms per memory overhead (well under 100ms budget).

#### Auto-Linking

When you add a memory, the system automatically links it to other memories that share tags. This creates RELATES_TO edges in your knowledge graph with notes explaining which tags they share:

```bash
# Add first memory with tags
voidm add "REST API design patterns" --tags "api,rest,http"

# Add second memory with overlapping tags
voidm add "SOAP protocol for APIs" --tags "api,soap,xml"

# System automatically creates a link: "Shares tags: api"
# Both memories are now connected in the graph
```

This automatic linking:
- Happens transparently (no user action needed)
- Is case-insensitive and deduplicates edges
- Uses both user-provided and auto-generated tags
- Creates bidirectional edges for discovery from either direction
- Can be configured via `insert.auto_link_limit` (default: 5 links per memory)

#### Secrets Redaction

Sensitive secrets (API keys, database credentials, JWT tokens, etc.) are automatically detected and redacted from memory content **before insertion**. This prevents accidental leakage of credentials into the vector database or search results.

```bash
# Add memory with embedded secrets (they'll be redacted automatically)
$ voidm add "API key is sk-1a2b3c4d5e6f7g8h9i0j for OpenAI access" --type procedural

# ⚠️  Redacted 1 secret from memory:
#     - 1 API key in memory.content
#
# Memory is stored with: "API key is sk-...0j for OpenAI access"

# All searches will also show the redacted version
$ voidm search "openai" 
# Result: "API key is sk-...0j for OpenAI access"
```

Redaction features:
- **Automatic detection** — API keys, database connection strings, JWT tokens, bearer tokens, emails
- **Masking strategy** — preserves first/last chars (e.g., `sk-...6f`) for context
- **Non-blocking** — redaction failures log warnings but don't prevent memory creation
- **Comprehensive scope** — redacts content, tags, metadata, and search results
- **Configurable** — enable/disable per secret type via config
- **Performance** — <100ms overhead per memory; gracefully degrades if patterns fail

What gets redacted:
- OpenAI API keys (`sk-...`)
- AWS access keys (`AKIA...`)
- Database connections (`mysql://user:pass@host/db` → `mysql://...@host/db`)
- JWT tokens (`eyJ...`)
- Bearer tokens
- Session tokens
- Email addresses (loose matching)

What does NOT get redacted (out of scope):
- Credit card numbers
- SSN/Tax IDs
- Phone numbers
- PII in general (focus is secrets only)

Configuration example:

```toml
# ~/.config/voidm/config.toml

[redaction]
enabled = true

[redaction.api_keys]
enabled = true
strategy = "mask"      # Preserve start/end: sk-...6f
prefix_length = 3
suffix_length = 2

[redaction.db_connections]
enabled = true
strategy = "mask"      # Special: hides credentials, shows host/db
```

### Search

```bash
voidm search "deployment"
voidm search "database" --scope work/acme --mode semantic
voidm search "migration" --min-score 0 --limit 20 --json
```

#### Query Expansion (enabled by default)

`voidm` automatically expands your search queries to improve recall. When you search for "Docker", the system expands to "Docker, docker-compose, Kubernetes, containerization" and searches for all variants. This finds more relevant results.

Query expansion uses small local LLMs (tinyllama by default) — no internet required after first use.

```bash
# Automatic expansion (enabled by default, uses tinyllama)
voidm search "Docker" --verbose
# Output: [query-expansion] Original: Docker
#         [query-expansion] Expanded: Docker, docker-compose, Kubernetes, containerization

# Disable expansion for specific search
voidm search "exact-match" --query-expand false

# Use different model (phi-2 for higher quality, slower)
voidm search "Docker" --query-expand-model phi-2 --verbose

# Use intent-aware expansion (guides toward a specific context)
voidm search "auth" --intent "oauth2"
# Output: [query-expansion] Original: auth
#         [query-expansion] Intent: oauth2
#         [query-expansion] Expanded: auth, OAuth2, OpenID Connect, JWT tokens...

# Intent falls back to scope if not explicitly provided (configured)
voidm search "deployment" --scope work/infra
# Uses "work/infra" as fallback intent if intent.use_scope_as_fallback = true

# Adjust timeout if needed (default 300ms)
voidm search "Docker" --query-expand-timeout 500
```

**Configuration** (in `~/.config/voidm/config.toml`):

```toml
[search.query_expansion]
enabled = true              # Enable/disable expansion globally
model = "tinyllama"         # tinyllama (default), phi-2 (highest quality), gpt2-small (fastest)
timeout_ms = 300            # Max wait for expansion (milliseconds)

[search.query_expansion.intent]
enabled = true              # Enable intent-aware expansion
use_scope_as_fallback = true # Use --scope as fallback intent
default_intent = null       # Optional default intent (e.g., "general", "technical")
```

**How it works:**
1. First search downloads the model (~300MB for tinyllama, 2.7GB for phi-2) — one-time, then cached
2. Query is expanded using appropriate template:
   - With intent: Uses intent-aware template that guides toward specific context
   - Without intent: Uses general improvement template for broader expansion
3. If no explicit intent but scope provided and `use_scope_as_fallback=true`, scope becomes intent
4. Model generates related terms via greedy decoding
5. Original query is prepended to expanded terms (enhancement, not replacement)
6. Expanded query is used for semantic search to find related content

**Performance:**
- First use: ~2-5 minutes (includes model download from HuggingFace Hub)
- Subsequent searches: <300ms per query (within timeout)

**Models:**
- `tinyllama` (1.1B, default) — balance of speed and quality
- `phi-2` (2.7B, recommended for accuracy) — highest quality expansions
- `gpt2-small` (124M, fastest) — lightweight, acceptable quality

**Notes:**
- Intent helps focus expansion on domain-specific terminology (e.g., "oauth2" for auth concepts)
- Expanded query includes the original term to ensure fallback matching works
- If expansion fails or times out, the original query is used
- All model inference is local; no data leaves your machine
- Intent parameter is optional; search works fine without it

#### Reranking (Optional, Disabled by Default)

For high-recall searches, enable reranking to improve result ordering. Reranking uses a cross-encoder model to re-score results based on relevance to the query.

```bash
# Enable reranking
voidm search "docker" --reranker true

# Or disable if latency matters more than ranking precision
voidm search "docker" --reranker false  # Default
```

**Configuration** (in `~/.config/voidm/config.toml`):

```toml
[search.reranker]
enabled = false                    # Disabled by default (adds ~1s latency when enabled)
model = "ms-marco-MiniLM-L-6-v2"  # RECOMMENDED: 100MB, ~1s latency, best balance
apply_to_top_k = 15               # Rerank top-15 results

# Passage extraction: Find sentences containing query terms
[search.reranker.passage_extraction]
enabled = true                    # Intelligent passage extraction (enabled by default)
context_sentences = 1             # Include ±1 sentence around match for context
fallback_length = 400             # If no match found, use first 400 chars
min_passage_length = 50           # Don't return passages shorter than this
```

**How Passage Extraction Works**:
Instead of passing full documents to the reranker (which is trained on short passages), passage extraction:
1. Finds sentences containing query terms
2. Extracts those sentences with surrounding context
3. Passes only the relevant passage to the reranker

This ensures high-quality reranking even on very long documents.

**Supported Models** (all ONNX-compatible, verified working):

**RECOMMENDED**:
- `ms-marco-MiniLM-L-6-v2` (100MB, ~1s latency)
  - Best balance of speed and quality
  - Safe default choice
  - Recommended for most use cases

**Fast Alternative**:
- `ms-marco-TinyBERT-L-2` (11MB, 0.6s latency)
  - Lightest model, fastest inference
  - Good quality-to-speed ratio
  - Best for latency-critical applications

**High Quality** (slower):
- `mmarco-mMiniLMv2-L12-H384-v1` (110MB, ~10s latency)
  - Better quality than ms-marco
  - Slower but still acceptable
  - For quality-focused applications

**Best Accuracy** (slowest):
- `qnli-distilroberta-base` (250MB, ~30s latency)
  - Highest accuracy
  - Unacceptably slow for interactive use
  - Only for offline batch processing

**When to Use Reranking**:
- Precision-focused searches where result ordering matters
- When you need top-k results to be most relevant
- Use `ms-marco-MiniLM-L-6-v2` as default (recommended)
- Keep disabled by default for speed-critical applications

**Note**: Reranking works on the initial search results. For low initial scores, improve query expansion instead.


#### Graph-Aware Retrieval (Tag & Concept Matching)

Automatically expand search results with related memories via shared tags and concept hierarchies. This improves recall without sacrificing precision.

```bash
# Tag-based retrieval (finds memories with shared tags)
voidm search "Docker" --verbose
# Output: [search] Direct results: 1
#         [graph] Tag-based: 2 related memories found
#         [graph] Concept-based: 1 related memory found
#         Total: 4 results

# Disable graph-aware retrieval if needed
voidm search "Docker" --no-graph-retrieval
```

**How it works:**

1. **Tag-based retrieval**: Finds memories with tag overlap
   - Minimum shared tags: 3 (configurable)
   - Minimum overlap %: 50% (configurable)
   - Score decay: 0.7x per tag-related result
   - Example: Query tags `["docker", "container", "linux"]` matches memory with tags `["docker", "container", "devops"]` (2/3 = 67% overlap)

2. **Concept-based retrieval**: Traverses ontology to find related memories
   - Bidirectional IS-A traversal (parents + children)
   - Max hops: 2 (default, prevents exponential expansion)
   - Distance-based scoring: score = 0.7^hops (1-hop=0.7, 2-hop=0.49)
   - Example: Memory linked to concept "Docker" → finds memories linked to "Containerization" (1-hop) and "DevOps" (2-hop)

**Performance:**
- Tag overlap: <200ms for 100K dataset
- Concept traversal: <300ms for 100K dataset
- Combined: <500ms for both functions

**Configuration** (in `~/.config/voidm/config.toml`):

```toml
[search.graph_retrieval]
enabled = true                  # Enable/disable graph-aware retrieval (default: true)
max_concept_hops = 2            # Global default: max concept traversal depth (default: 2)

[search.graph_retrieval.tags]
enabled = true                  # Enable tag-based retrieval
min_overlap = 3                 # Minimum shared tags (default: 3)
min_percentage = 50.0           # Minimum overlap % (default: 50%)
decay_factor = 0.7              # Score multiplier (default: 0.7)
limit = 5                       # Max results per direct result (default: 5)

[search.graph_retrieval.concepts]
enabled = true                  # Enable concept-based retrieval
max_hops = 2                    # Optional: override global max_concept_hops
decay_factor = 0.7              # Score multiplier per hop (default: 0.7)
limit = 3                       # Max results per direct result (default: 3)
```

**Tuning Performance:**
- `max_concept_hops=1`: Conservative (fewer results, faster)
- `max_concept_hops=2`: Balanced, recommended
- `max_concept_hops=3`: Aggressive (more results, slower)
- `max_concept_hops≥4`: Not recommended (exponential growth)

**When to Disable:**
- Latency-critical applications (use `--no-graph-retrieval`)
- When exact matches are important and related results add noise
- Sparse knowledge graphs (few concept connections)

**Examples:**

```bash
# Find Docker-related memories via tags and concepts
voidm search "Docker container" --verbose

# Disable for speed
voidm search "Docker" --no-graph-retrieval

# Use with other options
voidm search "auth" --intent "oauth2" --scope work/auth --verbose
```

Filter by quality score (0.0-1.0, added automatically):

```bash
# Only high-quality memories (0.8+)
voidm search "pattern" --min-quality 0.8 --limit 10

# All memories regardless of quality
voidm search "pattern" --min-quality 0.0
```

Quality scores reflect genericity, abstraction, temporal independence, and substance. Use `--min-quality` to skip low-confidence memories.

### Link memories together

```bash
voidm link <runbook-id> DERIVED_FROM <migration-fact-id>
voidm link <decision-id> SUPPORTS <fact-id>
voidm link <id1> RELATES_TO <id2> --note "both affect deploy order"
```

When you add a memory, `voidm` returns `suggested_links` (similarity ≥ 0.7) and flags `duplicate_warning` (similarity ≥ 0.95).

### Explore the graph

```bash
voidm graph neighbors <id> --depth 2
voidm graph pagerank --top 10
voidm graph cypher "MATCH (a:Memory)-[:SUPPORTS]->(b:Memory) RETURN a.memory_id, b.memory_id LIMIT 20"
voidm graph cypher "MATCH (c:Concept) WHERE c.name = 'AuthService' RETURN c.id, c.description"
```

Supported Cypher clauses: `MATCH`, `WHERE`, `RETURN`, `ORDER BY`, `LIMIT`, `WITH`. Write operations are rejected. Both `:Memory` and `:Concept` node labels are supported.

### Export and visualize the graph

```bash
# Export as interactive HTML (force-directed, searchable, filterable)
voidm graph export --format html > graph.html
open graph.html

# Export as DOT (Graphviz format)
voidm graph export --format dot > graph.dot
dot -Tsvg graph.dot -o graph.svg

# Export as JSON (for custom tools)
voidm graph export --format json > graph.json

# Export as CSV (edge list, for spreadsheets)
voidm graph export --format csv > edges.csv
```

---

## Ontology

The ontology layer adds first-class concept nodes — classes, categories, architectural components — that memories can be attached to as instances.

### Define concepts

```bash
# Create a concept class
voidm ontology concept add "AuthService" --description "Handles JWT + OAuth2 flows" --scope work/acme

# List concepts
voidm ontology concept list --scope work/acme

# Get a concept with its instances, subclasses, and superclasses
voidm ontology concept get <id>
```

### IS-A hierarchies

Concepts can form class hierarchies via IS_A edges. Subsumption is computed with recursive CTEs — querying a parent returns all instances of all subclasses too.

```bash
voidm ontology link <child-concept-id> --from-kind concept \
  IS_A <parent-concept-id> --to-kind concept
```

### Link memories to concepts

```bash
# Make a memory an instance of a concept class
voidm ontology link <memory-id> --from-kind memory \
  INSTANCE_OF <concept-id> --to-kind concept

# Query all instances (transitive — includes subclass instances)
voidm ontology concept get <concept-id>
```

Ontology edge types: `IS_A`, `INSTANCE_OF`, `HAS_PROPERTY`, `CONTRADICTS`, `INVALIDATES`.

### Batch NER enrichment

Extract named entities from all stored memories and auto-link them to matching concepts:

```bash
voidm ontology enrich-memories              # process all unprocessed memories
voidm ontology enrich-memories --scope work/acme --add   # also create missing concepts
voidm ontology enrich-memories --force      # reprocess already-processed memories
voidm ontology enrich-memories --dry-run    # preview without writing
voidm ontology enrich-memories --limit 50   # cap at N memories
```

The NER model (`Xenova/bert-base-NER`) is downloaded once to `~/.cache/voidm/ner/`. A tracking table (`ontology_ner_processed`) prevents redundant re-runs.

### Extract entities from a single memory

```bash
voidm ontology extract <memory-id>
voidm ontology extract <memory-id> --add --min-score 0.8
```

### NLI-based enrichment

Use a local NLI model to classify relations between two texts and detect contradictions:

```bash
voidm ontology enrich <text1> <text2>
voidm ontology concept add "..." --enrich   # enrich at creation time
```

The NLI model (`cross-encoder/nli-deberta-v3-small`) is downloaded once to `~/.cache/voidm/nli/`. Contradiction threshold: 0.80.

### Concept Deduplication

voidm detects and merges duplicate concepts in three ways:

#### 1. Manual Merge
```bash
voidm ontology concept merge <source-id> <target-id>
# Retargets all INSTANCE_OF and IS_A edges from source to target, then deletes source
```

#### 2. Auto-Detection
```bash
voidm ontology concept find-merge-candidates --threshold 0.90
# Lists concept pairs with > 90% name similarity

voidm ontology concept find-merge-candidates --threshold 0.90 --output candidates.json
# Save to file for batch processing
```

#### 3. Batch Merge (Preview & Execute)
```bash
# Preview impact without changing anything
voidm ontology concept merge-batch --from candidates.json

# Execute the merges
voidm ontology concept merge-batch --from candidates.json --execute

# View merge history
voidm ontology concept merge-history

# Rollback a merge if needed
voidm ontology concept rollback-merge <merge-id>
```

#### 4. Prevention at Creation Time
When adding a concept, similar existing concepts are checked and reported:
```bash
voidm ontology concept add "DatabaseConnection"
# Warning: Similar concepts found (consider merging):
#   - Database (87% similar, 5 edges)
#   - DBConnection (94% similar, 3 edges)
```

---

## Conflict Management

Contradicting concepts surface as `CONTRADICTS` edges. Review and resolve them with:

```bash
# List all unresolved conflicts
voidm conflicts list
voidm conflicts list --scope work/acme

# Resolve: keep the winner, mark the loser as [SUPERSEDED]
voidm conflicts resolve <edge-id> --keep <winning-concept-id>
```

Resolving replaces the `CONTRADICTS` edge with an `INVALIDATES` edge (winner → loser) and prepends `[SUPERSEDED]` to the loser's description.

---

## CLI Reference

### Memory

| Command | Description |
|---------|-------------|
| `voidm add` | Add a memory. Returns `suggested_links` and `duplicate_warning`. |
| `voidm learn add` | Add a structured trajectory-informed learning tip. |
| `voidm learn ingest --from <file>` | Extract candidate learning tips from a trajectory file. Preview by default; use `--write` to persist. |
| `voidm learn consolidate` | Cluster overlapping learning tips and optionally write canonical records. |
| `voidm learn search <query>` | Search only structured learning tips. |
| `voidm learn get <id>` | Retrieve a structured learning tip by ID or short prefix. |
| `voidm get <id>` | Retrieve a memory by ID or short prefix. |
| `voidm delete <id>` | Delete a memory. |
| `voidm list` | List memories, filtered by scope or type. |
| `voidm search <query>` | Hybrid search. Modes: `hybrid`, `semantic`, `bm25`, `fuzzy`, `keyword`. |
| `voidm link <from> <EDGE> <to>` | Create a graph edge. `RELATES_TO` requires `--note`. |
| `voidm unlink <from> <EDGE> <to>` | Remove a graph edge. |
| `voidm export` | Export memories as JSON. |

### Graph

| Command | Description |
|---------|-------------|
| `voidm graph neighbors <id>` | N-hop neighbors (`--depth`, default 1). |
| `voidm graph pagerank` | Rank memories + concepts by graph centrality. |
| `voidm graph cypher "<query>"` | Read-only Cypher. `:Memory` and `:Concept` labels supported. |
| `voidm graph path <from> <to>` | Shortest path between two memories. |
| `voidm graph stats` | Edge counts by type. |
| `voidm graph export --format <fmt>` | Export graph. Formats: `html` (interactive), `dot` (Graphviz), `json`, `csv`. |

### Ontology

| Command | Description |
|---------|-------------|
| `voidm ontology concept add <name>` | Create a concept. `--description`, `--scope`. |
| `voidm ontology concept get <id>` | Get concept with instances, subclasses, superclasses. |
| `voidm ontology concept list` | List concepts. `--scope`. |
| `voidm ontology concept delete <id>` | Delete a concept. |
| `voidm ontology link <from> <EDGE> <to>` | Create ontology edge. `--from-kind`, `--to-kind` (memory\|concept). |
| `voidm ontology unlink <from> <EDGE> <to>` | Remove ontology edge. |
| `voidm ontology edges <id>` | List edges for a concept. |
| `voidm ontology hierarchy <id>` | Full IS-A hierarchy for a concept. |
| `voidm ontology instances <id>` | All instances (transitive). |
| `voidm ontology extract <id>` | Extract NER entities from a memory. `--add`, `--min-score`. |
| `voidm ontology enrich-memories` | Batch NER enrichment. `--scope`, `--add`, `--force`, `--dry-run`, `--limit`. |
| `voidm ontology enrich <text1> <text2>` | NLI relation classification between two texts. |
| `voidm ontology concept merge <src> <tgt>` | Manually merge source concept into target. |
| `voidm ontology concept find-merge-candidates` | Auto-detect duplicates. `--threshold` (0.0-1.0), `--output` (JSON file). |
| `voidm ontology concept merge-batch --from <file>` | Preview or execute batch merge. Add `--execute` to apply. |
| `voidm ontology concept merge-history` | View merge audit trail. Filter: `--batch`, `--status`. |
| `voidm ontology concept rollback-merge <id>` | Undo a merge operation. |
| `voidm ontology benchmark` | NLI model benchmark on built-in test pairs. |

### Conflicts

| Command | Description |
|---------|-------------|
| `voidm conflicts list` | List unresolved CONTRADICTS edges. `--scope`. |
| `voidm conflicts resolve <edge-id>` | Resolve conflict. `--keep <winning-id>`. |

### System

| Command | Description |
|---------|-------------|
| `voidm models list` | List available embedding models. |
| `voidm models reembed` | Re-embed all memories with current model. |
| `voidm init` | Pre-download all models to `~/.cache/voidm/models/`. Idempotent. |
| `voidm config show/set` | Show or update config. |
| `voidm info` | DB path, config path, model, search defaults. |
| `voidm stats` | Memory counts, embedding coverage, top tags, DB size. |
| `voidm instructions` | Print agent usage guide. |

Use `--json` on any command for machine-readable output. Use `--help` for full flag reference.

---

## Architecture

```
voidm/
├── crates/
│   ├── voidm-core/    # DB, embeddings, CRUD, hybrid search, ontology, NER, NLI, config
│   ├── voidm-graph/   # EAV graph schema, Cypher parser + translator (:Memory + :Concept)
│   └── voidm-cli/     # Clap CLI, JSON/table output, all subcommands
└── migrations/        # SQLite schema (sqlx)
```

- **Storage:** platform-local data dir by default, for example `~/Library/Application Support/voidm/memories.db` on macOS or `~/.local/share/voidm/memories.db` on Linux
- **Shared terminal + Codex setup:** set `[database.sqlite] path = "~/.codex/memories/voidm/memories.db"` in `~/.config/voidm/config.toml` if you want both environments to use one DB
- **Codex sandbox fallback:** when no explicit DB path is set, sandboxed runs fall back to `~/.codex/memories/voidm/memories.db` so agent writes stay inside writable roots
- **Config:** `~/.config/voidm/config.toml`
- **ML cache:** `~/.cache/voidm/` (NER + NLI ONNX models, downloaded on first use)
- **Search pipeline:** Vector ANN (sqlite-vec) + BM25 (FTS5) + fuzzy (strsim) → RRF merge
- **Graph:** Pure SQLx EAV schema — no external graph DB, fully transactional
- **Ontology:** `ontology_concepts` + `ontology_edges` tables; recursive CTE subsumption
- **NER:** `Xenova/bert-base-NER` quantized ONNX (~103 MB); subword span stitching for CamelCase
- **NLI:** `cross-encoder/nli-deberta-v3-small` ONNX; contradiction threshold 0.80

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Not found |
| `2` | Error (bad args, write Cypher rejected, missing required field) |

---

## Acknowledgements

Thanks to the original author of [autonomous-toaster/voidm](https://github.com/autonomous-toaster/voidm) for building and sharing the upstream project.

Inspired by [byteowlz/mmry](https://github.com/byteowlz/mmry) and [colliery-io/graphqlite](https://github.com/colliery-io/graphqlite).

RRF (Reciprocal Rank Fusion) signal fusion approach informed by [QMD project](https://github.com/tobil/qmd) architecture and research.

Built with ❤️ and [pi-coding-agent](https://github.com/badlogic/pi-mono).

---

## License

MIT — see [LICENSE](LICENSE).
