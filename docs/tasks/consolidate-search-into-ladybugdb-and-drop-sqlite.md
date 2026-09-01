---
title: SQLite leaves the binary — search and vectors move into LadybugDB
status: done-with-caveat
created: 2026-08-16
updated: 2026-08-16
tags: [ast, wiki, memory, ladybug, sqlite, search, storage, incremental]
---

# SQLite leaves the binary — search and vectors move into LadybugDB

**Origin:** instruction from the Engineer — "today there is both ladybug for the graph and
sqlite for fts/bm25 and semantic search, the problem is that having two databases weighs a
lot on disk and on processing. i want it to be only ladybug".

An earlier attempt, on 2026-07-26, was made and **reverted** (commits `354a32c` and
`c90e73f`). This one did not reuse that code — it was written from scratch, using the changelogs
of that batch only as a map of the traps.

---

## Why now, if it was rejected before

July's three blockers were re-verified on this date. Two are still true; what
changed was the **baseline**, not the engine.

| July blocker | State on 2026-08-16 |
|---|---|
| The FTS index is not maintained on insert — 22 of 25 rows invisible | **Still true.** `TestLadybugFTSPerRowInsertIsReliable` (inverted on purpose) passed 12/12. `go-ladybug v0.17.0` is still the newest. |
| Cascading failure in the incremental | Already solved in the design: drop the indexes before mutating. |
| "Incremental 5.3 s against ~330 ms" | **Obsolete baseline.** Those 330 ms were measured with in-place writes. |
| Intermittent string corruption | Still true, and it predates this change — it already hits the graph in production. |

The number that killed the migration compared Ladybug's cost against an **in-place** incremental.
That arrangement stopped being production's: `inPlaceIncrementalEnabled()` only turned on with
`GRAPHIT_INPLACE_INCREMENTAL=1`, and the default — copy+swap — already copies the database's entire
directory and already pays 215 ms–5.0 s just closing the mutated copy. The DROP+CREATE of the FTS
indexes now falls **inside that copy**, which is already O(corpus).

And the disk measurement that motivated all of it, on this laptop, project `<private-corpus>`:

| | size |
|---|---|
| `ladybugdb` (the graph) | 833 MB |
| `ladybugdb.search.sqlite` (the index) | **2.3 GB** |

The index was 2.8× the graph it described.

---

## What changed

### 1. The in-place path was removed; copy+swap is the only one

The Engineer's request. **Watch out for an inversion:** the literal request was "only the
possibility of `GRAPHIT_INPLACE_INCREMENTAL=1` should exist", but that flag TURNS ON in-place
writing — the opposite of "the production database is always read-only", which was how he described
the system in the first message. Presented with the measurement, he chose copy+swap:

```
escritor                                          leituras ok  abertura falhou  crashou
in-place, commit + CHECKPOINT                        43/60           11            6
copy+swap, produção nunca escrita                    60/60            0            0
```

The 6 are SIGSEGV inside cgo, in `open`: the CHECKPOINT rewrites pages underneath a
reader that is opening the same file. There is no retrying — it takes the MCP process down with it.

Removed: `incrementalInPlace`, `inPlaceIncrementalEnabled`, `deleteFileDataChecked`, the
flag, `internal/ast/incremental_mode_test.go` and
`TestLadybugInPlaceWritesUnderCrossProcessReaders`. The measurement survives in the header
comment of `IncrementalRebuild`.

### 2. The AST index lives INSIDE the graph database

`internal/ast/fts_sqlite.go` (1159 lines) removed. In its place:

| file | role |
|---|---|
| `internal/ast/search_index.go` | schema, rebuild, incremental |
| `internal/ast/search_query.go` | lexical, semantic and hybrid search, reading source |
| `internal/ast/search_common.go` | what is engine-independent: tokenization, split, trigrams, ordering |

Tables `SearchFile` and `SearchEntity` in `ladybugdb` itself, **not** in a sibling. That is the
decision that closes the design: an index in a separate file does not join the swap without a second
copy of its own, and updating it in place — which is what SQLite did, from a goroutine,
while readers were reading — left search OUTSIDE the atomic publication. A crash between the two
left graph and index describing different corpora. Now a single `rename(2)` publishes both
or neither.

Consequence: `SearchIndex` does not open a database of its own. It borrows the already open
`*LadybugBackend` (`NewSearchIndexOn`), because the engine bars a second RW handle on the same
database. In the incremental it is built on the `workingBackend` — the COPY — never on production.
Readers use `OpenSearchIndexReadOnly`.

### 3. The wiki (knowledge + memory) too

`internal/wiki/fts.go` (1667 lines) removed. In its place `internal/wiki/store.go`,
`internal/wiki/store_query.go` and `internal/wiki/search_text.go`. Tables `WikiChunk`,
`WikiXRef`, `WikiMeta`, `WikiSyncLog`.

`InsertChunkVector` now takes `[]float32` instead of a serialized blob, and
`EmbeddingCache` became `map[string][]float32`: the blob was `sqlite-vec`'s wire format, and
the embedding loop had no business knowing the engine's byte layout. The shard format on
disk (`.emb.json`) did NOT change — it is a file format, now owned by `process_cache.go`.

### 4. `internal/ladybugstore`, new

A thin shared layer. It exists because `internal/wiki` cannot import `internal/ast`, and
both came to need the same primitives: open/close, load extension, execute, coerce a graph
result value.

### 5. SQLite leaves the binary

Removed `mattn/go-sqlite3`, `asg017/sqlite-vec-go-bindings`, `BUILD_TAGS := fts5` from the
Makefile, `build-tags: [fts5]` from `.golangci.yml` and the two guard files
`fts5_required.go`. `go build ./...`, `go vet` and `golangci-lint` run **without the tag**.

---

## What the engine forced, and how each piece was answered

| FTS5 capability | in LadybugDB | answer |
|---|---|---|
| `bm25(0,0,10,3,2,1)` — per-column weights in one index | does not exist | one FTS index **per field**, weights applied in the RRF fusion |
| `trigram` tokenizer | does not exist | bag of 3-grams precomputed at write time, indexed with the word tokenizer — it matches BETTER, because it scores partial overlap instead of requiring containment |
| prefix / wildcard index (`conf*`) | does not exist | the same bag of trigrams: 11/11 against 9/11 for the prefix index on truncated queries |
| phrase operator | does not exist | conjunctive form of `QUERY_FTS_INDEX` |
| queries shorter than a trigram | — | `CONTAINS` fallback, scoped to the label |
| `vec0` | native vector index | **gain**: maintained on insert AND delete, so `entity_vec_map` goes away and so does the whole-file rewrite that vec0's space leak forced |

Measured on this date, and in doubt back in July: `UNWIND` with `FLOAT[768]` is **accepted**, so
vectors go in the batch; the vector index is accepted on an **empty** table; and rows with `emb` NULL
are ignored by the query.

The stemmer is pinned explicitly (`stemmer := 'porter'`) instead of left at the default. The default
IS porter today — probed: `'none'` matches only the literal `schema`, while the default, `'porter'`
and `'english'` all reach `schemas` —, and the ranking depends on it.

---

## Three fusion defects, found by measurement

RRF discards the magnitude of the score and keeps only the position, which breaks the reconstruction
of `bm25()` in three places. None of them was predicted; all showed up as a probe getting it wrong.

1. **A tie turned into an advantage.** Two documents with an identical score were separated by the
   deterministic ordering, which is alphabetical, and that position went into the fusion. `schema`
   returned `SchemaValidator` over `validateSchema` on an alphabetical coin toss. Fixed
   with competition ranking — tied entries share the rank.
2. **Summing fields gave a full vote to a weak signal.** Rank 0 in the `path` index (weight 1) is
   worth 1/(k+1) no matter how weak the match, and at k=60 that beats ~6 positions in the name index
   (weight 10). `config` returned `parseConfig` over `configLoader` just because
   `parseConfig` lives in `config.go`. Fixed with the strongest signal at full strength + the rest
   damped to 0.2.
3. **An exact name match got displaced.** `config` returned `parseConfig` — which matches on
   name, docstring and path — over the struct literally called `Config`. Fixed with an
   exact-name boost.

In the wiki, the same family: the title's trigram bag was initially weighted at 6.0,
**above** the `body` field (1.0). Since the bag matches disjunctively, a single shared
3-gram puts the document at rank 0 of it — and the query `credenciais`, a word that
appears in the BODY of one document and in no title, ranked another document first. The
wiki's trigram weights went below the weakest field (0.7 and 0.4), which is where the
FTS5 design had them (a 0.7 pass against 1.5 for the term passes).

---

## Two SWAP defects that SQLite was hiding

Both only showed up in the daemon's e2e, and neither is about search — they are about publishing the
database. While the index was a SQLite file outside the swap, both were invisible: they only
affected pages of the graph, which no test checked at that resolution. With the index inside the
store, they became a total and immediate failure of search.

### 1. `AtomicSwapDB` left the engine's sidecars behind

It renamed the file and removed only `<path>.wal`. The other liblbug sidecars —
`.shadow`, `.wal.checkpoint`, `.tmp` and the two checkpoint locks — are named after the
PATH, not after the file's identity, so they survived the rename and stayed next to the
NEW file describing the PREVIOUS incarnation. The next open recovered from
them, on top of what had just been published.

Measured: after a swap, `ladybugdb.shadow` at 1.1 MB and `ladybugdb.wal.checkpoint` at
44 KB next to a `ladybugdb` of 2.5 MB. With the sidecars removed, the same store goes to
516 KB.

The fix names the suffixes instead of sweeping `<path>.*`, for the same reason the cleanup of an
interrupted swap had to learn: a glob also catches the working copies and the sidecars
of whatever comes next.

### 2. The full rebuild built search into the file the rename had just detached

`RebuildFromJSON` writes into a TEMPORARY database and publishes it by renaming it over production.
`pipeline.go` called that rebuild and ONLY THEN built the search index, through the
`lb` handle — which points at the file detached by the rename. Rows and FTS indexes all went
there, and were discarded on the next open.

The symptom was exactly what the test reported: `MATCH (n:SearchEntity) RETURN count(n)`
returned 3 rows, `FileSourceAt` returned the file's text, and every `QUERY_FTS_INDEX`
returned zero.

Fixed with `RebuildFromJSONWithSearch`, which fills the search tables INSIDE the temporary
database, before the rename. Graph and index are now published by the same operation.

## A silent bug that died by construction

`fts_sqlite.go:466`: `UpdateIncremental` inserted into `entity_trigram` **without the
`name_tri` column**. The rebuild filled it in (line 347), the incremental did not. Every entity
touched by an incremental lost its trigrams until the next full rebuild, and the abbreviation
recall pass (`config` → `coreConf`) stopped reaching them, silently. Now the row is built in one
place only (`entityRowFor`), so it is not expressible.

---

## Tests

`internal/ast` and `internal/wiki` green with `-count=1`, with no build tag.

Converted rather than deleted, because the expectation survives the change of engine:

- `TestConsolidationQualityGate` (Ladybug × SQLite) **removed** — there is no second engine to
  compare against, and a differential test with one side removed compares an implementation against
  a toy. Its role belongs to `TestSearchIndexQualityFloor`, which asserts an absolute floor on the
  same corpus and the same probes. The corpus and `TestLadybugFTSFeatureParity` stayed.
- `TestExpansionFieldCeiling` — the assertion was "the two phrasings tie at 9/9", which is
  behavior of FTS5's prefix index. Without it the morphological phrasing drops to 8/9 and only
  the exact-token one reaches 9/9. Rewritten to assert the real condition, which is one more reason
  not to build the expansion field.
- `TestFileSourceAtDoesNotMigrateTheIndex` → `TestFileSourceAtDoesNotMutateTheStore` plus
  `TestFileSourceAtLeavesTheWriteHandleFree`. The danger changed shape: it used to be a schema
  migration that dropped `file_fts`; now it is the writer slot, which a read-write read would
  take from the daemon.
- Tests for `quoteToken`, `buildPhraseQuery`, `buildANDQuery`, `buildORQuery`,
  `buildPrefixQuery` removed along with the functions — there is no phrase, explicit boolean or
  wildcard to build.

Quality measured: **14/16** on the lexical floor (against 13 for the SQLite it replaced) and 11/11 on
the hybrid's decisive probes.

---

## ⚠️ MEASUREMENT ON THE REAL CORPUS: the design does NOT scale on this project

Measured over the production shard cache of project `<private-corpus>`,
rebuilding into a scratch path — production was not touched.

**39,429 files, 2,501,342 entities** — 12.5× the 200k entities of July's measurement.
That scale had never been measured.

| | new (Ladybug) | old (SQLite) |
|---|---|---|
| store | 5.70 GB | 5.5 GB (873 MB graph + 4.65 GB index) |
| full rebuild | 988 s (16.5 min) | — |
| **1-file incremental** | **1,178 s (19.6 min)** | ~330 ms (measured at 200k, in-place) |
| query | 487–778 ms | 50–146 ms (measured at 200k) |
| buffer pool required | **8 GiB** | — |

On a synthetic corpus the numbers are good and anticipated none of this: 2000 files /
10,000 entities give full 2.30 s, incremental 1.19 s, query 69 ms.

### Three problems, in order of severity

**1. The incremental costs ~20 minutes per file, and it is SLOWER than the full rebuild.** It
copies 5.7 GB, mutates, drops and recreates the 9 FTS indexes, closes and swaps. The DROP+CREATE is
O(corpus) — the same work as a full build — and the copy comes on top. For a daemon
that reindexes on every save, it is unviable.

**2. It does not build with the buffer pool the project ships.** `boundedDBBufferPool` clamps at
1 GiB per database (`dbBufferPoolCeil`); with it, the creation of `se_path` dies with
*"Buffer manager exception: Unable to allocate memory! The buffer pool is full"*. The 988 s
above are with `GRAPHIT_DB_BUFFER_MB=8192`.

**3. The disk gain did not exist.** 5.70 GB against 5.5 GB — a tie. The saving that motivated
the whole migration did not show up on this corpus.

### What that means

The copy+swap reframing dissolved July's RELATIVE argument — the 330 ms were an
in-place baseline, and in-place is no longer production's arrangement. It did not dissolve the
ABSOLUTE one: liblbug does not maintain an FTS index on insert, so every write is O(corpus), and at
2.5 M entities that is tens of minutes. That is the original blocker, measured at the scale that
matters.

What is solid and is not affected: search is CORRECT and better (14/16 against 13/16, and
queries over the 2.5 M entities return results in 487–778 ms with a fresh handle); the
wiki migrated well, because wikis are small and always rebuilt whole; SQLite left the
binary; ICU left.

### Cheapest path before any bigger decision

Cut FTS indexes. There are 9 today. The three that are expensive and low-value are `sf_source` (it
indexes the entire text of ALL files), `se_path` (weight 1) and `se_type` (weight 2). Cutting to
4–5 should reduce build and disk significantly and is measurable in one run. It stays
O(corpus): it shrinks the 20 min, it does not eliminate them.

If that is not enough, the options are: raise the incremental's threshold a lot (reindex rarely and
in batches, not per save); keep SQLite only for the AST index and leave the wiki on Ladybug; or
wait for upstream — `TestLadybugFTSPerRowInsertIsReliable` is inverted exactly to
warn when liblbug starts maintaining the index on insert.

**A harness trap, recorded because it cost time:** querying through the `lb` handle
AFTER an `IncrementalRebuild` returns zero for everything. The swap renames over it and the
handle keeps pointing at the detached inode — same family as the two swap defects
above. A fresh handle on the same store answers normally.

---

## ICU left along with it

A question raised in the same session. The answer INVERTS the premise that it was there because
of SQLite: ICU belongs to neither of the two.

- `liblbug.so` v0.17.0 does not declare ICU in `NEEDED` (`libdl`, `libpthread`, `libssl`,
  `libcrypto`, `libm`, `libgcc_s`, `libc`, `ld-linux`) and `strings -a` does not find `libicu`
  in it, so it is not `dlopen`ed by name either.
- LadybugDB's build documentation does not list ICU on any platform.
- SQLite was already ruled out as a consumer: it was compiled only with the `fts5` tag, never
  `sqlite_icu`. Removing it changed nothing in the calculation.
- Functional proof, on Linux: delete the ICU files from an extracted runtime and run
  `ast query --hybrid`, which exercises LadybugDB, the text index, the vector index and ONNX at
  once. It worked.

Removed from all THREE platforms: the `find` for `libicu*` in `build-linux` and in `build-darwin`
(plus the two `rm -f` that existed only to clean up the duplicates those globs
produced), the `-licuuc -licuin -licudt` flags and the ICU includes in `build-windows` and in
`build-windows-native`, the three copies of `/mingw64/bin/libicu*.dll`, and a
`! -name "*icu*"` exclusion in the `find` that copies every DLL from the mingw sysroot.

**macOS and Windows were not verified** — that requires an artifact from each platform, and the
Engineer decided to remove it anyway and put it back if something breaks. The risk asymmetry
is recorded in the Makefile comment: on macOS a missing dylib aborts the process at
startup, so a break there is total, not partial. `libicu-dev` / `icu4c` may still be
needed to COMPILE; that is a different thing and it did not change.

Value: 37–73 MiB on Linux, varying with how many ICU majors the build machine had,
because the glob picked up all of them.
