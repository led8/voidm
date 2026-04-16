# Task: Align voidm with a trajectory-informed learning layer

## Step 1 - Implementation Plan

1. Define the new learning-layer domain model.
Inputs: IBM paper, current memory types, current graph edges, current search model.
Outputs: schema for `trajectory`, `learning_tip`, `tip_category` (`strategy`, `recovery`, `optimization`), provenance, trigger, application context, task category, priority, and source outcome.
Success criteria: every paper concept is mapped to a first-class field or explicitly rejected with rationale.

2. Decide the storage strategy and migration boundary.
Inputs: current `Memory`/`AddMemoryRequest` model, SQLite schema, ontology model, graph model.
Outputs: storage decision between new first-class tables vs structured metadata on memories plus indexes, and a backward-compatible migration plan.
Success criteria: existing memories remain readable and searchable, and the new layer can be added without breaking current CLI/MCP workflows.

3. Checkpoint A: schema review before implementation.
Inputs: steps 1 and 2 outputs.
Outputs: short review list of unresolved choices only.
Success criteria: no ambiguity remains around whether learnings are separate entities or upgraded memories.

4. Design the ingestion pipeline from trajectory to candidate learnings.
Inputs: expected trajectory shape, current add/search flows, paper guidance on extracting tips from successful, recovered, and inefficient trajectories.
Outputs: segmentation rules, extraction stages, outcome taxonomy, and provenance attachment rules.
Success criteria: one trajectory can be converted into structured candidate tips with source references.

5. Design the consolidation pipeline.
Inputs: extracted candidates, current embeddings, duplicate warning flow, concept dedup primitives.
Outputs: clustering rules for similar tips, canonical tip selection rules, alias/variant attachment rules, and conflict/invalidations handling.
Success criteria: repeated similar learnings converge to one canonical record instead of many near-duplicates.

6. Checkpoint B: dry-run the extraction and consolidation design on a small fixture set.
Inputs: 3-5 representative trajectories.
Outputs: example canonical tips, rejected candidates, and conflict cases.
Success criteria: outputs are stable, interpretable, and closer to the paper's generalized tip model than raw episodic memories.

7. Design retrieval and ranking for the new layer.
Inputs: tip schema, current hybrid retrieval, graph retrieval, paper guidance on metadata-aware retrieval and LLM-guided selection.
Outputs: retrieval spec covering trigger matching, subtask matching, metadata filters, priority weighting, provenance-aware ranking, and optional selector/reranker behavior.
Success criteria: retrieval can answer "which learning applies here?" using more than raw content similarity.

8. Design the public interfaces.
Inputs: finalized storage and retrieval plan.
Outputs: proposed CLI/MCP surface such as `learn ingest`, `learn search`, `learn consolidate`, `learn feedback`, and read-only inspection commands.
Success criteria: the new flow is additive, agent-friendly, and does not degrade the existing generic `add/search/link` experience.

9. Define evaluation and rollout.
Inputs: sample trajectories, current test structure, paper metrics.
Outputs: benchmark plan for extraction precision, consolidation rate, retrieval relevance, and downstream task improvement; feature-flag and migration strategy.
Success criteria: the new layer can be evaluated on measurable agent improvement rather than memory growth.

10. Checkpoint C: implementation readiness review.
Inputs: steps 1-9 outputs.
Outputs: final phased build order with clear phase boundaries.
Success criteria: the first implementation slice is small, testable, and independently useful.

## Step 2 - Required Libraries / Dependencies

### Mandatory

#### Runtime

- None new initially.
Why: the first alignment pass can build on the current Rust, SQLite, embedding, graph, and CLI stack.
Minimal alternative: not applicable.

#### Dev / Test / Tooling

- None new initially.
Why: current `cargo` test and benchmark setup is enough to start design and early implementation.
Minimal alternative: not applicable.

### Optional

#### Runtime

- `schemars` or `jsonschema`
Why: useful if CLI/MCP accepts structured tip payloads and needs schema validation.
Minimal alternative: manual validation in Rust.

- Clustering library
Why: useful only if cosine-threshold consolidation is not sufficient at scale.
Minimal alternative: reuse current embeddings plus similarity thresholds.

#### Dev / Test / Tooling

- Snapshot testing such as `insta`
Why: useful if CLI/MCP outputs for the new layer become large or heavily structured.
Minimal alternative: plain string and JSON assertions.

- JSON / JSONL trajectory fixture corpus
Why: enables repeatable extraction, consolidation, and retrieval tests.
Minimal alternative: hard-coded Rust fixtures.

## Step 3 - Skills To Use

- `[HAVE]` `general`
Why: repo-safe architecture and phased implementation discipline.

- `[HAVE]` `voidm-memory`
Why: preserve durable architectural decisions once implementation begins.

- `[MAY NEED]` `python`
Why: useful if evaluation scripts or fixture tooling are faster to write outside Rust.

- `[MAY NEED]` `mermaid`
Why: useful if the learning pipeline needs a compact architecture diagram.

## Step 4 - MCP Tools To Use

- `[HAVE]` No MCP tool is required for the initial design itself.

- `[MAY NEED]` `mcp__context7__resolve-library-id`
Why: needed only if implementation introduces a new crate and current docs are required.

- `[MAY NEED]` `mcp__context7__query-docs`
Why: needed only if implementation introduces a new crate and current docs are required.
