Wiki/Knowledge/Refactor - Remove Duplication

**Date:** 2026-06-18  
**Status:** Done

## Problema

The modules `knowledge` and `memory` contained exact copies of functions that belonged to the module `wiki` (the shared library). Each new improvement had to be made in two places.

## O que foi movido para `internal/wiki`

### `internal/wiki/helpers.go` [NOVO]

Function | Source (removed)
|--------|------------------|
The elements in the set are identical.
The two expressions are identical.
The two expressions are identical.
| `StripFrontmatter(content string) string` | `knowledge.stripFrontmatter` — deveria ser compartilhado |

### `internal/wiki/fastpath.go` [NOVO]

Type/Function | Description
|-------------|-----------|
| `DocHashEntry` | Struct com `CacheKey`, `ContentHash`, `Slug` |
Here's the Portuguese text translated into idiomatic English:

"Encapsulates the repeated pattern: DB exists + all hashes in cache + without deletions"

This translation maintains the meaning of the original Portuguese text while rendering it in a more natural English phrasing.

## O que foi simplificado

### `internal/knowledge/wiki.go`
Four private functions have been removed (the body replaced by thin wrappers of one line)
- Fast-path inline de ~50 linhas → `wiki.FastPathCheck` + fallback compacto
- Import `"unicode"` removido

### `internal/memory/wiki.go`
Three private functions have been removed (the body replaced by thin wrapper classes of one line)
- Fast-path inline de ~47 linhas → `wiki.FastPathCheck` (15 linhas)
- Import `"unicode"` removido

Verification

```
go build ./internal/wiki/... ./internal/knowledge/... ./internal/memory/... — OK
go test ./internal/wiki/... ./internal/knowledge/... ./internal/memory/... — PASS
  wiki:      0.016s
  knowledge: 0.075s
  memory:    28.4s
```
