# Fix: self-update spawning daemon as root

## Problem

Running `sudo graphit self-update` spawned a `graphit-core daemon` process under `/root/.graphit/`, owned by root. This caused MCP hangs and resource conflicts with the user's daemon.

## Root Cause

The `PersistentPreRun` hook in `root.go` calls `daemon.EnsureRunning()` for every command except `daemon`, `setup`, `uninstall`, and `_internal`. The `self-update` command was not excluded, so when run with `sudo` it started a daemon as root.

## Fix

Added `"self-update"` to the exclusion list in `root.go:30`:

```diff
-if name == "daemon" || name == "setup" || name == "uninstall" || name == "_internal" {
+if name == "daemon" || name == "setup" || name == "uninstall" || name == "self-update" || name == "_internal" {
```

## Files Changed

- `cmd/graphit/commands/root.go` — added self-update to PersistentPreRun exclusion list

## Cleanup Applied

- Killed root daemon process (PID 2424066)
- Removed `/root/.graphit/` directory
- Verified no root crontab entries for graphit
