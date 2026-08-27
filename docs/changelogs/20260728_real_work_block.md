# Watcher enters CI on three platforms, wiki 40/40, cross-process in-place and the end of the Ladybug trail

**Date:** 2026-07-28
**Scope:** `.github/workflows/ci.yml`, `internal/wiki/fts_embedding_test.go`,
`internal/ast/ladybug_crossprocess_test.go`, `internal/ast/ladybug_field_scale_test.go`,
`docs/upstream/`
**Origin:** Engineer's request to do the "Real work" block

---

## 1. macOS/Windows — what was possible, and what wasn't

**Cannot execute from here.** What can be done is make CI execute forever, which is better
than a manual run.

Discovery on inspection: `ci.yml` is **entirely `ubuntu-22.04`** — Lint, Tests, Security, Build
Check, UI. The `macos-14` and `windows-2022` seen in the repository are from `release.yml`, which
**builds** releases without running tests. The watcher, whose behavior is OS-provided
(inotify, kqueue, ReadDirectoryChangesW) and which replaced a `git status` poll that
behaved the same everywhere, had never been executed outside Linux.

New job `watcher-cross-platform`, three-matrix, `fail-fast: false`. Runs only
`internal/fswatch` and `internal/ignorer`, which depend only on `fsnotify` and two leaf
packages — no CGO, no ICU, no LadybugDB — so needs no per-platform dependency install and is fast. `-count=1` so green means it really ran.

**Honest caveat:** test debounce is 60ms and shared runner is slow and
noisy. The first run may come up red, and that's the result, not a job defect.

**ORT still has no execution coverage.** It's pure CGO; cross-compile from here hits
toolchain (`build constraints exclude all Go files`) and truly verifying requires the platform
with the native library installed. `release.yml` builds on all three; executing, nobody ever does.

## 2. Wiki FTS — 33/40 to **40/40**

The seven missing were the embeddings path, and the reason they stayed out was
wrong: they seemed to need a model. They don't — they receive and return vectors, so synthetic
vectors exercise all branches. What is tested is storage and ordering, not
embedding quality.

`unitVec(axis)` generates unit vectors pointing each to one axis, which makes geometry
trivial to reason about: a query aimed at an axis must rank that chunk first.

Covered: pending queue draining on embed, `EmbeddingStats` tracking, nearest
neighbor ranking correctly, `topK` respected, chunk without vector **never** appearing in semantic
result, hybrid in four states (text only, vector only, both agreeing, neither), and
rebuild not leaving vector pointing at a chunk that no longer exists.

`optimizeTables` only runs every tenth rebuild — nine of ten executions never get there. The test
does ten and verifies the index remains usable after segment merge, which is the real
risk there.

## 3. In-place with cross-process reader

`TestLadybugSeparateHandleDuringWrite` already covered a second handle **in the same process**. Production
is not like that: MCP is its own process, started separately, reading while the daemon writes. The
claim "in-place write doesn't break unlocked read" was in-process applied to a cross-process
arrangement.

Readers are now real subprocesses — the test binary re-executes itself with
`GRAPHIT_XPROC_READER`.

```
writer: 33771 writes, 0 errors
reader 0: 4932200 reads, 0 anomalies
reader 1: 4917800 reads, 0 anomalies
reader 2: 4944000 reads, 0 anomalies
```

Each row is written as a clean repetition of a marker, so torn reads appear as
a body that isn't a repetition.

### What the first attempt revealed

It failed with `failed to open database with status 1`, and the cause wasn't the test being
wrong about the engine — it was me opening readers read-write. **A reader that opens
read-write takes the single writer slot and locks the indexer out.** Production gets it
right (`NewLadybugDBReadOnly`), but nothing guaranteed it, and the failure appears on the indexer, far from the
process that caused it, as an opaque "failed to open database".

`TestLadybugReadWriteOpenerLocksOutTheIndexer` pins both sides: with a read-only holder the
indexer attaches, with a read-write holder it doesn't.

## 4. Ladybug corruption — trail ended

Field scale was the last untested dimension. Reproduced exactly: 35358 rows,
866 MB of accented text including C1 control characters, batch size 64,
`CREATE_FTS_INDEX` on the same column and scan of all rows.

```
cleanly built index
35358 of 35358 rows returned
0 invalid, 0 with wrong size
```

With volume eliminated alongside data shape, concurrency and cgo pointer, **the 5th report
is probably misaddressed and should not be sent as is.**

What no probe reproduces is the **path strings travel before the database**: in the field they
leave a parser, sit in a shard cache, are serialized to JSON and read back,
and arrive at the writer with several goroutines in flight. Every probe hands the database a string
built right there, seconds before.

So the next place to look is **above the database** — the parse cache round-trip and
string handling in this project — not liblbug. The report stays for the value of eliminations,
with that conclusion written in.

The test is skipped by default (`GRAPHIT_FIELD_SCALE=1`), because it writes ~1 GB and takes 2m13s.

## State

Full suite with `-race` clean. `internal/wiki`: 40/40 functions of `fts.go`.
