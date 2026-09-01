---
title: The knowledge artifact ships the tables in Parquet, and the mechanism becomes a single one
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [hub, knowledge, wiki, parquet, ladybugstore, refactor]
---

# Knowledge uses the same mechanism as the AST

**Origin:** the Engineer's observation. A `knowledge` artifact goes to the Hub and is
versioned in the same way as an `ast` one, and it also has embeddings — so it should have the same
mechanism.

My initial answer looked at **size** and said it was not worth it: the largest knowledge
wiki on this machine is 19 MB, against 8.8 GB of AST shards. That is not the right
criterion, and the correction came from the Engineer.

---

## The criterion is mutability, not size

| | memory | knowledge |
|---|---|---|
| the `.md` source travels | **yes** — it is the truth, written from both sides | no — it does not change on the consumer |
| writers | **many** | **one** — the publishing project |
| the consumer | recompiles from the source | **only indexes** |

What makes the mechanism valid is the artifact being **immutable, from a single publisher and pinned
by the version** — exactly what the doc already said about the AST store: *"Nothing a consuming
project does can change the result, and the version pins it"*. Knowledge has that
property; **memory does not have it, and never can**: a consumer adds and pushes, so
a table built by someone else would be something to overwrite, not to extend.

---

## What changed

**`internal/ladybugstore/transfer.go`, new.** Export and import of tables from the
catalog — `show_tables`, `table_info`, `show_connection` — without knowing anything about the AST or
about the wiki. It is there because `internal/wiki` and `internal/ast` cannot import each other, which is the
reason the package exists.

Each module keeps what is its own: **which indexes its tables need, and in what order.**

| | AST | wiki |
|---|---|---|
| export | `ast.ExportGraphToParquet` | `wiki.ExportToParquet` |
| import | `ast.ImportGraphFromParquet` | `wiki.ImportFromParquet` |
| indexes after the load | `ensureVectorIndex` + `rebuildFTSIndexes` | `ensureWikiVectorIndex` + `buildWikiFTSIndexes` |

**Knowledge publication** (`prepareKnowledgePublish`): exports the tables and carries the
**pages**. The pages keep travelling — they are what a reader opens, and nothing
rebuilds them without the sources, which stay in the publisher's repository. The shards give way to the
tables; `wiki.db` remains derived and does not travel.

**Knowledge installation** (`internal/hub/service.go`): bundle present → `ImportFromParquet`;
otherwise → `BuildDBFromCache`, which is the usual path and what keeps everything published
before still installable.

---

## A defect found in passing, and it is the same one as before

`ensureWikiSchema` created `CREATE_VECTOR_INDEX('WikiChunk','wc_vec','emb')` **in the schema**, that
is, before any row went in — while `buildWikiFTSIndexes`, right next to it, built
afterwards on purpose and explained why in the comment.

It is the same defect fixed in the AST in `search-index-copy-load-and-late-vector-index.md`, with
the same measurement behind it: building the vector index over rows already present cost **3.1x
less**, and the opposite arrangement **gets worse with size** (10.088 µs/row at 2.000 against 17.202 at
20.000), because each insert maintains an HNSW.

Extracted into `ensureWikiVectorIndex`, called after the writes in `Rebuild` and in the import.
On a wiki this is worth little in seconds; it is worth it for consistency and because the import loads in
batches, which is exactly where the difference shows up.

---

## Tests

- `TestWikiParquetRoundTrip` — builds a wiki, exports, imports into an empty one, and checks
  counts, cross-refs, sync log, **search working** over rows that arrived instead of
  rows this process wrote, and one row field by field. Counting alone would not catch
  the defect that matters: `COPY` maps by position, and a mismatch preserves every count
  and shifts the values by one column.
- `TestParquetRoundTripPreservesGraph` — ported to the shared package, with no change in
  behaviour.

`internal/ast`, `internal/hub`, `internal/wiki` and `internal/knowledge` green.

---

## The gain is asymmetric, and it is honest to say so

An AST artifact goes from 394.6 MB of shards to 121.3 MB, and installing it is no longer a
full rebuild. A knowledge wiki has a few megabytes either way and reindexes in
seconds.

Knowledge gets the mechanism because the **shape** is identical and one path is cheaper to
keep correct than two — not because the numbers demanded it. If a knowledge corpus grows,
the mechanism is already there.
