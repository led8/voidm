# Concepts

This page covers the core model behind `voidm`: what a memory is, how scopes work, how nodes are connected, and how retrieval differs from recall.

## Memory Types

`voidm` stores typed memories. Choosing the right type matters because retrieval and recall use these types as signals.

| Type | Use for | Avoid when |
| --- | --- | --- |
| `episodic` | Time-bound events, incidents, observations, one-off executions | The content should remain true independent of time |
| `semantic` | Stable facts, constraints, rules, best practices | The content is mainly a step-by-step workflow |
| `procedural` | Runbooks, checklists, repeatable sequences | The content is mainly rationale rather than execution |
| `conceptual` | Architecture, trade-offs, design rationale, principles | The content is a concrete fact or operational recipe |
| `contextual` | Project-specific conventions, local setup, ownership, environment facts | The content should apply across scopes or repositories |

## Scopes

Scopes are slash-delimited strings such as `work/acme/api`.

Key rules:

- Scopes are prefixes, not exact-match namespaces.
- `--scope work/acme` matches `work/acme`, `work/acme/api`, and `work/acme/backend`.
- A memory can carry multiple scopes.
- Use scopes to narrow retrieval, not to encode every attribute.

Recommended pattern:

- repository or project
- component
- layer or concern

Example:

```text
voidm
voidm/search
voidm/search/reranker
```

## Edge Types

Memories can be linked in the graph. Prefer the strongest relation that is true.

| Edge | Directed | Use when |
| --- | --- | --- |
| `SUPPORTS` | yes | One memory strengthens another |
| `CONTRADICTS` | yes | Two memories conflict |
| `DERIVED_FROM` | yes | One memory is synthesized from another |
| `PRECEDES` | yes | One event happened before another |
| `PART_OF` | yes | One memory is a sub-part of a larger unit |
| `EXEMPLIFIES` | yes | A concrete case illustrates a general idea |
| `INVALIDATES` | yes | A newer memory supersedes an older one |
| `RELATES_TO` | no | Generic association when no stronger edge fits |

`RELATES_TO` should carry a note so the relationship stays interpretable later.

## Retrieval Modes

`voidm search` supports several retrieval modes.

| Mode | Meaning |
| --- | --- |
| `hybrid` | Default mixed retrieval across vector, BM25, and fuzzy signals |
| `hybrid-rrf` | Explicit rank-fusion mode for the same hybrid family |
| `semantic` | Vector-only retrieval |
| `keyword` / `bm25` | Full-text retrieval only |
| `fuzzy` | Typo-tolerant string matching |

Current implementation note:

- The hybrid family uses the unified rank-fusion pipeline today.
- Hybrid score thresholds are therefore calibrated for small rank-fusion scores, not older weighted scores.
- `semantic` is useful when you want vector-only behavior without hybrid fusion.

## Search vs Recall

`search` and `recall` serve different jobs.

### Search

Use `voidm search` when you have an explicit question or query string.

- It accepts scope and type filters.
- It can use query expansion, reranking, graph retrieval, and neighbor expansion.
- It returns ranked matches.

### Recall

Use `voidm recall` when you want startup context for a scope or task.

Recall collects memories into buckets:

- `architecture`
- `constraints`
- `decisions`
- `procedures`
- `preferences`

Behavior:

- It uses structured searches first.
- It filters results by memory type, prefixes such as `Decision:` or `Constraint:`, and metadata context.
- It falls back to recent scoped memories when a bucket would otherwise be empty.
- `--task` biases the retrieval toward a current topic.
- `--also` appends extra targeted searches.

## IDs and Output Modes

Operational details that matter in practice:

- Most commands accept short UUID prefixes after the first few characters are unique.
- `--json` is the stable machine-readable output mode.
- `--agent` is a compact variant optimized for LLM consumption.
- Human-readable output remains useful for interactive review and debugging.

## Learning Tips

Trajectory-informed learning tips reuse the same memory store instead of introducing a separate storage lane.

- They are stored as regular memories with structured metadata.
- Dedicated `voidm learn` commands handle insertion, ingestion, consolidation, and retrieval.
- See [Trajectory-informed learning layer](TRAJECTORY_LEARNING_LAYER.md) for the design details.
