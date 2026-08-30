---
title: Pre-migration SQLite wiki.db breaks wiki embedding forever, and the error never says why
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [wiki, ladybug, migration, bugfix, diagnostics]
---

A SQLite `wiki.db` survives the migration and nobody notices.

Origin: the alert reported a failure opening three wikis. I attributed it to a
temporary daemon hiccup. **I was wrong** — and the log proved it.

---

## The Defect

The wiki index used to be SQLite and has now become LadybugDB, keeping the same file
name as before: `wiki.db`. Nothing converts or discards the old file, so any machine
that indexed before the change is left with a `wiki.db` file that the current engine
can no longer open.

```
level=WARN msg="wiki embedding cycle error" module=ast
  dir=…/wiki/memory/user/…  error="open wiki db: open wiki store: open wiki.db: failed to open database with status 1"
```

1,533 repetitions in ninety minutes, three wikis stalled, wiki embedding never running.

Confirmed by magic bytes, not by inference:

| file | format | date |
|---|---|---|
| `memory/user/<hash>/wiki.db` | `SQLite format 3` | 08-14 21:23 |
| `memory/project/<id>/wiki.db` | `SQLite format 3` | 08-17 19:03 |
| `knowledge/project/<id>/wiki.db` | `SQLite format 3` | 08-18 11:57 |

The `knowledge` entry is from just a few hours before the fix — the binary was still
writing SQLite that morning.

## Why it took so long to find

The message is as unhelpful as it gets: **status 1** is ambiguous by construction,
because the C API has no channel for the underlying C++ error text. It's identical
whether the cause is a lock, a missing file, creation under read-only, or an incorrect
format.

And checking for open file handles showed no process at all had the file open, which
ruled out contention — that's what pushed me to look at the format instead.

## Correction

**The fix:**

`discardPreMigrationDB`, called by `OpenWikiDB` before opening:

reads the first 16 bytes, and if they are `SQLite format 3\0`, removes the file along
with its `-wal`/`-shm`/`-journal` sidecars.

Deleting it is safe because the database is **derived** — this was already established,
and it's the only thing in a wiki directory that never travels: the pages and shards
next to it are exactly what it's reconstructed from, and a memory wiki is compiled
from its worktree.

The test is not "did the open fail." A healthy store that fails to open because of a
lock or some other transient condition must never be destroyed; only a file this
engine can never possibly read — because it isn't SQLite — should be discarded.
`TestOpenKeepsAStoreItCannotIdentifyAsSQLite` exists to catch exactly that distinction.

## Tests

- `TestOpenDiscardsPreMigrationSQLiteDB` — a file with the SQLite magic bytes plus
  sidecars; it opens, reconstructs, verifies the counts, and confirms the sidecars
  are gone.
- `TestOpenKeepsAStoreItCannotIdentifyAsSQLite` — an unreadable file that **is not**
  SQLite survives the attempt unscathed.

---

## Three more bugs found after the first one

Fixing the open path exposed the next layer of bugs. All three, independently, end
up in the same final state — an empty index, no errors — for unrelated reasons.

Both skip paths were only asking whether the file existed.

It first showed up in `StatPreCheck`, then again in `FastPathCheck`: the store —
present but empty — satisfies every other condition (the sources haven't changed,
and the pages that do exist have already been generated), so generation gets skipped
and the index never gets built at all. Execution short-circuits again:
**`memory index`** reported "complete" in 0.0 seconds on a database with 152 pages,
weighing in at 16 KB.

The fix folded the condition into a single check, defined once and used in both
places: whether the store was opened via generation, not via an existing file. The
alternative would have been a considerable rework — checking a file's magic number,
which fails silently the first time it's used on a store that starts out empty and
then changes.

**The two tests that failed because of this had encoded the bug as the expected
behavior** — they created an empty `wiki.db` on disk with the comment "an empty
wiki.db on disk is the default state for a pristine wiki," and each one only
asserted a partial truth (page count still valid / new-source detection still
valid); what changed is that the fixture now satisfies all the remaining conditions.

Bug two: the error from building the index was being discarded.

`internal/memory/wiki.go` did `_ = wiki.RebuildDB(...)`. Any failure silently turned
into a success: pages and shards were written, "memory index complete" was printed,
and the store stayed empty. And search kept responding, because it fell back to a
BM25 query over the `.md` files — so nothing looked broken, as long as the query
relied on the whole directory.

Bug three: there was no `--reset`.

The `--reset` and `--force` flags exist for exactly this: for when the index is
wrong for a reason that **is not** a memory change — which is exactly what a normal
run won't fix, because it skips over unchanged hashes.

To be safe: memory lives in its own Git worktree, and the entire wiki is derived
from them.

## What was left open — RESOLVED on 2026-08-18

The text below is the original write-up, kept as-is because both of its claims
turned out to be wrong, and how they were wrong is the lesson:

With all three bugs fixed, it reaches `RebuildDB` with 152 chunks and then returns
with no error — and yet the store is still empty. Every isolated reproduction of
`Rebuild` works. The remaining difference must be in the real memory path, and it
hasn't been found yet.

It did NOT actually return without error. The error was being swallowed by the same
`_ = wiki.RebuildDB(...)` discard described above — the second fix above replaced
that no-op with a line that logs the error instead of staying silent. "No error" had
been read off a log that was structurally incapable of ever showing one.

And the actual difference from the isolated reproduction wasn't in the memory path
at all, but in the corpus: `writeChunks` was sending a single UNWIND with rows that
had a vector alongside rows that didn't, which the driver rejected. The test
fixtures give vectors to all chunks or to none, so only a real-world corpus produces
the mix.

See `docs/tasks/wiki-indice-vazio-por-lote-de-vetores-misto.md`.

### Note: why it was so costly

In this episode, I made four consecutive wrong guesses before landing on the actual
cause: the old swap handling, embedding dedup by text, the daemon holding the write
slot, and embeddings never being injected. All four had the same shape — a
conclusion drawn from an instantaneous snapshot of a system that has asynchronous
phases, without reading the log that actually records those phases.

The log had been there the whole time, at `.graphit/runtime/daemon/daemon.log`
(per-project, written by `projectRebuildLogger`). I looked at
`~/.graphit/logs/graphit.log`, saw it was stale, and gave up instead of looking for
the right one. That per-project log is what actually resolved both real cases:

```
13:02:49  initial cycle complete            entities_embedded=36178
13:02:49  rebuilding DB to inject embeddings
13:03:23  search index rebuild  files=730 entities=57657 vectors=36295
```

This proves that my zero-vector measurements were taken, and that AST embedding does
work, within the twenty-minute window during which it was still running.
