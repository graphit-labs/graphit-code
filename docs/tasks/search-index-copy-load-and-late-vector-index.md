---
title: Loading the search index via COPY, and the vector index built afterwards
status: done-with-caveat
created: 2026-08-17
updated: 2026-08-17
tags: [ast, search, ladybug, vector, performance, copy]
---

# Loading the search index via COPY

**Origin:** the search index load phase never finished. In one measurement over a real corpus
(39,429 files, 2,501,342 entities) it grew by 270 MB in 55 minutes and projected more than
12 hours, which made any full rebuild unfeasible.

---

## The matrix, measured

80,000 rows, all carrying `FLOAT[768]`:

| arrangement | total |
|---|---|
| `COPY` + index afterwards (`cache_embeddings` default) | **89 s** |
| `COPY` + index afterwards (`cache_embeddings := false`) | 398 s |
| `UNWIND` + index afterwards | 440 s |
| `COPY` + index **before** | **>600 s** (interrupted) |

The same `UNWIND` on a table **without** a vector column costs 48 µs/row, against 4,643 with
it. The cost is in the driver converting each element of the vector on its way into the query
parameter — not in the index.

And the index ordering counts on its own: with it in place, every insert maintains an HNSW and the
cost is **superlinear** (10,088 µs/row at 2,000, 17,202 at 20,000), whereas building it afterwards
stays flat (~5,500). `EnsureSchema` was creating `se_vec` on the empty table with a comment saying
that an empty table accepts the index and therefore the order did not matter; it does matter, only
as cost and not as correctness.

## Result on the real corpus

| phase | before | after |
|---|---|---|
| search index load | >12 h (projected) | **321 s** |

The four files above the `COPY` value ceiling went through the `UNWIND` route, named in a
`Debug` log.

---

## What the implementation forced

**The `json` extension became a hard dependency of the write path.** Without it, `COPY` fails in
the binder with `Cannot load from file type json`. Eight tests broke on that before
`EnsureSchema` started loading it with a hard error, as it already did for `fts` and `vector`.

**`estimateRowBytes` was counting a `FLOAT[768]` as 8 bytes**, like any scalar, which made the
byte budget fiction for search rows. It now counts ~12 bytes per dimension, which is the cost in
JSON text.

**JSON `COPY` has its ceiling on the VALUE, not on the document** — and that corrects the lesson
`json_rebuild.go` had recorded:

| document | result |
|---|---|
| 1 row, source 16 MB (file 29.8 MB) | ok |
| 1 row, source 32 MB (file 59.7 MB) | **fails** |
| 60 rows of 1 MB (file **111.9 MB**) | ok |

So the batch budget is not what needs to shrink. A row above the ceiling goes on its own through
`UNWIND`, which stores 32, 64 and 128 MB intact — `SearchFile.source` is the only queryable copy of
the text, so truncating it to fit would be worse than failing. The threshold
(`copyValueCeil`) is about the RAW value and is conservative on purpose, because JSON escaping
expands the text in a content-dependent way: XML doubles, since every `<` becomes `<`.

---

## The caveat, which is the `done-with-caveat`

The fast vector index build loads every embedding into memory. On the real corpus that
**blew up an 8 GiB buffer pool** — 2.5 M entities are ~7.7 GB of raw vector before any index
structure — and production limits the pool to 1 GiB (`dbBufferPoolCeil`).

The recovery is `cache_embeddings := false`, attempted **only after the failure** and with a warning
in the log: 73 s against 383 s on 80,000 vectors, same answer quality. It is ~5x slower and it is
the difference between a store with semantic search and one without — which is why it is not a
silent downgrade.

**That recovery route was not validated end to end at 2.5 M**, because at that rate the build
projects ~3.3 hours. What is validated at that scale is the row load (321 s).

---

## The probes, and what each one supports

They stay in the repository, all behind an environment variable — none of them runs in `make ci`.
They exist because several constants and orderings in the production code are not derivable by
reading: without the probe, whoever finds `copyValueCeil` has a comment and no way to reproduce
the number.

| probe | guard | what it supports |
|---|---|---|
| `vector_bulk_load_probe_test.go` | `GRAPHIT_VEC_BULK` | `COPY` instead of `UNWIND` (24x with a vector column) |
| `vector_index_order_probe_test.go` | `GRAPHIT_VEC_ORDER` | vector index built AFTER the load (3.1x, and superlinear the other way) |
| `vector_index_memory_probe_test.go` | `GRAPHIT_VEC_MEM` | the retry with `cache_embeddings := false`, and its cost |
| `copy_json_row_size_test.go` | `GRAPHIT_COPY_ROWSIZE` | `copyValueCeil` — the ceiling is per VALUE, not per document |
| `copy_format_value_ceiling_test.go` | `GRAPHIT_COPY_ROWSIZE` | Parquet loads the value that JSON and CSV refuse |
| `copy_table_export_probe_test.go` | `GRAPHIT_COPY_ROWSIZE` | `RETURN n.*` and the PK prefix on the edges |
| `copy_parquet_fidelity_test.go` | `GRAPHIT_COPY_ROWSIZE` | POSITIONAL mapping, and the loss on non-UTF-8 bytes |
| `copy_format_probe_test.go` | `GRAPHIT_COPY_FORMATS` | Parquet/CSV/JSON tie on load; `COPY` × `UNWIND`; glob |
| `fts_scaling_probe_test.go` | `GRAPHIT_FTS_SCALING` | cost of `CREATE_FTS_INDEX` by size; `ATTACH (dbtype lbug)` |
| `fts_hotcold_probe_test.go` | `GRAPHIT_FTS_SCALING` | `CREATE_FTS_INDEX` is O(table); FTS does not cross `ATTACH` |
| `fts_shape_probe_test.go` | `GRAPHIT_FTS_SCALING` | multi-property index; the inverted index is not a table |
| `shard_parquet_size_probe_test.go` | `GRAPHIT_SHARD_SIZE` | why the LOCAL shards remain JSON |
| `real_corpus_incremental_probe_test.go` | `GRAPHIT_REAL_CACHE` | rebuild and incremental per phase over a real cache shard |

## What remains open

The **incremental** at 1178 s is untouched by anything here: there the insert covers only the files
that changed, and the cost is the DROP+CREATE of the nine FTS indexes, which is O(corpus) because
the engine has no partial build. Measured that `CREATE_FTS_INDEX` costs O(rows *of that table*) and
not O(database) — with 400,000 cold rows intact alongside, rebuilding 5 indexes of a 300-row table
takes 1.64 s —, which supports a two-segment design with tombstones and periodic merge. Its own
task.
