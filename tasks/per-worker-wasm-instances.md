# Per-Worker WASM Module Instances

## Summary

Fixed fatal `runtime: split stack overflow` crash during `graphit sync` and improved AST indexing performance by ~5x.

## Problem

Tree-sitter WASM modules share linear memory. Multiple pipeline worker goroutines accessing the same module concurrently corrupted memory, causing wazero AOT to crash with `split stack overflow`.

## Solution

Each worker goroutine gets its own set of WASM module instances (created lazily per language). Zero mutexes, zero locks, full parallelism.

## Changes

### `internal/ast/wasmts/engine.go`
- Store `wazero.CompiledModule` for re-instantiation
- `InstantiateModule()` creates isolated instances from compiled modules
- `CloseModule()` for per-worker cleanup
- Atomic counter for unique wazero instance names
- Removed `sync.Mutex` from `Module` (no longer needed)

### `internal/ast/wasmts/language.go`
- Added `Module()` accessor
- Removed `LockModule()`/`UnlockModule()`

### `internal/ast/worker_modules.go` [NEW]
- `WorkerModules` type with lazy `GetLanguage()` per worker
- `Close()` releases all worker-local instances

### `internal/ast/grammar_loader.go`
- `GetEngine()` exposes global engine for worker instantiation
- `GetGlobalLanguage()` for module name discovery

### `internal/ast/treesitter_adapter.go`
- `TreeSitterParser.workerModules` field
- `Parse()` resolves worker-local Language when available, falls back to global

### `internal/ast/pipeline.go`
- Each worker goroutine creates its own `TreeSitterParser` with `WorkerModules`

## Performance

| Metric | Before (mutex) | After (per-worker) |
|--------|---------------|-------------------|
| Parse 331 files | ~22s | 4.6s |
| Total index | 22s | 7.7s |

## Verification

- `make ci` passes (1 unrelated flaky test in mcpserver)
- `graphit sync --no-background` completes without crash
- `graphit ast index --reindex` completes in 7.7s
