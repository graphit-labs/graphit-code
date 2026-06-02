# Fix Remove Project: Full Cleanup (Global Lock + MCP _graphitManagedMcpKeys)

## Date
2026-06-02

## Summary
Fixed the `remove` command to properly clean up all project traces: MCP config (`_graphitManagedMcpKeys`), global lock registry, and IDE-specific artifacts.

## Problem
Three bugs in the remove flow:
1. **MCP cleanup never happened** — `OnRemove` never called the IDE adapter's `Remove` method, leaving orphaned servers in the IDE's MCP config.
2. **Global lock unregister was broken** — CLI and MCP tool tried to load the lockfile *after* `OnRemove` deleted it via `UninstallAll`, so `lf` was always `nil` and `UnregisterProject` silently did nothing.
3. **Per-IDE removal didn't clean up MCP** — When removing one IDE while others remain, the removed IDE's MCP config kept stale server entries.

## Changes

### `internal/hub/lifecycle.go`
- Rewrote `OnRemove` to load the lockfile **first** (before any destructive action) to capture `projectID` and installed artifacts.
- **Always** calls `adapter.Remove(pp, flat)` for the removed IDE — cleans up `_graphitManagedMcpKeys` regardless of whether other IDEs remain.
- Moved global lock `UnregisterProject` inside `OnRemove`, **before** `UninstallAll` deletes the lockfile.
- Extracted `buildInstalledFlat` helper (also used by `syncIDEAdapter`) to build the artifact flat map with `project_id`.

### `cmd/graphit/commands/lifecycle.go`
- Removed redundant (and broken) global lock unregister block from `newRemoveCmd`.

### `internal/mcpstdio/tools_lifecycle.go`
- Removed redundant (and broken) global lock unregister block from the MCP remove tool.

## Verification
- `make ci` passed with zero errors.
