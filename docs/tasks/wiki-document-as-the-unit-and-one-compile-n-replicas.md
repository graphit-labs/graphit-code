---
Title: The document becomes the unit of the wiki, and memory gains a compile with N replicas.
Description: "Removed the section splitter from the wiki of knowledge (1 document = 1 page), made the slug deterministic, and fixed eight functionality defects in the wiki layer: dead embedding, three divergent directory resolvers, swallowed errors, empty index signalless search, unsorted trigram pass, and inconsistency between .md and SQLite." The memory now has a single authoritative replicated compile for all projects.
content-type: task-log
audience: developers
keywords:
  - wiki
  - chunking
  - knowledge
  - memory
  - replication
  - embeddings
  - fts5
related:
  - "docs/specs/wiki_module.md"
  - "docs/specs/memory_module.md"
  - "docs/specs/daemon_module.md"
  - "docs/guides/retrieval_architecture.md"
---
The document becomes the unit of the wiki, and memory gains a compile with N replicas.

**Date:** 2026-08-14

## Objective

Duas coisas, pedidas nesta ordem pelo Engenheiro:

Stop dividing a document into sections — the entire document is one unit.
recovery
2. Corrigir os defeitos de funcionalidade que a auditoria da camada de wiki
He rose without concern for backward compatibility and was functioning on Windows.
   Linux e macOS.

## O que estava errado

Producing blank pages that competed in the search for sections

Measured in this repository's current index before the change: **2,294 chunks of 165.
Documents (13.9 per document), average of 82.6 words.

| `word_count` | chunks | % |
|---|---|---|
| ≤ 2 (vazio) | 261 | 11,4% |
| 3–9 (abaixo do piso de embedding) | 79 | 3,4% |
| ≥ 400 | 15 | 0,7% |

It excluded from the body of a section its contents, which are the subsections' content.
It is correct, which avoids repeating the same text in all levels. The error was
Output: `emitSections` emits the entire chunk without delay. A heading whose content is entirely...
Subsections produced an empty body, and that empty body turned into a page on the disk, a line in the text.
`chunks` e linha em `chunks_fts`.

Dos 261 vazios, 195 continham apenas a linha `**Parent:** [[slug]]` que o gerador
Injecting. The most affected titles: `Use Cases` 24/24 empty, `Progress Log` 19/19,
`Changes` 20/20, `Test Cases & Acceptance Criteria` 23/24.

Worse: when a document opens with an H1 followed by only H2s, the page titled "Name" appears.
The document was blank — and that's precisely what it asks for, INLINE_0.

The damage was reproducible and amplified by weight: INLINE_0__ uses
`bm25(title=10, body=1, summary=5, breadcrumb=2, doc_type=3)`, e 668 dos 2294 chunks
They shared the title with another chunk.

```
knowledge_search("use cases for the daemon watcher")   ← antes
  1. Use_Cases_20.md                    → **Parent:** [[Task-_Git-Based_Daemon...]]
  2. Test_Cases_Acceptance_Criteria.md   → **Parent:** [[Task-_Git-Based_Daemon...]]
```

Two blank pages in the first two positions.

It was a dead knob with two comments contradicting it.

`ChunkOpts.MinTokens` era documentado como *"Minimum tokens before merging with
parent"*, setado pelo chamador, defaultado no chunker e **nunca comparado com nada**.
`emitSections` dizia *"it will get merged by the post-processing step (wireParentChild
handles MinTokens merging)"* e `wireParentChild` se declarava *"performs MinTokens
Merging - the function only reconstructed INLINE_0. There was no fusion anywhere.

They were also computed for `SemanticChunk.Children`, `.Level`, `.StartByte`, and `.EndByte`.
nunca lidos, e os offsets de `splitLargeSection` eram falsos
Brazilian Portuguese:
(`accumStart + len(textoTrimado)`, not the actual position in the document).

### O slug era posicional

Inline 0 counted collisions Inline 1, Inline 2, ... in order of iteration, and Inline 3 was
Ordered by `(docType, title)` with `sort.Slice`, which is not stable. Add one
documento renumerava os outros e repontava silenciosamente todo `[[wikilink]]` e todo
Here is the translation:

"Already xrefed – no error, no log, no lint."

### Nada embedava wiki, em nenhum projeto

`WikiEmbeddingModule`, `NewWikiEmbeddingModule` e `RunWikiEmbeddingLoop` existiam
completos e corretos. **`NewWikiEmbeddingModule` nunca era chamado.** O que o daemon
It was registered as INLINE_0, which is INLINE_1 — AST, not wiki.

And the three manual paths pointed to an nonexistent directory:
The index is one level above. As
`OpenWikiDB` **cria** o que abre, cada um criava um banco vazio no lugar errado,
He found no discrepancies and returned success — the CLI printing
*"All wiki chunks already have embeddings"* sobre um arquivo que ele mesmo acabara de
criar vazio.

### O sync engolia o erro do embedding

`if embClient, err := ai.NewEmbeddingClientFromConfig(); err == nil` fazia o passo
inteiro desaparecer quando o cliente falhava, e `_, _ = embedder.RunCycle(...)`
He dismissed everything else. A non-uploading embedder was indistinguishable from a wiki already.
embedado.

Brazilian Portuguese to idiomatic English:

It did not distinguish an empty index from an empty response.

It was precisely because of this that _INLINE_0_ was fixed, and the comment in question is...
`search.go` documenta o incidente. Mas `wiki_search` fazia `OpenWikiDB` + `Search`
direto — e como `OpenWikiDB` cria, num projeto sem wiki compilado a ferramenta criava
A bank was empty, and he returned with nothing without explaining why. The mode INLINE_0 degraded to FTS.
Pure and silent when there was no vector.

### O passe de trigrama alimentava o RRF com a ordem de armazenamento

`queryChunksTrigram` era `SELECT … WHERE chunks_trigram MATCH ? LIMIT ?`, **sem
`ORDER BY`**, e todos os hits levavam `Score: 0.1` fixo. Como o RRF pontua por
Position (`0.7/(60+rank+1)`), the weight of the pass was distributed according to the order in which it occurred.
SQLite devolvia as linhas.

### `.md` e SQLite podiam divergir

`content_hash` era `sha256(chunk.Body)` calculado **antes** de injetar o parent link,
Linking and resolving wiki links, and writing on the page was interrupted when the hash appeared.
She signed the document as it was, plus a new document titled "A" that references the original: Document A unchanged + New Document B titled "A".
The variable `chunks.Body` in the bank received `[[B]]`, but `A.md` was not rewritten.
`BuildCrossRefGraph` calcula backlinks **lendo os arquivos**.

The memory had two compilations competing, and no replication for whoever reads

The royal chain, verified in code:

```
Remote ── Git ──▶ Worktree ── Compile ──▶ Wiki Global ── Copy ──▶ Replica of the Project
                (verdade)             (autoritativo)        (o que a busca abre)
```

The worktree (_`<global>/memory-wt/memory-<scope>-<id>`) is the true source.
The ``AddMemory`` writes it and does `CommitAndPush`, which doesn't remove anything— it's a check-in.
Complete, not staging area.
It is empty on disk: just.
  placeholder, `syncToLocalInternal` sobrescreve `m.localDir` com o worktree.
The authoritative compiled version is INLINE_0, which is

Translation:

The officially approved compiled version is INLINE_0, which is
Already chose.
The replica (INLINE_0) is what readers open.

The defects: compiled directly into the replica,
enquanto o daemon compilava no global — dois arquivos, inodes distintos, e quem
He decided what a project could remember last.
**INLINE_0**, which is the path through which a memory comes from **remote**.
Arrived, compiled globally and **did not replicate for anyone**. A server memory
He didn't appear in any project until someone ran a sync within it.

Additionally, it would return `""` when the replica did not exist.
The raw store, which is the true source, was unreachable until something had already existed.
Compiled from it. A new clone couldn't even boot itself.
Memories.

## O que mudou

### Granularidade

`internal/wiki/chunker.go` foi **removido** (762 linhas, autocontido, sem teste
Removed were `wiki.SplitByH2Headers`, `SplitDoc`, and `docutil.go`.
Also— a second splitter, whose sole consumer was a test shim.

`GenerateKnowledgeWiki` monta um `knowledgeDoc` por arquivo, com o corpo inteiro.
`breadcrumb` passa a ser o caminho da fonte normalizado com `filepath.ToSlash`, o que
It makes the path searchable — `source` is not indexed as a column in FTS.

Corrigido no caminho um bug latente: quando o fast-path de stat acertava,
`src.data` ficava `nil`; um miss de cache depois disso indexava o documento como
Empty. Now the content is read at this point.

Deterministic Slug

Title when the title is unique in the corpus; path of origin when ambiguous or uncertain.
Useless. The ordering won the path as a tiebreaker, making the entire order complete.
O caminho passa por `filepath.ToSlash` **antes** do slug: `SafeSlug` hoje troca `\`
por `-` e por isso o resultado coincidia por sorte, e um slug que dependesse do
Separator would give page names different per platform.

Stable Byte Page, and the Decision to Rewrite by Bytes

The ``updated:`` in the front matter now originates from the **source's mtime**, not from ``time.Now()``.
which was more than just true; it prevented comparing bytes: the rendered page
She changed every day. With this, INLINE_0__ compared the rendered page with the original.
disk and auto-link/backlink do not become older.

Directory Resolver

`resolveWikiDBDir` e `resolveWikiEmbedDir` foram removidos. Sobrou
Here is the translation:

"`resolveWikiScopeDir` delegates to `resolveWikiDir` that indexers already use."
The `RunWikiEmbeddingLoop` passed to receive targets instead of deriving them, and
`memory.ProjectReplicaDir` substituiu `GlobalScopeDir`, cujo nome dizia o oposto do
What it returned was the function.

A compilation of N replicas

The only place where authority is transformed into something new is INLINE_0__.
In response. `ReplicateWikiToProjects` returns how many were updated and the failures.
`ReplicateMemoryScope` (em `internal/daemon`) decide os alvos por significado do
Scope:
`project`: Only the project of ID;
`user`: All projects registered,
because user memory is personal, not stored in the repository; context → only where it applies
Replica already exists because imported context is opt-in.

The ``MemorySyncModule`` maintains an observation point (the base worktree, which already covered)
All branches, including those created after), and then fanouts after each compile. The ones.
ciclos in-project compilam no autoritativo e replicam para o projeto corrente.

Signal instead of silence

Brazilian Portuguese:
- `openWikiForRead` refuses index without content and says it's an empty index, not a response.
  vazia.
The mode INLINE_0 reports when it degraded to FTS and why.
The accumulator, `graphit_sync`, accumulates and returns notes; a sync that skipped half of the work doesn't
  diz mais "completed successfully" sozinho.
The synchronization embedding runs **after** the memory cycle, because in the first instance
The execution of the memory wiki has not yet been implemented before it.

### Busca

Passe de trigrama ganhou `ORDER BY chunks_trigram.rank` e score real.
The INLINE 0 is the only snippet constructor centered around the first term that fits,
com largura de 320 e as bordas puxadas para fronteira de palavra com folga limitada e
`utf8.RuneStart` como rede. `extractSnippet` delega a ele.

### Tamanho

The ``details`` of ``sync_log`` covers only the pages that were synced to, and ``Rebuild`` maintains.
Retention of 100 entries. INLINE_0 has transitioned into transaction mode.
`MaxSourceChars` do embedder foi de 800 para 1600, para preencher a janela de 512
Tokens of the model now that the chunk is a document.

### Schema v3

The `parent_slug` function (column, index, and field) exited - it existed to connect a section's chunk.
ao chunk do heading pai. `wikiDBSchemaVersion` foi para 3.

## Cross-platform

No new code contains hardcoded versions of `syscall`, `os.Chmod`, `os.Symlink`, or separators.
In every comparison value: breadcrumb, exclusion rule for copying
  filtro de escopo do embed, slug derivado de caminho.
Duplicate project deduplication is case-insensitive in replication on Windows and macOS.
(`runtime.GOOS`), because two different box paths lead to the same place
directory
Replicas never receive `-wal` or `-shm`. A log is valid only next to the exact database.
What produced it, and deleting one from under a reader is equally bad. The authoritative
It is checked (`Checkpoint()`), another checkpoint at the end of `Rebuild` and the loop
Before embedding, there is only one INLINE_0__ auto-contained for copying.
Failure of replication is expected and survivable in Windows: a replica with
The _INLINE_0_ that is open by a reader cannot be overwritten or deleted there.
Contrary to Unix, replication is idempotent and guided by loops – the next step
He takes what he did not write, and an unfinished project does not hinder others. The task is complete when it's done.
Failures are logged with the project and the reason.
The first says that the wiki isn't
Compiled, the second one that compiled and the copy of a project is behind.

## Files Changed

File | Change
|---|---|
Removed - section break
| `internal/wiki/docutil.go` | `SplitByH2Headers`, `SplitDoc`, `contentHash16` removidos |
Here is the idiomatic English translation:

"Each document equals one chunk; deterministic slug;mtime data from `updated`; sync log details in `writePageIfChanged`; receives slugs in `knowledgeIndexPage`."
Inline 0: Schema V3 without Inline 1, Inline 2 in the Trigram, Inline 3 retention of sync_log, Inline 4 restore within transaction.
The constructor is delegated to the unique constructor.
Inline 0: Inline 1: 800 → 1600; post-cycle checkpoint
The element receives `[]EmbedTarget` with a hook; it does not derive any further path.
| `internal/wiki/process_cache.go` | `CachedChunk.ParentTitle` removido |
Brazilian Portuguese to idiomatic English:

"_`internal/memory/replicate.go`_ | New - Replication, Exclusion of WAL, Target Deduplication"
| `internal/memory/cycle.go` | ciclos compilam no autoritativo e replicam; `ReplicaErr` |
The delegation is passed through replication.
The port is now no longer dependent on the replica's raw direction.
Pipe: after compilation, fan-out; `ReplicateMemoryScope`; logger
| `internal/daemon/adapters.go` | `WikiEmbeddingModule` com alvos; `WikiEmbedTargets` |
The inline comment is hidden, and it also honors the honorable exclusion in the initial copy.
The output is already in idiomatic English, so no further translation is needed:

"The inline 0 is a resolver; inline 1; hybrid degradation report uses the daemon's targets."
The Portuguese sentence "embedding reports failure; restarts after memory cycle" translates to idiomatic English as:

"The embedding fails, and it restarts after the memory cycle."
| `cmd/graphit/commands/daemon.go` | registra `WikiEmbeddingModule` |
| `cmd/graphit/commands/runners.go` | CLI de embed usa os alvos do daemon |

## Verification

`go build -tags fts5 ./...` e `go test -tags fts5 -count=1 ./...` verdes.
`go vet -tags fts5` limpo nos pacotes tocados.

Reindexed with the binary installed:

| | antes | depois |
|---|---|---|
| chunks / fontes | 2294 / 165 | **170 / 170** |
| chunks vazios | 261 (11,4%) | **0** |
| abaixo do piso de embedding | 340 (14,8%) | **0** |
Average number of words: 82.6 | **1,197.8**
Titles Duplicated | 668 (29%) | 0 |
Brazilian Portuguese:
| Slugs with numbers `_N` | Many | 0 |
| `sync_log` | 306 entradas / 99 MB | 1 entrada / 37 KB |
| tamanho do `wiki.db` | 117 MB | **8,3 MB** |

Busca, mesma query de antes:

```
knowledge_search("use cases for the daemon watcher")   ← depois
  1. Fix_Watcher_Mtime_Blind_Spot.md                 7.42
  2. Task-_Git-Based_Daemon_Auto-Sync_Watcher_Overhaul.md  7.22
  ...
```

Documentos reais, snippet centrado no termo.

Reproduction: The replica of memory was **completely erased and reconstructed from scratch**.
autoritativo por `graphit memory index` — 100 chunks nos dois lados, nenhum sidecar
of WAL in replica.

## System Knowledge

A new binary with an altered schema is DESTROYED by another binary.
with an old schema. **It happened during validation: the daemon was running the binary.
Installed (v2) reopened the written index by binary local (v3), saw version
  diferente, dropou tudo, recriou em v2 — e como o process cache dizia que nada
Changed, but did not repopulate. Left `chunks = 0` with `parent_slug`. The CLI
Start the daemon by calling `PersistentPreRun` first, then testing schema changes require
**INLINE 0**, not only **INLINE 1**.
The `copyDirRecursive` was the path of the first copy, and it did not honor exclusion.
Found through its own test new: the rule that excludes `-wal` / `-shm` worked in
Mirror and was ignored precisely when destiny did not yet exist, which is the
  caso comum.
The working tree is durable, not staged. `CommitAndPush` does not delete anything.
Of this machine's 11 worktrees, 9 are empty because those projects don't have them.
Memory - not because they were emptied.
Brazilian Portuguese to idiomatic English:

**The `ListActiveProjects` filter looks for an existing lock file, not a live process.**
It is doing GC on entries whose lockfile has disappeared. This list is correct for replication.
The wiki of memory exists at two locations with distinct nodes, and both had
The same content because both writers were riding. Now one compiles, and the other is
Copy.

## Technical Debt

- [ ] Inline 0 and Inline 1 exist, but they do not
They are called by `GenerateMemoryWiki` — the memory wiki does not generate `index.md`
Nem inline 0. Likely dead code, not touched here.
Only for vectors in `semanticSearchLocked`.
Normalized by L2. If the embedder doesn't normalize, the score of mode INLINE_0 is
Wrong (in INLINE_0, the RRF overrides and hides). Not verifiable here: ONNX.
The runtime is not present on this machine.
- [ ] As stopwords de `wikiStopwords` incluem `no`, `not`, `use`, `using`, `where`,
Here is the translation:

"`when`, `how`, and `why` - aggressive for technical documentation in AND/OR statements."
      prefix.
- [ ] Replicating copies the entire `wiki.db`. With many projects and one,
Large index is linear I/O with projects changing every number.
memory. A load test would decide whether it's worth copying only when `content_hash`
      do conjunto muda.
