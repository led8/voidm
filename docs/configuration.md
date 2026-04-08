# Configuration

`voidm` resolves configuration from three layers, from lowest priority to highest priority:

1. config file
2. environment variables
3. CLI arguments

## Where Configuration Lives

Default config path:

- `XDG_CONFIG_HOME/voidm/config.toml`, when `XDG_CONFIG_HOME` is set
- otherwise `~/.config/voidm/config.toml`

Useful command:

```bash
voidm info
```

That reports the resolved config path, the active database path, its source, and current search defaults.

## Database Path Resolution

SQLite is the default backend.

Resolution order:

1. `--db`
2. `VOIDM_DB`
3. `--database-sqlite-path`
4. `VOIDM_DATABASE_SQLITE_PATH`
5. config file path
6. platform data directory

When running inside the Codex sandbox, `voidm` falls back to a writable path under `~/.codex/memories/voidm/` if no explicit database path was provided.

## Minimal Config Example

Start from [config.example.toml](../config.example.toml) and trim it down to the parts you actually use.

```toml
[database]
backend = "sqlite"

[embeddings]
enabled = true
model = "Xenova/all-MiniLM-L6-v2"

[search]
mode = "hybrid"
default_limit = 10
min_score = 0.0
```

## Environment Variable Naming

Environment overrides use the `VOIDM_` prefix with section names flattened to uppercase.

Pattern:

```text
VOIDM_SECTION_SUBSECTION_PARAM
```

Examples:

- `VOIDM_SEARCH_MODE`
- `VOIDM_DATABASE_SQLITE_PATH`
- `VOIDM_SEARCH_RERANKER_ENABLED`

See [config.example.toml](../config.example.toml) and [main.rs](../crates/voidm-cli/src/main.rs) for the currently exposed override surface.

## Search Defaults

Current built-in defaults in `voidm-core`:

- mode: `hybrid`
- default limit: `10`
- minimum score: `0.0`
- reranker: disabled
- query expansion: disabled
- graph retrieval: disabled unless configured

Why `min_score` defaults to `0.0`:

- The hybrid family uses rank-fusion style scores.
- Those scores are small compared with older weighted-score expectations.
- A non-zero default threshold can suppress valid results if it assumes a different score scale.

## Search Modes and Tuning

### Hybrid family

- `hybrid` is the default operator-facing mode.
- `hybrid-rrf` remains available as the explicit rank-fusion variant.
- The current implementation routes the hybrid family through the unified fusion pipeline.

Practical effect:

- use `hybrid` for general retrieval
- use `semantic` when you want vector-only behavior
- set `--min-score` explicitly only when you understand the score range you are filtering

### Optional subsystems

These are off by default and can be enabled selectively:

- reranker
- query expansion
- graph-aware retrieval

Recommended approach:

1. get base retrieval right with `hybrid` or `semantic`
2. enable reranking only when you need better top-result precision
3. enable query expansion only when recall is weak on underspecified queries
4. enable graph retrieval when concept or tag relationships are part of the retrieval strategy

## Models and Cache

`voidm` uses local models for embeddings and optional inference features.

Common commands:

```bash
voidm init
voidm models list
voidm models reembed
```

Model cache paths are managed under the user cache directory, typically under `~/.cache/voidm/`.

## Recommended Debugging Commands

```bash
voidm info
voidm config show
voidm search "your query" --json
voidm search "your query" --mode semantic --json
```

Use `info` first when behavior looks wrong. It shows which database path and search defaults the process is actually using.
