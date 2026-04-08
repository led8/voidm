# Development

This page is for contributors working inside the repository.

## Prerequisites

- Rust 1.94.0 or newer
- a working Cargo toolchain
- enough disk space for local model downloads when running inference-heavy commands

## Common Commands

```bash
cargo check -p voidm-cli
cargo check -p voidm-core
cargo check -p voidm-graph

cargo test -p voidm-cli
cargo test -p voidm-graph

cargo install --path crates/voidm-cli
```

Useful runtime commands while developing:

```bash
voidm info
voidm instructions
voidm init
```

## Where To Make Changes

### CLI behavior

Start in:

- `crates/voidm-cli/src/main.rs`
- `crates/voidm-cli/src/commands/*.rs`

### Search and recall

Start in:

- `crates/voidm-core/src/search.rs`
- `crates/voidm-core/src/rrf_fusion.rs`
- `crates/voidm-cli/src/commands/search.rs`
- `crates/voidm-cli/src/commands/recall.rs`

### CRUD and memory lifecycle

Start in:

- `crates/voidm-core/src/crud.rs`
- `crates/voidm-cli/src/commands/add.rs`
- `crates/voidm-cli/src/commands/mem_update.rs`
- `crates/voidm-cli/src/commands/list.rs`

### Graph and ontology

Start in:

- `crates/voidm-graph/src/*`
- `crates/voidm-core/src/ontology.rs`
- `crates/voidm-cli/src/commands/graph.rs`
- `crates/voidm-cli/src/commands/ontology.rs`

### Learning layer

Start in:

- `crates/voidm-core/src/learning.rs`
- `crates/voidm-cli/src/commands/learn.rs`
- [TRAJECTORY_LEARNING_LAYER.md](TRAJECTORY_LEARNING_LAYER.md)

### Configuration and runtime resolution

Start in:

- `crates/voidm-core/src/config.rs`
- `crates/voidm-core/src/config_loader.rs`
- `crates/voidm-cli/src/commands/info.rs`

## Development Workflow

Recommended loop:

1. inspect the relevant command and core module
2. change the smallest layer that solves the issue
3. run `cargo check` on the affected crate first
4. run the narrowest useful tests
5. update docs if behavior or defaults changed

## Documentation Rules

Keep the docs split stable:

- `README.md`: project entry point only
- `docs/concepts.md`: mental model
- `docs/cli.md`: operator-facing command map
- `docs/configuration.md`: runtime and tuning details
- `docs/architecture.md`: subsystem boundaries
- `docs/development.md`: contributor workflow

If a change introduces detailed operational behavior, it belongs in `docs/`, not in the README.

## Practical Notes

- `voidm info` is the fastest way to diagnose config and path resolution issues.
- Search defaults matter more than surface text in docs. When defaults change, update both code and `docs/configuration.md`.
- Recall behavior depends on structured bucket rules, not just string matching. Keep [docs/concepts.md](concepts.md) aligned with [recall.rs](../crates/voidm-cli/src/commands/recall.rs).
