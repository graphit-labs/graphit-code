# FTS index is not maintained on CREATE — rows stay unsearchable until the index is rebuilt

**Project:** LadybugDB / liblbug
**Version:** 0.18.2 (via `github.com/LadybugDB/go-ladybug` v0.17.0)
**Platform:** Linux x86-64

## Summary

Rows inserted into a table that already has an FTS index are not added to that index. They
exist and are matched by `CONTAINS`, but `QUERY_FTS_INDEX` never returns them. No error is
raised at insert time.

The number of rows that *do* become visible is small and stable rather than random, which
suggests a fixed buffer or visibility window rather than a race: with 10 seeded rows indexed
and 25 rows inserted afterwards, exactly 3 of the 25 were reachable, in 12 out of 12
independent runs.

Only `DROP_FTS_INDEX` + `CREATE_FTS_INDEX` makes the rows searchable.

Both write paths are affected — single-row `CREATE` and `UNWIND $batch AS row CREATE`.

The vector index does **not** share the problem: in the same database, `CREATE_VECTOR_INDEX`
reflects both `DELETE` and `CREATE` immediately.

## Reproduction

```cypher
INSTALL fts;
LOAD EXTENSION fts;

CREATE NODE TABLE R(uid STRING, name STRING, PRIMARY KEY(uid));

-- Seed and index, so the index exists before the writes under test.
CREATE (:R {uid:'s0', name:'seed_0_token'});
-- ... nine more seed rows ...
CALL CREATE_FTS_INDEX('R','r_idx',['name']);

-- Insert 25 rows AFTER index creation.
CREATE (:R {uid:'n0', name:'novo_0_marcador'});
-- ... twenty-four more ...

-- Each of these should return the matching row. 22 of 25 return nothing.
CALL QUERY_FTS_INDEX('R','r_idx','novo_0_marcador') RETURN node.uid;
```

Observed: 22 of the 25 inserted rows are not returned. `MATCH (r:R) RETURN count(r)` shows
all 35 rows present, and `WHERE lower(r.name) CONTAINS 'marcador'` finds all 25.

After `CALL DROP_FTS_INDEX('R','r_idx')` followed by
`CALL CREATE_FTS_INDEX('R','r_idx',['name'])`, all 25 are returned.

Note on sample size: a probe that inserts only one or two rows appears to work, because those
fall inside the visible window. That is what initially hid the bug for us — several small
probes "proved" in-place maintenance before a 25-row probe showed otherwise. Reproducing this
needs more than a couple of rows.

A runnable Go version is `TestLadybugFTSPerRowInsertIsReliable` in
`internal/ast/ladybug_fts_perrow_test.go` of https://github.com/graphit-labs/graphit-code
(12 iterations, fresh database each time).

## Related symptom

Deleting a row whose terms never reached the index fails with:

```
Runtime exception: FTS index 'se_split' is inconsistent: term 'checksum' is missing during
delete. Drop and recreate the FTS index.
```

The message is accurate about the remedy, which is what pointed us at the cause — but the
inconsistency is created by an ordinary `CREATE`, not by anything unusual.

## Expected

Either `CREATE` maintains the FTS index, or the insert fails loudly, or the limitation is
documented so callers know an index rebuild is required after writes.

## Impact

We index a 35k-file Oracle PL/SQL corpus and serve search over it. The workaround —
recreating every FTS index after each write — turns an incremental update of one edited file
into O(corpus) work: measured 979 ms against 89 ms for the same operation on the previous
storage engine, at 800 files, and 5.3 s at 200k entities. The cost grows with corpus size
while the change does not.
