# Task: Remove Hardcoded Fallbacks & Dynamic Query Types

**Date:** 2026-06-02
**Status:** Completed

## What was done

Three interconnected changes to complete the YAML-driven externalization:

### Item 4 — Import Match Types
- Added `contains`, `suffix`, `regex` support to import matching in `enrichment.go`
- Regex patterns are compiled once at lookup build time via `compileImportRegex()`
- Total match types now: `prefix` (default), `exact`, `contains`, `suffix`, `regex`

### Item 2 — Dynamic Entity/Relation Discrimination
- Added `type` and `relation_type` fields to `ExternalQueryDef` and `tsQueryDef`
- Updated 15 of 16 YAML files (SQL has no relations) with `type: relation` annotations
- Replaced ALL hardcoded dataKey checks (`"calls"`, `"heritage"`, etc.) with dynamic lookup
- New functions: `buildRelationTypeMap()`, `processRelations()`
- Updated `attachDecorators()` and `detectExports()` to use the dynamic map
- New relation types can now be added via YAML without recompilation

### Item 3 — Remove Hardcoded Fallback Queries
- Replaced 1200-line `treeSitterLangs` with 16-line `treeSitterGrammars` map
- `treesitter_adapter.go`: 1921 → 748 lines (**-1173 lines, -61%**)
- Removed `Queries` field from `tsLangConfig`
- `init()` now builds `tsExtMap` from embedded YAML extensions
- Removed `builtIn` parameter from `mergedQueriesFor()`
- YAML is now the **sole source** of queries — no Go fallback

## Files modified

- `internal/ast/enrichment.go` — import match types
- `internal/ast/query_loader.go` — ExternalQueryDef struct + mergedQueriesFor
- `internal/ast/treesitter_adapter.go` — major refactor (-1173 lines)
- `internal/ast/queries/*.yaml` — 15 files annotated with type/relation_type

## Verification

- `go build ./...` ✅
- `go test ./internal/ast/... -v` ✅ (all tests pass)
