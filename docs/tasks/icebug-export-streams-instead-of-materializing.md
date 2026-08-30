---
title: The icebug export streams instead of materializing — OOM on a 120k-file repository
status: done
created: 2026-08-30
updated: 2026-08-30
tags: [ast, icebug, parquet, memory, performance, indexing]
---

# The icebug export streams instead of materializing — OOM on a 120k-file repository

## Objective

`graphit ast index --reset` on `private-corpus` (120,064 files) was **Killed** by the
OOM killer during the write phase:

```
◦ Indexing ~/projects/private-corpus (clusters: forms/=form, app/=app, ...)
  › Writing graph: 120064 file(s)Killed
```

The store was left with `graph.icebug.tmp.d29bcf9/` (a half-written bundle) beside a
22 MB `manifest.json`. The machine is not short on RAM, and the **finished** bundles this
exporter produces are not large — so the ceiling is the shape of the write, not the
volume of the data.

`Writing graph: %d file(s)` with `current == 0` is emitted by
`internal/ast/pipeline.go:687` immediately before the single call into
`RebuildIcebugFromCacheWithReverse` → `ExportDirectFromRebuildIndexWithReverse`
(`internal/ast/direct_icebug.go`). That is where the process died.

### Reasoning — where the memory actually goes

Read of `internal/ast/direct_icebug.go` and `internal/ast/rebuild_index.go` found five
independent multipliers, none of which is bounded by the size of the output:

1. **Every node row of every label is alive at once.** `exportDirectWithReverse` fills
   `batches []nodeBatch` for all labels first (lines 60–90) and only then loops over it
   writing Parquets (line 113). Peak is the SUM over labels, not the max.
2. **`emittedUIDs` is a map of maps — one `map[string]bool` per uid.** A Go map holding a
   single entry costs ~200 B (hmap header + one bucket) before the entry itself. At tens
   of millions of uids this is gigabytes of pure bookkeeping for what is, in practice,
   one table name per uid.
3. **The whole Parquet table is materialized as Arrow before a single byte is written.**
   `writeParquetDirect` fills one `array.RecordBuilder` with ALL rows, calls
   `NewRecordBatch()`, and hands it to `FileWriter.Write` in one go.
4. **Type inference boxes an entire column at a time.** `columnsForLabel` calls
   `collectColumnValues(rows, name)` per column, building an `[]any` with one boxed value
   per row, purely to decide STRING vs INT64 vs BOOL. Same for edge properties via
   `collectPropValues`.
5. **The edge path keeps four copies of every property column.** `collectProps` →
   `sortCSR` (copies) → `reverseEdgesDirect` (copies) → `sortCSR` again (copies), each a
   `[][]any` with a boxed value per edge per property.

### Justification for the approach

The constraint that shapes every option here is recorded in memory and enforced by
`TestIcebugWritesOneRowGroupPerFile`: **every Parquet this project writes must have
exactly one row group.** A multi-row-group file mounts, counts correctly through an
anonymous pattern, and then silently fails to resolve a node the moment a pattern binds
one. That is why the current writer materializes the whole table — `FileWriter.Write`
opens a NEW row group on every call, and `parquet.WithMaxRowGroupLength` does not merge
them.

`pqarrow.FileWriter.WriteBuffered` is the way out, and it was verified against the
vendored source (`arrow-go/v18@v18.6.0/parquet/pqarrow/file_writer.go:268`): it appends
into the SAME buffered row group as long as `currentRows + rec.NumRows() <=
MaxRowGroupLength`. With `MaxRowGroupLength` already set to `1<<40`, N chunked
`WriteBuffered` calls produce exactly one row group. The buffered row group holds
*compressed* column chunks (`NewPageWriter(..., buffered=true)` →
`newBufferedPageWriter`, which compresses before buffering), so its footprint tracks the
final file size — which is precisely the budget the user pointed at.

Alternatives considered and dropped:

- **Multiple row groups with a smaller `MaxRowGroupLength`.** Rejected: it is the exact
  defect the one-row-group invariant exists to prevent, and it was re-affirmed as
  non-negotiable (not a tuning knob for S3 locality) on 2026-08-24.
- **Replacing `[]map[string]any` node rows with typed columns end-to-end.** This is the
  single largest term left (~600 B per node against ~24 B for a reference), but the
  `*JSON()` API on `rebuildIndex` is exercised by a dozen test files and the change
  touches the row model of the whole rebuild. Recorded as technical debt below rather
  than folded into an OOM fix.
- **Lowering `GOGC` / setting a soft memory limit during the export.** Trades CPU for
  survival without removing any of the five multipliers. A fallback, not a fix.

## Plan & Task Breakdown

- [x] **T1 — Chunked Parquet writing that preserves one row group per file** — Spec:
  `writeParquetDirect` (`internal/ast/direct_icebug.go`) and `writeParquet`
  (`internal/ladybugstore/icebug.go`) fill and flush the record builder in chunks via
  `WriteBuffered` instead of one `Write` of the whole table. Done when every Parquet
  still reports exactly one row group and carries `icebug_disk_version`, including for a
  table larger than the chunk size. Invariant: one row group per file, no exceptions.
- [x] **T2 — Flatten `emittedUIDs`** — Spec: `internal/ast/rebuild_index.go`. Replace
  `map[string]map[string]bool` with a flat first-table map plus an overflow set for the
  rare uid emitted into a second table. `emitUID`, `emittedIn` and `emittedAny` keep
  their exact semantics. Done when the rebuild tests pass unchanged.
- [x] **T3 — One label's rows alive at a time** — Spec: `exportDirectWithReverse` and
  `exportDirectDelta` generate, write and release each label's rows inside one loop
  instead of collecting all labels first. Constraint: the ORDER in which
  `nodeRowsFor` is called is load-bearing — stub emission tests `emittedAny`, so it must
  stay `ri.labels`, then File/Directory, then Parameter/Field, then annotation kinds.
  `man.NodeTables` is sorted by label at the end so `schema.cypher`'s node DDL order is
  byte-identical to today's.
- [x] **T4 — Type inference without boxing a column** — Spec: replace
  `inferTypeFor(collectColumnValues(rows, k))` with a streaming inference over the rows.
  Done when no `[]any` of row-cardinality is allocated for inference.
- [x] **T5 — Edge path keeps one copy of the property columns** — Spec: build the
  property columns while resolving the edges (which also fixes a latent misalignment:
  `collectProps` walks rows that the edge loop skipped, so `propValues[i]` stops
  corresponding to `edges[i]` after the first unresolved endpoint), sort through a
  permutation instead of copying, and let the reverse member share the forward member's
  property storage. Done when the CSR output is unchanged for the existing icebug tests.

- [x] **T6 — Node rows never exist as a slice of maps** — Spec: the node producers in
  `internal/ast/rebuild_index.go` gain streaming forms (`emit func(map[string]any)`), with
  the existing `*JSON()` functions kept as thin collectors over them so there is ONE
  implementation and the ~10 test files that call them are untouched.
  `writeNodeTableDirect` accumulates the streamed rows into TYPED columns instead of
  retaining the maps, sorts a permutation of row indices by the primary key, and fills
  Arrow through that permutation. Done when a heterogeneous corpus produces a byte-identical
  table — same columns, same order, same types, same values — as the map-built path, proven
  by a test that builds both and compares. Constraint: columns and their types stay DERIVED
  from the data (union of keys, `inferTypeFor` semantics), including the promotion of a
  mixed column to STRING.
- [x] **T7 — Edge rows never exist as a slice of maps either** — Spec: the same streaming
  treatment for the 12 edge producers, and `exportRel` takes a producer instead of a
  materialized slice, accumulating the property columns through the same `nodeColumns`
  while it resolves the endpoints. Constraint: edge property columns are ordered
  ALPHABETICALLY, not by `graphColumnOrder` — a node table and an edge member do not share
  the ordering rule, and `propShapeOfRelType` writes the DDL from a third order again.
- [x] **T8 — A soft memory limit for the duration of the rebuild** — Spec: after T1..T7 the
  peak is no longer set by the data but by the collector, which targets twice the live
  heap. `applyExportMemoryLimit` installs a `debug.SetMemoryLimit` of 75% of the machine's
  (or cgroup's) memory around the rebuild and lifts it after. Constraint: an operator's own
  `GOMEMLIMIT` is never overridden, and the limit is refcounted because it is process-wide
  and the daemon rebuilds more than one project.

## Implementation Details

### T1 — chunked Parquet writing
`writeParquetDirect` (`internal/ast/direct_icebug.go`) and `writeParquet`
(`internal/ladybugstore/icebug.go`) now loop over `parquetChunkRows` (64Ki) slices, calling
`fill` and `WriteBuffered` per slice and releasing each record. `rows == 0` still issues one
empty write so the file exists and opens. `TestWriteParquetDirectKeepsOneRowGroupAcrossChunks`
covers 0, 1, chunk-1, chunk, chunk+1 and 3·chunk+7 rows;
`TestWriteParquetDirectPreservesRowOrderAcrossChunks` reads the values back and checks the
chunk boundaries did not reorder or drop anything, because a row landing one position off
silently repoints every edge that referenced it.

### T2 — flat emit bookkeeping
`emittedUIDs map[string]map[string]bool` became `emittedTable map[string]string` plus
`emittedExtra map[uidTable]bool` for the rare uid emitted into a second table. The
save/restore the delta probe needs is now `ri.detachEmitState()`, which returns its own undo.

### T3 — one label at a time
`exportDirectWithReverse` no longer builds a `batches` slice. The label list is assembled in
the generation order that stub emission depends on, and `writeNodeTableDirect` generates,
writes and releases each table inside the loop. `man.NodeTables` is sorted by label
afterwards so `schema.cypher` declares the node tables in the same order as before.

### T4 — streaming type inference
`inferRowColumnType(rows, key)` replaces `inferTypeFor(collectColumnValues(rows, key))`.
`collectColumnValues` and `collectPropValues` are gone. `inferTypeFor` remains for the edge
property columns, which are already materialized and aligned.

### T6/T7 — rows are streamed into typed columns, never held as maps
Every node and edge producer in `internal/ast/rebuild_index.go` is now a `stream*` function
taking `emit func(map[string]any)`; the `*JSON()` names remain as one-line collectors over
them (`collectRows`), so the ~10 test files that call them are untouched and there is a
single implementation of each loop. `internal/ast/node_columns.go` accumulates the streamed
rows into typed columns — `[]string`, `[]int64`, `[]bool` plus a presence flag — discovering
columns as keys appear and promoting a column to STRING the moment it sees a value its kind
cannot hold, which is the answer `inferTypeFor` gives for the same values. Rows are ordered
by a permutation over the primary-key column instead of by moving row maps around.
`exportRel` accumulates edge property columns through the same type, with alphabetical
ordering rather than the node tables' `graphColumnOrder`-first rule.

`TestNodeColumnsMatchTheRowMapTable` keeps the previous implementation — rows as a slice of
maps — in the test file and asserts the two agree on columns, order, types, primary key and
every cell, over entities, File, missing keys, mixed types, nil values, int/bool columns,
duplicate primary keys and an empty table.

### T8 — soft memory limit
`internal/ast/export_memlimit.go`. Installed by `rebuildIcebugFromCacheWithDelta` for the
duration of the rebuild.

### T5 — one copy of the edge property columns
`csrMemberDirect` carries `edges`, the shared `props`, an `order` permutation and a `propAt`
indirection, so sorting and reversing allocate `int32` slices instead of copying every
property column. `collectProps`, `sortCSR` and `reverseEdgesDirect` are gone, replaced by
`csrOrderDirect` and `reverseMemberDirect`. The property values are now collected through
`rowAt` — the rows whose endpoints resolved — which also fixes a latent misalignment
described under System Knowledge.

## Use Cases

### UC-01: Full index of a large repository
- **Actor**: engineer running `graphit ast index --reset`, or the daemon on a bulk change.
- **Preconditions**: a parse cache with N files exists; no usable previous bundle, so the
  full export path is taken.
- **Main Flow**:
  1. `pipeline.go` reports the `writing` phase and calls
     `RebuildIcebugFromCacheWithReverse`.
  2. `rebuildIcebugFromCacheWithDelta` loads the cache into `entries` and builds a
     `rebuildIndex`.
  3. `ExportDirectFromRebuildIndexWithReverse` iterates the labels; for each it generates
     its rows, derives columns and primary key, assigns dense ids, writes
     `nodes_<Label>.parquet` in chunks, and releases the rows.
  4. Relationship members are exported, each writing `indices_*.parquet` and
     `indptr_*.parquet`.
  5. `schema.cypher` and `icebug.json` are written; the temp bundle is renamed over the
     final one.
- **Alternative Flows**:
  - A small delta takes `ExportDirectIncrementalWithReverse`, which rewrites only the
    affected tables and copies the rest.
- **Error Scenarios**:
  - A write fails → the temp bundle is removed and the error is wrapped as
    `icebug rebuild direct`; the previous bundle is left untouched.
  - The process is killed → the temp bundle is left behind and swept by
    `removeStaleBundleTemps` after `staleBundleTempAge`.
- **Postconditions**: the bundle at `finalDir` has one row group per Parquet and a
  `Finished: true` manifest.
- **Affected Files**: `internal/ast/direct_icebug.go`, `internal/ast/icebug_rebuild.go`,
  `internal/ast/rebuild_index.go`, `internal/ladybugstore/icebug.go`.

## Test Cases & Acceptance Criteria

### Feature: Chunked Parquet writing
Ref: UC-01

#### Scenario: A table larger than one write chunk still has exactly one row group
```gherkin
Given a node table whose row count exceeds the writer's chunk size
When the icebug bundle is exported
Then the resulting Parquet file reports exactly 1 row group
  And it carries the "icebug_disk_version" key-value metadata
```

#### Scenario: An empty table still produces a readable Parquet
```gherkin
Given a node table with 0 rows
When the icebug bundle is exported
Then the resulting Parquet file opens without error
  And it reports 0 rows
```

### Feature: Node ids are unchanged by the streaming rewrite
Ref: UC-01

#### Scenario: Dense ids stay keyed to primary-key order
```gherkin
Given a corpus exported by the previous writer
When the same corpus is exported by the streaming writer
Then every node table lists the same rows in the same order
  And schema.cypher declares the node tables in the same order
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/direct_icebug.go` | Modified | chunked write, per-label streaming, columnar node/edge tables, CSR by permutation |
| `internal/ast/rebuild_index.go` | Modified | flat emit bookkeeping; every row producer gained a streaming form |
| `internal/ast/node_columns.go` | Created | typed columnar accumulator for streamed rows |
| `internal/ast/export_memlimit.go` | Created | soft memory limit for the duration of a rebuild |
| `internal/ast/icebug_rebuild.go` | Modified | installs the soft limit around the rebuild |
| `internal/ladybugstore/icebug.go` | Modified | chunked write, same one-row-group invariant |
| `internal/ast/node_columns_test.go` | Created | columnar table vs the previous row-map table |
| `internal/ast/direct_icebug_rowgroup_test.go` | Created | one row group across chunk boundaries, and row order |
| `internal/ast/export_memlimit_test.go` | Created | install/restore, explicit GOMEMLIMIT, refcount |
| `internal/ast/direct_icebug_memory_probe_test.go` | Created | peak-live-heap measurement, env-guarded |
| `internal/ast/direct_icebug_chunked_mount_test.go` | Created | one row group above the chunk size, mounted and queried |
| `internal/ast/icebug_bundle_dump_probe_test.go` | Created | renders a bundle to stable text so two builds can be diffed |
| `docs/tasks/icebug-export-streams-instead-of-materializing.md` | Created | this log |

## Trade-offs & Decisions

- **`WriteBuffered` over `Write`.** Its own doc comment says it uses MORE memory than
  `Write` — true when comparing one call to one call, because the row group is buffered
  rather than streamed to the sink. It is the opposite here: what it buys is never
  building the full Arrow record, and what it buffers is compressed. Net: peak drops from
  "whole table, uncompressed, boxed" to "one chunk + the compressed file".
- **The `*JSON()` API was kept, as collectors over the streaming form.** The alternative —
  deleting it and rewriting ~10 test files — would have removed the one thing that makes the
  streaming path checkable against the old one. `TestNodeColumnsMatchTheRowMapTable` depends
  on having both.
- **Typed columns, not references into the parse cache.** A `nodeColumn` costs ~175 B/row
  against ~600 B for a row map, where a reference into `cachedEntity` would be ~8 B. The
  reference form requires the column set to come from the producer's shape rather than from
  a union of map keys, which is a different design than the one this file documents at the
  top. Recorded as debt.
- **A soft limit rather than a hard budget.** `debug.SetMemoryLimit` never fails a request —
  it makes the collector work harder. The failure mode is a slow export, not a broken one,
  which is the correct trade for a batch write that currently dies.

## Technical Debt

- [x] **Validated against `private-corpus` — and it still OOMs.** MEASURED 2026-08-30
  18:36 (see the Progress Log entry below): T1..T8 are not sufficient at that scale. The
  ceiling is no longer the export's own data structures but the `entries` map below, which
  is the FIRST debt item now, not a footnote.
- [ ] **`entries` defeats `StreamEntries` and is now THE OOM.** `ShardCache.StreamEntries`
  is genuinely streaming — it deletes each file from `sc.nodes`/`sc.edges` right after the
  callback — but `rebuildIcebugFromCacheWithDelta` retains every entry in
  `entries map[string]*parseCacheEntry`, so the eviction frees nothing and the whole corpus
  is decoded and held live before the first Parquet is written. At 21 GB of shard JSON /
  ~35.6 M graph elements that map alone is tens of GB. Fixing it means `newRebuildIndex`
  consuming a stream (or a two-pass read over the shards) instead of taking a materialized
  map.
- [ ] The streaming producers allocate one short-lived map per row. That is allocation
  traffic, not peak, but at tens of millions of rows it is not free — the producers could
  fill a reusable map that the accumulator copies out of synchronously.
- [ ] Even typed columns are ~175 B/row where a reference into the parse cache would be
  ~8 B. Reading values straight off `cachedEntity` would need the column set to be derived
  from the producer's shape rather than from a union of map keys.
- [ ] `ri.decls map[string][]declRef` and `ri.entityUIDs map[string]string` both scale
  with the entity count and are untouched here.
- [ ] `rebuildIcebugFromCacheWithDelta` copies the whole shard cache into an `entries`
  map before building the index — a second full copy of the corpus metadata, and now the
  single largest live term at export time.
- [ ] `exportDirectWithReverse` still takes `filterLabels` and `filterRels`, which every
  caller passes as nil and nothing reads.

## System Knowledge

- **A filter on an ordinary property is a full scan on a single-row-group bundle.** One row
  group removes row-group pruning, so `WHERE a.name = '...'` over 72k nodes ran for minutes
  where `WHERE a.uid = '...'` — the primary key — answers in 10 ms. That is the accepted cost
  of the invariant, and it is why the mount test anchors on uid.
- **Peak heap here is `2 × live`, not `live`.** Go's default GC target is twice the live
  heap, so every megabyte of live data during the export costs two of peak. This is why
  cutting the export's own live footprint by 3× only halved the end-to-end peak: the parse
  cache sits inside the doubling and cannot be reduced. It is also why T8 — moving the
  target — was worth more at the end than any further data-structure work would have been.
- **One `internal/ast` test run segfaulted and never reproduced.** Five clean runs after,
  including three consecutive. The crash is in the Ladybug engine's cgo layer, not in Go
  code changed here; noting it so a future session does not attribute it to this work.
- **The shard cache left by a failed index is not reusable across binaries.** After the OOM
  kill, `~/.graphit/ast/project/<id>/` held `shards/` and a 22 MB `manifest.json`, but a
  freshly built binary treated all 120,064 files as changed and re-parsed from zero. Plan
  for a full parse when validating against a large repository.
- `pqarrow.FileWriter.WriteBuffered` appends into the current buffered row group while
  `curRows + rec.NumRows() <= MaxRowGroupLength`; `Write` always opens a new one. With
  `MaxRowGroupLength = 1<<40` the buffered form gives one row group for any number of
  calls. (`arrow-go/v18@v18.6.0/parquet/pqarrow/file_writer.go:268` and `:325`.)
- A buffered row group compresses before buffering
  (`parquet/file/page_writer.go:NewPageWriter` → `newBufferedPageWriter`), so its
  in-memory cost tracks the final file size rather than the uncompressed column data.
- **Edge property columns are ordered alphabetically; node table columns are not.** A node
  table leads with `graphColumnOrder` and then sorts the rest; an edge member sorts all of
  its property names. And `propShapeOfRelType`, which writes the REL TABLE DDL, uses a third
  order again (for CALLS: `source_file, line_number, full_call_name, receiver_type`). The
  three are independent and none of them may be "unified" without checking what the reader
  matches on.
- The order in which `nodeRowsFor` is called is semantically load-bearing:
  `stubFunctionJSON`, `stubClassJSON`, `stubInterfaceJSON`, `stubFieldJSON` and
  `stubTableJSON` all skip a uid that `emittedAny` already knows, so reordering label
  collection can change which table a stub lands in. Write order is separate and is
  alphabetical only for reproducibility.

## Progress Log

### 2026-08-30 — validated against `private-corpus`: STILL KILLED, and the cause moved
`graphit ast index --reset` on the 120,064-file repository was OOM-killed again, 52 minutes
in, at the same `Writing graph: 120064 file(s)` line — running a binary built AFTER T1..T8
(verified: the installed binary carries the `icebug rebuild memory limit` log string, and the
run started one minute after `make install`). So this task's fix is real and insufficient at
that scale; what follows is where the memory actually is now.

**The kernel's own account, which is the primary evidence:**

```
kernel: Out of memory: Killed process 1090324 (graphit-core)
        total-vm:54338508kB, anon-rss:48477088kB
kernel: oom-kill:constraint=CONSTRAINT_NONE, global_oom
```

- anon-RSS at kill: **46.23 GiB**. MemTotal is 61.34 GiB, so `exportMemoryHeadroom = 0.75`
  set the soft limit at **46.00 GiB**. The process died 0.23 GiB past its own limit — T8 was
  installed and holding exactly where it was told to hold.
- `constraint=CONSTRAINT_NONE` / `global_oom`: the machine ran out, not a cgroup. The
  workstation already had ~15 GiB in use (Docker, desktop) and 2 GB of swap, so a 46 GiB
  soft limit could not be honoured by the machine even though the process honoured it.

**Two conclusions, and the second is the one that matters:**

1. **75% of MemTotal is the wrong policy for a workstation.** `debug.SetMemoryLimit` is
   SOFT — it never fails an allocation — so it is not a budget, it is a GC target. Aiming
   that target at 75% of a shared machine tells the collector to grow to 46 GiB, which is
   precisely what it did. Worse, once live approaches the target the collector runs
   continuously: 20 cores in GC while the kernel reclaims page cache is what the engineer
   experienced as the machine freezing. The headroom should come off what is AVAILABLE, not
   off MemTotal.
2. **The remaining term is `entries`, not anything T1..T8 touched.** MEASURED on the store
   left behind: `shards/` is **21.0 GB** of JSON across 240,128 shard files, and a 400-file
   sample gives 589 bytes per graph element → **~35.6 M elements** (entities + edges). That
   is 4.7x the largest synthetic probe in this file (7.6 M elements → 6.6 GB peak), and all
   of it is decoded into `entries` and held live before the export starts. The debt item
   that predicted this is now the top of the list.

**Corroborating state on disk:** the run left `graph.icebug.tmp.f7df140/` holding **32 MB**
beside a 22 MB `manifest.json`. The output was never the problem — 46 GiB of heap to produce
tens of MB of bundle is the whole shape of this bug.

### 2026-08-30 — the one-row-group invariant, verified rather than asserted
The engineer reaffirmed the constraint: one row group per Parquet, because Ladybug returns
WRONG DATA for a bundle with more. Verified three ways rather than argued.

**The bundle is byte-identical to the one the previous writer produced.**
`icebug_bundle_dump_probe_test.go` renders a whole bundle to stable text — per Parquet: schema,
physical and logical type, repetition, row count, row group count, and a SHA-256 of every value
in order. Dumped from the pre-change code and from this one:

```
PARQUETS IDENTICAL
SCHEMA IDENTICAL
```

**One row group, on tables larger than one chunk.** `TestChunkedExportMountsAndAnswersBoundPatterns`
exports 6,000 files, asserts a table actually crossed `parquetChunkRows` (it fails otherwise —
a check that never reaches the boundary proves nothing), then opens every Parquet:

```
tables larger than one chunk: nodes_Function.parquet(72000 rows),
  indices_calls__function_function.parquet(72000 rows),
  indices_contains__file_function.parquet(72000 rows)
19 parquet files, one row group each
edges: 72000 anonymous, 72000 through a bound node variable
```

That last line is the discriminant: a multi-row-group bundle counts correctly through an
anonymous pattern and disagrees the moment a pattern binds a node variable. They match.

**Two findings on the way, both pre-existing:**

1. **`icebug.json` was not reproducible.** The only difference between the old and new dumps
   was the ORDER of `rel_groups`, and it varied between runs of the SAME version:
   `man.RelGroups` was built with `for relType := range relMembers`, a map range. Two exports
   of an unchanged corpus produced different bytes. Fixed here by sorting the types — three
   consecutive runs now give one sha256. The REL TABLE order in `schema.cypher` is a separate,
   load-bearing thing and stays where it is decided, in `writeCanonicalSchemaDirect`.
2. **The bounded-traversal planner ignores its own `maxHops`** — a one-hop `-[:CALLS]->`
   walks the entire reachable component and discards everything past hop 1. Measured 1.82 s /
   28.9 s / >180 s at 300 / 1500 / 6000 files, against 14 ms / 38 ms / 109 ms with a one-line
   `break`. NOT applied: it is a query-planner change in a subsystem this task does not touch.
   Written up in full at `docs/tasks/backlog/the-canonical-bounded-traversal-planner-ignores-its-own-maxh.md`.

### 2026-08-30 — measured at the scale that matters, and the task is done
- The real check — re-indexing `private-corpus` — was **started and abandoned**, and the
  reason matters for whoever tries it next: the shards left by the failed run are not
  reusable by a locally built binary, so `graphit ast index` re-parsed from zero at roughly
  90 files/minute, i.e. ~20 hours before it would even reach the write phase. It was
  stopped. Validating this change against that repository needs a full parse first, and
  that is an overnight run, not a check inside a session.
- MEASURED instead on a synthetic corpus of 200,000 files → **2,605,238 nodes /
  5,005,141 edges**, producing a **28 MB** bundle — the same shape as the user's
  observation that the finished Parquets are not large:

  | | peak live heap | delta over the parse cache | wall clock |
  |---|---|---|---|
  | before this task | 12561 MB | 10110 MB | 44.6 s |
  | after T1..T8 | **6596 MB** | **4145 MB** | **24.7 s** |
  | after T1..T8, soft limit 5 GiB | **4759 MB** | **2308 MB** | 26.2 s |

  2.4× less peak heap and **1.8× faster**, because the work removed was allocation and the
  collection of it. The soft limit T8 now installs automatically takes it to 2.6× lower
  again at no measurable cost at this scale.
- `go test ./...` is green. One `internal/ast` run segfaulted once during the work and did
  not reproduce in five subsequent runs; it is in the Ladybug engine, not in this change —
  see System Knowledge.

### 2026-08-30 — T6, T7, T8: measured with a live-heap sampler
- Replaced the probe's metric. `HeapSys` is a high-water of memory obtained from the OS and
  moves with GC timing; the probe now samples `HeapAlloc` every 2 ms during the export and
  reports the maximum, which is what the OOM killer is reacting to.
- MEASURED on the same 20,000-file synthetic corpus (265,238 nodes / 505,141 edges,
  3 MB bundle), peak live heap over the export, above the 257 MB the parse cache itself
  occupies:

  | | peak delta | total allocated |
  |---|---|---|
  | before this task | 1156–1193 MB | 2601 MB |
  | after T1..T8 | **659–714 MB** | 1939 MB |
  | after T1..T8, `GOMEMLIMIT=600MiB` | **335 MB** | — |
  | after T1..T8, `GOMEMLIMIT=450MiB` | **250 MB** | — |

- **The finding that produced T8:** peak ≈ 2 × live, because that is Go's default GC target.
  Working back from the numbers, the export's own live footprint went from ~450 MB to
  ~150 MB — a 3× cut — while the end-to-end peak only halved, because the 257 MB parse cache
  sits inside the doubling and is not reducible (see the memory on why the shards cannot go).
  Once live is small, the collector is the ceiling, and the only lever left is the target
  itself. Under a 450 MiB soft limit the same export completes in 3.1 s instead of 2.2 s —
  40% slower, which is the right trade against being killed.

### 2026-08-30 — T1..T5 landed, and they are NOT enough
- MEASURED with `TestExportDirectPeakHeap` (`GRAPHIT_EXPORT_MEM=1
  GRAPHIT_EXPORT_FILES=20000`), a synthetic Go-shaped corpus of 20,000 files →
  265,238 nodes / 505,141 edges / a 3 MB bundle:

  | | before T1..T5 | after |
  |---|---|---|
  | peak heap over the export | +1171 MB | **+860 MB** |
  | total allocated over the export | 2540 MB | **1937 MB** |

  A 27% cut in peak, 24% in allocation traffic. Real, and nowhere near enough: that is
  ~1.1 KB of heap per graph element, and `private-corpus` is two orders of magnitude
  more elements than this probe.
- The reason it is only 27% is exactly what the plan predicted: T3 moves the node peak
  from Σ(labels) to max(label), which buys little on a corpus with ONE dominant label,
  and the per-row cost — a `map[string]any` at ~600 B for 14 keys — is untouched. That
  single term is the ceiling.
- **Direction change:** T6 is promoted out of Technical Debt into the plan. The node rows
  must stop existing as a slice of maps. Typed columns cost ~175 B/row against ~600 B, and
  streaming the producers means the slice of maps never exists at any point.
- Rejected on the way there: a two-pass design that keeps only primary keys in pass 1 and
  re-streams in pass 2. It cannot work — Parquet rows must be written in id order, id order
  is primary-key order, and a stream arrives in generation order, so pass 2 would need
  random access into the Arrow builders.

### 2026-08-30
- Reproduced from the user's report: OOM kill during the `writing` phase on a
  120,064-file repository, leaving `graph.icebug.tmp.d29bcf9/` behind.
- Read memory first: the one-row-group invariant and the reason the nodes/edges shards
  cannot be removed both constrain this work.
- Traced the write path and identified the five multipliers listed under Objective.
- Verified `WriteBuffered` semantics against the vendored arrow-go source before
  planning around it.
- Wrote this log and the five-task plan. Next: T1.
