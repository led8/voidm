# Task: single-memory-db-path

## Step 1 - Implementation Plan

1. Clone the current active shared DB into a workspace-local SQLite path that Codex can write.
Inputs: current active DB `/Users/adhuy/code/led8/ai/spark/voidm-memories-db/memories.db`, workspace-local target path `/Users/adhuy/code/led8/ai/spark/voidm/.spark_utils/data/voidm-memories-db/memories.db`, SQLite file-copy or backup tooling.
Outputs: a valid copy of the active DB exists under `.spark_utils/data/voidm-memories-db/`.
Success criteria: the workspace-local DB opens successfully and matches the source memory count before any further edits.

Checkpoint: verify resolved path before any data migration.

2. Repoint the persistent local `voidm` config to the workspace-local path.
Inputs: approved target path `/Users/adhuy/code/led8/ai/spark/voidm/.spark_utils/data/voidm-memories-db/memories.db`, current path precedence in `crates/voidm-core/src/config.rs`, local `voidm config` support.
Outputs: persistent user config resolves the DB path to the workspace-local database.
Success criteria: `voidm info` resolves to the `.spark_utils/data` path without requiring CLI overrides.

Checkpoint: confirm `voidm info` reports the workspace-local path.

3. Refresh durable repo memory so the stored path preference matches the final workspace-local location.
Inputs: the workspace-local DB, the stale external-path memories, and the final approved shared path.
Outputs: stale external-path memories are removed and the workspace-local path preference is stored durably in the active DB.
Success criteria: a repo-scoped search or direct get resolves the workspace-local path preference rather than the superseded sibling-path or `.codex` preferences.

Checkpoint: confirm the new preference memory IDs exist in the active DB.

4. Verify the new path works for both local terminal usage and plain sandboxed Codex commands.
Inputs: persisted config, workspace-local DB, and plain sandboxed `voidm` commands.
Outputs: `voidm info`, `get`, and `stats` all operate against the same workspace-local DB path by default.
Success criteria: the new path is the persistent default for future terminal runs and Codex sandbox runs.

Checkpoint: confirm `voidm info` reports the new path and plain sandboxed commands succeed.

## Step 2 - Required Libraries / Dependencies

### Mandatory

- Runtime: none new
Why: the fix uses existing `voidm` config handling and existing SQLite databases.
Minimal alternative: repo code changes, which are unnecessary for a user-specific single-path setup.

- Dev/test/tooling: `voidm` CLI
Why: needed to pin the persistent DB path and verify runtime resolution.
Minimal alternative: manual config-file editing.

- Dev/test/tooling: `sqlite3`
Why: needed to inspect the copied database and verify counts and schema during the relocation.
Minimal alternative: `voidm stats`, which is less direct when validating raw SQLite state.

### Optional

- Runtime: none
- Dev/test/tooling: none

## Step 3 - Skills

- `[HAVE]` `voidm-memory`
Why: repo continuity and durable decision tracking apply here.

## Step 4 - MCP Tools

- `[HAVE]` none
Why: the task is local to the repository, local config, and local SQLite files.
