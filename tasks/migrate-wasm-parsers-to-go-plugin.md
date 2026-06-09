# Task: Migrate WASM Parsers to go-plugin Architecture

## Summary

Migrated the slow WASM-based tree-sitter and ANTLR parsers (running on Wazero) to a high-performance native parser plugin architecture using HashiCorp `go-plugin` for both tree-sitter and ANTLR v4 PL/SQL grammars.

## Problem

WASM-based execution on Wazero introduced high startup, translation, and parsing overhead, which resulted in very slow AST index generation. Furthermore, it required maintaining platform-independent WASM files that were prone to stack overflows and compilation delays.

## Changes

### `cmd/graphit-parser-plugin/`
- Implemented the plugin entrypoint in [main.go](../cmd/graphit-parser-plugin/main.go) with a `net/rpc` server serving the `Parser` interface.
- Implemented CGO-based native tree-sitter parsing in [treesitter.go](../cmd/graphit-parser-plugin/treesitter.go).
- Implemented native ANTLR PL/SQL parsing in [antlr.go](../cmd/graphit-parser-plugin/antlr.go).
- Implemented C wrappers for all 17 tree-sitter grammars in [treesitter/](../cmd/graphit-parser-plugin/treesitter/).

### `internal/ast/`
- Added RPC structs and interfaces in [plugin_rpc.go](../internal/ast/plugin_rpc.go).
- Refactored [treesitter_adapter.go](../internal/ast/treesitter_adapter.go) and [antlr_adapter.go](../internal/ast/antlr_adapter.go) to communicate with the plugin via the RPC client.
- Cleaned up [grammar_loader.go](../internal/ast/grammar_loader.go) by removing all wazero WASM engine code and adding go-plugin process management.
- Removed unused [worker_modules.go](../internal/ast/worker_modules.go) and [antlr_worker.go](../internal/ast/antlr_worker.go) files.
- Refactored [pipeline.go](../internal/ast/pipeline.go) and [composite_parser.go](../internal/ast/composite_parser.go) to remove unused WASM worker references.

### Build & Packaging
- Updated [Makefile](../Makefile) to build the native `graphit-parser-plugin` for all targets (`build-linux`, `build-darwin`, `build-windows`, `build-windows-native`) and bundle it in `cmd/launcher/runtime/` to be automatically managed and extracted by the launcher.
- Added a `build-parser-plugin` target to simplify building the plugin locally and added it as a dependency for the `test` target.
- Cleaned up the unused WASM files in `internal/ast/grammars/`.

## Design Decisions

1. **Native CGO parsing**: Moving parsing logic to native binaries eliminates WASM runtime startup/execution overhead and significantly increases parsing speed.
2. **HashiCorp go-plugin**: Provides reliable subprocess management over RPC, clean crash handling, and automatic process termination.
3. **Decoupled AST analysis**: AST pattern matching, complexity computation, docstring extraction, and export detection are processed directly inside the plugin process, sending only the final `ParsedFile` back to the host process.

## Verification

- Built the plugin locally using `make build-parser-plugin`.
- Ran unit tests using `go test ./internal/ast/...` (all tests passed with the WASM tests skipping gracefully).
