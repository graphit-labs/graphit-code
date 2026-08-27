# Read-only reader stops deleting engine files, and connect stops memorizing failure

**Date:** 2026-07-29
**Scope:** `internal/ast/ladybug.go`, `internal/ast/ladybug_reader_isolation_test.go`
**Origin:** Engineer's report — `graphit_ast_query`/`schema` failing with
`ladybug open: failed to open database with status 1`, on tools that only query

---

## The symptom and what it wasn't

The natural hypothesis was lock: daemon holds the single writer slot during reindex and
read falls in that window. **Wrong.** Two-process probe, 80 read-only openings per
scenario, each in a new subprocess (the shape of an MCP call):

| writer | ok | `status 1` | crash |
|---|---|---|---|
| just holding RW handle, without writing | 80/80 | 0 | 0 |
| commit in transaction, without CHECKPOINT | 79/80 | 0 | 1 |
| **commit + CHECKPOINT** | 63/80 | **14** | **3** |
| same, + `CleanupInterruptedSwap` on reader | 63/80 | 13 | 4 |

Holding the database read-write is harmless. The trigger is **CHECKPOINT**: 21% of read-only openings
fail and ~4% SIGSEGV inside `lbug_database_init` — cgo crash, not
recoverable, kills the `graphit-mcp` process.

Why only now: in the old copy+swap model CHECKPOINT and `Close` landed on the disposable
copy, and production was only replaced via `rename`. With in-place writes the daemon
checkpoints **the file readers open** — `Shutdown()` runs `CHECKPOINT`, `Close()` does
the same flush, `reindexAST` does both per reindex, and `auto_checkpoint` stays at the engine
default.

## 1. `CleanupInterruptedSwap` was deleting live liblbug files

The function ran at the top of `connect()` — **on every open, read-only included** — and the rule
was "glob `<dbPath>.*`, delete everything except exact `.wal` and the search index". But the engine
names its own sidecars the same way (`storage_utils.h`):

```
<p>.wal                     WAL                          forgiven
<p>.wal.checkpoint          WAL checkpoint               DELETED (comparison was equality)
<p>.shadow                  shadow file, live during checkpoint  DELETED
<p>.tmp                                                  DELETED
<p>.checkpoint.intent.lock  <p>.checkpoint.apply.lock    DELETED
```

Measured: over 80 concurrent read-only openings with a checkpointing writer, the reader deleted
`ladybugdb.shadow` 20 times and `ladybugdb.wal.checkpoint` 21 times. A reader tearing the
checkpoint state out from under the writer — plus `.old` (only copy of production during the
`AtomicSwapDB` window) and the working copy of an in-flight incremental.

**The rule was inverted:** delete only what this project creates — `<p>.old`, `<p>.staging`
(legacy name) and working copies `<p>.<7 hex>` from `shortHex()`, with their sidecars. An
allowlist cannot be kept in sync with a dependency's naming; naming what we
create can.

And the call moved from the start of `connect()` to **after a successful read-write open**. At that point we know the writer slot is ours, so no other process
is mid-swap — and the reader never runs cleanup. In passing, `os.MkdirAll` also
became writer-only: creating a directory is not a reader's job.

## 2. `connect()` memorized failure forever

The entire body lived inside a `sync.Once` that stored the error in `connectErr`. An
opening that landed in the writer's checkpoint window **poisoned the backend for its entire
lifetime**: every subsequent call returned the stale error without ever trying again.

Now `connect()` takes `k.mu` and delegates to `connectLocked()` (retry) → `openOnce()` (one
attempt). Five attempts, backoff doubling from 50 ms, ~750 ms sleep in the worst case — the
budget a read tool can spend without looking stuck. Failure is not
memorized.

`ensureConnected` now calls `connectLocked` because its caller already holds `k.mu`; before
it called `connect()`, which would now take the mutex again.

Read-only open of a non-existent database fails **without spending the retry** and with a message that says what
it is (`no database at <path>`), instead of generic `status 1` — liblbug's C API has no
message channel at all for `lbug_database_init`, it returns only 0 or 1, so every cause
arrives with the same text.

## Tests

`internal/ast/ladybug_reader_isolation_test.go`, four:

- `TestCleanupInterruptedSwapSparesEngineSidecars` — plants the engine's six sidecars, the three
  search-index ones and five swap leftovers; requires only leftovers to die.
- `TestReadOnlyConnectDeletesNothing` — writer attached and idle (arrangement the engine tolerates, and
  which leaves a real `.wal` on disk), read-only reader connects, directory listing
  compared before and after. Planting a fake `.wal` doesn't work: the engine tries to recover from it and
  open fails — that's how the first version of the test failed.
- `TestReadOnlyConnectOnMissingDatabaseFailsWithoutCreating` — fast failure and no directory
  created.
- `TestConnectRetriesAfterAFailedAttempt` — failure, database appears, the **same** backend connects.
  Red with `sync.Once`.

`go vet` clean. `internal/mcpstdio` and `internal/daemon` green with `-race`. In `internal/ast`
three tests fail — `TestDataFormatCollectionsAndCDATA`, `TestCreateTableYieldsTableEntityWithItsColumns`,
`TestOracleSchemaGraphIsQueryable` — and they are from another front in flight on the tree (34 query YAMLs,
`treesitter_adapter.go`, `antlr_adapter.go` and two unversioned test files), not from this
block: none touches database opening.

## 3. The checkpoint window: copy+swap returns as default

Engineer's decision: *if the only way for reads to be completely unaffected
is swap, go with it*. The conditional was measured before taking effect — same probe, one read-only
open per subprocess, now with row-integrity verification (every `body` is a
clean repetition of a marker) so "didn't crash" doesn't pass as "didn't tear":

| writer | ok | `status 1` | crash | torn |
|---|---|---|---|---|
| in-place, commit + CHECKPOINT | 43/60 | 11 | 6 | 0 |
| **copy+swap, production never opened by writer** | **60/60** | 0 | 0 | 0 |
| copy+swap as `reindexAST` does today | 59/60 | 1 | 0 | 0 |

Swap delivers untouched reads **by construction**: the writer mutates a copy and publishes with
`rename(2)`, so the reader opens either the old or the new file, never a file being written.

`inPlaceIncrementalEnabled` became **opt-in** — `GRAPHIT_INPLACE_INCREMENTAL=1` enables, and only `1`
enables, so a typo doesn't silently swap the model. Pinned by
`TestInPlaceIncrementalIsOptIn`, because this is a read-safety decision not a
tuning knob.

**What this costs, stated without sugar-coating:** incremental goes from 304–355 ms to 0.6–5.6 s, with
closing the mutated copy dominating and varying 12x. In-place remains available and tested for
where nothing reads in parallel — CI, one-shot indexing.

**The residual 1/60 has an address.** It's not from swap, it's outside it: `reindexAST` opens production
read-write, runs `CreateGraphSchema` on it and closes it at the end of each reindex — and it's that `Close`'s flush that
reopens the window. On the copy+swap path the production `lb` is only used for
`cfg.DBPath` and `AtomicSwapDB` renames, never for queries, so in principle the
indexer doesn't need to open it read-write. Removing that is a `reindexAST`/`RunPipeline` refactor,
not done here, and with the retry from item 2 above this class of failure is already absorbed.

## What remains open

- **The 1/60 from `reindexAST`** — stop opening production read-write on the copy+swap path.
- **Open message.** Go can enrich `status 1` by probing what it knows: does path
  exist? is `.wal`/`.shadow` present? does someone hold the lock?
- **Duplicated daemon.** There were two `graphit-core daemon` on the same project (PIDs 109917 and
  110052), doubling checkpoint windows. `daemonctl`'s spawn lock isn't holding.
