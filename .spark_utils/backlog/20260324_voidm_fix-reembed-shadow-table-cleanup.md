# Task: fix-reembed-shadow-table-cleanup

## Step 1 - Implementation Plan

1. Reproduce and isolate the `reembed` failure path around stale `vec_memories_new*` artifacts.
Inputs: `crates/voidm-core/src/vector.rs`, current shared DB behavior, observed `voidm models reembed` errors.
Outputs: precise understanding of which SQLite objects survive interrupted runs and why existing cleanup misses them.
Success criteria: the failing path is tied to concrete temp/shadow table names and current cleanup scope.

Checkpoint: confirm the current cleanup logic only drops `vec_memories_new`.

2. Extend vector temp-table cleanup to remove all stale `vec_memories_new*` tables safely.
Inputs: current cleanup logic and SQLite `sqlite_master` metadata.
Outputs: helper logic that discovers all stale temp/shadow tables for the `vec_memories_new` prefix and drops them deterministically.
Success criteria: no stale `vec_memories_new*` objects remain after cleanup.

Checkpoint: targeted test demonstrates shadow-table cleanup even when the main temp table is already absent.

3. Harden `reembed_all()` so failed runs do not poison future rebuilds.
Inputs: current `reembed_all()` flow and new cleanup helper.
Outputs: rebuild path cleans before starting and on error before returning failure.
Success criteria: rerunning `voidm models reembed` after an interrupted or failed attempt is possible without manual SQLite surgery.

Checkpoint: targeted test demonstrates cleanup on injected failure.

4. Add regression tests and verify locally.
Inputs: modified vector module and current test setup.
Outputs: new tests covering stale temp/shadow table cleanup and `reembed_all()` temp-table handling.
Success criteria: focused Cargo tests pass.

Checkpoint: `cargo test` for the affected module(s) passes.

5. Verify end to end on the shared DB.
Inputs: fixed code, shared DB at `/Users/adhuy/.codex/memories/voidm/memories.db`.
Outputs: successful `voidm models reembed` and improved embedding coverage.
Success criteria: plain `voidm models reembed` succeeds and `voidm stats --json` reflects restored coverage.

Checkpoint: final CLI verification succeeds without direct SQLite intervention.

## Step 2 - Required Libraries / Dependencies

### Mandatory

- Runtime: none new
Why: the fix is within existing Rust vector/SQLite logic.
Minimal alternative: repeated manual cleanup scripts, which would leave the product bug intact.

- Dev/test/tooling: Cargo / Rust toolchain
Why: needed for regression tests and end-to-end validation.
Minimal alternative: manual runtime checks only, which are not sufficient for this bug.

### Optional

- Runtime: none
- Dev/test/tooling: none

## Step 3 - Skills

- `[HAVE]` `voidm-memory`
Why: the repo already contains durable knowledge about this failure pattern and path policy.

## Step 4 - MCP Tools

- `[HAVE]` none
Why: this is a local repository bug fix and verification task.
