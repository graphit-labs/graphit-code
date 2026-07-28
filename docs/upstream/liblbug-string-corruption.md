# Intermittent string corruption: stored values come back as invalid UTF-8

**Project:** LadybugDB / liblbug
**Version:** 0.18.2 (via `github.com/LadybugDB/go-ladybug` v0.17.0)
**Platform:** Linux x86-64
**Status:** **not reproduced in isolation.** Field observation plus the hypotheses that have
been eliminated. Filed because the failure is silent data loss and because the eliminations
may be more useful to someone who knows the storage layer than another guess would be.

## Observation

Indexing a corpus of 35358 files, each written to a `STRING` column, intermittently failed at

```
CALL CREATE_FTS_INDEX('SearchFile', 'sf_source', ['source'])
  -> Runtime exception: Failed calling LOWER: Invalid UTF-8.
```

Scanning the table after the failure found **4 rows out of 35358** whose `source` value was
not valid UTF-8. Every one of those 4 files is valid UTF-8 on disk, verified byte by byte
before the write and again afterwards. Re-running the identical indexing job over the
identical corpus produced **zero** corrupt rows and no failure.

So the input is valid, the output is usually valid, and occasionally a handful of rows are
not. The corruption is on the path between a Go string and what the database stores or
returns, and it does not depend on which file is being written.

## Why this matters even though the error is loud

The exception is raised by `LOWER` during index construction, which is incidental — that is
merely the first operation to look closely at the bytes. Nothing reports a problem at write
time, and a value that is corrupted but still happens to be valid UTF-8 would never raise
anything at all. The observable failure is a symptom of silent corruption, not its boundary.

## What has been eliminated

Each of these was a hypothesis with a probe written against it. None reproduced the
corruption. They are listed so the same ground is not covered again.

| hypothesis | how it was tested | result |
|---|---|---|
| Bad input data | every file in the corpus decoded byte by byte | all valid UTF-8 |
| C1 control characters (CP1252 text read as Latin-1: U+0083 appears 915 times, U+0087 761 times) | synthetic values built from those codepoints | stored and returned intact |
| Value size / truncation mid-codepoint | values from 200 B to several hundred KB | intact at every size |
| Batch size or repetition | `UNWIND $batch AS row CREATE` at batch sizes 1–64, repeated runs | intact |
| Invalid UTF-8 present at the source | deliberately malformed input | rejected consistently, not intermittently |
| **Concurrent writers** | 8 goroutines writing to one database | **not applicable** — the engine refuses with `Cannot start a new write transaction in the system. Only one write transaction at a time is allowed in the system.` The production writer is therefore serial, which removes this whole class |
| **Garbage collection moving or freeing the Go string behind a C pointer** | `debug.SetGCPercent(1)` plus a goroutine calling `runtime.GC()` continuously, during 3000 batched inserts, with a second connection reading the same table throughout | no corruption |
| **Illegal Go pointer passed into C by the binding** | the same probe rebuilt with `GOEXPERIMENT=cgocheck2`, which validates every pointer crossing the cgo boundary | clean, no diagnostic |

| **Scale combined with value size** | the field run reproduced exactly: 35358 rows, 866 MB of accented text including the C1 control characters, written in batches of 64, then `CREATE_FTS_INDEX` over the same column and a full scan of every row | index built cleanly, 35358 of 35358 rows returned, **0 invalid, 0 wrong length** |

## Where this leaves the diagnosis

With volume ruled out alongside content shape, concurrency and cgo pointer handling, the
honest conclusion is that **this report may be misdirected**. A synthetic run matching the
field failure on row count, value size, byte composition, batch size and index build produces
nothing.

What the probes do not reproduce is the *path the strings travel before reaching the
database*: in the field they are produced by a parser, held in a shard cache, serialised to
JSON and read back, then handed to the writer, with several worker goroutines in flight.
Every probe here hands the database a string built inline moments earlier.

So the next place to look is upstream of the database — the parse cache round trip and the
string handling in the reporting project itself — rather than in liblbug. This file is left
in place because the eliminations are worth having on record, and because a storage-layer
defect that only volume plus real content triggers is not impossible. But it should not be
filed as a liblbug bug on the current evidence.

## Reproduction

None available. The probes that failed to reproduce it are kept in the reporting project as
`ladybug_fts_utf8_test.go`, `ladybug_bulk_string_integrity_test.go` and
`ladybug_gc_pressure_test.go`, and can be shared if useful.

## What would help from upstream

Whether `STRING` values pass through a shared or reused buffer anywhere between the client
API and storage — one that a subsequent write, an index build, or a concurrent reader could
overwrite before the value is durably copied. That shape would explain all of it: rare,
volume-dependent, independent of content, and invisible to a re-run.
