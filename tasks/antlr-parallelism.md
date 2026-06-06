# Task: ANTLR Per-Worker WASM Instances for Parallelism

## Summary

Added per-worker ANTLR WASM instances so each pipeline worker goroutine gets its own ANTLR parser instance, matching the existing tree-sitter `WorkerModules` pattern.

## Problem

The ANTLR engine used a singleton `parserProc` per grammar, protected by a mutex. This forced all worker goroutines to serialize ANTLR parsing, negating the parallelism benefit of the worker pool.

## Changes

### `internal/ast/wasmantlr/engine.go`
- Exported `parserProc` → `ParserProc` with public fields (`Stdin`, `Stdout`, `Close`, `Mu`)
- Added `compiled map[string]wazero.CompiledModule` to `Engine` to cache compiled WASM modules
- `Compile()` now stores the compiled module in `e.compiled[name]` before creating the singleton proc
- Added `NewWorkerProc(name)` — creates a per-worker WASM instance from the shared compiled module
- Added `workerCounter atomic.Int64` for unique wazero module names (module names must be unique)
- `HasCompiled()` now also checks the `compiled` map, not just `procs`

### `internal/ast/antlr_worker.go` (new file)
- `AntlrWorkerModules` — per-goroutine container for ANTLR parser procs
- `NewAntlrWorkerModules(engine)` — constructor (nil-safe)
- `Parse(name, source)` — lazily creates worker proc on first call per grammar, then reuses
- `Close()` — cleans up all worker procs

### `internal/ast/antlr_adapter.go`
- Added `workerModules *AntlrWorkerModules` field to `AntlrParser`
- `Parse()` routes to worker modules when available, falls back to singleton engine
- `getAntlrModule()` call kept unconditional to ensure grammar is compiled before workers use it

### `internal/ast/composite_parser.go`
- `NewCompositeParser` now accepts `*AntlrWorkerModules` as third parameter

### `internal/ast/pipeline.go`
- `RunPipeline`: passes `nil` for AWM (non-worker path)
- Worker goroutines: create `AntlrWorkerModules` alongside `WorkerModules`, pass to `NewCompositeParser`, defer `Close()`

## Design Decisions

1. **Compiled module sharing**: The wazero `CompiledModule` is thread-safe and shared. Only instances are per-worker, keeping memory overhead low.
2. **Atomic counter for names**: wazero requires unique module names per instantiation. An atomic counter generates `"antlr-plsql-w1"`, `"antlr-plsql-w2"`, etc.
3. **Lazy worker proc creation**: Worker procs are created on first parse of each grammar, not eagerly. This avoids creating unused instances.
4. **Unconditional getAntlrModule**: Kept in the adapter to ensure compilation before workers attempt to create instances.
5. **Nil-safe AntlrWorkerModules**: When engine is nil (no ANTLR grammars), `NewAntlrWorkerModules` returns nil, and `Parse`/`Close` handle nil receiver gracefully.

## Verification

- `go build ./internal/ast/...` — passes with zero errors
