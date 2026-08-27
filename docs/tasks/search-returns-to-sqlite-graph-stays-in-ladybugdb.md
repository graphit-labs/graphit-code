---
Title: Text Search Returns to SQLite; LadybugDB Is Left with Graph Only
status: done
created: 2026-08-19
updated: 2026-08-19
tags: [ast, wiki, memory, sqlite, ladybug, search, fts, parquet, hub, storage]
---

# Busca textual volta para o SQLite

Origin: Engineer's instruction - "I'm having many issues with the ladybug for"
All of this depends on the indexes.
Embedding and FTS are very limited and slow. Everything that is FTS, Embedding, Trigram, and
tudo o mais que responder por busca textual precisa ser feito no sqlite. O ladybug fica com o
grafo e o sqlite com a busca textual."*

This reverses the direction of INLINE_0 ("SQLite exits from binary format"), which was changed on August 16, 2026, three days earlier.

---

## Por que

A property measured alone decides: "the lib bug doesn't maintain an FTS index"
Insertion - The first two lines are hidden in 25 iterations, while the last two lines remain visible after 12 iterations. This requires INLINE_0 to be set.
Here is the translation:

"Starting with the eighth index after each write: work _INLINE_1_ for a change"

This translation maintains the structure and meaning of the original Portuguese text, expressing that starting from the eighth index following each write operation, there's some kind of work or process happening. The underscore followed by "INLINE_1_" suggests this is part of a larger context involving inline operations or changes.
`O(1)`.

No corpus real, 39.429 arquivos e 2.501.342 entidades:

| | LadybugDB | SQLite |
|---|---|---|
| full rebuild | 988 s | — |
| incremental de UM arquivo | **1.178 s** | ~300 ms |

The increment of an archive was slower than reconstructing everything. That number is the argument.
inteiro.

## O que NÃO foi feito: um rollback

The Engineer was explicit - "it's not just rolling back to previous commits."
The project progressed; you can consult back there to understand how SQLite works, but it's not recommended for production use.
The state of the code today is different; there have been improvements and corrections.*

Then **INLINE_0** served as a reference for SQL mechanics, not a destination. What remains is the

Translation: Then INLINE_0 was used as a reference for SQL mechanics, not as a final destination. What's left is the
estado de hoje, com a camada de armazenamento trocada por baixo:

Survived unscathed for what?
|---|---|
The following is the translation from Portuguese to English:

| `internal/ast/search_common.go` | It is purposefully storage-independent — tokenization, trigram bag, RRF, determinism. This is what made the migration of going into a single layer re-writing and the return as well |
The fusion (`search_fusion.go`, `store_query.go`) corrects three defects measured that a single `bm25()` does not express. They traversed the two engines without changing.
The logical scheme | `name_split` / `name_tri` / `etype` , and the precomputed 3-grams sack |
The probes of liblbug continue inverted for a purpose: they run while the bug is present.

Without backward compatibility — Engineer's decision, "We're in development." No fallback for
artifact published in an old format, no existing store migration exists.

---

The three design decisions

The index is updated in-place.

It does not follow the copy+swap graph update technique. This is what takes approximately 50 milliseconds to update the index.
In this corpus of this repository (measured, see below), against the 1,178 sentences that the previous arrangement included
pagava por um arquivo.

The cost is real, known, and accepted: a reader can see through an update's width.
New index against the old graph — or, if the swap fails, against a graph that didn't change.
Neither of them is corruption, and the next incremental correction will fix it. Pay `O(corpus)` for editing.
To close this window, it is precisely the exchange you are refusing.

The searchable text resides in tables BASE.

It was the opposite: INLINE 0 **was** storage, with the source inside the virtual table.

It changed because today's Hub persists as an artifact through **Parquet**, and a virtual table is
Motor structure, not provided—nothing to export from it. Then:

```
files, entities, entity_emb          tabelas base — o que viaja
file_fts, entity_fts, file_tri,      external-content FTS5 sobre elas, mantidas por triggers
entity_tri
Entity vector (vec0)                  Index vector constructed from entity embedding
```

The triggers are the mechanism that the other engine lacked: SQLite maintains an FTS5 index to
measure where lines arrive, then nothing is knocked down and rebuilt afterwards.

3. Leave only the SEARCH LOAD in the graph

Source file, matching fields, and label vectors declared by the grammar in
The graph maintains its nodes and edges as they are—**including `Comment.name`**, which is
Accessible by Cypher and not just text search, and that the AST skill documents.

---

## O Hub: o SQLite carrega do Parquet

New requirement that did not exist before `fb19403`. `internal/ladybugstore/transfer.go` moves
Tables of the Ladybug; now there is the twin `internal/sqlitestore/transfer.go`, led by
`sqlite_master`, com a mesma forma e as mesmas duas recusas:

Indices don't travel. FTS5 and vec0 are engine structure; the consumer builds them. It's
Also what compelled the scheme to have tables of bases.
The file from the database does not travel. A `.sqlite` loads page format and a set of
Compiled modules: A consumer without keys would open and find illegible tables.

An artifact of AST now has two bundles, `graph/` and `search/`, in directories.
separated, because any half may legitimately be missing and there is only one manifesto
It would be undetectable.

Columns are named on both sides, never by position. The graph bundle
He learned this in the opposite direction: his INLINE_0 maps by position and writes into the wrong column.
Silence when two types are compatible.

The identifiers of the manifesto are validated against INLINE 0. One
Manifesto is not ours entry—come inside of a repository that another person published and...
Brazilian Portuguese:
"Create an `INSERT` with the names found there."

---

What came out of reverting to FTS5: the prefix index

The LadybugDB does not have prefix marriage or wildcards (_`conf*`_ marries nothing). The sack of
Three grams was built to replace him. With FTS5 back, there are both of them, and they
They respond with different questions: the prefix reaches a genuinely specific term.
It starts; the bag scores partial overlap anywhere in the name.

Measured in this session, on the quality floor:

| | sem passe de prefixo | com |
|---|---|---|
| piso lexical | 12/16 | **13/16** |
| truncamento | 9/9 | 9/9 |

The 13th probe is ___LINE\_0___ → ___LINE\_1___: they tie exactly in ___LINE\_2___
Field name only, and nothing else in the fusion separates them. The index of prefixes separates – the query is
prefixo de "validate" e apenas substring de "Validator".

**English:** It changed its conclusion right alongside it, and even the test documentation itself was affected.
Here's the Portuguese text translated into idiomatic English:

"Prior to this, you had two drafts that both reached 9/9 and a drop.

This translation aims for natural-sounding English while maintaining the original meaning. The technical terms like "prefix index" have been kept as they are in the original text.
For 8 means that the prefix pass has stopped reaching the index of terms.

A defect of bloating resolved by construction

The `vec0` allocates fixed blocks of 1024 lines and never returns the space for a deleted line — it was.
What led an index to 20GB with 760MB of live data, and the only remedy was writing one.
arquivo novo inteiro.

Agora os vetores moram em `entity_emb`, uma tabela comum. `compactVectorIndexIfNeeded` conta
The dead lines and reconstruct `entity_vec` from it when they cross `max(4096, 25% of`
Vivas - cheap because it doesn't recycle the model, and rare enough for common cases to follow
milissegundos.

---

## Arquivos

| arquivo | o que |
|---|---|
Brazilian Portuguese:
| `internal/ast/search_sqlite.go` | New - scheme, write, and low-level queries |

Idiomatic English:
| `internal/ast/search_sqlite.go` | Updated - new framework, fresh code, and streamlined database access |

This translation maintains the technical nature of the original while making it more conversational in idiomatic English.
____ | New — The fusion, extracted from `search_query.go`, now storage-independent |
The inline elements have been removed — implementation on LadybugDB.
Created and removed within the same session: the accessors of `QueryRecord` have no use left.
| `internal/wiki/store.go`, `store_query.go` | reescritos sobre SQLite |
| `internal/wiki/values.go` | **novo** — acessores de linha e o parser do formato do sqlite-vec |
Brazilian Portuguese to idiomatic English:

"`internal/sqlitestore/transfer.go` | New — Parquet ↔ SQLite, twin of `ladybugstore`"
| `internal/ast/parquet_transfer.go` | exporta e importa as DUAS metades |
The index is updated in place again.
The inline 0 is rebuilt AFTER the swap.
The buffer pool's paper-backed ceiling lost its justification measurement; the number remains, and the comment states that it is without a measurement.
| `Makefile`, `.golangci.yml`, `fts5_required.go` (x2) | a tag `fts5` e as guardas voltam |

Removed probes measure what doesn't exist anymore: `fts_bufferpool_probe_test.go`.
`fts_hotcold_probe_test.go`, `fts_scaling_probe_test.go`, `fts_shape_probe_test.go`,
`search_copy_load_test.go`, `search_emb_mixed_batch_test.go`,
`search_oversized_source_test.go` — todos sobre o build de FTS, o `COPY` e o lote misto de
vetores **dentro do LadybugDB**.

## Estado

Complete green suite with `-count=1` in 40 packages. `go vet` and `golangci-lint` clean.
(0 issues). Quality floor 13/16, truncation 9/9, abbreviation recall 4/4.

### O incremental, MEDIDO no store real (2026-08-19, depois do `make install`)

Repository corpus: 737 files, 58,500 entities, 36,674 vectors.

| fase | custo |
|---|---|
Here is the Portuguese text translated into idiomatic English:

The file is one of 46 entities. It took three executions to run in 69, 51, and 48 milliseconds respectively.

This translation maintains the meaning while using more natural phrasing in English. The technical terms "entidades" (entities) are kept as they are not easily translated into a common English term.
Copy the directory of the graph (95 MB, warm page cache) | 31 ms
| incremental fim-a-fim, pelo CLI | **7,2 s / 8,6 s / 17,6 s** |

The search index is 0.3-0.7% of the increment. The ~50ms correspond to the order of magnitude.
Historic of SQLite (~300 ms) and better than it - external-content triggers
They kept FTs without anything, which was their entire bet.

What remains — 7 to 17 seconds, with this variation — is the "copy + swap of the graph": the insertion into
LadybugDB plus the mutated copy's `Shutdown` and `Close`, which is the header of `IncrementalRebuild`
It had already documented in 215 milliseconds - 5 seconds and this measurement confirms it to be larger and more variable than
isso hoje. Nada disso foi tocado por este trabalho.

The consequence for those optimizing is that the bottleneck of incremental no longer exists.
Attack on search index does not move number; attack on closing of graph copy moves.

Sonda: `internal/ast/incremental_cost_probe_test.go`, pulada sem
`GRAPHIT_COST_PROBE_STORE`.

---

References

- [Storage Layout](../architecture/storage_layout.md#two-engines-and-which-one-owns-what)
- SQLite exits from binary (./consolidate-search-into-ladybugdb-and-drop-sqlite.md) — the direction
  que este trabalho reverte
- [Artefato de AST leva as tabelas em Parquet](./ast-artifact-ships-parquet-tables.md)
- [Artefato de knowledge idem](./knowledge-artifact-ships-parquet-tables.md)
