# Task: Build UUID Stamp Optimization

**Date:** 2026-06-18  
**Status:** ✅ Complete

## Problem

In dev mode (`Version == "dev"`), on every launcher invocation `devStampChanged` called `computeStamp(exe)`, which opened and SHA-256 hashed the entire launcher binary (potentially hundreds of MB) just to detect if it had changed since the last run.

## Solution

Embed a random UUID at compile time via `-ldflags` and use it as the base for the stamp. Instead of hashing the binary, the stamp is `sha256(BuildID)` — a hash of 36 bytes instead of megabytes.

## Files Changed

### `internal/version/version.go`
- Added `var BuildID = ""` — empty by default, injected at build time via ldflag.

### `cmd/launcher/main.go`
- Added `computeBuildIDStamp()` — returns `sha256(version.BuildID)`, O(36 bytes).
- Updated `writeLauncherStamp` — prefers `computeBuildIDStamp()`, falls back to `computeStamp(path)` when `BuildID == ""`.
- Updated `devStampChanged` — fast path via `computeBuildIDStamp()` when available; falls back to binary hash for old installs without BuildID.
- Added doc comment to `computeStamp` explaining it is now a fallback.

### `Makefile`
- Added `BUILD_ID` variable, generated once per `make` invocation:
  - **Windows**: `powershell -Command "[System.Guid]::NewGuid().ToString()"` (always available)
  - **Linux**: `cat /proc/sys/kernel/random/uuid` (always available in kernel)
  - **macOS**: `uuidgen` (always available)
- Added ldflag: `-X 'github.com/graphit-labs/graphit-code/internal/version.BuildID=$(BUILD_ID)'`

## Backward Compatibility

- Old stamp files (containing a binary hash) will not match the new BuildID-based stamp → triggers one re-extraction on first run after upgrade. Benign.
- Builds without `BuildID` (e.g. `go run ./cmd/launcher` without ldflags) continue using the binary-hash fallback.

## Tests

All existing tests pass without modification:
- `TestWriteLauncherStamp/*` — unaffected because `BuildID == ""` in tests, fallback path is used.
- `TestReadLauncherStamp_*`, `TestLauncherStampPath_*` — unrelated to this change.
- `go vet` and `golangci-lint` — 0 issues.
