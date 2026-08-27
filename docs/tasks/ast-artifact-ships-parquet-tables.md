---
title: The AST artifact will now carry the tables in Parquet, and installation will no longer require a rebuild.
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [ast, hub, parquet, performance, artifact]
---

# AST Artifact takes the tables, not the shards

**Source:** measure why installing a Hub AST context is expensive. `ensureASTStore` did
`NewShardCache` + `NewShardEmbCache` + `RebuildFromJSON` + `BuildSearchIndexFor` — that is,
**installing meant running the full rebuild**, the same cost that
`search-index-copy-load-and-late-vector-index.md` measured in hours on a large corpus. And each consumer paid separately for a result that the version had already frozen.

---

## What's changed

Publishing exports the **already constructed** graph: one Parquet file per table — per FROM-TO in the relationship tables — in `graph/`, with a `graph/tables.json` manifest that also loads the exact `CREATE` from the source store.

Install one `COPY` per table.

| | shards (before) | Parquet (now) |
|---|---|---|
| artifact size | 394.6 MB | **121.3 MB** (3.25x smaller) |
| consumer work | parse JSON, transpose, load | `COPY` per table |
| where the transposition runs | once **per consumer** | once, in the publisher |

Measured in this repository's store, 132 tables and 316,828 rows, including file text
and embeddings.

---

## The four things the engine forced, and how each one was addressed

**1. `COPY` maps columns by POSITION, not by name.** A file with columns out of order only fails when types clash; if types are compatible, it silently writes to the wrong column. The answer isn't discipline, but rather that the order is not expressed: the export uses `RETURN n.*`, which expands in the order declared by the table, and the import does not list any columns. Neither side enumerates it.

**2. Edge needs keys before properties.** `RETURN r.*` only exports the relation's
properties, and the `COPY` rejects it with `Number of columns mismatch. Expected 4 but
got 2`. The export prefixes the nodes' PKs — `RETURN a.<pk>, b.<pk>, r.*` — and the names
of these PKs come from `show_connection`, which provides them as a pair.

3. A relation table with multiple pairs requires the named pair. The engine rejects the ambiguous form:
  * "The table REFS has multiple FROM and TO pairs defined in the schema."*
  * Import always `COPY R FROM 'f' (from='A', to='B')` **should not** just when there are multiple pairs — a schema that gains a second pair later cannot silently change what an existing bundle means.

4. The schema is not derivable on the consumer. The graph DDL comes from the source, assembled from the parse — which labels exist, which pairs a relation joins. Someone replicating a bundle does not have the parse. Therefore, the manifest loads the reconstructed DDL from the source, and the bundle is self-descriptive.

---

## What does NOT travel

**FTS and vector indexes.** These are engine structures, not data, so the consumer still builds them — in the same order as a local rebuild uses and for the same measured reasons: vector after loading (3.1x cheaper than maintaining it during), FTS last (the engine does not maintain it during insert).

Installing stops being a rebuild without becoming free.

**The database file.** It is the obvious alternative and it's worse: the on-disk layout belongs to the engine and has format versions that break between releases — v0.17.0 documents that an updated database cannot be opened in a previous binary, and that the upgrade is **in-place**. A newer consumer would silently mutate a shared, versioned store that other projects point to. Parquet is not a storage engine format, so the artifact survives engine version changes.

---

## Compatibility

`prepareASTPublish` falls back to the shards when the store does not exist or cannot be exported,
and `ensureASTStore` checks for the presence of `graph/tables.json` — written **last**,
so its presence means that the export has finished. Artifacts published before this change
remain installable via the old path, without migration.

---

## Tests

- `TestParquetRoundTripPreservesGraph` — builds a graph with the **pipeline** (not fixture),
  exports it, imports it into an empty store, and compares it table by table. Equal counts are not enough: a
  positional mismatch preserves the count but shifts all values, so there is also a full row comparison in `File` and `Function`.
- `TestPrepareASTPublishPrefersParquet` — the constructed store publishes Parquet and does **not** publish
  shards together (otherwise the artifact would double); a store without a database falls back to shards.

`internal/ast` and `internal/hub` are green.

---

## Pending

The `.emb.json` of the local cache remains decimal text: measured at **9.552 B per vector
versus 3.072 binary, 3.11x**, and it is **69% of the 8.8 GB** of a large corpus shard cache.
This is from the local cache, not the artifact, and is not touched here. `NewShardEmbCache` also
loads all embedding shards into memory during construction, which is part of the
cache startup time. Both things are separate tasks.
