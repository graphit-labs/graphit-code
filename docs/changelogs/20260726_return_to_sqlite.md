# Back to SQLite: search index migration reverted

**Date:** 2026-07-26
**Scope:** `internal/ast/` (search engine and tests)
**Origin:** Engineer's decision after measurements on the real corpus

---

## WHAT changed

The search index returns to SQLite (FTS5 + sqlite-vec). `internal/ast/search_index.go` and the
LadybugDB implementation were removed; `internal/ast/fts_sqlite.go` came back.

**What did NOT come back with it — and is the point of this reversion:** the three improvements made to
`fts_sqlite.go` before the migration never existed in a commit with the file alive
(they were written and the file was deleted in the same batch). A `git revert` would have discarded them
silently. They were reapplied by hand:

1. **Trigram bag** — `entity_trigram` stops using `tokenize='trigram'` and now has
   `name_tri` with precomputed trigrams over a word tokenizer. That's what makes `config`
   find `coreConf` (2/4 → 4/4), which was the original request.
2. **`ORDER BY rank`** in the trigram pass, which didn't exist — RRF scores by position, and without
   ordering the pass injected arbitrary positions into the fusion.
3. **`sortResultsDeterministic`** at four points (output of `queryFTS`, of `queryTrigram` and
   final sorts of `Search` and `HybridSearch`), without which top-1 changed between builds of
   the same corpus.

Also survives `searchIndexSuffix` as a single constant, instead of the literal
`".search.sqlite"` repeated in five places — the divergence between those places and the exclusion list of
`CleanupInterruptedSwap` would delete the index.

## WHY the migration was reverted

Three LadybugDB 0.18.2 defects, all measured:

1. **FTS index is not maintained on insert.** 22 of 25 rows remain invisible, in 12/12
   iterations. Forces `DROP` + `CREATE` of the seven indexes after each write: O(corpus)
   work for O(1) change.
2. **Cascade failure on incremental.** Starting from the fourth consecutive update, delete
   aborted with *"FTS index 'sf_source' is inconsistent: document for node offset 3002 is
   missing during delete"*. Fixed in design (drop indexes before mutating), but the
   cost remains.
3. **Intermittent string corruption.** Over a 35,358-file corpus — all valid UTF-8 on
   disk, verified byte by byte — 4 stored rows came back with invalid UTF-8 and
   indexing failed; the identical re-run did not reproduce. It was not reduced to a minimal probe
   (`TestLadybugBulkInsertStringIntegrity` doesn't catch it in 5 runs).

The third is decisive: silent loss of data integrity, non-deterministic.

Comparison on the full corpus, same machine, same sampling:

| 35,358 files | SQLite | Ladybug |
|---|---|---|
| FULL total | 4m13.6s | 4m39.6s |
| FULL write | 13.2s | 38.9s |
| incremental per edit | 288–331ms | 5.0s |

The gain that justified the migration was the vector index updating in-place, eliminating the
whole-file rebuild forced by vec0's space leak. Still real, but doesn't pay for the three defects above.

## What stayed in the repository as evidence

LadybugDB probes remain, because they document why not to migrate and serve as repro if
upstream fixes it:

- `ladybug_fts_perrow_test.go` — **inverted on purpose**: passes while the bug exists and
  fails when fixed.
- `ladybug_fts_update_test.go`, `ladybug_fts_bulk_test.go`, `ladybug_fts_persist_test.go`,
  `ladybug_fts_param_test.go` — FTS update semantics, with caveat that their
  greens describe the 2–3 row visibility window.
- `ladybug_vector_test.go`, `ladybug_vector_schema_test.go` — the vector index works and
  updates in-place; that's what would be worth reusing.
- `ladybug_llm_test.go` — the `llm` extension is an external-provider client, not an embedded model.
- `ladybug_fts_utf8_test.go`, `ladybug_bulk_string_integrity_test.go` — attempts to
  reduce the corruption to a minimal probe. None reproduces; they remain recorded as what has been
  ruled out (control characters, size, invalid UTF-8 at source).

Requirement tests that survive the engine swap, now measured on SQLite:
`TestSearchIndexQualityFloor` (floor 12/16), `TestTruncatedQueryCoverage` (floor 9/11),
`TestAbbreviationRecallByNameAlone`, `TestSearchOrderIsDeterministic`,
`TestSearchIndexIncremental` and `TestSearchIndexIncrementalRepeated`.

`TestExpansionFieldCeiling` changed conclusion with the engine and was rewritten to record both:
on FTS5 both wordings reach 9/9, because the prefix index matches `config` with
`configuration`; without prefix index, only the wording with the exact token gets there. The assertion
now fails if parity is lost, which would signal loss of prefix matching.

## State

Full suite green with `-count=1`: `internal/ai` 86.5s, `internal/ast` 16.4s,
`internal/wiki`, `internal/fswatch`, `internal/daemon`, `internal/sysutil`.
`go build -tags fts5 ./...` and `go vet` clean.

E2E on the real corpus (3000 files sampled across the 13 object types):

```
FULL  parsed=2999 empty=287 errors=0 | parse=15.7s write=1.47s total=17.2s
INCR  6 scoped rounds, 28-40ms each | INCR total=99ms
```

Against the same e2e on LadybugDB: 4.7s write and ~1.0s per round. Requirements remain
green — `config` reaches `coreConf` and `CONF_MGR` by name, quality floor at 12/16,
truncation at 9/11.
