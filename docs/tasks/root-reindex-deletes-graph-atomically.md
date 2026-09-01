---
title: "`ast index --reindex` at the root deletes the graph node by node — minutes of silence that look like a hang"
status: done
created: 2026-08-20
updated: 2026-08-20
tags: [ast, reindex, performance, delete, progress, large-corpus]
---

# `ast index --reindex` at the root deletes the graph node by node

## Objective

The Engineer reported: `graphit ast index . --reindex` finishes normally on `graphit-code`
(743 files, 28.0 s total), but on a large corpus **it stalls at the first stage** — it prints
`› Removing previous index for <root>...` and never prints `✓ Repository data removed`. At the
time of the report there was a process stuck in that state.

Two questions to answer, in this order:

1. **Is it stuck or is it slow?** The two require opposite fixes — a deadlock is resolved with
   locking/ordering, an algorithmic cost is resolved by fixing the algorithm. Confusing the two
   is the expensive mistake here.
2. **Why only on the large corpus?** If the answer is "because it's bigger", the next question
   is *how much* the cost grows with size: linear is acceptable, multiplicative is not.

### Starting reasoning

The project's memory already had two plausible candidates, and both were ruled out as the
primary cause:

- The documented per-pipeline CPU/RAM budget limitation — N supervisors in the daemon
  multiply CPU/RAM. Real, and present on this machine right now (daemon averaging 1027% CPU),
  but this **aggravates**, it doesn't explain: it would explain proportional slowness on any
  project, including the small one, which finishes in 28 s.
- LadybugDB's write lock. Ruled out by code order: `p.Step("Removing previous
  index")` (`cmd/graphit/commands/runners.go:315`) is only reached **after**
  `newASTBackend()` (`:296`), so the database had already opened. Whatever was blocking
  wouldn't be the opening.

## Plan & Task Breakdown

- [x] **T1 — Determine stuck vs. slow from process evidence** — Spec: measure state,
  accumulated CPU and I/O counters of the PID under `/proc`; "stuck" requires CPU and `wchar`
  to be **stopped** between two samples. Do not conclude from `State: S`, which is normal for a
  thread pool.
- [x] **T2 — Locate the exact phase from CLI output** — Spec: map the last printed line to the
  section of `runASTIndex`; the output is the state machine, so the absence of
  `Repository data removed` delimits the phase without needing a stack trace.
- [x] **T3 — Read `GraphWriter.DeleteRepository`'s algorithm and derive its complexity** —
  Spec: `internal/ast/writer.go`; identify how the scope-by-prefix behaves when the prefix is
  the root itself. Invariant to respect in any fix: a partial delete (subdirectory) must keep
  preserving a node whose owner survived — there's a test for this in
  `internal/ast/writer_delete_repository_test.go`.
- [x] **T4 — Confront the derived complexity with the two observed times** — Spec: the model
  only holds if it explains 0 s on `graphit-code` **and** >9 min on the large corpus with the
  same formula.
- [x] **T4b — Check whether the delete's result is even used** — Spec: follow what
  `ForceRebuild` does in the pipeline through to graph publication. Result: **it isn't used**.
  See below; this reclassifies T5.
- [ ] **T5 — Remove the delete from `--reindex`'s path** — Spec: `runASTIndex`
  (`cmd/graphit/commands/runners.go:314-321`) and/or `DeleteRepository`. Discovery from T4b:
  with `ForceRebuild: true` the pipeline always takes the full rebuild, which builds a new
  database at `<db>.<hex>` and renames it over production — so the graph that
  `DeleteRepository` empties is **discarded entirely** right afterward. This stops being an
  optimization and becomes removal of dead work. "Done" = `--reindex` no longer walks the
  graph, and the final state is identical. Constraint that still holds: **don't discard
  `shards/`** (parse + embedding cache) — that's what distinguishes this from calling
  `--reset`.
- [x] **T4c — Close T4b's debt: why removing `DeleteRepository` from stub form fixed something**
  — Spec: date the two changes and find the real mechanism. Result below — and it revealed a
  **correctness bug**, not just a cost issue.
- [x] **T5a — Prune files that vanished from disk from the shard cache on the `ForceRebuild`
  path too** — Spec: `internal/ast/pipeline.go:294-300`, today inside the
  `!opts.ForceRebuild` branch. It's the fix for the fix: the graph AND the search index are
  both built from the cache, so pruning the cache is what makes a deleted file disappear from
  both. Constraint: do **not** apply this on the `scoped` branch — there the tree hasn't been
  walked, so `files` isn't the whole corpus and pruning against it would delete everything
  else.
- [x] **T5b — Remove the `DeleteRepository` call from both reindex call sites** — Spec:
  `cmd/graphit/commands/runners.go:314-321` and `internal/mcpstdio/tools_ast.go:143-150`.
  Acceptance: `--reindex` doesn't walk the graph, and the published graph is identical.
- [x] **T5c — Remove `DeleteRepository`, `pathsUnder`, `labelHasPath` and their test** — Spec:
  `internal/ast/writer.go` and `internal/ast/writer_delete_repository_test.go`. `pathsUnder`
  and `labelHasPath` exist only for `DeleteRepository`; `ActiveNodeLabels` stays, it's used by
  `bundle.go`. Justification in Trade-offs.
- [x] **T5d — Test that `--reindex` drops a file deleted from disk** — Spec: pipeline-level
  test with `ForceRebuild: true`; check its absence from the graph **and** from the search
  index. This test fails on the current tree — it's the `a6dd378` regression.
- [x] **T6 — Give the delete phase progress reporting** — **CANCELLED by T5b**: with no delete
  phase, there's no silence left to cover. The long silence of the *write* phase is already
  covered by `pipeline.go:555-557`. Kept in the log instead of deleted because the reason for
  cancelling matters.
- [x] **T7 — Measure before/after on the large corpus** — Spec: blocked while the current
  process holds the database.

### T4c — the date closes the debt, and opens a bigger bug

```
2026-07-28  460da0a  DeleteRepository stops being a stub
2026-08-05  a6dd378  fix(ast): rebuild was publishing a one-file graph over a complete one
                     → both write paths now build <db>.<hex> and rename it
```

`DeleteRepository` was written when writes still reached production. Eight days later,
`a6dd378` turned every write into a copy+rename — and with that, made the delete **inert**,
without anything failing. Nobody removed it because nothing broke: it became invisible time.

**But the symptom it used to fix came back, and it's live now.** The real mechanism:
`pipeline.go:294-300` detects a file that left disk and calls `jsonCache.Remove(cached)` — and
it's **inside the `else if jsonCache != nil && !opts.ForceRebuild` branch** (`:221`). With
`--reindex`, `ForceRebuild` is `true`, the flow falls into the `else` at `:306`
(`changedFiles = files`) and the pruning **never runs**. The shard cache keeps the deleted
file's shard, and `RebuildFromJSONWithSearch` replicates the entire cache.

So, today: **`ast index . --reindex` does NOT remove the entity of a file deleted from disk** —
not from the graph, not from the search index. The delete was what masked this until
2026-08-05. The most expensive mode of the command fails at exactly the one thing that
distinguishes it from a normal index.

### The two problems in the report, and where each one is resolved

1. **Wasted work** → T5b/T5c. The delete is discarded by the swap.
2. **Search index (sqlite)** → two halves:
   - *divergence window*: the delete empties the graph and doesn't touch
     `ladybugdb.search.sqlite`, so for 23 minutes the graph says nothing exists and search says
     everything exists — and the state becomes **permanent** if the process dies halfway
     through. The design in `json_rebuild.go:49-55` deliberately accepts a window where
     *search* lags behind an *intact graph*; the delete inverted that. Resolved by T5b: without
     the delete, the graph keeps serving until the swap.
   - *deleted file survives in search*: the index is rebuilt from the same unpruned cache.
     Resolved by T5a.

## Implementation Details

Nothing implemented yet. T1–T4 are investigation; the fix is T5/T6.

### What was measured (T1) — it isn't stuck

`PID 436704`, `graphit-core ast index . --reindex`, two samples 20 s apart:

| measurement | t0 (15:21:36) | t1 (15:21:56) |
|---|---|---|
| elapsed / accumulated CPU | 08:52 / 10:33 | — |
| average %CPU | 118% | — |
| RSS | 2.99 GB | — |
| `wchar` | 2,485,668,205 | 2,537,225,698 |
| `ladybugdb.wal` | 4,768,102 B | 5,499,274 B |

`wchar` grew ~51 MB in 20 s (~2.5 MB/s) and the WAL grows and gets checkpointed — the process
is **progressing**. `State: S` with 33 threads in `futex_do_wait` is LadybugDB's pool between
units of work, not a deadlock. 2.48 GB written so far, all in the delete phase.

`ladybugdb.search.sqlite` had an mtime of 10:40, untouched: the delete phase doesn't touch the
search index, which is rebuilt afterward by the write phase.

### The phase (T2)

`runASTIndex` (`cmd/graphit/commands/runners.go:281`):

```
296  db, err := newASTBackend()                    // opened — so not an opening lock
314  if reindex && !reset {
315      p.Step("Removing previous index for %s...", absPath)
316      writer := ast.NewGraphWriter(db, absPath, true)
317      if err := writer.DeleteRepository(ctx, absPath); err != nil { ... }
320      p.StepOK("Repository data removed")
321  }
```

The output stopped between `:315` and `:320`. It's inside `DeleteRepository`.

### The algorithm (T3)

`GraphWriter.DeleteRepository` (`internal/ast/writer.go`), with `prefix := w.rel(repoPath)`:

1. `pathsUnder(LabelFile, prefix)` runs `MATCH (n:File) RETURN n.path` and filters in Go. When
   `prefix == "."` — which is exactly `ast index .` at the root — **the filter accepts
   everything**: the result is the entire list of corpus files.
2. For each label with a `path` column:
   `MATCH (n:`Label`) WHERE n.path IN $paths DETACH DELETE n`, with the whole list in `$paths`.
   With no index on `path`, it's a full table scan, with a membership test against the list per
   row, and edge removal per node.
3. For labels **without** `path` (`Parameter`/`Field` when the grammar doesn't declare them):
   `MATCH (n:`Label`) OPTIONAL MATCH ()-[r]->(n) WITH n, count(r) AS owners WHERE owners = 0 DELETE n`
   — another full scan per label, aggregating incoming edges.

The point: for the whole-root case, `$paths` contains **all** files and the predicate matches
**all** rows. In other words, "emptying the graph" is expressed as ~20 table scans with lists
of tens of thousands of elements, when the same final state is reached by `--reset` with an
`os.RemoveAll` (`runners.go:289-294`).

The scope-by-prefix is correct and is the reason for the design — see
the earlier indexing-location and reindex-deletion investigation, where
`DeleteRepository` stopped being a stub. The defect isn't the scope: it's that there's no short
path for the case where the scope is the entire graph.

### Confronting the model with the observed times (T4)

| | files | nodes | delete time |
|---|---|---|---|
| graphit-code | 743 | ~63 k | ~0 s (of the 28.0 s: discover 0.01 + hash 0.00 + parse 1.93 + write 26.08 = 28.02) |
| large corpus | ~39 k | ~2.5 M | >9 min and counting |

A linear model in nodes predicts 40× — doesn't explain it. A `nodes × files` model predicts
40 × 52 ≈ **2000×**, and 2000 × ~0.3 s ≈ 10 min, which matches the observed order of magnitude.
The fit supports that `IN $paths` is evaluated by scanning the list, not via a hash set —
**inferred from the fit, not measured directly**; measuring this directly is part of T7.

### The delete is discarded (T4b) — the finding that reclassifies the fix

`runASTIndex:352` passes `ForceRebuild: reindex` to the pipeline. This has two effects, and the
second is what matters:

1. `pipeline.go:221` and `:306-309` — with `ForceRebuild`, the mtime/hash comparison is
   skipped and **every** file is treated as changed. It also disables the `:335` shortcut
   ("nothing changed").
2. `pipeline.go:589` — `useIncremental` requires `!opts.ForceRebuild`. So `--reindex` **always**
   falls into the `else` at `:612`: `RebuildFromJSONWithSearch`.

And the comment at `pipeline.go:559-565` is explicit about what both write paths do: *"Both
write paths build into a `<db>.<hex>` copy and rename it over production, so NEITHER opens
production read-write"*. Confirmed in `internal/ast/json_rebuild.go`:
`tempDBPath := lb.cfg.DBPath + "." + shortHex()` (`:128`), the whole construction happens
there, and `lb.AtomicSwapDB(tempDBPath)` (`:407`).

**Consequence:** the production database that `DeleteRepository` spent 13+ minutes emptying
node by node gets replaced by a rename right afterward. The delete's result doesn't survive
the very execution that requested it. The only path through which production state would
propagate is the incremental one (`CopyDBDir(prodPath, workingPath)`,
`incremental_rebuild.go:155`) — and that's exactly the path `ForceRebuild` disqualifies.

This also applies to subdirectory reindexing: `scoped` uses `jsonCache.Count()` as the corpus
(`pipeline.go:315-317`) and the rebuild replicates the **entire** shard cache, so the graph
gets fully republished no matter what scope was requested.

### Where the embedding cache lives — verified, not inferred

`ShardEmbCache.shardPath` = `filepath.Join(ec.dir, "shards", relPath+".emb.json")`
(`internal/ast/shard_emb_cache.go:147`), with `ec.dir` = `opts.CacheDir` =
`filepath.Dir(ladybugCfg.DBPath)` (`runners.go:350`) — the **same** directory that `--reset`
removes with `os.RemoveAll` (`runners.go:289-294`). And `Get(relPath, uid, currentHash)`
(`:91`) compares content hashes, so a file's vector is reused when its content hasn't changed,
even on a `--reindex`.

That's what makes `--reindex` and `--reset` not interchangeable: `--reindex` forces a reparse
**while preserving** the vectors; `--reset` destroys both caches.

## Use Cases

### UC-01: Forced reindex of a project's root
- **Actor**: Engineer, via `graphit ast index . --reindex`; and agent, via
  `graphit_ast_index(reindex: true)` → `registerASTTools` (`internal/mcpstdio/tools_ast.go:100`).
- **Preconditions**: the project's graph exists; `reset` was not requested.
- **Main Flow**:
  1. `runASTIndex` opens the backend.
  2. `DeleteRepository(ctx, absPath)` removes every node under the root — which is every node.
  3. The pipeline reparses and rewrites the graph and the search index.
- **Alternative Flows**:
  - `path` is a subdirectory: the scope-by-prefix does a partial delete, with proportional
    cost.
  - `--reset`: `os.RemoveAll` of the store, without walking the graph — **and without
    preserving `shards/`**.
- **Error Scenarios**:
  - Error during delete: `p.StepWarn("Reindex cleanup: %v")` and the index proceeds anyway —
    reindex stacks new data on top of old, which is the original defect the stub used to
    cause.
  - `SIGINT`: context `cancel()`; the delete stops midway, leaving a partial graph.
- **Postconditions**: a graph equivalent to a clean index of the current tree.
- **Affected Files**: `cmd/graphit/commands/runners.go`, `internal/ast/writer.go`,
  `internal/mcpstdio/tools_ast.go`.

### UC-02: Watching the progress of a long reindex
- **Actor**: Engineer at a terminal.
- **Preconditions**: `ast index` is running.
- **Main Flow**: `indexProgressReporter` (`runners.go:251`) emits `Parsing:` and
  `Writing graph:` lines, throttled to 200 ms on a TTY and 10 s off it.
- **Error Scenarios**: **[GAP — subject of T6]** the delete phase doesn't go through this
  reporter. On a large corpus it's the longest phase, and the only one that gives no signal,
  which reproduces exactly the failure that `indexProgressReporter`'s comment says already cost
  an investigation ("16 minutes of silence … indistinguishable from a hang, which is exactly
  how a real hang went unnoticed").
- **Postconditions**: no phase is indistinguishable from a hang.
- **Affected Files**: `cmd/graphit/commands/runners.go`, `internal/ast/writer.go`.

## Test Cases & Acceptance Criteria

### Feature: root reindex doesn't walk the graph
Ref: UC-01

#### Scenario: the whole root uses the short path
```gherkin
Given an indexed graph with at least one node of each label
When DeleteRepository is called with repoPath equal to the indexed root
Then the graph ends up empty
  And no query with a path list is executed
```

#### Scenario: a subdirectory keeps the scope-by-prefix
```gherkin
Given a graph with files under "a/" and under "b/"
When DeleteRepository is called with repoPath equal to "<root>/a"
Then the nodes under "a/" disappear
  And the nodes under "b/" remain
  And a Parameter whose owner is under "b/" remains
```

#### Scenario: a path outside the root deletes nothing
```gherkin
Given an indexed graph
When DeleteRepository is called with a path outside the root
Then no node is removed
```

### Feature: the delete phase reports progress
Ref: UC-02

#### Scenario: a long delete doesn't go silent
```gherkin
Given a graph large enough for the delete to take more than 10 seconds
When ast index --reindex runs in a terminal
Then a progress line for the removal phase is emitted at least every 10 seconds
  And the next phase only prints after removal confirms it's done
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/pipeline.go` | Modified | pruning of files that left disk now also runs on the `ForceRebuild` branch; `pruneVanished` and `relSet` extracted |
| `cmd/graphit/commands/runners.go` | Modified | removed the delete phase (9 lines) |
| `internal/mcpstdio/tools_ast.go` | Modified | removed the same call; `reset`'s description in the MCP schema now says what it costs |
| `cmd/graphit/commands/ast.go` | Modified | `--reindex`/`--reset` help used to describe what the flag no longer does |
| `internal/ast/writer.go` | Modified | removed `DeleteRepository`, `pathsUnder`, `labelHasPath` (-121 lines) and two imports |
| `internal/ast/writer_delete_repository_test.go` | **Removed** | covered exclusively the removed function (-183 lines) |
| `internal/ast/reindex_drops_deleted_files_test.go` | **Created** | regression test for `a6dd378`: proves `--reindex` drops a file deleted from disk from both the graph AND the search index |

Net: 46 insertions, 334 deletions.

## Verification

| what | result |
|---|---|
| `TestForceRebuildDropsAFileThatLeftTheDisk` **without** the fix | **FAILS**, on all three assertions: node in the graph, line in `entities`, line in `files` |
| the same test **with** the fix | PASS (1.11 s) |
| `internal/ast` full suite (352 tests) | PASS |
| `internal/mcpstdio`, `cmd/graphit/commands` | PASS |
| `go build -tags fts5 ./...` | exit 0 |
| `go vet -tags fts5 -unreachable=false` on the 3 packages | exit 0 |
| `golangci-lint run` on the 3 packages | 0 issues |
| end-to-end: `ast index . --reindex` on this repo | 743 files, **no removal phase in the output**, intact graph (630 go / 46 yaml / 35 tsx / 18 ts / …) |

The negative proof is the one that matters, and it was done on purpose: I disabled the pruning
with `if false &&`, recompiled, and the test failed exactly where predicted, in all three
places. Without that, the test would just be an assertion that the current code does what it
does.

What was **not** measured: the gain on the large corpus (T7). The reported process ended at
15:36 after ~23 min spent solely in the delete phase, and I didn't reindex that corpus again —
the daemon has the embedding module running on it at 475% CPU. The expected number is "the
~23 minutes disappear", because the phase no longer exists; the rest of the pipeline wasn't
touched.

## Trade-offs & Decisions

**Why NOT map a root `--reindex` to `--reset`**, which would be the one-line fix: `--reset`
does `os.RemoveAll(filepath.Dir(DBPath))`, and that directory contains `shards/` — 3 GB on the
large corpus, with the embedding cache inside (see
the recorded shard-manifest version mismatch behavior). Recomputing embeddings for
~2.5 M entities is orders of magnitude more expensive than the delete it's trying to avoid. The
final state of the graph is the same; the total cost is not. The fix needs to empty the
**graph** without touching the store.

**I removed `DeleteRepository` instead of leaving it without a caller.** This isn't cosmetic
cleanup: no write path has opened production read-write since `a6dd378` (`pipeline.go:559-565`
itself states this), so a function whose only mode of operation is mutating the published
graph in place is **architecturally obsolete**, not merely unused. Leaving it there is exactly
what sustained 23 minutes of invisible work for fifteen days: it looked like it was pulling
weight. Recoverable via `git show 460da0a:internal/ast/writer.go` should a path that writes in
place ever exist again. `pathsUnder` and `labelHasPath` went with it, since they existed only
for it; `ActiveNodeLabels` stayed, `bundle.go` uses it.

**The pruning stayed in the pipeline, not at the call site.** Both call sites (CLI and MCP)
only declare `ForceRebuild: true`; how to honor that is the pipeline's decision, since it's the
only place that knows whether it's going to republish the whole graph. That was the earlier
asymmetry — the call site did its own graph cleanup, blind to what the pipeline would do next.

**Scope-by-prefix wasn't replaced with anything: it was already unused.** The pipeline's
`scoped` path (`opts.ChangedPaths`/`DeletedPaths`, the daemon's path) handles deletion by file
name, without scanning the graph. Subdirectory `--reindex` keeps working — it reparses the
files in that directory and republishes the entire shard-cache graph, which is what it already
did.

## Technical Debt

- [x] `pathsUnder` materialized every `File.path` into a Go slice before filtering — resolved
  by removing it along with `DeleteRepository`.
- [ ] **The pruning depends on `files` covering the whole corpus, which is an implicit
  precondition.** `pruneVanished` deletes from the cache anything not in `live`; if someone
  ever calls the non-`scoped` branch with a partial list, it prunes the rest of the project.
  This is documented in a NOTE in the function, and the `scoped` branch is excluded by
  construction, but there's no test enforcing the rule. A test that passes a partial list and
  requires rejection (or a type that only discovery can produce) would close this.
- [ ] **The divergence window between the graph and the search index still exists, by design.**
  `json_rebuild.go:49-55` assumes it: the index is rebuilt AFTER the swap, so for one rebuild,
  search answers from the old corpus while the graph is already new. This task removed the
  inversion (empty graph + full search), not the window. If it ever matters, the fix would be
  publishing both in the same rename, which is impossible today — they're separate files.
- ~~`--reindex` on a large corpus is still expensive because of the write phase, and the
  cheaper path (deleting `ladybugdb` while keeping `shards/`) isn't exposed as a flag.~~
  **REJECTED by the Engineer on 2026-08-20**: there will be no flag for this. Recorded here
  instead of deleted because `pipeline.go:319-327` documents the path with measured numbers
  (~95 s vs. 16 min on a 36k-file repo), and a future session reading that comment will
  propose exposing it again. The decision has already been made: don't propose it.
- [ ] The per-pipeline budget is still open and worsens any measurement done on this machine:
  the daemon averaged 1027% CPU during the investigation and 475% during verification. Already
  recorded in memory; noted here because **it's why T7 wasn't measured**.

## System Knowledge

- The **CLI output is the `ast index` state machine**: whichever line was printed last delimits
  the phase without needing a stack trace. This applies to any hang report here.
- **Stuck vs. slow is decided by `wchar` across two samples**, not by the process's `State`: a
  LadybugDB thread pool sits in `futex_do_wait` most of the sampled instants while working
  normally.
- The delete phase **doesn't touch** `ladybugdb.search.sqlite`; the search index is rebuilt
  entirely afterward, by the write phase. So, on a root reindex, the cost of walking the graph
  to erase it is spent on a structure that was going to be discarded anyway.
- The daemon **does have** a serialized indexing slot (`daemon: waited for the indexing slot`,
  seen in logs with a 20m29s wait), but it coordinates the daemon with itself — not with a
  `graphit ast index` running in a terminal.

## Progress Log

### 2026-08-20

- Engineer's report, with the process still alive. Memory consulted first: two candidates
  (per-pipeline budget, LadybugDB lock) raised and both ruled out as the primary cause, with
  the reasoning recorded in Objective.
- T1 complete: two samples of `/proc/436704/io` 20 s apart show ~51 MB written and the WAL
  growing — real progress, not a deadlock. Recorded that `State: S` isn't evidence of a hang.
- T2 complete: the output stops between `runners.go:315` and `:320`, right inside
  `DeleteRepository`. The backend had already opened at `:296`, which rules out an opening
  lock.
- T3 complete: derived that `prefix == "."` makes `$paths` contain every file, and the
  predicate matches every row, across ~20 labels.
- T4 complete: the `nodes × files` model (~2000×) explains both times; the linear-in-nodes
  model (40×) doesn't. The `IN` evaluation via list scan is an **inference from the fit**, not
  yet measured.
- Second defect identified, independent of the first: the delete phase doesn't go through
  `indexProgressReporter` — the same class of failure that this reporter's comment documents
  as having already hidden a real hang, one phase earlier.
- Ruled out the one-line fix (root `reindex` → `reset`) because of the embedding cache in
  `shards/`. Recorded in Trade-offs.
- **T4b, while answering the Engineer's question about the difference between `--reindex` and
  `--reset`** — followed `ForceRebuild` through to graph publication and discovered that
  **the delete is discarded**: the full rebuild builds `<db>.<hex>` and renames it over
  production. T5 stopped being "make the delete fast" and became "remove the delete", which is
  simpler and safer. Verified in `pipeline.go:221,306-309,335,559-565,589` and
  `json_rebuild.go:128,407`.
- Verified (previously only inferred) that the embedding cache lives at
  `<store>/shards/*.emb.json` and is keyed by content hash — which confirms the Trade-off
  against using `--reset` as the fix, and explains why the two flags aren't interchangeable.
- **Next step**: T5 is now small — don't call `DeleteRepository` when the pipeline is going to
  republish the whole graph anyway. T6 (progress) still stands as it was, since the write
  phase also has a long silence. T7 isn't measurable while the current process holds the
  database, and compiling now competes with the daemon at 1027% CPU.

- **T5a–T5d implemented and verified.** The order was deliberate: pruning (T5a) BEFORE removing
  the delete (T5b), because while the delete still existed it masked part of the symptom;
  reversed, the tree would pass through a state where `--reindex` was worse than before.
- **Negative proof performed:** I disabled the pruning with `if false &&`, recompiled, and the
  test failed on all three assertions (graph, `entities`, `files`). Then I restored it and
  everything went green. Without that step the test wouldn't prove anything.
- **A stumble along the way that turned into a memory fix:** `go build ./...` without
  `-tags fts5` failed with an undefined symbol, and TWO memories claimed that tag no longer
  existed since `fb19403`. They described the window from August 16 to 19; `61f2dee`
  (2026-08-19) brought SQLite back along with `BUILD_TAGS := fts5` and the guard files. Both
  were corrected instead of being left for the next session to trip over.
- **Flag help corrected** (`ast.go`, and `reset`'s description in the MCP schema): `--reindex`
  was described as "Remove only this repository's data before re-indexing", which described
  precisely the part that had just been removed. Now both texts state the difference that
  decides between them — that `--reset` discards the embeddings and `--reindex` doesn't.
- **Operational note for the next session on this machine:** the liblbug path has a `!` in its
  name (`.../github.com/!ladybug!d!b/...`). Interpolated inside DOUBLE quotes in an interactive
  shell, the `!` triggers history expansion and the command returns the output of a PREVIOUS
  command and then hangs — it looks like the test is stuck, and it isn't. Use
  `LBUG=$(echo ~/go/pkg/mod/github.com/*ladybug*/go-ladybug@v0.17.0/lib)` and quote the
  variable. This cost several attempts before I spotted the pattern.

#### The review debt T4b opens — CLOSED

The second hypothesis was right, and the answer is the dating done in T4c: `DeleteRepository`
was written in `460da0a` (2026-07-28), when writes still reached production — so back then the
delete **genuinely mattered** and fixed the symptom. `a6dd378` (2026-08-05) turned every write
into a copy+rename, and with that made the delete inert, without anything failing. No current
path writes in place, so T5b/T5c could remove it unconditionally — and T5d covers the original
symptom through the mechanism that actually governs it today: shard-cache pruning.
