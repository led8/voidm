# Task: fix-reembed-shadow-table-cleanup

## Checklist
- [x] inspect current reembed failure path and approve the fix plan
- [x] implement stale `vec_memories_new*` cleanup helper
- [x] harden `reembed_all()` cleanup behavior
- [x] add regression tests
- [x] run targeted Cargo tests
- [x] verify `voidm models reembed` on the shared DB
- [x] run final verification

## Blocked
- [ ] none

## Notes
- shared DB is pinned to `~/.codex/memories/voidm/memories.db`
- old rename-based reembed leaves live shadow tables under `vec_memories_new_*`; startup cleanup can then break the current vector table
- current fix rebuilds `vec_memories` in place and only cleans legacy temp objects when it is safe
- shared DB was repaired from `/tmp/voidm-repaired.db`, plain `voidm models reembed` now succeeds, and `voidm stats --json` reports 29/29 embedded
