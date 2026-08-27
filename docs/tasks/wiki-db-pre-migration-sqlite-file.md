---
Title: The wiki.db file fails to embed the wiki forever before migration, and the error does not specify the reason.
status: done
created: 2026-08-18
updated: 2026-08-18
tags: [wiki, ladybug, migration, bugfix, diagnostics]
---

A wiki.db SQLite survives migration and nobody notices.

Origin: The **INLINE_0** passed reporting failure upon opening three **INLINE_1**. I attributed it to
Temporary containment with the daemon. **I was wrong**— and the project log proved it.

---

## O defeito

The index of the wiki was SQLite and it has now become LadybugDB, with the same name as before.
file**. Nothing converts or discards the old file, so any machine that has
Indexed before the change remains with an **INLINE_0** that the current engine cannot open.

```
level=WARN msg="wiki embedding cycle error" module=ast
  dir=…/wiki/memory/user/…  error="open wiki db: open wiki store: open wiki.db: failed to open database with status 1"
```

Fifteen thirty-three repetitions in ninety minutes, three wikis halted, embedded wiki not running.

Confirmed by magic bytes, not by inference:

| arquivo | formato | data |
|---|---|---|
| `memory/user/<hash>/wiki.db` | `SQLite format 3` | 08-14 21:23 |
| `memory/project/<id>/wiki.db` | `SQLite format 3` | 08-17 19:03 |
| `knowledge/project/<id>/wiki.db` | `SQLite format 3` | 08-18 11:57 |

The knowledge is few hours before INLINE 0 -- the binary still writes
In that morning.

## Por que custou tanto para achar

The message is as bad as it can be: **INLINE_0** is ambiguous
Construction, because the C API does not have a channel for the C++ text. It is identical to lock.
Missing file, creation under read-only - and for incorrect format.

And it showed no process at all with the file open, which eliminated contention and was the solution.
que empurrou para olhar o formato.

Correction

**Translation:**

``discardPreMigrationDB``, called by ``OpenWikiDB`` before opening:

reads the first 16 bytes, and,
se forem `SQLite format 3\0`, remove o arquivo e os sidecars `-wal`/`-shm`/`-journal`.

Deleting is safe because the bank is **derived** — Inline 0 already said this, and it's the only one.
thing in a wiki directory that never travels: the pages and shards next to it are exactly what
He is reconstructed, and a memory wiki is compiled from the worktree.

The test is not "the opening failed." A healthy store that fails by lock or
It never can be destroyed; only an unproven file could possibly be lost by this engine.
He will never read it, and it is discarded. `TestOpenKeepsAStoreItCannotIdentifyAsSQLite` exists to catch.
That distinction.

## Testes

- `TestOpenDiscardsPreMigrationSQLiteDB` — arquivo com a magic do SQLite mais sidecars; abre,
Reconstructs, verifies counts, and the sidecars disappeared.
- _INLINE_0__ - illegible file that **is not** SQLite
Survives unscathed from the attempt.

---

Three more flaws found after the first one

Correcting the opening exposed the next steps. The three produce.
Even in its final state — empty index, no errors — due to unforeseen causes.

The two doors of skip were asking if the file existed.

It ended at `StatPreCheck`, and it started again at `FastPathCheck`. One
The store, present and empty, satisfies all other conditions — the sources have not changed and the ones that remain are still there.
Pages were generated — therefore, the generation is skipped, and the index never gets built, and all of them.
The execution jumps again. **`memory index` responded "complete" in 0.0 seconds on a database.
"Com 152 pages, starting at 16 KB."

The condition turned into INLINE 0, stated once and used twice. It costs one.
Store opening via generation – not by file. The alternative was a sizeable floor.
file that needs a magic number and fails silently on the first time it's used in a store
vazio mudar.

**Os dois testes que falharam com isso codificavam o bug como expectativa** — criavam um
The wiki.db file should be able to pass through, with the comment "An empty wiki.db"
Disk is the default state on a pristine wiki, *. Each name declares (completeness of
Pages continue valid (new source detection); what changed is that the fixture now satisfies
The remaining conditions of truth.

The error in building the index was discarded.

`internal/memory/wiki.go` fazia `_ = wiki.RebuildDB(...)`. Qualquer falha virava sucesso
Silent: Pages and shards written, "Memory Index Complete" printed, empty storage. And...
The search continued responding because it fell into a BM25 query about `.md`. So nothing seemed broken.
While all queries rely on the entire directory.

The third one did not have `--reset`.

The _INLINE_0_ and _INLINE_1_ have memory that didn't exist, and it's precisely the command for which they are.
It needs when the index is wrong for some reason that **is not** a memory alteration — which is
Exactly what a common execution doesn't fix because it skips over unchanged hashes.

To be safe: Git's memory lives in its own worktree, and the entire wiki is derived from it.
delas.

## O que ficou em aberto — RESOLVIDO em 2026-08-18

The text below is the original statement, maintained because both of its assertions were present.
Incorrect and how they were is the lesson:

With the three corrected, it reaches `RebuildDB` with 152 chunks, and then returns.
Without error, and even so, the store is empty like that. Every isolated reproduction of `Rebuild` works.
The remaining difference lies in the true path of memory and has not yet been found.

He did not return without error. The mistake was written in INLINE 0, which is the handler
NOP - the second correction above replaced INLINE_0 with an error log line and kept silent, then.
"No error" was read from an log that couldn't have any errors.

And the difference with isolated reproduction was not in memory's path, but in the corpus:
`writeChunks` mandava um UNWIND com linhas COM vetor ao lado de linhas SEM, o que o driver
Refusal. The fixtures give vectors to all chunks or none at all, so only a real corpus mixes.

Ver `docs/tasks/wiki-indice-vazio-por-lote-de-vetores-misto.md`.

Note: Because it was expensive

In this episode, I made four consecutive mistakes before finally getting it right: after handling the old one.
swap, dedup de embedding por texto, daemon segurando o slot de escrita, e embeddings nunca
Injected. All had the same form — "conclusion drawn from an instant snapshot of a system"
With asynchronous phases, without reading the log that records the phases.

O log existia o tempo todo, em `.graphit/runtime/daemon/daemon.log` (por projeto, escrito por
`projectRebuildLogger`). Eu olhei `~/.graphit/logs/graphit.log`, vi que estava desatualizado
e desisti em vez de procurar o certo. Foi ele que resolveu os dois casos reais:

```
13:02:49  initial cycle complete            entities_embedded=36178
13:02:49  rebuilding DB to inject embeddings
13:03:23  search index rebuild  files=730 entities=57657 vectors=36295
```

Proving that my zero-vector measurements fell, and that the AST embedding is working.
dentro da janela de vinte minutos em que ele ainda rodava.
