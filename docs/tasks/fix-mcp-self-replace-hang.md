# Fix: MCP Proxy Hang After Launcher Update

**Date**: 2026-06-20

## Problem

After the launcher extracts new binaries and updates `launcher.stamp`, both the
daemon and the MCP binary attempt to self-replace. The daemon succeeds. The MCP
binary ends up frozen and unresponsive to the IDE.

## Root Cause

The `watchLauncherStamp` goroutine in `cmd/mcp/main.go` called
`sysutil.ReplaceProcess` (i.e. `syscall.Exec` on Unix) when it detected the
stamp changed. `syscall.Exec` replaces the process image while keeping the same
PID and file descriptors — from the IDE's perspective it looks like the same
process.

However, the new process starts fresh from `main()` and enters
`mcpproxy.RunProxy`. With `firstConnect == true`, it blocks on:

```go
initReq, err = stdioConn.Read(ctx)  // waits for MCP "initialize"
```

The IDE already completed the MCP `initialize` handshake with the old process
and **never resends it**. The new process hangs forever.

## Fix

Removed the `watchLauncherStamp` goroutine entirely from `cmd/mcp/main.go`.

The existing `watchDaemonFiles` mechanism in `internal/mcpproxy/proxy.go`
already handles daemon restarts transparently: it detects when `mcp.port` /
`mcp.key` change, cancels the relay, and reconnects to the new daemon — all
without touching the stdio channel to the IDE. The MCP binary itself (a thin
proxy with no business logic) does not need to be replaced mid-session.

The new MCP binary on disk will be picked up automatically the next time the
IDE starts a new MCP process (e.g. on IDE restart), which is standard behavior.

## Files Modified

- `cmd/mcp/main.go` — removed `versionCheckInterval` const, `bootStamp`
  variable, `watchLauncherStamp` goroutine launch, `watchLauncherStamp`
  function, and unused imports (`"time"`, `internal/sysutil`)
