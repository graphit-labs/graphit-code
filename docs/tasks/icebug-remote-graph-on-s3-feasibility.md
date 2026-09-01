---
title: "Feasibility: icebug-disk + S3 to query an imported context without downloading the graph"
status: done
created: 2026-08-20
updated: 2026-08-21
tags: [ast, hub, ladybug, icebug, parquet, s3, distribution, investigation]
---

# Feasibility: icebug-disk + S3 to query an imported context without downloading the graph

## Objective

The Engineer asked whether it is possible to:

1. use LadybugDB's **icebug** format with storage on **S3**, so that **every
   query against an imported context happens on-the-fly** — without downloading the full graph to
   the local machine;
2. make the **export to the Hub** (which would now export to S3) use the
   **icebug-format tools**;
3. and have that work **100% on Windows, macOS and Linux, with self-contained
   dependencies**.

Reference doc provided by the Engineer: <https://docs.ladybugdb.com/import/icebug/>.

This is a feasibility investigation: the product is an honest verdict, with the
measured/verified blockers kept separate from the ones that are only assumption, and the minimal
design that would satisfy the request should the Engineer want to proceed. **No code is changed in
this task.**

### Input reasoning

What was already known from this repository before investigating (memory + wiki):

- A Hub AST artifact **already ships in Parquet** per table since 2026-08-18
  (`docs/tasks/ast-artifact-ships-parquet-tables.md`). Installing is a `COPY` per table, not a
  rebuild.
- An AST store is **TWO stores**: the graph (LadybugDB, `graph/` bundle) and the search
  index (**SQLite** FTS5+vec0, `search/` bundle). File text and embeddings live
  on the **SQLite** side — `internal/ast/parquet_transfer.go` says explicitly that without the
  `search/` bundle the context "can be traversed but neither searched nor read".
- The decision **not** to ship the database file was deliberate: on-disk layout is engine format,
  with a version that breaks between releases and in-place upgrades.
- `ATTACH ... (dbtype lbug)` works, but **FTS does not cross the attach** (measured on
  2026-08-16, liblbug 0.18.2).

So the question splits into two asymmetric halves, and that is what the investigation needs to
separate: the **graph** can plausibly become remote; the **search index** is SQLite and has no
remote equivalent.

## Plan & Task Breakdown

- [x] **T1 — Read the icebug doc and extract what the format requires** — Spec: what
  icebug-disk/icebug-memory are, how they are generated, which remote URIs they support, which
  extension is needed. Acceptance: know whether S3 is supported and through which path.
- [x] **T2 — Map the current state of AST artifact distribution** — Spec:
  `internal/ladybugstore/transfer.go`, `internal/sqlitestore/transfer.go`,
  `internal/ast/parquet_transfer.go`. Acceptance: know exactly what ships today and in which
  format.
- [x] **T3 — Verify whether `icebug-format` accepts input this project can produce**
  — Spec: the doc only documents `--source-db <duckdb>` and `--graphar <archive>`; this project has
  LadybugDB + Parquet per table. Acceptance: know whether there is a path without DuckDB/GraphAr,
  and what the runtime dependency is (uvx/Python).
- [x] **T4 — Verify the `httpfs` extension: offline install and platform coverage** —
  Spec: `INSTALL` downloads from the official extension server; `LOAD EXTENSION '<path>'` loads from
  a file. Acceptance: know whether the extension binary can be pre-embedded per platform, as is
  already done with grammars.
- [ ] **T5 — Evaluate the cost of remote querying** — NOT MEASURED, and declared as such: moved
  to T15 of the migration log, where there is a real artifact to measure against. — Spec: the queries of this framework are
- [x] **T6 — Map what the Hub does today (git) vs. S3** — Done, and the number that matters: the
  GitStore has **five** responsibilities, not one (registry, orphan branch per artifact,
  `refs/events/*`, rules on `main`, memory worktrees). Mapping to object prefixes
  in `docs/specs/hub-s3-object-layout.md`.
- [x] **T7 — Write the verdict and the minimal design** — Done, and the Engineer decided based
  on it: see `hub-em-s3-icebug-e-lancedb.md`.

## Progress Log

### 2026-08-21

- T3, T4, T6 and T7 closed; T5 explicitly NOT measured and moved to the migration log.
  The Engineer approved moving on to implementation, so this document becomes history.

### 2026-08-20

- Log opened. T1 and T2 were completed before the log was opened (the investigation was already
  under way when the need for the record was identified) — content consolidated below in
  "Findings", which is updated as T3–T7 advance.

## Findings

### T1 — What icebug is, and what it requires

From the doc (<https://docs.ladybugdb.com/import/icebug/>, read on 2026-08-20; content
paraphrased):

- **Two flavors.** `icebug-disk` is Ladybug's native "graph-aware" Parquet;
  `icebug-memory` is Arrow. Both store the graph in **CSR**.
- **How it is generated.** With the `icebug-format` tool, invoked as `uvx icebug-format`, from
  either (a) a **DuckDB** database or (b) a **GraphAr** file. The output is a directory of
  Parquet plus a `schema.cypher`.
- **How it is used.** The `schema.cypher` declares
  `CREATE NODE TABLE ... WITH (storage = '<dir>', format = 'icebug-disk')`. Ladybug is brought up
  with `-i schema.cypher`, or the DDL is run against the instance.
- **Remote is supported out of the box.** `storage` accepts a URI: `s3://`, `gcs://`, `az://`,
  `xet://`, `https://`. S3/GCS/HTTPS require the **httpfs** extension; Azure requires **azure**.
- **S3 credentials** via `CALL s3_credential(key_id=..., secret=..., region=...)` after
  `INSTALL httpfs; LOAD httpfs;`.

Direct consequence: **Ladybug really does know how to query graph tables in Parquet on S3
without ingestion.** The "query without downloading" part is not invented — it is a documented
feature.

### T2 — What this project distributes today

| Half | Engine | Bundle | What it carries |
|---|---|---|---|
| graph | LadybugDB | `graph/` (`tables.json` + 1 Parquet per table/pair) | nodes and edges |
| search | SQLite | `search/` (`tables.json` + Parquet per base table) | **file text**, embeddings |

Neither one carries an index: FTS5, vec0 and the vector index are engine structure and the
consumer builds them (`internal/ast/parquet_transfer.go`, `ImportGraphFromParquet`).

Today's Parquet is **per table, flat** — generated by `COPY ... TO` with `RETURN n.*` so as not
to express column order. It is not CSR and it is not icebug: icebug is a layout with `indptr` /
`indices`, so the current Parquet is **not** reusable as icebug without conversion.

### T3 — `icebug-format` accepts input this project can produce: YES

Besides `--source-db <duckdb>` and `--graphar`, the tool accepts **`--source-dir` with a directory
of Parquet** of vertex/edge pairs. That is exactly the shape of what
`ladybugstore.ExportTables` already produces, so **there is no need to go through DuckDB or
GraphAr**.

What does NOT exist: a writer in Go. The official implementation is Python (PyArrow, DuckDB and
DataFusion backends) and no other language has published bindings. Runtime dependency:
Python + `uv`, **only on the machine that publishes**.

### T4 — `httpfs` offline: YES, per file

`INSTALL httpfs` downloads from the official server into `~/.lbug/extensions`, but there is
`LOAD EXTENSION '<caminho>.lbug_extension'`, documented for locally compiled
extensions. That allows pre-embedding the binary per platform in the launcher payload — which
already does exactly that with `liblbug`, ONNX Runtime, ICU and the grammar YAMLs — and never
touching the network on a query.

### T5, T6, T7 — answered by the decision, not by the investigation

The Engineer decided to move on with the migration before T5–T7 were written up as a study. The
verdict, the design and the execution plan are in
[hub-em-s3-icebug-e-lancedb.md](hub-em-s3-icebug-e-lancedb.md), which **supersedes this
document**. In short: feasible, with one half the investigation had not foreseen — the search
index, which had no remote equivalent in SQLite, got one in **LanceDB**, which has an official Go
SDK and queries `s3://` directly.

## Technical Debt

- [ ] icebug-disk writer in Go (item in the improvements backlog; see the migration log).

## System Knowledge

- Consolidated in the migration log and in memory. The finding that changes the design most:
  `--source-dir` removes DuckDB from the path, and LanceDB solves the half the investigation
  had marked as blocked.
