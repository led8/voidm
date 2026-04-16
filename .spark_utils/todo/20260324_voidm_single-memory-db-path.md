# Task: single-memory-db-path

## Checklist
- [x] inspect the current shared DB path and confirm the new repo-adjacent target
- [x] keep the approved backlog and todo aligned with the requested path change
- [x] clone the active shared DB into `/Users/adhuy/code/led8/ai/spark/voidm/.spark_utils/data/voidm-memories-db/memories.db`
- [x] pin `voidm` config to `/Users/adhuy/code/led8/ai/spark/voidm/.spark_utils/data/voidm-memories-db/memories.db`
- [x] verify plain sandboxed `voidm` commands operate on the workspace-local DB
- [x] replace the stale external-path memories with the workspace-local path preference
- [x] run final verification

## Blocked
- [ ] none

## Notes
- final canonical shared DB path on 2026-04-01 is `/Users/adhuy/code/led8/ai/spark/voidm/.spark_utils/data/voidm-memories-db/memories.db`
- migration strategy remained clone, not move; the previous sibling-path DB and the older `.codex` DB were left in place as fallback copies
- persistent config now points to the workspace-local path via `~/.config/voidm/config.toml`
- workspace-local verification confirmed the cloned DB opened cleanly with 52 memories
- plain sandboxed `voidm info`, `voidm get`, and `voidm stats` now work against the shared DB without elevated access
- refreshed durable repo memory ID `3cf5a547-b38e-4ae8-8291-0ec82a01010c` stores the workspace-local path preference
- stale memory IDs `1890fdc2-c403-489b-b973-a86c222e9c29` and `77a14c58-2204-4c1b-a565-87d52dcbdf6d` were deleted after the workspace-local switch
- the older path-preference memory ID `41e92bec-b6f5-4ce2-a99b-22fd8c6f9b8e` had already been replaced during the prior sibling-path migration
