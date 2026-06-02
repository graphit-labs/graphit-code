# Task: Add Unit Tests for Untested Modules

## Summary

Added unit tests for four internal modules that had zero or shallow test coverage:
- `internal/slogutil/` — NEW: `slogutil_test.go`
- `internal/wikisvc/` — NEW: `wikisvc_test.go`
- `internal/mcpstdio/` — NEW: `tools_test.go`
- `internal/uiserver/` — EXPANDED: `uiserver_test.go`

## Changes

### `internal/slogutil/slogutil_test.go` (NEW)
- Tests for `NOP()`, `Stderr()`, and `Resolve()` functions
- Tests for `discardHandler` methods: `Enabled`, `Handle`, `WithAttrs`, `WithGroup`
- Verifies NOP logger truly discards output
- Verifies Stderr logger operates at Debug level
- Table-driven tests for `Resolve()` nil/non-nil input

### `internal/wikisvc/wikisvc_test.go` (NEW)
- Tests for `NewWikiService` constructor
- Tests for `ResolveWikiSource` with "project" and "memory" names
- Error cases: missing wiki directories
- Wiki subdirectory fallback (`wiki/` subdir resolution)
- `ResolveSources` with valid, mixed, and empty inputs
- Uses `t.TempDir()` for all filesystem operations

### `internal/mcpstdio/tools_test.go` (NEW)
- Tests for helper functions: `textResult`, `errResult`, `jsonResult`
- `safeTool` panic recovery verification
- `sanitizeContextName` path traversal protection (table-driven)
- `resolveProjectDir` validation (empty, nonexistent, valid)
- `withProjectDir` chdir/restore and error propagation
- `splitLastNLocal` log splitting (table-driven)
- `scopeFromString` boolean conversion
- `nopWriteCloser` Write/Close behavior

### `internal/uiserver/uiserver_test.go` (EXPANDED)
- `extractPageMeta`: page types (index, log, community, god-node, entity), tags, source, confidence, word count, nested paths
- `listWikiPages`: sorting order, nested directories, empty directories
- `countMarkdownFiles`: recursive counting, empty dirs
- `resolveDir`: symlink resolution, non-existent paths
- `isAllowedOrigin`: localhost variants, disallowed origins (table-driven)
- `corsJSON` middleware: Content-Type, security headers, CORS, OPTIONS
- `writeJSON` output validation
- HTTP handlers: pages, page, search, sessions, hub-knowledge, ai-search, multi-search
- Path traversal protection in `handlePage`
- Error cases: missing parameters, nil AI client, nil hub service

## Testing

- All tests pass with `go test -race -count=1`
- No new lint issues introduced (4 pre-existing lint issues unrelated to this change)
- `make ci` lint failures are pre-existing in `mcpserver_test.go` and `daemon_test.go`
