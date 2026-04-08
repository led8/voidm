# CLI Guide

This page summarizes the command surface without duplicating every flag from `--help`.

## Common Global Flags

| Flag | Use |
| --- | --- |
| `--json` | Machine-readable output for automation and agents |
| `--agent` | Compact agent-optimized output |
| `--quiet` | Suppress decorative output |
| `--db` | Override the database path directly |
| `--search-mode` | Override the default search mode for this invocation |

For the full global override set, run `voidm --help`.

## Memory Lifecycle

| Command | Purpose |
| --- | --- |
| `voidm add` | Add one memory and receive duplicate and link suggestions |
| `voidm batch-add` | Import multiple memories from JSON in one call |
| `voidm get <id>` | Fetch one memory by id or short id |
| `voidm update <id>` | Update a memory in place |
| `voidm delete <id>` | Delete a memory and its graph edges |
| `voidm list` | List memories, newest first |
| `voidm stale` | Find older memories for review |
| `voidm why <id>` | Show provenance, tags, age, and graph context for one memory |

## Retrieval

| Command | Purpose |
| --- | --- |
| `voidm search <query>` | Ranked retrieval across the configured search mode |
| `voidm recall` | Startup context grouped into architecture, constraints, decisions, procedures, and preferences |
| `voidm scopes list` | Show known scopes so filtering is easier |

Useful patterns:

```bash
voidm search "oauth refresh" --scope voidm --mode semantic
voidm search "deployment" --min-quality 0.7
voidm recall --scope voidm --task "search scoring"
```

## Learning Commands

| Command | Purpose |
| --- | --- |
| `voidm learn add` | Add one structured learning tip |
| `voidm learn ingest --from <file>` | Extract candidate tips from trajectory data |
| `voidm learn consolidate` | Cluster overlapping tips into canonical records |
| `voidm learn search <query>` | Search only structured learning tips |
| `voidm learn get <id>` | Fetch one learning tip |

## Graph Commands

| Command | Purpose |
| --- | --- |
| `voidm link <from> <EDGE> <to>` | Create a memory edge |
| `voidm unlink <from> <EDGE> <to>` | Remove a memory edge |
| `voidm graph neighbors <id>` | Traverse nearby nodes |
| `voidm graph path <from> <to>` | Find the shortest path |
| `voidm graph pagerank` | Rank important nodes by centrality |
| `voidm graph stats` | Inspect graph edge counts |
| `voidm graph cypher "<query>"` | Run read-only Cypher over the graph model |

Important constraints:

- `RELATES_TO` needs `--note`.
- Cypher is read-only.
- Neighbor expansion in search is separate from graph traversal commands.

## Ontology and Conflict Commands

| Command | Purpose |
| --- | --- |
| `voidm ontology concept add|get|list|delete` | Manage ontology concepts |
| `voidm ontology link|unlink` | Create or remove ontology edges |
| `voidm ontology hierarchy` | Traverse `IS_A` relationships |
| `voidm ontology instances` | Resolve transitive instances |
| `voidm ontology extract` | Run NER against a memory and propose concepts |
| `voidm ontology enrich-memories` | Batch enrichment across memories |
| `voidm ontology enrich` | Classify relation or contradiction between two texts |
| `voidm ontology concept merge*` | Find, apply, inspect, and roll back concept merges |
| `voidm conflicts list|resolve` | Review and resolve contradiction edges |

## Export and Runtime Commands

| Command | Purpose |
| --- | --- |
| `voidm export` | Export memories as JSON, Markdown, or full relationship bundles |
| `voidm config show|set` | Inspect or update config values |
| `voidm info` | Show resolved config, database path, and search defaults |
| `voidm stats` | Show memory and graph summary stats |
| `voidm models list|reembed` | Inspect available embedding models or re-embed memories |
| `voidm init` | Pre-download models for offline or CI-friendly use |
| `voidm migrate` | Move data between supported backends |
| `voidm instructions` | Print the agent usage guide |
| `voidm check-update` | Check upstream release availability |

## Recommended Workflows

### Add and link knowledge

```bash
voidm add "RRF scores are small and should not use a 0.3 default cutoff" \
  --type semantic \
  --scope voidm/search
```

Then inspect the returned link suggestions before creating stronger edges manually.

### Retrieve context for a task

```bash
voidm recall --scope voidm --task "documentation split"
voidm search "README architecture docs" --scope voidm
```

### Inspect why a result matters

```bash
voidm why <memory-id>
voidm graph neighbors <memory-id> --depth 2
```

## Operator Notes

- Prefer `--json` when another tool will consume the result.
- Prefer `search` for direct lookup and `recall` for startup context.
- Keep scope filters broad enough to avoid hiding relevant memories.
- Use `voidm instructions` when you want the CLI to describe its own intended agent workflow.
