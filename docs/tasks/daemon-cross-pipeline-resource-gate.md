---
title: Budget for resources across pipelines, lock of the sync, and debouncing of Git hooks
status: done
created: 2026-08-08
updated: 2026-08-08
tags: [daemon, performance, recursos, sync, hooks, competition]
---

Budget for resources between pipelines, lock of the sync, and debouncing of Git hooks

## Objective

The user machine (20 CPUs, 61 GB of RAM, swap of 2 GB) crashed with load average 30.47 and swap at 100%. The measurement pointed to the Graphit daemon with a CPU lifetime of approximately 1047% — roughly 20.6 hours of CPU usage in 1h58m of process — and 10.7 GB of RSS, competing with an `gce ast index` and three `graphit-core sync --heavy` simultaneously.

The inline codes are placeholders for the actual inline code or inline content that should be present in the original text but have been omitted due to space constraints.

The user's hypothesis was that it doesn't make sense to run `sync --heavy` in parallel. The investigation confirmed the hypothesis and found a larger cause behind it: the resource budget for the indexer is applied **via pipeline**, not through processes, and the daemon runs N pipelines simultaneously within the same process.

The objective of this task is to close the three gaps: the pipeline budget gap, the duplication of synchronization processes, and the triggers that produce this duplication.

Outside the scope due to an explicit user decision: reject the registration of directories under
`/tmp` as a project, and reappeal the orphaned processes `graphit-mcp`.

## Implementation Details

Diagnosis (what was established by the investigation)

1. The `sysutil.CPUBudget()` (`internal/sysutil/cpu.go:14`) returns `NumCPU - max(2, NumCPU/4)`.
    — 15 in the measured machine. They derive three independent pools:
   the Go workers (`ast.SafeWorkers`, `internal/ast/throttle.go:56`), native threads of LadybugDB
   (`boundedDBThreads`, `internal/ast/resources.go:96`), and ONNX intra-op (`boundedEmbedThreads`, `internal/ai/embedding_local.go:151`). The `ortInitOnce` only initializes the *environment* of ONNX Runtime; the session is client-based, so each embedding module opens its own.
2. `internal/daemon/daemon.go:reconcileProjects` and `ProjectSupervisor.Start`
   (`internal/daemon/project.go:70`) had no semaphore at all. Each active project gets a supervisor with two modules running in parallel, all in the same process. The `~/.graphit/daemon/daemon.log` registers three live supervisors between 11:11:39 and 11:12:09 (this repository, private corpus, and a temporary directory) — exactly the window where the user measured the hang.
3. `boundedDBBufferPool` limits to 1 GiB **per database**, and the incremental rebuild keeps two open (production + copy+swap replica). Three projects × two databases = up to 6 GiB of buffer pool only.
4. `spawnBackgroundSync` (`cmd/graphit/commands/lifecycle.go`) executed
   `sync --heavy` without locking or checking if it was already running.
5. The duplication trigger was the git hooks: `internal/git/hooks.go` installed
   `(graphit sync </dev/null >/dev/null 2>&1 &)` in `post-commit`, `pre-push` **and**
   `post-merge`. Each one runs the full Phase 1 (reindex complete of AST + knowledge + memory cycles) synchronously (full reindex of AST + knowledge + memory cycles), and only then spawns the `--heavy`.

Please note that the inline code blocks are preserved as is, without translation.

Changes

**`internal/sysutil/gate.go` (new)** — heavy lifting semaphore, process scope.
`AcquireHeavy(ctx)` returns the function of release (idempotent, safe for `defer` alongside an explicit call).
`HeavySlots()` resolves capacity: 1 by default, __INLINE_33__ overrides, always limited by `CPUBudget()`. A `ctx` canceled returns `ctx.Err()` with nil release and the caller **must** skip the work.
`resetHeavyGate()` exists only for testing.

**`internal/lockfile/` (new)** — consultative file lock, between processes.
`TryAcquire(path)` returns `ErrLocked` when another process holds the lock; `Release()` is idempotent and safe in a `*Lock` nil. The lock lives in the *open file description*, so a dying process liberates itself alone — there are no obsolete locks to clean up nor PIDs to validate. The PID and timestamp recorded in the file are for those opening during debugging.

---INLINE_43--- ---INLINE_44--- transitioned from two independent `switch`/`if`
to an explicit decision (`astWork`, `knowledgeWork`) followed by a single gate acquisition for the entire batch. A batch without work returns **before** touching the gate, so an idle supervisor never sits behind a busy one. An wait above 1s is logged in the log.

---INLINE_43--- ---INLINE_44--- transitioned from two independent `switch`/`if`
to an explicit decision (`astWork`, `knowledgeWork`) followed by a single gate acquisition for the entire batch. A batch without work returns **before** touching the gate, so an idle supervisor never sits behind a busy one. An wait above 1s is logged in the log.

**INLINE_49** — the repeated body of the cycle (initial and tick-based) turned into a closure **INLINE_50** that takes the gate for the duration of the cycle and the `triggerEmbeddingRebuild` that follows. Unique collateral effect in log: the error message from the periodic cycle passed from **INLINE_52** to **INLINE_53**.

**INLINE_54** — the command **INLINE_55** gained:
- **INLINE_56**, with three outcomes: free lock (follows it), locked (skips — the other is already doing this), or unable to create a lock (continues **without** lock, degrading to old behavior and registering in **INLINE_57**).
- Separated locks by phase: **INLINE_58** for Phase 1 and **INLINE_59** for Phase 2.
- Flag **INLINE_60**, with **INLINE_61** reading **INLINE_62** and **INLINE_63** writing the stamp at the end of Phase 1.

**`internal/git/hooks.go`** — os hooks passam `--debounce 60s`, via a constante
`hookDebounce`.

Why two locks, not one

Spawn `sync --heavy` and return immediately after spawning it. With just one lock, the child would run against its own father's release and accidentally lose. Two phases, two locks, each idempotent relative to the other.

Why is the stamp placed after Phase 1

Phase 2 is forget-me-not for the project. Waiting for it would keep the debounce window open indefinitely during an embedding round, and Phase 1 is exactly what a Git hook exists to trigger.

## Use Cases

### UC-01: Two active projects reindex simultaneously on the daemon

**Actor:** The daemon (`ProjectSupervisor` of each active project)

**Preconditions:** The daemon is running; two or more projects within the activity window;
modified files in more than one of them.

**Main Flow:**

1. Each `fswatch` delivers a batch to its `handleBatch`.
2. `handleBatch` classifies the batch and decides `astWork` / `knowledgeWork`.
3. If there is work, it calls `sysutil.AcquireHeavy(ctx)`.
4. The first one arrives takes the only slot; the others wait.
5. The holder runs `reindexAST` or `reindexKnowledge` and releases the slot on `defer`.
6. The next in line proceeds.

**Alternative Flows:**

- `GRAPHIT_HEAVY_SLOTS=N` increases capacity to `min(N, CPUBudget())`.
- A batch with `Rescan` is work for both indexers, regardless of which paths it names.

**Error Scenarios:**

- During the wait (supervisor parked or daemon shutting down), `ctx` is canceled:
  - `AcquireHeavy` returns an error; `handleBatch` does not index and the slot that was never obtained is not released. The next batch reforges the work.

**Postconditions:** At most `HeavySlots()` pipelines are running in the process.

- **Affected Files**: `internal/sysutil/gate.go`, `internal/daemon/syncmodule.go`

### UC-02: A batch without work does not enter the queue

**Actor**: daemon (`SyncModule`)

**Preconditions**: batches whose paths are all ignored, or extensions without a parser.

**Main Flow**:
1. Classifies and retrieves `astWork == false` and `knowledgeWork == false`.
2. Immediately returns without calling `AcquireHeavy`.

**Error Scenarios**: None — the path does not have any possible failure.

**Postconditions**: The slot remains available for those who need to do something.

**Affected Files**: `internal/daemon/syncmodule.go`

### UC-03: Embedding Cycle Competes with Reindex

**Actor**: daemon (via `EmbeddingModule`)

**Preconditions**: the embedding loop is active; entities pending for embedding.

**Main Flow**:
1. The tick triggers `cycle("embedding cycle", true)`.
2. A closure takes the weighted slot.
3. Reloads the parse cache, runs `embedder.RunCycle`, and if the cycle produces something,
   `triggerEmbeddingRebuild`.
4. Releases the slot.

**Alternative Flows**: 
- The initial embedding loop runs as `cycle("initial cycle", false)` without reloading the recently opened cache.

**Error Scenarios**:
- `ctx` is canceled during waiting: the cycle skips; the next tick tries again.
- `RunCycle` fails: registered as `<label> error`; the slot is released by `defer`.

**Postconditions**: The embedding cycle never runs alongside a reindex of the same process.

**Affected Files**: `internal/ast/embedder.go`

### UC-04: Two processes are fired almost simultaneously

- **Actor**: CLI (spawned by __INLINE_107__, typically), usually spawned by __INLINE_108__ or __INLINE_109__
- **Preconditions**: one phase already executing on the same project.
- **Main Flow**:
  1. The second process calls __INLINE_111__.
  2. __INLINE_112__ returns __INLINE_113__.
  3. The command silently returns 0, without proceeding to Phase 2.

- **Error Scenarios**:
  - The lock could not be created (directory lacks permissions): the error goes to __INLINE_114__, and the command continues without locking — never stops synchronizing due to this.

- **Postconditions**: one Phase 2 per project, once.
- **Affected Files**: __INLINE_115__ and __INLINE_116__

### UC-05: Commit followed by push triggers hooks in sequence

- **Actor**: git (`post-commit`, then `pre-push`)
- **Preconditions**: Hooks installed; the tree has changed once.
- **Main Flow**:
  1. `post-commit` runs `graphit sync --debounce 60s`.
  2. There is no recent signature, the lock is free: Phase 1 runs and writes
     `.graphit/sync.stamp`.
  3. A few seconds later, `pre-push` runs the same command.
  4. `syncedWithin` reads the signature, sees that it's less than 60s old, and returns 0 without doing anything.

- **Alternative Flows**:
  - The two hooks overlap instead of following each other: the second loses
    `.graphit/sync.lock` and exits silently — debouncing and locking cover different cases.
  - `graphit sync` interactive (without `--debounce`): never skips the signature, and when it loses the lock, prints "Another sync is already running — skipping" instead of just exiting quietly.

- **Error Scenarios**:
  - `.graphit/sync.stamp` missing, illegible, or with invalid content: reads as "I don't know" and runs the sync. The debounce only skips what it can prove redundant.

- **Postconditions**: one Phase 1 per 60-second window for each project.
- **Affected Files**: `internal/git/hooks.go`, `cmd/graphit/commands/lifecycle.go`

## Test Cases & Acceptance Criteria

### Feature: Cross-pipeline resource gate
Ref: UC-01, UC-02, UC-03

Scenario: Eight Heavy Concurrent Jobs Never Interfere
```gherkin
Given that GRAPHIT_HEAVY_SLOTS is defined as "1"
And the gate was reconstructed from the environment.
When eight goroutines call AcquireHeavy and hold the slot for one millisecond each.
The maximum observed competition is one.
```

Scenario: The slot returns to the queue after release
```gherkin
Given that GRAPHIT_HEAVY_SLOTS is defined as "1"
And one goroutine obtained the sole slot.
When she calls the function twice
Then a new call to AcquireHeavy obtains the slot.
And the duplicate release did not free up an nonexistent second slot.
```

#### Scenario: Uma espera cancelada abandona a fila
```gherkin
Given that GRAPHIT_HEAVY_SLOTS is defined as "1"
And the only slot is occupied.
When AcquireHeavy is called with an already canceled context
The error is context-canceled.
The function's return value for release is null.
```

Scenario Outline: The capacity is limited by the CPU budget
```gherkin
Given that GRAPHIT_HEAVY_SLOTS is defined as "<override>"
When HeavySlots is consulted
The result is "<expected>"

Examples:
  | override | expected                |
  | <vazio>  | 1                       |
  | 3        | min(3, CPUBudget())     |
  | 100000   | CPUBudget()             |
```

Scenario: An empty batch does not queue up
```gherkin
The only heavy slot is already occupied by another project.
When handleBatch receives an unindexable batch
Then ele retorna em menos de 5 segundos
  And nunca chamou AcquireHeavy
```

Scenario: A supervisor is not indexed in the output
```gherkin
Given an IndexingSyncModule pointed to a project without an index
When `handleBatch` is called with a batch of Rescans and an aborted context
Then none of the AST banks are open in the project directory
And the heavy slot remains available for the next caller.
```

### Feature: Advisory file lock
Ref: UC-04

The second holder is rejected.
```gherkin
Given um lock foi obtido em .graphit/sync.lock
When `TryAcquire` is called on the same path by another open file description
The error returned is ErrLocked
And no lock is returned with the error
```

Scenario: The lock is released for the next one.
```gherkin
Given um lock foi obtido e depois liberado
When `TryAcquire` is called on the same path
Then it is unlocked.
```

Scenario: The two phases do not compete for the same lock
```gherkin
Given a Phase 1 lock.
When a Fase 2 pede .graphit/sync-heavy.lock
Then it is unlocked.
```

Scenario: Release is Idempotent
```gherkin
Given um lock foi obtido
When "Release" is called twice
And Release is called in a *Lock Nil state.
Then none of the calls panics.
```

Scenario: The file names who holds it
```gherkin
Given um lock foi obtido
When the lock file is read
Then the first line is the process ID of the holding process.
And the second line is a timestamp.
```

### Feature: Hook debounce
Ref: UC-05

Scenario: A recent synchronization has been skipped
```gherkin
Given stampSync acabou de gravar .graphit/sync.stamp
When it is consulted, it uses a window of one minute.
Then ele responde que o sync deve ser pulado
```

Scenario Outline: Windows That Cannot Jump Anything
```gherkin
Given "<estado>" do carimbo
When it is consulted, it uses the window "<window>".
Then ele responde que o sync deve rodar

Examples:
  | estado                        | janela      |
  | nenhum carimbo em disco       | 1 minuto    |
Stamp with an invalid text | 1 hour
Carved stamp newly impressed with 0.
Stamp newly engraved | -1 minute
Carving newly engraved is 1 nanosecond.
```

#### Scenario: O script do hook carrega a janela
```gherkin
Given that hookScript is generated for any of the three hooks
When it's inspected
It contains "--debounce" followed by "hookDebounce".
And `hookDebounce` is a duration that the command `sync` can interpret.
```

## Files Changed

Here is the translation from Brazilian Portuguese to idiomatic English:

```markdown
| File | Change | Reason |
|---|---|---|
| `internal/sysutil/gate.go` | Created | Heavy work semaphore with process scope |
| `internal/sysutil/gate_test.go` | Created | Serialization, idempotent release, cancellation, capacity limits |
| `internal/lockfile/lockfile.go` | Created | Consultative lock between processes |
| `internal/lockfile/lockfile_unix.go` | Created | `flock(2)` with `LOCK_EX\|LOCK_NB` |
| `internal/lockfile/lockfile_windows.go` | Created | `LockFileEx` with `FAIL_IMMEDIATELY` |
| `internal/lockfile/lockfile_test.go` | Created | Exclusion, release, creation of parent directory, signature by owner |
| `internal/daemon/syncmodule.go` | Modified | Takes a slot per batch and exits early when there is no work |
| `internal/daemon/syncmodule_gate_test.go` | Created | Empty batch does not queue; cancellation does not index or empty the slot |
| `internal/ast/embedder.go` | Modified | Embedding cycle and its rebuild run under gate |
| `cmd/graphit/commands/lifecycle.go` | Modified | Phase locks, flag `--debounce`, sync signature |
| `cmd/graphit/commands/sync_guard_test.go` | Created | Phase locks, debounce windows, illegible signature |
| `internal/git/hooks.go` | Modified | Hooks pass `--debounce 60s` |
| `internal/git/git_test.go` | Modified | The hook script needs a valid window |
| `docs/specs/daemon_module.md` | Modified | Documentates the gate between pipelines and its scope limit |
| `docs/specs/git_module.md` | Modified | Documents debounce hooks |

```

## Trade-offs & Decisions

- **Serializing instead of dividing the budget.** The alternative was to divide `CPUBudget()` by the number of active supervisors while keeping everyone progressing with fewer threads.
  It was discarded: dividing threads does not limit RAM (the buffer pool is open, not per thread), and working in batches does not become faster when split — it becomes slower, due to thrashing. Serializing limits CPU and memory using the same mechanism.
- **Capacity 1 as default, not precautionary.** `CPUBudget` already delivers everything that the pipeline can have on the machine; a second slot is by definition overloading. `GRAPHIT_HEAVY_SLOTS` serves as an escape for those who prefer to swap memory peaks for draining.
- **A slot per batch, not index indexer.** A batch that touches code and documentation could take and return the slot twice, giving more chances to other projects. Prefered not to go back in the queue mid-work while already having the slot in hand.
- **Failure to create a lock degrades it to "no lock".** An `.graphit` directory without write permissions is reason for logging and continuing with the old behavior, never stopping synchronization silently at the project's end.
- **Separate locks per phase.** See "Why two locks and not one", above.
- **Conscious duplication of wrappers of `flock`.** `internal/daemon/pidfile_unix.go` and `internal/daemonctl/flock_unix.go` already have equivalent wrappers. Unifying them in three places would require changing the daemon's PID management and spawn path — a larger change, in sensitive code without relation to the issue at hand. See Technical Debt.

Note: The inline references (e.g., `CPUBudget()`) are placeholders for actual lines or sections of text that should be replaced with the corresponding content when translating from Portuguese to English.

## Technical Debt

- [ ] **Nothing coordinates the daemon against an __INLINE_159__ CLI command line interface (CLI).** The gate is process-based, and file locks cover CLI-to-CLI interactions. A commit could still run a full reindex while the `SyncModule` of the daemon reacts to the same write events—another 2× collision that remains unresolved. Resolving this would require a process lock that the daemon respects, deciding what it does when it loses it (waiting for the watcher to catch up; skipping ahead leaves the index behind in the tree).
- [ ] **Three copies of the `flock` wrappers**—`internal/lockfile`, `internal/daemonctl`, and `internal/daemon/pidfile_*`. Consolidate them into `internal/lockfile` when someone is already working on managing the daemon’s PID handling.
- [x] **Section 2 and Section 4 describe polling of __INLINE_167__ that the `fswatch` replaces.** Previous debt from this task, found during documentation of the gate. Resolved in [[correct-defect-docs-watcher-fswatch]], where audit showed that the defect also affected `ast_module.md`, `memory_module.md`, the architecture diagram, and an orphaned comment in `internal/daemon/memorysyncmodule.go`. 
- [ ] **Temporary directories registered as permanent projects.** A directory under `/tmp` had been in `~/.graphit/global.lock.json` since 2026-08-06, still being supervised. Out of scope due to user decision.
- [ ] **Orphaned processes.** 13 running at the time of measurement, six during runtime, with uptime ranging from 3 to 5 days. Out of scope due to user decision.

## System Knowledge

- **The median of the `%CPU` is life, not instantaneous.** The 1047% of the daemon means ~20.6 hours of CPU accumulated in 1h58m — sustained saturation, not a peak. Useful for not confusing a process that burned the machine with one that was burning at that second.
- **Parking by idleness masks the problem instead of resolving it.** The window of 30 minutes makes it rare to have more than one active project, so the multiplication of budget only appears when two or three coincide. That's why it went unnoticed.
- **`ortInitOnce` does not share the ONNX session.** It initializes only the *environment* of the ONNX Runtime. Each `EmbeddingClient` opens its own session with `SetIntraOpNumThreads(boundedEmbedThreads())`, so N modules of embedding mean N pools of threads of the full budget size.
- **`flock` is due to open file description, not process.** Two calls to `TryAcquire` in the same path within the same process are excluded, which makes the lock testable without subprocesses.
- **The discovery of daemon projects is `hub.GlobalLockManager.ListActiveProjects` (`cmd/graphit/commands/daemon.go`), reading `~/.graphit/global.lock.json`. Any `graphit init` in a temporary directory produces indefinite supervision. - __INLINE_187___ is partially delayed — see Technical Debt. When reading that spec, trust what it says about discovery and schedulers, not what it says about detecting changes.
- **The median of the `%CPU` is life, not instantaneous.** The 1047% of the daemon means ~20.6 hours of CPU accumulated in 1h58m — sustained saturation, not a peak. Useful for not confusing a process that burned the machine with one that was burning at that second.
- **Parking by idleness masks the problem instead of resolving it.** The window of 30 minutes makes it rare to have more than one active project, so the multiplication of budget only appears when two or three coincide. That's why it went unnoticed.
- **The discovery of daemon projects is `hub.GlobalLockManager.ListActiveProjects` (`cmd/graphit/commands/daemon.go`), reading `~/.graphit/global.lock.json`. Any `graphit init` in a temporary directory produces indefinite supervision. - __INLINE_187___ is partially delayed — see Technical Debt. When reading that spec, trust what it says about discovery and schedulers, not what it says about detecting changes.

## Progress Log

### 2026-08-08

- Measure the machine: load 30.47/20 CPUs, with `some avg10=32.38` of CPU pressure, zeroed I/O pressure, and a swap space of 132 KB free from 2 GB.
- Traced the path from `sync --heavy` to `spawnBackgroundSync`, then to the three Git hooks—causing process duplication.
- Found the root cause in ___INLINE_191__: simultaneous supervisors without semaphores among them, with CPU budget applied by pipeline.
- Implemented gate (`internal/sysutil/gate.go`), lock (`internal/lockfile/`), phase locks, and debounce.
- `go build -tags fts5 ./...` and `go vet -tags fts5 ./...` are clean. Suites of `internal/daemon`, `internal/ast`, `internal/git`, `internal/lockfile`, `internal/sysutil`, and `cmd/graphit/commands` pass.
- Observed during the execution of tests: **another agent was editing this same repository in parallel** (the `git status` changed between two commands and a `zz_calls_test.go` transitional command broke a test compilation). This change has nothing to do with it, but relevant for those reproducing results.

Note: The code blocks and inline comments have been preserved as per your request.
