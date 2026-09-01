---
title: Resource budget across pipelines, sync lock and git hook debounce
status: done
created: 2026-08-08
updated: 2026-08-08
tags: [daemon, performance, recursos, sync, hooks, concorrência]
---

# Resource budget across pipelines, sync lock and git hook debounce

## Objective

The user's machine (20 CPUs, 61 GB of RAM, 2 GB of swap) locked up with load average 30.47
and swap 100% full. The measurement pointed at the Graphit daemon with a lifetime average of 1047% of
CPU — about 20.6 hours of CPU in 1h58m of process — and 10.7 GB of RSS, contending for the
machine with a `gce ast index` and three simultaneous `graphit-core sync --heavy`.

The user's hypothesis was that running `sync --heavy` in parallel makes no sense. The
investigation confirmed the hypothesis and found a bigger cause behind it: the indexer's
resource budget is applied **per pipeline**, not per process, and the daemon runs
N simultaneous pipelines in the same process.

The goal of this task is to close the three gaps: the budget across pipelines, the
duplication of sync processes, and the triggers that produce that duplication.

Out of scope by the user's explicit decision: refusing to register directories under
`/tmp` as a project, and reaping the orphaned `graphit-mcp` processes.

## Implementation Details

### Diagnosis (what the investigation established)

1. `sysutil.CPUBudget()` (`internal/sysutil/cpu.go:14`) returns `NumCPU - max(2, NumCPU/4)`
   — 15 on the machine measured. Three independent pools derive from it: the Go workers
   (`ast.SafeWorkers`, `internal/ast/throttle.go:56`), LadybugDB's native threads
   (`boundedDBThreads`, `internal/ast/resources.go:96`) and ONNX's intra-op
   (`boundedEmbedThreads`, `internal/ai/embedding_local.go:151`). `ortInitOnce` only
   initializes the ONNX Runtime *environment*; the session is per client, so each embedding
   module opens its own.
2. `internal/daemon/daemon.go:reconcileProjects` and `ProjectSupervisor.Start`
   (`internal/daemon/project.go:70`) had no semaphore at all. Every active project gets
   a supervisor with two modules running in parallel, all in the same process.
   `~/.graphit/daemon/daemon.log` records three supervisors alive at the same time between
   11:11:39 and 11:12:09 (this repository, the private corpus and a temporary directory) —
   exactly the window in which the user measured the lockup.
3. `boundedDBBufferPool` caps 1 GiB **per database**, and the incremental rebuild keeps two
   open (production + the copy from the copy+swap). Three projects × two databases = up to 6 GiB of
   buffer pool alone.
4. `spawnBackgroundSync` (`cmd/graphit/commands/lifecycle.go`) ran
   `sync --heavy` with no lock and without checking whether one was already running.
5. The trigger for the duplication was the git hooks: `internal/git/hooks.go` installed
   `(graphit sync </dev/null >/dev/null 2>&1 &)` in `post-commit`, `pre-push` **and**
   `post-merge`. Each of them runs the whole of Phase 1 synchronously (a full reindex of the AST
   + knowledge + memory cycles) and only then spawns the `--heavy`.

### Changes

**`internal/sysutil/gate.go` (new)** — a heavy-work semaphore, process-scoped.
`AcquireHeavy(ctx)` returns the release function (idempotent, safe for `defer` alongside
an explicit call). `HeavySlots()` resolves the capacity: 1 by default,
`GRAPHIT_HEAVY_SLOTS` overrides it, always capped by `CPUBudget()`. A cancelled `ctx`
returns `ctx.Err()` with a nil release and the caller **must** skip the work.
`resetHeavyGate()` exists only for the tests.

**`internal/lockfile/` (new)** — an advisory file lock, across processes.
`TryAcquire(path)` returns `ErrLocked` when another process holds the lock; `Release()` is
idempotent and safe on a nil `*Lock`. The lock lives on the *open file description*, so a
process that dies releases it by itself — there is no stale lock to clean up and no PID to
validate. The pid and the timestamp written into the file are for whoever opens it during a debug.

**`internal/daemon/syncmodule.go`** — `handleBatch` went from two independent
`switch`/`if` to an explicit decision (`astWork`, `knowledgeWork`) followed by a single
acquisition of the gate for the whole batch. A batch with no work returns **before** touching the
gate, so an idle supervisor never queues behind a busy one. A wait above
1s is logged.

**`internal/ast/embedder.go`** — the repeated body of the cycle (the initial one and the per-tick one) became a
`cycle(label, reload)` closure that takes the gate for the duration of the cycle and of the
`triggerEmbeddingRebuild` that follows. The only side effect in the log: the error message
of the periodic cycle went from `cycle error` to `embedding cycle error`.

**`cmd/graphit/commands/lifecycle.go`** — the `sync` command gained:
- `acquireSyncLock(wd, name)`, with three outcomes: lock free (proceeds with it), lock
  taken (skips — the other one is already doing this), or the lock could not be created (proceeds
  **without** a lock, degrading to the old behavior and recording it in `sync.log`).
- Separate locks per phase: `.graphit/sync.lock` for Phase 1 and
  `.graphit/sync-heavy.lock` for Phase 2.
- A `--debounce <duração>` flag, with `syncedWithin` reading `.graphit/sync.stamp` and
  `stampSync` writing the stamp at the end of Phase 1.

**`internal/git/hooks.go`** — the hooks pass `--debounce 60s`, via the `hookDebounce`
constant.

### Why two locks and not one

`graphit sync` spawns `sync --heavy` and returns right afterwards. With a single lock, the child
would race against its own parent's release and lose by chance. Two phases, two locks,
each idempotent with respect to the other.

### Why the stamp is written after Phase 1

Phase 2 is fire-and-forget per project. Waiting for it would leave the debounce window
open for the entire duration of a round of embeddings, and Phase 1 is precisely the part that
a git hook exists to trigger.

## Use Cases

### UC-01: Two active projects reindex at the same time in the daemon
- **Actor**: daemon (the `ProjectSupervisor` of each active project)
- **Preconditions**: daemon running; two or more projects inside the activity window;
  modified files in more than one of them.
- **Main Flow**:
  1. Each `SyncModule`'s `fswatch` delivers a batch to its `handleBatch`.
  2. `handleBatch` classifies the batch and decides `astWork` / `knowledgeWork`.
  3. If there is work, it calls `sysutil.AcquireHeavy(ctx)`.
  4. The first to arrive takes the single slot; the others wait.
  5. The holder runs `reindexAST` and/or `reindexKnowledge` and releases the slot in the `defer`.
  6. The next in the queue proceeds.
- **Alternative Flows**:
  - `GRAPHIT_HEAVY_SLOTS=N` raises the capacity to `min(N, CPUBudget())`.
  - A batch with `Rescan` is work for both indexers, regardless of the
    paths it names.
- **Error Scenarios**:
  - `ctx` cancelled during the wait (a supervisor being parked or the daemon shutting down):
    `AcquireHeavy` returns an error, `handleBatch` returns without indexing, and the slot that was never
    obtained is not released. The next batch redoes the work.
- **Postconditions**: at most `HeavySlots()` heavy pipelines running in the process.
- **Affected Files**: `internal/sysutil/gate.go`, `internal/daemon/syncmodule.go`

### UC-02: A batch with no work does not join the queue
- **Actor**: daemon (`SyncModule`)
- **Preconditions**: a batch whose paths are all ignored, or of extensions with no parser.
- **Main Flow**:
  1. `handleBatch` classifies and gets `astWork == false` and `knowledgeWork == false`.
  2. It returns immediately, without calling `AcquireHeavy`.
- **Error Scenarios**: none — the path has no possible failure.
- **Postconditions**: the slot stays available for whoever has something to do.
- **Affected Files**: `internal/daemon/syncmodule.go`

### UC-03: An embedding cycle competes with a reindex
- **Actor**: daemon (`EmbeddingModule` via `ast.RunEmbeddingLoop`)
- **Preconditions**: embedding loop active; entities pending embedding.
- **Main Flow**:
  1. The tick fires `cycle("embedding cycle", true)`.
  2. The closure takes the heavy slot.
  3. It reloads the parse cache, runs `embedder.RunCycle` and, if the cycle produced anything,
     `triggerEmbeddingRebuild`.
  4. It releases the slot.
- **Alternative Flows**:
  - The initial cycle runs as `cycle("initial cycle", false)`, without reloading the cache that
    was just opened.
- **Error Scenarios**:
  - `ctx` cancelled during the wait: the cycle is skipped; the next tick tries again.
  - `RunCycle` fails: recorded as `<label> error`; the slot is released by the `defer`.
- **Postconditions**: the embedding cycle never runs alongside a reindex from the same
  process.
- **Affected Files**: `internal/ast/embedder.go`

### UC-04: Two `sync --heavy` are fired almost at the same time
- **Actor**: CLI (`graphit sync --heavy`), typically spawned by `sync` or `init`
- **Preconditions**: a `sync --heavy` already running over the same project.
- **Main Flow**:
  1. The second process calls `acquireSyncLock(wd, "sync-heavy.lock")`.
  2. `lockfile.TryAcquire` returns `ErrLocked`.
  3. The command returns 0 silently, without running Phase 2.
- **Error Scenarios**:
  - The lock could not be created (a directory with no permission): the error goes to `sync.log` and the
    command **proceeds without a lock** — it never stops syncing because of that.
- **Postconditions**: a single Phase 2 per project at a time.
- **Affected Files**: `cmd/graphit/commands/lifecycle.go`, `internal/lockfile/lockfile.go`

### UC-05: A commit followed by a push fires the hooks in sequence
- **Actor**: git (`post-commit`, then `pre-push`)
- **Preconditions**: hooks installed; the tree changed once.
- **Main Flow**:
  1. `post-commit` runs `graphit sync --debounce 60s`.
  2. There is no recent stamp, the lock is free: Phase 1 runs and writes
     `.graphit/sync.stamp`.
  3. Seconds later, `pre-push` runs the same command.
  4. `syncedWithin` reads the stamp, sees it is less than 60s old, and the command returns 0 without
     doing anything.
- **Alternative Flows**:
  - The two hooks overlap instead of following each other: the second loses the
    `.graphit/sync.lock` and exits silently — the debounce and the lock cover different cases.
  - An interactive `graphit sync` (without `--debounce`): it is never skipped by the stamp, and on losing
    the lock it prints "Another sync is already running — skipping" instead of exiting quietly.
- **Error Scenarios**:
  - `.graphit/sync.stamp` missing, unreadable or with invalid content: it reads as "I don't know" and
    the sync runs. The debounce only skips what it can prove redundant.
- **Postconditions**: one Phase 1 per 60s window per project.
- **Affected Files**: `internal/git/hooks.go`, `cmd/graphit/commands/lifecycle.go`

## Test Cases & Acceptance Criteria

### Feature: Cross-pipeline resource gate
Ref: UC-01, UC-02, UC-03

#### Scenario: Eight concurrent heavy jobs never overlap
```gherkin
Given GRAPHIT_HEAVY_SLOTS is set to "1"
  And the gate was rebuilt from the environment
When 8 goroutines call AcquireHeavy and hold the slot for 1 millisecond each
Then the maximum concurrency observed is 1
```

#### Scenario: The slot goes back to the queue after the release
```gherkin
Given GRAPHIT_HEAVY_SLOTS is set to "1"
  And a goroutine obtained the single slot
When it calls the release function twice
Then a new call to AcquireHeavy obtains the slot
  And the duplicate release did not free a second, nonexistent slot
```

#### Scenario: A cancelled wait abandons the queue
```gherkin
Given GRAPHIT_HEAVY_SLOTS is set to "1"
  And the single slot is taken
When AcquireHeavy is called with an already cancelled context
Then the error returned is context.Canceled
  And the release function returned is nil
```

#### Scenario Outline: The capacity is capped by the CPU budget
```gherkin
Given GRAPHIT_HEAVY_SLOTS is set to "<override>"
When HeavySlots is queried
Then the result is "<expected>"

Examples:
  | override | expected                |
  | <empty>  | 1                       |
  | 3        | min(3, CPUBudget())     |
  | 100000   | CPUBudget()             |
```

#### Scenario: An empty batch does not sit in the queue
```gherkin
Given the single heavy slot is taken by another project
When handleBatch receives a batch with no indexable paths
Then it returns in less than 5 seconds
  And it never called AcquireHeavy
```

#### Scenario: A supervisor being parked does not index on the way out
```gherkin
Given a SyncModule pointed at a project with no index
When handleBatch is called with a Rescan batch and a cancelled context
Then no AST database is opened in the project's directory
  And the heavy slot stays available for the next caller
```

### Feature: Advisory file lock
Ref: UC-04

#### Scenario: The second holder is refused
```gherkin
Given a lock was obtained on .graphit/sync.lock
When TryAcquire is called on the same path by another open file description
Then the error returned is ErrLocked
  And no Lock is returned along with the error
```

#### Scenario: The lock is released for the next one
```gherkin
Given a lock was obtained and then released
When TryAcquire is called on the same path
Then the lock is granted
```

#### Scenario: The two phases do not contend for the same lock
```gherkin
Given Phase 1 holds .graphit/sync.lock
When Phase 2 asks for .graphit/sync-heavy.lock
Then the lock is granted
```

#### Scenario: Release is idempotent
```gherkin
Given a lock was obtained
When Release is called twice
  And Release is called on a nil *Lock
Then none of the calls panics
```

#### Scenario: The file names who holds it
```gherkin
Given a lock was obtained
When the lock file is read
Then the first line is the pid of the holding process
  And the second line is a timestamp
```

### Feature: Hook debounce
Ref: UC-05

#### Scenario: A sync that just finished is skipped
```gherkin
Given stampSync has just written .graphit/sync.stamp
When syncedWithin is queried with a window of 1 minute
Then it answers that the sync should be skipped
```

#### Scenario Outline: Windows that cannot skip anything
```gherkin
Given "<state>" of the stamp
When syncedWithin is queried with the window "<window>"
Then it answers that the sync should run

Examples:
  | state                         | window      |
  | no stamp on disk              | 1 minute    |
  | stamp with invalid text       | 1 hour      |
  | stamp just written            | 0           |
  | stamp just written            | -1 minute   |
  | stamp just written            | 1 nanosecond |
```

#### Scenario: The hook script carries the window
```gherkin
Given hookScript is generated for any one of the three hooks
When the script is inspected
Then it contains "--debounce" followed by hookDebounce
  And hookDebounce is a duration the sync command can parse
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/sysutil/gate.go` | Created | Process-scoped heavy-work semaphore |
| `internal/sysutil/gate_test.go` | Created | Serialization, release idempotency, cancellation, capacity limits |
| `internal/lockfile/lockfile.go` | Created | Advisory lock across processes |
| `internal/lockfile/lockfile_unix.go` | Created | `flock(2)` with `LOCK_EX\|LOCK_NB` |
| `internal/lockfile/lockfile_windows.go` | Created | `LockFileEx` with `FAIL_IMMEDIATELY` |
| `internal/lockfile/lockfile_test.go` | Created | Exclusion, release, parent directory creation, holder stamp |
| `internal/daemon/syncmodule.go` | Modified | `handleBatch` takes one slot per batch and exits early when there is no work |
| `internal/daemon/syncmodule_gate_test.go` | Created | An empty batch does not queue; cancellation neither indexes nor leaks the slot |
| `internal/ast/embedder.go` | Modified | The embedding cycle and its rebuild run under the gate |
| `cmd/graphit/commands/lifecycle.go` | Modified | Per-phase locks, `--debounce` flag, sync stamp |
| `cmd/graphit/commands/sync_guard_test.go` | Created | Per-phase locks, debounce windows, unreadable stamp |
| `internal/git/hooks.go` | Modified | The hooks pass `--debounce 60s` |
| `internal/git/git_test.go` | Modified | The hook script has to carry a valid window |
| `docs/specs/daemon_module.md` | Modified | Documents the cross-pipeline gate and its scope limit |
| `docs/specs/git_module.md` | Modified | Documents the hook debounce |

## Trade-offs & Decisions

- **Serialize instead of splitting the budget.** The alternative was to divide `CPUBudget()`
  by the number of active supervisors, keeping them all progressing with fewer threads each.
  It was discarded: splitting threads does not cap RAM (the buffer pool is per open database, not
  per thread), and batch work does not get faster when sliced — it gets slower,
  from thrash. Serializing caps CPU and memory with the same mechanism.
- **Capacity 1 as the default, not as a precaution.** `CPUBudget` already hands a single
  pipeline everything it can have from the machine; a second slot is, by definition,
  overload. `GRAPHIT_HEAVY_SLOTS` remains as an escape hatch for anyone who prefers to trade peak
  memory for throughput.
- **One slot per batch, not per indexer.** A batch that touches code and documentation
  could take and give back the slot twice, giving the other projects more of a turn. We preferred
  not to go back to the end of the queue in the middle of work that already has the slot in hand.
- **Failing to create the lock degrades to "no lock".** A `.graphit` directory without write
  permission is a reason to record it and proceed with the old behavior, never to stop
  syncing the project silently.
- **Separate locks per phase.** See "Why two locks and not one", above.
- **Deliberate duplication of the `flock` wrappers.** `internal/daemon/pidfile_unix.go` and
  `internal/daemonctl/flock_unix.go` already have equivalent wrappers. Unifying them across the three
  places would require touching the daemon's PID handling and the spawn path — a bigger change,
  in sensitive code, unrelated to the problem at hand. See Technical Debt.

## Technical Debt

- [ ] **Nothing coordinates the daemon against a CLI `graphit sync`.** The gate is per process and
  the file locks cover CLI-against-CLI. A commit can still make the hook run a
  full reindex while the daemon's `SyncModule` reacts to the same write events —
  a 2× collision that is left over. Solving it would require a cross-process lock that the daemon
  honors, and deciding what it does when it loses it (waiting stalls the watcher; skipping leaves the
  index behind the tree).
- [ ] **Three copies of the `flock` wrappers** — `internal/lockfile`, `internal/daemonctl` and
  `internal/daemon/pidfile_*`. Consolidate into `internal/lockfile` when somebody is already
  touching the daemon's PID handling.
- [x] **`docs/specs/daemon_module.md` §2 and §4 describe the `git status` polling that
  `fswatch` replaced.** Debt predating this task, found while documenting the gate.
  Resolved in [[fix-docs-watcher-fswatch-lag]], where the audit showed that the
  lag also reached `ast_module.md`, `memory_module.md`, the architecture
  diagram and an orphaned comment in `internal/daemon/memorysyncmodule.go`.
- [ ] **Temporary directories registered as a permanent project.** A directory under
  `/tmp` had been in `~/.graphit/global.lock.json` since 2026-08-06, still supervised.
  Out of scope by the user's decision.
- [ ] **Orphaned `graphit-mcp` processes.** 13 alive at measurement time, six from the `v0.1.27` runtime,
  with 3 to 5 days of uptime. Out of scope by the user's decision.

## System Knowledge

- **`ps`'s `%CPU` is a lifetime average, not an instantaneous value.** The daemon's 1047% means ~20.6
  hours of CPU accumulated in 1h58m — sustained saturation, not a spike. Useful for not
  confusing a process that burned the machine with one that was burning it that second.
- **Parking by idleness masks the problem instead of solving it.** The 30-minute
  window means there is rarely more than one active project, so the multiplication of the
  budget only shows up when two or three coincide. That is why it went unnoticed.
- **`ortInitOnce` does not share the ONNX session.** It only initializes the ONNX Runtime
  *environment*. Each `EmbeddingClient` opens its own session with
  `SetIntraOpNumThreads(boundedEmbedThreads())`, so N embedding modules mean N
  thread pools the size of the whole budget.
- **`flock` is per open file description, not per process.** Two calls to `TryAcquire`
  on the same path within the same process exclude each other, which is what makes the lock testable
  without subprocesses.
- **The daemon's project discovery is `hub.GlobalLockManager.ListActiveProjects`**
  (`cmd/graphit/commands/daemon.go`), reading `~/.graphit/global.lock.json`. Any
  `graphit init` in a temporary directory produces supervision for an indefinite time.
- **`docs/specs/daemon_module.md` is partially out of date** — see Technical Debt. When
  reading that spec, trust what it says about discovery and schedulers, not what it says
  about change detection.

## Progress Log

### 2026-08-08
- Measured the machine: load 30.47/20 CPUs, `some avg10=32.38` of CPU pressure, I/O
  pressure at zero, swap with 132 KB free out of 2 GB.
- Traced the path of `sync --heavy` down to `spawnBackgroundSync`, and from there to the three git
  hooks — the cause of the process duplication.
- Found the bigger cause in `daemon.log`: three simultaneous supervisors, with no semaphore
  between them, with the CPU budget applied per pipeline.
- Implemented the gate (`internal/sysutil/gate.go`), the lock (`internal/lockfile/`), the
  per-phase locks and the debounce.
- `go build -tags fts5 ./...` and `go vet -tags fts5 ./...` clean. The
  `internal/daemon`, `internal/ast`, `internal/git`, `internal/lockfile`,
  `internal/sysutil` and `cmd/graphit/commands` suites passing.
- Observed while running the tests: **another agent session was editing this
  same repository in parallel** (the `git status` changed between two commands and a
  transient `zz_calls_test.go` broke a test compilation). Nothing to do with this
  change, but relevant for anyone reproducing the results.
