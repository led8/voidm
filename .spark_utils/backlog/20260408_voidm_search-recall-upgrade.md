# Backlog: Search Recall Upgrade
**Date:** 2026-04-08
**Repo:** voidm
**Status:** Validated

---

## Step 1 — Implementation Plan

### Feature 1 — Hybrid search threshold calibration

**Goal:** Make default hybrid search return real matches under the current RRF-based scoring model.

1. Inspect how hybrid and semantic modes currently diverge in signal selection and thresholding
2. Update hybrid threshold behavior so valid RRF-ranked matches are not discarded by the legacy `0.3` cutoff
3. Keep explicit `--min-score` overrides working as-is for callers that want stricter filtering
4. Verify scoped hybrid search returns known memories without requiring `--min-score 0`

**Inputs:** current hybrid search pipeline, current default config, real CLI reproductions
**Outputs:** corrected hybrid default threshold behavior
**Success criteria:** `voidm search "mfst" --scope mfst-agent` returns relevant results with default settings

---

### Feature 2 — Recall retrieval hardening

**Goal:** Make `voidm recall` return useful scoped context instead of an empty digest when category terms are weak.

1. Replace or augment category-word-only retrieval with type- and content-aware bucket assignment
2. Ensure recall is not blocked by hybrid default thresholding
3. Deduplicate results across buckets while preserving category usefulness
4. Verify recall returns non-empty categorized output for known scopes in the current database

**Inputs:** current `recall` implementation, existing memory conventions such as `Architecture:`, `Decision:`, `Constraint:`, `Procedure:`, `Preference:`
**Outputs:** more reliable `recall` digest generation
**Success criteria:** `voidm recall --scope mfst-agent` and `voidm recall --scope voidm` both return useful categorized output

**Checkpoint:** compare CLI output before and after the patch for `voidm` and `mfst-agent` scopes.

---

### Feature 3 — Regression coverage

**Goal:** Add tests that lock in the fixed behavior for both search and recall.

1. Add focused tests around hybrid threshold behavior and/or bucket assignment helpers
2. Cover the score-scale mismatch that caused valid hybrid results to disappear
3. Keep tests local and deterministic without requiring external services

**Inputs:** existing Rust test utilities and current search/recall code
**Outputs:** targeted tests for the fixed behavior
**Success criteria:** new tests fail on the buggy logic and pass after the fix

---

### Feature 4 — Documentation alignment

**Goal:** Keep user-facing docs aligned with the actual search defaults and troubleshooting path.

1. Update README search configuration guidance if defaults or semantics changed
2. Document practical guidance for hybrid versus semantic retrieval where needed

**Inputs:** README search section and final implementation
**Outputs:** accurate high-level docs
**Success criteria:** README no longer suggests misleading hybrid threshold behavior

---

## Step 2 — Dependencies

### Mandatory / Runtime
| Dependency | Why | Already present |
|---|---|---|
| `clap` | CLI argument handling remains unchanged | Yes |
| `sqlx` | Search and memory retrieval continue to use SQLite-backed queries | Yes |
| `serde_json` | Existing JSON output paths for search and recall | Yes |

### Mandatory / Dev/Test/Tooling
| Dependency | Why | Already present |
|---|---|---|
| Rust toolchain (`cargo`) | Compile and run targeted tests/checks | Yes |

### Optional
No new dependencies planned.

---

## Step 3 — Skills

- `voidm-memory` [HAVE] — continuity guidance for repo-local persistent memory usage; source inspection remains primary because retrieval is the surface under repair
- `general` [HAVE] — standard implementation discipline for local code changes

---

## Step 4 — MCP Tools

None required. Implementation and verification are local to the Rust workspace and local `voidm` CLI.

---

## Suggested Execution Order

1. Create backlog/todo tracker
2. Fix hybrid threshold behavior
3. Harden recall bucketing and fallback behavior
4. Add regression tests
5. Update README if needed
6. Run compile and CLI verification
