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

- **Runtime**: extracted by the launcher during binary setup, overwritten on every version
- **User global**: never touched by the framework — only the user edits it
- **Project**: overrides everything for that project

### New Files

- `internal/ast/queries/*.yaml` — 16 YAML (javascript, typescript, tsx, csharp, php, go, sql, python, java, rust, c, cpp, kotlin, ruby, swift, dart)
- `internal/ast/query_loader.go` — loading, resolution, caching
- `internal/ast/query_loader_test.go` — 18 tests

### Modified Files

- `internal/brand/brand.go` — `RuntimeDir(version)` returns `~/.graphit/runtime/<version>/`
- `internal/ast/treesitter_adapter.go` — `projectDir` + dynamic merge
- `internal/ast/pipeline.go` — passes the project root
- `internal/ast/prescan.go` — accepts project dir

### Key Functions

| Function | Description |
|--------|-----------|
| `LoadExternalQueries()` | Loads from `.graphit/ast/queries/` (project) |
| `LoadUserQueries()` | Loads from `~/.graphit/ast/queries/` (user global) |
| `LoadRuntimeQueries()` | Loads from `~/.graphit/runtime/<version>/ast/queries/` |
| `resolveQueriesForLang()` | Chain: project > user > runtime |
| `mergedQueriesFor()` | Resolves + validates + caches (thread-safe) |

## Verification

- 18 unit tests pass
- `make ci` passes
