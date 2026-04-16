# Task: Agent UX Improvements

## Checklist
- [x] F3 — `voidm scope detect` (git root → scope string)
- [x] F1 — `voidm update <id>` (in-place memory patch, preserve edges)
- [x] F5 — `--agent` output mode (compact JSON, token-minimal)
- [x] F2 — `voidm recall` startup command (5-category digest, single call)
- [x] F6 — `voidm stats --scope` (per-scope breakdown)
- [x] F11 — scope traversal consistency (audited: prefix match already consistent; `--exact-scope` deferred)
- [x] F4 — staleness scoring (`last_accessed_at`, `age_days` in results, `voidm stale`)
- [x] F8 — `voidm learn ingest --stdin` (live trajectory ingestion)
- [x] F7 — `voidm add --batch <file>` (atomic multi-memory insert)
- [x] F9 — conflict surfacing in search results (inline CONTRADICTS warning)
- [x] F10 — `voidm why <id>` (provenance summary)

## Notes
- Backlog: `.spark_utils/backlog/20260403_voidm_agent_ux_improvements.md`
- No new external dependencies needed
- F3 → F1 → F5 → F2 is the recommended warm-up sequence (fast wins, high leverage)
- F11 requires an audit of all `--scope` usages before patching
