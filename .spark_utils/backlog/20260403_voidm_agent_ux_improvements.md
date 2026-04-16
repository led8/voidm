# Backlog: Agent UX Improvements
**Date:** 2026-04-03
**Repo:** voidm
**Status:** Validated

---

## Step 1 — Implementation Plan

### Feature 1 — `voidm update <id>` command

**Goal:** Allow in-place update of a memory's content (and optionally type/tags/importance) without losing ID, links, or history.

1. Add `UpdateArgs` struct to CLI with `--content`, `--type`, `--tags`, `--importance` options
2. Add `update_memory(id, patch)` to `voidm-core` — partial update, preserve all graph edges
3. Re-embed updated content with current model
4. Re-run duplicate detection against updated content
5. Return updated memory with diff summary

**Inputs:** memory ID (prefix ok), optional field patches
**Outputs:** updated memory record, warning if near-duplicate detected
**Success criteria:** memory retains same ID and all edges; content + embedding updated

---

### Feature 2 — `voidm recall` startup command

**Goal:** Single command that runs the full 5-category startup protocol and returns a structured digest.

1. Accept `--scope` and optional `--task` hint
2. Internally run searches for: `architecture`, `constraints`, `decisions`, `procedures`, `user preferences`
3. Accept optional extra query terms via `--also <term>` (repeatable)
4. Deduplicate results across categories (by ID)
5. Return merged, ranked digest — JSON and human-readable modes

**Inputs:** `--scope`, optional `--task`, optional `--also`
**Outputs:** structured digest with category labels, deduped results
**Success criteria:** replaces 5–10 agent round-trips with 1; output is token-efficient

**Checkpoint:** compare token count of `recall` output vs equivalent manual 5-search output.

---

### Feature 3 — `voidm scope detect`

**Goal:** Auto-detect the current repo scope from the working directory.

1. Walk up from `$PWD` to find `.git` root
2. Extract repo name from git remote `origin` URL (fall back to directory name)
3. Return normalized scope string (lowercase, no special chars)
4. Support `--print` (print and exit) and `--export` (emit `export VOIDM_SCOPE=...` for shell eval)

**Inputs:** none (reads `$PWD` and git config)
**Outputs:** scope string
**Success criteria:** `eval $(voidm scope detect --export)` sets `$VOIDM_SCOPE` correctly in a git repo

---

### Feature 4 — Staleness / freshness scoring

**Goal:** Surface aged memories so agents can decide whether to trust or refresh them.

1. Add `last_accessed_at` timestamp column to memories table (updated on every `get` and `search` hit)
2. Add `age_days` to search result output
3. Add `--max-age <days>` filter to `search`, `recall`, `list`
4. Add `voidm stale --scope <repo> --older-than <days>` command to list candidates for review
5. Add optional time-decay weight to hybrid score: `score *= decay(age_days, half_life=90)`

**Inputs:** config `search.time_decay.enabled`, `search.time_decay.half_life_days`
**Outputs:** `age_days` in results; `voidm stale` list
**Success criteria:** memories older than threshold rank lower and are surfaced for review

**Checkpoint:** verify decay does not break hybrid-rrf score ordering for fresh memories.

---

### Feature 5 — Agent-optimized output mode (`--agent`)

**Goal:** Compact, token-minimal output for LLM agent consumption.

1. Add `--agent` global flag (implies `--json` but with trimmed schema)
2. For search results: emit only `id`, `score`, `type`, `scope`, `content` (truncated to 300 chars)
3. For `recall`: emit structured JSON with category buckets
4. For `add`/`update`: emit only `id` and `duplicate_warning` (if any)
5. Strip ANSI, tables, decorative output entirely when `--agent` is set

**Inputs:** `--agent` flag or `VOIDM_AGENT_MODE=1` env var
**Outputs:** minimal JSON payload
**Success criteria:** agent-mode output is ≤30% of equivalent `--json` payload size

---

### Feature 6 — Per-scope stats (`voidm stats --scope <repo>`)

**Goal:** Let agents understand what is known about a specific scope before deciding to run recall.

1. Add `--scope` filter to existing `voidm stats`
2. Output per-scope breakdown: memory count by type, embedding coverage %, oldest/newest memory age, top tags, edge count
3. Add `--json` support for agent consumption
4. Optionally add `coverage_score` heuristic (0–1) based on type diversity and recency

**Inputs:** `--scope`
**Outputs:** per-scope memory statistics
**Success criteria:** agent can determine "is recall worth it" from a single fast command

---

### Feature 7 — Batch add (`voidm add --batch <file>`)

**Goal:** Add a cluster of related memories atomically from a JSON file.

1. Accept `--batch <path>` pointing to a JSON array of memory objects
2. Validate all entries before inserting any (schema check)
3. Insert all in a single DB transaction
4. Auto-link entries within the batch using `auto_link_threshold`
5. Return array of created IDs with any duplicate warnings

**Input schema:** `[{ "content": "...", "type": "...", "scope": "...", "tags": "...", "importance": 5 }, ...]`
**Outputs:** array of `{ id, duplicate_warning }` objects
**Success criteria:** all-or-nothing insert; intra-batch auto-links created

---

### Feature 8 — Live trajectory ingestion (`voidm learn ingest --stdin`)

**Goal:** Allow agents to pipe trajectory JSON at runtime without staging a file.

1. Add `--stdin` flag to `voidm learn ingest` as alternative to `--from <file>`
2. Read complete JSON from stdin (object, array, or JSONL)
3. Reuse existing extraction + dry-run / write pipeline unchanged
4. Add `--quiet` mode that suppresses progress and emits only stored IDs

**Inputs:** JSON via stdin
**Outputs:** same as `--from` mode
**Success criteria:** `echo '...' | voidm learn ingest --stdin --write --scope voidm` works correctly

---

### Feature 9 — Conflict surfacing in search results

**Goal:** Warn agents inline when search results contain contradicting memories.

1. After retrieving top-k results, query `CONTRADICTS` edges among result IDs
2. If any pair found, inject a `conflicts` array into the result payload
3. Each conflict entry: `{ id_a, id_b, edge_id, note }` with content previews
4. In human-readable mode: print a warning block below results
5. In `--agent` / `--json` mode: include `conflicts` key in root response object

**Inputs:** existing search result set
**Outputs:** `conflicts` array appended to results when relevant
**Success criteria:** agent receives conflict signal without a separate `voidm conflicts list` call

---

### Feature 10 — `voidm why <id>` — memory provenance

**Goal:** Single command to explain why a memory exists and what depends on it.

1. Show creation metadata: date, source trajectory (if learning tip), scope
2. Show all inbound and outbound graph edges with neighbor summaries
3. Show `last_accessed_at` and access count
4. Show any conflicts (CONTRADICTS edges)
5. In `--agent` mode: compact JSON with edges and age

**Inputs:** memory ID
**Outputs:** provenance summary
**Success criteria:** agent can decide "trust, update, or delete" from one command

---

### Feature 11 — Consistent hierarchical scope traversal

**Goal:** Ensure `--scope myrepo` consistently includes `myrepo/auth/jwt` across all commands.

1. Audit all commands that accept `--scope`: `search`, `list`, `stats`, `recall`, `learn search`, `export`, `conflicts list`, `ontology` commands
2. Identify which use exact match vs prefix match
3. Standardize all to prefix match (i.e., `scope LIKE 'myrepo%'`)
4. Add `--exact-scope` flag for commands where exact match is useful
5. Add integration test: add memory with scope `myrepo/auth`, search with `--scope myrepo`, assert it appears

**Inputs:** scope string
**Outputs:** consistent prefix-based filtering everywhere
**Success criteria:** all commands behave identically for hierarchical scopes; regression tests pass

---

## Step 2 — Dependencies

### Mandatory / Runtime
| Dependency | Why | Already present |
|---|---|---|
| `sqlx` | DB migrations for `last_accessed_at`, batch transactions | Yes |
| `clap` | New subcommands and flags | Yes |
| `serde_json` | Batch input parsing, agent output | Yes |
| `tokio` | Async stdin reading | Yes |

### Optional / Dev
| Dependency | Why |
|---|---|
| `assert_cmd` or similar | Integration tests for scope traversal and batch add |

No new external dependencies required.

---

## Step 3 — Skills

- `general` [HAVE] — general coding rules
- `sqlalchemy` — N/A (Rust/SQLx, not Python)
- `voidm-memory` [HAVE] — reference for agent usage patterns

---

## Step 4 — MCP Tools

None required for implementation. All changes are local Rust CLI.

---

## Priority order (suggested implementation sequence)

1. Feature 3 — `voidm scope detect` (tiny, unblocks everything)
2. Feature 1 — `voidm update` (single biggest friction point)
3. Feature 5 — `--agent` output mode (enables efficient agent testing)
4. Feature 2 — `voidm recall` (high leverage, builds on 3 and 5)
5. Feature 6 — per-scope stats (fast to add, high signal value)
6. Feature 11 — scope traversal consistency (correctness fix)
7. Feature 4 — staleness scoring (schema migration required)
8. Feature 8 — stdin ingestion (small addition to existing pipeline)
9. Feature 7 — batch add (transactional add, moderate complexity)
10. Feature 9 — conflict surfacing in search (post-retrieval enrichment)
11. Feature 10 — `voidm why` (read-only aggregation, low risk)
