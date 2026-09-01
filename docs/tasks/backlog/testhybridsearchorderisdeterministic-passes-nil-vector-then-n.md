# TestHybridSearchOrderIsDeterministic passes a nil vector, so it never tested the hybrid path

`internal/ast/search_determinism_test.go`, `TestHybridSearchOrderIsDeterministic`, calls
`si.HybridSearch(context.Background(), q, nil, 10)` — with a **nil** vector. `HybridSearch` degrades
to `Search` when `len(vec) == 0`, so the test measures the keyword path, exactly the same one as
`TestSearchOrderIsDeterministic` right above it. The fusion channel was never exercised by it.

This was discovered on 2026-08-24 while fixing the score scale defect between the entity and the
file passes (see `docs/tasks/search-returns-only-files-and-index-not-rebuilt.md`). The real hybrid
path ended up covered by
`TestHybridTopResultAndSetAreStableAcrossRebuilds` in `internal/ast/search_scale_test.go`, which
builds the index with real embeddings.

What to do: the name lies and will mislead anyone who trusts it into concluding that the hybrid is
covered. Either rename it to say it is the degraded path (which is ALSO worth testing — degrading to
keyword when there is no vector is real behavior), or give it an `embLookup` and a query vector and
make it measure what the name promises. If it is the second, watch out for the trap recorded in the
task log: giving the SAME embedding to every entity makes the vector channel pure noise with maximum
confidence, and the test flaps.

Cost: small. Risk of not doing it: someone removes or weakens the new gate believing that this one
already covers the hybrid.
