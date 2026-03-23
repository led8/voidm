# Trajectory-Informed Learning Layer

Phase 1 adds a structured learning-tip layer on top of existing `voidm` memories without changing the storage model.
Phase 2 adds trajectory ingestion for coding-agent runs so `voidm` can extract candidate learning tips from structured run traces.
Phase 3 adds consolidation so overlapping tips can be clustered into canonical learning records instead of accumulating as near-duplicates.

## What Exists In Phase 1

- Learning tips are stored as regular memories with structured metadata under `metadata.learning_tip`.
- Each learning tip records:
  - `category`: `strategy`, `recovery`, or `optimization`
  - `trigger`
  - `application_context`
  - `task_category`
  - `subtask` (optional)
  - `priority` (1-10)
  - `source_outcome`: `success`, `recovered_failure`, `failure`, or `inefficient`
  - `source_trajectory_ids`
  - `negative_example` (optional)
- JSON indexes are created for key learning-tip fields so retrieval does not depend only on full-text similarity.
- New CLI commands:
  - `voidm learn add`
  - `voidm learn search`
  - `voidm learn get`

## What Exists In Phase 2

- `voidm learn ingest --from <file>` reads a trajectory file and extracts candidate learning tips.
- Supported input formats:
  - one JSON trajectory object
  - a JSON array of trajectory objects
  - JSONL with one trajectory object per line
- Ingest is preview-first:
  - `--dry-run` previews candidates without writing
  - `--write` persists extracted candidates as regular memories with `metadata.learning_tip`
- The trajectory format is aimed at coding-agent runs:
  - top-level fields: `trajectory_id`, `task`, `outcome`
  - optional fields: `task_category`, `application_context`, `subtask`, `summary`, `scopes`, `tags`, `agent`
  - steps contain structured signals such as `kind`, `action`, `observation`, `error`, `resolution`, `why_useful`, `subtask`, and `outcome`
- Current extraction heuristics:
  - recovery tips from `error` + `resolution`
  - optimization tips from inefficient or optimization-marked steps
  - strategy tips from successful steps with `why_useful` or `observation`

## What Exists In Phase 3

- `voidm learn consolidate` finds overlapping active learning tips.
- Consolidation works on active tips only:
  - tips already superseded by another learning tip through `INVALIDATES` are skipped
- Clustering heuristics use:
  - category match
  - task-category similarity
  - trigger similarity
  - tip content similarity
  - application-context similarity
  - optional subtask similarity
- `--dry-run` previews the clusters and canonical candidates.
- `--write` creates one canonical learning memory per cluster and links it to member tips with:
  - `DERIVED_FROM`
  - `INVALIDATES`
- The canonical memory keeps merged provenance:
  - union of source trajectory ids
  - highest priority in the cluster
  - representative trigger, context, and task category
  - `metadata.learning_consolidation` with cluster membership and similarity score
- `voidm learn search` now skips superseded learning tips by default, so canonical records are preferred after consolidation.

## Storage Strategy

The learning layer is intentionally backward-compatible:

- no new top-level memory table
- no new memory type added to the core enum
- structured learnings reuse the existing memory, graph, quality, and retrieval stack

This keeps all existing `add`, `search`, `link`, graph, and export behavior intact while allowing dedicated learning retrieval.

## Command Examples

```bash
voidm learn add "Use jittered retries when OAuth refresh gets a transient 401." \
  --category recovery \
  --trigger "transient 401 during token refresh" \
  --application-context "OAuth2 token refresh flow" \
  --task-category authentication \
  --source-outcome recovered_failure \
  --trajectory traj-auth-20260316-01

voidm learn search "oauth refresh" --category recovery --task-category authentication

voidm learn get <id>

voidm learn ingest --from trajectory.json --dry-run

voidm learn ingest --from trajectory.json --write --scope voidm

voidm learn consolidate --scope voidm --dry-run

voidm learn consolidate --scope voidm --write
```

Example trajectory:

```json
{
  "trajectory_id": "traj-voidm-auth-01",
  "task": "Fix OAuth refresh failures in the CLI flow",
  "task_category": "authentication",
  "application_context": "voidm Rust CLI repository",
  "outcome": "recovered_failure",
  "scopes": ["voidm"],
  "steps": [
    {
      "kind": "recovery",
      "outcome": "recovered",
      "subtask": "token refresh",
      "error": "Transient 401 during token refresh",
      "action": "Retry immediately",
      "resolution": "Use jittered retries before failing the refresh flow"
    },
    {
      "kind": "inspect",
      "outcome": "success",
      "subtask": "auth debugging",
      "action": "Inspect the existing auth code before editing",
      "why_useful": "it reveals the real failure path before writing a fix"
    }
  ]
}
```

## Current Boundary

The learning layer still does not implement:

- feedback-driven reprioritization
- automatic ingestion from live agent runtimes or streams

Those belong to later phases from the approved backlog plan.
