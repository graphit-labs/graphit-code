# Native LadybugDB vector measured: the real gain from consolidation

**Date:** 2026-07-26
**Scope:** `internal/ast/ladybug_vector_test.go` (new), `internal/ast/abbrev_semantic_test.go` (fix)
**Origin:** Engineer's question — whether semantic covers the hard cases, consolidation
into LadybugDB is worthwhile, and whether it supports native vectors

---

## WHAT was measured

`TestLadybugVectorIndex` proves the half that no prior measurement touched:

| probe | result |
|---|---|
| `vector` extension loads | ✅ |
| `FLOAT[768]` column (width of `ai.EmbeddingDimensions`) | ✅ |
| `[]float32` binds as query parameter | ✅ (no 768-element literal) |
| nearest-neighbor ranking | ✅ `loadUserConfig` ranked 1st |
| **DELETE reflected without rebuild** | ✅ leaves the index |
| **INSERT reflected without rebuild** | ✅ new vector becomes 1st |

## WHY this changes the decision

`sqlite-vec` (vec0) allocates fixed 1024-row chunks and **never reclaims space on
delete** — it only marks a validity bit. That's exactly why `RebuildFromCache`
cannot update in place and must write a new file and rename (comment in
`fts_sqlite.go:140-147`). Ladybug's vector index updates in-place in both
directions, so that entire workaround disappears.

Second effect, not anticipated: **this same day's trigram change made migration
cheaper.** The only FTS5 resource without a Ladybug equivalent was the native
`trigram` tokenizer. With the precomputed trigram bag, the dependency became
word tokenizer over a common field — which Ladybug has
(`TestLadybugIndexedSubstring`). FTS parity dropped to word tokenizer +
BM25 + per-field weighting, all already proven.

## Position revision on record

The changelog `20260726_trigram_bag_search.md` concluded that "the search-quality
argument for migrating no longer exists". That remains correct **for the FTS half** and
is incomplete as a general conclusion: the vector half had not been measured, and that's where
the gain is. The recommendation to stop the migration is revised.

## Remaining gap

The FTS5 prefix index (`prefix='2 3 4'`, which feeds the `token*` pass in
`buildPrefixQuery`) **has no equivalent in Ladybug** — probe `conf*` returns empty.
The hypothesis is that the trigram bag makes it redundant (`config*` reaching
`configuration` also matches via trigram overlap), but this is **hypothesis, not
measurement**. Needs to be measured before any SQLite removal.

## Own assertion fix

`TestSemanticReachOfAbbreviations` required `CFG_LOAD` in the "upper half" of the ranking. That
was wrong with the data already in view: `CFG_LOAD` is 1st/7 for query `config` but
4th/7 for `configuration`, i.e., the assertion encoded the luck of one wording as
requirement. Moreover it was redundant — `worstRelated > bestUnrelated` already covered the
essence. Replaced by the property that matters: `CFG_LOAD` must beat every
irrelevant identifier (measured margin ~4×), without requiring fixed position.

## Verification state

Closes the pending item from the previous changelog. Green: `internal/ai`, `internal/ast`,
`internal/fswatch`, `internal/daemon`, `internal/sysutil`. `go build -tags fts5 ./...` and
`go vet` clean, with ORT 1.26.0.
