# Task: Deduplicate model.onnx — Use Symlink Instead of Copy

**Date:** 2026-06-05  
**Status:** ✅ Complete  
**CI:** All checks passed (0 errors)

## Summary

Deduplicated model.onnx (~132MB) and tokenizer.json across runtime and cache directories using symlinks, saving ~132MB of disk per version upgrade.

## Problem

The model file could exist in two locations simultaneously:
1. **Bundled**: `~/.graphit/runtime/<version>/models/model.onnx` (extracted from embedded gzip at launcher startup)
2. **Cached**: `~/.graphit/models/coderankembed/model.onnx` (downloaded when no bundle exists)

On version upgrades, the runtime dir was cleaned and re-extracted, creating a fresh ~132MB copy even when the cache already had the same file.

## Solution

Two dedup layers, both with graceful degradation on Windows:

1. **Launcher** (`cmd/launcher/dedup.go`): `deduplicateModels()` runs after `extractRuntime()` — moves extracted files to the shared cache dir and creates symlinks back.

2. **Model Manager** (`internal/ai/dedup.go`): `deduplicateToCache()` called from `EnsureModel()` when bundled models are found — moves them to cache and symlinks.

**Canonical location**: `~/.graphit/models/coderankembed/` (persists across version upgrades).

## Files Changed

- `cmd/launcher/dedup.go` — [NEW] Launcher dedup logic
- `cmd/launcher/dedup_test.go` — [NEW] 9 tests
- `cmd/launcher/main.go` — [MODIFY] Added `deduplicateModels(runtimeDir)` call
- `internal/ai/dedup.go` — [NEW] Model manager dedup logic
- `internal/ai/dedup_test.go` — [NEW] 6 tests
- `internal/ai/model_manager.go` — [MODIFY] Added `deduplicateToCache()` in `EnsureModel`

## Cross-Platform

- **Linux/macOS**: Full symlink dedup works without privileges
- **Windows**: `os.Symlink` requires Developer Mode or admin. On failure, files stay in place and everything works as before (graceful degradation)
- `os.Stat` follows symlinks, so `isValid()` works transparently with symlinked files
