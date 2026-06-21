# Fix: Daemon Self-Replacement via Spawn-and-Exit (All Platforms)

**Date**: 2026-06-21
**Status**: Done

## Problem

The old daemon self-replacement used two mechanisms:
- **Unix/macOS**: `syscall.Exec` (same PID) via `sysutil.ReplaceProcess`
- **Windows**: a relay loop in the same OS process (same PID) — a workaround because Windows has no `execve`

The relay approach on Windows was complex, fragile, and carried dead code (`GRAPHIT_RELAY_WORKER` env var, `relayExitCode=42`, `runChild`, `ensureWorkerEnv`, `isRelayWorker`, `RunLoop`, `SkipPIDFile`).

## Solution: Spawn-and-Exit (All Platforms)

When the daemon detects a stamp change, it now:
1. Shuts down project supervisors
2. Closes MCP (removes port/key files)
3. Returns `ErrReplace` to the caller (`runDaemonStart`)
4. Caller closes MCP, then **spawns the new daemon as a detached child process** (new PID)
5. Old daemon exits cleanly

### Why it's safe
- `defer d.pid.Remove()` runs inside `daemon.Start()` before returning `ErrReplace` — at this point the new daemon hasn't been spawned yet, so there's no race
- `IsAlive()` handles stale PID files from crashes: checks process liveness and removes the file if dead

## Cross-Platform PID Liveness Check

`Signal(0)` doesn't work on Windows. `pidIsAlive()` is now split by platform:

| Platform | Mechanism |
|---|---|
| Unix/macOS | `proc.Signal(syscall.Signal(0))` |
| Windows | `OpenProcess` + `GetExitCodeProcess` (STILL_ACTIVE = 259) |

## Files Changed

### Deleted
- `internal/sysutil/run_loop.go` — `relayWorkerEnv`, `ErrReplace`, `EffectivePID`
- `internal/sysutil/run_loop_unix.go` — `RunLoop` (Unix)
- `internal/sysutil/run_loop_windows.go` — `RunLoop` (Windows relay)

### Modified
- `internal/sysutil/exec_unix.go` — kept `ReplaceProcess`+`SanitizeInheritedFDs` (used by launcher)
- `internal/sysutil/exec_windows.go` — `ReplaceProcess` now spawn+wait+exit, no relay loop
- `internal/sysutil/sysutil_test.go` — removed relay tests, keep `DetachProcess` test
- `internal/daemon/pidfile.go` — removed `syscall` dep; `IsAlive()` delegates to platform-specific `pidIsAlive()`; consolidated `Signal`/`SignalOS` into single `Signal(os.Signal)`
- `internal/daemon/pidfile_unix.go` — **[NEW]** `pidIsAlive` via `Signal(0)`
- `internal/daemon/pidfile_windows.go` — **[NEW]** `pidIsAlive` via `OpenProcess`+`GetExitCodeProcess`
- `internal/daemon/daemon.go` — removed `SkipPIDFile`; plain `defer d.pid.Remove()`
- `cmd/graphit/commands/daemon.go` — `runDaemonStart` uses `spawnDetachedDaemon` (new); removed `sysutil.ReplaceProcess`; removed Windows `SkipPIDFile` branch
- `cmd/graphit/commands/lifecycle.go` — `SignalOS` → `Signal`
- `internal/daemon/pidfile_test.go` — removed `SignalOS` tests; `Signal(0)` → `Signal(syscall.Signal(0))`
- `internal/daemon/pidfile_extra_test.go` — removed `SignalOS` tests
- `go.mod` — promoted `golang.org/x/sys` to direct dependency (used by `pidfile_windows.go`)

## Behavior After Fix

| Platform | Mechanism | PID Changes? |
|---|---|---|
| Unix/macOS | spawn detached + exit | **Yes** (new PID) |
| Windows | spawn detached + exit | **Yes** (new PID) |

PID changes are acceptable: `IsAlive()` handles stale files, and the daemon's own "already running" check prevents duplicates.
