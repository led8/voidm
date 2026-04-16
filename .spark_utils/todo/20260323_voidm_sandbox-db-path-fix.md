# Task: sandbox-db-path-fix

## Checklist
- [x] reproduce sandbox readonly-database failure and confirm writable-path workaround
- [x] implement sandbox-aware default DB path fallback
- [x] fix DB source diagnostics and sandbox guidance
- [x] add regression tests
- [x] update README
- [x] run final verification

## Blocked

## Notes
- sandbox allows writes in repo, `/tmp`, and `/Users/adhuy/.codex/memories`
- current default/user-resolved DB path is outside sandbox writable roots
- verified `cargo run --quiet -p voidm-cli -- info --json` resolves to `~/.codex/memories/voidm/memories.db` in sandbox
- verified sandboxed `cargo run --quiet -p voidm-cli -- add ...` succeeds without manual `VOIDM_DB`
