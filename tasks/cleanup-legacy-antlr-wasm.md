# Task: Cleanup Legacy ANTLR WASM Code and Comments

Removed old WASM-based comments and benchmark files for ANTLR now that it runs natively in-process in Go.

## Completed Changes
- Deleted obsolete/broken benchmark script `tools/bench_plsql_test.go` which depended on `wasmantlr.Engine` and `antlr-plsql.wasm`.
- Cleaned up legacy/misleading comments in `internal/ast/antlr_adapter.go` referencing dynamic WASM loading or lazy-loading binaries.
- Added a validation check in `antlr_adapter.go` to explicitly fail if a non-PL/SQL grammar is configured for ANTLR, preventing bugs when users add other ANTLR grammars dynamically without recompiling.
