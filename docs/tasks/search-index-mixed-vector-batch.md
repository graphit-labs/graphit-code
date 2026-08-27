---
Title: The Index Builder Didn't Use Real Corpus Data - Same Lot With and Without Vectors Unwind
status: done
created: 2026-08-17
updated: 2026-08-17
tags: [ast, search, ladybug, vector, bugfix]
---

The search index was not built on a real corpus

**Origem:** medir o incremental de 1178 s registrado em
The measurement did not reach timing — or rather, the `consolidate-search-into-ladybugdb-and-drop-sqlite.md` is not yet complete.
rebuild completo morreu antes, num defeito que nenhum teste pegava.

---

## O defeito

```
search index build failed — keeping the previous database
error="insert entities: ladybug query: failed to convert Go value to Lbug value:
failed to create LIST value with status: 1. please make sure all the values are of the same type"
```

An INLINE_0 cannot mix lines that carry vectors and lines that do not
loads. The driver sets **an** INLINE\_0 parameter to **the** INTEGER\_INLINE\_1 and refuses types of
elemento diferentes.

| lote | resultado |
|---|---|
| todas as linhas com vetor | ok |
| todas com `emb: nil` | ok |
| misturado, vetor antes de nil | **falha** |
| misturado, nil antes de vetor | **falha** |

Mixed is the usual case, not the border. Whoever receives the vector makes the decision of grammar by itself.
List of YAML - then any file that mixes an embedded label with one
Embedded production yields both types of line, and any corpus of any size overflows with it.
primeiro flush.

### Por que nada pegava

Two reasons that combined:

The synthetic fixtures give vectors to **all** entities or none at all. A field
Optional field left blank uniformly does not exercise the optional field.
The production machine development kit's build store was still in its previous format —
"inline" alongside the graph -- because the installed binary was 16/8.
03:44 And migration is from August 16, 2016: 09:16. The new path has never found a real corpus.

---

Correction

The two obvious exits don't work, and they were taken before choosing:

| tentativa | resultado |
|---|---|
| `var typedNil []float32` | `failed to create LIST value because the slice is empty` |
| `[]float32{}` | idem |

The only way the driver accepts is homogeneous batch. `flushEntities` partitions the lot.
usa duas queries: `insertEntityQuery` com `emb`, e `insertEntityQueryNoVec` **sem a
property, leaving the column NULL – which is what vectorized queries already ignore by default.
construction. `RebuildFromCache` and `UpdateIncremental` pass through the same helper, so both
The writing paths are covered.

Regression in INLINE_0 without environment guard: four cases, 0.09 seconds.

Suite Verde, 98.5 S.

---

What was measured but did not become code here

Registered because it decides the next step, and because an earlier baseline has fallen with this.
The sensors used were discarded after measurement; nothing of instrumentation was left in the way.
quente.

The **INLINE_0** does not have the limitation that forced **INLINE_1**. The **INLINE_2** of a Parquet with
`FLOAT[768]` carregou 20.000 linhas, metade com vetor e metade com NULL, em 0,52 s. Se a
The index migration from `UNWIND` to `COPY` no longer requires partitioning.

The bottleneck for the complete rebuild is the column vector load by `UNWIND`. Same as 80,000.
linhas, todas com `FLOAT[768]`:

| caminho | tempo | µs/linha |
|---|---|---|
| `UNWIND` em lotes de 500 | 371,4 s | 4.643 |
| `COPY` de Parquet staged | **15,7 s** (+4,4 s staging) | **196** |

24x na carga. O mesmo `UNWIND` numa tabela *sem* coluna vetorial custa 48 µs/linha — a
coluna sozinha multiplica o insert por ~93, no conversor de valor do driver. Estado final
verified: constructed after 82s, `INLINE_0` returning the
match exato.

The creation order of the vector index costs 3.1 times more. `EnsureSchema` creates `se_vec`.
Before loading, with the comment "accepted on an empty table, so there is no ordering"
Constraint against the load "* -- correct on correction, wrong on cost: each insert passes.
Maintaining an HNSW results in a superlinear cost (10.088 µs per line at 2K, 17.202 µs at 20K).
"After" is linear. The FTS indices have already been created after purpose.

Consequence for a baseline: 988 rebuilt units registered in
They cannot have been measured with.
embeddings — com cobertura parcial estourariam no bug acima, e com cobertura total teriam
Taken hours. That number is not comparable to an actual build.

And what is not a bottleneck, with number: the copy of the store (833 MB ext4 → ext4 in 0.27 seconds; the disk
It doesn't support reflink and still is cheap, but the staging file format (Parquet, CSV, and)
JSON empatam no `COPY`: 0,71 / 0,74 / 0,79 s para 200k linhas) e a arquitetura de
competition — the doc allows for an INLINE 0 or multiple INLINE 1s, and the
Copy and swap follows this rule instead of circumventing it.

O incremental de 1178 s continua sendo outro problema, em outra fase: o DROP+CREATE dos nove
Indices FTS, which is the corpus because the engine does not have partial construction. Measured that
The cost of `CREATE_FTS_INDEX` is O(lines * that table), not O(database) — with 400,000 lines.
Frozen intact next to each other, reconstructing five indices from a table of 300 rows takes 1. 64 seconds --
It supports the drawing of two segments. It remains for an appropriate task.
