# Migrate WASM Tree-sitter to Wasmtime-Go and Run ANTLR PL/SQL Natively

## Summary
Migrated the slow/interpreted Tree-sitter WASM grammars from Wazero to `wasmtime-go/v21` for compiled, near-native execution speed. Native Go ANTLR parsing of PL/SQL was also re-integrated directly in-process. This allowed completely removing the complex and heavy HashiCorp `go-plugin` process-based IPC parser plugin.

## Details
1. **Wasmtime-Go Engine Integration**:
   - Replaced `wazero` dependency with `wasmtime-go`.
   - Created `WasmMemory` shim inside `internal/ast/wasmts/engine.go` to wrap raw linear memory slices and match the old `api.Memory` API, preserving `node.go`, `query.go`, `tree.go`, and `parser.go` without changes.
   - Configured Wasmtime JIT compilation cache in the project cache directory to ensure sub-millisecond warm startup.
2. **Wasmtime Language Instance Pooling**:
   - Added thread-safe language instance pooling via `sync.Pool` and `sync.Map` in `internal/ast/grammar_loader.go` to support concurrent/parallel parsing without SIGSEGV crashes.
   - Updated `internal/ast/treesitter_adapter.go` to acquire/release `Language` instances from the pool.
3. **In-process Native ANTLR Parser**:
   - Refactored `internal/ast/antlr_adapter.go` to parse PL/SQL natively in-process using `tools/antlr-go-grammars/plsql/parser` and `shared.ParseSLLThenLL`.
   - Replaced all IPC RPC client code.
4. **Direct AST Conversion (No JSON Bottleneck)**:
   - Replaced the heavy and expensive JSON serialization (`TreeToJSON`) and deserialization (`ParseTreeFromJSON`) roundtrip.
   - Implemented direct memory conversion (`convertParseTree`) from the native ANTLR `antlr.Tree` to `wasmantlr.TreeNode`.
   - **Performance Results**: Halved the elapsed time from **3m 54s** to **1m 51s** (a ~52% speedup, parsing 315 files/sec) and reduced total CPU time from **976s** to **526s** on 35k SQL files.
5. **Cleaned up Plugin**:
   - Deleted `internal/ast/plugin_rpc.go` and `cmd/graphit-parser-plugin/`.
   - Cleaned up `Makefile` build targets.
   - Tidied `go.mod` to remove `wazero` and `go-plugin`.
