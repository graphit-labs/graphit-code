# Fix MCP Proxy Reconnection on Daemon Death

## Problem
When the daemon dies unexpectedly (crash, SIGKILL), the MCP proxy enters an infinite
reconnect loop because:
1. `mcp.port` and `mcp.key` files remain on disk (stale)
2. `waitForDaemon()` trusts these files without validating liveness
3. `connectHTTP()` failure path doesn't call `EnsureDaemon()`

## Changes

### `internal/mcpproxy/proxy.go`
- `waitForDaemon()`: Added TCP health check (`isPortAlive`) before returning port/key.
  Stale files are removed if the port is unreachable, allowing `EnsureDaemon()` to start fresh.
- `RunProxy()` connect failure path: Now removes stale files and calls `EnsureDaemon()`
  before retrying, breaking the infinite loop.
- Added `isPortAlive()` helper using `net.DialTimeout`.

### `internal/mcpproxy/proxy_test.go` [NEW]
- Tests for `isPortAlive` (live vs dead port)
- Tests for `waitForDaemon` with stale files (validates cleanup + EnsureDaemon calls)
- Tests for `waitForDaemon` with live daemon (validates happy path)

### `cmd/graphit/commands/daemon.go`
- `daemon stop` SIGKILL path: Now cleans up `mcp.port` and `mcp.key` alongside PID file.
- `daemon restart` SIGKILL path: Same cleanup.

## Status
- [x] Build passes
- [x] All existing tests pass
- [x] New tests pass (4/4)
