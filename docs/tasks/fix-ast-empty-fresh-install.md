# Fix: AST Empty After Fresh Install

**Date**: 2026-06-16  
**Status**: Complete  
**Impact**: Critical — AST was completely non-functional on fresh installations

## Problem

Running `graphit setup` + `graphit init` on a new machine reported "437 files up to date" but the `.graphit/ast/project` directory remained empty. No LadybugDB was created, no shard cache was populated.

## Root Causes

### 1. Missing Grammar Build Dependency (Makefile)

The build targets (`build-linux`, `build-darwin`, `build-windows`, `build-windows-native`) called `$(call bundle_grammars)` to copy tree-sitter `.so` files into `cmd/launcher/runtime/grammars/treesitter/`, but did NOT depend on `grammars-treesitter` (the target that actually compiles them). If the `.so` files didn't exist in `.build/grammars/treesitter/`, the macro silently copied nothing, leaving only a `.keep` placeholder file.

This meant the launcher binary had no grammar libraries embedded. When extracted to `~/.graphit/runtime/<version>/grammars/treesitter/`, only the `.keep` file appeared.

### 2. Silent Parse Error Masking (lifecycle.go)

In `runSyncPhase1()`, the AST result reporting checked `result.ParsedFiles == 0` and reported "up to date" without checking `result.ErrorCount`. When all 437 files failed to parse (because no grammar `.so` files were loadable), the code displayed a green checkmark and "up to date" instead of reporting the errors.

## Changes

### Makefile
- Added `grammars-treesitter` as a dependency to all 4 build targets
- Added validation step in `bundle_grammars` macro that fatally errors if no grammar libraries were bundled

### cmd/graphit/commands/lifecycle.go
- Added `ErrorCount` checks before the "up to date" message:
  - All files errored → `task.Fail(...)` with "grammars may be missing" hint
  - Some files errored → `task.Done(...)` with error count in the message
  - No errors, no parsed files → "up to date" (original behavior)

### Not Changed
- `runners.go`: Already had correct error-first checking at lines 287-289
- `tools_lifecycle.go` (MCP): Discards pipeline result — no output to fix

## Verification

- `go vet ./cmd/graphit/commands/` passes
