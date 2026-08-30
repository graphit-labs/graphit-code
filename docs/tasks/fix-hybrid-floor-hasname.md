---
title: Fix missing `hasName` helper in hybrid floor test
type: task
status: complete
created: 2026-08-24
updated: 2026-08-24
---

# Fix `hasName` in the hybrid floor test

## Objective

Compile cleanly both with the default build and with `-tags lancedb`, without changing the Hybrid Search Quality Contract.

## Reasoning

`TestHybridSearchQualityFloor` references `hasName`, but the search locates the declaration in `internal/ast/search_lance_test.go`. The hypothesis to verify is that the declaration is guarded by the `lancedb` build tag, which leaves the floor test visible in the default build without the helper. The fix must place the helper in a build domain shared with all its consumers, so it does not duplicate symbols when the tag is active.

## Plan

- [x] Confirm the existing declaration's tags, consumers, and semantics.
- [x] Move or replace the helper into the smallest correct shared scope.
- [x] Run `internal/ast` without the tag and with `-tags lancedb`.
- [x] Record the result and keep the fix in a separate commit to preserve clean reverse edges.

## Progress

Added by the Engineer after Suite T17 revealed `undefined: hasName` in the default build. Memory and wiki were consulted; the AST found `hasName` declared in `internal/ast/search_lance_test.go:95`, consumed at `internal/ast/search_hybrid_floor_test.go:168`. Root cause confirmed: the declaration was gated behind the `lancedb` build tag, but the consumer was not. The helper was moved to `search_test_helpers_test.go`, without a tag, preserving all seven consumers.

- `go test ./internal/ast -run TestHybridSearchQualityFloor -count=1` compiles and passes/skips correctly on the standard build; it also went green after the reverse-manifest fix.
- `go test -tags lancedb ./internal/ast ./internal/hub -run 'TestHybridSearchQualityFloor|TestPrepareASTPublishProducesOnlyIcebug|TestIcebugArtifactMountsAndAnswers' -count=1` passes; both the variant with the original helper and the variant without the tag now compile.
- Full suite tagged `lancedb` run across the following packages, with `-count=1`, at commit `42cc1af`:

```bash
go test -tags lancedb ./internal/config ./internal/ladybugstore ./internal/ast
```
