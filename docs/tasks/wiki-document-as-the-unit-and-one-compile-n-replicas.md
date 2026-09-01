---
title: "The document becomes the wiki's unit, and memory gains one compile with N replicas"
description: "Removed the per-section chunker from the knowledge wiki (1 document = 1 page), made the slug deterministic, and fixed eight functionality defects in the wiki layer: dead embedding, three divergent directory resolvers, swallowed errors, search with no empty-index signal, an unordered trigram pass and divergence between .md and SQLite. Memory now has a single authoritative compile replicated to every project."
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
# The document becomes the wiki's unit, and memory gains one compile with N replicas

**Date:** 2026-08-14

## Objective

Two things, asked for in this order by the Engineer:

1. Stop splitting a document into subsections — the whole document is the unit of
   retrieval.
2. Fix the functionality defects the audit of the wiki layer
   raised, with no concern for backward compatibility, and working on Windows,
   Linux and macOS.

## What was wrong

### Splitting by section produced empty pages that competed in search

Measured on this repository's real index before the change: **2294 chunks from 165
documents** (13.9 per document), average of 82.6 words.

| `word_count` | chunks | % |
|---|---|---|
| ≤ 2 (empty) | 261 | 11.4% |
| 3–9 (below the embedding floor) | 79 | 3.4% |
| ≥ 400 | 15 | 0.7% |

`collectSectionBody` excluded the subsections' content from a section's body — which
is **correct**, it is what avoids duplicating the same text at every level. The mistake was
`emitSections` emitting the chunk anyway. A heading whose entire content is
subsections produced an empty body, and that empty body became a page on disk, a row in
`chunks` and a row in `chunks_fts`.

Of the 261 empty ones, 195 contained only the `**Parent:** [[slug]]` line the generator
injected. The most affected titles: `Use Cases` 24/24 empty, `Progress Log` 19/19,
`Changes` 20/20, `Test Cases & Acceptance Criteria` 23/24.

Worse: when a document opens with an H1 followed only by H2s, **the page named after the
document was the empty page** — and that is precisely the one a `[[wikilink]]` asks for.

The damage was reproducible, and amplified by the weights: `chunks_fts` uses
`bm25(title=10, body=1, summary=5, breadcrumb=2, doc_type=3)`, and 668 of the 2294 chunks
(29%) shared a title with another chunk.

```
knowledge_search("use cases for the daemon watcher")   ← antes
  1. Use_Cases_20.md                    → **Parent:** [[Task-_Git-Based_Daemon...]]
  2. Test_Cases_Acceptance_Criteria.md   → **Parent:** [[Task-_Git-Based_Daemon...]]
```

Two empty pages in the first two positions.

### `MinTokens` was a dead knob, with two comments claiming otherwise

`ChunkOpts.MinTokens` was documented as *"Minimum tokens before merging with
parent"*, set by the caller, defaulted in the chunker and **never compared with anything**.
`emitSections` said *"it will get merged by the post-processing step (wireParentChild
handles MinTokens merging)"* and `wireParentChild` declared itself *"performs MinTokens
merging"* — the function only rebuilt `Children`. There was no merging anywhere.

`SemanticChunk.Children`, `.Level`, `.StartByte` and `.EndByte` were also computed and
never read, and the offsets from `splitLargeSection` were false
(`accumStart + len(textoTrimado)`, not the real position in the document).

### The slug was positional

`UniqueSlug` numbered collisions `_2`, `_3`, … in iteration order, and `docs` was
sorted by `(docType, title)` with `sort.Slice`, which is **not stable**. Adding a
document renumbered the others and silently repointed every `[[wikilink]]` and every
xref already written — with no error, no log, no lint.

### Nothing embedded the wiki, in any project

`WikiEmbeddingModule`, `NewWikiEmbeddingModule` and `RunWikiEmbeddingLoop` existed,
complete and correct. **`NewWikiEmbeddingModule` was never called.** What the daemon
registered was `EmbeddingModule`, which is `ast.RunEmbeddingLoop` — AST, not wiki.

And the three manual paths pointed at a nonexistent directory:
`<project>/.graphit/knowledge/project/wiki`. The index lives one level above. Since
`OpenWikiDB` **creates** what it opens, each of them created an empty database in the wrong
place, found zero pending work and returned success — the CLI printing
*"All wiki chunks already have embeddings"* about a file it had itself just
created empty.

### The sync swallowed the embedding error

`if embClient, err := ai.NewEmbeddingClientFromConfig(); err == nil` made the whole
step disappear when the client failed, and `_, _ = embedder.RunCycle(...)`
discarded the rest. An embedder that does not come up was indistinguishable from a wiki
already embedded.

### `wiki_search` did not distinguish an empty index from an empty answer

`searchCompiledWiki` was fixed for exactly this reason, and the comment in
`search.go` documents the incident. But `wiki_search` did `OpenWikiDB` + `Search`
directly — and since `OpenWikiDB` creates, in a project with no compiled wiki the tool created
an empty database and returned zero without saying why. The `hybrid` mode degraded to pure
FTS in silence when there was no vector.

### The trigram pass fed the RRF with storage order

`queryChunksTrigram` was `SELECT … WHERE chunks_trigram MATCH ? LIMIT ?`, **with no
`ORDER BY`**, and every hit carried a fixed `Score: 0.1`. Since the RRF scores by
position (`0.7/(60+rank+1)`), the pass's weight was distributed according to the order in which
SQLite returned the rows.

### `.md` and SQLite could diverge

`content_hash` was `sha256(chunk.Body)` computed **before** injecting the parent link,
autolinking and resolving wikilinks, and writing the page was skipped when the hash
matched. Document A unchanged + new document B whose title A mentions: the
`chunks.Body` in the database got the `[[B]]`, but `A.md` was not rewritten. And
`BuildCrossRefGraph` computes backlinks **by reading the files**.

### Memory had two compiles competing, and no replication for the readers

The real chain, verified in the code:

```
remote ──git──▶ worktree ──compile──▶ wiki global ──copy──▶ réplica do projeto
                (verdade)             (autoritativo)        (o que a busca abre)
```

- The **worktree** (`<global>/memory-wt/memory-<scope>-<id>`) is the source of truth.
  `AddMemory` writes there and does `CommitAndPush`, which removes nothing — it is a full
  checkout, not a staging area.
- `MemoryLocalDir` (`<global>/memory/<scope>`) is **empty** on disk: it is only a
  placeholder, `syncToLocalInternal` overwrites `m.localDir` with the worktree.
- The **authoritative compiled** one is `MemoryWikiGlobalDir`, which is what
  `newMemorySvcInternal` was already electing.
- The **replica** (`<project>/<dotdir>/memory/<scope>`) is what readers open.

The defects: `RunProjectCycle`/`RunUserCycle` compiled **straight into the replica**,
while the daemon compiled into the global one — two files, distinct inodes, and whoever
ran last decided what a project was able to remember. And
`MemorySyncModule.recompile`, which is the path through which a memory coming from the
**remote** arrives, compiled into the global one and **replicated to nobody**. A memory from
the server did not show up in any project until someone ran a sync inside it.

On top of that, `WorktreeRawDirForScope` returned `""` when the replica did not exist — the
raw store, which is the source of truth, was unreachable until something had already
compiled from it. A fresh clone could not bootstrap its own
memories.

## What changed

### Granularity

`internal/wiki/chunker.go` was **removed** (762 lines, self-contained, with no test
of its own). `wiki.SplitByH2Headers` + `SplitDoc` in `docutil.go` were removed
too — a second splitter whose only consumer was a test shim.

`GenerateKnowledgeWiki` assembles one `knowledgeDoc` per file, with the whole body.
`breadcrumb` now is the source path normalized with `filepath.ToSlash`, which
makes the path searchable — `source` is not an indexed column in the FTS.

A latent bug was fixed on the way: when the stat fast-path hit,
`src.data` stayed `nil`; a cache miss after that indexed the document as
empty. Now the content is read at that point.

### Deterministic slug

The title when the title is unique in the corpus; the source path when it is ambiguous or
unusable. The ordering gained the path as a tie-break, making the order total.
The path goes through `filepath.ToSlash` **before** the slug: `SafeSlug` today swaps `\`
for `-` and that is why the result coincided by luck, and a slug that depended on the
separator would give different page names per platform.

### Byte-stable page, and the decision to rewrite by bytes

`updated:` in the frontmatter now comes from the **source's mtime**, not from `time.Now()` —
which was, besides being truer, what prevented comparing bytes: the rendered page
changed every day. With that, `writePageIfChanged` compares the rendered page against
disk, and autolink/backlink no longer go stale.

### One directory resolver

`resolveWikiDBDir` and `resolveWikiEmbedDir` were removed. What is left is
`resolveWikiScopeDir`, which delegates to the `resolveWikiDir` the indexers already use.
`RunWikiEmbeddingLoop` now **receives** the targets instead of deriving them, and
`memory.ProjectReplicaDir` replaced `GlobalScopeDir`, whose name said the opposite of
what the function returned.

### One compile, N replicas

`internal/memory/replicate.go` (new) is the only place that turns the authoritative one
into a replica. `ReplicateWikiToProjects` returns how many it updated **and the failures**.
`ReplicateMemoryScope` (in `internal/daemon`) decides the targets by the meaning of the
scope: `project` → only the project of the id; `user` → every registered project,
because user memory belongs to the person and not to the repository; context → only where the
replica already exists, because an imported context is opt-in.

`MemorySyncModule` keeps **one observation point** (the base worktree, which already covered
every branch, including the ones created later) and now fans out after each compile. The
in-project cycles compile into the authoritative one and replicate to the current project.

### Signal instead of silence

- `openWikiForRead` refuses an index with no content and says it is an empty index, not an
  empty answer.
- The `hybrid` mode reports when it degraded to FTS and why.
- `graphit_sync` accumulates notes and returns them; a sync that skipped half the work no
  longer says "completed successfully" on its own.
- The sync's embedding runs **after** the memory cycle, because on a first
  run the memory wiki does not exist yet before it.

### Search

The trigram pass gained `ORDER BY chunks_trigram.rank` and a real score.
`snippetAround` is the only snippet builder, centered on the first matching term,
with a width of 320 and the edges pulled to a word boundary with limited slack and
`utf8.RuneStart` as a net. `extractSnippet` delegates to it.

### Size

`sync_log`'s `details` covers only the pages the sync touched, and `Rebuild` keeps
a retention of 100 entries. `restoreEmbeddingsFromCache` now runs in a transaction.
The embedder's `MaxSourceChars` went from 800 to 1600, to fill the model's 512-token
window now that a chunk is a document.

### Schema v3

`parent_slug` (column, index and field) is gone — it existed to link a section's chunk
to the parent heading's chunk. `wikiDBSchemaVersion` went to 3.

## Cross-platform

- No `syscall`, `os.Chmod`, `os.Symlink` or hardcoded separator in the new code.
- `filepath.ToSlash` on every compared value: breadcrumb, the copy's exclusion rule,
  the embed's scope filter, the path-derived slug.
- Project dedup in replication is case-insensitive on Windows and macOS
  (`runtime.GOOS`), because there two paths differing in case are the same
  directory.
- **Replicas never receive `-wal`/`-shm`.** A log is only valid next to the exact database
  that produced it, and deleting one from under a reader is equally bad. The authoritative
  one is checkpointed (`Checkpoint()`, plus one checkpoint at the end of `Rebuild` and of the
  embedder's cycle) before replicating, so there is only a self-contained `wiki.db` to copy.
- A replication failure is **expected and survivable on Windows**: a replica with
  `wiki.db` open by a reader cannot be overwritten or deleted there, unlike
  on Unix. Replication is idempotent and loop-driven — the next pass
  picks up what this one did not write, and one stuck project does not block the others. The
  failures are logged with the project and the reason.
- `CycleResult.ReplicaErr` is separate from `Err`: the first says the wiki did not
  compile, the second that it compiled and one project's copy is behind.

## Files Changed

| File | Change |
|---|---|
| `internal/wiki/chunker.go` | **removed** — splitting by section |
| `internal/wiki/docutil.go` | `SplitByH2Headers`, `SplitDoc`, `contentHash16` removed |
| `internal/knowledge/wiki.go` | 1 doc = 1 chunk; deterministic slug; `updated` from mtime; `writePageIfChanged`; sync_log details; `knowledgeIndexPage` receives the slugs |
| `internal/wiki/fts.go` | schema v3 without `parent_slug`; `ORDER BY` on the trigram; `snippetAround`; sync_log retention; `Checkpoint()`; restore in a transaction |
| `internal/wiki/search.go` | `extractSnippet` delegates to the single builder |
| `internal/wiki/embedder.go` | `MaxSourceChars` 800→1600; post-cycle checkpoint |
| `internal/wiki/embed_loop.go` | receives `[]EmbedTarget` with a hook; no longer derives a path |
| `internal/wiki/process_cache.go` | `CachedChunk.ParentTitle` removed |
| `internal/memory/replicate.go` | **new** — replication, WAL exclusion, target dedup |
| `internal/memory/cycle.go` | cycles compile into the authoritative one and replicate; `ReplicaErr` |
| `internal/memory/memory.go` | `ensureProjectCopy` delegates to replication |
| `internal/memory/paths.go` | `GlobalScopeDir`→`ProjectReplicaDir`; the raw dir no longer depends on the replica |
| `internal/daemon/memorysyncmodule.go` | fan-out after compile; `ReplicateMemoryScope`; logger |
| `internal/daemon/adapters.go` | `WikiEmbeddingModule` with targets; `WikiEmbedTargets` |
| `internal/paths/copy.go` | `SyncCopyDirExcept`; the exclusion is honored in the initial copy too |
| `internal/mcpstdio/tools_wiki.go` | one resolver; `openWikiForRead`; hybrid reports the degradation; embed uses the daemon's targets |
| `internal/mcpstdio/tools_lifecycle.go` | embedding reports failure; runs after the memory cycle |
| `cmd/graphit/commands/daemon.go` | registers `WikiEmbeddingModule` |
| `cmd/graphit/commands/runners.go` | the embed CLI uses the daemon's targets |

## Verification

`go build -tags fts5 ./...` and `go test -tags fts5 -count=1 ./...` green.
`go vet -tags fts5` clean on the packages touched.

Reindexed with the installed binary:

| | before | after |
|---|---|---|
| chunks / sources | 2294 / 165 | **170 / 170** |
| empty chunks | 261 (11.4%) | **0** |
| below the embedding floor | 340 (14.8%) | **0** |
| average words | 82.6 | **1197.8** |
| duplicate titles | 668 (29%) | **0** |
| slugs with `_N` numbering | many | **0** |
| `sync_log` | 306 entries / 99 MB | 1 entry / 37 KB |
| `wiki.db` size | 117 MB | **8.3 MB** |

Search, same query as before:

```
knowledge_search("use cases for the daemon watcher")   ← depois
  1. Fix_Watcher_Mtime_Blind_Spot.md                 7.42
  2. Task-_Git-Based_Daemon_Auto-Sync_Watcher_Overhaul.md  7.22
  ...
```

Real documents, snippet centered on the term.

Replication: the memory replica was **deleted entirely** and rebuilt from the
authoritative one by `graphit memory index` — 100 chunks on both sides, no WAL sidecar
in the replica.

## System Knowledge

- **A `wiki.db` written by a binary with the new schema is DESTROYED by a binary
  with the old schema.** It happened during validation: the daemon running the installed
  binary (v2) reopened the index written by the local binary (v3), saw the different
  version, dropped everything, recreated it in v2 — and since the process cache said nothing
  had changed, it did not repopulate. It ended up `chunks = 0` with `parent_slug` back. The CLI
  autostarts the daemon through `PersistentPreRun`, so **testing a schema change requires
  `make install`**, not just `go build`.
- **`copyDirRecursive` was the path of the FIRST copy**, and it did not honor the exclusion.
  Found by the new test itself: the rule that excludes `-wal`/`-shm` worked on the
  mirroring and was ignored exactly when the destination did not exist yet, which is the
  common case.
- **The memory worktree is durable, not staging.** `CommitAndPush` removes nothing.
  Of the 11 worktrees on this machine, 9 are empty because those projects have no
  memory — not because they were emptied.
- **`ListActiveProjects` filters by existing lockfile, not by live process**, and
  GCs the entries whose lockfile disappeared. It is the right list for replication.
- **The memory wiki exists in two places with distinct inodes** and both had
  the same content only because both writers were running. Now one compiles and the other is
  a copy.

## Technical Debt

- [ ] `memoryIndexPage` and `appendMemLog` in `internal/memory/wiki.go` exist and are not
      called by `GenerateMemoryWiki` — the memory wiki generates neither `index.md`
      nor `log.md`. Probably dead code, not touched here.
- [ ] `cosineSim = 1 - d²/2` in `semanticSearchLocked` only holds for
      L2-normalized vectors. If the embedder does not normalize, the `semantic` mode's score is
      wrong (in `hybrid` the RRF overwrites it and hides it). Not verifiable here: the ONNX
      Runtime is not present on this machine.
- [ ] The stopwords in `wikiStopwords` include `no`, `not`, `use`, `using`, `where`,
      `when`, `how`, `why` — aggressive for technical documentation in the AND/OR/
      prefix passes.
- [ ] Replication copies the whole `wiki.db` per project. With many projects and a
      large index that is I/O linear in the number of projects on every memory
      change. A load test would decide whether it is worth copying only when the set's
      `content_hash` changes.
