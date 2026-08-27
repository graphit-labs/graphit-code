Order of lines in equalized two-way channels swaps between rebuilds on the hybrid path

Measured on August 24, 2026 (see `docs/tasks/busca-devolve-so-arquivos-e-index-nao-reconstroi.md`).

The key phrase (`sortResultsDeterministic`) ties scores **TIED** by identity, and this is what makes the keyword channel reproducible across rebuilds. In the hybrid channel, the engine places equal lines in both channels with RRF values **DISTINCT** — `1/(60+rank)`, differing only in the fourth decimal place because it had to order the lines — so the tie-by-identity never engages, and these lines swap between rebuilds.

What is guaranteed today, and tied by
`TestHybridTopResultAndSetAreStableAcrossRebuilds` (`internal/ast/search_scale_test.go`): the result of rank 1 and the SET of results are identical in 8 rebuilds. What is not: the internal order of lines that do not distinguish.

Why was it not corrected: recovering the tie would require deciding in Go that two ranks of the engine are "close enough to be a tie" — rounding the score, or treating differences below an epsilon as ties. This is a ranking policy issue, and this module does not have such a policy (the same reason why 331 lines of `search_fusion.go` were deleted from T14). An epsilon would also distort genuinely different ranks in a long list.

Another route, registered and also rejected at this point: to order the lines in INLINE_7 on ESCRITA, so as to make the physical ordering of the function index of the data. The comment INLINE_8 already rejected it with a valid argument — updates incrementally cause append, therefore the physical ordering of an incremental-rebuilt index diverges from any reconstruction from scratch in any case.

Impact: Real impact is low, and it's worth measuring before investing effort. Affected lines are those that the ranker cannot distinguish. If there’s an instance where this matters (an agent receiving different responses to the same question in two sessions), the concrete case should guide the correction.

---

**Note:** The provided text appears to be a technical specification or guideline related to some form of ranking system, possibly for a chatbot or similar application. The translation aims to convey the essence and intent of the original Portuguese text while maintaining its idiomatic feel.
