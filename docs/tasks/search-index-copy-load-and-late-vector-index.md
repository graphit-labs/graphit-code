---
Title: Load of Search Index by COPY and Built After Vector Index
status: done-with-caveat
created: 2026-08-17
updated: 2026-08-17
tags: [ast, search, ladybug, vector, performance, copy]
---

Index Search Load for COPY

Origin: The indexing index loading phase did not terminate. A measurement on a real corpus
(39.429 arquivos, 2.501.342 entidades) ela cresceu 270 MB em 55 minutos e projetava mais de
12 horas, o que inviabilizava qualquer rebuild completo.

---

## A matriz, medida

80.000 linhas, todas carregando `FLOAT[768]`:

| arranjo | total |
|---|---|
"89 seconds after index after (default)"
The sum of the inline index after (`cache_embeddings := false`) is 398 seconds.
"Add the inline value followed by index."
"Inline 0 + Index Before > 600 seconds (Interrupted)"

O mesmo `UNWIND` numa tabela **sem** coluna vetorial custa 48 µs/linha, contra 4.643 com
She. The cost is in the driver converting each element of the vector to the parameter of the...
Query - not in index.

And the order of the index stands on its own: with it in place, every insert maintains an HNSW and the cost is
**superlinear** (10.088 µs/linha a 2.000, 17.202 a 20.000), enquanto construir depois fica
Plan (~5, 500). `EnsureSchema` creates `se_vec` in an empty table with the comment that one
Empty table accepts index, so order doesn't matter; just that cost and
not correcting.

## Resultado no corpus real

| fase | antes | depois |
|---|---|---|
Index search load | > 12 hours (projected) | **321 seconds** |

Os quatro arquivos acima do teto de valor do `COPY` foram pela rota `UNWIND`, nomeados em
log `Debug`.

---

What was forced upon implementation

The extension **INLINE_0** became a hard dependency of the writing path. Without it, **INLINE_1** fails.
no binder com `Cannot load from file type json`. Oito testes quebraram nisso antes de
Passing it through with hard error, as it used to do with `fts` and `vector`.

**`estimateRowBytes` contava um `FLOAT[768]` como 8 bytes**, como qualquer escalar, o que
It transformed the byte budget into fiction for search lines. It started counting approximately 12 bytes per...
Dimension, which is the cost in JSON text.

The **INLINE 0** of JSON has a ceiling on its value, not in the document itself — and this corrects the lesson that
`json_rebuild.go` tinha registrado:

| documento | resultado |
|---|---|
| 1 linha, source 16 MB (arquivo 29,8 MB) | ok |
| 1 linha, source 32 MB (arquivo 59,7 MB) | **falha** |
| 60 linhas de 1 MB (arquivo **111,9 MB**) | ok |

So, the lotto ticket budget isn't what needs shrinking. Above the ceiling is isolated by
The variable `UNWIND` stores 32, 64, and 128 MB integers - `SearchFile.source` is the only copy.
Elegant in text, then truncating would be worse than failing. The threshold
It's about the value of CRU and purposeful conservative, because escaping JSON
Expand the text in a way dependent on its content: XML doubles, as every `<` becomes `<`.

---

The caveat, which is INLINE_0

The fast index vector build loads all embeddings into memory. In the real corpus
It "burst" a 8 GiB buffer pool - 2.5 million entities were approximately 7.7 GB of raw vector before.
Any index structure — and production limits the pool to 1 GiB (inline_0__).

Recovery is attempted only after failure and with an alert.
Log: 73 times slower than 383 in 80,000 vectors, same quality of response. It's approximately 5x slower and is
The difference between a search engine with semantic search and one without— that's why it's not a downgrade
silencioso.

This recovery route was not validated end to end in 2.5 million, because at that speed,

Translation is already English, so it remains unchanged:

"This recovery route was not validated end to end in 2.5 million, because at that speed,"
Build project takes approximately 3.3 hours. What is validated in this scale is the load on the lines (321 s).

---

## As sondas, e o que cada uma sustenta

They remain in the repository, all behind environment variable — none on `make ci`.
There exist because several constants and orders of production code are not derivable from
Reading: Without the probe, whoever finds `copyValueCeil` gets a comment and no way to access it.
To reproduce the number.

| sonda | guarda | o que sustenta |
|---|---|---|
| `vector_bulk_load_probe_test.go` | `GRAPHIT_VEC_BULK` | `COPY` no lugar de `UNWIND` (24x com coluna vetorial) |
Index vector constructed AFTER loading (3.1x, and superlinear in the opposite direction)
| `vector_index_memory_probe_test.go` | `GRAPHIT_VEC_MEM` | o retry com `cache_embeddings := false`, e o custo dele |
The roof is priced by value, not by document.
| `copy_format_value_ceiling_test.go` | `GRAPHIT_COPY_ROWSIZE` | Parquet carrega o valor que JSON e CSV recusam |
| `copy_table_export_probe_test.go` | `GRAPHIT_COPY_ROWSIZE` | `RETURN n.*` e o prefixo de PKs nas arestas |
Mapping Positional and Non-UTF-8 Byte Loss
| `copy_format_probe_test.go` | `GRAPHIT_COPY_FORMATS` | Parquet/CSV/JSON empatam na carga; `COPY` × `UNWIND`; glob |
| `fts_scaling_probe_test.go` | `GRAPHIT_FTS_SCALING` | custo do `CREATE_FTS_INDEX` por tamanho; `ATTACH (dbtype lbug)` |
The table is | `fts_hotcold_probe_test.go` | `GRAPHIT_FTS_SCALING` | `CREATE_FTS_INDEX`; FTS does not traverse | `ATTACH` |
Index Multi-Property; The Inverted Index is Not a Table
| `shard_parquet_size_probe_test.go` | `GRAPHIT_SHARD_SIZE` | por que os shards LOCAIS continuam JSON |
| `real_corpus_incremental_probe_test.go` | `GRAPHIT_REAL_CACHE` | rebuild e incremental por fase sobre um shard cache real |

## O que continua em aberto

The incremental of 1,178 s is not touched here: it's only the inserts for files.
Changed and the cost is the DROP + CREATE of the nine FTS indices, which is O(corpus) because the engine doesn't
There is partial construction. Measured as `CREATE_FTS_INDEX`, it costs O(lines * that table), and not
Bank - with 400, 000 intact cold backups next to it, reconstructing five indices of a table of
300 linhas leva 1,64 s —, o que sustenta um desenho de dois segmentos com tombstones e merge
Periodical. Own task.
