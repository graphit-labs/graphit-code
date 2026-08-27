---
title: Two consecutive syncs broke the Hub pull, and one corrupt row broke every entity lookup
status: done
created: 2026-08-10
updated: 2026-08-10
tags: [hub, git, sync, concurrency, ast, ladybug, robustness]
---

# Two consecutive syncs broke the Hub pull, and one corrupt row broke every entity lookup

## Objective

Two unrelated failures, both surfaced in the same session, both diagnosed and fixed here.

**1. `graphit sync` run twice in a row.** The first run succeeded; the second failed with

```
✗ Hub sync: syncing hub repository: exit status 128: From bitbucket.org:…;
  * branch main -> FETCH_HEAD; fatal: Cannot rebase onto multiple branches. (3.3s)
```

**2. `ast_source` with an `entity` parameter.** Resolving `runSyncPhase1` in
`cmd/graphit/commands/lifecycle.go` failed with

```
resolve entity "runSyncPhase1": ladybug query: Runtime exception: Failed calling LOWER: Invalid UTF-8.
```

The goal was the root cause of each, not a workaround.

## Implementation Details

### 1. Hub sync — a FETCH_HEAD race between a foreground sync and a background one

`git pull --rebase` chooses its upstream by reading `FETCH_HEAD` and dies if it finds more than
one for-merge entry. `git fetch` **truncates** `FETCH_HEAD` when it starts and **appends** to it
when it finishes; those are two steps, not one. Two fetches overlapping in the same checkout
can therefore leave two for-merge lines, and whichever `pull` reads them aborts with
`Cannot rebase onto multiple branches`.

The overlap is built into the sync flow, which is why running sync twice in a row triggers it:

1. `newSyncCmd` → `runSyncPhase1` (`cmd/graphit/commands/lifecycle.go:1021`) pulls the Hub in
   the **foreground**, under `sync.lock`.
2. It then calls `spawnBackgroundSync` (`lifecycle.go:752`), and that process runs
   `runSyncHeavyTasks` under a **different** lock, `sync-heavy.lock`. Its
   `hub.NewRegistryManager` → `EnsureCloned` → `Sync` pulls the Hub **again**
   (`lifecycle.go:879`).
3. A second `graphit sync` takes `sync.lock` — free, the first foreground half already
   finished — and pulls the Hub while step 2 is still talking to Bitbucket over SSH.

Neither lock helps: they guard different phases of one project, and `~/.graphit/hub` is a
single checkout shared by every project on the machine.

Two changes, because either alone leaves a hole:

- **`Sync()` no longer reads `FETCH_HEAD`.** It fetches into `refs/remotes/origin/<branch>` and
  rebases onto that ref. Remote-tracking refs are updated under git's own ref lock, so an
  overlapping fetch is harmless.
- **A cross-process lock** at `~/.graphit/hub.git.lock` wraps `Sync()` and `CommitAndPush()`,
  so two processes no longer run `rebase`/`stash`/`commit` over one index either. Added
  `lockfile.Acquire(path, wait)` for it: `TryAcquire` is wrong here because the second pull
  still has to happen, it just must not happen *at the same time*.

An in-process `sync.Mutex` sits in front of the flock. flock is held per open file
description, so two goroutines in one process would both acquire it and neither would wait.
`syncLocked()` exists so `CommitAndPush` can hold the lock across commit → sync → push without
`Sync()` trying to take it a second time and blocking on itself.

**A third finding, which is why this fired at all.** `Sync()`'s fast path compared the remote
tip to `HEAD` and fetched whenever they differed. Unequal tips do not mean *behind*: the Hub's
bootstrap commit (`bootstrapEmptyRepo`) is committed locally and rebased on top of the remote's
initial commit, and on this machine it was never pushed — so local `main` sat permanently one
commit **ahead** of `origin/main` and every single sync did a full ls-remote + fetch + no-op
rebase, holding the race window open forever. `Sync()` now also returns early when the remote
tip is already an ancestor of `HEAD`. On the reporting machine this removes the fetch entirely.

### 2. `LOWER: Invalid UTF-8` — one corrupt row, amplified by an unlabelled `toLower`

The graph held **four rows of garbage bytes**, all four `Comment.name` values from
`internal/ast/helper.go`. Everything else in the project — 810 files, 56850 names — was fine.

`resolveEntity` (`internal/ast/source_service.go`) asked for
`MATCH (e) WHERE toLower(e.name) = toLower($name)`. Unlabelled, that lowercases the name of
**every node in the project**, so four bad rows in one file failed every `ast_source
entity:` call for every entity in the repository. That amplification is the part that was
ours to fix, and it is what changed:

- The lookup now runs **exact match first** (`e.name = $name`, no `LOWER` anywhere), and only
  falls back to the case-insensitive form when exact finds nothing. Identifiers match exactly
  nearly always, so the blast radius of a bad row shrinks to the lookups that genuinely need
  folding.
- If the fold *does* run and fails, the error now says what happened and what fixes it
  (reindex the affected file) instead of surfacing a bare Ladybug exception.

The corruption itself is **not ours** and is not fixed here. It is the known open issue in
`docs/upstream/liblbug-string-corruption.md`, which this session advanced with new evidence —
see that file. The decisive new data point: the **stored byte length was wrong too**, by a
different delta on every row (68→72, 71→65, 63→69, 66→88). A Go string carries its own length,
so nothing on the caller's side can produce that; what was stored was a different
(offset, length) pair, not a mangled copy of the right value.

The data was repaired by reindexing the one file. Note that a normal `graphit sync` never would
have: the content hash is unchanged, so the shard cache reports the file up to date
(`✓ AST: 810 files up to date`) and the corrupt rows survive indefinitely. Only
`ast_index` with `reindex: true` rewrites them.

## Use Cases

### UC-01: A project syncs while another sync's background half is still running
- **Actor**: developer running `graphit sync`, twice in quick succession
- **Preconditions**: a Hub remote is configured; the first sync's `spawnBackgroundSync` child
  is still reconciling artifacts
- **Main Flow**:
  1. Second `graphit sync` reaches `runSyncPhase1` → `hub.GitStore.Sync()`.
  2. `Sync()` acquires `~/.graphit/hub.git.lock`, waiting up to 90 s for the background
     process to finish its own Hub git work.
  3. `syncLocked()` runs `remoteCommit(branch)` (ls-remote, cached 30 s).
  4. If the remote tip equals `HEAD`, or is already an ancestor of `HEAD`, it returns — no
     network fetch.
  5. Otherwise it fetches `+refs/heads/<branch>:refs/remotes/origin/<branch>` and rebases onto
     `refs/remotes/origin/<branch>`.
  6. The lock is released; the other process proceeds.
- **Alternative Flows**:
  - No remote configured → `Sync()` returns immediately, no lock taken.
  - Empty repository (`isEmptyRepo`) → returns after `syncRemote()`.
  - Lock not obtained within 90 s → logs a warning and proceeds anyway; safe because the git
    sequence no longer depends on `FETCH_HEAD`.
- **Error Scenarios**:
  - Rebase leaves conflict state (`isRebasing`) → `rebase --abort`, and the caller gets
    "conflict detected during hub sync … please retry".
  - Fetch fails (network, auth) → wrapped as `syncing hub repository: %w`.
- **Postconditions**: local `main` contains the remote tip; no rebase state left behind; the
  lock is released however the function exits.
- **Affected Files**: `internal/hub/git_store.go`, `internal/lockfile/lockfile.go`

### UC-02: The Hub branch carries unpushed local commits
- **Actor**: any caller of `GitStore.Sync()`
- **Preconditions**: local `main` is a descendant of `origin/main` — the bootstrap commit was
  never pushed
- **Main Flow**:
  1. `remoteCommit(branch)` returns the remote tip; it differs from `HeadCommit()`.
  2. `hasCommit(remoteTip)` confirms the object is present locally.
  3. `isAncestor(remoteTip, "HEAD")` is true, so there is nothing to pull and `Sync()` returns.
- **Alternative Flows**:
  - Shallow repository without the remote tip object → `hasCommit` is false, so the check is
    skipped and the fetch runs.
- **Error Scenarios**:
  - `merge-base` cannot decide → non-zero exit → treated as "not an ancestor" → fetch runs.
    Failing towards fetching is the safe direction.
- **Postconditions**: no network fetch and no rebase for a branch that is merely ahead.
- **Affected Files**: `internal/hub/git_store.go`

### UC-03: An entity is resolved for `ast_source` while the graph holds a value `LOWER` rejects
- **Actor**: agent or user calling `ast_source` with `entity`
- **Preconditions**: at least one node in the project has a name the storage layer returns as
  invalid UTF-8
- **Main Flow**:
  1. `resolveEntity` runs the exact-match query (`e.name = $name AND e.path = $path`).
  2. It succeeds without evaluating `LOWER` on any row, and the entity resolves.
- **Alternative Flows**:
  - Exact match finds nothing (caller used different casing) → the case-insensitive query runs
    as a fallback.
- **Error Scenarios**:
  - The fallback itself fails on the corrupt row → the error states both that the entity was
    not found under its own name and that the case-insensitive retry could not run, and names
    reindexing as the repair.
  - Entity found but without a line range → "has no line range information", unchanged.
- **Postconditions**: a corrupt row can only affect lookups that need case folding, never
  exact-name lookups.
- **Affected Files**: `internal/ast/source_service.go`

## Test Cases & Acceptance Criteria

### Feature: Hub sync survives an overlapping fetch
Ref: UC-01

#### Scenario: two for-merge entries in FETCH_HEAD kill `git pull --rebase`
```gherkin
Given a repository whose main carries one local commit on top of its remote tip
When git pull --rebase --autostash is asked for two refspecs, so FETCH_HEAD gets two for-merge entries
Then git aborts with "Cannot rebase onto multiple branches"
```

#### Scenario: fetching into the tracking ref and rebasing onto it ignores FETCH_HEAD entirely
```gherkin
Given a repository whose main carries one local commit on top of its remote tip
  And FETCH_HEAD has been overwritten with two for-merge entries
When the remote branch is fetched into refs/remotes/origin/main
  And the branch is rebased onto refs/remotes/origin/main
Then the rebase succeeds
  And the history still contains both the local commit and the remote tip
```

Implemented as `TestSyncRebasesOnTrackingRefNotFetchHead` in
`internal/hub/git_store_sync_test.go`.

### Feature: A branch that is ahead is not treated as behind
Ref: UC-02

#### Scenario: the remote tip is an ancestor of HEAD
```gherkin
Given a repository whose main carries one local commit on top of the fetched remote tip
When the remote tip is compared against HEAD
Then hasCommit reports the remote tip as present
  And isAncestor(remote tip, HEAD) is true
  And isAncestor(HEAD, remote tip) is false
```

#### Scenario: an object that is not in the repository
```gherkin
Given a repository that has never seen the all-zero object id
When hasCommit is asked about it
Then it reports the object as absent
```

Implemented as `TestGitStoreAncestorFastPath` in `internal/hub/git_store_sync_test.go`.

### Feature: A bounded wait for a cross-process lock
Ref: UC-01

#### Scenario: the holder releases and the waiter proceeds
```gherkin
Given a lock file already held by this process
When the holder releases it after 150ms
  And another Acquire is waiting with a 5s budget
Then Acquire returns a lock
  And it waited at least 100ms rather than failing immediately
```

#### Scenario: the holder never releases
```gherkin
Given a lock file held for the duration of the test
When Acquire is called with a 120ms budget
Then it returns ErrLocked
  And it did not return before the budget elapsed
```

Implemented as `TestAcquireWaitsForTheHolder` and `TestAcquireGivesUpAtTheDeadline` in
`internal/lockfile/lockfile_test.go`.

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/git_store.go` | Modified | `Sync()` fetches into the tracking ref and rebases onto it instead of `pull --rebase`; new `syncLocked`, `lockGit`, `gitLockPath`, `hasCommit`, `isAncestor`, `trackingRefFor`, `fetchTrackingRef`, `rebaseOntoTrackingRef`, `invalidateCachedRemoteCommit`; `CommitAndPush` holds the lock across commit → sync → push and syncs with `freshRemoteFirst`; both `MemoryWorktree` paths off `pull --rebase` |
| `internal/memory/memory_git_store.go` | Modified | `MemoryWorktree.Pull` and `pushBranchInBackground` off `pull --rebase` onto a new `rebaseOntoTrackingRef`/`trackingRefFor` |
| `internal/memory/memory_git_store_rebase_test.go` | Created | Covers the memory worktree replacement against a poisoned `FETCH_HEAD` and independently rooted branches |
| `internal/lockfile/lockfile.go` | Modified | Added `Acquire(path, wait)` — a bounded blocking acquire, for work that must happen but not concurrently |
| `internal/lockfile/lockfile_test.go` | Modified | Coverage for the wait path and the deadline |
| `internal/hub/git_store_sync_test.go` | Created | Regression tests for the FETCH_HEAD failure mode and the ancestor fast path |
| `internal/ast/source_service.go` | Modified | `resolveEntity` tries exact match before case-insensitive, so one unlowercasable row cannot fail every lookup; actionable error when the fold fails |
| `docs/specs/hub_collaboration.md` | Modified | Documented that the Hub checkout is global and how its git access is serialised |
| `docs/upstream/liblbug-string-corruption.md` | Modified | Second field occurrence, the wrong-stored-length evidence, and the elimination of the parse cache |

## Trade-offs & Decisions

- **Two mechanisms rather than one.** The lock alone would have fixed the reported failure, but
  a lock that cannot be taken (timeout, a stale holder, a platform where flock misbehaves) would
  put the `FETCH_HEAD` hazard straight back. Removing the dependency on `FETCH_HEAD` makes the
  overlap survivable rather than merely unlikely, so on lock timeout the code proceeds instead
  of skipping a pull that still needs to happen.
- **`Acquire` polls at 50 ms** rather than using a timed flock. There is no portable timed
  variant across the platforms this builds for, and the alternative — a platform-specific
  primitive per OS — is not worth it for a wait measured in seconds.
- **`hasCommit` before `isAncestor`.** Asking about ancestry in a shallow repository that lacks
  the remote tip answers about nothing. Both checks failing towards *fetch* keeps the change
  conservative: the worst outcome is the old behaviour.
- **Exact-match-first over sanitising on write.** Coercing strings with
  `strings.ToValidUTF8` before writing was considered and rejected as the primary fix: the
  evidence says the bytes are already valid when handed over and are corrupted afterwards, so
  sanitising would have added a pass over every string while fixing nothing. Exact-match-first
  is also faster on the common path.
- **The corruption is documented, not worked around.** No retry loop, no automatic reindex on
  a `LOWER` failure. Both would hide a silent-data-loss bug that needs to stay visible — and,
  as the transient mode below shows, reindexing would be the wrong reflex in half the cases.
- **`CommitAndPush` opts out of the remote-commit cache** (`freshRemoteFirst`) while `Sync()`
  keeps it (`cachedRemoteOK`). Both of `syncLocked`'s fast paths decide *not to fetch*; deciding
  that from a tip up to 30 s stale is free for a read-only sync and wrong before a push, where
  it would skip a needed rebase and get the push rejected as non-fast-forward. Two named
  constants rather than a bare bool, because `syncLocked(true)` at the call site says nothing.
- **The shared rebase helper is duplicated per package rather than extracted.** `internal/hub`
  and `internal/memory` each have their own `rebaseOntoTrackingRef`; they run git through
  different accessors (`gitExec()` vs `gitmod.Default()`) and a new shared package for six lines
  would buy less than it costs.

## Technical Debt

- [x] **`internal/memory/memory_git_store.go:352` and `:418` used `git pull --rebase`** —
  resolved. Both now call a package-local `rebaseOntoTrackingRef(wtDir, branch, strategy...)`
  that fetches `+refs/heads/<b>:refs/remotes/origin/<b>` and rebases onto that ref, preserving
  each call site's options (`--autostash -X ours` for `Pull`, `-X ours` for the background
  push). `--allow-unrelated-histories` was dropped: `git pull` forwards it to merge only and
  ignores it under `--rebase`, and a rebase replays commits across unrelated roots without it —
  verified against git 2.53.0 and pinned by
  `TestRebaseOntoTrackingRefIgnoresFetchHead`, which builds an independently rooted local branch
  on purpose.
- [x] **`internal/hub/git_store.go` `MemoryWorktree.Pull` and `CommitAndPush` used
  `git pull --rebase`** — resolved. Both call the hub package's `rebaseOntoTrackingRef`, which
  `Sync()` now also shares via `fetchTrackingRef`/`trackingRefFor`, so the three paths cannot
  drift apart.
- [x] **`SyncEvents()` is not under the hub git lock** — verified as not needing it, rather than
  changed. Its git commands are `hash-object -w`, `mktree`, `commit-tree`, `update-ref` on
  `refs/events/*`, `push`, and `update-ref -d`: object-database writes and a ref outside
  `refs/heads`, touching neither the index nor `HEAD`. Objects and ref updates are already
  concurrency-safe in git. Revisit only if it grows an index-touching step.
- [x] **The Hub bootstrap commit is never pushed** — verified benign, not changed.
  `bootstrapEmptyRepo` pushes only when a remote exists at init time, so a hub configured
  afterwards keeps one unpushed local commit. The ancestor fast path absorbs it, and
  `CommitAndPush` pushes `HEAD`, so the first real publish carries it. Inventing a push inside
  `Sync()` was rejected: a read-named method that writes to a shared remote is a worse defect
  than the cosmetic one it would fix.
- [ ] **Upstream: LadybugDB string corruption** — stays open, tracked in
  `docs/upstream/liblbug-string-corruption.md`. Not actionable here beyond the amplification
  fix; a corrupt row is still silent until something calls `LOWER` on it.
- [ ] **The `resolveEntity` fix is not live until the binary is replaced** — the running MCP
  server holds the previous build, so `ast_source entity:` kept hitting the old wide-`toLower`
  path during this session. Needs `make install` (and an MCP server restart) to take effect.

## System Knowledge

- **`~/.graphit/hub` is one checkout for the whole machine.** Per-project locks
  (`.graphit/sync.lock`, `sync-heavy.lock`) cannot serialise access to it — anything guarding
  Hub git work has to live in the global directory.
- **`graphit sync` pulls the Hub twice per invocation**, once in the foreground
  (`runSyncPhase1`) and once in the background process it spawns (`runSyncHeavyTasks` →
  `NewRegistryManager` → `EnsureCloned` → `Sync`). The comment at `git_store.go:21-24` notes
  the double call; what it does not note is that the second one is in a *different process*,
  which is what makes it a race rather than a redundancy.
- **`git fetch` truncate-then-append on `FETCH_HEAD` is not atomic**, and `git pull --rebase`
  fails hard on two for-merge entries. Confirmed against git 2.53.0. Any code that runs
  `pull --rebase` in a repository another process might fetch has this bug latent.
- **A corrupt row in the AST graph is invisible to `graphit sync`.** The shard cache keys on
  content hash, so an unmodified file reports "up to date" and the bad rows persist. Repair is
  `ast_index` with `reindex: true` on the specific path.
- **`Failed calling LOWER: Invalid UTF-8` has two causes, and reindexing only fixes one.** A
  durable corrupt row fails on every query, in every process, across a daemon restart, and the
  **labelled** scan of its own table fails too. The transient mode fails only on a wide
  unlabelled scan while the daemon is writing — every one of the 37 labels scanned individually
  succeeds — and is gone on retry. The one-query test that separates them: run
  `MATCH (n:<Label>) RETURN count(toLower(n.name))` per label. If all labels pass while
  `MATCH (n)` fails, there is no corrupt row and nothing to reindex. Both were observed in this
  session, hours apart.
- **`toLower()` over an unlabelled `MATCH (n)` is evaluated widely enough that a `WHERE` filter
  does not protect it.** `MATCH (n) WHERE n.path STARTS WITH 'cmd/' RETURN count(toLower(n.name))`
  failed on a bad row that lives in `internal/`. Filtering by path does not stop the projection
  from touching other rows, which is why bisecting by path gave three "failing" partitions for
  a single bad file. Bisect by **label** first — that does narrow it.
- **Existing probes for the corruption** live in `internal/ast/ladybug_fts_utf8_test.go`,
  `ladybug_bulk_string_integrity_test.go`, `ladybug_gc_pressure_test.go` and
  `ladybug_field_scale_test.go`. They document what has already been ruled out; read them
  before proposing a hypothesis.
- **`internal/ast` tests need `-tags fts5`.** `BUILD_TAGS := fts5` in the Makefile. Without it
  the search-index tests fail with `no such module: fts5`, which looks like a regression and is
  not one.

## Progress Log

### 2026-08-10

- Reproduced the Hub state on the reporting machine: local `main` = `7d0bf46`, `origin/main` =
  `36cadb2`, local permanently one commit ahead; `FETCH_HEAD` held a single entry, so the
  failing run's own fetch was not the one that wrote two.
- Traced the second writer to `spawnBackgroundSync` → `runSyncHeavyTasks` →
  `NewRegistryManager` → `EnsureCloned` → `Sync`, in a separate process under a separate lock.
- Confirmed against git 2.53.0 that two for-merge entries in `FETCH_HEAD` abort
  `pull --rebase` with the exact reported message.
- Rewrote `Sync()` to fetch into the tracking ref and rebase onto it, added the global hub git
  lock and `lockfile.Acquire`, and added the ancestor fast path. Verified on the live hub that
  the remote tip is an ancestor of `HEAD`, so sync now short-circuits with no fetch at all.
- Located the `LOWER` failure by bisecting with `count(toLower(n.name))` per label, then per
  file: four `Comment.name` rows in `internal/ast/helper.go`, deterministic across processes,
  with stored lengths that disagreed with the source by a different amount each.
- Repaired the data with `ast_index reindex` on that one file; `toLower()` over all 56850 names
  and all 55727 docstrings then succeeded.
- Verified all 2176 shard cache files are valid UTF-8 with zero U+FFFD, which — given Go's
  `encoding/json` substitutes U+FFFD for invalid UTF-8 — eliminates the parse cache as a
  carrier and contradicts the previous conclusion in the upstream report.
- Made `resolveEntity` try exact match before case folding, so the next corrupt row degrades
  one lookup instead of all of them.
- Tests: `internal/hub`, `internal/hub/adapters/ide`, `internal/lockfile` all pass;
  `go test -tags fts5 ./internal/ast/ -run 'Source|Entity'` passes.

### 2026-08-10 (later — technical debt pass)

- Closed the two `pull --rebase` debt items. All five call sites in the codebase are gone; a
  repo-wide grep for `"pull"` outside tests now returns nothing. The hub package shares
  `fetchTrackingRef`/`trackingRefFor`/`rebaseOntoTrackingRef` between `Sync()` and both
  `MemoryWorktree` paths; `internal/memory` has its own equivalent.
- Verified against git 2.53.0 that `git rebase -X ours <ref>` replays commits across unrelated
  roots with no `--allow-unrelated-histories`, before dropping that flag from the memory paths.
- **Found and fixed a regression I had introduced earlier in the session**: the new ancestor
  fast path let `CommitAndPush` skip a needed rebase when the 30 s remote-commit cache was
  stale, which would surface as a rejected non-fast-forward push. Added
  `invalidateCachedRemoteCommit` and the `cachedRemoteOK`/`freshRemoteFirst` modes on
  `syncLocked`.
- Verified the other two debt items rather than changing them: `SyncEvents()` touches no index
  and no `HEAD` (read its git commands), and the unpushed bootstrap commit is carried by
  `CommitAndPush`'s `push HEAD`.
- **Corrected an over-broad claim from earlier today.** `LOWER` failed again mid-session, and
  this occurrence was *transient*: all 37 labels scanned clean individually while the unlabelled
  scan failed, and the same query succeeded on retry. So the error text covers two distinct
  modes and only the durable one is data loss. Recorded the separating test in System Knowledge
  and split the two in the upstream report.
- Tests: `internal/hub`, `internal/hub/adapters/ide`, `internal/lockfile`, `internal/memory`
  (18.4s) all pass. New: `TestRebaseOntoTrackingRefIgnoresFetchHead` in both packages,
  `TestRebaseOntoTrackingRefReportsAMissingBranch`,
  `TestPrePushSyncDoesNotTrustTheRemoteCommitCache`.
