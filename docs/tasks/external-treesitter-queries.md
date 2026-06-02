# External Tree-sitter Queries

## Summary

Externalized all tree-sitter query patterns to YAML files with a three-level
resolution chain: **project > user global > runtime**.

## Date

2026-06-01

## Changes

### Architecture — 3-level resolution

```
.graphit/ast/queries/                           ← projeto (máxima prioridade)
   ↓
~/.graphit/ast/queries/                         ← user global (customizações)
   ↓
~/.graphit/runtime/<version>/ast/queries/       ← runtime (extraído pelo launcher)
```

- **Runtime**: extraído pelo launcher durante setup do binário, sobrescrito a cada versão
- **User global**: nunca tocado pelo framework — só o usuário edita
- **Projeto**: sobrescreve tudo para aquele projeto

### New Files

- `internal/ast/queries/*.yaml` — 16 YAML (javascript, typescript, tsx, csharp, php, go, sql, python, java, rust, c, cpp, kotlin, ruby, swift, dart)
- `internal/ast/query_loader.go` — carregamento, resolução, caching
- `internal/ast/query_loader_test.go` — 18 testes

### Modified Files

- `internal/brand/brand.go` — `RuntimeDir(version)` retorna `~/.graphit/runtime/<version>/`
- `internal/ast/treesitter_adapter.go` — `projectDir` + merge dinâmico
- `internal/ast/pipeline.go` — passa project root
- `internal/ast/prescan.go` — aceita project dir

### Key Functions

| Função | Descrição |
|--------|-----------|
| `LoadExternalQueries()` | Carrega de `.graphit/ast/queries/` (projeto) |
| `LoadUserQueries()` | Carrega de `~/.graphit/ast/queries/` (user global) |
| `LoadRuntimeQueries()` | Carrega de `~/.graphit/runtime/<version>/ast/queries/` |
| `resolveQueriesForLang()` | Cadeia: project > user > runtime |
| `mergedQueriesFor()` | Resolve + valida + cache (thread-safe) |

## Verification

- 18 unit tests pass
- `make ci` passes
