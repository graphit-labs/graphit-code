---
title: "Universal ai_optimized Parameter"
status: completed
created: 2026-06-16
---

# Universal `ai_optimized` Parameter

## Summary

Added the `ai_optimized` parameter to **all MCP tools** that return structured JSON data. When set to `true`, the response uses compact TOON (Token-Optimized Object Notation) format instead of verbose JSON, reducing token consumption by ~60-80% for AI agents.

## Changes

### New Package: `internal/toon`

- Created generic `FormatAny()` function using Go reflection
- Handles slices of structs (tabular pipe-delimited), single structs (key:value), maps, string slices
- Sanitizes pipe characters and newlines in string values
- Truncates long strings to 200 chars
- 13 unit tests

### Server Helper

- Added `toonResult(v any)` helper in `internal/mcpstdio/server.go`
- Wraps `toon.FormatAny()` → `textResult()` for consistent output

### Tool Files Modified (9 files)

| File | Structs | Handlers |
|------|---------|----------|
| `tools_ast.go` | +4 (6 total) | +4 (8 total) |
| `tools_wiki.go` | existing (4) | existing (5) |
| `tools_knowledge.go` | +4 | +4 |
| `tools_memory.go` | +5 | +5 |
| `tools_hub.go` | +6 | +6 |
| `tools_cluster.go` | +2 | +2 |
| `tools_dream.go` | +4 | +4 |
| `tools_lifecycle.go` | +1 | +1 |
| `tools_daemon.go` | +1 | +2 |

### Not Modified (text-only output)

- `tools_improvements.go` — only returns `textResult()`

## TOON Format Examples

```
# Slice of structs
results[3]{id|name|score}:
  abc|Alpha|0.95
  def|Beta|0.80
  ghi|Gamma|0.65

# Single struct
{running:true|pid:1234|status:active}

# String slice
items[3]:
  foo
  bar
  baz
```

## Verification

- `go build ./...` — clean
- `go test ./internal/toon/...` — 13/13 pass
- `go test ./internal/mcpstdio/...` — all pass
