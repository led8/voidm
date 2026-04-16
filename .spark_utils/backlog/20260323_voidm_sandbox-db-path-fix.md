# Task: sandbox-db-path-fix

## Step 1 - Implementation Plan

1. Establish the current sandbox-related DB path behavior.
Inputs: current path resolution in `crates/voidm-core/src/config.rs`, DB open path in `crates/voidm-core/src/db/sqlite.rs`, CLI diagnostics in `crates/voidm-cli/src/commands/info.rs`.
Outputs: confirmed precedence rules and exact sandbox failure mode.
Success criteria: clear separation between explicit DB configuration and implicit defaults.

Checkpoint: `voidm info --json` and a sandboxed `voidm add` reproduction are understood before edits.

2. Implement sandbox-aware fallback for implicit default DB paths only.
Inputs: current config/path logic and Codex sandbox environment markers.
Outputs: resolved DB path uses a writable Codex location when running in sandbox and no explicit DB path was chosen.
Success criteria: `--db`, `VOIDM_DB`, and configured SQLite paths continue to win over any sandbox fallback.

Checkpoint: local path-resolution tests cover default and explicit override cases.

3. Improve diagnostics for active DB source and sandbox write failures.
Inputs: current `info` output and observed readonly-database failure.
Outputs: source reporting reflects actual precedence, and error text points users to actionable overrides.
Success criteria: `voidm info` no longer labels default paths as config-driven, and failures mention `VOIDM_DB` / `database.sqlite_path`.

Checkpoint: targeted tests or command output demonstrate corrected reporting.

4. Document the behavior and verify end to end.
Inputs: runtime behavior after changes.
Outputs: concise README note for Codex sandbox usage and verification run results.
Success criteria: sandboxed write succeeds without manual override when no explicit DB path is configured.

Checkpoint: targeted Cargo tests pass and a sandboxed `voidm add` succeeds.

## Step 2 - Required Libraries / Dependencies

### Mandatory

- Runtime: none new
Why: the fix only needs existing stdlib, `dirs`, `clap`, and current SQLite handling.
Minimal alternative: agent-only environment overrides, which would leave `voidm` behavior inconsistent.

- Dev/test/tooling: none new
Why: existing Cargo tests are sufficient for regression coverage.
Minimal alternative: manual verification only.

### Optional

- Runtime: none
- Dev/test/tooling: none

## Step 3 - Skills

- `[HAVE]` `voidm-memory`
Why: repository continuity and durable project knowledge policy apply here, even though normal recall is limited by the current sandbox-path bug.

## Step 4 - MCP Tools

- `[HAVE]` none
Why: this task is fully local to the repository and shell environment.
