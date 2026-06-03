# Task: Sync Without IDE Defaults to All Lockfile IDEs

**Date**: 2026-06-03
**Status**: ✅ Completed

## Problem

Running `graphit sync --no-background` without `--ide` resolved to a single IDE via `resolveIDEFlag()`. When the resolved IDE was invalid (e.g., `"ide"` from a stale config), this caused rule installation warnings and adapter sync failures.

## Changes

### `cmd/graphit/commands/lifecycle.go`
- Removed `resolveIDEFlag(cmd)` call from sync command handler
- Removed `else` fallback branch that used the resolved IDE
- When no `--ide` and lockfile has IDEs, uses `hub.FilterSupportedIDEs(lf.IDEs)`
- `spawnBackgroundSync` passes `""` instead of resolved IDE

### `internal/mcpstdio/tools_lifecycle.go`
- Removed `config.ResolveProjectIDE()` single-IDE resolution
- When `input.IDE` is empty, uses `hub.FilterSupportedIDEs(ides)` from lockfile
- Rules and adapter sync iterate over all valid IDEs

### `internal/hub/ide_adapter.go`
- Added `FilterSupportedIDEs(ides []string) []string`
- Added `SupportedIDEs() []string`

## Verification
- All tests pass in `commands`, `hub`, `mcpstdio` packages
- Manual sync runs clean with zero IDE warnings
