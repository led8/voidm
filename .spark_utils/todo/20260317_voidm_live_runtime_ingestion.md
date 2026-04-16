# Task: Add live runtime ingestion for the trajectory-informed learning layer

## Checklist
- [ ] define the live session lifecycle and event schema
- [ ] choose persistence for active sessions
- [ ] implement core session state and finalize flow
- [ ] add CLI commands for session start, step, finish, abort, and inspection
- [ ] reuse phase 2 extraction on session finalize
- [ ] define whether finalize can trigger phase 3 consolidation
- [ ] add tests for incremental session ingestion and finalize behavior
- [ ] run final verification

## Blocked
- [ ] none

## Notes
- created as a separate phase 4 planning tracker before implementation starts
- phase 4 should build on the existing trajectory model rather than invent a parallel ingestion format
- implementation is intentionally deferred until phase 1 through phase 3 behavior is exercised manually
