# Migration step 1: search index on LadybugDB (FTS + vector), and the blocker that prevents removing SQLite

**Date:** 2026-07-26
**Scope:** `internal/ast/ladybug_search.go` (new, production) + 7 test files
**Origin:** Engineer's instruction — write step 1 with FTS and vector together and remove SQLite

---

## WHAT was delivered

`internal/ast/ladybug_search.go` — complete search index on LadybugDB, alongside
SQLite. Nothing was removed.

- **Schema:** `SearchEntity(uid, name, name_split, name_tri, docstring, etype, path, line,
  emb FLOAT[768])`, `SearchFile(path, name, name_split, name_tri, source)`, `SearchMeta`.
  `name` is what the result displays, `name_split` is what the index matches — separated on
  purpose, because the SQLite index stored the split in the same column and returned
  `"config.go config go"` as the name.
- **Seven FTS indexes per field**, which is how per-field weighting is reconstructed (the
  `bm25(0,10,3,2,1)` from FTS5 has no equivalent).
- **Vector index** over the same table — no auxiliary table. Rows with NULL vector are
  accepted and ignored by the query, and the index can be created on an empty table
  (`TestLadybugVectorSchemaConstraints`), which removes the need for `entity_vec_map` from SQLite.
- **`RebuildFromCache`** with `StreamEntries` (O(batch) memory, not O(corpus)), batched insert via
  `UNWIND`, and atomic swap via sibling path + rename.
- **`UpdateIncremental`** in-place.
- **`Search` / `SemanticSearch` / `HybridSearch`** with RRF fusion, plus the `CONTAINS` fallback
  for queries shorter than a trigram.

### Measured parity (same `ShardCache` feeding both engines)

| | SQLite | Ladybug |
|---|---|---|
| expected top-1, 16 probes | 12/16 | **14/16** |
| empty results | 0 | 0 |
| semantic: `CFG_LOAD` for `config` | — | **1st/5** |

Five differential tests: parity, rebuild idempotency, incremental (stale rows
leave, new ones enter, repeat doesn't duplicate), end-to-end semantic with the real embedder, and
fusion that doesn't lose what each half found alone.

## WHY SQLite was NOT removed

The plan stated as established fact: *"FTS and VECTOR update on insert/delete without
rebuild"*. For VECTOR it's true. **For FTS it's false.**

```
row-by-row insertion into live FTS index:
  22 of 25 rows invisible — in 12 of 12 iterations
```

Always 3 visible: fixed pattern, not random, which indicates a visibility window. This
explains why all prior probes — those in the plan and my five from this session —
"proved" in-place update: they inserted 1 or 2 rows, within that window. It was green by
sample size.

Consequence: any write requires recreating the FTS indexes, O(corpus) work for
O(1) change.

| | SQLite | Ladybug |
|---|---|---|
| total rebuild, 200k entities | 3.2s | 15.8s |
| query latency | 50–146ms | 92–158ms |
| **incremental of 1 file** | **~330ms** (current pipeline) | **5.3s** |

Removing SQLite today would deliver ~16× regression on the daemon's hot path. The
authorization to remove was given before these numbers existed, so the decision returns to the
Engineer instead of being executed on a disproven premise.

## Path to diagnosis (five refuted hypotheses)

Recorded because the cost of each mistake was real and the pattern is reusable:

1. *"Batch insert doesn't maintain the index"* — refuted, it does (with 2 rows).
2. *"Index created before inserts doesn't see them"* — refuted (with 2 rows).
3. *"Parameterized query doesn't match"* — refuted, works the same as literal.
4. *"The index doesn't survive CHECKPOINT/close/rename"* — refuted, it survives.
5. *"It's the corpus, the build order or the batch size"* — refuted by bisection; all six
   combinations failed the same.

What located the defect was a layered test (`TestLadybugSearchIndexDiagnostic`):
written rows → raw FTS query → wrapper pass → `Search()`. Layer 2 came back
empty with data present, which eliminated the entire upper half at once. And what
killed hypotheses 1 and 2 was **repeating** the probe with 25 rows instead of 2.

My mistake worth recording: an intermediate probe used `alphaToken` and queried `alpha`,
concluding FTS was broken. `alpha` is not a token of `alphaToken` — I myself had
already measured that three changelogs ago. The probe was wrong, not Ladybug.

## Fix applied within the design

`createIndexes()` now runs **after** bulk load, not in the schema. This made
rebuild deterministic (before: correct in one execution, empty in the next) and also more
correct: with the index built during load, `checksum` returned 5 documents with
identical score (0.0202); after, it returns only what matches.

`UpdateIncremental` calls `rebuildFTSIndexes()` (DROP + CREATE) after writing, which is the
recovery documented by liblbug's own error message. It is correct and is the origin
of the 5.3s.

## Tests that watch, instead of just passing

- `TestLadybugFTSPerRowInsertIsReliable` is **inverted on purpose**: passes while the
  liblbug bug exists and **fails when fixed**, signaling that
  `rebuildFTSIndexes` can be removed and that the O(corpus) cost per edit disappears.
- `TestLadybugFTSBulkInsertMaintainsIndex` and `TestLadybugFTSUpdateSemantics` gained
  explicit caveat that their greens describe the visibility window with 2–3 rows,
  and not index maintenance — so no one reads them as proof again.

## Upstream report to file (4th in the list)

**LadybugDB/liblbug 0.18.2** — FTS index is not maintained on `CREATE`, neither row-by-row nor via
`UNWIND` batch. Minimal repro: seed 10 rows, `CREATE_FTS_INDEX`, insert 25 rows,
query each one → 22 not found, reproducible in 12/12 runs. Only
`DROP_FTS_INDEX` + `CREATE_FTS_INDEX` recovers. The VECTOR index, in the same database, updates
in-place correctly.

## State

Full suite green with `-count=1`: `internal/ai` 102.8s, `internal/ast` 52.6s,
`internal/fswatch`, `internal/daemon`, `internal/sysutil`. `go build -tags fts5 ./...` and
`go vet` clean.

Removed bissection scaffolding along with the `fileInsertBatchOverride` knob it
required in production code — investigation closed, the hook should not outlive it.
