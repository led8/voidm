# Task: Search Recall Upgrade

## Checklist
- [x] create validated backlog and todo tracker
- [x] patch hybrid search threshold behavior for RRF-backed search
- [x] harden recall retrieval and bucket assignment
- [x] add regression coverage
- [x] update README if implementation semantics changed
- [x] run final compile and CLI verification

## Notes
- Backlog: `.spark_utils/backlog/20260408_voidm_search-recall-upgrade.md`
- Primary bug: hybrid search uses RRF-scale scores but still applies a legacy `0.3` default threshold
- Primary symptom: `voidm search "mfst" --scope mfst-agent` and `voidm recall --scope mfst-agent` return empty results while data exists
- Verification completed with:
  - `cargo check -q -p voidm-core`
  - `cargo check -q -p voidm-cli`
  - `cargo check -q --tests -p voidm-cli`
- Full `cargo test` remains blocked on the known local macOS linker issue (`clang_rt.osx` missing)
- `cargo check --tests -p voidm-core` still fails due pre-existing unrelated test code gaps in `db/tests.rs` and `db/neo4j.rs`
