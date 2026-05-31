# Fix Daemon Deadlock and Launcher-Aware Respawn

## Summary

Fixed two critical bugs in the daemon's launcher update detection mechanism:

1. **Deadlock in `reconcileProjects`** — the main goroutine froze because `d.log()` shared the same `sync.RWMutex` as `reconcileProjects()`, and Go's RWMutex is not re-entrant.
2. **`EnsureRunning` using wrong binary** — `os.Executable()` returned the old graphit-core path which may have been deleted during upgrade.

## Changes

### `internal/daemon/daemon.go`
- Added `logMu sync.Mutex` field to `Daemon` struct — dedicated lock for log file writes
- `d.log()` now uses `d.logMu` instead of `d.mu`, preventing deadlock when called from `reconcileProjects()` which holds `d.mu.Lock()`
- Cross-platform safe: Windows doesn't guarantee POSIX `O_APPEND` atomicity, so a separate mutex is needed

### `internal/daemon/autostart.go`
- Added `resolveDaemonExe()` helper that prefers `GRAPHIT_LAUNCHER_PATH` env var
- Falls back to `os.Executable()` for non-launcher installations
- The launcher handles runtime extraction and correct version switching during upgrades

## Verification

- `make ci` — ✅ all checks passed (0 errors, 19 pre-existing warnings)
