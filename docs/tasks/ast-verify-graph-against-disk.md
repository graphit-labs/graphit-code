# `graphit ast verify`: integrity probe, and the transient that almost made it useless

## What exists now

`internal/ast/verify.go` compares the graph's TEXT with the file on disk. It groups nodes by
`path`, reads each file once, and requires that each `name` line appears in the window
`[line_number, end_line]`. Exposed as `graphit ast verify`, with exit 1 when it confirms
a divergence, to serve as a gate.

It detects the SILENT mode of LadybugDB's string corruption — a wrong (offset, length)
pair that lands on **valid text from another line**. The value comes back well-formed,
consistent with the other columns, and simply wrong. `count(toLower(n.name))` passes
cleanly in that case because the bytes are valid UTF-8; it is not a weaker version of this
check, it is a check for something else.

Measured at rest: **46,867 nodes, 694 files, 0 divergences, ~4s**.

## The discovery that changed the design

The backlog prototype was single-pass. On the first real run it reported
**1915 divergences** — and the next run reported **zero**. With recheck added
but **without a pause**, it still confirmed **1873** on a graph that verified clean as soon as the
writer became idle.

In other words: the TRANSIENT mode — a read that falls inside the daemon's write window —
**numerically dominates the durable one, and by a large margin**. And re-reading on the same
connection, at the same instant, doesn't escape it: both reads fall on the same bad state.

What discriminates is `verifyRecheckDelay` (3s) before the second pass — a pause for the
whole batch, because what needs to change between reads is the WRITER's state, and it
is the same for all candidates. Only what fails BOTH times becomes a divergence; what
recovers is counted as `Transient` and named in the report, with an explicit warning
when it exceeds 5% of the graph.

After the fix: 8 consecutive runs under daemon activity, all clean.

## Corollary: DO NOT hang it at the end of `ast index`

The backlog suggested "subcommand **or** hung at the end of `ast index`". The second option
is discarded by measurement: it is exactly the worst moment, with the writer just active.
During a reindex the database doesn't even open (`ladybug open: failed to open database with status
1`) — and refusing there is the honest answer, not a defect.

## Decisions that avoid noise

- **Containment by line, not equality.** An end-of-line comment shares the line
  with code, and a declaration's name is one token among several. Equality would report the
  entire corpus on every run, and a probe that screams is a probe that gets turned off.
- **Labels come from the live schema**, minus `File`/`Directory`/`Module` (they name a path, not
  a source snippet) and minus those named by content whose text goes through `dataText`
  (`Value`, `AttributeValue`, `Text` truncate; `Comment` does not truncate and stays inside).
- **File that disappeared from disk is `Skipped`**, not a divergence: that is a stale index, not
  corruption, and reporting it would drown the signal.

## What remains out of reach

Repair. The defect is upstream and is open, and `graphit sync` doesn't fix it: the shard cache
is keyed by content hash, so it reports the intact file as up-to-date and never rewrites the
line. Only `--reindex`/`--reset` rewrites it.

## Progress Log

- 2026-08-16 — Probe, subcommand and seven tests: clean graph without noise, text borrowed
  from another file detected, end-of-line and multi-line block comments accepted,
  missing file skipped, and both sides of the recheck (what recovers is transient, what
  persists is confirmed). Full suite green, `make lint` 0 issues.
