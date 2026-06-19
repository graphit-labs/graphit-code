# mcpproxy: daemon restart reconnect

## Problem

When the daemon restarts it writes a new `mcp.port` and `mcp.key`. The proxy had a reconnect loop, but it only re-read those files after `relay()` returned an error. With the TCP connection staying ESTABLISHED after a daemon restart, `httpConn.Read()` blocked forever — the loop never advanced.

## Fix

Added `watchDaemonFiles` in `internal/mcpproxy/proxy.go`. It runs as a goroutine alongside `relay()`, polling `mcp.port` and `mcp.key` every 500ms. When either value changes (or the files disappear mid-restart), it cancels the relay's child context, unblocking the stuck `Read()`. The outer loop then calls `waitForDaemon` again and connects with the new port+key.

The stdio client sees no disruption — the initialize handshake is replayed transparently on reconnect.

## Files changed

- `internal/mcpproxy/proxy.go` — added `watchDaemonFiles`, `watchPollInterval`, `errDaemonRestartedMsg`, `isDaemonRestarted`; wired relay into a child context
- `internal/mcpproxy/proxy_test.go` — added `TestWatchDaemonFiles_{PortChange,KeyChange,MissingFiles,NoChange}`
