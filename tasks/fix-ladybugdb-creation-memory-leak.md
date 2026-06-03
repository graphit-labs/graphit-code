# Fix LadybugDB Creation & Memory Leak on WSL

**Date**: 2026-06-03
**Status**: ✅ Complete

## Problem

On WSL Ubuntu 24.04:
1. LadybugDB directory not created during indexing — only the `.search.sqlite` file appeared
2. Memory leak causing WSL to crash after extended use

## Root Causes

### DB Not Created
- CGO errors in `lbug.OpenDatabase()` / `lbug.OpenConnection()` were silently swallowed by `sync.Once`
- No logging existed to show why the database failed to initialize

### Memory Leak (5 sources)
1. `QueryService.searchIndex` (SQLite handle) leaked in `handleSearch()` — missing `Close()`
2. `dbForContext()` created new CGO KuzuDB per HTTP request — rapid open/close fragmented WSL memory
3. `handleContexts()` opened multiple temporary DBs per request for counting
4. `handleFile()` created uncached DBs for context lookups
5. `Shutdown()` opened a second CGO connection just for CHECKPOINT

## Changes

| File | Change |
|------|--------|
| `internal/ast/server.go` | Added DB connection cache with 5-min TTL, eviction goroutine, and `closeDBCache()` on shutdown. Replaced all per-request DB creation with cached lookups. Added `defer qs.Close()` in `handleSearch()`. Simplified `dbForContext()` signature. |
| `internal/ast/ladybug.go` | Added `slog.Error()` logging in `connect()` and `ensureConnected()`. Fixed `Shutdown()` to reuse existing connection. Added `db.Close()` on connection failure. |
| `internal/ast/json_rebuild.go` | Added error logging when temp DB connect fails during rebuild. |
| `internal/daemon/syncmodule.go` | Added error logging for `CreateGraphSchema()` and `RunPipeline()` failures. |

## Verification

- ✅ `go build ./...`
- ✅ `go test ./internal/ast/... ./internal/daemon/...`
- ✅ `make ci`
- ✅ `graphit_sync`
