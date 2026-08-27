---
title: Artifact of knowledge brings tables into Parquet, and the mechanism becomes one.
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [hub, knowledge, wiki, parquet, ladybugstore, refactor]
---

# Knowledge usa o mesmo mecanismo do AST

Origin: Observation by the Engineer. An artifact of `knowledge` will go to the Hub and is versioned in the same way as one of `ast`, and it also has embeddings—so it should have the same mechanism.

My initial response looked at the **size** and said it didn't matter: the largest wiki of knowledge on this machine has 19 MB, while the shards of the AST are 8.8 GB. The right criterion is not that one, and the correction came from the Engineer.

---

The criterion is mutability, not size.

Translation:

```markdown
| | memory | knowledge |
|---|---|---|
| the source `.md` travels | Yes — it is truth, written from both sides | No — does not change for the consumer |
| writers | many | one — the publishing project |
| the consumer | recompiles from the source | only indexes |
```

What makes the mechanism valid is that the artifact must be **immutable, published by one public publisher and fixed by version** — exactly as the doc already said about the AST store: "Nothing a consuming project does can change the result, and the version pins it." Knowledge has this property; **memory doesn't have, and never could have**: a consumer adds and pushes, so another table built would overwrite something, not extend it.

---

What has changed

**`internal/ladybugstore/transfer.go`, new.** Export and import tables from the catalog — **INLINE_4**, **INLINE_5**, **INLINE_6** — without knowing anything about AST or wiki. It exists because `internal/wiki` and `internal/ast` cannot be imported, which is the reason for the package's existence.

This translation preserves the original meaning while adapting it to idiomatic English.

Each module retains what is its own: **the indices of the tables it needs and in what order**.

```markdown
| | AST | wiki |
|---|---|---|
| export | `ast.ExportGraphToParquet` | `wiki.ExportToParquet` |
| import | `ast.ImportGraphFromParquet` | `wiki.ImportFromParquet` |
| indices after load | `ensureVectorIndex` + `rebuildFTSIndexes` | `ensureWikiVectorIndex` + `buildWikiFTSIndexes` |
```

Publication of knowledge (`prepareKnowledgePublish`): exports the tables and loads the pages.
The pages continue to travel — they are what a reader opens, and nothing reconstructs them without the sources, which remain in the publisher's repository. The shards take their place as tables; `wiki.db` continues to be derived but does not travel.

Installation of Knowledge (`internal/hub/service.go`): present bundle → `ImportFromParquet`;
otherwise → `BuildDBFromCache`, which is the always-used path and what keeps all that was published before.

---

There is an issue found in the passage, it's the same as before

It created __INLINE_22__ in the schema, or
otherwise before any line entered— while __INLINE_24__, right next to it, constructed
with purpose and explained why in a comment.

It is the same defect fixed in the AST at `search-index-copy-load-and-late-vector-index.md`, with the same measurement behind: constructing the vector index on lines already present cost **3.1 times less**, and the opposite arrangement worsens with size (10.088 µs per line for 2000 against 17.202 µs per line for 20000), because each insert maintains an HNSW.

Extracted for `ensureWikiVectorIndex`, called after the writings in `Rebuild` and on import.
On a wiki, this is worth little in seconds; it's valuable for consistency and because the import loads in lots, which is exactly where the difference appears.

---

## Testes

- `TestWikiParquetRoundTrip` builds a wiki, exports it to an empty directory, and verifies counts, cross-references, sync logs, **checking for working lines that arrived instead of the lines this process wrote**, and fields line by line. Alone, count would not catch the defect that import maps: `COPY` positions map, and a mismatch preserves all counts and shifts values one column.
- `TestParquetRoundTripPreservesGraph` ported to shared package without changing behavior.

`internal/ast`, `internal/hub`, `internal/wiki` e `internal/knowledge` verdes.

---

The gain is asymmetric, and that's honest to say.

An artifact of AST is from 394.6 MB shards to 121.3 MB, and installing it no longer requires a full rebuild. A knowledge wiki has few megabytes anyway and indexes in seconds.

Knowledge accepts the mechanism because the form is identical and a single path is easier to maintain correctly than two — not because of the numbers required. If a corpus of knowledge grows, the mechanism already exists.
