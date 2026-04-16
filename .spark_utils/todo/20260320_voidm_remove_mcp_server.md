# Task: Remove the MCP server from voidm

## Checklist
- [x] inventory the full MCP removal surface
- [x] remove the `mcp` CLI command and command module wiring
- [x] remove MCP-only dependencies and imports
- [x] remove MCP-specific tests
- [x] clean README and learning-layer docs
- [x] sweep residual MCP server references
- [x] run final verification

## Blocked
- [ ] none

## Notes
- created after plan approval
- removed the `mcp` CLI entry, module, and `rmcp` dependency
- README MCP section removed and learning-layer doc MCP references removed
- residual grep is clean outside `.spark_utils`
- verified with `cargo test -p voidm-cli --quiet`, `cargo test -p voidm-core learning --quiet`, and `cargo run -q -p voidm-cli -- --help`
