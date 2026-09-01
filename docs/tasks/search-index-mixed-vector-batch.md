---
title: The search index did not build on a real corpus — a batch with and without a vector in the same UNWIND
status: done
created: 2026-08-17
updated: 2026-08-17
tags: [ast, search, ladybug, vector, bugfix]
---

# The search index did not build on a real corpus

**Origin:** measuring the 1178 s incremental recorded in
`consolidate-search-into-ladybugdb-and-drop-sqlite.md`. The measurement never got to the timing — the
full rebuild died before that, on a defect no test caught.

---

## The defect

```
search index build failed — keeping the previous database
error="insert entities: ladybug query: failed to convert Go value to Lbug value:
failed to create LIST value with status: 1. please make sure all the values are of the same type"
```

An `UNWIND` cannot mix, in the same batch, a row that carries a vector and a row that does not.
The driver builds **one** `LIST` for the whole `$batch` parameter and refuses different element
types.

| batch | result |
|---|---|
| all rows with a vector | ok |
| all with `emb: nil` | ok |
| mixed, vector before nil | **failure** |
| mixed, nil before vector | **failure** |

**Mixed is the normal case, not the edge.** Which entities get a vector is the grammar's decision,
through the YAML's `embed_labels` list — so any file that mixes an embedded label with a
non-embedded one produces both kinds of row, and a corpus of any size blows up on the
first flush.

### Why nothing caught it

Two reasons that added up:

1. The synthetic fixtures give a vector to **all** the entities or to **none**. An optional
   field filled uniformly does not exercise the optional field.
2. The development machine's production store was still in the previous format —
   `ladybugdb.search.sqlite` next to the graph — because the installed binary was from Aug 16
   03:44 and the migration is from Aug 16 16:09. The new path had never met a real corpus.

---

## The fix

The two obvious ways out do not work, and they were measured before choosing:

| attempt | result |
|---|---|
| `var typedNil []float32` | `failed to create LIST value because the slice is empty` |
| `[]float32{}` | same |

The only form the driver accepts is a **homogeneous batch**. `flushEntities` partitions the batch and
uses two queries: `insertEntityQuery` with `emb`, and `insertEntityQueryNoVec` **without the
property**, leaving the column NULL — which is what the vector query already ignores by
construction. `RebuildFromCache` and `UpdateIncremental` go through the same helper, so both
write paths are covered.

Regression in `search_emb_mixed_batch_test.go`, without an environment guard: four cases, 0.09 s.

`internal/ast` suite green, 98.5 s.

---

## What the investigation measured, and did not become code here

Recorded because it decides the next step, and because an earlier baseline fell with it. The
probes used were discarded after measuring; no instrumentation was left in the hot
path.

**`COPY` does not have the limitation that forced `flushEntities`.** `COPY FROM` on a Parquet with
`FLOAT[768]` loaded 20,000 rows, half with a vector and half with NULL, in 0.52 s. If the
index load migrates from `UNWIND` to `COPY`, the partitioning stops being necessary.

**The load through `UNWIND` with a vector column is the bottleneck of the full rebuild.** The same 80,000
rows, all with `FLOAT[768]`:

| path | time | µs/row |
|---|---|---|
| `UNWIND` in batches of 500 | 371.4 s | 4,643 |
| `COPY` from staged Parquet | **15.7 s** (+4.4 s staging) | **196** |

24x on the load. The same `UNWIND` on a table *without* a vector column costs 48 µs/row — the
column alone multiplies the insert by ~93, in the driver's value converter. Final state
verified: vector index built afterwards in 82 s, `QUERY_VECTOR_INDEX` returning the
exact match.

**The order of creating the vector index costs another 3.1x.** `EnsureSchema` creates `se_vec`
before the load, with the comment *"accepted on an empty table, so there is no ordering
constraint against the load"* — right about correctness, wrong about cost: each insert now
has to maintain an HNSW, and the cost is superlinear (10,088 µs/row at 2k, 17,202 at 20k) whereas
"afterwards" is linear. The FTS indexes are already created afterwards on purpose.

**Consequence for a baseline:** the 988 s of full rebuild recorded in
`consolidate-search-into-ladybugdb-and-drop-sqlite.md` cannot have been measured with
embeddings — with partial coverage they would blow up on the bug above, and with full coverage they
would have taken hours. That number is not comparable to a real build.

**And what is NOT a bottleneck, with numbers:** copying the store (833 MB ext4→ext4 in 0.27 s; the disk
does not support reflink and it is cheap even so), the format of the staging file (Parquet, CSV and
JSON tie on `COPY`: 0.71 / 0.74 / 0.79 s for 200k rows) and the concurrency
architecture — the engine's doc allows one `READ_WRITE` **or** several `READ_ONLY`, and the
copy+swap obeys that rule instead of working around it.

The 1178 s incremental remains another problem, in another phase: the DROP+CREATE of the nine
FTS indexes, which is O(corpus) because the engine has no partial build. Measured that
`CREATE_FTS_INDEX` costs O(rows *of that table*) and not O(database) — with 400,000 cold
rows untouched alongside, rebuilding 5 indexes of a 300-row table takes 1.64 s —, which
supports a two-segment design. Left for a task of its own.
