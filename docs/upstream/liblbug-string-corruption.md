# Intermittent string corruption: stored values come back as invalid UTF-8

**Project:** LadybugDB / liblbug
**Version:** 0.18.2 (via `github.com/LadybugDB/go-ladybug` v0.17.0)
**Platform:** Linux x86-64
**Status:** **not reproduced in isolation.** Field observations plus the hypotheses that have
been eliminated. Filed because the failure is silent data loss and because the eliminations
may be more useful to someone who knows the storage layer than another guess would be.

Three occurrences are recorded. The second (2026-08-10) is the load-bearing one for the
diagnosis: the **stored byte length** was wrong as well as the bytes, which no corruption on
the caller's side can produce, and it rules the parse cache out as a carrier. Read that section
first. The third (2026-08-12) is the load-bearing one for the *severity*: the same defect
returning another row's perfectly valid text, with nothing raised anywhere.

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

## Second occurrence, 2026-08-10: the stored *length* was wrong too

A second field occurrence, in a different table and a much smaller project — 810 files, 56850
node names — puts the diagnosis back on the storage layer. It reproduces nothing, but it
narrows what the corruption can be.

This time the corrupt column was `Comment.name` in the node property table, not
`SearchFile.sf_source` in the FTS table, so this is not specific to FTS or to file source.
Four rows corrupted, all four belonging to **one** file (`internal/ast/helper.go`); every other
file in the project was intact.

**What makes it new: `size()` disagreed with the input by a different amount on every row.**

| row | bytes written | bytes the database reported | what `RETURN n.name` returned |
|---|---|---|---|
| line 22 | 68 | 72 | `""` |
| line 24 | 71 | 65 | `"BencL"` |
| line 115 | 63 | 69 | `"BeT"` |
| line 204 | 66 | 88 | `"Bencb"` |

A Go string handed to the driver carries its own length. Corrupting the *bytes* behind it —
by a moved allocation, a reused buffer on our side, a GC interaction — cannot change the
length the database goes on to report, and it certainly cannot change it by a different
delta per row. What was stored was therefore not a mangled copy of the right value: it was a
different (offset, length) pair, pointing somewhere else in the string data. That is a
storage-layer shape, not a caller shape.

Two further properties of this occurrence:

- **It was durable, not a read artifact.** The same garbage came back from repeated queries,
  from separate processes, and across a daemon restart. It survived on disk until the row was
  rewritten.
- **Rewriting the single file repaired it completely.** `ast_index --reindex` on that one path
  produced correct values for all four rows, and `toLower()` over all 56850 names then
  succeeded. Nothing else in the project needed touching.

### Third occurrence, 2026-08-12: the silent case, observed

The two reports above both surface through `LOWER`, i.e. through *invalid* UTF-8. The first
report notes in passing that "a value that is corrupted but still happens to be valid UTF-8
would never raise anything at all". That case has now been observed, and it is the same
wrong-`(offset, length)` shape landing on a neighbouring value instead of on nonsense.

On the same project's graph, before the reindex that cleared it, this query

```cypher
MATCH (c:Comment)
WHERE c.name CONTAINS 'deliberate' OR c.name CONTAINS 'on purpose'
RETURN c.name, c.path, c.line_number ORDER BY c.path
```

returned rows whose `path` and `line_number` were correct — they pointed at real comment nodes
in those files — while `name` held the **complete, valid text of a different row's comment**,
every one of them from a file under `internal/ast/`:

| returned `path` : `line_number` | `name` the database returned | where that text actually lives |
|---|---|---|
| `internal/hub/adapters/ide/mandate.go:143` | `This is deliberately not tokenize='trigram'. FTS5's trigram tokenizer` | `internal/ast/fts_sqlite.go:109` |
| `internal/git/git_test.go:971` | `reRelTypeList is deliberately narrow: it only accepts a bracket whose content` | `internal/ast/ladybug.go:49` |
| `internal/ui/src/components/dream/DreamDashboard.tsx:461` | *(same Go comment as above)* | `internal/ast/ladybug.go:49` |

The last row is the clearest tell: a Go comment returned on a `.tsx` node.

**Why this is the worst mode rather than a curiosity.** Nothing fails. The value is valid
UTF-8, plausible, and internally consistent with its own columns, so no exception is raised
at write time, at read time, or during index construction. `LOWER` is happy. The query simply
answers wrong, with confidence, and any consumer — a person or an agent — has no signal at
all. In this instance the wrong answer was caught only because a reviewer knew the codebase
well enough to notice that a Cypher-related comment could not live in a React component.

**It coexisted with the durable loud mode in one graph.** Earlier in the same session, on the
same graph, `MATCH (n) WHERE toLower(n.name) CONTAINS '...'` failed with
`Failed calling LOWER: Invalid UTF-8`. So one graph held both a value corrupted into invalid
bytes and values corrupted into other rows' text — consistent with a single defect in
offset/length resolution, whose visible symptom depends only on where the bad offset happens
to land.

**Repair and verification.** `ast index --reset` cleared it. Verified afterwards not by
`LOWER` — which cannot see this mode — but by cross-checking every `Comment.name` in the graph
against the file content at its own `path` and line range: 10287 of 10287 Go comment nodes
matched, 0 divergences. That cross-check is the only test that detects this mode, and it is
worth having as a standing integrity probe rather than as a one-off.

### A second, distinct mode: the same error transiently, with no corrupt row behind it

Later the same day, `LOWER` failed again on this project — and this one is **not** the same
failure, which matters because the repair is different and reindexing would have been the wrong
move.

```
MATCH (n) RETURN count(toLower(n.name))   -> Failed calling LOWER: Invalid UTF-8.
```

while the same aggregate run against **each of the 37 node labels individually** succeeded, all
37 of them. Corrupt data cannot produce that: a bad row lives in some table, and that table's
own scan would have failed too. Retried a few seconds later, the unlabelled query returned
57105 rows and no error. It coincided with the daemon reindexing files that had just been
edited.

So there are two modes behind one error message, and they must not be conflated:

| | durable | transient |
|---|---|---|
| Reproducible | every query, every process, survives a daemon restart | gone on retry seconds later |
| Labelled scan of the affected table | also fails | succeeds |
| Where it shows | one specific table, specific rows | wide multi-table scan, no row attributable |
| Repair | reindex the affected file; nothing else clears it | none needed — retry |

Only the durable mode is data loss. The transient one appears to be a read landing while a
write is in flight, which is a known-sharp area in this engine for this project (read-only opens
during a checkpoint already fail with `status 1` and occasionally crash). It is listed here
because it is the same exception text, and because "reindex on a `LOWER` failure" is the wrong
reflex for half the cases.

### This also eliminates the parse cache

The previous section's conclusion — look upstream, at the parse cache round trip — does not
survive contact with how that cache is encoded. **The shard cache is JSON written by Go's
`encoding/json`, which replaces invalid UTF-8 with U+FFFD rather than emitting it.** A value
that travelled through the cache therefore *cannot* arrive at the writer as invalid UTF-8; it
would arrive as a valid replacement character and `LOWER` would never complain. All 2176 shard
files on disk were checked: every one valid UTF-8, zero U+FFFD.

That leaves the window between the writer's Go strings and what the database returns, which is
where the first report started.

## Where this leaves the diagnosis

Content shape, value size, batch size, volume, concurrency, cgo pointer handling and the parse
cache round trip have each been eliminated. What remains consistent with every observation of
the **durable** mode — rare, independent of content, unreproducible on re-run, and with a
**wrong stored length** rather than merely wrong bytes — is a string whose offset/length
metadata is written or resolved incorrectly inside the storage layer.

The transient mode may well be the same defect seen from the read side rather than a second
one; nothing here decides that. What it does establish is that a `LOWER` failure alone is not
evidence of a corrupt row — the labelled-scan check above is what tells the two apart, and it
costs one query.

The 2026-08-10 occurrence coincided with the daemon being killed and replaced mid-run (a
launcher stamp change) while an indexing write was in flight. That is recorded as timing, not
as cause: nothing here establishes that an interrupted write produces this, and the earlier
field occurrences have no such event attached.

### What would help from upstream

Whether `STRING` values pass through a shared or reused buffer anywhere between the client API
and storage — one that a subsequent write, an index build, or a concurrent reader could
overwrite before the value is durably copied — and specifically whether a value's stored
offset/length can be committed independently of the bytes it points at. That second question
is the one the 2026-08-10 lengths raise, and it would explain all of it.

## Reproduction

None available. The probes that failed to reproduce it are kept in the reporting project as
`ladybug_fts_utf8_test.go`, `ladybug_bulk_string_integrity_test.go` and
`ladybug_gc_pressure_test.go`, and can be shared if useful.

