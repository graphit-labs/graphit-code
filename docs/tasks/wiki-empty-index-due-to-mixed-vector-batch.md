---
Title: The index of the wiki was empty because an UNWIND cannot mix a line with a vector and a line without one.
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [wiki, ladybug, embedding, bugfix, strace]
---

# O que sobrou de `wiki-db-pre-migration-sqlite-file.md`

The task log closed three defects and left one open, as stated:

With three corrected, it reaches `RebuildDB` with 152 chunks, and then returns.
> sem erro, e o store sai vazio assim mesmo.

As duas metades daquela frase estavam erradas, e cada uma por um motivo diferente.

---

State at the beginning

About 152 memories, with the recently installed binary:

```
› Cleared …/wiki/memory/project/01KSH1CRFFG8Z74B5ZS78WW808
✓ Memory index complete (0.2s)
```

152 pages written, 455 shards beside, **inline** 0 of **16.384 bytes**, no errors. Two
Things point to the problem without naming it: 0.2 seconds is too little for 152 chunks plus index.
FTS has an index vector, and 16 KB is less than the inline scope Wiki's size of `user`, which has **zero**.
Memories occupy 1.9 MB. A store with nothing inside was larger than a store with 152 pages.
lado — sinal de que o de 16 KB nem chegou a receber o schema.

How the defect became invisible

**`Rebuild` publica por swap**: escreve um store novo em `wiki.db.new` e o renomeia por cima do
I am alive. Whenever any step fails, INLINE_0 **erases** the temporary and the old index remains.
Exactly where you were. The only evidence of a failure is the absence of change— which is
Indistinguishable from "there was nothing to do."

Foi o `strace` que nomeou o defeito antes de eu saber qual era:

```
openat("…/wiki.db.new", O_RDWR|O_CREAT)      = 8     ← criado
pwrite64(9<…/wiki.db.new.wal>, …)                    ← populado
pwrite64(8<…/wiki.db.new>, "…WikiSyn"…)              ← checkpoint entrou no arquivo
unlinkat("…/wiki.db.new", 0)                 = 0     ← APAGADO
```

Nenhum `rename`. O caminho executado era o de erro, e o `Rebuild` estava retornando erro.

Method, because it goes beyond this atomic swap issue: when a pipeline with an atomic swap "does nothing."
Trace the system calls. Publishing and aborting have different forms on the disk even when they are the same.
obvious result

## Por que "retorna sem erro" parecia verdade

`internal/memory/wiki.go` reportava a falha assim:

```go
slogutil.Resolve(nil).Error("memory wiki index build failed; …")
```

The function returns a value of type `discardHandler`, which is an instance of class or object `Enabled`. The variable `slogutil.Resolve(nil)` holds the return value, and it can be used in further calculations.
The previous commit replaced `_ = wiki.RebuildDB(...)` with an inline log and maintained silence:
The error has been written and discarded. The previous session read "no errors" from a log that
nunca poderia ter um, e foi procurar o defeito em outro lugar.

The **INLINE_0** — the funnel through which every **INLINE_1** passes — does not go through the logger. In other words, the
The only caller that mattered was exactly what it reported nowhere.

The error becomes apparent once it is visible.

```
rebuild wiki db: insert wiki chunks: failed to convert Go value to Lbug value:
failed to create LIST value with status: 1. please make sure all the values are of the same type
```

It is the same defect of commit 1a8839c, fixed in the index of search for the Abstract Syntax Tree on August 17 and never.
In the wiki. The driver creates a single LIST for the parameter `$batch` of type integer and refuses types.
different elements, then a batch with line carrying `FLOAT[768]` next to a line with
`emb` nil morre inteiro.

Mixed is the usual case, not the border. A single chunk receives a vector if three things are true of it.
even time — the content hash is in the embedding cache, the vector has the right dimension, and
The chunk has at least 10 words, inline. Every wiki whose embedding is
Partially, it produces both types of lines, including all wikis that the embedder has not yet embedded.
He finished and all the wikis had one short page.

Why did the Wiki of `user` survive: with no memories, no lines, nothing to mix? Why did it?
Another project survived: 8 memories that fell on the same side.

Correction

Brazilian Portuguese to idiomatic English:

**`internal/wiki/store.go` — `writeChunks` divides each lot into two homogeneous parts and uses**
duas queries: `insertWikiChunkQuery` com `emb`, e `insertWikiChunkQueryNoVec` sem a
property, leaving the column NULL -- which is what vectorized queries already ignore by default.
construction

Each half is pulled when empty. This is not defensive programming: a UNWIND on an empty list fails
with "failed to create LIST value because the slice is empty," then an encyclopedia with vectors in all sections
The chunks (or none) cannot receive the other query.

The two obvious exits have already been measured in 1a8839c and do not work: `[]float32` is null.
tipado e `[]float32{}` falham os dois pelo mesmo "slice is empty".

**`internal/memory/wiki.go` — `errLogger`**: prefere o logger do chamador e cai no
`slog.Default()`, nunca no NOP. `slogutil.Resolve` continua correto para conversa de rotina; o
The path he cannot be is the report that says the index does not exist.

## Resultado medido

| | antes | depois |
|---|---|---|
| `wiki.db` do projeto | 16.384 B | 30.777.344 B |
Pages: 152, 153
| tempo de `memory index --reset` | 0,2 s | 2,7 s |

The 0.2s were the cost of writing and failing; the 2.7s are the index being built.

The ``graphit_memory_search`` responded constantly, using fallback BM25 on the ``.md``, which is
Exactly because nothing seemed broken while every query relied on the entire directory.

## Testes

`internal/wiki/chunk_partial_embedding_test.go`:

- **INLINE 0** — five vector distributions on the

This text is already in English, so no translation was needed.
The same 6 chunks (alternating, only the first, only the last, all, none). Check the count.
Chunks and vectors: partitioning the lot cannot cost an entire chunk its embedding.
- `TestPartialEmbeddingSurvivesMoreThanOneBatch` — `wikiBatchRows + 7` chunks, porque a
The partition occurs in lots, and the result cannot depend on where the lot was cut.

The fixture writes bodies with 40 words for purpose: below `wikiEmbedMinWords`, none
linha ganha vetor, e o teste deixaria de exercitar a mistura sem falhar.

It belongs to another defect of this session - verify
`docs/tasks/daemon-herdava-ambiente-de-git-hook.md`.

## Arquivos alterados

File | Change | Reason
|---|---|---|
| `internal/wiki/store.go` | Modificado | `writeChunks` particiona o lote; duas queries de insert |
Modified | `internal/memory/wiki.go` | Updated so that index failure is no longer discarded |
Created | Mixed Lot Regression, Including Lote Traversal
Modified | Item left open is closed |

Debt Technical

The square brackets indicate that this is an incomplete sentence, and "INLINE_0" likely refers to a specific inline code or variable. The English translation would be:

- [ ] **`BuildDBFromCache` returns 0 chunks on a wiki memory directory with 455**

This suggests the function or script being tested returns zero results when processing a directory containing 455 files in a wiki memory system.
Shards. ** Observed during this defect inspection, not investigated: `LoadAllChunks()` is not
You find nothing where INLINE_0 just wrote. It doesn't affect memory—nothing installs one.
Wiki memory from shards — but it's the path through which a publicly published Wiki by the Hub is
Mounted, then it's worth confirming whether the format of memory shards and what is INLINE_0?
The wait is the same.
- [ ] The partition exits when migration of data from UNWIND to COPY occurs. `COPY` does not have this feature.
limitation - measured at 1a8839c, loads `FLOAT[768]` with NULL mixed in. Applies to the AST and
  para o wiki.
