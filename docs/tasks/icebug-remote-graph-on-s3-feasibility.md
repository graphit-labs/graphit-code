---
title: "Viabilidade: icebug-disk + S3 para consultar contexto importado sem baixar o grafo"
status: done
created: 2026-08-20
updated: 2026-08-21
tags: [ast, hub, ladybug, icebug, parquet, s3, distribution, investigation]
---

# Viabilidade: icebug-disk + S3 para consultar contexto importado sem baixar o grafo

## Objective

The Engineer asked if it is possible:

1. usar o formato **icebug** do LadybugDB com armazenamento em **S3**, de modo que **toda
   consulta a contexto importado seja on-the-fly** — sem baixar o grafo completo para a
machine in the vicinity; nearby machine
2. fazer o **export para o Hub** (que passaria a ser exportado para o S3) usar as
   **ferramentas icebug-format**;
And that it works 100% on Windows, macOS, and Linux with dependencies
   autocontidas**.

Reference document provided by the Engineer: <https://docs.ladybugdb.com/import/icebug/>.

This is an investigation of viability: the product is a truthful verdict with restrictions.
measured/verified separately from those that are only speculation, and the minimum design that would meet
Request if the Engineer wishes to proceed. **No code is altered in this task.**

Input reasoning

What is already known about this repository before investigating (memory + wiki):

A device from the ASSET hub has already traveled on a Parquet table since August 18, 2026.
Installing is done by default in table format, not
  rebuild.
A store of ASTs is **two stores**: the graph (LadybugDB, bundle INLINE_0) and the index.
  busca (**SQLite** FTS5+vec0, bundle `search/`). O texto dos arquivos e os embeddings vivem
  no lado **SQLite** — `internal/ast/parquet_transfer.go` diz explicitamente que sem o bundle
The context "can be traversed but not researched or read."
The decision not to send the database file was deliberate: disk layout is format of
Motor, with version that breaks between releases and upgrades in place.
It works, but FTS does not traverse the attach (measured in)
  2026-08-16, liblbug 0.18.2).

Therefore, the question splits into two asymmetrical halves, which is precisely what the investigation needs.
separate: the graph can probably be converted into a remote; the search index is SQLite and not
tem equivalente remoto.

## Plan & Task Breakdown

"**T1 - Read Icebug's document and extract what the format requires** – Spec: What is it?"
Icebug-disk and Icebug-memory are generated as follows, what remote URIs they support, and what extension is needed.
Acceptance: confirm whether S3 is supported and by what path.
- [ ] T2 - Map the current state of artifact distribution from the AST (Automated Software Tooling) - Specification:
  `internal/ladybugstore/transfer.go`, `internal/sqlitestore/transfer.go`,
  `internal/ast/parquet_transfer.go`. Aceite: saber exatamente o que viaja hoje e em qual
  formato.
- [x] **T3 — Verificar se `icebug-format` aceita entrada que este projeto consegue produzir**
— Spec: the doc only documents `--source-db <duckdb>` and `--graphar <archive>`; this project has
  LadybugDB + Parquet por tabela. Aceite: saber se existe caminho sem DuckDB/GraphAr, e qual
It is the runtime dependency (uvx/Python).
- [ ] T4 - Verify Inline 0 Extension: Offline Installation and Platform Coverage
Specification: `INSTALL` downloads from the official extension server; `LOAD EXTENSION '<path>'` loads from
File. Accept: knowing if it's possible to pre-burn the binary extension for platform compatibility, as already mentioned.
It is constructed with grammars.
- [ ] **T5 — Avaliar o custo de consulta remota** — NÃO MEDIDO, e declarado como tal: passou
For T15 of the migration log, where there is a real artifact against which to measure. — This specifies that the queries in this framework are
Done, and what matters most is the number: the

Translation: Done, and what counts most is the number: the
GitStore has five responsibilities, not one (registry, orphaned branch by artifact, etc.).
Mapping to object prefixes
  em `docs/specs/hub-s3-object-layout.md`.
Done, and the Engineer decided with
  base nele: ver `hub-em-s3-icebug-e-lancedb.md`.

## Progress Log

### 2026-08-21

T3, T4, T6, and T7 are closed; T5 is explicitly not measured and passed to the migration log.
The Engineer approved to proceed with implementation, so this document becomes historical.

### 2026-08-20

Open log. T1 and T2 completed before the log opened (the investigation is already underway when it starts).
The need for registration was identified — content consolidated below in
"Findings," updated as T3-T7 progress.

## Findings

Icebug is what it is and what it demands.

Here's the translation:

From Ladybug (<https://docs.ladybugdb.com/import/icebug/>), read on August 20, 2026 (content)
parafraseado):

Two flavors. INLINE_0 is Ladybug's native "graph-aware" Parquet;
The ``icebug-memory`` is an arrow. Both hold the graph in **CSR**.
- **Como se gera.** Com a ferramenta `icebug-format`, invocada como `uvx icebug-format`, a
Starting from either a **DuckDB** database or an **GraphAr** file, the output is a directory of
  Parquet mais um `schema.cypher`.
- **Como se usa.** O `schema.cypher` declara
  `CREATE NODE TABLE ... WITH (storage = '<dir>', format = 'icebug-disk')`. Sobe-se o Ladybug
With `-i schema.cypher` enabled, the DDL is executed on the instance.
Remote is factory supported. The INLINE 0 accepts URIs: INLINE 1, INLINE 2, INLINE 3,
Inline 0 and Inline 1 require the extension **httpfs**; Azure requires **azure**.
- **Credenciais de S3** via `CALL s3_credential(key_id=..., secret=..., region=...)` depois
  de `INSTALL httpfs; LOAD httpfs;`.

Direct consequence: The ladybug really knows how to query graph tables in Parquet on S3
Without downloading. The phrase "consult without downloading" is not an invention; it's a documented feature.

### T2 — O que este projeto distribui hoje

| Metade | Motor | Bundle | O que carrega |
|---|---|---|---|
Graph | LadybugDB | Inline 0 (Inline 1 + 1 Parquet per table/Parquet) | Nodes and Edges
| busca | SQLite | `search/` (`tables.json` + Parquet por tabela base) | **texto dos arquivos**, embeddings |

Neither of them carries an index: FTS5, vec0, and the vector index are a motor structure and the one.
Consumer builds (`internal/ast/parquet_transfer.go`, `ImportGraphFromParquet`).

The parquet today is **by default, flat** — generated by `COPY ... TO` with `RETURN n.*` for
Do not express column order. Not CSR and not IceBug: IceBug is a layout with `indptr`/

"`indices`, then the current Parquet file **is not** reprocessable as an Iceberg without conversion."

### T3 — `icebug-format` aceita entrada que este projeto consegue produzir: SIM

In addition to `--source-db <duckdb>` and `--graphar`, the tool accepts `--source-dir` with a directory.
Of Parquet, ** of vertices/edges. It is exactly the shape of what
**It already produces**, so **there's no need to go through DuckDB or any other database.**
GraphAr**.

What DOESN'T exist: a writer in Go. The official implementation is Python (backends PyArrow, DuckDB and Parquet).
DataFusion has no other language with published runtime dependencies.
Python with inline 0, only on the machine that publishes.

### T4 — `httpfs` offline: SIM, por arquivo

`INSTALL httpfs` baixa do servidor oficial para `~/.lbug/extensions`, mas existe
**ID:** `LOAD EXTENSION '<caminho>.lbug_extension'`, documented for compiled extensions
Locally, this allows pre-burial of the binary by platform in the launcher's payload – which
It has already done this with `liblbug`, ONNX Runtime, ICU, and Yaml grammars — and never.
tocar a rede numa consulta.

T5, T6, and T7 have been responded to by decision, not investigation.

The Engineer decided to proceed with migration before T5-T7 were written as studies.
The verdict, the drawing, and the execution plan are in.
[hub-on-s3-icebug-and-lancedb.md](hub-on-s3-icebug-and-lancedb.md), which **supersedes this
document  
In summary: feasible, with half of it not anticipated by the investigation - the index of
Search, which lacked a remote equivalent in SQLite, found a home in **LanceDB**, which has a Go SDK.
oficial e consulta `s3://` direto.

## Technical Debt

- [] Icebug disk writer in Go (an item on the backlog for improvements; see migration log.)

## System Knowledge

Consolidated in the migration log and in memory. The find that most changes the design:

"Remove DuckDB from the path, and LanceDB resolves half of the investigation."
  tinha marcado como bloqueada.
