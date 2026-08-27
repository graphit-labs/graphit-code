---
Title: Helper 'hasName' is absent in hybrid floor test correction
type: task
status: complete
created: 2026-08-24
updated: 2026-08-24
---

Correct `hasName` in the hybrid floor test

## Objetivo

To compile both with the default build and with `-tags lancedb`, without changing the
Hybrid Search Quality Contract.

Reasoning

``TestHybridSearchQualityFloor` references `hasName`, while the search locates the declaration`
In **INLINE_0**, the hypothesis to be verified is that the declaration is protected by the tag
``lancedb``, leaving the floor test visible in the default build without the helper. The correction should place ```
The helper operates within the same domain of build as consumers, without duplicating symbols when the tag is active.

## Plano

Confirming existing declaration's tags, consumers, and semantics.
- [x] Mover ou substituir o helper no menor escopo compartilhado correto.
- [x] Rodar `internal/ast` sem tag e com `-tags lancedb`.
[x] Register the result and maintain the correction in a separate commit for reverse edges.

## Progresso

Added by the Engineer after Suite T17 revealed `undefined: hasName` in the default build.
Memory and wiki consulted; the AST found `hasName` in `internal/ast/search_lance_test.go:95`.
  consumidor em `internal/ast/search_hybrid_floor_test.go:168`.
Confirmation Cause: The statement was under INLINE 0, but the consumer did not. The helper
  foi movido para `search_test_helpers_test.go`, sem tag, preservando os sete consumidores.
- `go test ./internal/ast -run TestHybridSearchQualityFloor -count=1` compila e passa/skip corretamente
No standard build; INLINE_0 also turned green after the reverse manifest correction.
- `go test -tags lancedb ./internal/ast ./internal/hub -run
  'TestHybridSearchQualityFloor|TestPrepareASTPublishProducesOnlyIcebug|TestIcebugArtifactMountsAndAnswers'
  -count=1` verde; a variante com o helper original e a variante sem tag agora compilam.
The complete suite is running with tests tagged as "lancedb" in the following directories:

```bash
go test -tags lancedb ./internal/config ./internal/ladybugstore ./internal/ast
```
The internal hub with a count of 1 is using inline 0 with the value `42cc1af`.
