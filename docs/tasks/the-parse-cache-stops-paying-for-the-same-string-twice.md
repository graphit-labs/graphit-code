---
title: The parse cache stops paying for the same string twice — and the rebuild's memory limit is removed
status: done
created: 2026-08-30
updated: 2026-08-30
tags: [ast, icebug, memory, oom, indexing, cross-platform]
---

# The parse cache stops paying for the same string twice — and the rebuild's memory limit is removed

## Objective

`graphit ast index --reset` on `private-corpus` (120,064 files) was OOM-killed at the
`Writing graph` phase, 52 minutes in, on a binary that already contained the T1..T8 streaming
work of [the icebug export task](icebug-export-streams-instead-of-materializing.md):

```
Out of memory: Killed process 1090324 (graphit-core)
    total-vm:54338508kB, anon-rss:48477088kB
oom-kill:constraint=CONSTRAINT_NONE, global_oom
```

**46.23 GiB of anonymous RSS on a 61.34 GiB machine, killed by a GLOBAL oom.** The
workstation ran out — Docker invoked the killer — not a cgroup.

### Reasoning — the first reading, which was right about the cause and wrong about the fix

`exportMemoryHeadroom = 0.75` × MemTotal (61.34 GiB) = **46.00 GiB**, and the process died at
46.23 GiB. The soft limit `applyExportMemoryLimit` installed was not merely failing to
prevent the OOM: it had authorised it. `debug.SetMemoryLimit` never fails an allocation, so a
limit is not a budget the process is held to, it is a size the process is *allowed to grow
into* — and taking that size off MemTotal is an instruction to ignore every other tenant on
the machine.

The obvious repair was to derive the limit from `MemAvailable` instead. That was implemented,
tested, and then **deleted** — see the Progress Log. Two engineer directives redirected the
task, and both were right:

1. *"the problem is that the solution is not under control and these limits end up hurting
   machines with good capacity"* — a memory limit is a symptom of the real term not being
   under control. The real term is the live footprint, and it had never been measured.
2. *"everything needs to run on linux, windows and macos"* — the limit's implementation read
   `/proc/meminfo` and `/sys/fs/cgroup` **with no build tag at all**, in a repository that
   already has the correct pattern for exactly this (`internal/sysutil/memory_{linux,darwin,windows,other}.go`).
   On Windows and macOS every read would have failed silently, capacity would have been 0,
   and no limit would have been installed — a policy that behaves differently per platform
   and says nothing about it.

### Justification for the approach that survived

**Measure the live footprint, then remove the term rather than cap it.** `TestShardCacheFootprint`
put a number on it for the first time: **26.0 GB** of live heap for this corpus. At that size
no limit could have helped — 26 GB live under a ~30 GiB limit is a collector running
continuously, not an export finishing.

`TestShardCacheStringDuplication` then showed the footprint was cheaply reducible: **50% of
the string bytes in a parse cache are the same value decoded again**, because `encoding/json`
allocates a fresh string for every field it fills and `entity.Path` has exactly one distinct
value per file. Interning at decode is pure Go, portable to all three platforms by
construction, and removes memory instead of rationing it.

Alternatives considered and dropped:

- **Keep the limit, correctly derived.** Implemented in full (`export_memlimit.go`, 11 tests)
  and then removed on the engineer's decision. The trade it offered — a slow export instead
  of a dead one — is real but only for a corpus this footprint work does not shrink enough,
  and it cost a platform-specific policy in a cross-platform product.
- **Lower `exportMemoryHeadroom` to 0.5.** Would have survived this run and still be a guess
  about how much of the machine someone else is using.
- **Stop `rebuildIcebugFromCacheWithDelta` retaining its `entries` map.** This was the first
  brief and it is WRONG: `ri.scan()` appends every entry into `ri.fileEntries`, which the row
  producers iterate, so both hold the same pointers and dropping the map frees single-digit MB.

## Plan & Task Breakdown

- [x] **T1 — Measure where the retained memory actually is** — Spec: an env-guarded probe that
  loads a SAMPLE of the real shard store and reports live heap per graph element. Done when
  there is a bytes-per-element number for this corpus. Constraint: it must sample — reproducing
  the OOM in order to study it is not a measurement, it is the same outage.
- [x] **T2 — The limit comes off MemAvailable** — DONE, then REVERTED by T7. Kept in this plan
  because the Progress Log refers to it and because deleting a completed item hides why the
  code that replaced it looks the way it does.
- [x] **T3 — Tests for the limit policy** — DONE, then REVERTED by T7.
- [x] **T4 — Cut the retained footprint by interning the parse cache** — Spec:
  `internal/ast/shard_compact.go`. Intern on adoption into the shard cache: a corpus-wide table
  for values whose cardinality is bounded (paths, labels, languages, module names, DML targets)
  and a per-file table for identifiers that repeat only inside one file. Constraints: values
  bit-identical after a round-trip; both tables capped so an all-distinct corpus cannot make the
  table cost what it saves; the shard format is unchanged, so **no `shardCacheVersion` bump**.
- [x] **T5 — Re-measure and report honestly.**
- [x] **T6 — A limit is a ceiling, never a floor** — DONE, then REVERTED by T7. It fixed a
  hazard in T2: the floor CLAMPED UP, so a large machine that momentarily had little free
  memory would have been handed a 2 GiB limit over a 14.6 GB live set — a hang, which is worse
  than the kill, because a kill is diagnosable.
- [x] **T7 — Remove the memory limit entirely** — Spec: delete `internal/ast/export_memlimit.go`
  and its test, and the `defer applyExportMemoryLimit(log)()` in
  `internal/ast/icebug_rebuild.go`. Done when nothing this task added reads `/proc` or
  `/sys/fs/cgroup`, and `go build ./...` and `go test ./...` are green. Constraint: the
  footprint work must stand on its own — state plainly what the removal costs.

## Implementation Details

### T4 — the parse cache stops paying for the same string thousands of times

`internal/ast/shard_compact.go` adds `shardInterner` and `compact` methods on `shardNodes` and
`shardEdges`. `ShardCache` carries one corpus-wide interner for its lifetime; a fresh per-file
interner is created for each adoption and discarded with it. Both are capped
(`shardInternLimit` 1Mi, `shardLocalInternLimit` 64Ki).

Which table a field goes into is the whole design:

| table | fields | why |
|---|---|---|
| corpus-wide | `Path`, `Lang`, `Label`, `Context`, `ContextType`, `SourceType`, `ReceiverType`, `RelType`, `ParentType`, `ParentLabel`, `ChildLabel`, `ModuleName`, `ModuleUID`, `RawImport`, `SourceFile`, `TargetUID`, `Decorators`, `DirPaths`, `Calls.CalleeUID`, `Inheritance.ParentUID`, `FieldAccess.FieldUID` | cardinality bounded by the grammar or by the file count, and every file repeats them |
| per-file | `UID`, `CallerUID`, `FuncUID`, `Fields.ParentUID`, `Contains.ParentUID`, `Contains.ChildUID`, `Inheritance.ChildUID`, `SourceUID`, `FileUID` | nearly unique corpus-wide, heavily repeated inside one file — a caller's uid across its calls, a parent's across its children |

> **Revised 2026-08-31.** The three fields moved into corpus-wide above were placed here
> originally on the assumption that a reference's target "repeats only within one file." That
> assumption was wrong for exactly the fields that name a declaration the referencing file
> merely POINTS AT rather than owns: a popular callee, a widely-subclassed base, a
> often-accessed field are referenced from many DIFFERENT files, which a per-file table
> (discarded after each file) cannot see. Measured on this repository's own store: 24% of
> `Calls` rows and 24% of `FieldAccess` rows carried a cross-file duplicate the per-file table
> was structurally blind to. See
> [edge UIDs pointing at a shared declaration now intern corpus-wide](edge-uids-pointing-at-a-shared-declaration-now-intern-corpus-wide.md)
> for the measurement and the fix. The remaining per-file fields were checked against the same
> question and confirmed to have a 0% cross-file gap — they stay per-file correctly.

`getEntryLocked` was reshaped so both halves of a file are adopted through one
`adoptShardsLocked` call and therefore share one local interner: an identifier a file declares
in its nodes shard is the same identifier its edges shard points at. `AllEntries` routes its
parallel loads through the same function.

`clip` releases the spare capacity `encoding/json` leaves on a decoded slice (it grows by half
a capacity at a time and then shortens only the LENGTH). It is applied to the top-level record
slices ONLY — see Trade-offs.

### T7 — the limit is gone

`internal/ast/export_memlimit.go` and `internal/ast/export_memlimit_test.go` are deleted, and
`rebuildIcebugFromCacheWithDelta` no longer installs anything. The rebuild runs under Go's
default GC target, on every platform, with no platform-specific code anywhere in this task's
output.

`internal/sysutil` is untouched and is a different thing entirely — see System Knowledge.

## Use Cases

### UC-01: A full index of a large repository
- **Actor**: engineer running `graphit ast index --reset`, or the daemon on a bulk change.
- **Preconditions**: a parse cache with N files exists on disk; the in-memory shards were
  evicted by the last `flushLocked`, so the rebuild re-reads them.
- **Main Flow**:
  1. `rebuildIcebugFromCacheWithDelta` streams the cache into `entries`.
  2. `StreamEntries` calls `GetEntry` per file; `getEntryLocked` decodes both shard halves.
  3. `adoptShardsLocked` compacts them through one shared per-file interner and the cache's
     corpus-wide interner, then stores them.
  4. `newRebuildIndex` scans, and the export writes the bundle.
- **Alternative Flows**:
  - One half of the file was already cached: only the freshly decoded half is compacted, and
    it still shares the local interner with nothing, which is correct — the cached half was
    compacted when it was adopted.
  - `AllEntries` loads in parallel and adopts under the lock through the same function.
- **Error Scenarios**:
  - A shard fails to decode → `getEntryLocked` returns nil and the file is skipped, and no
    half-adopted file is left in the cache.
  - The corpus is large enough that `2 × live` exceeds what the machine can spare → the
    process is OOM-killed. **This is now the accepted failure mode** — see Technical Debt.
- **Postconditions**: the parse cache holds one allocation per distinct string; every value
  is identical to what a plain decode would have produced.
- **Affected Files**: `internal/ast/shard_compact.go`, `internal/ast/shard_cache.go`,
  `internal/ast/icebug_rebuild.go`.

## Test Cases & Acceptance Criteria

### Feature: Parse cache compaction
Ref: UC-01

#### Scenario: A round-trip through the compacted cache changes no value
```gherkin
Given a corpus exercising every cached record kind, with values repeated within and across files
When it is stored, saved, and read back through a freshly opened shard cache
Then every entry equals the one that was stored, field by field
```

#### Scenario: A value repeated inside a file costs one allocation
```gherkin
Given a file whose entities all carry the same path
When the file is read back through the shard cache
Then both entities' Path fields point at the same string data
  And two different files' Path fields point at different string data
```

#### Scenario: A value shared across files costs one allocation
```gherkin
Given two files whose entities share a language and a label
When both are read back through the shard cache
Then their Lang fields point at the same string data
  And their Label fields point at the same string data
```

#### Scenario: The intern table stops growing at its limit
```gherkin
Given an interner with a limit of 2 entries holding "a" and "b"
When a third distinct value is interned
Then the value is returned unchanged
  And the table still holds 2 entries
  And an already-interned value still resolves
```

#### Scenario Outline: Clip releases spare capacity without changing the values
```gherkin
Given a slice with "<len>" elements and "<cap>" capacity
When it is clipped
Then the result holds the same elements in the same order
  And its capacity is "<result_cap>"

Examples:
  | len | cap | result_cap                  |
  | 3   | 64  | 3                           |
  | 2   | 2   | 2 (returned untouched)      |
  | 0   | 16  | 0 (returned as nil)         |
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/shard_compact.go` | Created | the interner, the compaction of both shard halves, and `clip` |
| `internal/ast/shard_compact_test.go` | Created | round-trip value equality, pointer identity for interned values, table limits, clip |
| `internal/ast/shard_cache.go` | Modified | one corpus-wide interner on the cache; both shard halves adopted through `adoptShardsLocked` |
| `internal/ast/shard_cache_footprint_probe_test.go` | Created | env-guarded probes: live heap per graph element on a real store, and string duplication by field |
| `internal/ast/icebug_rebuild.go` | Modified | no longer installs a memory limit |
| `internal/ast/export_memlimit.go` | Deleted | a platform-specific soft limit, in a cross-platform product, guarding a term that was fixed instead |
| `internal/ast/export_memlimit_test.go` | Deleted | with it |
| `docs/tasks/icebug-export-streams-instead-of-materializing.md` | Modified | its debt item "not validated against private-corpus" is closed, with the failure that closed it |

## Trade-offs & Decisions

- **The limit was removed rather than kept and fixed, and this costs something.** A correctly
  derived limit would have turned "killed" into "slow" for a corpus this footprint work does
  not shrink enough. The engineer's call, made twice and explicitly: a memory limit indicates
  the real term is not under control, and it cost a `/proc`-reading policy in a product that
  must run on Linux, Windows and macOS. The failure mode is now an OOM kill, which is at least
  diagnosable from the kernel log — as this very task demonstrates.
- **`clip` on the top-level record slices only.** MEASURED on 4,000 real files: clipping every
  per-entity `Decorators`/`Args` too cost **6.5 s of an 8.4 s load** — 77% more time — and
  bought 1.0 GB of the projected 15.6 GB. Clipping only the record slices keeps the memory
  result identical (14.6 GB) at 6.68 s against a 6.15 s baseline: **+9% load time**. Numerous
  tiny slices are the worst possible ratio for a copy-to-clip.
- **Interning is capped rather than adaptive.** A per-field cardinality estimate would target
  the tables better and needs a sampling pass the loader does not have. The cap gives the same
  protection against a pathological corpus for two comparisons per miss.
- **The shard format is untouched, so `shardCacheVersion` is NOT bumped.** Compaction changes
  how bytes on disk become objects in memory, never what is written; `TestCompactionPreservesEveryValue`
  is what holds that line. A bump would have invalidated every cached shard on every machine
  for a change that alters no value.

## Technical Debt

- [ ] **With no limit, the peak is `2 × live` and a corpus roughly twice this one is killed
  again.** Accepted deliberately (see Trade-offs). The number to watch: live is ~14.6 GB for
  37.5 M graph elements, so the default target is ~29 GB, and the next OOM will be a corpus
  around 75 M elements on a machine like this one. `TestShardCacheFootprint` is how to check a
  store before running the index against it.
- [ ] **The rebuild still holds the whole corpus live — ~14.6 GB, still O(corpus).**
  `ri.scan()` appends every entry into `ri.fileEntries`, and each node label and each
  relationship type is a separate pass over it, so the corpus must be either resident or
  re-readable. Re-scoped brief in
  `docs/tasks/backlog/the-icebug-rebuild-holds-the-whole-parse-cache-live-in-entri.md`.
- [ ] `ShardCache.Store` does not compact, so the PARSE phase still pays one allocation per
  occurrence. Bounded by the flush interval rather than by the corpus, so it is not the OOM
  path — but the interner is already on the cache and would apply.
- [ ] `entity.UID` is 61.8 MB of 129.8 MB of entity string bytes on a 2,000-file sample and is
  almost entirely distinct, so interning cannot touch it. It is largely `path + ":" + name`,
  i.e. derivable rather than stored.
- [ ] The probes extrapolate from a strided sample. The stride crosses clusters, but a corpus
  whose per-file cost is bimodal would still be mis-projected. The only exact answer is a full
  run.
- [ ] `GOOS=windows go build ./...` and `GOOS=darwin go build ./...` fail on this checkout, and
  did so **before** this task — the tree-sitter grammar packages (`plpgsql`, `clojure`, `dart`,
  `dockerfile`, `elixir`, …) are cgo and excluded by build constraints when cross-compiling.
  The Makefile builds per-host with a platform toolchain instead. So a plain `GOOS=` build is
  NOT a portability check here, and there is currently no cheap one — which is exactly why
  platform-specific code has to be caught by review rather than by the compiler.

## System Knowledge

- **A soft memory limit is a GC TARGET, not a budget.** `debug.SetMemoryLimit` never fails an
  allocation. Whatever number goes in is a size the process is *authorised to grow into*, and
  Go will use it. This is why the previous policy did not merely fail to prevent the OOM — it
  caused it: the process grew to 46 GiB because it had been told it could.
- **When a Go process is OOM-killed at a limit you gave it, compare the RSS to the limit before
  assuming the limit failed.** 46.23 GiB against a 46.00 GiB limit means the limit worked and
  the limit was wrong. That comparison is the whole diagnosis and it costs one line of the
  kernel log.
- **`internal/sysutil.MemoryLimitBytes` is NOT a cap on the process** — it is machine SIZING,
  and it is the correct pattern for reading the machine's memory in this repository. It answers
  "how big should this cache be here", and its only callers are `AntlrHeapBudget` (the ANTLR
  sidecar's parse-cache budget) and `boundedDBBufferPool` (LadybugDB's buffer pool, whose own
  default is ~80% of physical RAM and needs bounding). It is split across
  `memory_linux.go` / `memory_darwin.go` / `memory_windows.go` / `memory_other.go` behind one
  `memory.go` façade — which is how a platform reading is supposed to be done here, and what
  the deleted `export_memlimit.go` failed to do.
- **MEASURED on this corpus (120,064 files, 21.0 GB of shard JSON, 240,128 shard files,
  ~37.5 M graph elements):**

  | | live heap, 4,000-file sample | per element | projected, whole store |
  |---|---|---|---|
  | parse cache, before | 752 MB | 631 B | 22.1 GB |
  | + rebuild index, before | 888 MB | 745 B | **26.0 GB** |
  | parse cache, after | 363 MB | 305 B | 10.6 GB |
  | + rebuild index, after | 499 MB | 419 B | **14.6 GB** |

- **50% of the string BYTES in a parse cache are the same value decoded again**, measured over
  a 2,002-file sample: 129.8 MB across 2.86 M occurrences, 65.5 MB across 416,620 distinct
  values. `entity.Path` alone is 51.7 MB of it and has exactly one distinct value per file. The
  16-byte header per use is not recoverable — only the bytes are.
- **`encoding/json` leaves slack on every slice it decodes.** It grows by half a capacity at a
  time and then shortens the LENGTH to what it read, never the capacity, so a decoded slice
  carries up to a third of its allocation as unreachable slack.
- **`flushLocked(evict: true)` drops the in-memory shards after writing them**, which is why the
  export path re-reads from disk and why compacting at decode reaches it. Compacting at `Store`
  would reach the parse phase instead, which is a different and bounded problem.

## Progress Log

### 2026-08-30 — the limit is removed, and the footprint fix stands alone
The engineer settled the question they had opened earlier: *"everything needs to run on linux,
windows and macos. honestly, if we can do without these limit controls I think that is better."*

Both halves acted on:
- **Portability.** `export_memlimit.go` read `/proc/meminfo` and `/sys/fs/cgroup` with **no
  build tag**, in a repository that already has `internal/sysutil/memory_{linux,darwin,windows,other}.go`
  for exactly this. On Windows and macOS every read fails, capacity is 0, and no limit is
  installed — a policy that quietly behaves differently per platform. That is a defect
  independent of whether the limit was wanted.
- **The limit itself.** Deleted, along with its test and the `defer applyExportMemoryLimit(log)()`
  in `rebuildIcebugFromCacheWithDelta`.

Audited what remains: nothing this task added touches `/proc`, `/sys`, `syscall` or
`golang.org/x/sys`. `shard_compact.go` is pure Go.

Checked whether `GOOS=windows`/`GOOS=darwin` builds could serve as a portability gate: they
cannot, and they already failed on a clean `HEAD` for an unrelated pre-existing reason (cgo
tree-sitter grammars). Recorded as debt so the next agent does not read that failure as
theirs.

`go build ./...` clean, `go vet ./internal/ast/` clean, `go test ./...` green across 72
packages.

**What the removal costs, stated plainly:** peak is now `2 × live` with nothing bounding it.
At ~14.6 GB live this is ~29 GB, which fits the 61 GiB machine that was being killed at
46 GiB. A corpus roughly twice this one will be killed again. That is the accepted trade.

### 2026-08-30 — the engineer questioned the limit, and found a real defect in it
Mid-task the engineer pushed back: a memory limit makes them uneasy because it suggests the
underlying problem is not under control, and because a limit can penalise a machine that has
capacity to spare.

**The first half was answered by T1/T4** — the limit was never the fix; with live at 14.6 GB
the collector's own default target already fits this machine, so the limit did not bind at all.

**The second half was a live defect** in T2 as written. `exportMemoryFloor` CLAMPED UP: a
machine with 128 GiB of capacity that momentarily had 100 MB free would have been handed a
2 GiB limit over a live set of 14.6 GB. That does not bound the process — it makes the
collector run continuously and never finish, which presents as a hang, strictly worse than the
kill this module existed to prevent. Changed so the floor REFUSES to install rather than
clamping, and no limit is installed that fails to clear what the process already holds
(T6). Superseded the next turn by T7, which removed all of it.

### 2026-08-30 — measured, and the diagnosis moved
- **T1.** Wrote `TestShardCacheFootprint`, a strided sample over the real store. 4,000 of
  120,064 files → 1,250,204 elements → **752 MB retained, 888 MB with the rebuild index**;
  631 and 745 bytes per element; **26.0 GB projected for the whole corpus.** That number
  re-scoped the task: no limit fits 26 GB of live heap on a machine with 15 GiB already spoken
  for.
- Wrote `TestShardCacheStringDuplication` to find out whether the footprint was cheaply
  reducible. It was: **50% of string bytes are duplicates**, `entity.Path` being 51.7 MB of
  129.8 MB with one distinct value per file.
- **T4.** Interning at shard adoption: **26.0 GB → 14.6 GB projected, a 1.78× cut**, at +9%
  load time after measuring `clip` separately and dropping it from the per-entity slices.
- **T2, T3.** The limit was reworked to come off `MemAvailable + HeapAlloc` with 11 tests
  against injected readings. Both superseded by T7.

### 2026-08-30 — the diagnosis
- Diagnosed from the kernel log, the store left behind, and the installed binary. Full evidence
  in the Progress Log of
  [the icebug export task](icebug-export-streams-instead-of-materializing.md).
- Read `internal/ast/icebug_rebuild.go` and `internal/ast/rebuild_index.go` before planning,
  and **corrected the brief that came out of the diagnosis.** It said the fix was to stop
  `rebuildIcebugFromCacheWithDelta` retaining `entries`. That is wrong: `ri.scan()` appends
  every entry into `ri.fileEntries`, which the producers iterate. Both hold the same
  `*parseCacheEntry` pointers, so dropping the map frees the map and none of the corpus.
