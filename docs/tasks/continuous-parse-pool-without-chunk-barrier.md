---
title: Continuous parser pool barrier - the chunk-based RWLock remains
status: done
created: 2026-08-10
updated: 2026-08-10
tags: [ast, pipeline, antlr, concorrencia, performance]
---

Continuous parser pool barrier falls when chunk exits, RW Mutex remains

## Objective

The `ast index` stopped reporting progress in short intervals and seemed stuck —
observed in ~2500 files with N-1 idle workers. The cause was not a deadlock; it was the
chunk barrier of the parse pool. `runFileWorkerPool` parsed in blocks of `antlrCacheCheckInterval` (250) files with an `wg.Wait()` between them, and **a chunk cost its slowest file**. In a corpus whose file sizes vary by three orders of magnitude, this tail dominates: a PL/SQL procedure of 704 KB runs alone in just 24.3 seconds, and the few files of this size are adjacent in the walk order, so they fall into the same chunk and get stuck.

Note: The inline codes (`ast index`, `runFileWorkerPool`, `antlrCacheCheckInterval`, `wg.Wait()`) have been omitted for brevity.

The barrier existed for a real reason: it was the only point where package-level caches of ANTLR (DFAs / prediction-context, ~2 MB retained per PL/SQL file, never flushed) could be reset without running afoul of an ongoing parse. Objective of this task: remove the barrier without reintroducing the race — and without leaving the heap growing unchecked.

## Implementation Details

The lock that makes this possible **already existed** in `internal/ast/antlr/common/parser_sll_ll.go`
(`staticMu sync.RWMutex`, with `LockParse`/`UnlockParse`/`WithCacheReset`), introduced to cover a scenario that the pipeline barrier never covered: the daemon runs one pipeline per project and the MCP parses on demand, all within the same process. Therefore, a reset initiated by a pipeline could run against the construction of another parser’s lexer/parser, where `decisionToDFA` is read.

---

Note: The inline codes are placeholders for actual code snippets or paths that should be replaced with the specific content when translating to English.

In other words: mutual exclusion was already guaranteed by the mutex. The barrier was redundant— and expensive.

**1. INLINE 17 — a single continuous pool.**
A producer feeds a non-buffered channel INLINE 18 with all the files, and INLINE 19 goroutines consume until the end. There are no more intermediate steps, so a worker stuck on a 24-second file does not prevent any other from proceeding.

INLINE 17: Inline 17
INLINE 18: Channel 18
INLINE 19: Goroutine 19
INLINE 20: Intermediate step

The heap pressure check has become an atomic shared counter (`sinceCheck`): every time `antlrCacheCheckInterval` completed files are closed, the worker that closes the count consults `antlrCachePressure()` and if it is above the ceiling, calls `ResetAntlrCaches()`. The reset is emitted **between parses** of this worker — never inside one — so the goroutine does not lock up when requesting the write lock.

**2. Inline 25 - Grammar race in the global project sprint.**
Found by the new test running on ___Inline_26__, and **prior to this task:** the queue has always been a contender.


```go
if a.projectDir != "" && antlrGrammarProjectDir == "" {
    antlrGrammarProjectDir = a.projectDir
}
```

— Reading and writing in a global, from all the workers. All workers write the same absolute path (the pool builds each `CompositeParser` with the same `abs`), so the result has never been incorrect; still it is a formal data race and the `-race` breaks execution, which is the CI gate. The global passed to be stored by `antlrGrammarProjectDirMu sync.RWMutex`, with `setAntlrGrammarProjectDirIfUnset` doing a read-write lock under one lock (check with the read lock and then get the write lock would leave two workers observing `""` and both writing), and `grammarProjectDir()` for reading. `SetAntlrGrammarProjectDir` also passed to be blocked.

**3. Comments describing the old design.**

The block of `antlrCacheCheckInterval` still stated that each check was a barrier and "the only place where cache resets can be safely performed"; the one at `ResetAntlrCaches` said, "Not safe to call while a parse is in flight." Both began describing the real contract, including the sole remaining restriction: who calls the reset cannot already hold the read lock.

## Use Cases

### UC-01: Indexing a Repository with Very Uneven File Sizes

**Actor**: CLI (Command Line Interface) and daemon, tool MCP (Multi-File Processor).

**Preconditions**: More than `antlrCacheCheckInterval` parseable files in the repository; at least one file whose parsing is orders of magnitude slower than the median.

**Main Flow**:
1. The producers send all paths via a non-buffered channel.
2. Workers continuously consume, each building their own `CompositeParser` and calling `Parse` for each file.
3. Each result goes to the channel `results`, which is consumed by the main loop, which stores in cache and emits `OnProgress("parsing", ...)`.
4. The slow file occupies **one** worker; the others drain the queue until it's empty.

- Alternative Flows:
  - Scoping execution (`ChangedPaths`/`DeletedPaths` filled): same mechanics without tree traversal.

- Error Scenarios:
  - Parsing error in an input file: counted in `ErrorCount`/`ErrorFiles`; the other workers are unaffected.
  - Cancelled: producers and workers exit via `<-ctx.Done()`; the non-buffered channel has cancellation arms on both sides, so there's no permanent blockage.

- Postconditions:
  - Every discovered file is parsed exactly once; progress advances continuously rather than jumping chunk by chunk.

**Affected Files**: `internal/ast/pipeline.go`

### UC-02: Prevent the ANTLR heap without draining the pool

**Actor**: Worker in the parse pool

**Preconditions**: At least `antlrCacheCheckInterval` completed files since the last check.

**Main Flow**:
1. The worker finishes a parse and delivers the result.
2. Increments `sinceCheck`; when it reaches the interval, resets the counter.
3. Calls `antlrCachePressure()` (a brief stop-the-world).
4. Above `antlrCacheHeapLimit`, calls `ResetAntlrCaches()`, which takes a write lock on `antlrcommon` and replaces the `decisionToDFA` of each registered grammar.
5. Parses in flight end holding the read lock; the reset waits for them before executing.

**Alternative Flows**:
- Below the ceiling: nothing is resetted — a warm DFA is worth more than the memory it occupies.
- End of pool: a final check, since caches are package-level and remain **alive** (not considered garbage) in a long-lived daemon.

**Error Scenarios**:
- Requested reset inside a parse (hypothetical regression): the goroutine already holds the read lock and would request the write lock — deadlock. This is what the new test detects by timeout.

**Postconditions**: heap contained; no parse observed state transition underneath itself.
- Affected Files: `internal/ast/pipeline.go`, `internal/ast/antlr_adapter.go`, `internal/ast/antlr/common/parser_sll_ll.go`

### UC-03: Publish the grammar directory of the project from concurrent workers

**Actor**: Actor 68, one per worker.

**Preconditions**: The inline condition is not empty; the inline variable has not been initialized yet.

**Main Flow**:
1. Worker calls `setAntlrGrammarProjectDirIfUnset(a.projectDir)`.
2. Under write lock, the global receives only if it is still empty.
3. `antlrDriversOnce.Do(initAntlrDrivers)` reads the value from `grammarProjectDir()` and searches for sidecar binaries in `<projeto>/.graphit/grammars/antlr` and `~/.graphit/grammars/antlr`.

**Alternative Flows**:
- Called externally: always overwrites, also under write lock.

**Error Scenarios**:
- No sidecar binary found: uses the native compiled driver.

**Postconditions**: Publishes a single value without data race.
- Affected Files: `internal/ast/antlr_adapter.go`.

## Test Cases & Acceptance Criteria

Feature: Continuous Parse Pool with Concurrent Cache Reset

Ref: UC-01, UC-02

Scenario: The Reset Button Fires Real-Time ANTLR Parses on Flight
```gherkin
Given um projeto com 24 arquivos PL/SQL, cada um com um corpo distinto
  And antlrCacheCheckInterval igual a 1
And with antlrCacheHeapLimit set to 0, ensuring that pressure checks always indicate pressure.
And the pipeline configured with four workers and the grammar fixed in "ANTLR-PL/SQL"
When RunPipeline indexa o projeto
Then the twenty-four files are parsed without error.
And the published graph contains exactly 24 SQL files.
And execution ends - a reset issued from within a parse would stall until the timeout.
```

Scenario: Resetting Concurrently Does Not Contradict Parser Construction
```gherkin
Given oito goroutines parseando PL/SQL continuamente pelo driver nativo
When `ResetAntlrCaches` is called 25 times consecutively
Then the race detection of Go does not report any races.
```
Ref: `TestResetAntlrCachesRace` (already existing, `internal/ast/antlr_race_reset_test.go`)

Scenario: Concurrent Publication of Grammar Directory for Project
```gherkin
Given quatro workers chamando parseWithConfig com o mesmo projectDir
When each publishes the directory of grammar projects
Then the Go race detector does not report any races
And the world has its own directory.
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/pipeline.go` | Modified | Pool continuous in place of chunks with barrier; shared atomic counter for pressure check; comment `antlrCacheCheckInterval` updated |
| `internal/ast/antlr_adapter.go` | Modified | `antlrGrammarProjectDir` stored by RWMutex with test-and-set; contract `ResetAntlrCaches` corrected |
| `internal/ast/pipeline_reset_inflight_test.go` | Created | Covers reset of cache with real parses from ANTLR in full pipeline |


## Trade-offs & Decisions

- **Mutex instead of barrier.** The barrier provided graceful exclusivity but at the cost of serializing the pool every 250 files. The mutex was already there for a case that the barrier did not cover (daemon multi-project in the same process), so keeping both was paying twice for the same guarantee.
- **The reset still applies to the world, and this is accepted.** Under heap pressure, the worker that requests the write lock waits for all parses to finish before proceeding, and the RWMutex from Go blocks new read locks while there are writers waiting—so a file of 24 seconds in flight still stymies the pool until 24 seconds have passed. The difference is that now this happens **only under heap pressure**, not every 250 files conditionally. Removing this window would require reset by grammar or worker caches, which is another task.
- **`sinceCheck.Add(1) >= interval` followed by `Store(0)` is not atomic as a parallel.** Two workers can almost close the count simultaneously and both reset; the second reset is cheap (caches have just been swapped) and the loss of count only delays the next check. Switching to a CAS would complicate the loop to correct something that does not have consequences.
- **The global race has been corrected here, not deferred.** It precedes this task but exposes the new test, and a test that fails under `-race` in CI cannot be delivered as "known credit."

## Technical Debt

- [ ] A forced reset still blocks the entire pool during the duration of the longest parse in flight (see Trade-offs). Directions: Reset by grammar, or use worker-level caches instead.
- [ ] `SetAntlrGrammarProjectDir` is exported and has no caller in the graph. Or it's an entry point for another binary, or dead code — check before removing.
- [ ] `TestParsePoolResetsAntlrCachesWithParsesInFlight` overrides `antlrCacheCheckInterval` and `antlrCacheHeapLimit`, which are global package globals, so it declares `t.Parallel()`.
  If `internal/ast` is parallelized (see memory about the latency of `make test`), this test needs explicit isolation.

## System Knowledge

- The reset budget is directed by pressure rather than count. Resetting every 500 files cost ~78% more parse time; never resetting led the heap to 23 GB in an Oracle corpus of 35k files. Therefore, there exists a check and it is amortized: each check is an INLINE_96, which is stop-the-world.
- The ANTLR caches remain alive, not discarded as garbage. They are package-level, so the GC never collects them — that's why the final check after INLINE_97 does not exist without which a long-lived daemon ensures the entire budget until the process dies.
- Every native driver secures the read lock by construction too. It is not a detail: INLINE_98 is read in INLINE_99/INLINE_100 before any rule runs. A lock that covers only INLINE_101 would leave the window open.
- INLINE_102 and discovering extension consult different global tables.
- INLINE_103 decides whether the extension is discovered; INLINE_104 resolves the override INLINE_105_. A test that wants PL/SQL from start to finish needs both — the tables are built from runtime query directories and user directories, so a project-specific grammar is discovered but not selectable by them.

## Progress Log

### 2026-08-10

- The previous session was terminated due to a machine crash (reisub); the diff of `pipeline.go` and `antlr_adapter.go` survived in the working tree, without a task log.
- It has been confirmed on the graph that the premise supporting the change: the 5 native drivers take `LockParse` at line 14 of each `driver.go` and release by `defer` at line 15.
- The comment of `antlrCacheCheckInterval` has been corrected, which still described the barrier.
- Written `TestParsePoolResetsAntlrCachesWithParsesInFlight`. It was written on ___INLINE_113__, exposing a pre-existing race in `antlr_adapter.go:256-257` (global `antlrGrammarProjectDir`).
- The race with RWMutex + test-and-set has been corrected.
- Green: `internal/ast` is complete both with and without `-race` (98.3 seconds on `-race`); `go vet` and `gofmt` are clean in the package.

Note: The inline comments and file paths have been preserved as per your request.
