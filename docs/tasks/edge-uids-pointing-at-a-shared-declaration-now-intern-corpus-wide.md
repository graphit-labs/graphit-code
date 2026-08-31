---
title: Edge UIDs pointing at a shared declaration now intern corpus-wide, not per-file
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [ast, icebug, shards, memory, interning]
---

# Edge UIDs pointing at a shared declaration now intern corpus-wide, not per-file

## Objective

The engineer reported the writing phase of `graphit ast index` (after parse, before and
after the Parquet writes) is slow, and suspected the shard UID "repeats and takes up a lot
of space." Investigate `internal/ast/shard_compact.go` — which already interns strings at
shard adoption (see [the parse cache interning task](the-parse-cache-stops-paying-for-the-same-string-twice.md)) — for whether the design missed a category of duplication, and fix
what is found.

Also folded in: the engineer's belief that the local, on-the-fly Ladybug is still a
file-based catalog with a copy+swap publish step. It is not — see the correction below.

## Correction: the copy+swap / `Close()` cost this repo's memories described no longer exists

Prior investigation in this session (and several project memories) described the local
graph as a `ladybugdb` file, published via `CopyDBDir` → `AtomicSwapDB`, with the dominant
incremental cost being `Shutdown`+`Close()` on the mutated copy (measured 7–17s, documented
in `docs/upstream/liblbug-close-and-unwind.md` and memory
`01M0CWRQ6VBRTPQBTK6T4ZNW3G`). **That architecture was removed** by
[Local Ladybug via in-memory Icebug filesystem, no swap and no legacy](local-icebug-filesystem-in-memory.md)
(commit `4796672`): the catalog is now always `:memory:`, mounted per-connection from
`graph.icebug/schema.cypher`; `AtomicSwapDB`, `CopyDBDir`, `.wal`/`.shadow`/`CHECKPOINT` and
the files that implemented them (`incremental_rebuild.go`, `json_rebuild.go`,
`ladybug_registry.go`, `reflink_*.go`) are deleted. Publishing a rebuilt bundle is a directory
rename (`tmp.<hex>/` → `graph.icebug/`), not a database `Close()`. **The `Close()` latency
bottleneck no longer applies to the local, on-the-fly graph.**

What is still true and still structural (see `internal/ast/icebug_rebuild.go` and
`internal/ast/direct_icebug.go` today):
- Before any Parquet is written, `rebuildIcebugFromCacheWithDelta` /
  `newRebuildIndex.scan()` decode the WHOLE shard cache into `ri.fileEntries` — even on the
  "incremental" path, which only decides WHICH Parquets get rewritten, not where the rows
  come from. This is a consequence of icebug being a global CSR: a node's id is the dense
  index inside its label's Parquet table, so assigning ids for one changed file still
  requires knowing that label's full current node set. See
  [the icebug rebuild still holds the whole corpus live](the-icebug-rebuild-holds-the-whole-parse-cache-live-in-entri.md)
  (open, unresolved by this task).
- While writing Parquets, `exportDirectWithReverse` (`internal/ast/direct_icebug.go:47`) makes
  one sequential pass per node label plus one per (relationship type, from-label, to-label)
  triple — on the order of 30 passes over the decoded shards for a typical corpus, with no
  parallelism between them. Also open, same backlog doc.

Neither of those is what this task fixes. This task fixes a real but narrower thing: how
much of `ri.fileEntries` there is to begin with.

## Reasoning

`shard_compact.go` interns every repeated string at shard adoption into one of two tables:
a corpus-wide `shared` table (for values whose cardinality is bounded by the grammar or the
file count — paths, langs, labels, `ModuleUID`, `References.TargetUID`, …) and a per-file
`local` table, discarded once the file is compacted, "for identifiers that repeat only
within it."

Reading the field list assigned to `local` against that stated criterion turned up three
fields that do NOT repeat only within their own file: `Calls.CalleeUID`,
`Inheritance.ParentUID`, and `FieldAccess.FieldUID`. Each names a declaration the referencing
file merely POINTS AT, and a popular one — a widely-called utility function, a common base
class, a public struct field — is pointed at by many DIFFERENT files, not just repeated
calls within one. That is exactly the shape `ModuleUID` and `References.TargetUID` already
recognized and put on the shared table; `CalleeUID`/`ParentUID`/`FieldUID` were left on
`local` by oversight, where a per-file table discarded after each file cannot see the
repetition at all.

Measured directly against this repository's own AST store (852 files sampled,
`GRAPHIT_SHARD_FOOTPRINT=~/.graphit/ast/project/01KSH1CRFFG8Z74B5ZS78WW808`, instrumented
`TestShardCacheStringDuplication` to also sum each field's PER-FILE distinct count and
compare it to the GLOBAL distinct count — the gap is duplication a local table cannot see):

| field | occurrences | distinct globally | distinct if only local-interned (summed per file) | cross-file duplicates local misses |
|---|---|---|---|---|
| `Calls.CalleeUID` | 62,339 | 3,981 | 18,875 | **14,894 (24% of all Calls rows)** |
| `FieldAccess.FieldUID` | 62,385 | 3,448 | 18,655 | **15,207 (24% of all FieldAccess rows)** |
| `Calls.CallerUID` (control) | 62,339 | 6,570 | 6,570 | 0 |
| `FieldAccess.SourceUID` (control) | 62,385 | 6,165 | 6,165 | 0 |
| `Contains.ParentUID`/`ChildUID` (control) | — | — | — | 0 |

The controls confirm the rest of the design is correct as-is: `CallerUID`, `SourceUID`, and
both `Contains` UIDs really do repeat only within their own file, so `local` is the right
table for them and this task leaves them alone. `Inheritance.ParentUID` has no occurrences
in this Go-heavy corpus to measure (no `INHERITS`/`IMPLEMENTS` edges), but is the same shape
by construction — a base class inherited from many files — and was moved on that reasoning,
not on a measurement this corpus can produce.

This repo's own store is small (852 files); a 24% cross-file duplication rate on it is a
lower bound, not an upper one — a larger, more re-used corpus is expected to show a heavier
tail on its most-called functions and most-inherited base classes, which is exactly what
Zipfian call-frequency distributions do at scale.

## Implementation

`internal/ast/shard_compact.go`: `Calls.CalleeUID`, `Inheritance.ParentUID`, and
`FieldAccess.FieldUID` now go through `shared.of(...)` instead of `local.of(...)`. Nothing
else changed — the shard format on disk is unchanged (values, not their allocation, is all
interning touches), so no `shardCacheVersion` bump.

`internal/ast/shard_compact_test.go`: `compactionCorpus()` gained a second `Inheritance` and
`FieldAccess` row per file pointing at a shared literal (`external:CommonBase`,
`external:CommonBase.f`) so the fixture can exercise cross-file sharing, which the previous
fixture (all UIDs prefixed by the file's own path) could not. `TestCompactionSharesRepeatedStrings`
gained three positive assertions (CalleeUID/ParentUID/FieldUID share pointer identity across
`pkg/one.go` and `pkg/two.go`) and one negative control (`CallerUID` must NOT be folded
across the two files, proving the change did not make `local` a no-op).

`internal/ast/shard_cache_footprint_probe_test.go`: `TestShardCacheStringDuplication`
extended to report a per-field "local-only-distinct (summed per file) vs. global distinct"
comparison — the tool that produced the measurement above, kept so a future change to this
design can be checked against real data instead of re-argued from first principles.

## Test Cases & Acceptance Criteria

```gherkin
Given two different files whose Calls both reference the same external CalleeUID
When both are read back through the shard cache
Then their CalleeUID fields point at the same string data

Given two different files whose Inheritance both reference the same external ParentUID
When both are read back through the shard cache
Then their ParentUID fields point at the same string data

Given two different files whose FieldAccess both reference the same external FieldUID
When both are read back through the shard cache
Then their FieldUID fields point at the same string data

Given two different files with different CallerUIDs
When both are read back through the shard cache
Then their CallerUID fields do NOT point at the same string data
```

- `go test ./internal/ast/... -timeout 600s`: green (52s for the core package).
- `go vet ./internal/ast/`: clean.
- `TestShardCacheStringDuplication` against this project's own real store
  (`GRAPHIT_SHARD_FOOTPRINT=~/.graphit/ast/project/01KSH1CRFFG8Z74B5ZS78WW808`): confirms the
  24%/24% cross-file gap pre-fix, and confirms via `TestCompactionSharesRepeatedStrings` that
  the fix closes it.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/shard_compact.go` | Modified | `CalleeUID`/`Inheritance.ParentUID`/`FieldAccess.FieldUID` moved from the per-file table to the corpus-wide table |
| `internal/ast/shard_compact_test.go` | Modified | fixture gained shared-literal rows; new cross-file assertions, positive and negative |
| `internal/ast/shard_cache_footprint_probe_test.go` | Modified | duplication probe now reports the local-vs-shared gap per field |

## Technical Debt (unchanged, not addressed by this task)

- [ ] The rebuild still decodes the whole shard cache before any Parquet is written —
  `docs/tasks/backlog/the-icebug-rebuild-holds-the-whole-parse-cache-live-in-entri.md`.
- [ ] `exportDirectWithReverse` makes ~30 sequential passes over the decoded shards, one per
  label/relationship-type — same backlog doc, "Reorganise the export around a single pass."
  This is the more consequential fix for a large corpus and was not attempted here: it is a
  rewrite of `exportDirectWithReverse` that must preserve the load-bearing stub-emission
  generation order (T3 constraint, see that doc), and deserves its own plan rather than being
  folded into a memory-footprint fix.
- [ ] `entity.UID` (the DECLARATION's own uid, not a reference to one) remains un-interned
  and is measured elsewhere as almost entirely distinct — correctly so, since it is a
  primary key. Not the same field family as this task's fix.

## Progress Log

### 2026-08-31
Corrected the engineer's premise that the local graph still uses a file-based catalog with
copy+swap — that architecture was removed on 2026-08-27/28
(`local-icebug-filesystem-in-memory.md`); several project memories describing its `Close()`
cost are now historical, not current.

Investigated the shard interning design (`shard_compact.go`) against its own stated rule
("local holds identifiers that repeat only within one file") and found three fields that
violate it: `Calls.CalleeUID`, `Inheritance.ParentUID`, `FieldAccess.FieldUID`. Measured the
violation directly against this repository's own AST store rather than assuming it:
extended `TestShardCacheStringDuplication` to compare global distinct values against the sum
of each file's own distinct values, isolating cross-file duplication specifically. Found 24%
of `Calls` rows and 24% of `FieldAccess` rows carry a UID that a per-file table structurally
cannot deduplicate, because two different files each get their own table, discarded after
use.

Fixed by moving the three fields to the corpus-wide `shared` interner, matching `ModuleUID`
and `References.TargetUID`, which already had this right. Added fixture rows and assertions
proving the fix (and a negative control proving it did not overreach). Full `internal/ast`
suite green, `go vet` clean.
