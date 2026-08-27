---
title: Per-file wiki cache, and knowledge that travels compiled
status: done
created: 2026-08-14
updated: 2026-08-14
tags: [wiki, knowledge, memory, hub, cache, concurrency]
---

# Per-file wiki cache, and knowledge that travels compiled

## Objective

Four changes that all follow from one property: **a wiki's process cache is not a
local scratch file — for memory it travels in a git branch that every developer on a
team pushes to.** Anything shared inside it is a merge conflict waiting for the second
person to push.

1. The cache's shared `manifest.json` becomes one sidecar per source file, so two
   people compiling independently never write the same file.
2. A shard becomes a **complete** chunk, carrying the fields that used to live only in
   `wiki.db` (cluster, confidence, updated, importance).
3. Because of (2), a knowledge context can travel **compiled, with no source**: the
   consumer indexes the shards instead of re-deriving pages it just downloaded.
4. Memory travels the other way around — source **and** shards — because memory is
   read-and-write and every consumer recompiles it.

This is the second phase of the storage work; the first
([Centralize AST, knowledge and memory stores in the global directory](centralize-stores-in-global-dir.md))
moved every compiled artifact into `~/.graphit` keyed by id. That phase made the wiki
directory a single shared object; this phase makes its *contents* safe to share.

No backwards compatibility was kept — the project is in development. The cache version
bump handles the only migration that matters.

## Implementation Details

### The cache: one file per source file (`internal/wiki/process_cache.go`, rewritten)

`wikiProcessCacheVersion` went `1` → `2`, which is the whole migration: an old cache
reads as a cold cache and the stale `manifest.json` is deleted on open.

**Before** — one `manifest.json` holding a map of every source file to its hash, stat,
chunk count and cross-refs, plus `shards/<rel>.wiki.json` and `shards/<rel>.emb.json`.

**After** — nothing but per-file state:

| File | Holds |
|---|---|
| `shards/<rel>.wiki.json` | the processed chunks (`cachedFileChunks`) |
| `shards/<rel>.emb.json` | the embedding vectors, keyed by content hash |
| `shards/<rel>.meta.json` | `fileMeta`: version, hash, chunk count, mtime, size, slug, out-refs |
| `watch/<slug>.json` | `watchFileEntry`: the stat of a non-source file whose change invalidates the wiki |

`loadMeta()` rebuilds the in-memory index by walking `shards/` for `*.meta.json` on
open — one pass over a few hundred small files, replacing one file read.

Mechanical consequences inside the file:

- The `dirty[""]` sentinel that meant "the manifest needs writing" is gone; a `metaDirt`
  map tracks which sidecars need writing, so `Save()` writes only what changed.
- `Store()` no longer resets the meta entry. It used to build a fresh one, which
  silently dropped the slug recorded earlier in the run — invisible before, because the
  slug was never persisted at all.
- `CachedStatEntry.Slug` was removed: dead field. `FastPathCheck` receives the slug from
  its caller, not from the cache.
- New: `Slug(relPath)`, and the lock-free internals `getLocked` / `removeLocked` that let
  `Store`/`Remove`/`Prune` share code without re-entering the mutex.
- `removeEmptyParents` prunes the directory a deleted sidecar leaves behind, so a
  nested docs tree does not accumulate empty shard directories.

### The shard is a complete chunk (`CachedChunk`)

Added `ClusterID`, `ClusterName`, `Confidence`, `Updated`, `Important` — everything a
`WikiChunk` needs that is not derivable. Derived rather than stored: `Source` is the
cache key, `Slug` lives in the sidecar (one per file, not one per chunk), `WordCount` is
counted from the body.

The corpus-level fields are only knowable after the whole corpus has been processed, so
they arrive through a separate setter:

```go
type DerivedChunkFields struct {
	Slug, ClusterName, Updated string
	ClusterID                  int
	Confidence                 float64
	Important                  bool
}

func (wc *WikiProcessCache) StoreDerived(relPath string, d DerivedChunkFields)
```

`StoreDerived` **compares before marking dirty**. Without that, it would dirty every
file on every run — it is called for every document, including the unchanged ones — and
the cache would stop being incremental while still looking correct. Wired in
`internal/knowledge/wiki.go` phase 5 (line ~752), the one place where slug, cluster and
confidence coexist.

### Building an index with no source (`internal/wiki/pipeline.go`)

```go
func BuildDBFromCache(wikiDir string) (int, error)  // chunks indexed, or 0 with no error
func ResetDir(dir string) (string, error)           // clear and recreate, return the dir
func IsDerivedFile(rel string) bool                 // wiki.db and its sidecars, nothing else
```

`BuildDBFromCache` opens the cache, `LoadAllChunks`, and `RebuildDB` with a synthetic log
entry. A chunk whose sidecar has no slug is skipped — it was never published.

`IsDerivedFile` is deliberately narrow. Only `wiki.db*` is derived; the pages, the index,
the log and the shards are all content. It is the exclusion filter for publication.

### Knowledge: export without `docs/`, install without recompiling

New `internal/knowledge/context.go`:

```go
func ResetContextWiki(name string) (string, error)  // wiki.ResetDir(ContextWriteDir(name))
func IndexContextWiki(name string) (int, error)     // wiki.BuildDBFromCache(ContextWriteDir(name))
```

An imported context arrives **compiled and is never recompiled**. Export mirrors the wiki
directory minus `wiki.db*`; install extracts it and indexes.

- `internal/knowledge/paths.go`: `resolveWikiRoot` died — there is one form now, not two.
  `ContextDocsDir` removed. `WikiDirForContextIn` returns the directory directly.
- `internal/store/store.go`: `KnowledgeContextDocsDir` removed, no callers left.
- `internal/mcpstdio/tools_knowledge.go`: new `installKnowledgeContext(gs, name)` and
  `exportWikiToWorktree(wikiDir, wtDir)`. Install and sync go through
  `ExtractBranchDir(branch, "wiki", dir)` + `IndexContextWiki` and **report `chunks == 0`**
  instead of installing an empty context silently. Export uses
  `paths.SyncCopyDirExcept(..., wiki.IsDerivedFile)` — a mirror, so a page the publisher
  deleted leaves the branch — and refuses to publish an uncompiled wiki.
- `cmd/graphit/commands/runners.go`: `runKnowledgeImport` / `runKnowledgeExport` on the
  same logic; removed an orphaned `wd`.
- `internal/hub/service.go`: new `publishedWikiDir(cloneDir)` detects the wiki directory in
  a clone by `index.md` or `shards/`. `TypeKnowledge` now does
  `wiki.ResetDir(store.KnowledgeContextDir)` + `SafeCopyDir` + `wiki.BuildDBFromCache`.
  It uses `store` + `wiki` directly and **cannot** import `internal/knowledge` — that is an
  import cycle, because `knowledge/rule.go` imports `hub`.

### Memory: the shards travel with the memories (`internal/memory/shardsync.go`, new)

```go
const worktreeShardDirName = ".wiki"

func ImportShards(rawDir, wikiDir string) (int, error)  // additive
func ExportShards(rawDir, wikiDir string) error         // mirrors
```

- **Import is additive** — never overwrites, never deletes. A local shard may belong to a
  memory this developer has not pushed yet. A shard whose hash does not match its source
  is inert, because `Get` compares the hash before returning anything, so importing blind
  is safe.
- **Export mirrors**, so a deleted memory's shard leaves the branch instead of outliving
  it. `mirrorShards` skips the `.tmp` files of the shard writer.

The hooks are in `GenerateMemoryWiki` (`internal/memory/wiki.go`), **not** in `RunCycle`:
`MemoryService.IndexMemories` compiles directly without going through a cycle. Import runs
first, before the process cache is opened; export runs only on the full-build path, so the
fast paths are untouched. New helper `firstLogger([]*slog.Logger)`.

Nothing else travels — pages, index and `wiki.db` stay local.

## Use Cases

### UC-01: Two developers push memories concurrently
- **Actor**: two developers on the same project, each running a memory sync
- **Preconditions**: both have the memory branch checked out in their worktree; each has
  written a different memory
- **Main Flow**:
  1. Each `GenerateMemoryWiki` calls `ImportShards(rawDir, wikiDir)` — additive
  2. Each compiles, writing `shards/<own file>.{wiki,emb,meta}.json`
  3. Each `ExportShards` mirrors its shard tree into `<rawDir>/.wiki/shards/`
  4. `CommitAndPush` runs `git add .`; the shards ride along
  5. The second push rebases onto the first: the two added **different files**, so git
     merges them without being asked
- **Alternative Flows**: a fast-path compile (nothing changed) skips the export entirely
- **Error Scenarios**: a push that still conflicts fails on the raw memory markdown, which
  is the pre-existing behaviour and unrelated to the cache
- **Postconditions**: the branch holds both memories and both shard sets; neither
  developer's embedding work was lost
- **Affected Files**: `internal/memory/shardsync.go`, `internal/memory/wiki.go`,
  `internal/wiki/process_cache.go`

### UC-02: Compile a wiki incrementally
- **Actor**: the daemon, on a docs-tree change
- **Preconditions**: a populated cache at version 2
- **Main Flow**:
  1. `NewWikiProcessCache` walks `shards/` for `*.meta.json` and builds the index
  2. `StatMatch` / `HasChanged` decide per file whether to reuse
  3. Unchanged files return cached chunks and vectors; the embedding model is not run
  4. `StoreDerived` records slug and community, marking dirty only where a value moved
  5. `Save()` writes only the dirty sidecars
- **Alternative Flows**: version mismatch on the sidecar → the file reads as changed
- **Error Scenarios**: an unreadable or truncated sidecar is treated as absent, so the file
  is recompiled rather than failing the run
- **Postconditions**: `wiki.db` is rebuilt; the cache carries the current corpus
- **Affected Files**: `internal/wiki/process_cache.go`, `internal/knowledge/wiki.go`

### UC-03: Publish a documentation wiki (`knowledge export`)
- **Actor**: a developer, or the agent on explicit request
- **Preconditions**: the project's wiki is compiled
- **Main Flow**:
  1. Resolve the wiki directory from the store
  2. `exportWikiToWorktree` mirrors it with `paths.SyncCopyDirExcept(..., wiki.IsDerivedFile)`
  3. Pages, `index.md`, `log.md` and `shards/` land in the branch; `wiki.db*` does not
  4. Commit and push to `knowledge/project/<project-id>`
- **Alternative Flows**: the mirror deletes a page removed since the last export
- **Error Scenarios**: an uncompiled wiki is refused with an explicit error rather than
  publishing an empty context
- **Postconditions**: the branch holds a wiki that can be indexed without any source
- **Affected Files**: `internal/mcpstdio/tools_knowledge.go`,
  `cmd/graphit/commands/runners.go`, `internal/wiki/pipeline.go`

### UC-04: Install a documentation context (`knowledge install`)
- **Actor**: a developer on the consuming project
- **Preconditions**: the publisher has exported at least once
- **Main Flow**:
  1. `ExtractBranchDir(branch, "wiki", dir)` into `store.KnowledgeContextDir(name)`
  2. `IndexContextWiki(name)` → `BuildDBFromCache` builds `wiki.db` from the shards
  3. The context is recorded in the project's `contexts.json` under kind `knowledge`
  4. Search works immediately; no source document exists on the machine
- **Alternative Flows**: reinstall resets the directory first, so a page the publisher
  deleted disappears here too
- **Error Scenarios**: `chunks == 0` is reported to the caller instead of registering an
  empty context
- **Postconditions**: two projects installing the same context share one copy, keyed by name
- **Affected Files**: `internal/knowledge/context.go`,
  `internal/mcpstdio/tools_knowledge.go`, `internal/wiki/pipeline.go`

### UC-05: Install a knowledge artifact from the Hub
- **Actor**: a developer running `hub install <id>`
- **Preconditions**: the artifact was submitted with a compiled wiki
- **Main Flow**:
  1. `publishedWikiDir(cloneDir)` locates the wiki in the clone by `index.md` or `shards/`
  2. `wiki.ResetDir(store.KnowledgeContextDir(name))`
  3. `SafeCopyDir`, then `wiki.BuildDBFromCache`
- **Alternative Flows**: an artifact whose clone has no recognisable wiki is rejected
- **Error Scenarios**: a build error surfaces to the install; the context is not registered
- **Postconditions**: the artifact is queryable as a knowledge context
- **Affected Files**: `internal/hub/service.go`

### UC-06: Pull a colleague's memory
- **Actor**: a developer syncing memory
- **Preconditions**: the colleague pushed memories with their shards
- **Main Flow**:
  1. The worktree pulls; `.wiki/shards/` arrives with the markdown
  2. `ImportShards` copies only the sidecars missing locally
  3. The compile finds a hash match for the new memories and reuses the vectors
- **Alternative Flows**: a shard whose hash does not match is ignored and the memory is
  re-embedded
- **Error Scenarios**: a missing or empty `.wiki/` imports nothing and is not an error
- **Postconditions**: the local wiki covers the colleague's memories without having run
  the embedding model over them
- **Affected Files**: `internal/memory/shardsync.go`, `internal/memory/wiki.go`

### UC-07: A memory is deleted
- **Actor**: a developer or the consolidation pass
- **Preconditions**: the memory and its shard are in the branch
- **Main Flow**:
  1. The markdown is removed from the worktree
  2. The compile prunes the shard locally (`Prune` → `removeLocked`)
  3. `ExportShards` mirrors, so the shard is deleted from `.wiki/shards/` too
  4. The next commit records both deletions
- **Error Scenarios**: export runs only after a successful full build, so a failed compile
  never deletes a shard whose memory still exists
- **Postconditions**: no orphan shard survives in the branch
- **Affected Files**: `internal/memory/shardsync.go`, `internal/wiki/process_cache.go`

## Test Cases & Acceptance Criteria

### Feature: The cache has no shared write target
Ref: UC-01, UC-02

#### Scenario: Nothing outside the per-file trees exists
```gherkin
Given a wiki process cache that has stored chunks for two source files
When the cache directory is walked
Then every file found is under "shards/" or "watch/"
  And no shared index file exists at the root of the cache directory
```

#### Scenario: Two writers do not clobber each other
```gherkin
Given two cache handles opened on the same directory
  And the first stores chunks for "mine.md"
  And the second stores chunks for "theirs.md"
When both are saved
  And a third handle is opened on that directory
Then both "mine.md" and "theirs.md" are present with their chunks
```

#### Scenario: A version-1 cache is ignored and its manifest removed
```gherkin
Given a cache directory containing a "manifest.json" written by version 1
When a version-2 cache is opened on it
Then the cache reports every file as changed
  And "manifest.json" no longer exists
```

#### Scenario: Removing a file drops its sidecar
```gherkin
Given a cache holding chunks for "a/b/deep.md"
When the entry is removed and the cache is pruned
Then no shard, embedding or meta file remains for that path
  And the empty parent directories are gone
```

### Feature: The cache stays incremental
Ref: UC-02

#### Scenario: StoreDerived writes nothing when nothing changed
```gherkin
Given a cache whose derived fields already match the values about to be stored
When StoreDerived is called with those same values
  And the cache is saved
Then the meta sidecar's modification time is unchanged
```

#### Scenario: The slug survives a content change
```gherkin
Given a source file whose slug was recorded
When its content changes and its chunks are stored again
Then the recorded slug is still returned for that file
```

### Feature: An index can be built with no source
Ref: UC-03, UC-04, UC-05

#### Scenario: Building from shards alone
```gherkin
Given a wiki directory holding only shards, with no source document anywhere
When BuildDBFromCache is called on it
Then it reports the number of chunks indexed
  And a search over the resulting database returns those chunks
```

#### Scenario: Corpus-level fields survive publication
```gherkin
Given a chunk carrying a cluster name, a confidence, an updated date and importance
When it is published as a shard and rebuilt with BuildDBFromCache
Then browsing the rebuilt database returns all of those fields intact
  And the word count is recomputed from the body
```

#### Scenario: A chunk with no slug is skipped
```gherkin
Given a shard whose meta sidecar records no slug
When BuildDBFromCache is called
Then that chunk is not indexed
```

#### Scenario: An empty publication reports itself
```gherkin
Given a wiki directory whose shard tree is empty
When BuildDBFromCache is called
Then it returns zero chunks and no error
```

### Feature: A published wiki installs without its sources
Ref: UC-03, UC-04

#### Scenario: End-to-end publish and install
```gherkin
Given a project whose wiki is compiled from a docs tree
When the wiki is published without "wiki.db" and without the docs tree
  And a second project installs it as a context
Then the context is indexed from the shards alone
  And searching the context returns the published page
```

#### Scenario: Reinstall drops a page the publisher deleted
```gherkin
Given an installed context containing two pages
When the publisher deletes one page and publishes again
  And the consumer reinstalls the context
Then the deleted page is absent from the consumer's context
  And the remaining page is still searchable
```

#### Scenario: Two projects share one context copy
```gherkin
Given two projects installing the same knowledge context by name
When both installs complete
Then one directory holds the context, keyed by its name
  And both projects record the context in their own registry
```

### Feature: Memory shards travel safely
Ref: UC-01, UC-06, UC-07

#### Scenario: Import never overwrites local work
```gherkin
Given a local shard for a memory that has not been pushed
  And a branch carrying a different shard for the same path
When ImportShards runs
Then the local shard is byte-for-byte unchanged
  And the shards absent locally are copied in
```

#### Scenario: Nothing to import is not an error
```gherkin
Given a worktree with no ".wiki" directory
When ImportShards runs
Then it reports zero files imported and no error
```

#### Scenario: Export mirrors and drops what is gone
```gherkin
Given a worktree mirror holding a shard whose memory was deleted
  And a compiled cache that no longer contains that shard
When ExportShards runs
Then the orphan shard is deleted from the mirror
  And the shards still compiled are present
  And a ".tmp" file left by the shard writer is not published
```

#### Scenario: Only the shard tree is published
```gherkin
Given a compiled memory wiki containing pages, an index and "wiki.db"
When ExportShards runs
Then the mirror contains only the shard tree
```

#### Scenario: The mirror is invisible to the memory scan
```gherkin
Given a worktree containing memory markdown and a ".wiki" directory
When the memory source files are listed
Then no entry from ".wiki" appears
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/wiki/process_cache.go` | Modified (rewritten) | shared `manifest.json` → per-file sidecars; complete `CachedChunk`; `StoreDerived`; version 2 |
| `internal/wiki/pipeline.go` | Modified | `BuildDBFromCache`, `ResetDir`, `IsDerivedFile` |
| `internal/knowledge/context.go` | Created | `ResetContextWiki`, `IndexContextWiki` — a context arrives compiled |
| `internal/knowledge/paths.go` | Modified | `resolveWikiRoot` and `ContextDocsDir` removed; one form only |
| `internal/knowledge/wiki.go` | Modified | phase 5 records derived fields through `StoreDerived` |
| `internal/mcpstdio/tools_knowledge.go` | Modified | `installKnowledgeContext`, `exportWikiToWorktree`; export without `docs/`; reports empty publication |
| `internal/hub/service.go` | Modified | `publishedWikiDir`; `TypeKnowledge` indexes instead of recompiling |
| `internal/memory/shardsync.go` | Created | `ImportShards` (additive), `ExportShards` (mirrors) |
| `internal/memory/wiki.go` | Modified | import before the cache opens, export on the full-build path; `firstLogger` |
| `internal/store/store.go` | Modified | `KnowledgeContextDocsDir` removed |
| `cmd/graphit/commands/runners.go` | Modified | CLI import/export on the same logic; orphaned `wd` removed |
| `internal/wiki/process_cache_perfile_test.go` | Created | 8 tests, including the two-writer property |
| `internal/wiki/build_from_cache_test.go` | Created | 5 tests: building with no source, field survival |
| `internal/memory/shardsync_test.go` | Created | 6 tests: additive import, mirroring export |
| `internal/knowledge/publish_test.go` | Created | 4 tests: end-to-end publish and install |
| `internal/knowledge/knowledge_test.go` | Modified | the `resolveWikiRoot` test now asserts the single form |
| `internal/store/store_test.go` | Modified | dropped the removed helper's cases |
| `docs/specs/wiki_module.md` | Modified | new section: Process Cache, one file per source file |
| `docs/specs/hub_collaboration.md` | Modified | new section: two channels, and what each branch carries |
| `docs/specs/memory_module.md` | Modified | new section: the shards travel with the memories |
| `docs/architecture/storage_layout.md` | Modified | wiki directory inventory; `.docs/` gone; `.wiki/` in the worktree |
| `docs/tasks/per-file-wiki-cache-and-compiled-knowledge-publication.md` | Created | this log |

## Trade-offs & Decisions

**Per-file sidecars over a shared manifest.** Rejected keeping the shared file: it
conflicts on every concurrent push to a memory branch and git cannot merge JSON. The cost
is real and accepted — opening the cache now walks `shards/` instead of reading one file.
For a few hundred documents that is milliseconds, and it buys a data structure with no
shared write target.

**The shard carries the corpus-level fields.** The alternative was to leave cluster and
confidence out and let the consumer recompute them, which is impossible: community
detection needs the whole corpus, and a consumer that installs one context does not have
it. Publishing them is what makes the compiled channel work at all.

**`StoreDerived` compares before dirtying.** The simple version — always write — would
have quietly destroyed incrementality, since the call happens for every document on every
run. The comparison is the reason the cache stays cheap.

**Memory does not use `StoreDerived`.** It recompiles from source; the shards exist there
only to spare the embedding model. It also stamps `updated: now` on every page, so routing
that through `StoreDerived` would dirty every sidecar on every run — the exact churn the
comparison was added to prevent.

**Shard hooks in `GenerateMemoryWiki`, not `RunCycle`.** `MemoryService.IndexMemories`
compiles directly without a cycle, so a hook in `RunCycle` would miss the common path.

**Import additive, export mirroring.** Asymmetric on purpose. Additive protects a shard
belonging to an unpushed memory; a divergent shard is inert because `Get` compares the
hash. Mirroring, only after a successful full build, is what lets a deleted memory's shard
leave the branch.

**The knowledge branch carries no `docs/`.** The user's explicit call. The consumer never
compiles, so shipping source meant every consumer re-derived pages it had just downloaded
and paid for the embedding model a second time.

**`hub/service.go` uses `store` + `wiki` directly.** Not a style choice: importing
`internal/knowledge` from `hub` is an import cycle, because `knowledge/rule.go` imports
`hub`. The duplication of three lines is the cheaper side of that trade.

## Technical Debt

- [ ] **No migration from the version-1 cache** — it is discarded and recompiled, which
  costs one full embedding pass per wiki. Acceptable in development; a release would need
  a converter or a warning.
- [ ] **`StoreSlug` and `Slug` are test-only in production terms** — production writes the
  slug through `StoreDerived`. They survive as the narrow setter/getter the tests use to
  build fixtures. Harmless, but they read like production API.
- [ ] **`loadMeta` walks the shard tree on every open** — fine at current corpus sizes,
  linear in file count. If a wiki ever reaches tens of thousands of documents this is the
  first thing to measure.
- [ ] **An unreadable sidecar is silently treated as absent** — the right behaviour for
  robustness, but a corrupted cache recompiles quietly with no signal that it happened.
- [x] **Ephemeral live-search sessions leave an orphan wiki** — resolved in
  [An ephemeral session owns no store](an-ephemeral-session-owns-no-store.md). Two
  corrections to how it was recorded here: the orphan was keyed by the session's **ULID**,
  not `path-<hash>` (the workspace has a lockfile, so `ProjectStoreID` never reached the
  hash fallback), and the wiki was the least of it — the session also acquired a memory
  scope, which meant an orphan branch and a worktree in the shared memory repository.
- [ ] Inherited from phase 1, still open: `ContextRecord.DBPath` is
  misnamed (it serves both graph and wiki); there is no migration from the old layout;
  `.graphit/ast/queries` in a project reads as a leftover; `ContextNamesFrom` splits
  `<x>-<x>` in the middle; `memory/wiki.go` stamps `updated: now` (pre-existing churn).

## System Knowledge

- **`StoreSlug` and `CachedStatEntry.Slug` were dead code before this task.** The slug was
  never persisted — `Store()` rebuilt the meta entry from scratch on every call, so
  anything recorded earlier in the run was dropped. The bug was invisible because nothing
  read it back.
- **`FastPathCheck` takes the slug from its caller**, not from the cache. Worth knowing
  before assuming the cache is the source of truth for slugs.
- **`memorySourceFileNames` uses non-recursive `os.ReadDir` and skips directories**, which
  is why `.wiki/` is invisible to the memory scan for free — no exclusion needed.
- **`CommitAndPush` runs `git add .`**, so the shard mirror rides along on the next commit
  without any explicit staging.
- **`knowledge/rule.go` imports `hub`**, so `hub` can never import `internal/knowledge`.
  Any shared logic between them belongs in `store` or `wiki`.
- **API shapes confirmed while writing the tests**: `BM25Result` has `Path`, `Title`,
  `Score`, `Snippet` — no `Slug`. `db.Browse(BrowseFilter{ClusterID: -1, Limit: n})`.
  `BrowseEntry` has `Slug`, `Title`, `Summary`, `DocType`, `Breadcrumb`, `ClusterName`,
  `Confidence`, `Important`, `WordCount` — no `Source`.
- **The repository is not gofmt-clean** (blank lines were stripped project-wide). Never run
  `gofmt -w` on an existing file; write new files already formatted.
- **`internal/wiki` and `internal/ast` need CGO** (`sqlite-vec-go-bindings`). Cross-compiling
  to Windows fails on `sqlite3.h`, which is an environment gap, not a code one.
  `internal/store` is CGO-free and vets on every platform.
- **The knowledge staleness `.manifest.json` is a different file** from the process cache
  manifest that was removed here. For a while both existed under confusingly similar
  names; only the staleness one remains, and it is knowledge-only, so it never travels in
  a memory branch.

## Progress Log

### 2026-08-14
- Rewrote `internal/wiki/process_cache.go`: shared manifest → per-file sidecars, version 2,
  `metaDirt` replacing the `dirty[""]` sentinel, `Store()` no longer resetting the meta.
- Completed `CachedChunk` with the corpus-level fields; added `DerivedChunkFields` and
  `StoreDerived` with the change comparison; wired it into `knowledge/wiki.go` phase 5.
- Added `BuildDBFromCache`, `ResetDir`, `IsDerivedFile` to the wiki pipeline.
- Discovered `StoreSlug` and `CachedStatEntry.Slug` were dead; removed the field.
- Knowledge export dropped `docs/`; install and Hub install now index instead of
  recompiling. Killed `resolveWikiRoot`, `ContextDocsDir`, `KnowledgeContextDocsDir`.
- Added `internal/memory/shardsync.go` and hooked it into `GenerateMemoryWiki` after
  finding that `IndexMemories` bypasses `RunCycle`.
- Wrote 23 tests across four new files. Full suite green, `make vet` clean, `make lint`
  0 issues, `internal/store` clean under `-race`.
- Documented: four spec/architecture sections plus this log.
- Blocked, needs the user: the running MCP server predates the `withProjectDir` fix, so
  `graphit_memory_insert` fails with `getwd: no such file or directory`. The two decisions
  from this phase (store centralisation, per-file cache and compiled publication) still
  need recording in memory after `make install` and an MCP reconnect.
