# Fix: Grammar Embedding in Release Pipeline

## Status: Complete

## Problem

The release pipeline (`.github/workflows/release.yml`) was shipping binaries **without any grammar files** — no tree-sitter `.so`/`.dylib`/`.dll` shared libraries, and no ANTLR sidecar binaries. Only the YAML query definitions were present in the runtime.

### Root Cause

All three release build jobs (Linux, macOS, Windows) passed `SKIP_GRAMMARS=1` and `SKIP_ANTLR_GRAMMARS=1` to `make`, which:

1. **Skipped grammar compilation** — `grammars-treesitter` and `grammars-antlr` targets never ran
2. **Skipped grammar bundling** — `bundle_grammars` and `bundle_antlr` macros were not called

This meant the `cmd/launcher/runtime/grammars/` directory was never populated, and the Go `//go:embed runtime/*` directive had no grammar files to embed.

### Secondary Issue: Platform Extension Mismatch

The `compile_ts_grammar` Makefile macro hardcoded `.so` as the output extension regardless of platform. On macOS (which expects `.dylib`) and Windows (which expects `.dll`), the `DynGrammarLoader` would fail to find the grammar files because `libraryCandidates()` only generated platform-specific extensions.

### Tertiary Issue: Uncompressed Embedding

All grammar shared libraries and ANTLR sidecar binaries were embedded uncompressed, significantly inflating the launcher binary. The launcher already supported `.gz` decompression in `extractRuntime()` (used by `model.onnx.gz`), but this wasn't applied to grammars.

### Platform CI Failures

**macOS**: The Python grammar's `scanner.c` references `ts_current_malloc/free/realloc` — tree-sitter runtime allocation functions. On Linux, the linker allows undefined symbols in `-shared` (lazy binding). On macOS, clang/ld64 rejects undefined symbols by default, causing `Undefined symbols for architecture arm64` errors.

**Windows**: `go env GOMODCACHE` returns `C:/Users/...` but MSYS2's `find` expects Unix-style `/c/Users/...` paths. The `SMACKER_DIR` variable resolved to empty, producing `parser.c not found in /c`.

## Changes

### `.github/workflows/release.yml`
- Removed `SKIP_GRAMMARS=1` and `SKIP_ANTLR_GRAMMARS=1` from all 3 build jobs (linux, darwin, windows)

### `Makefile`
- Added `SHLIB_EXT` platform detection: `.dll` on Windows, `.dylib` on Darwin, `.so` on Linux
- Changed `compile_ts_grammar` output from hardcoded `.so` to `$(SHLIB_EXT)`
- Changed summary count glob from `*.so` to `*$(SHLIB_EXT)`
- Added `gzip -9` compression in `bundle_grammars` after validation (~65-75% size reduction)
- Added `gzip -9` compression in `bundle_antlr` after validation (~70-80% size reduction)
- Both macros log before/after sizes and compression ratio
- Added `ts_current_malloc/free/realloc/calloc` defines to `TS_CFLAGS` and `TS_CXXFLAGS` to fix macOS linker errors
- Fixed C++ branch of `compile_ts_grammar` to use `$(TS_CFLAGS)`/`$(TS_CXXFLAGS)` via `$(filter-out -shared,...)` instead of hardcoded flags
- Added `cygpath -u` conversion of `GOMODCACHE` on Windows for MSYS2 path compatibility

### `internal/ast/treesitter_dynload.go`
- Added `.so` as a universal fallback candidate in `libraryCandidates()` for non-Linux platforms
- This provides defense-in-depth: even if a grammar file has `.so` extension on macOS/Windows, the loader can still find and load it (both `dlopen` and `LoadLibrary` work regardless of extension)

## Impact

After this fix, `make build-linux/darwin/windows-native` will:
1. Compile all 37 tree-sitter grammars as platform-native shared libraries
2. Compile all 5 ANTLR sidecar binaries
3. Copy them into `cmd/launcher/runtime/grammars/`
4. Embed them via Go `embed.FS`
5. Extract them at install time to `~/.graphit/runtime/<version>/grammars/`
