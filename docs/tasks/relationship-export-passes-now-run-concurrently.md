---
title: The relationship half of exportDirectWithReverse's ~30 passes now run concurrently
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [ast, icebug, performance, concurrency]
---

# The relationship half of exportDirectWithReverse's ~30 passes now run concurrently

> **Extended 2026-08-31, same day.** The node-label loop, described below as staying
> sequential "correctly so," was ALSO parallelized on its write side — on the engineer's
> explicit instruction, after being shown the memory trade-off, to optimize for wall time and
> not preserve the "one table at a time" memory bound. See the addendum at the bottom of this
> doc. Everything below this note describes the state as of the FIRST change of the day; the
> addendum describes what changed after it.

## Objective

Follow-up to
[edge UIDs pointing at a shared declaration now intern corpus-wide](edge-uids-pointing-at-a-shared-declaration-now-intern-corpus-wide.md):
the engineer asked to fix the remaining, larger open item from that investigation —
`exportDirectWithReverse` (`internal/ast/direct_icebug.go:47`) makes on the order of 30
sequential passes over the already-decoded shard cache (`ri.fileEntries`), one per node
label and one per relationship-type/label-pair. The backlog
(`docs/tasks/backlog/the-icebug-rebuild-holds-the-whole-parse-cache-live-in-entri.md`)
described a "reorganise around a single pass" rewrite as the fix, but flagged it as risky:
it has to preserve a load-bearing generation ORDER that stub emission depends on.

## Reasoning

Read `internal/ast/rebuild_index.go` end to end before writing anything, specifically to
answer: which of the ~30 passes actually MUTATE shared state, and which only READ it? The
answer splits the export cleanly in two:

- **The node-table loop** (`for _, label := range labels { writeNodeTableDirect(...) }`,
  `direct_icebug.go:91`) is where `ri.emitUID` gets called — `streamEntities` records every
  real declaration, and for `Function`/`Class`/`Interface`/`Field`/`Table` labels,
  `streamNodeRowsFor` (`direct_icebug.go:717`) ALSO calls the matching `streamStubX`
  (`streamStubFunctions`, `streamStubClasses`, …) in the SAME call, which checks
  `!ri.emittedAny(uid)` before deciding a foreign symbol needs a stub row. Which LABEL's
  stub check runs first — because it comes first in the `labels` slice — decides which
  table an ambiguous foreign uid lands in. This is the order the code's own SAFETY comment
  calls load-bearing, and it genuinely cannot be parallelized without either a lock around
  every `emitUID` call (which would serialize the work anyway) or a real redesign.
- **The relationship-export calls** (everything reached through the old `exportRel`
  closure — `HAS_PARAMETER`, `HAS_FIELD`, annotations, `IMPORTS`, `CALLS`,
  `INHERITS`/`IMPLEMENTS`, `READS_FIELD`/`WRITES_FIELD`, DML, `CONTAINS`) run strictly AFTER
  the node loop finishes. Checked every `streamXEdges` function
  (`streamCallEdges`, `streamInheritEdges`, `streamFieldAccessEdges`, `streamDMLEdges`,
  `streamAnnotationEdges`, `streamContainsFileEntity`, …): none of them call `emitUID`.
  They only call `emittedIn`/`emittedAny` (reads) and `resolveCallee`/`resolveFieldTarget`/
  `resolveRefTarget` (pure functions over `ri.decls`/`ri.rules`, built once in `ri.scan()`
  and never mutated again). By the time this half of the export starts, `ri` is frozen.

So the risky rewrite the backlog worried about was never necessary for this half: with no
mutation happening, the relationship passes can run concurrently exactly as they are today,
as long as the file-writing side (`writeIndicesDirect`/`writeIndptrDirect`, ultimately
`writeParquetDirect`) doesn't share mutable state either — checked: each call creates its
own `os.File`, its own `array.RecordBuilder`, its own `pqarrow.FileWriter`; the only shared
piece is Arrow's `memory.DefaultAllocator`, which is safe for concurrent use by design.

The one thing that DOES need care with concurrent completion is determinism: `relMembers`,
`relReverse`, `usedMembers`, and `man.EdgeCount` are shared accumulators, and the ORDER
`relMembers[relType]` gets appended to is what makes two exports of an unchanged corpus
produce byte-identical `icebug.json` (the file has its own comment about this, re: map
ranging). Fixed by keeping the accumulation sequential and separate from the concurrent
work: build the job list in the EXACT order the old code called `exportRel`, run the
computation for each job in parallel, then replay the results into `relMembers`/
`relReverse`/`usedMembers`/`man.EdgeCount` in that SAME original order afterward — so
which goroutine happens to finish first never affects the output.

## Implementation

`internal/ast/direct_icebug.go`, `exportDirectWithReverse`:

- The node-table loop is UNCHANGED — still sequential, still exactly where stub placement
  is decided.
- `exportRel` is split into `computeRelJob` (everything except updating
  `relMembers`/`relReverse`/`usedMembers`/`man.EdgeCount`) and a job list (`[]relJob`,
  built with `addJob`, in the exact call order the old sequential code used).
- `parallelForEach` (already in `internal/ast/throttle.go`, used elsewhere in this package)
  runs `computeRelJob` over the job list with `SafeWorkers(0)` workers (the project's shared
  CPU-budget helper, same one LadybugDB/ONNX threading already uses). Each `relResult`
  carries the job's original index; `collect` (which `parallelForEach` guarantees runs on
  the calling goroutine, so it needs no lock) writes it into `results[idx]`.
- After all jobs finish, a plain sequential loop over `results` in original index order does
  exactly what `exportRel` used to do inline: check for a first error, then the
  `usedMembers` collision check, then append to `relMembers`/`relReverse`, then
  `man.EdgeCount += `. Byte-for-byte the same operations in the same order — only WHEN the
  expensive part (streaming edges, building the CSR, writing Parquet) happens moved, from
  inline-and-sequential to concurrent-and-collected-after.

## Verification

- `TestDumpBundle` (`internal/ast/icebug_bundle_dump_probe_test.go`, renders a whole bundle
  — every Parquet's schema, row/row-group counts, and a SHA-256 of its column values — to
  stable text) run 3 times after this change: byte-identical output every time.
- Same test run once more under `go test -race`: byte-identical to the non-race runs, no
  race reported.
- Same test run against `git stash` (i.e. the pre-parallelization code, one commit back):
  **byte-for-byte identical** to the parallelized output. This is the strongest evidence
  available that the refactor changed nothing observable — same schema, same row counts,
  same column values, same file names, same manifest ordering.
- `go test ./internal/ast/... -timeout 600s`: green, 53s (matches pre-change baseline).
- `go test -race -count=1 ./internal/ast/ -timeout 600s`: green, 95s, no races.
- `go vet ./internal/ast/`: clean.
- Timed with a throwaway probe (not committed) against this repository's own real AST store
  (852 files, 228k+ edges), 3 runs each, `git stash` for the before/after comparison:

  | | `exportDirectWithReverse` | scan + export total |
  |---|---|---|
  | before (sequential) | ~808–818 ms | ~865–874 ms |
  | after (parallel) | ~427–486 ms | ~479–543 ms |

  **~1.8x** on this repository's own (small, 852-file) store. A larger, more polyglot
  corpus is expected to benefit more: it has more distinct caller/callee label pairs, DML
  source/target label combinations, and inheritance label pairs — i.e. more independent
  jobs to spread across workers — and each job's own pass over `ri.fileEntries` is more
  expensive because the corpus is bigger.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/direct_icebug.go` | Modified | `exportRel` split into `computeRelJob` (parallel, read-only) + a sequential job list and merge loop; `writeNodeTableDirect` removed and replaced with a two-phase collect-then-parallel-write over every label |
| `docs/tasks/backlog/the-icebug-rebuild-holds-the-whole-parse-cache-live-in-entri.md` | Modified | recorded as done; clarified this is a wall-clock fix, not a memory fix |

## What this does NOT fix, and one thing it deliberately UNDOES

- **Peak memory is now WORSE for the node-table phase, on purpose.** The pre-addendum
  `writeNodeTableDirect` held one label's table at a time by design ("the peak is the
  largest single table instead of the sum of every one of them" — a lesson from a real OOM).
  The addendum collects every label's table before writing any of them, so peak memory
  during node writing is now the SUM of every label's table, not the largest one. This was
  an explicit, informed choice by the engineer after the trade-off was described, not an
  oversight — but it means a future OOM investigation on a very large corpus should look
  here first, not assume this discipline still holds.
- **`ri.fileEntries` itself is still fully resident** regardless of either change here —
  concurrency parallelizes reads of it, it never shrinks what has to be resident. The
  ~14.6 GB O(corpus) figure in the backlog doc is about THAT retention and is untouched by
  either change in this task.
- **The "single pass, reorganized around the output" rewrite the backlog described** would
  have fixed the memory side while ALSO speeding things up. This task took the faster,
  narrower route instead — it fixes wall time on both halves of the export, and on the node
  half it does so by spending more memory rather than avoiding the rewrite's complexity in a
  memory-neutral way.

## Progress Log

### 2026-08-31
Read `rebuild_index.go` in full to establish which of the ~30 passes mutate shared state
(only the node-table loop's stub emission does) versus which are pure reads (every
relationship-export pass). Split `exportRel` into a parallel-safe compute step and a
sequential merge step preserving the original call order, using the project's existing
`parallelForEach` helper rather than introducing a new concurrency primitive. Verified with
`TestDumpBundle` (3 runs, once under `-race`, once against the pre-change code via `git
stash`) that output is byte-for-byte identical; verified the full `internal/ast` suite green
both with and without `-race`. Measured ~1.8x on this repository's own real store with a
throwaway (uncommitted) timing probe.

### 2026-08-31 — addendum: the node loop's write side, on explicit instruction

Asked afterward whether the (still sequential) node-label loop could also be sped up.
Measured its own breakdown with another throwaway probe: of ~263ms total, ~118ms is
`streamNodeRowsFor` (collect — mutates `ri.emitUID`, must stay sequential for stub
placement) and ~145ms is sort+id-assignment+Parquet write (pure, once a label's rows are
already collected — same shape as the relationship half).

Flagged the trade-off before touching anything: `writeNodeTableDirect`'s existing comment
says rows are "generated, written and released inside this loop... so the peak is the
largest single table instead of the sum of every one of them" — a deliberate design that
came out of a real OOM incident in production (see
`docs/tasks/the-parse-cache-stops-paying-for-the-same-string-twice.md`). Collecting every
label's table up front so the writes can run in parallel reintroduces exactly that "sum of
every table" peak, on the same code path that OOM incident touched.

**The engineer's explicit answer: optimize for wall time, do not preserve that memory
bound.** Implemented accordingly: `writeNodeTableDirect` is gone; the node-label loop is now
two phases — collection stays a single sequential pass in the original `labels` order
(unchanged, still what decides stub placement), then EVERY label's already-collected table
is hand off to `parallelForEach` for the sort+ids+Parquet-write step, all held in memory at
once rather than one at a time. `man.NodeTables` needed no order-preserving merge here
(unlike the relationship half): it is re-sorted by label right after the loop regardless of
insertion order, so `labelIDs` (a map) and `man.NodeTables` are both fine being populated in
whatever order `parallelForEach`'s completion callback happens to deliver.

Verified the same way as the first change: `TestDumpBundle` 3 runs (incl. one under `-race`,
one diffed against the pre-this-addendum code via `git stash`) — byte-for-byte identical
every time. Full `internal/ast` suite green with and without `-race` (52s / 95s). Timed on
this repository's own real store: `exportDirectWithReverse` **334–397 ms**, down from
427–486 ms (relationship-only parallel) and from the original 808–818 ms sequential baseline
— **~2.25x cumulative** on this small (852-file) corpus.
