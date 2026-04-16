# Task: Remove the MCP server from voidm

## Step 1 - Implementation Plan

1. Inventory and lock the MCP removal surface.
Inputs: current CLI commands, `commands/mcp.rs`, `voidm-cli` dependencies, README/docs references, MCP-specific tests.
Outputs: confirmed file list and exact removal scope.
Success criteria: no MCP server entry point or MCP-only dependency is missed.

2. Remove the CLI entry point and command wiring.
Inputs: `crates/voidm-cli/src/main.rs`, `crates/voidm-cli/src/commands/mod.rs`, `crates/voidm-cli/src/commands/mcp.rs`.
Outputs: no `Mcp` subcommand, no `commands::mcp`, no MCP server module.
Success criteria: `voidm --help` no longer exposes `mcp`, and the crate compiles without the module.

3. Remove MCP-only dependencies and imports.
Inputs: `crates/voidm-cli/Cargo.toml`, any MCP-related imports and usages.
Outputs: `rmcp` removed, unused imports cleaned up.
Success criteria: dependency graph no longer includes MCP server runtime pieces and there are no dead imports left from the removal.

4. Remove MCP-specific tests and assertions.
Inputs: the `commands::mcp::*` test surface.
Outputs: MCP tests deleted or removed with the module.
Success criteria: test suite no longer references MCP symbols.

5. Clean user-facing docs and learning-layer docs.
Inputs: `README.md`, `docs/TRAJECTORY_LEARNING_LAYER.md`.
Outputs: no README MCP section, no MCP tool references in learning docs.
Success criteria: docs match the product state after removal.

6. Sweep residual MCP-server-related references.
Inputs: repo-wide search for `mcp`, `remember_learning_tip`, `search_learning_tips`, `get_learning_tip`, and MCP server wording.
Outputs: remaining references either removed or intentionally kept only if unrelated to the server feature.
Success criteria: no stale mentions of the removed server remain.

7. Verification checkpoint.
Inputs: updated codebase.
Outputs: build/test results and a final grep pass.
Success criteria: `cargo test` passes for touched crates, `voidm --help` shows no MCP command, and repo search shows no stale MCP server docs/code.

## Step 2 - Required Libraries / Dependencies

### Mandatory

#### Runtime

- None to add.
Why: this is a removal task.
Minimal alternative: not applicable.

#### Dev / Test / Tooling

- None to add.
Why: existing `cargo` tooling is enough.
Minimal alternative: not applicable.

### Optional

#### Runtime

- Remove `rmcp` from `crates/voidm-cli/Cargo.toml` if no remaining code needs it.
Why: it is MCP-server-specific.
Minimal alternative: keep it temporarily only if another module unexpectedly depends on it.

#### Dev / Test / Tooling

- None.

## Step 3 - Skills To Use

- `[HAVE]` `general`
Why: straightforward repo-safe removal and verification.

- `[HAVE]` `voidm-memory`
Why: continuity on repo decisions and removal scope.

## Step 4 - MCP Tools To Use

- `[HAVE]` None needed.
