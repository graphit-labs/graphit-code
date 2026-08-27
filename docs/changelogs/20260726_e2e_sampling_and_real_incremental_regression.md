# E2E: corpus sampling fixed and the real incremental regression

**Date:** 2026-07-26
**Scope:** `internal/ast/e2e_bench_test.go`, `internal/ast/oracle_extraction_census_test.go`,
`internal/ast/oracle_corpus_extraction_test.go`,
`internal/ast/oracle_pipeline_extraction_test.go`
**Origin:** validate on the real 35k-file corpus the search that was just replaced

---

## The problem: the limited e2e was measuring empty extraction

Running `TestE2EIndex` with `GRAPHIT_E2E_MAX_FILES=800` reported:

```
FULL  files=799 parsed=799 empty=799 errors=0 | parse=452ms
```

**All 799 files without a single entity.** The search index stayed empty, so
any time measured there — rebuild, incremental, query latency — described an
empty index, not the corpus.

And `parse=452ms` for 799 files (0.57ms each) already gave it away: PL/SQL via ANTLR costs ~70ms
per file. The real parse wasn't happening.

### Cause: walk prefix, not a bug

Four hypotheses were tested and discarded before finding the right one:

1. *"the PL/SQL grammar is not resolving"* — `TestOracleCorpusExtraction` shows
   **73 entities in 6 files** via `antlr-plsql` (tree-sitter-sql extracts 0, expected: it's
   generic SQL against Oracle DDL).
2. *"the pipeline doesn't call ANTLR"* — `TestOraclePipelineExtraction` shows the pipeline
   storing entities normally.
3. *"it's the directory structure or the copied `.astignore`"* — bisected: neither matters.
4. *"it's the number of files"* — also not.

What matters is **which** files. The corpus has one directory per object type,
`filepath.WalkDir` is lexical, and the first type is `comments/` — files with `COMMENT ON`,
which have no named entity to extract. Copying a **prefix** of 800 files only took
`comments/`.

`TestOracleExtractionCensus` measures by type (12 files each):

| type | entities | empty | median/file |
|---|---|---|---|
| comments | **0** | 12/12 | 0 |
| constraints | 42 | 0/12 | 3 |
| functions | 122 | 0/12 | 9 |
| indexes | 12 | 0/12 | 1 |
| mviews | 12 | 0/12 | 1 |
| packages | **776** | 0/12 | 58 |
| procedures | 134 | 0/12 | 10 |
| sequences | 12 | 0/12 | 1 |
| synonyms | 12 | 0/12 | 1 |
| tables | 102 | 0/12 | 7 |
| triggers | 39 | 0/12 | 4 |
| types | 7 | 0/7 | 1 |
| views | 12 | 0/12 | 1 |

**1282 entities in 151 files, 8% empty.** Extraction works for 12 of 13 types;
`comments` yielding zero is correct, not a defect.

### Fix

`TestE2EIndex` now samples **round-robin across top-level directories** instead of taking a
prefix. With that, 800 files now give `empty=67` (8%, matching the census) and
`parse=6.3s` instead of 452ms.

Consequence for history: **every prior measurement taken with `GRAPHIT_E2E_MAX_FILES`
describes an empty index** and should not be cited. This includes the "~330ms" baseline for the
incremental that I myself used as reference in the previous changelog.

## The real incremental regression

With sampling fixed, apples-to-apples comparison on the same 800-file subset
and same machine, using a worktree from the commit before the migration:

| | SQLite (before) | Ladybug (after) |
|---|---|---|
| FULL total | 7.24s | 8.58s |
| FULL write | 706ms | 2.24s |
| scoped incremental, per round | **9–11ms** | **615–875ms** |
| INCR total | **89ms** | **979ms** |

Incremental regresses ~11× in total and ~85× in scoped write. The cause is known and already
recorded: Ladybug 0.18.2's FTS index is not maintained on insert, forcing `DROP` +
`CREATE` of the seven indexes after each write — O(corpus) work for O(1) change. That's why
the cost **grows with the corpus**: 979ms with 800 files, 5.3s with 200k synthetic entities.

Record correction: the previous changelog cited "~330ms → 5.3s (16×)". The direction was
right, the baseline wasn't — it came from the empty prefix. The numbers above are the valid ones.

## State

Measurement on the full corpus (35,358 files) in progress at time of writing.
`go vet` clean.
