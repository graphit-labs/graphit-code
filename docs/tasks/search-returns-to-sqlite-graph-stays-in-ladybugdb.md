---
Title: Text Search Returns to SQLite; LadybugDB Is Left with Graph Only
status: done
created: 2026-08-19
updated: 2026-08-19
tags: [ast, wiki, memory, sqlite, ladybug, search, fts, parquet, hub, storage]
---

# Text Search Returns to SQLite

Origin: an instruction from the Engineer — "I'm having a lot of problems with Ladybug for search: everything depends on the indexes, and embedding and FTS are both very limited and slow. Everything that's FTS, embedding, trigram, and anything else answering for text search needs to be done in SQLite. Ladybug keeps the graph, and SQLite gets the text search."

This reverses the direction of the earlier decision to consolidate search into LadybugDB and drop SQLite (see `consolidate-search-into-ladybugdb-and-drop-sqlite.md`), which had been made on August 16, 2026, just three days earlier.

---

## Why

One measured fact settles this on its own: LadybugDB's library doesn't maintain its FTS index incrementally — inserting a single row forces a full rebuild of the index from scratch after every write. That makes an update cost `O(corpus)` instead of `O(1)`.

On the real corpus, 39,429 files and 2,501,342 entities:

| | LadybugDB | SQLite |
|---|---|---|
| full rebuild | 988 s | — |
| incremental, ONE file | **1,178 s** | ~300 ms |

Incrementally updating a single file was slower than rebuilding everything from scratch. That number alone is the whole argument.

## What Was NOT Done: a Rollback

The Engineer was explicit: "this is not just reverting to the previous commits." The project has moved on since then — you can go back and look at how SQLite was used before, but that's a reference, not a target: today's code is different, with improvements and fixes made along the way.

So the old SQLite-era code served as a reference for the SQL mechanics, not as a destination to restore. What's left is today's state, with the storage layer swapped out underneath it:

| What | Survived untouched |
|---|---|
| `internal/ast/search_common.go` | Deliberately storage-independent — tokenization, trigram bag, RRF, determinism. This is exactly what made moving to a single rewritten layer, and moving back, both possible without touching it. |
| Fusion (`search_fusion.go`, `store_query.go`) | Fixes three measured defects that a plain `bm25()` doesn't express. It carried over across both engines unchanged. |
| Logical schema | `name_split` / `name_tri` / `etype`, and the precomputed trigram bag. |
| Ladybug probes | Kept, inverted on purpose: they pass only while the upstream bug is still present, so they'll fail the day it's fixed. |

No backward compatibility — the Engineer's call: "we're still in development." No fallback for an artifact published in the old format, and no migration path for an existing store.

---

## The Three Design Decisions

**1. The index updates in place.**

It doesn't follow the copy+swap technique used for the graph. That's what brings the index update down to roughly 50 milliseconds on this repository's corpus (measured, see below), against the 1,178 seconds the previous arrangement paid for a single file.

The cost is real, known, and accepted: for the width of an update, a reader might see the new index paired with the old graph — or, if the swap fails, paired with a graph that never changed. Neither case is corruption, and the next incremental update fixes it. Paying `O(corpus)` on every edit just to close that window is exactly the trade-off being rejected here.

**2. Searchable text lives in base tables.**

It used to be the opposite: the FTS5 virtual table **was** the storage, with the source text living inside it. That changed because the Hub now persists this as a **Parquet** artifact, and a virtual table is engine-internal structure — there's nothing in it you can export. So:

```
files, entities, entity_emb          base tables — what travels
file_fts, entity_fts, file_tri,      external-content FTS5 over them, maintained by triggers
entity_tri
entity vector (vec0)                 vector index built from entity embeddings
```

Triggers are the mechanism the other engine lacked: SQLite maintains the FTS5 index incrementally as rows arrive, instead of tearing everything down and rebuilding it afterward.

**3. Leave only the search workload in the graph.**

Source file, matching fields, and vectors labeled by the grammar's declarations — none of that moves. The graph keeps its nodes and edges exactly as they are, **including `Comment.name`**, which stays reachable through Cypher and not just through text search, as the ast skill documents.

---

## The Hub: SQLite Loads from Parquet

A new requirement that didn't exist before `fb19403`. `internal/ladybugstore/transfer.go` moves LadybugDB's tables; now there's a twin, `internal/sqlitestore/transfer.go`, driven by `sqlite_master`, with the same shape and the same two refusals:

- Indexes don't travel. FTS5 and vec0 are engine-internal structure; the consumer rebuilds them locally — this is also what forced the schema to have base tables in the first place.
- The database file itself doesn't travel. A `.sqlite` file carries page format and a set of compiled extension modules; a consumer without the matching build would open it and find unreadable tables.

An AST artifact now ships two bundles, `graph/` and `search/`, in separate directories, because either half can legitimately be missing, and with a single manifest that would go undetected.

Columns are matched by name on both sides, never by position. The graph bundle learned this the hard way, in the opposite direction: its Parquet loader used to map by position and silently write into the wrong column whenever two types happened to be compatible.

The identifiers in the manifest are validated against the actual table schema. A manifest is not a trusted input — it arrives inside an artifact someone else published, and the loader builds its `INSERT` using the column names it actually finds there.

---

## What Came Out of Going Back to FTS5: the Prefix Index

LadybugDB has no prefix matching or wildcards (`conf*` matches nothing). The trigram bag was built to stand in for it. Now that FTS5 is back, both exist side by side, and they answer different questions: the prefix index reaches a term that genuinely starts with the given string, while the bag scores partial overlap anywhere in the name.

Measured in this session, on the quality floor:

| | without prefix pass | with |
|---|---|---|
| lexical floor | 12/16 | **13/16** |
| truncation | 9/9 | 9/9 |

The 13th probe is `validate` → `Validator`: they tie exactly on the name field alone, and nothing else in the fusion tells them apart. The prefix index breaks the tie — the query is a prefix of "validate" and only a substring of "Validator".

The truncation test's own documentation had to be corrected too: earlier revisions reported 9/9 with an occasional drop to 8/9, whenever the prefix pass failed to reach the truncated form of a term.

## A Bloat Defect Solved by Construction

`vec0` allocates fixed blocks of 1,024 rows and never returns the space from a deleted row — this is what drove one index up to 20 GB while holding only 760 MB of live data, and the only remedy was writing an entirely new file.

Now the vectors live in `entity_emb`, an ordinary table. `compactVectorIndexIfNeeded` counts the dead rows and rebuilds `entity_vec` from it once they cross `max(4096, 25% of live rows)` — cheap, since it doesn't reload the model, and rare enough that the common case stays in milliseconds.

---

## Files

| file | what |
|---|---|
| `internal/ast/search_sqlite.go` | New — schema, writes, and low-level queries |
| `internal/ast/search_fusion.go` | New — the fusion logic, extracted from `search_query.go`, now storage-independent |
| `internal/ladybugstore/search_*.go` (FTS/vec0 code) | Removed — the LadybugDB-based implementation |
| `internal/ast/store_query.go` (`QueryRecord` accessors) | Added and removed within the same session — no longer used |
| `internal/wiki/store.go`, `store_query.go` | rewritten on top of SQLite |
| `internal/wiki/values.go` | **new** — row accessors and the parser for the sqlite-vec format |
| `internal/sqlitestore/transfer.go` | new — Parquet ↔ SQLite, twin of `ladybugstore` |
| `internal/ast/parquet_transfer.go` | exports and imports both halves |
| search index (FTS5/trigram) | updated in place again, not copy+swap |
| `entity_vec` | rebuilt AFTER the swap |
| buffer pool ceiling | the measurement that justified it no longer holds; the number stays, but the comment now says it's unmeasured |
| `Makefile`, `.golangci.yml`, `fts5_required.go` (x2) | the `fts5` build tag and its guards are back |

Removed probes that measured things that no longer exist: `fts_bufferpool_probe_test.go`, `fts_hotcold_probe_test.go`, `fts_scaling_probe_test.go`, `fts_shape_probe_test.go`, `search_copy_load_test.go`, `search_emb_mixed_batch_test.go`, `search_oversized_source_test.go` — all of them about the FTS build, the `COPY`, and the mixed vector batch **inside LadybugDB**.

## State

Full green suite with `-count=1` across 40 packages. `go vet` and `golangci-lint` clean (0 issues). Quality floor 13/16, truncation 9/9, abbreviation recall 4/4.

### The Incremental Cost, MEASURED on the Real Store (2026-08-19, after `make install`)

Repository corpus: 737 files, 58,500 entities, 36,674 vectors.

| phase | cost |
|---|---|
| search index update, one file / 46 entities (three runs) | 69 ms / 51 ms / 48 ms |
| copying the graph directory (95 MB, warm page cache) | 31 ms |
| end-to-end incremental, via the CLI | **7.2 s / 8.6 s / 17.6 s** |

The search index is only 0.3–0.7% of the increment's total cost. The ~50 ms is in the same order of magnitude as SQLite's historical figure (~300 ms) — and actually better, since external-content triggers kept the FTS index in sync at no extra cost, which was the whole bet behind this design.

What's left — 7 to 17 seconds, with that much variation — is the graph's "copy + swap": the insert into LadybugDB, plus `Shutdown` and `Close` on the mutated copy, which is exactly what `IncrementalRebuild`'s own doc comment already flagged as ranging from 215 milliseconds to 5 seconds — and this measurement confirms it's larger and more variable today than that range suggests. None of this was touched by this piece of work.

The consequence for anyone optimizing next: the incremental bottleneck isn't the search index anymore. Attacking the search index won't move the number; attacking the graph copy's shutdown/close path will.

Probe: `internal/ast/incremental_cost_probe_test.go`, skipped unless `GRAPHIT_COST_PROBE_STORE` is set.

---

## References

- [Storage Layout](../architecture/storage_layout.md#two-engines-and-which-one-owns-what)
- [Consolidate search into LadybugDB and drop SQLite](./consolidate-search-into-ladybugdb-and-drop-sqlite.md) — the direction this work reverses
- [AST artifact ships Parquet tables](./ast-artifact-ships-parquet-tables.md)
- [Knowledge artifact, same thing](./knowledge-artifact-ships-parquet-tables.md)
