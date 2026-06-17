# Fix: MCP proxy session re-initialization on daemon reconnect

## Problem

When the graphit daemon restarts (e.g., after a version update via the launcher), the MCP proxy processes that IDEs use lose their HTTP connection to the old daemon. The proxy has a reconnect loop that successfully finds the new daemon's port and API key, but the reconnected session fails because:

1. The new daemon expects an MCP `initialize` handshake before accepting `tools/call` requests
2. The proxy was doing a "blind relay" — it had no awareness of the MCP protocol lifecycle
3. The IDE doesn't know the connection was reset, so it sends `tools/call` directly
4. The daemon rejects with: `"method 'tools/call' is invalid during session initialization"`

## Root Cause

In `internal/mcpproxy/proxy.go`, the `RunProxy` function's reconnect loop would create a new HTTP connection to the daemon and immediately start relaying messages. But the MCP protocol requires a handshake sequence before any tool calls:

```
client → initialize (request)
server → initialize (response)
client → notifications/initialized (notification)
```

The proxy never cached or replayed this handshake on reconnect.

## Fix

Modified `RunProxy` to:

1. **First connect**: intercept and cache the `initialize` request and `notifications/initialized` notification from the IDE, forwarding them to the daemon and storing copies
2. **Reconnect**: before starting the relay, replay the cached `initialize` + `notifications/initialized` to the new daemon, consume the daemon's response (don't forward to IDE since it already got the original), then start normal relay

This makes daemon restarts transparent to the IDE — the proxy handles session re-initialization automatically.

## Files Changed

- `internal/mcpproxy/proxy.go` — added MCP session caching and replay on reconnect
- `cmd/graphit/commands/root.go` — excluded `self-update` from daemon auto-start (related fix)

## Testing

- All existing proxy tests pass (`go test ./internal/mcpproxy/`)
- Build verification: `go build ./internal/mcpproxy/ ./cmd/mcp/ ./cmd/graphit/`
- Manual verification: daemon HTTP endpoint responds correctly to initialize + tools/call
