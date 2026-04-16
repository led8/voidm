# Task: Add live runtime ingestion for the trajectory-informed learning layer

## Step 1 - Implementation Plan

1. Define the live-ingestion session model.
Inputs: current `LearningTrajectory` and `LearningTrajectoryStep`, phase 2 file-based ingestion, phase 3 consolidation rules.
Outputs: session lifecycle for `start`, `append_step`, `finish`, and `abort`, plus an event schema that maps cleanly to the existing trajectory model.
Success criteria: every live event can be converted into the current trajectory format without losing fields or provenance.

2. Decide persistence for in-progress sessions.
Inputs: current SQLite schema, crash-recovery expectations, current memory storage model.
Outputs: storage decision for active sessions and their steps, including whether they live in dedicated SQLite tables and how they are cleaned up.
Success criteria: in-progress sessions survive process restarts and do not pollute the normal memory store.

3. Checkpoint A: schema and lifecycle review.
Inputs: steps 1 and 2 outputs.
Outputs: short list of unresolved decisions only.
Success criteria: no ambiguity remains around session state, finalize behavior, or cleanup rules.

4. Implement the core session model in `voidm-core`.
Inputs: approved lifecycle and storage design.
Outputs: APIs to create sessions, append steps, inspect state, finalize to a `LearningTrajectory`, and abort safely.
Success criteria: a session can be built incrementally and finalized through the same extraction path used by phase 2.

5. Implement the public interface in `voidm-cli`.
Inputs: core session APIs.
Outputs: additive commands such as `voidm learn session start`, `step`, `finish`, `abort`, `get`, and `list`.
Success criteria: an external agent runtime can stream a run into `voidm` without first exporting a trajectory file.

6. Reuse existing extraction and consolidation on finalize.
Inputs: finalized live session state, phase 2 extraction pipeline, phase 3 consolidation pipeline.
Outputs: `finish` behavior that can preview or persist extracted tips and optionally consolidate afterward.
Success criteria: there is one extraction and consolidation path shared by file-based and live ingestion.

7. Add inspection and safety behavior.
Inputs: session storage and CLI flow.
Outputs: idempotent finish and abort behavior, duplicate-finalize protection, and clear status reporting for incomplete sessions.
Success criteria: partial or retried runs are safe and debuggable.

8. Checkpoint B: end-to-end validation on representative coding-agent runs.
Inputs: one or two example live sessions.
Outputs: tested flow from `start` to `step` to `finish`, with extracted learning tips and optional consolidation.
Success criteria: live ingestion produces equivalent or better results than file-based ingestion for the same run data.

9. Define evaluation and rollout.
Inputs: implemented live-ingestion path, current learning-layer behavior, paper-driven goals.
Outputs: comparison plan for file-based vs live ingestion, reliability checks, and retrieval-quality checks after consolidation.
Success criteria: usability improves without degrading learning quality or increasing memory noise.

10. Checkpoint C: implementation readiness review.
Inputs: steps 1 through 9 outputs.
Outputs: final scoped build order for the first implementation slice.
Success criteria: the first live-ingestion slice is small, testable, and independently useful.

## Step 2 - Required Libraries / Dependencies

### Mandatory

#### Runtime

- None planned initially.
Why: live runtime ingestion can build on the current Rust, SQLite, `sqlx`, `tokio`, and learning-layer stack.
Minimal alternative: not applicable.

#### Dev / Test / Tooling

- None planned initially.
Why: the current `cargo test` setup is enough for the first slice.
Minimal alternative: not applicable.

### Optional

#### Runtime

- `schemars` or `jsonschema`
Why: useful only if session-step payload validation needs a stronger schema contract at the CLI or MCP boundary.
Minimal alternative: manual Rust validation.

#### Dev / Test / Tooling

- `insta`
Why: useful if live-session JSON output becomes large or deeply structured.
Minimal alternative: plain JSON assertions.

## Step 3 - Skills To Use

- `[HAVE]` `general`
Why: preserve the current architecture and keep the session model simple.

- `[HAVE]` `voidm-memory`
Why: capture the durable runtime-ingestion contract once implementation starts.

- `[MAY NEED]` `python`
Why: useful if fixture generation or evaluation helpers are faster outside Rust.

## Step 4 - MCP Tools To Use

- `[HAVE]` None required for the first implementation slice.

- `[MAY NEED]` `mcp__context7__resolve-library-id`
Why: only needed if implementation introduces a new crate and current docs are required.

- `[MAY NEED]` `mcp__context7__query-docs`
Why: only needed if implementation introduces a new crate and current docs are required.
