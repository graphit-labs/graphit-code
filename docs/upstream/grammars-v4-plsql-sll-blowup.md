# PL/SQL: 1.7 KB Oracle file consumes >2 GB in the SLL prediction stage

**Project:** antlr/grammars-v4 — `sql/plsql`
**Runtime:** antlr4 Go v4.13.1
**Platform:** Linux x86-64

## Summary

A 1.7 KB Oracle PL/SQL file drives ATN simulation in the SLL (`PredictionModeSLL`) stage to
consume more than 2 GB of heap and get OOM-killed. The same file parses in the LL stage in
362 ms using 81 MB.

This makes the usual "try SLL first, fall back to LL" strategy unusable for this grammar: the
fast path is the one that dies, and it dies by exhausting memory rather than by failing in a
way a fallback can catch.

## Reproduction

Parse `VERIF_ITP.sql` (1.7 KB, Oracle 19c DDL: a package body with nested cursors and an
inline `SELECT` with several correlated subqueries) with:

- `PredictionMode = SLL` → RSS climbs past 2 GB, killed by the OOM killer
- `PredictionMode = LL` → completes in 362 ms, 81 MB peak

Start rule `sql_script`. We can attach the file if useful; it is customer DDL, so we would
send a reduced version.

## Notes

- The blowup is not cancellable: it happens inside ATN simulation, so a context deadline or
  `Interrupt()` does not take effect before the process dies.
- Reducing the input helps proportionally, which suggests unbounded configuration-set growth
  rather than a single pathological construct.

## Impact

We index Oracle corpora of ~35k files. We now force LL for every grammar, which costs parse
time we would rather not spend, because SLL cannot be attempted safely.
