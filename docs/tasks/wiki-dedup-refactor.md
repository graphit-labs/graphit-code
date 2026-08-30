Wiki/Knowledge/Refactor - Remove Duplication

**Date:** 2026-06-18  
**Status:** Done

## Problem

The modules `knowledge` and `memory` contained exact copies of functions that belonged to the module `wiki` (the shared library). Each new improvement had to be made in two places.

## What was moved to `internal/wiki`

### `internal/wiki/helpers.go` [NEW]

| Function | Source (removed) |
|--------|------------------|
| `StripFrontmatter(content string) string` | `knowledge.stripFrontmatter` — should be shared |

### `internal/wiki/fastpath.go` [NEW]

| Type/Function | Description |
|-------------|-----------|
| `DocHashEntry` | Struct with `CacheKey`, `ContentHash`, `Slug` |
| `FastPathCheck` | Encapsulates the repeated pattern: DB exists + all hashes in cache + no deletions |

## What was simplified

### `internal/knowledge/wiki.go`
Four private functions have been removed (the body replaced by a thin one-line wrapper)
- ~50-line inline fast path → `wiki.FastPathCheck` + a compact fallback
- `"unicode"` import removed

### `internal/memory/wiki.go`
Three private functions have been removed (the body replaced by a thin one-line wrapper)
- ~47-line inline fast path → `wiki.FastPathCheck` (15 lines)
- `"unicode"` import removed

## Verification

```
go build ./internal/wiki/... ./internal/knowledge/... ./internal/memory/... — OK
go test ./internal/wiki/... ./internal/knowledge/... ./internal/memory/... — PASS
  wiki:      0.016s
  knowledge: 0.075s
  memory:    28.4s
```
