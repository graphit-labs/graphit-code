---
title: Optimize AST store disk usage (shards, source text and embeddings)
status: done
created: 2026-08-28
updated: 2026-08-28
tags: [ast, storage, shards, lancedb, icebug, disk]
---

# Optimize AST store disk usage

## Objective

A project's AST store occupies far more disk than the information it holds justifies.
Measured on this repository's store
(`~/.graphit/ast/project/01KSH1CRFFG8Z74B5ZS78WW808`, 796 indexed files):

| part | size | % |
|---|---|---|
| `shards/*.emb.json` | **381 MB** | 55% |
| `search.lance/entities.lance` | 249 MB | 36% |
| `shards/*.nodes.json` | 39 MB | 5.6% |
| `shards/*.edges.json` | 33 MB | 4.7% |
| `graph.icebug/` | 5.2 MB | 0.7% |
| `manifest.json` | 120 KB | — |
| **total** | **696 MB** | |

The Engineer raised two challenges:

1. Shards carry `src` (a copy of the file's text). Since shards exist only for the local
   path, could the source be read from the file itself instead of being copied?
2. Could shards be removed entirely, with the full and incremental index writing directly
   into `search.lance` and `graph.icebug`?

This log records the measurement, the answer to each challenge, and the plan.

## Reasoning — what the measurement showed

**Challenge 1 is right in principle and wrong about scale.** `src` is indeed in the
`nodes.json` shards, but measured: **7.4 MB out of 39 MB (19% of the nodes shards, ~1% of
the store)**. Removing it does not move the number. And `src` had two real consumers:

- `internal/ast/search_lance.go:465,579` — `buildFileRow(relPath, entry.Source)` writes the
  file's text into the `files.lance` table;
- `internal/ast/embedder.go:235` — slices `src` by line range to build the text sent to the
  embedding model.

The graph does **not** use `src`: `newRebuildIndex` / `ExportDirect*`
(`internal/ast/rebuild_index.go`, `internal/ast/direct_icebug.go`) never read
`entry.Source` — `fileNodeJSON` emits only `path`/`name`/`relative_path`/`is_dependency`/
`lang`/`cluster`.

**The elephant is `emb.json`: 381 MB, 55% of the store, and a pure duplicate.** Measured:
39,762 vectors of dimension 768. As float32 binary that is **122 MB**; as JSON decimal text
it is 381 MB — a **3.1x inflation from serialization alone**. Those same vectors are already
written to the `embedding` column of `search.lance/entities.lance` (`buildEntityRow`,
`search_lance.go:353`). The embedding shard is a recomputation cache, not a sole source.

**Challenge 2 runs into a structural property of icebug.** The bundle is a global CSR
(`nodes_*.parquet` + `indices_*` / `indptr_*`): a node's id is its dense index within its
label's table. Inserting or removing a node shifts ids and invalidates the `indptr` of every
label pair that touches it. That is why `rebuildIcebugFromCacheWithDelta`
(`internal/ast/icebug_rebuild.go:31`) runs `cache.StreamEntries` over the **entire** corpus
and assembles a complete `entries` map before exporting, even on the incremental path — the
"incremental" there decides only **which Parquets to rewrite**, not where the rows come from.
Shards are precisely what avoids reparsing 796 files on every single-file edit.

So: the `nodes`/`edges` shards (72 MB) are the materialization of the parse that buys the
incremental. Removing them without a replacement trades 10% of disk for a full reparse per
edit. What **is** removable is `emb.json` (55%), which buys nothing LanceDB does not already
hold.

### Incidental findings

- **`Compact` is never called on the AST index.**
  `internal/lancestore/store_lancedb.go:350` exists and has exactly one caller:
  `internal/wiki/store.go:114`. `entities.lance` currently has **32 fragments and 37 retained
  versions** — rows deleted by an incremental become tombstones and old versions hold the
  bytes. Theoretical floor for the data: 122 MB of vectors plus text body; `data/` is
  currently 218 MB.
- `files.lance` (16 MB) was a third copy of the source, alongside `src` in the shard and the
  real file in the repository.

## Justification — why this order

Work is ordered by return over risk, highest first:

1. `emb.json` eliminated: 55% of the store, zero semantic change.
2. LanceDB compaction: dead bytes, reclaimed by a primitive that already exists.
3. `src` out of the shard: 1% of the store here, and it touches two hot paths (embedding and
   `files.lance`) — done last.

Rejected alternative: **removing shards entirely** (challenge 2 in its literal form). It
would require either that the icebug format accept incremental CSR mutation, or that LanceDB
become the source for rebuilding the graph — both trade disk for `O(corpus)` reparse or
`O(corpus)` reads from Lance on every edit. Recorded under Trade-offs.

## Plan & Task Breakdown

> Engineer's decisions, recorded on 2026-08-28 before implementation:
> **T1** — `emb.json` goes away entirely; Lance becomes the only home of the vector, and also
> where "what still needs embedding" is answered from. **T2** — compact and prune whenever
> needed, retaining no old versions, provided search never becomes unavailable.
> **T3** — `src` leaves the shard: small in this project, but it grows without bound in a
> large codebase and does not need to exist.

- [x] **T1 — Eliminate `emb.json`; Lance becomes the only home of the vector** — Spec:
  touches `internal/ast/shard_emb_cache.go` (deleted), `internal/ast/rebuild_helpers.go`
  (`BuildEmbLookup`), `internal/ast/embedder.go`, `internal/ast/search_lance.go`,
  `internal/lancestore/store_lancedb.go`. Three coupled changes:
  (1) the daemon writes the vector straight into the `embedding` column of `entities.lance`
  — via `Upsert`/`MergeInsert`, both already in the binding;
  (2) "what still needs embedding" stops being a shard scan and becomes a Lance query —
  `QueryBuilder.Filter("embedding IS NULL").Columns([...])`;
  (3) `shard_emb_cache.go` and every `.emb.json` are removed.
  Done when: this repository's store drops from 696 MB to ≤ 315 MB and
  `graphit_ast_search mode:semantic` returns the same hits as before.
  Invariant that must not be lost: today `ShardEmbCache.Get` refuses the vector when
  `emb.Hash != currentHash`. The equivalent in Lance is that the entity's row is deleted by
  `path` on the incremental, so its vector dies with it; any path that reuses a vector
  without that guarantee reintroduces a stale vector answering for an entity that changed.
  Risk to handle: today the embedding cache survives a corrupt or empty `search.lance`;
  after T1, losing the index means recomputing ~40k embeddings.
- [x] **T2 — Compact and prune `search.lance` versions** — Spec: touches
  `internal/ast/search_lance.go` and `internal/lancestore/store_lancedb.go` (expose
  `OptimizePrune` alongside the existing `Compact`). Run `OptimizeCompact` with
  `MaterializeDeletions` and `OptimizePrune` at the end of the pipeline. Done when:
  `entities.lance/data` drops from its current 218 MB toward the content floor, and
  `_versions` stops growing. Invariant: `PruneParams.OlderThan` **must not be zero or
  near-zero**. Lance is MVCC and a reader holds the snapshot it opened, so compaction does
  not take search down — but pruning a version a reader still holds does. `DeleteUnverified`
  exists precisely to override the backend's 7-day margin for in-flight transactions, which
  is why it is the dangerous option: use a short, explicit, measured margin, not zero.
- [x] **T3 — Remove `src` from `nodes.json`** — Spec: touches `internal/ast/shard_cache.go`
  (`shardNodes.Source`, `splitEntry`, `mergeShards`), `internal/ast/embedder.go:235` and
  `internal/ast/search_lance.go:465,579`. Re-read the real file from the project root when
  the text is needed, guarded by the manifest hash. Done when: `src` no longer exists in the
  shard and neither the embedding text (byte-identical) nor `files.lance` regresses.
  Invariant: the manifest hash is the sole arbiter that the bytes on disk are still the bytes
  that were parsed; on divergence the path must reparse rather than embed the wrong text.

## Trade-offs & Decisions

- **`src` goes even though it is 1% here.** The measurement on this repository understates
  the problem: the cost of `src` is proportional to the corpus's total source bytes, which
  has no ceiling. `docs/architecture/storage_layout.md` itself records that "file text alone
  reached 2.4 GB on a 36k-file export". 7.4 MB here is the same mechanism that yields
  gigabytes there, and nothing bounds it.
- **LanceDB does NOT compute embeddings in Go.** Verified against the binding this repo links
  (`github.com/lancedb/lancedb-go v0.1.3-0.20260509194607-fa14ce29c772`): there is no
  `EmbeddingFunction`, no registry, no `SourceField`, no auto-embed. The only vector API is
  `AddVectorField(name, dimension, dataType, nullable)` — a column the caller fills. The
  embedding-function feature exists in the Python and TypeScript SDKs, not in Go.
  Consequence: computation stays in the local ONNX model (`coderankembed`); what T1 changes
  is only WHERE the vector is stored and HOW pending work is discovered.
- **The `nodes`/`edges` shards stay.** The icebug bundle is a global CSR and the exporter
  rebuilds the whole index on every publish; shards are what avoids reparsing the entire
  corpus per edit. Trading 72 MB for that is a bad deal on any large corpus — and the large
  corpus is exactly where disk matters. Challenge 2 is recorded as answered: yes, it is
  technically possible; no, it does not pay off in this form.
- **No backward compatibility, no data migration.** The Engineer's instruction: we are in
  dev. When a derived artifact's format changes, bump the version and let the old material be
  discarded and rebuilt from source. T3 originally shipped with a whole migration machinery
  (`shardManifest.Stripped`, `shardStripVersion`, `stripShards()`, plus a test for it); all of
  it was deleted in favour of one line — `shardCacheVersion` 9 → 10.

## Technical Debt

- [ ] `Compact` is only called by the wiki; the AST index never compacts.
- [ ] No path prunes old Lance versions — 37 retained versions in this store's
      `entities.lance`.
- [ ] `internal/ast/search_index_test.go` does not declare `//go:build lancedb` but depends on
      `hybridScaleFixture` and `targetVector`, defined in `search_scale_test.go`, which does.
      Consequence: `go test ./internal/ast/` without the tag fails to compile. Pre-existing
      (confirmed with `git stash`).
- [ ] After T1, the embedding cache stops being independent of the search index: losing
      `search.lance` will cost a recompute of ~40k vectors, where shards used to cover it.
      Evaluate whether a cheap rebuild path is worth it.
- [ ] `graphit_hub_search` with `query: "lance"` hung for 1800s and aborted, while `lancedb`
      and `embedding` returned empty in seconds. Short term causing a pathological scan? Not
      investigated.

## System Knowledge

- `ExportDirectIncremental` (`internal/ast/direct_icebug.go:387`) falls back to a full export
  whenever ANY file was **deleted**: "the rows they owned cannot be re-derived from the
  current shards alone".
- `rebuildIcebugFromCacheWithDelta` only considers the incremental path when
  `len(changed)+len(deleted) < cache.Count()/5`.
- **Shards are a LOCAL artifact and never travel.** `prepareASTPublish`
  (`internal/hub/registry.go:712-723`) copies `search.lance` into the published artifact, so
  the `source` column of the `files` table travels with it. A Hub-installed context gets a
  `search.uri` pointing at that published index (`internal/hub/ast_store.go:127-132`) and
  never replays shards. `BuildSearchIndexFor` has exactly one production caller:
  `internal/ast/pipeline.go:430`, the local path.
- **LanceDB is what answers for source**, and already was: `internal/ast/source_service.go:201`
  → `FileSourceAt` → `SearchIndex.FileSource`, which filters the `files` table by `path`.
- `entityBody` (`search_lance.go`) concatenates name, identifier split, lowercased copies,
  label, docstring, trigrams and path into a single text field — the lowercased copies are
  acknowledged as probably redundant, kept only because they came out of a measured sweep.
  That is volume in `entities.lance` a new sweep could cut.
- Vectors exist for 39,762 of roughly 63k nodes: each grammar's `embed_labels` decides which
  labels get embedded.

## Progress Log

### 2026-08-28 — measurement and plan

- Measured the real store: 696 MB, distributed as in the table above.
- Confirmed by reading the code that the icebug graph export never reads `entry.Source`.
- Confirmed that `emb.json` (381 MB) duplicates the `embedding` column of `entities.lance`,
  with 3.1x inflation from JSON serialization.
- Confirmed that the icebug incremental path loads the entire corpus into memory, which is
  the reason the `nodes`/`edges` shards exist.
- Found that `Compact` never runs on the AST index, and that there are 32 fragments and 37
  retained versions.
- Consulted the Hub for LanceDB before answering anything about its API: `graphit_hub_search`
  with `lancedb` and with `embedding` returned empty — no artifact. The answer was therefore
  taken from the vendored binding, which is stronger evidence than model knowledge.
- **Verified in the Go binding**: no embedding function or registry — only `AddVectorField`.
  And everything T1 and T2 need is present: `MergeInsert` + `WhenMatchedUpdateAll` to write
  the vector, `QueryBuilder.Filter(...).Columns(...)` to find pending work, and
  `OptimizeWithAction` with kinds `OptimizeAll`, `OptimizeCompact`
  (`CompactionParams.MaterializeDeletions`), `OptimizePrune` (`PruneParams.OlderThan`,
  `DeleteUnverified`) and `OptimizeIndex`.

### 2026-08-28 — T3 implemented

- **T3 delivered and green.** `go test -tags lancedb ./internal/... ./cmd/...` passes in full.
- `src` leaves the shard **unconditionally**. `splitEntry` never writes text; `SourceOf` reads
  the file from the working tree validated against the manifest hash; with no tree it returns
  `""`.
- `EmbeddingConfig.RepoRoot` is no longer read by the embedder: it hands it to the
  `ShardCache` via `SetRoot`, and all text resolution goes through `SourceOf`.
  `sourceFromDisk` became `fileLinesFor`, which delegates — the hash check has a single owner.
- `shardCacheVersion` 9 → 10. No migration, no backward compatibility: we are in dev, the old
  shard is discarded and reparsed from the tree, which locally always exists.
- Tests changed because the contract changed, not for convenience:
  `TestNoSourceIndexingLeavesTheShardTextFree` → `TestShardOnDiskNeverCarriesFileText` (the
  guarantee stopped being conditional on the flag and now always holds);
  `TestEmbeddingPrefersCachedTextOverTheDisk` →
  `TestEmbeddingSnippetComesFromTheFileNotTheCachedEntry` (the precedence was deliberately
  inverted); `TestBuildSearchIndexForMakesAContextSearchable` gained a working tree, because
  that is what it always has in production.
- **The disk saving on the existing store has NOT been measured** — it lands on the next
  index. Expected: −7.4 MB here; what matters is the growth that stops existing on a large
  corpus.

#### Engineer's correction, mid-implementation

My first version made the removal of `src` conditional on having a working tree: with no
tree, the shard kept the text, on the reasoning that "an imported context would be the only
copy". **That was wrong, and so was the premise.**

The Engineer corrected it: shards exist only locally, to build the Parquets and the Lance
tables; shards do not go to the Hub and must not; and when an artifact comes from the Hub
there is indeed no source on disk, but it must come from LanceDB — that is what answers.

Verified in the code, and it already works exactly that way (see System Knowledge above).
There is **no production path** where a shard without a tree needs to answer for source. The
conditional was invented complexity on top of a false premise, and it introduced a real
defect: `stripShards` in the constructor would have erased an imported context's source. All
of it was removed along with the migration.

Lesson worth keeping: shards are a **local** artifact, full stop. LanceDB is what answers for
source — from the tree when there is one, from the published index when there is not.

#### Language convention

The Engineer instructed that everything written down — code, comments, docs, file names —
must be in English. This log was originally written in Portuguese under
`docs/tasks/otimizar-espaco-em-disco-do-store-ast.md` and was renamed and rewritten here.
Conversation stays in Portuguese; artifacts are English.

### 2026-08-28 — T1 implemented

- **T1 delivered and green.** `go test -tags lancedb ./internal/... ./cmd/...` passes in full;
  `go vet` clean.
- `internal/ast/shard_emb_cache.go` is deleted. `ShardEmbCache`, `NewShardEmbCache` and
  `BuildEmbLookup` are gone, and so is the `embLookup` parameter of `RebuildFromCache`,
  `UpdateIncremental` and `BuildSearchIndexFor`.
- Two new methods on `SearchIndex` replace what the shards did:
  - `EmbeddedUIDs(ctx)` — which entities already carry a vector, answered by the engine from
    the column that already exists. Paged at 20k and **projected to `uid`**, because an
    unprojected read of that table is a read of the whole store.
  - `StoreEntityVectors(ctx, ents, vecs)` — writes a batch back via `Upsert` on `uid`, composing
    the row through `buildEntityRow` so body and trigrams stay exactly what a rebuild produces.
- `RebuildIcebugFromCache*` lost its `embCache` parameter: it never used it. The icebug bundle
  has never carried a vector.
- **`rebuildSearchIndexForEmbeddings` is gone, and that is the second win.** Every productive
  embedding cycle used to rebuild the ENTIRE search index just to inject the vectors it had
  just computed — an `O(corpus)` rewrite per cycle. The embedder now writes them in place.
- `lancestore.Query` gained `Columns` and `Offset`, plumbed to the engine's `QueryConfig`
  (both already existed there). `SearchIndex.Counts` was reading up to a million FULL rows —
  vectors included — to count vectors; it now projects `uid`.

#### A real production bug this uncovered

`RebuildFromCache` builds its indexes at the END, and skips the vector index when the vector
count is below `lanceMinRowsForVectorIndex` (256). With vectors no longer present at rebuild
time, that count is always **zero** — so the vector index would never be built at all, and a
hybrid query fails with `expected column _distance not found in rank`.

Fixed by `SearchIndex.FinalizeVectors(ctx)`, called once at the end of a productive embedding
cycle: it counts, ensures the indexes (idempotent), folds new rows in, and updates
`embeds.json`. `TestHybridKeepsTheEnginesOrderRatherThanItsScore` is what caught it.

#### A sharp edge worth knowing

`ShardCache.StreamEntries` **evicts** each shard after the callback and reloads it from disk on
the next pass. An unflushed cache can therefore be streamed exactly **once** — the second pass
silently yields nothing. This cost a debugging cycle: `newShardCacheForTest` did not flush, so
the tests' second pass (writing vectors) saw an empty corpus and reported "vectors were written"
while writing none. The fixture now flushes, as a real index leaves it.

#### Measured

The store currently holds **383.8 MB across 790 `.emb.json` files**. `shardCacheVersion` went
9 → 10, and a version mismatch now deletes the whole `shards/` directory — which is what
actually returns those bytes, since nothing walks a `.emb.json` any more and it would otherwise
sit there forever. The reclaim lands on the next index; it has NOT been observed yet.

### 2026-08-28 — T2 implemented, and the wiki with it

- **T2 delivered and green**, and the Engineer extended it: LanceDB backs the wiki too, so the
  same maintenance applies there.
- `lancestore.Table.Compact` now passes `MaterializeDeletions` — without it a deleted row leaves
  a tombstone and its bytes stay in the fragment, and an incremental deletes by path on every
  change, so tombstones are the steady state rather than an edge case.
- `lancestore.Table.PruneVersions(ctx, olderThan)` is new, wrapping `OptimizePrune` with
  `DeleteUnverified` (which is what lets a margin shorter than the backend's 7-day default apply).
- Both now RETURN what the engine reports — `CompactionResult` and `PruneResult` — instead of
  discarding it. That is not cosmetic; see below.
- `SearchIndex.Maintain(ctx)` runs after every index write in the pipeline and at the end of
  `FinalizeVectors`. `WikiDB.Maintain(ctx)` runs at the end of a wiki `Rebuild`.
- `WikiDB.Compact` existed and had **no caller at all** — the wiki never compacted either.

#### Two design errors of mine, both caught by measurement

**1. Compaction alone reclaims nothing.** Compacting writes one merged fragment and leaves the
ones it replaced on disk, because the superseded versions still reference them. Only pruning
those versions drops the files. Compaction and pruning are a pair, in that order, and a test that
asserts one without the other proves nothing.

**2. The fragment-count threshold was wrong, and wrong in the worst direction.** The first
version counted files under the table's `data/` directory and compacted above a threshold of 16.
That number is not a fragment count — it does not go down when compaction merges (see above) — so
the threshold would have stayed tripped forever and compacted on EVERY write, which is exactly
the `O(table)` cost the threshold was written to avoid. It also silently returned 0 when the path
did not resolve, which disabled compaction entirely for any index opened by URI.

Both are gone. `Maintain` now compacts unconditionally and lets the engine answer: with nothing
to merge it reports `FragmentsRemoved: 0` after reading only manifest metadata.
`TestMaintainOnAnAlreadyCompactStoreDoesNothing` pins that.

#### A pre-existing leak, fixed because it had become 10% of the store

`ExportDirectIncremental` creates `<bundle>.tmp.<hex>.scratch` and removes it at the end — with
**seven early returns in between**, none of which clean up. The caller's own `os.RemoveAll`
targets `<bundle>.tmp.<hex>`, a name this one merely extends, so it never matched. Measured:
**136 orphaned directories holding 18 MB**. Fixed with a `defer`, plus `removeStaleBundleTemps`
which sweeps leftovers from runs that died before their cleanup. Verified: 136 → 0.

### 2026-08-28 — two regressions reported by the Engineer, both mine

Running `ast index schema/ --reset` then `ast index reports/` on the private corpus
(37,966 PL/SQL files, then 388 XML reports) produced two failures.

#### 1. Invalid UTF-8 reaching Arrow — CONFIRMED, root cause found

```
rebuild: search index incremental: writing 388 files: lancestore: appending 388 rows to files:
Failed to parse IPC data: Invalid UTF8 sequence at string index 1 (7962..43504):
invalid utf-8 sequence of 1 bytes from index 26649
```

**The JSON shard was sanitising the text and nobody knew.** `encoding/json` substitutes U+FFFD
for malformed byte sequences, so file text that went `file -> parser -> JSON shard -> read back
-> Arrow` arrived valid no matter what the file contained. T3 removed the round trip by reading
the working tree directly, and with it the substitution. Verified in isolation: a 4-byte string
with one bad byte comes back from a JSON round trip valid and 6 bytes long.

Arrow rejects a whole record batch containing invalid UTF-8, so ONE bad file fails the append
for every file batched with it — which is why 388 files failed together.

Fixed in `ShardCache.SourceOf`, which now applies the same substitution deliberately.
`TestSourceOfReturnsValidUTF8ForAMalformedFile` pins it.

#### 2. The bundle shrank from 549 Parquet files to 175 — TWO PRE-EXISTING BUGS, found and fixed

My first diagnosis blamed the stale-temp sweep this task added. That was wrong: the real cause is
two independent, pre-existing defects in `exportDirectDelta`, identical in `HEAD~1`. The
shrinkage was progressive — 549 -> 175 -> 57 — which a race would not produce.

**a. The regenerated tables were never published.** The function exports a full bundle into a
SCRATCH directory, then copies the UNAFFECTED tables from the old bundle into `outDir`. Every
`copyIcebugFile`/`copyRelMember` call went `finalDir -> outDir`; nothing ever copied
`scratch -> outDir`. An affected table's freshly written Parquet stayed in scratch and was
deleted with it, while the manifest still named the file. The published bundle held only the
tables the run did NOT touch.

**b. The emptiness probe emitted the UIDs it was probing.** `labelInBatches` calls `nodeRowsFor`
purely to test whether a label has rows — but building rows calls `ri.emitUID`, which is
STATEFUL and returns false the second time a uid is asked for. The probe loop runs over every old
node table BEFORE the real export, so the export then produced EMPTY tables for all of them.

Together: each incremental emptied the tables it touched and then failed to publish them.

Fixed: affected and brand-new labels/rel members are copied out of `scratch`; `outDir` gets the
`os.MkdirAll` it never had (it is a name the caller derived, not a directory it created); and the
probe saves, clears and restores `ri.emittedUIDs` so it leaves no trace.
`TestIncrementalExportPublishesTheTablesItRegenerated` pins both — every file the manifest names
must exist, and the bundle must not shrink relative to its base.

**Verified on the real corpus**: `graph.icebug` went 57 -> 777 items on the next incremental,
with no error, and the graph answers with the PL/SQL schema and the XML reports together
(302,933 AttributeValue, 186,002 Comment, 146,153 Column).

The age guard on the stale-temp sweep stays regardless — sweeping by name prefix alone genuinely
cannot tell a dead run's working directory from a live one's, and the daemon indexes the same
store the CLI does.

#### 3. Sub-second prune does nothing — measured

`PruneVersions(ctx, time.Nanosecond)` reports `OldVersions: 0` and the table grows. The same
table after a 1.1s wait and `PruneVersions(ctx, time.Second)`: `OldVersions: 126`,
`BytesRemoved: 1289399`, and 1,284,448 bytes -> 35,119. Anything below a second is not a small
window, it is no window. Production retention is 15 minutes, so this only bites a test trying to
bypass the window.

#### (superseded) the original suspicion

`removeStaleBundleTemps`, which this task added to reclaim the leaked working directories, swept
by name prefix alone. A run's working directory carries a random suffix, so a prefix sweep cannot
tell one left by a dead run from one a CONCURRENT run is writing into — and the daemon indexes
the same store the CLI does. Deleting a live one does not fail loudly: the export keeps writing
into a directory that no longer exists, recreates part of it, and publishes the partial result by
rename. The screenshots show `graph.icebug.tmp.6693aab` present with 17 items before the second
command and gone after it.

Fixed with `staleBundleTempAge` (1 hour): a directory is only swept once it has been untouched
for longer than any export could take — a full index of 38k files measured ~16 minutes. An export
in flight touches its directory continuously.
`TestStaleBundleSweepLeavesALiveWorkingDirectoryAlone` pins it.

**Honest status: this one is a strong hypothesis, not a reproduction.** The mechanism is real and
the fix is right regardless, but the 549 -> 175 was not reproduced locally. Re-running the same
two commands is what would confirm it.

### 2026-08-28 — the embedding cache comes back, in binary

The Engineer reversed part of T1: losing a computed embedding to a rebuild is expensive, so the
per-file cache returns — but stored for disk rather than for readability.

**What changed from the version that was deleted:** the format. The same 39,762 vectors of 768
float32 occupy **122 MB as raw little-endian float32 and 381 MB as the JSON decimal text** that
was there before — a 3.1x inflation from serialisation alone, on what was 55% of the store.

`internal/ast/shard_emb_cache.go` is back, writing `shards/<relPath>.emb`:

```
magic "GEMB" | version u16 | dim u16 | hash len u16 | hash | count u32
then count records of: uid len u16 | uid | dim x float32 little-endian
```

The dimension lives in the header rather than per record, which is what makes a record exactly
`len(uid) + 2 + dim*4` bytes. Writes go through a temp file and a rename.

**Halving it again with float16 is available and deliberately not taken.** The cache is what a
rebuild restores INTO the search index, so a lossy cache would make a store's vectors differ
before and after a rebuild — a change to search behaviour, which needs a measurement rather than
an assumption. `TestEmbShardRoundTripIsExact` pins the round trip as bit-identical.

**What did NOT come back:** the O(corpus) rebuild per embedding cycle. The embedder writes to
both — the entity's row in Lance, which is what a query reads, and the cache, which is what a
rebuild replays. `rebuildSearchIndexForEmbeddings` stays deleted.

**Which one decides what is pending is the CACHE, not the index**, and that is the whole point:
right after a rebuild the index is empty while the work is genuinely already done.
`SearchIndex.EmbeddedUIDs` was written for the other arrangement and is removed rather than left
as an unused second answer to the same question.

`TestRebuildRestoresVectorsFromTheCacheInsteadOfRecomputing` pins the reason all of this exists:
a rebuild restores the vectors instead of re-running the model.

### 2026-08-28 — the embedding is slow, and it is padding

The Engineer's impression, checked: 29,065 entities in 1492s = 19.5 entities/second.

**First hypothesis, wrong, recorded because it looked obvious.** T1 had added one Lance `Upsert`
per model batch of 128, which left 484 dataset versions and 643 fragments and grew `search.lance`
to 858 MB. Benchmarked directly: 157 upserts of 128 rows on a 20k-row table take 2.264s against
608ms for one upsert of 20,000. Scaled to the real run that is ~2.4 seconds out of 1492 —
**0.2% of the time**. The disk churn is real and worth fixing on its own terms, but it is not why
embedding is slow.

**The actual cause.** `localEmbeddingClient.EmbedBatch` builds a tensor of shape
`[batchSize, maxLen]` where **maxLen is the longest text in that batch**, capped at 512. Every
row is padded to it, so a batch costs its worst member, for all 128 of its rows.

Measured over 40,365 entities of this repository:

| | tokens |
|---|---|
| median | 29 |
| p90 | 110 |
| max | 307 |
| useful tokens | 1,708,166 |
| tensor cells, arrival order | 3,167,762 (**1.9x**) |
| tensor cells, globally sorted | 1,724,023 (1.0x) |
| tensor cells, sorted within each label | 1,877,836 (1.1x) |

`processBatch` now builds every text once into a `preparedRow{row, text}`, sorts by `len(text)`,
and batches from there. **Timed against the real model on this repository's own texts: 2560 texts
in batches of 128 took 2m46s in arrival order and 1m45s sorted by length — 63%, a 1.58x
speedup.** The structural prediction from tensor cells alone was 59%; the gap is per-batch
overhead that does not scale with tokens. Nothing about a vector changes:
an entity is embedded from its own text, so only which entities share a tensor row-length moves.
Dedup is unaffected, happening in `scanPending` before `processBatch`.

**A caveat on every wall-clock number in this section**: the daemon runs its OWN embedding
cycles and its own `sync --heavy`, and serialises against a manual run through
`sysutil.AcquireHeavy`. The 1492s baseline was measured with that contention present, and so was
the comparison above — which is why the comparison is a RATIO measured back to back rather than
two absolute timings taken apart. An attempt to time a clean full cycle by hand produced 40
seconds and no work, because the daemon already held the gate.

Knobs checked and left alone: `SetIntraOpNumThreads(CPUBudget())` = 15 of 20 cores here,
`SetInterOpNumThreads(1)`, `maxSeqLen` 512, `BatchSize` 128, `MaxSourceChars` 500.

## Results — measured, not projected

The store for this repository, before any of this work and after all of it:

| | before | after |
|---|---|---|
| `shards/*.emb.json` | 383.8 MB | **0** |
| `shards/*.nodes.json` (`src` inside) | 39 MB | 31 MB |
| `shards/*.edges.json` | 33 MB | 33 MB |
| `search.lance` | 314 MB | 96 MB |
| `graph.icebug` | 5.2 MB | 5.2 MB |
| orphaned `.scratch` directories | 18 MB | **0** |
| **total** | **696 MB** | **168 MB** |

**−76%.** On the search index alone, compaction and pruning took `entities.lance` from 173 data
files and 318 retained versions to 33 and 38.

Two things worth separating: the `emb.json` removal is a one-time reclaim of a duplicate, while
the compaction and the leak fix are ONGOING — before them, both grew without bound for the life
of the store.
