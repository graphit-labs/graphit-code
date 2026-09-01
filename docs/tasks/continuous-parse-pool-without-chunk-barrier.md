---
title: Continuous parse pool — the per-chunk barrier goes, the RWMutex stays
status: done
created: 2026-08-10
updated: 2026-08-10
tags: [ast, pipeline, antlr, concorrencia, performance]
---

# Continuous parse pool — the per-chunk barrier goes, the RWMutex stays

## Objective

`ast index` stopped reporting progress for minutes at a time and looked hung —
observed on ~2500 files, with N-1 idle workers. The cause was not a deadlock: it was the
parse pool's chunk barrier. `runFileWorkerPool` parsed in blocks of
`antlrCacheCheckInterval` (250) files with a `wg.Wait()` between them, and **a chunk costs
its slowest file**. In a corpus whose file sizes vary by three orders of
magnitude, that tail dominates: a 704 KB PL/SQL procedure takes 24.3 s on its own, and the
few files of that size end up adjacent in walk order, so they land in the same
chunk and stall it.

The barrier existed for a real reason: it was the only point where the ANTLR package-level
caches (DFA / prediction-context, ~2 MB retained per PL/SQL file, never evicted) could
be reset without racing an in-flight parse. Goal of this task: remove the
barrier without reintroducing the race — and without letting the heap grow without bound.

## Implementation Details

The lock that makes this possible **already existed** in `internal/ast/antlr/common/parser_sll_ll.go`
(`staticMu sync.RWMutex`, with `LockParse`/`UnlockParse`/`WithCacheReset`), introduced to
cover a scenario the per-pipeline barrier never covered: the daemon runs one pipeline per
project and the MCP parses on demand, all in the same process, so a reset triggered by one
pipeline could race another's parser construction. The five native drivers
(`cobol85`, `db2`, `plsql`, `postgresql`, `tsql`) already take the read lock on the first line of
`Parse` and release it with `defer` — covering lexer/parser construction, which is where
`decisionToDFA` is read.

In other words: mutual exclusion was already guaranteed by the mutex. The barrier was redundant — and expensive.

**1. `internal/ast/pipeline.go` — a single continuous pool.**
A single producer feeds an unbuffered `paths` channel with every file, and
`opts.Workers` goroutines consume until the end. There is no intermediate `wg.Wait()` anymore, so
a worker stuck on a 24 s file does not stop any other from moving on.

The heap pressure check became a shared atomic counter (`sinceCheck`): every
`antlrCacheCheckInterval` completed files, the worker that closes the count consults
`antlrCachePressure()` and, if it is above the ceiling, calls `ResetAntlrCaches()`. The reset is
issued **between parses** of that worker — never from inside one — so the goroutine is not holding
the read lock when it asks for the write lock.

**2. `internal/ast/antlr_adapter.go` — race on the project grammar global.**
Found by the new test running under `-race`, and **predating this task**: the pool was always
concurrent. `parseWithConfig` did

```go
if a.projectDir != "" && antlrGrammarProjectDir == "" {
    antlrGrammarProjectDir = a.projectDir
}
```

— reading and writing a global, from every worker. All of them write the same absolute
path (the pool builds each `CompositeParser` with the same `abs`), so the result was never
incorrect; it is still a formal data race and `-race` kills the run, which is the CI
gate. The global is now guarded by `antlrGrammarProjectDirMu sync.RWMutex`, with
`setAntlrGrammarProjectDirIfUnset` doing test-and-set under a single lock (checking with the read
lock and then taking the write lock would let two workers observe `""` and both write) and
`grammarProjectDir()` for reading. `SetAntlrGrammarProjectDir` now locks as well.

**3. Comments that described the old design.** The `antlrCacheCheckInterval` block
still said that each check was a barrier and "the only point where the caches can be
safely reset"; the one on `ResetAntlrCaches` said "NOT safe to call while a parse is in
flight". Both now describe the real contract, including the one restriction that remains:
whoever calls the reset must not already be holding the read lock.

## Use Cases

### UC-01: Index a repository with very uneven file sizes
- **Actor**: `graphit ast index` (CLI), the daemon's `SyncModule`, the `ast_index` MCP tool.
- **Preconditions**: repository with more than `antlrCacheCheckInterval` parseable files; at least one file whose parse is orders of magnitude slower than the median.
- **Main Flow**:
  1. `RunPipeline` → `runFileWorkerPool` discovers the files and assembles `changedFiles`.
  2. A producer sends every path over an unbuffered channel.
  3. `opts.Workers` workers consume continuously; each builds its own `CompositeParser` and calls `Parse` per file.
  4. Each result goes to the `results` channel, consumed by the main loop, which writes to the cache and emits `OnProgress("parsing", ...)`.
  5. The slow file occupies **one** worker; the rest keep draining the queue to the end.
- **Alternative Flows**:
  - Scoped run (`ChangedPaths`/`DeletedPaths` populated): same mechanics, without the tree walk.
- **Error Scenarios**:
  - Parse error on a file: accounted for in `ErrorCount`/`ErrorFiles`; the other workers are unaffected.
  - `ctx` canceled: producer and workers exit through the `<-ctx.Done()` arm; the unbuffered channel has a cancellation arm on both sides, so there is no permanent block.
- **Postconditions**: every discovered file was parsed exactly once; progress advances continuously instead of jumping from chunk to chunk.
- **Affected Files**: `internal/ast/pipeline.go`.

### UC-02: Contain the ANTLR heap without draining the pool
- **Actor**: parse pool worker.
- **Preconditions**: at least `antlrCacheCheckInterval` files completed since the last check.
- **Main Flow**:
  1. The worker finishes a parse and delivers the result.
  2. It increments `sinceCheck`; on reaching the interval, it zeroes the counter.
  3. It calls `antlrCachePressure()` (a `runtime.ReadMemStats`, a brief stop-the-world).
  4. Above `antlrCacheHeapLimit`, it calls `ResetAntlrCaches()`, which takes `antlrcommon`'s write lock and swaps the `decisionToDFA` of every registered grammar.
  5. In-flight parses finish holding the read lock; the reset waits for them and only then runs.
- **Alternative Flows**:
  - Heap below the ceiling: nothing is reset — a warm DFA is worth more than the memory it occupies.
  - End of the pool: one final check, because the caches are package-level and stay **alive** (they do not become garbage) in a long-lived daemon.
- **Error Scenarios**:
  - Reset requested from inside a parse (hypothetical regression): the goroutine already holds the read lock and would ask for the write lock — deadlock. That is what the new test detects by timeout.
- **Postconditions**: heap contained; no parse observed static state being swapped underneath it.
- **Affected Files**: `internal/ast/pipeline.go`, `internal/ast/antlr_adapter.go`, `internal/ast/antlr/common/parser_sll_ll.go`.

### UC-03: Publish the project's grammar directory from concurrent workers
- **Actor**: `AntlrParser.parseWithConfig`, one per worker.
- **Preconditions**: `a.projectDir` not empty; `antlrDrivers` not yet initialized.
- **Main Flow**:
  1. The worker calls `setAntlrGrammarProjectDirIfUnset(a.projectDir)`.
  2. Under the write lock, the global takes the value only if it is still empty.
  3. `antlrDriversOnce.Do(initAntlrDrivers)` reads the value through `grammarProjectDir()` and looks for sidecar binaries in `<projeto>/.graphit/grammars/antlr` and `~/.graphit/grammars/antlr`.
- **Alternative Flows**:
  - `SetAntlrGrammarProjectDir` called from outside: overwrites unconditionally, also under the write lock.
- **Error Scenarios**:
  - No sidecar binary found: the compiled native driver is used.
- **Postconditions**: a single published value, with no data race.
- **Affected Files**: `internal/ast/antlr_adapter.go`.

## Test Cases & Acceptance Criteria

### Feature: Continuous parse pool with concurrent cache reset
Ref: UC-01, UC-02

#### Scenario: reset fires with real ANTLR parses in flight
```gherkin
Given a project with 24 PL/SQL files, each with a distinct body
  And antlrCacheCheckInterval equal to 1
  And antlrCacheHeapLimit equal to 0, so that the pressure check always indicates pressure
  And the pipeline configured with 4 workers and the grammar pinned to "antlr-plsql"
When RunPipeline indexes the project
Then the 24 files are parsed without error
  And the published graph contains exactly 24 .sql files
  And the run finishes — a reset issued from inside a parse would hang it until the timeout
```

#### Scenario: concurrent reset does not race parser construction
```gherkin
Given eight goroutines parsing PL/SQL continuously through the native driver
When ResetAntlrCaches is called 25 times in sequence
Then Go's race detector reports no race
```
Ref: `TestResetAntlrCachesRace` (already existing, `internal/ast/antlr_race_reset_test.go`)

#### Scenario: concurrent publication of the project's grammar directory
```gherkin
Given four workers calling parseWithConfig with the same projectDir
When each one publishes the project's grammar directory
Then Go's race detector reports no race
  And the global holds that directory
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/pipeline.go` | Modified | Continuous pool in place of the barriered chunks; shared atomic counter for the pressure check; `antlrCacheCheckInterval` comment updated |
| `internal/ast/antlr_adapter.go` | Modified | `antlrGrammarProjectDir` guarded by RWMutex with test-and-set; `ResetAntlrCaches` contract corrected |
| `internal/ast/pipeline_reset_inflight_test.go` | Created | Covers cache reset with real ANTLR parses in flight through the full pipeline |

## Trade-offs & Decisions

- **Mutex instead of barrier.** The barrier gave exclusion for free, but at the cost of serializing
  the pool every 250 files. The mutex was already there for a case the barrier did not cover
  (multi-project daemon in the same process), so keeping both was paying twice for the
  same guarantee.
- **The reset still stops the world, and that is accepted.** Under heap pressure, the worker asking
  for the write lock waits for every in-flight parse to finish, and Go's RWMutex blocks new read
  locks while a writer is waiting — so a 24 s file in flight still stalls the pool
  for up to 24 s. The difference is that this now happens **only under heap pressure**, not
  every 250 files unconditionally. Eliminating that window would require per-grammar reset or
  per-worker caches, which is another task.
- **`sinceCheck.Add(1) >= interval` followed by `Store(0)` is not atomic as a pair.** Two
  workers can close the count almost together and both reset; the second reset is cheap
  (the caches were just swapped) and the lost count merely postpones the next check.
  Switching to a CAS would complicate the loop to fix something with no consequence.
- **The global's race was fixed here, not deferred.** It predates this task, but the
  new test exposes it, and a test that fails under `-race` in CI cannot be delivered as
  "known debt".

## Technical Debt

- [ ] A reset under pressure still blocks the whole pool for the duration of the longest in-flight
  parse (see Trade-offs). Directions: per-grammar reset, or per-worker caches instead of
  package-level ones.
- [ ] `SetAntlrGrammarProjectDir` is exported and has no caller in the graph. Either it is an
  entry point for another binary, or it is dead code — check before removing.
- [ ] `TestParsePoolResetsAntlrCachesWithParsesInFlight` overwrites `antlrCacheCheckInterval`
  and `antlrCacheHeapLimit`, which are package globals, so it does not declare `t.Parallel()`.
  If `internal/ast` gets parallelized (see the memory about `make test` being slow), this
  test needs explicit isolation.

## System Knowledge

- **The reset budget is driven by pressure, not by count.** Resetting every 500
  files cost ~78% more parse time; never resetting took the heap to 23 GB on an Oracle
  corpus of 35k files. That is why the check exists and why it is amortized: each
  check is a `runtime.ReadMemStats`, which is stop-the-world.
- **The ANTLR caches stay alive, they do not become garbage.** They are package-level, so the GC never
  collects them — hence the final check after `wg.Wait()`, without which a long-lived daemon
  holds the entire budget until the process dies.
- **Every native driver holds the read lock through construction too.** This is not a detail:
  `decisionToDFA` is read in `NewXxxParser`/`NewXxxLexer`, before any rule runs. A
  lock covering only `Parse()` would leave the window open.
- **`ParseWithGrammar` and extension-based discovery consult different global tables.**
  `antlrExtMap` decides whether the extension is discovered; `antlrGrammarMap` resolves the
  `--grammar` override. A test that wants PL/SQL end to end has to populate both — the
  tables are built from the runtime and user query directories, so a grammar local to the
  project is discovered but not selectable through them.

## Progress Log

### 2026-08-10
- The previous session ended in a machine freeze (reisub); the diff of `pipeline.go` and
  `antlr_adapter.go` survived in the working tree, with no task log.
- Confirmed in the graph the premise the change rests on: the 5 native drivers take
  `LockParse` on line 14 of each `driver.go` and release it via `defer` on line 15.
- Fixed the `antlrCacheCheckInterval` comment, which still described the barrier.
- Wrote `TestParsePoolResetsAntlrCachesWithParsesInFlight`. Under `-race` it exposed a
  **pre-existing** data race in `antlr_adapter.go:256-257` (the `antlrGrammarProjectDir` global).
- Fixed the race with RWMutex + test-and-set.
- Green: all of `internal/ast` with and without `-race` (98.3 s under `-race`); `go vet` and `gofmt`
  clean in the package.
