# Task: Align voidm with a trajectory-informed learning layer

## Checklist
- [x] finalize learning-layer domain model
- [x] choose storage and migration strategy
- [x] define and implement trajectory ingestion and extraction flow
- [x] add `voidm learn ingest` with preview-first write behavior
- [x] add trajectory parsing and extraction tests
- [x] define and implement consolidation and canonicalization rules
- [x] add `voidm learn consolidate` with preview and apply modes
- [x] ensure learning search skips superseded tips after consolidation
- [x] define retrieval and ranking behavior
- [x] define CLI and MCP interface changes
- [ ] define evaluation and rollout plan
- [ ] run implementation readiness review

## Blocked
- [ ] none

## Notes
- approved plan stored in `.spark_utils/backlog/20260316_voidm_trajectory_informed_learning_layer.md`
- todo file is the active tracker for the approved task
- phase 1 stores learning tips as regular memories with structured `metadata.learning_tip`
- phase 1 adds `voidm learn add/search/get` and MCP tools for structured learning tips
- phase 1 adds metadata indexes for learning category, task category, and source outcome
- phase 2 adds `voidm learn ingest --from <file>` for JSON/JSONL trajectory ingestion
- phase 2 is preview-first; use `--write` to persist extracted candidates
- phase 3 adds `voidm learn consolidate` to create canonical tips and invalidate clustered variants
