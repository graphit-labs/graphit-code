# External Tree-sitter Queries

## Summary

Externalized all tree-sitter query patterns to YAML files with a three-level
resolution chain: **project > user global > runtime**.

## Date

2026-06-01

## Changes

### Architecture — 3-level resolution

```
project (highest priority)
   ↓
User Global (Customizations): ~/.graphit/ast/queries
   ↓
Here's the translation:

"~/.graphit/runtime/<version>/ast/queries  ← runtime (extracted by launcher)"
```

- **Runtime**: extracted by the launcher during binary setup, overwritten with each version
- **User global**: never touched by the framework—only the user edits
- **Project**: overwrites everything for that project

### New Files

- `internal/ast/queries/*.yaml` - 16 YAML (javascript, typescript, tsx, csharp, php, go, sql, python, java, rust, c, cpp, kotlin, ruby, swift, dart)
- `internal/ast/query_loader.go` - loading, resolution, caching
- `internal/ast/query_loader_test.go` - 18 tests

### Modified Files

- `internal/brand/brand.go` returns `RuntimeDir(version)`
- `internal/ast/treesitter_adapter.go` concatenates `projectDir` with dynamic merge
- `internal/ast/pipeline.go` passes the project root
- `internal/ast/prescan.go` accepts project directory

### Key Functions

| Function | Description |
| -------- | ------------ |
| `LoadExternalQueries()` | Loads from `.graphit/ast/queries/` (project) |
| `LoadUserQueries()` | Loads from `~/.graphit/ast/queries/` (user global) |
| `LoadRuntimeQueries()` | Loads from `~/.graphit/runtime/<version>/ast/queries/` |
| `resolveQueriesForLang()` | Chain: project > user > runtime |
| `mergedQueriesFor()` | Resolves + validates + caches (thread-safe) |

The function loads data from the specified source. The first three functions load data from a project, while the fourth function loads data from a user global context. The fifth function chains together these sources to form a single chain: project > user > runtime. The sixth function ensures thread safety by resolving, validating, and caching the loaded data.

## Verification

- 18 unit tests pass
- `make ci` passes
