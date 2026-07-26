# Two smaller issues: Close() latency after small writes, and a crash in UNWIND ... CREATE

**Project:** LadybugDB / liblbug
**Version:** 0.18.2 (via `github.com/LadybugDB/go-ladybug` v0.17.0)
**Platform:** Linux x86-64

## 1. `Close()` takes 0.2-5.0 s after mutating a single file's worth of rows

Closing a database that has had a small number of rows written takes between 0.2 s and 5.0 s,
with the variance not obviously tied to the amount written. On an interactive path this is
the dominant cost: for us it was 12x the rest of the operation.

We worked around it by mutating in place inside a transaction instead of the previous
copy-mutate-swap model, which avoids closing a second database per update — the commit itself
takes 0.27 ms. So the workaround is good, but the underlying `Close()` cost is unexplained and
would still be paid by anyone opening and closing per operation.

Reproduction: open a database, write ~50 rows across two node tables, `CHECKPOINT`, then
`Close()` and time it. We can provide a Go harness.

## 2. Internal crash in `UNWIND ... CREATE`

Some `UNWIND $rows AS row CREATE (...)` statements fail with an internal error rather than a
query error:

```
Runtime exception: ... unordered_map ...
```

and in other cases the process reports `status 1` with no message. The same rows inserted one
statement at a time succeed, which is how we work around it (retry per row on detecting
either signature).

It is data-dependent and we have not reduced it to a minimal input yet — it appears with
batches in the hundreds of rows containing null-valued optional properties. If a maintainer is
interested we can bisect a failing batch and attach it.

## Impact

Both are worked around, neither blocks us. Filing so the behaviour is known: the first is a
performance surprise for short-lived connections, the second is an internal error surfacing as
a query failure.
