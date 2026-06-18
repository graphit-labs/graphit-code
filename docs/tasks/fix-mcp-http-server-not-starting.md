# Fix: MCP HTTP Server Not Starting After Daemon Upgrade

## Date
2026-06-17

## Problem
After the launcher installs a new version and the old daemon shuts down, the new daemon starts but its MCP HTTP server never starts. The IDE's MCP stdio proxy processes hang forever waiting for an HTTP endpoint that never appears.

## Root Cause
In `cmd/graphit/commands/daemon.go`, the MCP HTTP server startup was guarded by:

```go
pidCheck := daemon.NewPIDFile()
if pidCheck.IsAlive() == nil {
    // start MCP HTTP server
}
```

This checks "if NO daemon is alive, start MCP". But by the time this code runs, the daemon has already written its own PID file via `d.pid.Write()` in `internal/daemon/daemon.go:110`. So `IsAlive()` finds the daemon's own PID, returns non-nil, and the MCP HTTP server is silently skipped.

The guard was redundant because `daemon.Start()` already has its own PID-based exclusion logic that prevents multiple daemons.

## Fix
Removed the `pidCheck.IsAlive() == nil` guard entirely. The MCP HTTP server now always starts unconditionally when `runDaemonStart()` is called.

## Files Changed
- `cmd/graphit/commands/daemon.go` — Removed self-defeating PID check guard around MCP HTTP server startup

## Verification
- `make ci` passes with 0 errors
