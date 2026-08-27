---
title: The `ast index --reindex` command at the root deletes node-to-node graph — minutes of silence that feels like a stall
status: done
created: 2026-08-20
updated: 2026-08-20
tags: [ast, reindex, performance, delete, progress, corpus-grande]
---

On root node, removes the graph from node to node

## Objective

The Engineer reported: The inline 1 ends normally at inline 2 (743 files, 28.0 seconds in total), but for the first stage — prints inline 3 and never prints inline 4. At the time of reporting, there was a process in this state.

Duas perguntas a responder, nesta ordem:

1. Is it stuck or slow? Both require opposite corrections — a deadlock is resolved with lock/sequence, an algorithmic cost is resolved with the algorithm. Confusing them is costly here.
2. Why only in the large corpus? If the answer is "because it's larger", the next question is *how much* the cost increases with size: linear is acceptable, multiplicative not.

Input reasoning

The project's memory had two plausible candidates already, but both were discarded as primary causes:

- `[[graphit-cpu-ram-budget-is-per-pipeline-not-global]]` — Supervisors in the daemon multiply CPU/RAM. Real and present on this machine now (daemon with an average of 1027% CPU), but it **aggravates**, not explaining: would explain proportional slowdown in any project, including small ones that end at 28 seconds.
- The Write Lock for LadybugDB is discarded by the code order: `p.Step("Removing previous index")` (`cmd/graphit/commands/runners.go:315`) can only be reached **after** `newASTBackend()` (`:296`), so the database was opened. What was blocking would not have been opening.

Note: The inline codes and commands are placeholders for actual code snippets or commands, which should be replaced with their specific content in the translation process.

## Plan & Task Breakdown

- [x] **T1 – Determine Trapped vs. Slow by Process Evidence** – Spec: measure state, CPU accumulated and I/O counters of PID in ____INLINE_9__; "trapped" requires CPU and ____INLINE_10__ to be stopped between two samples. Do not conclude if ____INLINE_11__, which is normal in thread pool.
- [x] **T2 – Locate Exact Phase by CLI Output** – Spec: map the last printed line to the section of ____INLINE_12__; the output is the machine state, so the absence of ____INLINE_13__ defines the phase without needing a stack.
- [x] **T3 – Read Algorithm of _`GraphWriter.DeleteRepository` and Derive Its Complexity** – Spec: ____INLINE_15__; identify how the scope by prefix behaves when the prefix is itself the root. The invariant to respect in any correction: partial delete (subdirectory) must continue preserving a node whose owner survives — there’s a test for this in ____INLINE_16__.
- [x] **T4 – Confront Complexity Derived with Two Observed Times** – Spec: the model only applies if it explains 0 seconds in _`graphit-code` and >9 minutes in the large corpus with the same formula.
- [x] **T4b – Verify if the Result of Delete is Even Used** – Spec: follow what `ForceRebuild` does in the pipeline until publishing the graph. Result: **not used**. See below; this reclassifies T5.
- [ ] **T5 – Remove delete from path of _`--reindex`** – Spec: ____INLINE_20__ (____INLINE_21__) and/or _`DeleteRepository`. Discovery of T4b: with ___INLINE_23__, the pipeline always takes a full rebuild, which constructs a new database in _`<db>.<hex>` and renames it to production — then the graph that empties _`DeleteRepository` is **totally discarded** immediately after. This stops being optimization and becomes removal of dead work. "Ready" = `--reindex` does not traverse the graph, and the final state is identical. Constraint still holds: **do not discard _`shards/` (parse + embedding cache)** — that separates this from calling `--reset`.
- [x] **T4c – Close Debit of T4b: why removing ____INLINE_29__ from stub resolved something** – Spec: date the two changes and find the real mechanism. Result below — and he revealed a **bug in correction**, not just cost.
- [x] **T5a – Remove files that disappeared from disk also from path of _`ForceRebuild`** – Spec: ___INLINE_31__, today inside the branch `!opts.ForceRebuild`. It’s the correction for correction: E and index search are built based on cache, so podar the cache is what makes file deleted from disk disappear in both. Constraint: **do not apply to branch _`scoped`** — there the tree hasn’t been traversed, so `files` isn’t the corpus and podar against it would delete everything else.
- [x] **T5b – Remove call of _`DeleteRepository` from two reindex call sites** – Spec: `cmd/graphit/commands/runners.go:314-321` and `internal/mcpstdio/tools_ast.go:143-150`.
  Acceptance: `--reindex` does not traverse the graph, and the published graph is identical.
- [x] **T5c – Remove _`DeleteRepository`, _`pathsUnder`, _`labelHasPath` and their test** – Spec: _`internal/ast/writer.go` and _`internal/ast/writer_delete_repository_test.go`. `pathsUnder` and
  `labelHasPath` only exist for __INLINE_46__; `ActiveNodeLabels` stays, is used by
  ___INLINE_48__. Justification in Trade-offs.
- [x] **T5d – Test that _`--reindex` removes file deleted from disk** – Spec: pipeline level with _`ForceRebuild: true`; check absence in graph and index search. This test fails on the current tree — it’s the regression of `a6dd378`.

Note: The code blocks, markdown, file paths, and technical terms have been preserved as requested.
- [ ] T6 – Progression to the Delete Phase – CANCELLED by T5b: Without a delete phase, there is silence covering. The long write phase’s silence is already covered by ___INLINE_52__. It is kept in the log instead of being deleted because the reason for cancellation pertains.
- [ ] T7 – Measure Before/After in Large Corpus – Spec: Blocked while the current process holds the database.

T4c resolves the debit, and opens a bigger issue.

```
2026-07-28  460da0a  DeleteRepository deixa de ser stub
2026-08-05  a6dd378  fix(ast): rebuild publicava um grafo de um arquivo por cima de um completo
                     → os dois caminhos de escrita passam a construir <db>.<hex> e renomear
```

The text was written when writing still produced results. Eight days later, `a6dd378` turned all writing into a copy+rename process— and with that, made the delete action **inactive**, without anything failing. No one deleted because nothing broke; it became invisible time.

But the symptom he resolved returned, and it is now alive. The real mechanism:
`pipeline.go:294-300` detects a file that has exited the disk and calls `jsonCache.Remove(cached)` —
and it is inside the branch `else if jsonCache != nil && !opts.ForceRebuild` (`:221`). With
`--reindex`, `ForceRebuild` is `true`, the flow falls into the `else` of `:306` (`changedFiles = files`)
and the pruning never runs. The shard cache maintains the deleted shard file, and
`RebuildFromJSONWithSearch` replicates the entire cache.

Note: Inline codes are placeholders for actual code blocks or technical details that would be provided in a full translation.

Therefore, today: **`ast index . --reindex` Does not remove the deleted file entity from the disk** — nor from the graph, nor from the search index. The delete was what masked this until 2026-08-05. The most expensive mode of the command fails exactly in the one thing that distinguishes it from a normal index.

The two problems of narration, where each one is resolved

1. **Work in vain** → T5b/T5c. The delete is discarded by the swap.
2. **Search Index (sqlite)** → two halves:
   - *Divergence window*: the delete empties the graph and does not touch `ladybugdb.search.sqlite`, so for 23 minutes the graph says nothing exists and the search says everything exists — and the state becomes **permanent** if the process dies halfway. The drawing in `json_rebuild.go:49-55` deliberately accepts a window where the *search* is behind the *intact graph*. The delete inverted this. It resolves to T5b: without delete, the graph continues serving until the swap.
   - *Deleted index survives in search*: the index is reconstructed from the same unbooted cache. It resolves to T5a.

## Implementation Details

Nothing implemented yet. T1-T4 are investigations; correction is T5/T6.

What was measured (T1) — not fixed

69. INLINE, 70. INLINE, two samples at 20 seconds distance:

Elapsed / CPU Accumulated: 08:52 / 10:33  
%CPU Average: 118%  
RSS: 2,99 GB  
___INLINE_71__: 2.485.668.205 bytes  
___INLINE_72__: 4.768.102 bytes

The inline 73 grew by ~51 MB in 20 seconds (~2.5 MB/s) and the WAL grows and is checkpointed — the process progresses. `State: S` with 33 threads on `futex_do_wait` is the LadybugDB worker pool, not a deadlock. Up to 2.48 GB written so far, all in the delete phase.

The file _INLINE_76_ has an access time of 10:40 and is untouched: the delete phase does not touch the search index, which is rebuilt after the write phase.

### A fase (T2)

`runASTIndex` (`cmd/graphit/commands/runners.go:281`):

```
Brazilian Portuguese to idiomatic English:

296 db, err := newASTBackend() // opened – immediately not a lock for opening
314  if reindex && !reset {
315      p.Step("Removing previous index for %s...", absPath)
316      writer := ast.NewGraphWriter(db, absPath, true)
317      if err := writer.DeleteRepository(ctx, absPath); err != nil { ... }
320      p.StepOK("Repository data removed")
321  }
```

The output stopped between `:315` and `:320`. It is within `DeleteRepository`.

This translation maintains the original structure, code blocks, markdown format, file paths, and technical terms as specified.

### O algoritmo (T3)

And inline 82 (inline 83), with inline 84:

1. The __INLINE_85__ loop processes the __INLINE_86__ and filters in Go.
When
`prefix == "."` (which is exactly `ast index .` at the root) **the filter accepts everything**: the result is an entire list of corpus files.
2. For each label with column __INLINE_89__, Label __INLINE_90__, with the entire list in __INLINE_91__.
Without index in __INLINE_92__, it's a full scan of the table, with a relevance test on the list per row, and removal of edges by node.
3. For labels without `path` (`Parameter`/`Field` when grammar doesn't declare them):
Label __INLINE_97__ — another full scan for label, aggregating input edge lists.

The point:
In the case of an integer root, **INLINE_99** contains **all** files and the predicate "houses" all lines. That is, "emptying the graph" is expressed as ~20 table scans with lists of hundreds of thousands of elements when the same final state is reached by **INLINE_100** with a **INLINE_101** (INLINE_102).

The scope by prefix is correct and is the reason for the design — see `[[indexing-saves-to-the-right-project-and-reindex-returns-to-deleting]]`, where `DeleteRepository` has been removed. The defect is not the scope; it’s that there is no short path to the case where the scope is the entire graph.

This translation aims to maintain the technical meaning while using idiomatic English phrasing for better readability and naturalness in a programming context.

The confrontation with times (T4)

| | files | we | time of delete |
|---|---|---|---|
| graphit-code | 743 | ~63 k | ~0 s (of the 28.0 s: discover 0.01 + hash 0.00 + parse 1.93 + write 26.08 = 28.02) |
| large corpus | ~39 k | ~2,5 M | >9 min and counting |

Linear model in nodes predicts 40× - does not explain. Model `nodes × files` predicts 
40 × 52 ≈ **2000×**, and 2000 × ~0,3 s ≈ 10 min, which is the observed order. The adjustment supports that
the `IN $paths` is evaluated by scanning the list, not by hash set - **inferred from the adjustment, not measured directly**; measuring this is part of T7.

The delete is discarded (T4b) - the found correction that reclassifies the correction

It passes through ___INLINE_108__ to the pipeline. This has two effects, and the second is what matters:

1. Inline 109 and Inline 110 — with Inline 111, the comparison of mtime/hash is skipped, and **every** file is treated as modified. Also disables shortcut to Inline 112 ("nothing changed").
2. Inline 113 — Inline 114 requires Inline 115. Therefore Inline 116 always falls into Inline 118's `RebuildFromJSONWithSearch`: Inline 117.

And the comment of `pipeline.go:559-565` is explicit about what both writing paths do:
*"Both write paths build into a `<db>.<hex>` copy and rename it over production, so
NEITHER opens production read-write"*. Confirmed in `internal/ast/json_rebuild.go`:
`tempDBPath := lb.cfg.DBPath + "." + shortHex()` (`:128`), the entire construction inside,
`lb.AtomicSwapDB(tempDBPath)` (`:407`).

**Consequence:** The production database that `DeleteRepository` passed 13+ minutes emptying a node to being renamed immediately afterwards. The result of the delete does not survive its own execution that it requested. The only way in which the production state propagates is through the incremental (__INLINE_128__, __INLINE_129__) — and exactly the path that `ForceRebuild` disqualifies.

This also applies to subdirectory reindexing: `scoped` uses `jsonCache.Count()` as the corpus.
(`pipeline.go:315-317`) and rebuilds the replica shard cache **entirely**, so the graph is completely republished for any requested scope.

Where is the embedding cache located — verified, not inferred

The translation is:

```
INLINE_134 = INLINE_135
(INLINE_136), with INLINE_137 = INLINE_138 =
INLINE_139 (INLINE_140) — the same directory as INLINE_141
removes INLINE_142 (INLINE_143). And INLINE_144
(INLINE_145) compares content hashes, so a file vector whose content has not changed is reused even in INLINE_146.
```

That is what makes `--reindex` and `--reset` not interchangeable: `--reindex` forces recomputation; `--reset` destroys the two caches.

## Use Cases

### UC-01: Forced Reindexing of the Root Node of a Project

- **Actor**: Engineer via ___INLINE_151__; and agent via
  ___INLINE_152__ → ___INLINE_153__ (___INLINE_154__).  
- **Preconditions**: The project's graph exists; ___INLINE_155__ was not requested.
- **Main Flow**:
  1. ___INLINE_156__ opens the backend.
  2. ___INLINE_157__ removes all nodes under the root — which is every node.
  3. The pipeline reparsees and rewrites the graph and index of search.

- **Alternative Flows**:
  - ___INLINE_158__ is a subdirectory: the scope by prefix deletes partially, with cost proportional.
  - ___INLINE_159__: ___INLINE_160__ from the store, without traversing the graph — **and without preserving ___INLINE_161__**.

- **Error Scenarios**:
  - Error in delete: ___INLINE_162__ and the index follows — reindex piles new on top of old, which is the original defect that stub caused.
  - ___INLINE_163__: context error; the delete stops partway, leaving a partial graph.

- **Postconditions**: The equivalent graph to a clean indexing of the current tree.
- **Affected Files**: ___INLINE_165__, ___INLINE_166__,
  ___INLINE_167__.

### UC-02: Monitor Progress of a Long Reindexing

**Actor:** Terminal Engineer  
**Preconditions:** The script is running.  

**Main Flow:** After emitting lines from the reporter in **Inline 169** and **Inline 170**, it runs for 200 ms with TTY throttle, then waits for 10 seconds.

**Error Scenarios:** In a large corpus, this phase is the longest and the only one that does not signal. This reproduces exactly the failure mentioned in **Inline 173**'s comment ("Silence of 16 minutes … indistinguishable from a crash, which is exactly how a real crash would pass through").

**Postconditions:** No phase will be distinguishable from a crash.

**Affected Files:** `cmd/graphit/commands/runners.go`, `internal/ast/writer.go`.

## Test Cases & Acceptance Criteria

Feature: Reindexing of Root Not Traverses Graph

Scenario: The integer root uses the short path
```gherkin
Given an indexed graph with at least one node per label
When `DeleteRepository` is called with `repoPath` equal to the indexed root
Then o grafo fica vazio
None of the path queries is executed.
```

Scenario: Subdirectory retains scope by prefix
```gherkin
Given um grafo com arquivos em "a/" e em "b/"
When `DeleteRepository` is called with `repoPath` equal to `<root>/a`
Then we disappear as "a/".
And the "b/" marks remain
And a parameter whose owner is in "b/".
```

Scenario: The path outside the root does not delete anything
```gherkin
Given um grafo indexado
When `DeleteRepository` is called with a path outside of the root
Then nothing is removed.
```

Feature: The Delete Phase Emits Progress

Ref: UC-02

Scenario: The long stays silent
```gherkin
Given um grafo grande o suficiente para o delete passar de 10 segundos
When ast index --reindex roda em terminal
Then a progress line for the removal phase is emitted at least every 10 seconds
  And the next phase only prints after the removal confirms completion
```

## Files Changed

Portuguese:
| File | Change | Reason |
|---|---|---|
| `internal/ast/pipeline.go` | Modified | The file that was removed from the disk is now running on `ForceRebuild` as well; `pruneVanished` and `relSet` extracted |
| `cmd/graphit/commands/runners.go` | Modified | Delete phase (9 lines) removed |
| `internal/mcpstdio/tools_ast.go` | Modified | Same call removed; description of `reset` in schema MCP says what it costs |
| `cmd/graphit/commands/ast.go` | Modified | Help for `--reindex`/`--reset` describes what the flag doesn't do anymore |
| `internal/ast/writer.go` | Modified | Removed `DeleteRepository`, `pathsUnder`, and `labelHasPath` (-121 lines) and two imports |
| `internal/ast/writer_delete_repository_test.go` | **Removed** | Covered exclusively the function removed (-183 lines) |
| `internal/ast/reindex_drops_deleted_files_test.go` | **Created** | Regression of `a6dd378`: proves that `--reindex` deletes a deleted file from the graph E in the index search |

English:
The file that was removed from the disk is now running on `ForceRebuild` as well; `pruneVanished` and `relSet` extracted. The Delete phase (9 lines) has been removed.
A call with the same name has also been removed, and a description of `reset` in schema MCP says what it costs.
The help for `--reindex`/`--reset` describes what the flag doesn't do anymore.
The file that was deleted (-121 lines) and two imports have also been removed. The function that was removed has been covered exclusively by this change, as there are now no more -183 lines to cover it.

Balance: 46 insertions, 334 deletions.

Verification

| What |
|---|
| Without correction | FAIL, in three assertions: node on graph, line at `entities`, line at `files` |
| The same test with correction | PASS (1, 11 s) |
| Complete | PASS |
| __LINE__ , __LINE__ | PASS |
| exit 0 | exit 0 |
| In the 3 packages | exit 0 |
| In the 3 packages | 0 issues |
| End-to-end: `ast index . --reindex` in this repo | 743 files, **without removal phase in output**, complete graph (630 go / 46 yaml / 35 tsx / 18 ts / …) |

The negative test is the one that matters and was done on purpose: I disconnected the poda with `if false &&`,
recompiled, and the test failed exactly at the three places predicted. Without this, the test would just be a statement of what the current code does.

What was not measured: the gain in the large corpus (T7). The reporting process ended at 15:36 after approximately 23 minutes only in the delete phase, and I did not reindex the new corpus — the daemon is running the embedding module on it at 475% CPU. The expected number is "the ~23 minutes disappear", because the phase no longer exists; the rest of the pipeline was not touched.

## Trade-offs & Decisions

Why NOT map `--reindex` from root to `--reset`, which would be the correction of a line:
`--reset` makes `os.RemoveAll(filepath.Dir(DBPath))`, and this directory contains `shards/` —
3 GB in the large corpus, with embedding cache inside (see
`[[shard-manifest-with-different-version-is-silently-discarded]]`). Recomputing embeddings for ~2.5 million entities is orders of magnitude more costly than the delete that you are trying to avoid.
The final state of the graph remains the same; the total cost is not. The correction empties the **graph** without touching the store.

I removed `DeleteRepository` instead of leaving it without a caller. It is not aesthetic cleaning: no path of writing opens read-write capability from `a6dd378` (the very `pipeline.go:559-565` itself affirms this), so a function whose only way to operate is to mutate the published graph in place is **architecturally obsolete**, not just unused. Leaving it there is exactly what
sustained 23 minutes of invisible work for fifteen days: it seemed to carry weight. Recoverable if ever there were a path that writes in place.
Together they were `pathsUnder` and `labelHasPath`, which only existed for her; `ActiveNodeLabels` remained, `bundle.go` uses.

The pruning was done in the pipeline, not at the call sites (CLI and MCP). Both call sites (CLI and MCP) only declare `ForceRebuild: true`; how to honor this is a decision for the pipeline, which is the only place that knows whether it will republish the entire graph. It was the asymmetry before — the call site performed its own graph cleanup without considering what the pipeline would do next.

The scope by prefix was not replaced with anything: it already wasn't used. The `scoped` of the pipeline (`opts.ChangedPaths`/`DeletedPaths`, the daemon's path) handles deletion by file names without traversing the graph. `--reindex` continues to function — reparsees the files from that directory and republishes the entire shard cache graph, which is what it already did.

## Technical Debt

- [ ] `pathsUnder` materialized all `File.path` in Go slice before filtering — resolved by removal along with `DeleteRepository`.
- [ ] The pruning depends on `files` covering the corpus, which is an implicit precondition. `pruneVanished` clears from cache everything that isn’t in `live`; if someone ever calls a branch without the full list with partial lists, it prunes the rest of the project. It’s documented in NOTE within the function and the branch `scoped` is built out-of-band but there are no tests that would block on the rule. A test passing a partial list and requiring rejection (or some type that only discovery can produce) would close this.
- [ ] The divergence window between graph and search index continues to exist due to design. `json_rebuild.go:49-55` assumes: the index is rebuilt after the swap, so by rebuilding it responds with the new graph corpus earlier. This task removes the inversion (empty graph + full search), not the window. If ever imported, the path would be publishing both in the same rename, which today is impossible — they are separate files.
- ~~`--reindex` a large corpus still remains expensive due to write phase, and the cheapest way (delete `ladybugdb` while keeping `shards/`) isn’t exposed in flag.~~ **Rejected by Engineer on 2026-08-20**: no flag for this exists. Registered here instead of deleted because `pipeline.go:319-327` documents the path with measured numbers (~95 s against 16 min in a repo of 36k), and a future session that reads that comment will propose exposing it again. The decision has already been made: don’t propose.
- [ ] The pipeline budget continues to be open and aggravates any measurement done on this machine: the daemon was at an average CPU usage of 1027% during investigation and 475% during verification. Registered in memory; noted here because **it’s why T7 wasn’t measured**.

## System Knowledge

- The **CLI output is the state machine of Inline 237**: the last printed line defines the phase without needing a stack trace. This applies to any crash report here.
- **Crashed vs. slow depends on `wchar` in two samples, not `State` from the process:
  a LadybugDB thread pool stays at `futex_do_wait` most of the time while working normally.
- The delete phase **does not touch** `ladybugdb.search.sqlite`; the search index is rebuilt entirely after that phase, by the write phase. So, during a root reindex, the cost of traversing the graph to remove it is spent on something that would have been discarded anyway.
- The daemon has a serialized indexing slot (`daemon: waited for the indexing slot`, seen in logs with 20m29s wait), but it coordinates the daemon with itself — not with another process running in the terminal.

## Progress Log

### 2026-08-20

- The Engineer's Report with the process still live. Memory consulted first: two candidates (pipeline pricing by-pipeline, LadybugDB lock) raised and both discarded as primary cause, with the reason registered in Objective.
- T1 completed: two samples of `/proc/436704/io` to 20 seconds show ~51 MB written and WAL growing — real progress, not deadlock. Registered that `State: S` is not evidence of a hang.
- T2 completed: the output between `runners.go:315` and `:320`, right inside `DeleteRepository`. The backend had already opened in `:296`, which eliminated the opening lock. - T3 completed: derived that `prefix == "."` makes `$paths` contain all files, and the predicate to marry all lines, in ~20 labels.
- T4 completed: the model `nodes × files` (~2000×) explains the two times; the linear on nodes (40×) does not. The evaluation of `IN` by list scan is **inference of adjustment**, still not measured.
- Second defect identified, independent of the first: the delete phase does not pass through `indexProgressReporter` — same class of failure as this reporter documents as already hiding a real hang-up, an earlier phase. - Corrected line (`reindex` root → `reset`) due to embedding cache in `shards/`. Registered in Trade-offs.
- **T4b, answering the Engineer's question about the difference between `--reindex` and `--reset`** — followed `ForceRebuild` until the graph was published and discovered that **the delete is discarded**: a full rebuild constructs `<db>.<hex>` and renames it to production. T5 stopped being "make the delete fast" and became "remove the delete", which is simpler and safer.
- Verified in `pipeline.go:221,306-309,335,559-565,589` and `json_rebuild.go:128,407`.
- Verified (it was inferred) that the embedding cache resides in `<store>/shards/*.emb.json` and is keyed by content hash — this confirms the Trade-off against using `--reset` as correction, and explains why the two flags are not interchangeable. - Next step: T5 now small — do not call `DeleteRepository` when the pipeline will republish the graph anyway. T6 (progress) continues to be valid as it was, because the write phase also has long silence. T7 is not measurable while the process holds the database, and compiling now competes with the daemon at 1027% CPU.

Note: The inline codes are placeholders for actual code blocks or variables that should be replaced with their specific values in the translation context.

- **T5a–T5d implemented and verified.** The order was deliberate: the pruning (T5a) before the deletion (T5b), because while the delete existed, it masked part of the symptom; inverted, the tree would pass through a state where `--reindex` is worse than before.
- **Negative test performed:** I disabled the pruning with `if false &&`, recompiled, and the test failed on all three assertions (graph, `entities`, `files`). After restoring it worked fine. Without this step, the test would not prove anything.
- **Memory error in a wrong path turned into a correction:** `go build ./...` without `-tags fts5` failed with an undefined symbol, and TWO memory areas stated that this tag no longer existed since `fb19403`. They described August 16 to 19, 2026; `61f2dee` (2026-08-19) brought back SQLite along with `BUILD_TAGS := fts5` and the backup files. The two were corrected instead of left for the next session to stumble.
- **Flags' help corrected** (`ast.go`, and the description of `reset` in schema MCP): `--reindex` was described as "Remove only this repository's data before re-indexing", which precisely described the part that had just been removed. Now both texts say what they decide between them — that `--reset` discards embeddings, and `--reindex` does not.
- **Operational note for the next session on this machine:** The liblbug path has `!` in its name (`.../github.com/!ladybug!d!b/...`). Inserted into double quotes in an interactive shell,
  `!` triggers history expansion, and then returns a previous command's output before hanging — it seems the test is stuck, but that’s not so. Use `LBUG=$(echo ~/go/pkg/mod/github.com/*ladybug*/go-ladybug@v0.17.0/lib)`
  and cite the variable. This cost several attempts before I saw the pattern.

**Note:** The translation has been crafted to maintain the technical context and ensure clarity while preserving the original meaning of the Portuguese text in idiomatic English.

Closed review credit release - T4b opens

The second hypothesis was correct, and the answer is that the data was written on `460da0a` (2026-07-28) when writing still reached production — then deleting **really** mattered and resolved the symptom. `a6dd378` (2026-08-05) made all writing copy+rename, so deleting became inert, without anything failing. Currently, no path writes in place, so T5b/T5c can remove without conditions — and T5d covers the original symptom by the mechanism that truly governs it today: the shard cache pruning.

This translation aims to maintain the technical context while translating idiomatic expressions into natural English phrasing.
