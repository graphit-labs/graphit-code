The `TestHybridSearchOrderIsDeterministic` passes an empty vector, so it has never tested the hybrid path.

__INLINE_0__, __INLINE_1__, calls
__INLINE_2__ — with vector **nil**. __INLINE_3__ degrades to __INLINE_4__ when __INLINE_5__. Then the test measures the path of the keyword, exactly as `TestSearchOrderIsDeterministic` above it does. The fusion channel was never exercised by him.

It was discovered on August 24, 2026, after correcting the score scale defect between entity pastes and file pastes (see `docs/tasks/busca-devolve-so-arquivos-e-index-nao-reconstroi.md`). The hybrid path was covered by `TestHybridTopResultAndSetAreStableAcrossRebuilds` in `internal/ast/search_scale_test.go`, which constructs the index using true embeddings.

What to do: the name deceives someone who trusts him into believing that the hybrid is covered. Or rename to say it's the degraded path (which also applies — degrading for a key when there isn't a vector is real behavior), or give him an INLINE_10 and a query vector and make him measure what the name promises. If it’s the second, be careful of the registered trap in the task log: giving the same embedding to all entities makes the vector channel pure noise with maximum confidence, and the test fails.

Cost: small. Risk of not doing: someone removes or weakens the new gate, believing that it already covers the hybrid.
