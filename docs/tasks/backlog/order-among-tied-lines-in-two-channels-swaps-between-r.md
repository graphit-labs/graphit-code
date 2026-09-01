# Order among tied lines in both channels permutes between rebuilds on the hybrid path

Measured on 2026-08-24 (see `docs/tasks/search-returns-only-files-and-index-not-rebuilt.md`).

`sortResultsDeterministic` (`internal/ast/search_common.go`) breaks ties between **EQUAL** scores by
identity, and that is what makes the keyword channel reproducible across rebuilds. In the hybrid
channel the engine gives lines tied in both channels **DISTINCT** RRF values — `1/(60+rank)`,
differing in the fourth decimal place only because it had to put the lines in some order — so the
tie-break by identity never engages, and those lines permute between rebuilds.

What IS guaranteed today, and is locked down by
`TestHybridTopResultAndSetAreStableAcrossRebuilds` (`internal/ast/search_scale_test.go`): the rank 1
result and the SET of results are identical across 8 rebuilds. What is not: the internal order of
lines that nothing distinguishes.

Why it was not fixed: recovering the tie-break would require deciding in Go that two ranks from the
engine are "close enough to be a tie" — rounding the score, or treating differences below an epsilon
as a tie. That is ranking policy, and this module does not have any (it is the same reason the 331
lines of `search_fusion.go` were deleted in T14). An epsilon would also distort genuinely different
adjacent ranks in a long list.

Another route, recorded and also rejected for now: sorting the lines at WRITE time, in
`RebuildFromCache`, to make the physical order of the index a function of the data. The comment on
`sortResultsDeterministic` already rejected this with a valid argument — incremental updates append,
so the physical order of an incrementally updated index diverges from one rebuilt from scratch
anyway.

Real impact: low, and worth measuring before spending effort. The affected lines are, by
construction, the ones the ranker cannot distinguish. If a case shows up where this matters (an
agent getting a different answer to the same question in two sessions), the concrete case is what
should guide the fix.
