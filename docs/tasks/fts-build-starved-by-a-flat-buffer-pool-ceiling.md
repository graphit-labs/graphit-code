---
title: The FTS build starved with a buffer pool ceiling flat at 1 GiB
status: done
created: 2026-08-19
updated: 2026-08-19
tags: [ast, search, ladybug, resources, bug-fix]
---

# The FTS build starved with a buffer pool ceiling flat at 1 GiB

## Objective

`graphit ast index .` over a large corpus (39,429 files, ~2.5 M entities) aborted with

```
! Completed with 1 error(s) out of 39429 files
  › Timing: discover 0.14s, hash 0.06s, parse 0.00s, write 262.10s
  › ! Write errors: 1 chunk(s)
  • rebuild: search index rebuild: create fts index se_path: ladybug query:
    Buffer manager exception: Unable to allocate memory! The buffer pool is full
    and no memory could be freed!
```

on a 61 GiB machine with ~24 GiB free. The message looks like the machine running out of RAM and it
is not: what was out of memory was LadybugDB's buffer pool, which the indexer caps.

The cost of the error is not just the message. `RebuildFromJSON` (internal/ast/json_rebuild.go) treats
a search build failure as a reason NOT to publish — it keeps the previous database on purpose,
because publishing a graph without a search index is worse. Which means: the 262 s of writing were thrown
away and the project stayed on the old graph.

## Implementation Details

### The cause

`openOnce` (internal/ast/ladybug.go) swaps liblbug's `BufferPoolSize` — which by default is
~80% of physical RAM — for the result of `boundedDBBufferPool` (internal/ast/resources.go).
That function was `MemoryFraction(0.10, 256 MiB, 1 GiB, def/2)`, and the **1 GiB ceiling is
reached by any machine with more than 10 GiB**. On a 61 GiB machine the indexer
got 1 GiB and never more than that.

The comment justifying the ceiling said that the pool is a lazy growth maximum and
that the graph is small, so a low ceiling was "free". The first half is true; the
second is not. `CREATE_FTS_INDEX` keeps the entire term dictionary in the pool, and that is what
a large corpus blows up.

Why it never showed up in earlier measurements: `TestFTSIndexBuildScaling` opens the store via
`ladybugstore.Open`, which passes `DefaultSystemConfig` — ~80% of RAM, **without the clamp**. Every
FTS measurement up to now ran with tens of GiB of pool, while production ran with
1 GiB. The two halves of the system were never measured against the same configuration.

### The fix

A ceiling **per role**, in internal/ast/resources.go:

| role | fraction | ceiling | why |
|---|---|---|---|
| write (indexer) | 0.25 | 8 GiB | it is the only one that runs `CREATE_FTS_INDEX`; it is short and serialized by `sysutil.AcquireHeavy` (`HeavySlots` = 1) |
| read | 0.10 | 1 GiB | the daemon and the MCP server hold a handle for hours, and a buffer pool does not give memory back — this ceiling is what caps their RSS for the whole session |

`boundedDBBufferPool` now takes `readOnly bool`, and `openOnce` passes
`k.cfg.ReadOnly`. `GRAPHIT_DB_BUFFER_MB` still overrides both roles, because it is the
emergency exit that the error message recommends.

Also: `LadybugBackend` records in `bufferPool` the ceiling the handle was actually opened with
(`BufferPoolBytes()`), and `rebuildFTSIndexes` now wraps pool exhaustion in a
message that states the number and what to do with it (`SearchIndex.ftsBuildError`), instead of
passing along the engine's phrasing.

### The measurement

A new probe: `internal/ast/fts_bufferpool_probe_test.go`, behind `GRAPHIT_FTS_BUFPOOL=1`.
It runs the production write path (`OpenSearchIndex` + `newSearchCopyLoader` + the nine
`CREATE_FTS_INDEX` in the real order) varying the pool via `GRAPHIT_DB_BUFFER_MB`.

| entities | pool | result |
|---|---|---|
| 400 k | 1 GiB | MARGINAL — failed on `se_tri` in one run, built all nine in the next |
| 400 k | 1 GiB + `CHECKPOINT` between indexes | failed on `se_tri` just the same, and every `CHECKPOINT` came back in 0.00 s |
| 400 k | 8 GiB | nine indexes, 20.3 s |
| 1.0 M | 1 GiB | failure on `se_doc` |
| 1.0 M | 1.5 GiB | failure on `sf_source` |
| 1.0 M | 2 GiB | failure on `se_tri` |
| 1.0 M | 3 GiB | nine indexes, 44.0 s |
| 2.5 M | 6 GiB | **SEGFAULT** inside the engine on `se_tri`, with the machine at ~14 GiB available and swap full |
| 2.5 M | 8 GiB | nine indexes, 129.4 s |

Three things the table establishes:

1. **The requirement grows with the corpus, ~3 GiB of pool per million entities.**
2. **WHICH index dies tells you nothing** — it is whichever runs after the pool is already
   full. The field reported `se_path`; the probe reports `se_tri`; it is the same failure.
3. **`CHECKPOINT` is not the fix.** The hypothesis that dirty pages accumulated between the
   indexes were the cause is ruled out: with a `CHECKPOINT` between each `CREATE_FTS_INDEX` the
   failure is identical, and the checkpoints come back instantly.

## Use Cases

### UC-01: Full indexing of a large corpus on a machine with RAM to spare
- **Actor**: developer running `graphit ast index .`, or the daemon reindexing
- **Preconditions**: a corpus large enough for the FTS build to go past 1 GiB of pool
  (in practice, from ~400 k entities up); a machine with more than ~16 GiB of effective RAM
- **Main Flow**:
  1. `openOnce` resolves the pool via `boundedDBBufferPool(def, readOnly=false)` — 25% of the
     effective memory limit, ceiling 8 GiB
  2. `RebuildFromCache` loads the rows and calls `rebuildFTSIndexes`
  3. The nine `CREATE_FTS_INDEX` complete
  4. `RebuildFromJSON` publishes with `AtomicSwapDB`
- **Alternative Flows**:
  - `GRAPHIT_DB_BUFFER_MB` set: it wins over the fraction and the ceiling, for both roles
  - A machine whose liblbug default is already at the floor (256 MiB) or below: the default is
    returned intact, without inflating it
- **Error Scenarios**:
  - A corpus too large for the ceiling (e.g.: 2.5 M entities on a 16 GiB host, which yields
    4 GiB): `ftsBuildError` reports the pool in MiB and tells you to raise `GRAPHIT_DB_BUFFER_MB`
    (~3072 per million entities), and logs the same at Error level
  - Exhaustion at very high scale can arrive as a SIGSEGV inside the engine instead of an error
    (observed at 2.5 M with 6 GiB and the machine out of available memory) — there is nothing to
    catch in that case
- **Postconditions**: graph and index published together by the same `rename(2)`, or the previous
  database preserved
- **Affected Files**: `internal/ast/resources.go`, `internal/ast/ladybug.go`,
  `internal/ast/search_index.go`, `internal/ast/json_rebuild.go`

### UC-02: Daemon and MCP server keeping a predictable RSS
- **Actor**: global daemon, MCP server
- **Preconditions**: a read-only handle on the graph, alive for hours
- **Main Flow**:
  1. `NewLadybugDBReadOnly` marks `cfg.ReadOnly`
  2. `openOnce` resolves the pool via `boundedDBBufferPool(def, readOnly=true)` — 10%, ceiling 1 GiB
  3. The long-lived process stays capped at the read ceiling, which is what it was before this change
- **Alternative Flows**:
  - A read-only caller that reuses the read-write handle already open in the process
    (`acquireDatabase`) inherits the writer's pool, which is what we want: it is the same pool
- **Error Scenarios**:
  - A very expensive read query can still blow past 1 GiB. It happened once, with the explorer's
    default query, and the fix was to make the query cheaper — not to raise the pool
- **Postconditions**: the long-lived process's RSS stays capped as before
- **Affected Files**: `internal/ast/resources.go`, `internal/ast/ladybug.go`

## Test Cases & Acceptance Criteria

### Feature: buffer pool ceiling per role
Ref: UC-01, UC-02

#### Scenario: a big machine gives the writer a big pool
```gherkin
Given a machine whose effective memory limit is 16 GiB or more
When boundedDBBufferPool is called with readOnly = false
Then the result is at least 4 GiB
  And it never goes past 8 GiB
```

#### Scenario: a big machine keeps the reader capped
```gherkin
Given a machine whose effective memory limit is 16 GiB or more
When boundedDBBufferPool is called with readOnly = true
Then the result does not go past 1 GiB
  And it is strictly smaller than the write pool on the same machine
```

#### Scenario: a tiny default is not inflated
```gherkin
Given a liblbug default of 128 MiB, already below the 256 MiB floor
When boundedDBBufferPool is called with that default
Then the result is exactly 128 MiB
  And that holds for both roles
```

#### Scenario: the environment variable wins over both roles
```gherkin
Given GRAPHIT_DB_BUFFER_MB = 128
When boundedDBBufferPool is called with a default of 16 GiB
Then the result is 128 MiB
  And that holds for readOnly = true and readOnly = false
```

### Feature: FTS build under a capped pool
Ref: UC-01

#### Scenario Outline: the minimum pool that builds the nine indexes
```gherkin
Given "<entities>" entities loaded through the production write path
When the nine CREATE_FTS_INDEX run with a pool of "<pool>"
Then the result is "<outcome>"

Examples:
  | entities | pool    | outcome                  |
  | 400000   | 1 GiB   | marginal, fails sometimes |
  | 400000   | 8 GiB   | nine indexes             |
  | 1000000  | 2 GiB   | failure                  |
  | 1000000  | 3 GiB   | nine indexes             |
  | 2500000  | 8 GiB   | nine indexes             |
```

#### Scenario: pool exhaustion is reported in an actionable way
```gherkin
Given a corpus too large for the pool the handle was opened with
When CREATE_FTS_INDEX fails with "Buffer manager exception"
Then the error names, in MiB, the pool the handle was opened with
  And it recommends GRAPHIT_DB_BUFFER_MB with the order of magnitude (~3072 per million entities)
  And the previous database is preserved, not replaced by a graph without an index
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/resources.go` | Modified | buffer pool ceiling and fraction per role; the measurement recorded in the comment |
| `internal/ast/ladybug.go` | Modified | `openOnce` passes `ReadOnly`; the backend records the effective pool, exposed by `BufferPoolBytes()` |
| `internal/ast/search_index.go` | Modified | `ftsBuildError` — pool exhaustion now states the number and the way out |
| `internal/ast/resources_test.go` | Modified | contract per role: big writer, capped reader, tiny default intact, env wins |
| `internal/ast/fts_bufferpool_probe_test.go` | Created | probe that measures the minimum pool per corpus size |
| `docs/tasks/fts-build-starved-by-a-flat-buffer-pool-ceiling.md` | Created | this record |

## Trade-offs & Decisions

**A ceiling per role, instead of a single number.** Raising the single ceiling to 8 GiB would fix
indexing and make the daemon worse: a process that lives for hours would be able to grow 8x, and a
buffer pool does not give memory back. Two ceilings cost one `bool` in the signature and one `if`.

**A fraction of TOTAL RAM, not of available RAM.** 25% of 61 GiB is 15.25 GiB, cut at the ceiling to
8 GiB — but the machine measured had only ~21 GiB available and swap full. Budgeting by available
memory would be safer and is not stable: the number changes between the calculation and the use. All the
rest of the file (`AntlrHeapBudget`) already budgets by total; keeping the same basis is more predictable
than being more right on average.

**No automatic retry with a bigger pool.** `ensureVectorIndex` has a retry (`cache_embeddings
:= false`) because the engine offers a cheaper option; `CREATE_FTS_INDEX` has no
equivalent. The only possible retry is reopening the database with a bigger pool and redoing the build, which
means reloading the rows. Left out: the actionable message covers the case, and the
machine that needs this is the one that does not have the memory to give.

**`CHECKPOINT` between indexes was measured and discarded**, not discarded by reasoning.

## Technical Debt

- [ ] `ladybugstore.Open` still passes `DefaultSystemConfig` — ~80% of physical RAM, with no
      clamp and no respect for cgroups. It is how the wiki and memory stores are opened, and it is
      also why the earlier FTS probes measured a world that production does not live in.
      An `OpenWithConfig` (or the same per-role clamp) would close the inconsistency.
- [ ] Exhaustion at high scale can become a SIGSEGV inside liblbug instead of an error (2.5 M /
      6 GiB, machine out of memory). A cgo crash cannot be caught from the inside; what can
      be done is to estimate the requirement before starting — the entity count is known before the
      build — and fail early with the actionable message instead of late with a core dump.
- [ ] `sf_source` is the most expensive index (51.7 s at 2.5 M in the probe, ~270 s projected on the
      real corpus) and the one with the least weight in the fusion. Cutting it remains the cheapest path for
      the incremental's cost, and it remains unmeasured against `TestSearchIndexQualityFloor`.
- [ ] **Two writers at 8 GiB on a machine with no available memory take both down.**
      Measured during this very session: `go test ./internal/ast/` running alongside a
      real indexing, with the machine at ~11 GiB available and swap full, gave a SIGSEGV; the
      same test selection passes in 94.9 s with ~22 GiB available. `AcquireHeavy`
      serializes the pipelines INSIDE the daemon and does not cover a `graphit ast index` from a terminal
      competing with the daemon, nor `go test`. The per-role ceiling reduced the risk in the long-lived
      process, not in this one. A gate between processes (a lockfile on the brand) or budgeting against available
      memory would solve it; neither was done.

## System Knowledge

- **`CREATE_FTS_INDEX` keeps the term dictionary in the buffer pool.** It is the dominant
  consumer of the pool on the write path, and the only one that scales with the corpus.
- **The failure is not deterministic at the boundary.** 400 k / 1 GiB passed once and failed another time,
  on the same machine, minutes apart. Near the limit, "it passed" is not evidence.
- **`ladybugstore` and `internal/ast` open LadybugDB with different configurations.** Any
  measurement made by the first does not describe the behavior of the second. That is exactly what
  the earlier FTS measurements hid.
- **`acquireDatabase` shares one `*lbug.Database` per path in the process, with the config
  of the first one that opened it.** A reader that reuses the writer's handle inherits the writer's
  pool; a writer that finds only a read-only handle opens a private handle with its own
  config.
- **A search build failure publishes nothing.** `RebuildFromJSON` keeps the previous database on
  purpose — the cost of an error here is the whole reindex, not a degraded index.
- **The write ceiling is per PROCESS, and nothing coordinates processes.** See the debt above: the
  sum of the ceilings is what the machine has to withstand, and nobody computes it.

## Progress Log

### 2026-08-19

- Reproduced the failure with a probe on the production write path: 400 k entities already
  blow past 1 GiB, which rules out "it only happens on a gigantic corpus".
- `CHECKPOINT` between indexes discarded by measurement, not by reasoning.
- Found the reason no earlier measurement had seen this: `ladybugstore.Open` does not
  apply the clamp, and that is where the FTS probes opened the store.
- The ceiling became per role. The `internal/ast` suite green in 94.9 s on an unloaded
  machine; the SIGSEGV seen before was contention with a concurrent real indexing.
- **Verified against the real case that originated the report**: 39,429 files, `write 728,43s`,
  zero errors, `MATCH (f:File) RETURN count(f)` = 39,429 and hybrid search returning
  results. Before this change the same indexing aborted at `create fts index se_path`.
- Still open: no gate between processes (the debt above).
