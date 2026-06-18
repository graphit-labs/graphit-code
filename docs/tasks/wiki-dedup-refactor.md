# Wiki/Knowledge/Memory Refactor — Eliminar Duplicação

**Date:** 2026-06-18  
**Status:** Done

## Problema

Os módulos `knowledge` e `memory` continham cópias exatas de funções que pertencem ao módulo `wiki` (a biblioteca compartilhada). Cada nova melhoria precisava ser feita em dois lugares.

## O que foi movido para `internal/wiki`

### `internal/wiki/helpers.go` [NOVO]

| Função | Origem (removida) |
|--------|------------------|
| `SafeSlug(name string) string` | `knowledge.safeFilename` + `memory.safeMemFilename` — idênticas |
| `UniqueSlug(base string, used map[string]bool) string` | `knowledge.uniqueKSlug` + `memory.uniqueMemSlug` — idênticas |
| `ReadFrontmatterField(path, field string) string` | `knowledge.readKnowledgeFrontmatterField` + `memory.readFrontmatterField` — idênticas |
| `StripFrontmatter(content string) string` | `knowledge.stripFrontmatter` — deveria ser compartilhado |

### `internal/wiki/fastpath.go` [NOVO]

| Tipo/Função | Descrição |
|-------------|-----------|
| `DocHashEntry` | Struct com `CacheKey`, `ContentHash`, `Slug` |
| `FastPathCheck(wikiDir, entries, cache)` | Encapsula o padrão repetido: DB existe + todos hashes no cache + sem deleções |

## O que foi simplificado

### `internal/knowledge/wiki.go`
- 4 funções privadas removidas (corpo substituído por thin wrappers de 1 linha)
- Fast-path inline de ~50 linhas → `wiki.FastPathCheck` + fallback compacto
- Import `"unicode"` removido

### `internal/memory/wiki.go`
- 3 funções privadas removidas (corpo substituído por thin wrappers de 1 linha)
- Fast-path inline de ~47 linhas → `wiki.FastPathCheck` (15 linhas)
- Import `"unicode"` removido

## Verificação

```
go build ./internal/wiki/... ./internal/knowledge/... ./internal/memory/... — OK
go test ./internal/wiki/... ./internal/knowledge/... ./internal/memory/... — PASS
  wiki:      0.016s
  knowledge: 0.075s
  memory:    28.4s
```
