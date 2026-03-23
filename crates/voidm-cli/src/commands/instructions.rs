use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct InstructionsArgs {}

pub fn run(_args: &InstructionsArgs, json: bool) -> Result<()> {
    if json {
        println!("{}", INSTRUCTIONS_JSON);
    } else {
        println!("{}", INSTRUCTIONS_MD);
    }
    Ok(())
}

const INSTRUCTIONS_MD: &str = r#"# voidm — Agent Usage Guide

## Overview

voidm is a local-first memory tool for LLM agents. Memories are stored as typed nodes
in a hybrid SQLite database (vector + graph + full-text). Use it to accumulate, connect,
and retrieve knowledge across sessions.

## Memory Types

| Type | When to use |
|------|-------------|
| `episodic` | Time-bound events: what happened, when, in what context |
| `semantic` | Timeless facts: best practices, rules that remain true |
| `procedural` | Step-by-step instructions, runbooks, workflows |
| `conceptual` | Abstract ideas, architectural decisions, the *why* |
| `contextual` | Scope-specific env facts: config, conventions, local state |

## Decision Flowchart

- Is it a concrete step-by-step process? → `procedural`
- Is it about *why* a decision was made? → `conceptual`
- Is it a timeless fact? → `semantic`
- Is it specific to a project/environment? → `contextual`
- Is it something that happened? → `episodic`

## Edge Types

| Edge | Directed | Use when |
|------|----------|----------|
| `SUPPORTS` | yes | A confirms or strengthens B |
| `CONTRADICTS` | yes | A conflicts with B |
| `DERIVED_FROM` | yes | A was inferred from B |
| `PRECEDES` | yes | A happened before B |
| `PART_OF` | yes | A is a sub-element of B |
| `EXEMPLIFIES` | yes | A is a concrete instance of abstract B |
| `INVALIDATES` | yes | A supersedes B (B is outdated) |
| `RELATES_TO` | undirected | Generic association — requires --note |

## Agent Insertion Workflow

1. `voidm add "<content>" --type <type> --json`
2. Check `duplicate_warning` — if score ≥ 0.95, consider whether to keep or delete
3. Check `suggested_links` — for each candidate, decide the edge type from the hint
4. `voidm link <new-id> <EDGE_TYPE> <candidate-id>` (use `--note` for RELATES_TO)
5. Resolve any `conflict_warning` from `voidm link`

## Trajectory-Informed Learning Tips

Use `voidm learn` when you want structured, reusable guidance distilled from agent trajectories.

- `voidm learn add` stores a generalized tip plus trigger, application context, task category, source outcome, priority, and source trajectory ids.
- `voidm learn ingest --from <file>` parses coding-agent trajectories, extracts candidate tips, and previews them by default.
- `voidm learn consolidate` clusters overlapping tips and creates canonical records when run with `--write`.
- `voidm learn search` retrieves only structured learning tips.
- `voidm learn get` shows the structured learning metadata for one tip.

## Examples

```bash
# Add a memory with immediate links
voidm add "Deployment takes 15 min" --type episodic --json
voidm add "Always run tests before deploy" --type procedural \
  --link <prev-id>:DERIVED_FROM:"learned from deployment incident" --json

# Search (with optional intent for focused query expansion)
voidm search "deployment" --json
voidm search "database" --scope work/acme --mode semantic --json
voidm search "auth" --intent "oauth2" --json              # Intent-guided expansion
voidm search "config" --scope work/acme --intent "environment" --json  # Intent overrides scope

# Add and search a trajectory-informed learning tip
voidm learn add "Use jittered retries when OAuth refresh gets a transient 401." \
  --category recovery \
  --trigger "transient 401 during token refresh" \
  --application-context "OAuth2 token refresh flow" \
  --task-category authentication \
  --source-outcome recovered_failure \
  --trajectory traj-20260316-auth-01 --json

# Preview candidate tips from a coding-agent trajectory
voidm learn ingest --from trajectory.json --dry-run --json

# Persist extracted tips
voidm learn ingest --from trajectory.json --write --scope voidm --json

# Preview consolidation clusters
voidm learn consolidate --scope voidm --dry-run --json

# Persist canonical learning tips and supersede clustered members
voidm learn consolidate --scope voidm --write --json

voidm learn search "oauth refresh" --category recovery --json

# Graph
voidm graph neighbors <id> --depth 2 --json
voidm graph cypher "MATCH (a:Memory)-[:SUPPORTS]->(b:Memory) RETURN a.memory_id, b.memory_id LIMIT 10"
voidm graph pagerank --top 10 --json

# Link two existing memories
voidm link <id1> SUPPORTS <id2>
voidm link <id1> RELATES_TO <id2> --note "both concern API design"
```

## Search with Intent

The `--intent` parameter guides query expansion toward a specific context:

```bash
# Without intent (broad expansion)
voidm search "auth" --json
# Expands to: auth, authentication, login, access control, identity...

# With intent (focused expansion)
voidm search "auth" --intent "oauth2" --json
# Expands to: auth, oauth2, oidc, jwt, bearer token, openid connect...

# Intent falls back to scope if configured
voidm search "config" --scope work/infra --json
# Uses "work/infra" as implicit intent if intent.use_scope_as_fallback=true
```

Intent is optional—all searches work without it. Use it when you need to focus expansion on a specific domain or technology.

## Scope Conventions

Scopes are slash-delimited strings: `project/component/layer`
- `--scope work/acme/backend` — specific scope
- Search with `--scope work/acme` matches all children by prefix
- Multiple scopes per memory: `--scope work/acme --scope work/acme/api`

## Exit Codes

- `0` — success
- `1` — not found
- `2` — error (bad args, write attempt on cypher, missing required field)
"#;

const INSTRUCTIONS_JSON: &str = r#"{
  "tool": "voidm",
  "version": "0.1.0",
  "description": "Local-first memory tool for LLM agents. SQLite + vector + graph.",
  "memory_types": {
    "episodic": {
      "description": "Time-bound events — what happened, when, in what context",
      "when_to_use": ["Actions taken", "Bugs fixed", "Conversations", "Errors encountered"],
      "when_not_to_use": ["Timeless facts → semantic"],
      "example": "Deployed API to production at 2pm, took 15 minutes"
    },
    "semantic": {
      "description": "Timeless facts and knowledge about how things work",
      "when_to_use": ["Technical facts", "Best practices", "Rules that remain true over time"],
      "when_not_to_use": ["Scope-specific env facts → contextual", "Design rationale → conceptual"],
      "example": "The database migration takes ~5 minutes to run"
    },
    "procedural": {
      "description": "Step-by-step instructions, workflows, runbooks",
      "when_to_use": ["Deployment steps", "Debugging procedures", "Recurring how-tos"],
      "when_not_to_use": ["Why a process exists → conceptual", "One-time events → episodic"],
      "example": "To deploy: 1) Run tests, 2) Build, 3) Push to registry, 4) Apply k8s manifest"
    },
    "conceptual": {
      "description": "Abstract ideas, architectural decisions, the why behind choices",
      "when_to_use": ["ADRs", "Design rationale", "Trade-off reasoning", "Principles"],
      "when_not_to_use": ["Concrete facts → semantic", "Implementation steps → procedural"],
      "example": "Chose PostgreSQL over MongoDB for ACID guarantees and complex query support"
    },
    "contextual": {
      "description": "Scope-specific environmental facts — config, conventions, local state",
      "when_to_use": ["Team conventions", "Env-specific config", "Local tooling", "Ownership"],
      "when_not_to_use": ["Universal facts → semantic"],
      "example": "The staging DB is at postgres://staging.internal:5432/app"
    }
  },
  "edge_types": {
    "SUPPORTS": { "directed": true, "description": "A confirms or strengthens B", "example": "episodic observation SUPPORTS semantic rule" },
    "CONTRADICTS": { "directed": true, "description": "A conflicts with B", "example": "new finding CONTRADICTS old assumption" },
    "DERIVED_FROM": { "directed": true, "description": "A was synthesized or inferred from B", "example": "procedure DERIVED_FROM incident analysis" },
    "PRECEDES": { "directed": true, "description": "A happened before B", "example": "deploy event PRECEDES rollback event" },
    "PART_OF": { "directed": true, "description": "A is a sub-element of B", "example": "step PART_OF procedure" },
    "EXEMPLIFIES": { "directed": true, "description": "A is a concrete instance of abstract idea B", "example": "specific bug EXEMPLIFIES general pattern" },
    "INVALIDATES": { "directed": true, "description": "A supersedes B; B should be considered outdated", "example": "new procedure INVALIDATES old one" },
    "RELATES_TO": { "directed": false, "description": "Generic association — requires --note. Use only when no stronger type applies.", "example": "two contextual facts that influence each other" }
  },
  "hint_table": {
    "episodic+episodic": ["PRECEDES", "RELATES_TO"],
    "episodic+semantic": ["SUPPORTS", "CONTRADICTS"],
    "episodic+procedural": ["DERIVED_FROM", "RELATES_TO"],
    "semantic+semantic": ["SUPPORTS", "CONTRADICTS", "DERIVED_FROM"],
    "semantic+conceptual": ["SUPPORTS", "EXEMPLIFIES"],
    "conceptual+conceptual": ["SUPPORTS", "CONTRADICTS", "DERIVED_FROM"],
    "conceptual+semantic": ["DERIVED_FROM", "SUPPORTS"],
    "procedural+procedural": ["INVALIDATES", "PART_OF"],
    "procedural+episodic": ["DERIVED_FROM"],
    "contextual+contextual": ["RELATES_TO (with note)", "PART_OF"],
    "contextual+semantic": ["EXEMPLIFIES", "RELATES_TO"]
  },
  "agent_workflow": [
    "1. voidm add \"<content>\" --type <type> --json",
    "2. Check duplicate_warning (score >= 0.95) → consider delete + link instead",
    "3. Check suggested_links → decide edge type from hint, call voidm link",
    "4. voidm link <new-id> <EDGE_TYPE> <candidate-id> [--note <reason>]",
    "5. Resolve conflict_warning if present (voidm unlink the opposing edge)"
  ],
  "exit_codes": { "0": "success", "1": "not found", "2": "error" }
}"#;
