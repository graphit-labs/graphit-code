# Fix: Daemon Self-Replacement with Same PID on All Platforms

**Date**: 2026-06-19  
**Status**: Done

## Problem

When a new launcher is installed and the daemon detects the stamp change, the old daemon was:
1. Calling `EnsureRunning()` which spawned a new child process with a **different PID**
2. The old daemon's `ctx.Done()` goroutine was removing `mcp.port` and `mcp.key` files **after** the new daemon had already written its own port/key to those files
3. This caused the MCP proxy to hang — it read stale/deleted port/key files and timed out

Additionally, the Windows behavior always created a new PID on replacement, which was incorrect.

## Root Cause

Race condition: old daemon's cleanup goroutine (`ctx.Done()` path) deleted the MCP port/key files written by the new (successor) daemon. The files belonged to the new daemon, but the old one deleted them unconditionally.

Additionally, the daemon used `exec.Command` (new PID) instead of `sysutil.ReplaceProcess` (same PID on Unix).

## Solution

### `internal/daemon/daemon.go`
- Added `ErrReplace` sentinel error returned when stamp changes
- Added `SkipPIDFile bool` to `Config` for Windows relay workers
- Removed `pidHandedOff` field (no longer needed)
- Changed stamp-change path: `shutdown()` → `return ErrReplace` (no child spawn)
- PID file always removed via simple `defer d.pid.Remove()` on non-SkipPIDFile path

### `cmd/graphit/commands/daemon.go`

**Unix/macOS** (`runDaemonUnix`):
- Calls `runDaemonCore()` once
- On `ErrReplace`: closes MCP listener/files, then calls `sysutil.ReplaceProcess` → `syscall.Exec` → **same PID**, new binary

**Windows** (`runDaemonWindowsRelay`):
- Writes PID file once with the relay's own PID
- Runs `runDaemonCore()` in a loop
- On `ErrReplace`: closes MCP, loops → `runDaemonCore()` restarts in the **same OS process** → **same PID**
- The OS process (and PID file) never changes across restarts

**`runDaemonCore`** (shared):
- Returns `(closeMCP func(), err error)`
- MCP cleanup via `sync.Once` + mutex: idempotent, called by ctx.Done() on normal shutdown OR by caller on ErrReplace
- `cfg.SkipPIDFile = true` on Windows (relay owns the PID file)

## Files Changed

- `internal/daemon/daemon.go` — ErrReplace, SkipPIDFile, removed pidHandedOff
- `internal/daemon/daemon_extra_test.go` — updated comment
- `cmd/graphit/commands/daemon.go` — relay loop, Unix exec, MCP sync.Once cleanup

## Behavior After Fix

| Platform | Mechanism | PID Changes? |
|---|---|---|
| Unix/macOS | `syscall.Exec` | **No** (same PID) |
| Windows | Relay loop in same OS process | **No** (same PID) |

The MCP proxy reconnects cleanly because:
1. `closeMCP()` is called before exec/restart (removes port/key files while they're still ours)
2. New daemon starts fresh, writes new port/key
3. No race condition possible
