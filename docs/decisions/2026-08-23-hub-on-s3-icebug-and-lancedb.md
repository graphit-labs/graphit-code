# Hub moves off git; search moves off SQLite; published artifact is mounted, not downloaded

- **Date:** 2026-08-23
- **Status:** accepted and implemented
- **Scope:** `internal/hub`, `internal/s3store`, `internal/lancestore`, `internal/ast`,
  `internal/wiki`, `internal/memory`, `Makefile`, `.github/workflows`
- **Task history:** Graphit Task `tsk-c049ad9ad5b7`

## What changed

Three mutually supporting decisions, taken as one:

1. **Hub ceases to be a git repository and becomes an S3 object prefix.** The registry, each artifact version, telemetry, rule distribution, and memory stores were five uses of git; now they are five key prefixes.
2. **Search ceases to be SQLite/FTS5 and becomes LanceDB, everywhere** — code graph, knowledge wiki, and memory wiki, local and published. **5,737 lines removed.**
3. **A published artifact is MOUNTED, not downloaded.** The graph travels as icebug and the search index as a LanceDB directory; installing registers the location and runs DDL. No data bytes are downloaded.

## Why

### Git was never the right mechanism for this

An artifact is immutable and version-addressed. Git gives history, merge, and conflict resolution — none of which is used here — and charges for them: one clone per consumer, one orphan branch per version, and a working tree that may be dirty. What was wanted was "read this object," which is what an object store does.

Two properties change shape for callers:

- **there is no commit.** Every write is durable when it returns, so there is no push and no working tree. What was commit atomicity became **ordering**: an artifact's prefix is uploaded BEFORE the registry entry names it. The inverse inconsistency — a prefix nothing points to — is just wasted bytes;
- **there is no clone.** A mountable artifact is never downloaded.

### SQLite does not survive "queryable on-the-fly"

A published artifact must be queryable over S3 without download. A SQLite file is not queryable that way: it carries a page format and a set of compiled modules, so a consumer without `vec0` — or with a different FTS5 — opens it and finds tables it cannot read.

And a published wiki has no `.md` on disk. Page text had to come from the index, which requires an index the engine can open remotely.

### Why not keep both

Considered and rejected: two backends mean **two relevances**, and which one the user hits depends on where the graph came from. For the same reason, the two publication fallbacks (Parquet bundle and shards) were deleted instead of kept — together they made an artifact's behavior depend on which path it happened to take, and a consumer had no way to know whether its context was mounted or copied.

## Consequences, and the price of each

| gain | price |
|---|---|
| installing transfers no data | DDL is downloaded (few KB), and without it there is nothing to point at objects |
| installing does not rebuild index | index travels inside the directory, so artifact is larger |
| single relevance, from the engine | fusion in Go (331 lines) was deleted; what the engine does not express is not done |
| no SQLite, no `fts5` tag | `lancedb` tag enters, which is **more** expensive: native Rust, no cross-compile |

### The build tag changed character, not just name

`fts5` was a compiler flag over vendored C. `lancedb` is a library built from Rust source for the host, which **does not cross-compile** — so release now runs one job per platform, and `build-all` ceased to exist as single-machine work.

**There is no guard file this time**, and it is a decision: `ErrNotBuilt` already names the tag and the fix, which is exactly what `no such module: fts5` did not do. What was fixed is failure being loud instead of silent — `NewQueryService` swallowed the open error and returned `nil, nil`, indistinguishable from a correct empty result.

### Format gaps were ACCEPTED, not resolved

By Engineer decision, with what exists being sufficient:

- **multi-hop traversal** over a mounted graph is weaker than over native;
- **one edge table holds one CSR**, so it declares exactly one FROM/TO pair. This graph has ~97 distinct pairs, so every label is folded into an `Entity` table with label as a column. `MATCH (n:Function)` against a mounted context must be `MATCH (n:Entity {label:'Function'})`.

The second failure is **loud**, not silent: the message names which tables exist. Measured.

Both are declared at the top of `internal/ast/icebug_transfer.go`, not only here, because that is where a caller finds them.

## What memory lost with git, and how it was restored

| what git gave | how it is now |
|---|---|
| user identity (`git config user.email`) | `unit.id` in global config — a ULID generated once |
| version history | frontmatter (`revision`, `previous`, `updated_by`) + `history/<id>/NNNN.md` |
| ignore boundary (searching for `.git` upward) | project root — the old boundary was **collected and inert** |

No backward compatibility and no preservation of old data, by explicit decision.

## How it was verified

- **entire suite green** with `-tags lancedb`, and `go build ./...` compiles with no tag;
- **publish and read over the network**, against real MinIO: `reading on-the-fly from s3://…/index.lance`;
- **full graph cycle**: index real source, publish, mount, query — `published bundle holds 7 data files; the local catalog holds 0`;
- **`$ORIGIN` of payload**, triggered by hiding the absolute rpath target: loader resolves via the binary's own directory.

**Not verified:** darwin and windows builds, due to lack of `clang` and mingw on the machine where this was done. The first workflow run is their test.

## Premise corrections worth more than the result

Three things measurement overturned, and reasoning would not have:

1. **The search quality floor measured TIE-BREAKING.** 13/16 looked like a deficit; five of sixteen probes had no single defensible answer by the project's own rule. One session read the resulting 11/16 as loss and was one step away from building a cross-encoder to close a gap that did not exist. Rederived: 11/11 strict + 5/5 recall.
2. **`ms-marco-MiniLM` is ten times smaller, eight times faster, and WORSENS ranking.** By latency and size it was the obvious choice. And `jina-reranker-v2`, first choice by code retrieval benchmark, is `cc-by-nc-4.0` — **license is a requirement, not a footnote.**
3. **A vector written as `[]float32` does not come back as `[]float32`**, and a two-value type assertion **does not error, it returns nil**. The symptom was empty `StoredEmbeddings` while `EmbeddingStats` counted the same rows as embedded.
