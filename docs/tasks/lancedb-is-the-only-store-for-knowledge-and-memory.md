---
title: LanceDB becomes the only store for knowledge and memory
status: in-progress
created: 2026-09-01
updated: 2026-09-02
tags: [wiki, memory, knowledge, lancedb, architecture]
---

# LanceDB becomes the only store for knowledge and memory

## Objective

Collapse three artifacts per wiki into one. Today a wiki directory holds compiled `.md`
pages, a Lance index, and a process cache; memory additionally holds a raw markdown store
that is its source of truth. The target:

1. **Compilation writes only into LanceDB**, including the incremental path. The frontmatter
   is columns, the body is its own column — already true, and extended to everything.
2. **No compiled `.md` pages.** They are a parallel output of the build, not a stage of it:
   `RebuildDB` receives the chunks assembled in memory and never reads a page. So they are
   duplication in the strict sense.
3. **Memory has no raw store.** The Lance table in S3 is the memory, written directly,
   shared by every unit that shares those memories. Concurrency becomes commit-retry.
4. **Knowledge's table is local**, and reaches S3 only when published to the Hub. A context
   consumed from the Hub is read from S3 in place and never changes.
5. **Every source read comes from LanceDB**, never from a file. `ReadPageFrom` already does
   this for Hub-mounted contexts; it becomes the only path.

Staleness is accepted: LanceDB is the source of truth, so there is nothing for it to be stale
against.

## Reasoning

### Why this is a simplification and not just a move

The page files are not a cache of the table — they are a second, independently written copy of
the same chunks, and several passes exist only to maintain them:

- `internal/wiki/crossref.go` reads every page and **rewrites backlinks into them**
  (`os.WriteFile`). The `xrefs` table already holds the same edges. The pass is redundant
  twice over.
- `internal/wiki/fastpath.go` decides the incremental skip with `os.ReadDir` plus a
  frontmatter read per page. `content_hash` is a column; this is a query.
- `internal/wiki/bm25.go` (`NewBM25Index`) walks and parses every page to run BM25 in Go. It
  exists as a fallback for the index being behind the pages, a failure mode that cannot exist
  when there are no pages.
- `internal/wiki/lint.go` reads pages to audit them. Queries.
- `index.md` and `log.md` are rewritten every build. `sync_log` is already a table.

For memory the raw store adds a whole second mechanism on top: `shardsync.go`
(`ImportShards`/`ExportShards`) exists to move embeddings alongside raw markdown so a memory
written elsewhere arrives with its vector. If the table IS the store, the vector is a column
and the mechanism disappears.

### What the objection to a shared writable table actually is, and what answers it

Today two units adding memories touch two different object keys and **cannot** conflict.
As one table they contend, and commits fail and must be retried — Lance publishes compact
manifests straight to storage rather than replaying a log or consulting a catalogue, which is
why it degrades far better than the alternatives under concurrency, but "better" is not
"never" (their own June 2026 benchmark reports a 16% commit-failure rate at 200 concurrent
writers, against 88–94% for Delta and Iceberg). At the concurrency of a few agents this is
small and a retry loop covers it.

The stronger objection was recoverability: with no raw store, the table is the only copy of
memory, and this project has already had a pass destroy frontmatter in 20 files — recovered
only because the raw store had a backup.

**Lance answers that itself.** A table keeps its versions; `lancestore` already exposes
`PruneVersions(ctx, olderThan)`, which means retention is a policy rather than an absence. A
bad write is recoverable by reading an earlier version of the table. That is a real backup,
native to the format, and it makes the raw store's removal materially safer than it first
looked. It has to be *verified*, not assumed: see T0.

### The one part that should not follow the rest

**`docs/` stays as authored files.** The proposal says knowledge does not need them, and for
the purpose of *building the wiki* that is true — but `docs/` is not a build input the way the
raw store is, it is the thing humans write:

- `README.md` must be a file. GitHub renders it from the repository and nothing else.
- a spec or an ADR is a contract, reviewed in the diff next to the code that implements it.
  Earlier in this same line of work we drew exactly this line: the spec stays versioned with
  the code, the execution record goes to memory. Task logs and backlog leaving `docs/` is that
  decision; specs and decisions leaving it is the opposite of it.
- a fresh clone with no Hub credentials would otherwise have no documentation at all.
- authoring happens in an editor, on a branch, with a preview. Through MCP into a table, none
  of that exists.

So: the **compiled pages** go, the **authored sources** stay. Knowledge keeps a rebuild source
for free, which is what makes "the table may be stale" safe for it. Memory has no such source,
which is why its safety net has to be Lance versioning instead.

## Constraint: no fallback, no backwards compatibility

Stated 2026-09-01. The project is in development, so nothing here carries a compatibility
path and nothing keeps a second way of answering:

- the markdown BM25 scan (`NewBM25Index`, `scanMarkdownBM25`) and the
  "index is behind its markdown" warning are **deleted**, not gated
- `wiki.ReadPage` and `resolvePageFile` are **deleted**. `ReadPageFrom` is the only read
- no dual-read path, no schema tolerance for a table written by an older binary, no flag to
  restore page output

One thing is explicitly NOT covered by this and stays: **the one-shot migration of the
existing raw memory store into the table.** That is not compatibility — it is 315 real
memories in this store alone, and losing them is data loss rather than a dropped code path.
It runs once, verifies by count and content hash, and is then deleted with the raw store.

The blast radius is therefore wider than `internal/wiki`: `internal/uiserver/wiki_handler.go`
serves pages from files, `internal/dream/prompt.go` instructs the agent to read `index.md`,
and `internal/hub/service.go` detects a wiki by the presence of `index.md`. All three are part
of T1.

## Decisions

- [x] **D1 — `docs/` stays in the repository.** What is redundant is the compiled `.md` wiki
  page, not the authored document: the compiled form already goes straight into LanceDB, so
  writing it as a file too is a second copy of the same chunks. `docs/` remains the input to the
  knowledge build and the thing humans write, review in a diff, and read on GitHub.
- [x] **D3 — one table per scope.** A memory scope is already an S3 prefix and a directory, so
  the table follows the boundary that exists. It keeps the blast radius of a bad commit inside
  one scope, keeps the `user` scope private, and makes retry contention per-scope rather than
  global.
- [x] **D4 — the Parquet bundle dies.** `internal/wiki/transfer.go` (`ExportToParquet` /
  `ImportFromParquet`) exists to move a wiki as a file bundle; nothing needs that once the table
  is the artifact. Memory is always written directly to S3. Knowledge is a local table until it
  is exported to the Hub, and publishing is writing that table to the published prefix. A
  context consumed from the Hub is read from S3 in place and never downloaded.
- [x] **D2 — YES: Lance version retention is accepted as memory's recovery mechanism**, replacing
  the raw store. Decided 2026-09-01 on T0's evidence, against MinIO rather than against the API:
  a table seeded with rows, emptied by `DeleteWhere("true")`, and restored to the version before
  the delete comes back with the same rows — and the restore APPENDS a version rather than
  discarding the ones after it, so the mistake stays in the history and the recovery is itself
  undoable. Retention became a policy (`wiki.version_retention`) because the 15-minute constant
  was chosen when nothing used time travel; a store holding the only copy of its data wants a
  window long enough to be a safety net, and the knowledge wiki still wants only a margin for
  in-flight readers because its recovery path is `docs/`.
  **The concurrency cost turned out to be smaller than the plan assumed and to live somewhere
  else** — see T0.3 below. **T2 may start.**

## Plan & Task Breakdown

- [x] **T0 — Prove the two properties the rest depends on.** DONE 2026-09-01, against MinIO.
  Both properties hold; D2 answered YES and T2 is unblocked. The plan's assumption about WHERE the
  contention lives turned out to be wrong — see the T0 entry in the Progress Log. Spec: a test, not a document.
  (a) a remote (`s3://`) Lance store can be written with retry on commit conflict, with two
  concurrent writers, against MinIO; (b) an earlier table version is readable after a
  destructive write, through `lancestore`, and `PruneVersions` retention is configurable.
  Done means both are covered by tests. **If (a) or (b) fails, T2 does not start** — the raw
  store stays until they hold.

- [x] **T1 — Stop writing compiled pages; move everything that read them onto the table.**
  DONE 2026-09-01, in four slices. No wiki writes a page, nothing reads one, and a page read carries
  its frontmatter again — so the memory protocol's chain walk works as documented and T5 has nothing
  to reword there. Markdown is produced on demand by `graphit wiki export`.
  Spec: `internal/wiki`. Delete the page-writing loop and the prune pass from
  `GenerateMemoryWiki` and the knowledge generator. Then, in order of who breaks:
  `crossref.go` writes backlinks into the `xrefs` table only; `FastPathCheck`/`StatPreCheck`
  compare `content_hash` by query; `lint.go` reads chunks; `index.md`/`log.md` become the
  `sync_log` table plus a rendered view; `NewBM25Index`/`scanMarkdownBM25` and the
  "index is behind its markdown" warning are deleted; `wiki.ReadPage` and `resolvePageFile`
  give way to `ReadPageFrom` for every scope, so `wiki_source` has one path. Add
  `wiki_export` producing markdown on demand for whoever wants Obsidian.
  Constraint: this is the reversible half and it lands on its own, before anything touches the
  memory store.

- [x] **T2 — Memory writes go straight to the table; the raw store retires.** DONE 2026-09-02. Spec:
  `internal/memory`. `AddMemory`/`UpdateMemory`/`RemoveMemory`/`Promote`/`Demote` become
  `Upsert`/`DeleteByKey` on the scope's table, with retry. The revision chain stays exactly as
  it is — `entity_id`/`revision_id`/`superseded`/`current_id` are already columns, so an
  archived revision is just another row and `history/` stops being a directory.
  `ListMemories`, `ListImportantMemories`, `consolidate`, `repair` and `dream` read the table.
  `shardsync.go` dies with the raw store. `ScopeStore` becomes a thin remote-writable Lance
  handle. Constraint: T0 must have passed. Per the user's 2026-09-02 clarification, test data may
  be discarded and no migration or compatibility reader is retained.

- [x] **T3 — Knowledge builds into a local table and publishes by writing it to S3.** DONE
  2026-09-02. Spec:
  `internal/knowledge`, `internal/wiki/transfer.go`, `internal/hub`. Local build unchanged in
  shape but with no page output; export writes the table to the published prefix instead of a
  Parquet bundle; a Hub context is opened read-only at its `s3://` URI and never downloaded.
  Constraint: depends on D1 for whether `docs/` remains the input.

- [x] **T4 — Incremental, on the table.** DONE 2026-09-02. Spec: `Upsert` by `slug` for changed documents,
  `DeleteByKey` for removed ones, `FoldNewRowsIntoIndexes` after a batch, `Compact` and
  `PruneVersions` on a schedule. Done means a one-document change does not rewrite the table
  and does not drop four tables, which is what `RebuildDB` does today.

- [x] **T5 — Rules, specs, ADR, and the memory that says the wiki has three artifacts.** DONE
  2026-09-02.
  Several memories and the knowledge/memory skills describe the page files as the source of
  truth. All of them become wrong on T1 and have to be corrected in the same pass.

## Technical Debt

- [ ] `search_body` still repeats `body`, because the engine's FTS targets one column. Unchanged
  by this work and still worth measuring — see
  `docs/tasks/memory-revision-chain-searchable-history.md`.

## Progress Log

### 2026-09-01

- Confirmed the premise in code: `RebuildDB` takes the chunks assembled in memory and never
  reads a page, so the compiled `.md` files are a parallel output rather than a stage.
- Mapped the blast radius. Reading the memory raw store: `internal/memory/wiki.go`,
  `memory.go`, `shardsync.go`, `cycle.go`, `paths.go`, `important.go`, `consolidate.go`, plus
  `internal/daemon/memorysyncmodule.go`, `internal/dream/dream.go` and
  `cmd/graphit/commands/lifecycle.go`. Reading compiled pages: `bm25.go`, `crossref.go`,
  `fastpath.go`, `lint.go`, `multi_search.go`, `search.go`, `okf_log.go`, `helpers.go`,
  `source.go`.
- Wrote this plan. Not starting code: T0 gates T2, and D1 changes the shape of T3.

### 2026-09-01 (T1, slice 1: reads stop going to files)

Decisions D1, D3 and D4 answered — recorded above. Landed the read half of T1; the pages are
still being written, which is slice 2.

- **Deleted `internal/wiki/bm25.go`** — the Go BM25 index over the page files, with
  `NewBM25Index`, `BM25Config`, `DefaultBM25Config`, the tokenizer and the stopword list.
  `BM25Result` moved to `types.go`, keeping its name: the ranking is still BM25, now the
  engine's.
- **`BM25Search` is index-only.** `scanMarkdownBM25` and the
  `"wiki index is behind its markdown"` warning are gone. The comment explaining why the
  fallback existed is replaced by one explaining why it cannot: there are no pages for the index
  to be behind.
- **Deleted `ReadPage`, `ListPages`, `resolvePageFile`, `findPageInsensitive`, `withinDir`.**
  New `ReadPageAt` / `ListPagesAt` open the index and delegate to `ReadPageFrom`, so a local
  wiki and a Hub-mounted one differ only in the URI the store opens. `wiki_source` and
  `graphit wiki source` both go through it.
- **Two affordances of the file reader were preserved deliberately**, because losing them would
  have been silent:
  - case-insensitive slug resolution. A column filter is exact, so `PageBody` gets a
    case-insensitive second pass on a miss only (`resolveSlugCaseInsensitively`).
  - a case-insensitive `.md` suffix (`trimPageExt`), since every tool hands slugs back with one.
- **The containment check became a malformed-reference check.** A slug with a separator or a
  `..` used to be refused for escaping the wiki directory; there is no directory now, so it is
  refused for not being a slug. The distinction it drew is what still matters: a mistyped slug is
  answered with the list of pages, a path is answered with "that is a path".
- **`multi_search.go` stopped reading files twice over.** The per-source `index.md` read became
  `wikiOverview` — a `Browse` query rendering slugs, titles and types — and the requested-page
  read became `loadWikiPageFromIndex`. That second one was the bug the tests caught: with pages
  unread, the multi-wiki loop found nothing and answered in one turn.
- Tests converted from writing page fixtures to building indexes (`indexedWiki`, `probeWiki`).
  `writeFile` moved to `helpers_shared_test.go`, since crossref still parses wikilinks out of
  text.
- `go test -tags lancedb ./...` green, `make lint` 0 issues.

**Next (T1 slice 2):** stop WRITING the pages — the write loop and prune pass in both
generators, `crossref.InjectBacklinks` writing into files, `FastPathCheck`/`StatPreCheck` reading
the directory, `lint.go`, `okf_log.go`'s `log.md`, `uiserver/wiki_handler.go` serving pages from
disk, `dream/prompt.go` pointing the agent at `index.md`, and `hub/service.go` detecting a wiki
by `index.md`. Then `wiki_export` for whoever wants markdown on demand.

### 2026-09-01 (T1, slice 2: the memory wiki stops writing pages)

- **`FastPathCheck` compares against the index, not the directory.** Condition (3) listed the
  `.md` files in `wikiDir` to detect deletions; it now reads the index's own slug set
  (`indexedSlugs`), which is both what exists and what a search can answer with. Its test was
  rewritten accordingly and renamed `TestFastPathCheck_FalseUntilTheIndexHoldsEveryEntry`.
- **`GenerateMemoryWiki` writes no pages and prunes nothing.** The render loop and the
  `keepFiles` sweep are gone. `ArticlesWritten` now counts the documents the rebuild carries
  rather than the files a loop created, which preserves its meaning for every caller that reports
  it and for the sync-log decision.
- **Peripheral consumers of the generated pages fixed:**
  - `internal/hub/service.go` `publishedWikiDir` recognised a published wiki by `index.md`. It
    now looks for the compiled index — a wiki published after this change would not have been
    found otherwise, which is a bug this pass created and closed in the same breath.
  - `internal/dream/prompt.go` instructed the agent to read
    `<dot>/knowledge/project/index.md` and the two memory `index.md` files. It now names the MCP
    tools, because a path instruction would send the agent to something that does not exist.
- **`internal/memory/okf_conformance_test.go` deleted.** It asserted that the generated markdown
  pages conform to OKF; OKF describes a markdown bundle, and there is no bundle. The conformance
  that still matters — one chunk per document, supersession columns — is covered by
  `TestReadPageFromIndexReturnsTheWholePage` and the chain tests.
- Memory tests moved from globbing `*.md` to querying the index, via a new
  `indexedMemoryPages` helper that reports live and superseded row counts.
- `go test -tags lancedb ./...` green, `make lint` 0 issues.

**Still open in T1, and it is one coherent piece rather than leftovers:** the knowledge generator
uses the PAGES as its medium for phases 2–4. `wiki.BuildCrossRefGraph(wikiDir)` builds the graph
by reading page files, `InjectBacklinks` writes backlinks into them, autolinks are resolved into
page text, and the cluster and staleness passes re-render pages — with `writePageIfChanged`
carrying a comment about why the file and the index must not disagree. Restructuring that means
building the cross-reference graph from `newOutRefs`, which the pipeline already computes in
memory for the cache, and letting `xrefs` be the only edge store. `internal/uiserver/wiki_handler.go`
is the other half: it serves pages, `log.md` and directory listings straight off disk in six
places. Neither is a small edit and neither should be started without its own pass.

### 2026-09-01 (T1, slice 3: the knowledge generator stops writing pages — PLAN)

**Where the work actually stood when this slice opened, which is not where the prompt said.**
The working tree held a partial revert of the previous commit: `internal/wiki/fastpath.go`,
`internal/memory/wiki.go` and this log were back at their `1f00ead` content, while the TEST
changes of `7aa2f89` were still in place. That combination is not a state anybody wrote — it
fails `TestFastPathCheck_FalseUntilTheIndexHoldsEveryEntry`, which asserts the index-based
condition (3) against a `ReadDir`-based implementation. Restored the three files from `HEAD`
rather than redoing slice 2 by hand; the reverted content is `1f00ead`'s and is in git.

So slice 2's items 1 and 2 (memory half) are **already done and committed**, and item 1's
second half needs correcting: `StatPreCheck` never read the wiki directory. Its deletion
signal is Phase B failing to `ReadFile` a vanished SOURCE file, and its index gate has been
`IndexHasContent` since before this work. Only its doc comment still describes "the generated
.md pages".

**What remains is one coherent piece: the knowledge generator uses the PAGES as the medium of
its phases 2–4.** Removing the write loop alone would silently empty the cross-reference
graph, the communities, the staleness pass and both lints, because all four read the files
the loop wrote. So the order below is dependency order, not preference.

- [x] **T1.3a — The cross-reference graph is built from data, not from a directory.**
  Spec: `internal/wiki/crossref.go`. `BuildCrossRefGraph(wikiDir)` reads every `.md` and
  re-extracts links with two regexes; the generator already computes the same edge set in
  memory (`doc.crossRefs` after autolinking, which is what it writes to the `xrefs` table).
  Replace it with `BuildCrossRefGraphFromRefs([]PageEdges{Slug,Title,Targets})` plus
  `BuildCrossRefGraphFromIndex(ctx, wikiDir)` for the callers that only hold a wikiDir.
  `InjectBacklinks` stops writing files and becomes `CrossRefStats(graph)`. Constraint: the
  regex extractors (`FindWikiLinks`, `isBundlePageLink`, `ResolveSlug`) stay — the UI and the
  autolinker still parse link text out of prose.

- [x] **T1.3b — The index answers the questions the directory used to.**
  Spec: `internal/wiki/store.go`. Add `Chunks(ctx)` (every row, for lint and export),
  `Chunk(ctx, slug)` (one row, for the UI page view), `PageHashes(ctx)` (slug → content_hash,
  projected, so added/updated detection costs no bodies) and `AllXRefs(ctx)`. Add the
  `stale_since` / `stale_reason` columns: they were page frontmatter and nothing else, so
  dropping the page without them loses the only record that a page is stale. Constraint: the
  new columns must be written through `buildChunkRow` — it is the single row constructor by
  design.

- [x] **T1.3c — `GenerateKnowledgeWiki` writes no page, no `index.md`, no `log.md`.**
  Spec: `internal/knowledge/wiki.go`. Delete the render loop, `writePageIfChanged`, the
  `keepFiles`/`os.Remove` prune, the three `index.md` writes, the `log.md` append,
  `knowledgeIndexPage`, `appendKnowledgeLog`, and the `processCache == nil` frontmatter
  fallback (a second skip path that reads pages). added/updated/deleted come from
  `PageHashes`; the graph comes from T1.3a; the cluster and staleness passes annotate the
  docs in memory instead of re-rendering pages. Constraint: `sync_log` already records
  added/updated/deleted, so the log is not lost — it moves.

- [x] **T1.3d — Both lints read the index.** Spec: `internal/wiki/lint.go` and
  `internal/knowledge/lint.go`. `LintWiki` takes a `ctx`, builds the graph from the index and
  checks columns: `doc_type` for OKF's one required field, `title`/`summary` for the
  recommended ones, `word_count` for an empty page, `updated`/`stale_since` for staleness.
  `LintKnowledgeWiki` takes the in-memory docs instead of a wikiDir. Constraint: the
  "uncited source" check can no longer fire — one document is one page and every page carries
  its own source — so it is deleted rather than left as a check that always passes.

- [x] **T1.3e — Every remaining page reader.** Spec: `internal/wiki/search.go`
  (`loadWikiPage`, `findBestFuzzyMatch`, the `index.md` fallback in `SearchWiki`),
  `internal/memory/search.go` (`chainFromPage`/`pageChainFields`, the frontmatter fallback of
  a markdown scan that no longer exists), `internal/uiserver/wiki_handler.go` (six places),
  `cmd/graphit/commands/runners.go` (`knowledge list` reads the directory),
  `internal/hub/registry.go` (`prepareKnowledgePublish` stages the pages beside the Parquet
  tables). Constraint: `internal/memory/{repair,consolidate,important}.go` read the RAW
  store, not the wiki — they are T2 and stay.

- [x] **T1.3f — `graphit wiki export`.** Spec: a new `internal/wiki/export.go` rendering the
  index into an Obsidian-compatible tree: `<slug>.md` with frontmatter marshalled from the
  columns, plus `index.md` from a `Browse` and `log.md` from `sync_log`. Constraint: the
  frontmatter is produced by `yaml.Marshal`, never by `Fprintf` — a title containing `: ` is
  the bug that cost 20 files, and a renderer is exactly where it would come back.

### 2026-09-01 (T1, slice 3: DONE — the knowledge generator stops writing pages, and nothing reads one)

All six items of the plan above landed. `go build -tags lancedb ./...`, `go test -tags lancedb ./...`
and `make lint` are green (0 issues); `make install` succeeded and the CLI was exercised against this
machine's real stores.

- [x] **T1.3a — the cross-reference graph is built from resolved edges.**
  `BuildCrossRefGraph(wikiDir)` is deleted. `BuildCrossRefGraphFromRefs([]PageEdges)` takes the edge
  set the producer already computed — `doc.crossRefs` after autolinking, which is the same set
  written to the `xrefs` table — and `BuildCrossRefGraphFromIndex(ctx, wikiDir)` reconstructs it from
  `chunks` + `xrefs` for a caller that only holds a directory. `InjectBacklinks` is now
  `CrossRefStats(graph)`: same four numbers, no file writes.
  The 354-phantom-broken-links guarantee moved one step earlier and is asserted where the edges are
  produced — `ExtractCrossRefs` → `FindWikiLinks` → `isBundlePageLink` — because that is the code
  that can still get it wrong.
  `injectBacklinksSection` SURVIVES, for the export: an Obsidian vault has no index to query.

- [x] **T1.3b — the index answers what the directory used to.** New on `WikiDB`: `Chunk(slug)`,
  `Chunks()`, `PageHashes()` (projected — slug and content_hash only), `PageTitles()` (projected),
  `AllXRefs()`, and `rowToChunk` as the inverse of `buildChunkRow`. `wiki.IndexedPageHashes` wraps
  the first for generators. **Six columns were added, and each one was a fact that had no other
  home:** `stale_since`, `stale_reason`, `revision`, `previous`, `next`, `created`.

- [x] **T1.3c — `GenerateKnowledgeWiki` writes nothing.** The render loop,
  `writePageIfChanged`, the `os.Remove` prune, the three `index.md` writes, the `log.md` append,
  `knowledgeEntityPage`, `knowledgeIndexPage`, `appendKnowledgeLog` and the `processCache == nil`
  frontmatter skip gate are all deleted. added/updated/deleted come from `IndexedPageHashes`; the
  cluster and staleness passes annotate the documents in memory instead of re-rendering pages.

- [x] **T1.3d — both lints read the index.** `LintWiki(ctx, …)` builds its graph from the index and
  checks columns; it now REFUSES an empty index rather than reporting "0 pages — no issues", which is
  the most misleading answer available given that opening a store creates it.
  `LintKnowledgeWiki(graph, docs, slugs)` takes the documents. Roughly 150 lines of frontmatter
  regexes went with them; only `parseFMInstant` survives.

- [x] **T1.3e — every remaining page reader.** `SearchWiki`'s `index.md` fallback → `WikiOverview`;
  `loadWikiPage` and `findBestFuzzyMatch` deleted; `parseMultiPageList` resolves a bare page name
  through the index; `memory.chainFromPage`/`pageChainFields` deleted; six sites in
  `uiserver/wiki_handler.go` moved to columns; `graphit knowledge list` reads `ListPagesAt`;
  `hub/registry.go`'s `prepareKnowledgePublish` no longer stages `.md` beside the Parquet tables.

- [x] **T1.3f — `graphit wiki export --out <dir>`.** New `internal/wiki/export.go` renders
  `<slug>.md` + `index.md` + `log.md` from `chunks`, `xrefs` and `sync_log`.

## Use Cases

### UC-01: A knowledge build compiles into the index and nothing else
- **Actor**: the daemon's docs watcher, or `graphit knowledge index`.
- **Preconditions**: a docs tree under `knowledge.docs_dir`; a wiki directory, empty or holding a
  previous index.
- **Main Flow**:
  1. `GenerateKnowledgeWiki` enumerates sources, applies `StatPreCheck`, and builds `knowledgeDoc`
     values from the process cache or from disk.
  2. `wiki.IndexedPageHashes` reads slug → content_hash from `chunks`; `FastPathCheck` returns early
     when the cache and the index already agree.
  3. Slugs are resolved, bodies are autolinked, and added/updated/deleted are decided against the
     indexed hashes.
  4. `BuildCrossRefGraphFromRefs` builds the graph from the resolved cross-references;
     `CrossRefStats` reports orphans, broken links and backlinked pages.
  5. Communities and staleness annotate the documents in memory.
  6. `LintKnowledgeWiki` audits the documents.
  7. `wiki.RebuildDB` writes `chunks`, `xrefs`, one `sync_log` row, and `meta`.
- **Alternative Flows**: cross-references unchanged (`CrossRefsUnchanged`) reuses the cluster cache
  and skips the graph entirely; nothing changed plus a non-empty index returns before the phases.
- **Error Scenarios**: `IndexedPageHashes` failing aborts the build with the store's error rather
  than silently treating every document as new; a `RebuildDB` failure is returned.
- **Postconditions**: the wiki directory holds `index.lance` and `shards/`. No `.md`.
- **Affected Files**: `internal/knowledge/wiki.go`, `internal/knowledge/lint.go`,
  `internal/wiki/{crossref,fastpath,store}.go`.

### UC-02: The explorer serves a wiki out of its index
- **Actor**: the web UI, over `/api/wiki/{modules,pages,page,search,ai-search}`.
- **Preconditions**: a compiled index at the resolved wiki directory.
- **Main Flow**:
  1. `/modules` resolves each wiki through `internal/store` and reports `Pages` from `Stats` and
     `HasLog` from the `sync_log` row count.
  2. `/pages` calls `Chunks` + `AllXRefs` and projects each row through `chunkPageMeta`.
  3. `/page` treats `path` as a slug and calls `Chunk`; the body is the `body` column.
  4. `/search` calls `BM25Search` once.
  5. `/ai-search` builds its catalogue from `Chunks`, cutting each body at 300 characters.
- **Alternative Flows**: `path` arrives with or without `.md`, and with a leading `/`.
- **Error Scenarios**: no index → 404 on `/page` and an error field on `/ai-search`; a path-shaped
  `path` → 404, because a wiki is flat.
- **Postconditions**: no file under the wiki directory is opened.
- **Affected Files**: `internal/uiserver/wiki_handler.go`.

### UC-03: Markdown on demand
- **Actor**: a person, via `graphit wiki export --out <dir>` (`--wiki memory`, `--context <name>`).
- **Preconditions**: a compiled index holding at least one page.
- **Main Flow**:
  1. `ExportMarkdown` reads `Chunks` and `AllXRefs` and builds the graph.
  2. Each chunk is rendered to `<slug>.md`: frontmatter via `yaml.Marshal`, the H1, the superseded
     banner, the stale banner, the breadcrumb, the summary, the provenance line, the
     cross-references, the body, and the backlinks section.
  3. `index.md` is the catalogue grouped by type, carrying `okf_version` and nothing else (§8/§12).
  4. `log.md` replays `sync_log` oldest-first through `AppendOKFLogEntries`, so the newest date is on
     top.
- **Error Scenarios**: an empty `--out` is refused; a wiki with no pages is refused rather than
  producing an empty directory.
- **Postconditions**: `outDir` holds one file per page plus the two reserved names. The wiki
  directory is untouched.
- **Affected Files**: `internal/wiki/export.go`, `cmd/graphit/commands/wiki.go`,
  `cmd/graphit/commands/runners.go`.

## Test Cases & Acceptance Criteria

### Feature: Compilation writes only to LanceDB
Ref: UC-01

#### Scenario: a knowledge build leaves no markdown behind
```gherkin
Given a docs tree with two documents and an empty wiki directory
When GenerateKnowledgeWiki runs
Then the wiki directory contains index.lance and shards and no .md file
  And both documents are reachable by slug through ListPagesAt
```

#### Scenario: a deleted document leaves the index
```gherkin
Given a compiled wiki holding page1 and page2
When page2's source is removed and the build runs again
Then page2's slug is absent from the index
  And the sync_log records it under Deleted
```

### Feature: The index answers what the directory used to
Ref: UC-01

#### Scenario: the incremental skip compares the index, not a listing
```gherkin
Given every source hash is unchanged in the process cache
When FastPathCheck runs against an index holding exactly those slugs
Then it returns true
But it returns false when the index is missing one of them
```

### Feature: The lint reads columns
Ref: UC-01

#### Scenario Outline: a page is reported for what its columns say
```gherkin
Given a compiled wiki holding one page whose "<column>" is "<value>"
When LintWiki runs with staleDays 30
Then the page appears in "<bucket>"

Examples:
  | column       | value      | bucket        |
  | doc_type     | (empty)    | MissingFields |
  | title        | (empty)    | WeakFields    |
  | word_count   | 1          | EmptyPages    |
  | updated      | 2020-01-01 | StalePages    |
  | stale_since  | today      | StalePages    |
  | updated      | (empty)    | (none)        |
```

#### Scenario: an empty index is refused rather than called clean
```gherkin
Given a directory that holds no compiled wiki
When LintWiki runs against it
Then it returns an error naming the directory
  And it does not report "0 pages — no issues found"
```

### Feature: Markdown on demand
Ref: UC-03

#### Scenario: a hostile title survives the round trip
```gherkin
Given an indexed page titled "Storage: where every artifact lives"
  And a summary beginning with "> "
When the wiki is exported to markdown
Then the page's frontmatter parses as YAML
  And the title reads back exactly, colon included
```

#### Scenario: an archived revision states its place in the chain
```gherkin
Given an indexed page with superseded true, revision 2, previous "0000" and next "01ENTITY.md"
When the wiki is exported to markdown
Then the frontmatter carries id, superseded, current, revision_id, revision, previous and next
  And the body opens with a banner naming the revision before and after it
```

#### Scenario: the log is replayed from the sync history
```gherkin
Given an index whose sync_log holds one entry adding "alpha" on 2026-08-29
When the wiki is exported to markdown
Then log.md carries no frontmatter
  And it groups the entry under "## 2026-08-29"
  And the entry reads "* **Creation**: Added [Alpha](alpha.md) — The first page."
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/wiki/crossref.go` | Modified | graph from resolved edges; `InjectBacklinks` → `CrossRefStats`; `BuildCrossRefGraph` deleted |
| `internal/wiki/store.go` | Modified | `Chunk`, `Chunks`, `PageHashes`, `PageTitles`, `AllXRefs`, `rowToChunk`; six new columns |
| `internal/wiki/types.go` | Modified | `WikiChunk` gains `Revision`, `Previous`, `Next`, `Created`, `StaleSince`, `StaleReason` |
| `internal/wiki/fastpath.go` | Modified | `IndexedPageHashes` |
| `internal/wiki/lint.go` | Modified | reads the index; frontmatter regexes deleted; `Fix` documented as inert |
| `internal/wiki/search.go` | Modified | catalogue from `WikiOverview`; `loadWikiPage`/`findBestFuzzyMatch` deleted |
| `internal/wiki/multi_search.go` | Modified | `wikiOverview` exported; `parseMultiPageList` takes a ctx |
| `internal/wiki/export.go` | Created | the only producer of markdown |
| `internal/knowledge/wiki.go` | Modified | no page, no `index.md`, no `log.md`; three renderers deleted |
| `internal/knowledge/lint.go` | Rewritten | takes documents, not a directory |
| `internal/memory/wiki.go` | Modified | `memoryEntityPage`, `memoryIndexPage`, `appendMemLog`, `memoryPageLink`, `firstLine` deleted; chain and `created` into columns |
| `internal/memory/search.go` | Modified | the frontmatter chain fallback deleted |
| `internal/uiserver/wiki_handler.go` | Modified | six page readers and the whole frontmatter parser replaced by columns |
| `internal/hub/registry.go` | Modified | publish stages the tables only |
| `cmd/graphit/commands/wiki.go` | Modified | `wiki export` |
| `cmd/graphit/commands/runners.go` | Modified | `runWikiExport`; `knowledge list` from the index; `LintWiki` takes a ctx |
| `internal/mcpstdio/tools_knowledge.go` | Modified | `LintWiki` takes a ctx |
| `internal/uiserver/wiki_fixture_test.go` | Created | `indexPage`/`indexChunk`, the fixture shape that replaced `os.WriteFile` |
| `internal/wiki/export_test.go` | Created | the page renderer's guarantees, at their new home |
| `internal/uiserver/wiki_okf_frontmatter_test.go` | Deleted | it tested a frontmatter parser that no longer exists |

## Trade-offs & Decisions

- **`CrossRefResult.BacklinksAdded` keeps its name and changes meaning** — it counted file writes,
  it now counts pages with at least one inbound reference. Renaming it would have reached
  `WikiResult`, `SyncLogEntry`, the `sync_log.backlinks_added` column, the CLI output and the UI, for
  a quantity a reader interprets the same way either side of the change.
- **added/updated is decided on the SOURCE content hash, not on rendered bytes.**
  `writePageIfChanged` compared the rendered page, so a document counted as "updated" when a sibling
  was added and changed its autolinks. That distinction existed to avoid a pointless file write;
  there is no file, and `RebuildDB` rewrites every row regardless.
- **Six columns rather than accepting the loss.** `stale_since`/`stale_reason` and
  `revision`/`previous`/`next`/`created` existed only in page frontmatter. Dropping the page without
  them would have removed the staleness pass's only output and made the memory revision chain
  unwalkable — silently, which is the failure mode this task's own notes warn about.
- **`LintConfig.Fix` is kept and inert.** Its one repair was a missing `## Backlinks` section, and
  the `xrefs` table cannot be missing an edge the graph is built from. The flag stays because the CLI
  flag and the MCP parameter are documented; T5 has to correct the documentation.
- **The fuzzy page-name match is gone, not ported.** `loadWikiPage` fell back to a 0.65 trigram match
  over the directory listing. The model is handed the catalogue as slugs and asked for slugs, and a
  wrong one is answered with the list of what exists. `CleanForFuzzy`/`TrigramSimilarity` survive —
  `FindBestFuzzyTitleMatch` still resolves wikilink targets by title.
- **`wiki export` writes where the caller asks, never into the wiki directory.** Rendering back into
  the wiki would recreate exactly the second copy this task removes.

## Technical Debt
- [x] **`wiki_source` returns the body, so an agent cannot read a page's frontmatter — and the memory
  protocol still tells it to.** CLOSED 2026-09-01: `ReadPageFrom` renders the header from the columns
  and prepends it, sharing one frontmatter definition with the markdown export. Verified against the
  real memory store — `graphit_wiki_source(pattern: "previous", after: 1)` returns the chain link on
  85 pages that carry one. The original text is kept below because it explains WHY the columns were
  added, which is still the load-bearing fact. The skill documents walking a revision chain by reading `previous` /
  `next` off the page with `graphit_wiki_source(pattern: "previous")`. That has been unreachable since
  slice 1, because a page read is the `body` column and the chain was in the file's frontmatter. The
  DATA is no longer lost (the columns landed here), but there is no surface: either `wiki_source`
  grows an option to prepend the rendered frontmatter, or `memory_history <id>` is added. **T5 must
  not simply reword the skill — the capability has to exist first.** This is the same gap as the
  existing "`graphit_memory_list` has no way to ask for a chain's revisions" debt in
  `docs/tasks/memory-revision-chain-searchable-history.md`; they should be closed together.
- [ ] `WikiPageMeta.Tags` is DERIVED — `[doc_type]`, plus `important` — because tags were never a
  column. The generators wrote `[knowledge|memory, <type>]` plus `important`/`superseded`, so nothing
  a reader used is missing, but it is a synthesis rather than a stored value. A `tags` column would
  make it a read.
- [ ] `LintConfig.Deep` (AI contradiction detection) was never wired to anything in `LintWiki` and
  still is not. Untouched by this pass, but now visibly inert beside `Fix`.
- [ ] The knowledge `manifest.json` and `clusters.json` are still files in the wiki directory. They
  are process-cache artifacts rather than wiki content, so they are not part of "one artifact per
  wiki" as stated — but they are the third artifact the objective wants collapsed, and T4 is where
  they should go.
- [ ] `internal/wiki/transfer.go` (`ExportToParquet`/`ImportFromParquet`) is still the Hub transport.
  D4 kills it; that is T3, and `prepareKnowledgePublish` is now down to that one call.
- [ ] `search_body` still repeats `body` — unchanged, and measured in
  `docs/tasks/memory-revision-chain-searchable-history.md`.

## System Knowledge

- **A `Query` with no predicate is refused by the engine, so every "read everything" query needs an
  always-true one.** `Browse` uses `word_count >= 0`; `AllXRefs` needs `source_slug IS NOT NULL`,
  because `xrefs` has no numeric column. This is a real constraint, not a style choice — the filter
  is how the scan is expressed.
- **`Query.Columns` is the difference between a cheap query and an expensive one on this table.**
  `chunks` carries the body, the derived `search_body`, and the embedding; `PageHashes` and
  `PageTitles` project two columns each and are run on every build.
- **A private `WikiDB` method does not call `ensureTables`, and that is a real trap.** `slugTitles`
  assumed open tables because its only caller was `FindXRefs`, which ensures them. Calling it from
  `BuildCrossRefGraphFromIndex` panicked on a nil table — a nil-pointer dereference inside
  `lancestore.Table.Search`, not a returned error. Exported entry points ensure; private helpers do
  not. `PageTitles` exists for exactly that reason.
- **`RebuildDB` drops and recreates all four tables, so adding a column is safe on the next full
  build and invisible until then.** An index written by the previous binary answers a new column as
  empty rather than failing — which is why the schema change needed `make install` plus
  `--reset`, and why a stale daemon binary would drop the new index (see the memory that records
  that).
- **`ReadPageFrom` returns the BODY, which for knowledge is the source document verbatim** — its own
  authored frontmatter included, since `doc.body` is the file's content. So `wiki source` on a
  knowledge page shows the document's frontmatter, and on a memory page shows none: memory passes
  `extractBodyAfterFrontmatter`. That asymmetry predates this slice and explains why the export
  renderer calls `StripFrontmatter` before writing its own block.
- **Measured on this machine after the change**: memory 403 pages (316 live + 87 archived revisions),
  0 `.md`; knowledge 283 pages, 0 `.md`. All 403 and all 283 exported pages parse as YAML and carry a
  non-empty `type`, including the titles containing `: `, which the encoder quotes.

## Progress Log

### 2026-09-01 (T1 slice 3)

- Found the working tree holding a partial revert of `7aa2f89` and restored the three files from
  `HEAD`; the inconsistency was provable, not a judgement call — `7aa2f89`'s test asserted the
  index-based `FastPathCheck` against a `ReadDir`-based implementation and failed.
- Corrected the premise about `StatPreCheck`: it never read the wiki directory. Its deletion signal
  is Phase B failing to read a vanished SOURCE file, and its index gate has been `IndexHasContent`
  since before this work. Only its doc comment was stale.
- Landed T1.3a–f in dependency order. The order mattered: removing the write loop first would have
  emptied the cross-reference graph, the communities, the staleness pass and both lints, since all
  four read the files the loop wrote.
- **Two facts were about to be lost silently, and both were caught by asking what a deleted renderer
  carried that no column did:** staleness (`stale_since`/`stale_reason`) and the memory revision
  chain (`revision`/`previous`/`next`, plus `created`). Six columns rather than six regressions.
- **Found a defect slice 1 introduced and this slice could not fix:** `wiki_source` returns the body,
  so the revision chain the memory protocol tells an agent to read off the page has been unreachable
  since `1f00ead`. The data now exists as columns; the surface does not. Recorded as the first debt
  item, with the note that T5 must build the capability before rewording the skill.
- Converted the test suites rather than deleting their assertions: `indexedWikiWithXRefs` and
  `lintChunk` in `internal/wiki`, `indexedKnowledgeSlugs`/`indexedKnowledgeBody` in
  `internal/knowledge`, `indexPage`/`indexChunk` in `internal/uiserver`, and `writeWiki` compiling an
  index with a sync-log row so `HasLog` still means something. The page-renderer tests deleted from
  `internal/knowledge` and `internal/memory` were replaced by `internal/wiki/export_test.go`, and
  `internal/knowledge/okf_conformance_test.go` now asserts OKF against the EXPORT, which is where
  markdown comes from.
- `internal/uiserver/wiki_okf_frontmatter_test.go` deleted outright: it tested a frontmatter parser
  the explorer no longer has. The guarantees it protected became structural — a column cannot be
  misparsed.
- Backed up the raw memory store to `/tmp/memory-raw-backup-t1s3` before the reindex (316 live
  memories), per this log's own warning.
- Verified: `go build`, `go test ./...`, `make lint` (0 issues), `make install`,
  `graphit memory index --reset`, `graphit knowledge index --reset`, `graphit wiki browse/log/xrefs/
  source`, `graphit knowledge list/lint`, and `graphit wiki export` for both wikis with every
  exported page's frontmatter parsed back.

**Next: T0** — prove remote (`s3://`) writes with commit-conflict retry under two concurrent writers
against MinIO, and a rollback to an earlier table version through `PruneVersions`. It gates D2 and
therefore T2. T1 is not fully closed: `wiki_source`'s inability to serve frontmatter is a T1-shaped
hole recorded above, and it should be closed before T5 rewrites the skills that depend on it.

### 2026-09-01 (T0: proving the two properties T2 rests on — PLAN)

T0 is a test, not a document, and it gates D2 and therefore T2. What is being proved is narrow and
was chosen because both halves are currently ASSUMED by the plan and neither is exercised anywhere:

**(a) a remote (`s3://`) Lance store can be WRITTEN, with retry on commit conflict, by two
concurrent writers, against MinIO.** Today it cannot be written at all: `Store.remote` is derived
from the URI scheme in `lancestore.Open`, and every mutating method — `Append`, `Upsert`,
`DeleteWhere`, `DeleteByKey`, `CreateTable`, `DropTable`, `EnsureIndexes`, `Compact`,
`PruneVersions`, `FoldNewRowsIntoIndexes` — opens with `if t.store.remote { return ErrReadOnly }`.
That is deliberate, and the package header says why: a published Hub artifact is immutable from a
consumer's side. So this cannot be "remove the guard"; the guard has to become a statement of
INTENT that a Hub consumer never makes.

**(b) an earlier table version is readable after a destructive write, through `lancestore`, and
retention is configurable.** The pinned `lancedb-go` SHA has the whole surface — `ListVersions`,
`Checkout`, `CheckoutLatest`, `Restore` — but on `contracts.ITableTimeTravel`, an OPTIONAL
capability interface reached by type assertion rather than part of `ITable`. `lancestore` exposes
none of it, and `lanceWikiVersionRetention` is a 15-minute constant whose own comment says
"nothing here uses time travel", which is precisely the premise D2 would overturn.

- [x] **T0.1 — a MinIO harness for tests.** Spec: start MinIO in Docker on a free port, create a
  bucket, return a `config.S3Config` pointing at it, tear it down. Skip — not fail — when Docker
  is unavailable, since a machine without it must still pass `go test`. Constraint: the two
  settings a compatible server needs are already derived by `Config.storageOptions` from the
  endpoint (`virtual_hosted_style_request=false`, and `allow_http=true` for an `http://`
  endpoint); the harness must not re-derive them, or the test proves the harness rather than the
  product.

- [x] **T0.2 — remote-writable is an explicit, opt-in intent.** Spec: `Config.Writable`, and the
  guards move from `store.remote` to a `store.readOnly` computed as `IsRemote() && !Writable`.
  Done means a remote store still refuses every write BY DEFAULT — a consumer of a published
  artifact does not set the field and cannot fork what the registry names — while a caller that
  states the intent can write. Constraint: `Store.Remote()` keeps its current meaning ("this is
  object storage"), because `WikiDB.Maintain` and `Rebuild` use it to decide whether maintenance
  applies at all; a second concept needs a second name.

- [x] **T0.3 — commit-conflict retry.** Spec: bounded exponential backoff with jitter around the
  mutating operations, and a classifier for the conflict. Constraint: **the classifier must be
  derived from the error the engine actually produces under two concurrent writers, not from a
  guess at its wording** — the same rule that caught `s3://` being compiled out of the published
  native. So the test comes first and reports the error, and the classifier is written against
  what it printed.

- [x] **T0.4 — time travel on `lancestore`.** Spec: `Table.Versions(ctx)`,
  `Table.CheckoutVersion(ctx, v)`, `Table.CheckoutLatest(ctx)` and `Table.RestoreVersion(ctx, v)`,
  each through a type assertion to `contracts.ITableTimeTravel` with a named error when the
  backend does not implement it. Done means a test writes rows, destroys them, restores the
  earlier version, and reads them back. Constraint: `Checkout` pins the table and REJECTS writes
  until `CheckoutLatest` or `Restore`, so `RestoreVersion` is a two-step (`Checkout` then
  `Restore`) and must not leave the handle pinned on failure.

- [x] **T0.5 — retention becomes a policy.** Spec: the 15-minute constant becomes configurable, so
  memory can keep versions long enough for them to be a recovery mechanism while knowledge keeps
  today's margin. Done means a test proves a version SURVIVES a prune under a long retention and
  is reclaimed under a short one. Constraint: **sub-second `olderThan` prunes nothing** — measured
  2026-08-28, `PruneVersions(ctx, time.Nanosecond)` reports `OldVersions: 0` while the versions
  plainly exist — so the "reclaimed" half of the test needs a real ≥1s window and a sleep, and
  asserting on `BytesRemoved` alone is asserting on an implementation detail.

- [x] **T0.6 — answer D2.** Spec: record the verdict in this log's Decisions section with the
  evidence, and say plainly whether T2 may start.

### 2026-09-01 (T0: DONE — both properties proved, and the plan's assumption about concurrency was wrong)

`go build -tags lancedb ./...`, `go build ./...`, `go test -tags lancedb ./...` and `make lint`
(0 issues) are green; `make install` succeeded. Six tests in `internal/lancestore` run against
MinIO in Docker and pass in ~8s.

- [x] **T0.1 — MinIO harness.** `internal/lancestore/minio_harness_lancedb_test.go`. Starts
  `minio/minio:latest` on a reserved free port, creates a bucket with `mc` inside the container,
  removes it on cleanup, and SKIPS when Docker is missing or unreachable. It passes only the
  endpoint and the credentials: path-style addressing and `allow_http` are derived by
  `Config.storageOptions` exactly as in production, so the test exercises the product's own
  derivation rather than the harness's.

- [x] **T0.2 — remote-writable is an explicit intent.** `Config.Writable` and `Config.ReadOnly()`;
  `Store` gained a `readOnly` field, and all eleven write guards moved from `remote` to it.
  `Store.Remote()` keeps its meaning, because `WikiDB.Maintain` and `Rebuild` read it to decide
  whether maintenance applies at all. `TestARemoteStoreRefusesEveryWriteUnlessTheCallerAsked`
  walks every mutating entry point on a default remote store and asserts `ErrReadOnly` on each,
  then asserts a read still works — a published artifact stays immutable, which is the property
  this must not have cost.

- [x] **T0.3 — commit-conflict retry, and THE ASSUMPTION IN THE PLAN WAS WRONG.**
  The plan expected the data writes to contend, citing Lance's own benchmark. Measured on MinIO
  with four concurrent writers on one remote table:

  | operation | writes | conflicts |
  |---|---|---|
  | append | 60 | 0 |
  | delete by key | 40 | 0 |
  | upsert, onto the SAME key | 40 | 0 |
  | build an inverted index | 9 | 0 |
  | **compact** | **9** | **6** |

  Lance's conflict resolver treats concurrent data writes as compatible transactions and settles
  them itself — even same-key upserts came through clean. What conflicts is the Rewrite class:
  compaction preempts compaction. So the retry earns its place on the MAINTENANCE path, which is
  where nobody was looking, and two processes maintaining one table is the normal case here: the
  daemon does it per project, and a shared memory scope would have one maintainer per unit.

  **And the first retry loop was useless, which is the more important finding.** Retrying the
  operation unchanged, three compactors failed 6 of 9 having burned 42 retries between them —
  every attempt conflicting on the same version number. A Rewrite is computed against the version
  the handle sits on; the winner moves the table forward and the losers keep re-submitting a
  rewrite of a version that is no longer the tip, so the conflict is permanent rather than
  transient. Adding `refreshToLatest` (a `CheckoutLatest` before each re-attempt) took it to
  **2 retries and 0 failures**. A retry that does not refresh its base is not a weaker retry — it
  is a busy-wait that cannot succeed, and it looks like resilience.

  The classifier matches on message text because the binding surfaces engine failures as plain
  errors with no code. It started as ten plausible phrases written from the API; the probe produced
  exactly one wording, and the seven invented ones were deleted. What remains is the three
  substrings that message actually contains.

- [x] **T0.4 — time travel on `lancestore`.** `internal/lancestore/timetravel_lancedb.go`:
  `Versions`, `CurrentVersion`, `CheckoutVersion`, `CheckoutLatest`, `RestoreVersion`, each through
  a type assertion to `contracts.ITableTimeTravel` with `ErrNoTimeTravel` when the backend lacks
  it. `RestoreVersion` is the two-step the binding requires and drops the pin if the promotion
  fails, so a failed recovery cannot leave a table that silently rejects every write.

- [x] **T0.5 — retention is a policy.** `wiki.version_retention` (`config.ResolveWikiVersionRetention`),
  defaulting to the 15 minutes that used to be a constant. **A sub-second value is refused in
  favour of the default**, which is measured rather than defensive: the engine prunes nothing at
  all below one second, so honouring `1ms` would silently disable pruning while reading as the
  most aggressive setting available.

- [x] **T0.6 — D2 answered YES.** Recorded in Decisions above with its evidence. T2 may start.

## Use Cases

### UC-04: A memory scope's table is written directly in object storage
- **Actor**: any unit sharing a memory scope — a CLI invocation, the daemon, an agent over MCP.
- **Preconditions**: an `s3://bucket/prefix` for the scope, and credentials the process can resolve.
- **Main Flow**:
  1. The caller opens the store with `lancestore.Config{URI: "s3://…", S3: …, Writable: true}`.
  2. `Append` / `Upsert` / `DeleteByKey` write to the table in the bucket; nothing is downloaded.
  3. A losing commit is retried with backoff after `refreshToLatest` brings the handle to the tip.
  4. Maintenance (`Compact`, `PruneVersions`, `FoldNewRowsIntoIndexes`) runs through the same retry.
- **Alternative Flows**: a consumer of a published Hub artifact omits `Writable`, and every write
  returns `ErrReadOnly` while reads work.
- **Error Scenarios**: contention outlasting the retries surfaces `ErrCommitConflict` wrapping the
  engine's own message, which names the version the rewrite kept losing to; a backend without time
  travel cannot be refreshed, and the retry proceeds without it rather than failing.
- **Postconditions**: every writer's rows are present. Verified: 24 concurrent appends from two
  independent stores, all 24 rows readable afterwards by a third, read-only store.
- **Affected Files**: `internal/lancestore/{config,store_lancedb,timetravel_lancedb}.go`.

### UC-05: A destructive write is undone
- **Actor**: a person, or an agent recovering from a pass that damaged the store.
- **Preconditions**: the damaging write is younger than the configured retention, so its
  predecessor has not been pruned.
- **Main Flow**:
  1. `Versions(ctx)` lists the history newest-first.
  2. `CheckoutVersion(ctx, v)` pins the table so the snapshot can be inspected — a read-only act.
  3. `RestoreVersion(ctx, v)` promotes it: `Checkout` then `Restore`, appending a new version.
  4. `CheckoutLatest(ctx)` returns an inspection to the tip without promoting anything.
- **Error Scenarios**: `ErrNoTimeTravel` when the backend lacks the capability; `ErrReadOnly` on a
  published artifact; a failed promotion drops the pin so the table still accepts writes.
- **Postconditions**: the rows are back, the damaging version is still in the history, and the
  restore is itself undoable.
- **Affected Files**: `internal/lancestore/timetravel_lancedb.go`.

## Test Cases & Acceptance Criteria

### Feature: A remote store is writable only on request
Ref: UC-04

#### Scenario: a published artifact refuses every write
```gherkin
Given a remote store opened with no stated write intent
When each of CreateTable, DropTable, Append, DeleteWhere, DeleteByKey, Upsert, EnsureIndexes, Compact, PruneVersions, FoldNewRowsIntoIndexes and RestoreVersion is called
Then every one returns ErrReadOnly
  And Remote() is still true
  And a read of the table still succeeds
```

#### Scenario: two writers on one remote table both commit
```gherkin
Given two independently opened writable stores on one s3:// table
When each appends 12 rows starting from a shared barrier
Then the table holds all 24 rows
  And each writer's 12 rows are individually readable
```

### Feature: Concurrent maintenance does not fail
Ref: UC-04

#### Scenario: three compactors contend and all succeed
```gherkin
Given a remote table with eight fragments and two tombstoned keys
When three independently opened stores each compact it three times from a shared barrier
Then no compaction returns an error
  And the row count is unchanged
```

### Feature: A destructive write is recoverable
Ref: UC-05

#### Scenario: restoring the version before a delete brings the rows back
```gherkin
Given a remote table holding six rows at a known version
When every row is deleted
  And that known version is restored
Then the table holds the six original rows
  And the current version is NEWER than the restored one
  And a subsequent write succeeds
```

#### Scenario: checking out a version does not promote it
```gherkin
Given a remote table written twice, with two rows at the first version
When the first version is checked out
Then the table reads two rows
When CheckoutLatest is called
Then the table reads every row again
```

### Feature: Retention decides what a prune reclaims
Ref: UC-05

#### Scenario Outline: the window is the safety net
```gherkin
Given a remote table with four versions, all older than one second
When PruneVersions is called with "<retention>"
Then it reclaims "<outcome>"
  And the four live rows are untouched

Examples:
  | retention | outcome  |
  | 1h        | nothing  |
  | 1s        | versions |
```

#### Scenario Outline: a sub-second retention is refused by configuration
```gherkin
Given wiki.version_retention is "<value>"
When the retention is resolved
Then it is the 15-minute default

Examples:
  | value          |
  | 1ms            |
  | 999ms          |
  | 0              |
  | -5m            |
  | not-a-duration |
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/lancestore/lancestore.go` | Modified | `Version`, `ErrNoTimeTravel`, `ErrCommitConflict` |
| `internal/lancestore/config.go` | Modified | `Config.Writable`, `Config.ReadOnly()` |
| `internal/lancestore/store_lancedb.go` | Modified | guards move to `readOnly`; the five mutating paths and the three maintenance paths go through the retry |
| `internal/lancestore/store_disabled.go` | Modified | mirrors the new surface, so the no-tag build keeps compiling |
| `internal/lancestore/timetravel_lancedb.go` | Created | the version history, the retry loop, the conflict classifier, `refreshToLatest` |
| `internal/lancestore/minio_harness_lancedb_test.go` | Created | MinIO in Docker, skipped when unavailable |
| `internal/lancestore/remote_write_lancedb_test.go` | Created | the six T0 proofs |
| `internal/config/config.go` | Modified | `wiki.version_retention` with a one-second floor |
| `internal/config/config_default_test.go` | Modified | the retention resolver, including every refused value |
| `internal/wiki/store.go` | Modified | the retention constant became `config.WikiVersionRetention()` |

## Trade-offs & Decisions

- **Writable is an INTENT on the config rather than a permission derived from the URI.** The scheme
  conflated "this is object storage" with "this is somebody else's artifact", and only the second is
  about permission. Keeping the default read-only is what preserves Hub immutability with no extra
  mechanism: a consumer simply never sets the field.
- **The classifier matches message text, and that is stated as a weakness rather than hidden.**
  There is no error code or sentinel in the binding. The failure direction was chosen deliberately:
  an upstream rewording makes a retryable conflict fail loudly instead of being silently retried
  forever.
- **The retry refreshes before every re-attempt, including on the data writes** that were measured
  not to conflict. It costs one call on a path that is not currently reached, and the alternative —
  refreshing only where a conflict was observed — bakes today's measurement into the control flow.
- **The MinIO tests SKIP without Docker rather than failing.** A machine without Docker must still
  pass `go test`; GitHub's `ubuntu-22.04` runners have it, so CI does exercise them.

## Technical Debt
- [ ] **`WikiDB` has no way to open a scope writable, and `Rebuild` still refuses a remote store.**
  T0 proved the capability in `lancestore`; the layer above it does not use it yet. `OpenWikiDB`
  takes only a directory, and `Rebuild` opens with `if w.Remote() { return ErrReadOnly }`. T2 needs
  a way to say "this scope's table lives at this URI and I may write it".
- [ ] **The retry is per-operation, not per-transaction.** `Upsert` is a delete followed by an
  append, so a conflict retried inside the delete does not re-derive the append. It is safe under
  the single-writer guarantee the incremental has today, and the measurements say concurrent
  same-key upserts do not conflict — but "does not conflict today" is not "is atomic".
- [ ] **Nothing prunes a remote table on a schedule.** `WikiDB.Maintain` returns early for a remote
  store, which was right when remote meant somebody else's artifact. A memory scope in a bucket
  needs an owner for its maintenance, and now that maintenance is the operation that contends, it
  wants one maintainer rather than one per unit.
- [ ] `wiki.version_retention` is read but never written by `setup`, and there is no CLI surface to
  READ a single config key back (`graphit config <key>` prints usage). Neither is new.
- [ ] The `internal/ast` package segfaulted once during a full `go test -tags lancedb ./...` run and
  passed both on its own and on a re-run of the whole suite. Not touched by this work; recorded so
  the next person who sees it knows it has been seen before.

## System Knowledge

- **Concurrent DATA writes to one Lance table do not conflict; concurrent COMPACTION does.** Four
  writers, 140 data writes, zero conflicts — including upserts onto the same key. Three compactors,
  9 runs, 6 failures before the retry was fixed. The conflict is the Rewrite class preempting
  itself.
- **A retry must refresh its base or it cannot succeed.** A Rewrite transaction is computed against
  the version the handle holds; without a `CheckoutLatest` between attempts every retry re-submits
  a rewrite of a stale version and conflicts identically. Measured: 42 retries and 6 failures
  without the refresh, 2 retries and 0 failures with it.
- **The exact conflict wording**, which is what the classifier is built from:
  `lance error: Retryable commit conflict for version N: This Rewrite transaction was preempted by
  concurrent transaction Rewrite at version N. Please retry.` from
  `rust/lance/src/io/commit/conflict_resolver.rs`.
- **Time travel is an OPTIONAL capability interface** (`contracts.ITableTimeTravel`), reached by
  type assertion, so every call needs a fallback path. `Checkout` pins the table and the engine
  rejects writes while pinned — which makes a failed `Restore` dangerous if it leaves the pin.
- **A restore is a forward operation.** It appends a version rather than truncating the history, so
  the damaging state remains available and the recovery can itself be recovered from.
- **`PruneVersions` below one second prunes nothing**, confirmed again here. The one-second floor in
  `ResolveWikiVersionRetention` exists so a configured `1ms` cannot silently disable pruning.
- **MinIO readiness is a TCP dial, not a health endpoint.** The health path has moved between
  releases; what the harness needs to know is only whether the port serves, plus a short settle
  before the first object request.

## Progress Log

### 2026-09-01 (T0)

- Read the surface before designing: the write guards all tested `store.remote`, so `s3://` could
  not be written at all, and the version history existed in the pinned `lancedb-go` SHA but only on
  an optional capability interface `lancestore` did not expose.
- Landed T0.1, T0.2 and T0.4, then wrote the concurrency test — which PASSED with zero conflicts
  and therefore proved nothing about the retry. Added a retry counter so a loop that never ran is
  distinguishable from one that worked, cranked the contention, and still got zero.
- Wrote a throwaway probe across five shapes of concurrent write to find out what actually
  conflicts. It found that only compaction does, and printed the engine's exact wording — which is
  what the classifier is now built from, replacing ten phrases invented from the API with the three
  that message contains. The probe was deleted once its findings were encoded in the permanent
  tests and comments.
- The compaction test then failed 6 of 9 despite 42 retries, all on the same version. That is the
  session's most useful finding: a retry without refreshing its base is a busy-wait wearing the
  costume of resilience. `refreshToLatest` took it to 2 retries and 0 failures.
- Made retention a policy with a one-second floor, and verified the key round-trips into the global
  config (`~/.graphit/config.json` gains `wiki.version_retention`); unset it afterwards rather than
  leaving a machine-level setting behind.
- Fixed the one lint finding this produced (`errorlint`: the exhausted-retry error now wraps both
  `ErrCommitConflict` and the engine's last message, so a caller can match the sentinel and a person
  can read which version the rewrite lost to).

### 2026-09-01 (a staleness bug this work EXPOSED, found by checking my own claim)

Asked whether the work was complete, I checked the task log's own checkboxes — still `[ ]` for
everything — ticked them, and then verified the index reflected the edit. It did not. Chasing that
found a real bug, and it is the exact failure mode this whole task exists to remove.

**Symptom.** Editing `docs/tasks/…md` and running `graphit knowledge index` reported
`0 articles` in 110 ms and the index kept serving the previous content. Reproduced deterministically:
`--reset`, append one line, index, and the line is absent.

**Two gates, both taking their evidence from the thing the same pass populates.**

1. `FastPathCheck` asked `processCache.HasChanged(key, hash)`. The generation pass calls
   `processCache.Store` with each document's NEW hash while assembling the documents, and only
   then calls the check — so the check asked a question it had already answered, and said "nothing
   changed" about a document that had just been edited.
2. `StatPreCheck` ended at `IndexHasContent` — "the index has rows" — which asks nothing about
   whether those rows correspond to these sources. Worse, it is reachable in a state that is
   PERMANENT: the pass saves the cache before it rebuilds the index, so anything stopping it in
   between (a crash, a cancelled context, or gate 1 above) leaves a cache saying "already
   processed" beside an index that never got the work. Every file then stat-matches, no hash is
   computed, and the run is waved through. Forever. That is why fixing gate 1 alone did not help:
   the earlier buggy run had already poisoned the cache, and the run never reached gate 1.

**Whose bug it is.** Gate 1's flaw is as old as the cache check; the previous incident recorded in
`FastPathCheck`'s own comments — "any fast-path condition based only on the processCache is
vacuously true on the first run, because the cache is populated by the same pass" — described this
mechanism and then fixed only the cold-start half of it. It stayed invisible because search fell
back to scanning the `.md` pages, which WERE rewritten from the fresh sources: the stale index was
answered around. Slice 1 deleted that fallback, which is what turned a masked defect into a visible
one. So this work did not create the bug; it removed the thing that was hiding it.

**Fix.** Both gates now take their evidence from the INDEX, which is written at the END of the pass
and therefore records what the last pass achieved:

- `FastPathCheck` compares the entries against `IndexedPageHashes` — slug set and content hash. The
  three old conditions (cache unchanged, slugs present, nothing orphaned) collapse into one
  comparison that covers additions, deletions and edits alike.
- `StatPreCheck` ends at `indexHoldsCachedSources`, which requires the index's row count and its
  multiset of `content_hash` values to match what the cache claims it processed.
- `StatPreCheck` gained a `ctx` instead of fabricating `context.Background()`.

**Regression tests**, each reproducing the arrangement rather than the symptom:
`TestFastPathCheckIgnoresAProcessCacheThatAlreadyAgrees`,
`TestFastPathCheckNoticesAdditionsAndDeletions`,
`TestStatPreCheckRefusesAnIndexThatDoesNotMatchTheCache`.

Verified end to end after `make install`: the edit that had been skipped indexed as `1 articles`,
and removing the line again also landed — both directions, on the real store.

**Next: T2** — memory writes go straight to the table and the raw store retires. D2 is answered, so
it is unblocked. Its first obstacle is recorded as debt above: `WikiDB` has no way to open a scope
writable, and `Rebuild` still refuses a remote store, so the capability T0 proved is not yet
reachable from the layer that would use it. The one-shot migration of the 316 live memories in this
store — verified by count and content hash before the raw store is deleted — remains the part of T2
that is not a dropped code path.

### 2026-09-01 (T1's remaining hole: a page read has to include its frontmatter — PLAN)

The one item keeping T1 at `[~]`. It is a live defect rather than a tidiness issue: the memory
protocol instructs an agent to walk a revision chain by reading `previous` / `next` off the page
with `graphit_wiki_source(pattern: "previous")`, and that has returned nothing since `1f00ead`,
because a page read became the `body` column and the chain lived in the file's frontmatter.

**What the contract used to be, and why restoring it is the fix rather than a workaround.** Before
`1f00ead`, `wiki_source` did `os.ReadFile` on `<slug>.md` and returned frontmatter AND body — that
is what "read the page" meant to every caller and to both skills. The columns to rebuild that
header all exist now (six of them were added precisely so nothing was lost), so the read can be
made faithful again instead of the documentation being reworded to describe the loss.

- [x] **T1.4a — one definition of a page's frontmatter, shared by the export and the read.**
  Spec: lift the frontmatter struct out of `internal/wiki/export.go` so `ReadPageFrom` and
  `ExportMarkdown` build the same block from the same columns. Constraint: `generated` (OKF §5.2)
  needs the producing module's actor, which the index does NOT record — the export knows it from
  its `moduleTag` argument and a read does not. So it becomes optional on the struct and is
  omitted on the read path, which is honest: a read is not a published bundle file.

- [x] **T1.4b — `ReadPageFrom` returns frontmatter + body.** Spec: `db.Chunk` instead of
  `db.PageBody`, and the slicing (`head`, `tail`, line ranges, `pattern`) applies to the whole
  page, exactly as it did when the page was a file. Constraint: no flag. A second shape of "read a
  page" is the dual-read path this task exists to remove, and the frontmatter is a dozen short
  lines against a body measured in hundreds.

- [x] **T1.4c — prove the documented instruction works.** Spec: a test that reads a superseded
  memory page and finds `previous`, `next`, `revision`, `superseded` and `current` in what
  `wiki_source` returns, including through `pattern: "previous"` — the exact call the memory skill
  prescribes. Done means the skill's instruction is true again, so T5 has nothing to reword here.

### 2026-09-01 (T1.4 DONE — T1 is closed)

`go build` (both tags), `go test -tags lancedb ./...`, `make lint` (0 issues), `make install`, and
`graphit memory index --reset` all green.

- [x] **T1.4a — one frontmatter definition, two consumers.** `pageFrontmatter(chunk, actor)` and
  `RenderPageHeader` in `internal/wiki/export.go`. `Generated` became a pointer with `omitempty`,
  because §5.2 requires an actor naming the producing module and the index does not record one — the
  export knows it from `moduleTag`, a read does not, and inventing one would be a claim about
  provenance. A read is not a published bundle file, so §11 does not apply to it.
- [x] **T1.4b — `ReadPageFrom` returns frontmatter + body.** It reads `db.Chunk` instead of
  `db.PageBody` and prepends the rendered header. The slicing applies to the whole page, as it did
  when the page was a file. No flag: a second shape of "read a page" is the dual-read path this work
  removes.
- [x] **T1.4c — the documented instruction is true again.**
  `TestReadPageCarriesTheRevisionChainTheMemoryProtocolReadsOffIt` asserts every field the protocol
  names and then makes the exact call the skill prescribes. Verified end to end on the real store:
  85 memory pages carry a `previous`, and
  `graphit_wiki_source(pattern: "previous", after: 1)` returns
  `previous: history/01M1C91GPFY5AZMYW90GY1NMCX/0001.md`.
  A page with no chain carries no chain keys, so a knowledge page is not littered with empty memory
  fields — asserted, because the alternative would have been noise on 283 pages.

**A test that had quietly become vacuous, found while doing this.**
`TestReadPageSlicesLikeTheSourceTool` asserted that `head: 4` does not reach a term late in the body.
With the header prepended, the first four lines are frontmatter, so the assertion passed for a reason
that had nothing to do with slicing. It now pins where those lines come from — the delimiter and the
`type` key — so it fails if the header stops being part of the page. Worth noting as a pattern: an
assertion of the form "X is absent" survives a change of meaning silently, where "Y is present" does
not.

## Trade-offs & Decisions (T1.4)

- **The header is always included, with no opt-out.** The page IS its frontmatter and its body; that
  is what every caller and both skills mean by "read the page", and it is what the file-backed reader
  returned. A flag would reintroduce two shapes of the same read. The cost is a dozen short lines
  against a body measured in hundreds, and it is the same block the export writes.
- **`generated` is omitted on a read rather than filled with a guess.** The alternative — deriving
  the actor from whether `entity_id` is set — would encode "memory pages have entity ids" as a
  provenance claim, which is exactly the kind of inference that is right until it is not.

### 2026-09-01 (T2: the memory store becomes a table — DESIGN, and a sequencing decision)

Mapped `internal/memory` in full before specifying, because T2 rewrites its write path and three of
the constraints only exist in the details.

**What the raw store actually is, which is more than storage.** It is also the multi-writer
TRANSPORT: `ScopeStore.Publish` uploads every file in the directory to `s3://…/memory/<scope>/<id>/`
in the background, `Pull` merges without deleting, and conflict is per object with last-writer-wins —
which is safe today only because each memory is its own object named by a ULID, so two units ADDING
memories cannot collide. Any table design has to replace both roles, and T0 is what makes that
possible: concurrent appends, deletes and same-key upserts on a remote Lance table produced zero
conflicts.

**Two shapes were possible, and the choice is a sequencing decision rather than a taste one.**

- **Shape A — memories become rows of the wiki's own `chunks` table.** One artifact, which is what
  the objective asks for. It is currently UNSAFE: `RebuildDB` drops and recreates all four tables on
  every compile, so the memories would be destroyed by the next index. Shape A therefore requires T4
  (incremental `Upsert`/`DeleteByKey` instead of drop-and-rebuild) to land first.
- **Shape B — a `memories` table that is the STORE, with the wiki still compiled from it.** The
  authored data and the derived index keep their separate lifecycles: the store is multi-writer and
  versioned (its history is the recovery path D2 accepted), the index is derived, disposable and
  carries `search_body` and the vectors.

**Chosen: Shape B, and the plan's ordering is what decides it.** T2 sits before T4, and inverting
them to get Shape A would mean rewriting the incremental against a table that does not exist yet.
Shape B also keeps the blast radius honest: if the store is wrong, the index is rebuilt from it; if
the index is wrong, nothing authored is lost. What "one artifact" then means for memory is that the
markdown raw store disappears — four things become two, and T4 can collapse the last two later if it
proves worth it. **This deserves the user's attention rather than being buried: it is a deliberate
deviation from reading the objective as "one table, full stop".**

**Two whole mechanisms retire, and neither is a port.**

- **`repair.go` (≈300 lines) becomes unnecessary rather than rewritten.** Every one of its four
  passes exists to heal ONE corruption: a write path that recovered a memory id from the FILE NAME,
  which forked 184 memories into twins under ids like `<ulid>_important_`. In a table keyed by the
  declared id there is no file name to derive an id from, so the defect is not expressible. The
  repair runs one last time as part of the migration and then goes.
- **`shardsync.go` goes too**, because its purpose — shipping embedding vectors alongside raw
  markdown so a colleague's memory arrives without paying the model again — is served by an
  `embedding` column on the store table. That column is in the schema from the start; without it,
  retiring shardsync would silently make every unit recompute every vector.

**Six fields exist only in the file today and have no column anywhere**, so the record type carries
them or they are lost: `scope`, `scope_id`, `project_id`, `updated_by`, `tags`, and the REAL
`updated_at` (the wiki's `updated` column is stamped with the compile date, so a memory's actual
last-write time survives nowhere but the file). Three structural facts also have to be reproduced:
the `# <Title>` H1 that `renderMemoryFile` emits and `extractBodyAfterFrontmatter` strips back off;
the LOCATION under `history/` which `buildMemDoc` treats as more authoritative than the frontmatter
when deciding `superseded`; and the lexicographic order of names within `history/<id>/`, which is how
`backfillChainLinks` derives successors.

- [x] **T2.1 — the store table, and a verified migration into it. NOTHING switches over yet.**
  Spec: a `memories` schema and a `MemoryTable` (open local or remote-writable, `Upsert`, `Delete`,
  `Get`, `List`) plus a `MemoryRecord` carrying every field above; then a one-shot migration that
  reads the raw store — live memories AND `history/**` — and writes the table, verifying by ROW
  COUNT and by CONTENT HASH per record. Done means it runs against this machine's real store (316
  live memories, 403 rows including revisions) and reports a clean verification, with the raw store
  untouched. Constraint: the migration is the only backwards-compatibility this task allows, and it
  is not compatibility — it is 316 memories whose loss is data loss. It must be re-runnable and must
  never write a record it could not read: `ParseMemoryFrontmatterOK` returning false means SKIP and
  REPORT, never write a record built from an empty struct.
  Rationale for doing this first: it answers the one question that could invalidate every later
  slice — is the schema sufficient to hold a memory without loss — and it answers it against real
  data rather than a fixture.

- [ ] **T2.2 — the five write paths write the table.** `AddMemory`, `updateMemory`, `RemoveMemory`,
  `changeRelevance`, and the archive helpers. The revision chain stays as it is conceptually:
  `history/` stops being a directory and an archived revision becomes another row, keyed by
  `<id>/<revision_id>`.

  **The scope URI question this slice used to open with is ANSWERED, by the user, on 2026-09-02:
  the table lives directly at `s3://…/memory/<scope>/<id>` and there is no raw dir at all** — not
  as a cache, not as a staging area, not as a local-first tier that syncs. So this slice does not
  "choose" between a local directory and a remote one; it points the write paths at the remote URI
  and deletes the markdown surface behind them. `Config.Writable` from T0 is what makes that legal,
  and T0's retry-on-refresh is what makes it safe under two writers.

  Consequence for anything that still touches the raw store: it is **deleted, not adapted**. That
  includes `MemoryRawRoot`/`MemoryRawDir`/`RawDir`/`RawDirFor`, `ScopeStore`'s file surface
  (`WriteFile`/`ReadFile`/`RemoveFile`/`uploadDir`/`copyDirRecursive`/`ExtractScopeDir`),
  `Pull`/`Publish` as file transport, and any test whose subject is one of those.
- [ ] **T2.3 — every reader reads the table.** `ListMemories`, `ListImportantMemories`,
  `consolidate`, `GenerateMemoryWiki` (which compiles the wiki FROM the table instead of from files),
  and `dream`.
- [ ] **T2.4 — retire the raw store, `repair.go` and `shardsync.go`**, after the migration has run
  and verified. This is the slice that deletes data, so it is last and it is separate.

### 2026-09-01 (T2.1 DONE — the schema holds a memory without loss, proved on 409 real records)

`go build ./...` (no tag) and `go build -tags lancedb ./...`, `go test -tags lancedb ./...` and
`make lint` (0 issues) all green.

**The result that mattered.** The migration ran against this machine's real project scope — 320 live
memories and 89 archived revisions — and verified clean: nothing skipped, nothing mismatched, nothing
unreadable, 409 rows for 409 files. Then the records were read BACK out of the table and counted by
field, because a verified round trip proves nothing if the fields were empty to begin with:

| field | records carrying it | note |
|---|---|---|
| `updated_at` | **409 / 409** | the wiki's `updated` column holds the COMPILE date, so this survived nowhere but the file |
| `tags` | 408 | |
| `scope`, `scope_id` | 408 | |
| `type` | 388 | 21 memories are genuinely untyped |
| `updated_by` | 215 | only writes made after the field existed |
| `project_id` | **0** | expected: it is set only on USER-scoped memories and this is the project scope — see debt |
| `created_at` | 407 | |
| `important` | 311 | |
| `superseded` | 89 | equals the archived count, so location→column resolved correctly |
| `previous` / `next` | 84 / 86 | the chain crossed |
| titles containing `": "` | **172** | |

That last row is the strongest single result. 172 of 409 titles carry the shape that made memories
unreadable — measured at 47 when the bug was found, so it is far more widespread than anyone knew —
and every one of them round-tripped, because both ends now go through the YAML marshaller rather than
through `Fprintf`.

**A verification design that was wrong on the first attempt, and it is worth writing down.** The
first version hashed the record's re-rendered markdown against `ContentHash`, which is the hash of
the SOURCE FILE. That is a byte-identity check, and legacy files fail it by design: a title
single-quoted by the recovering parse, a different key order, or a field that postdates the file all
render to different bytes carrying identical facts. It would have reported all 409 records as
mismatched and proved nothing. What matters is that no FIELD was lost, so both sides are now
canonicalised through the same renderer and those hashes compared — normalisation cancels, a missing
field does not. `TestVerificationCatchesALostField` pins that the check can still FAIL, which is the
other half of a verification being worth anything.

**And a fixture that could not be broken the obvious way.** The "unreadable frontmatter" test first
used `title: [unclosed`, which the migration happily migrated — because `title` is one of the keys
`quoteUnquotedScalars` single-quotes when unquoted, so the recovering parse repairs it into
`title: '[unclosed'`. That is the recovering pass doing exactly its job. Breaking it required a key
the pass does not touch (`tags: {unclosed`). Useful fact: the recovering parse is stronger than its
own debt note suggests.

## Use Cases

### UC-06: A memory becomes a row
- **Actor**: `internal/memory`, on behalf of any write path (T2.2 wires them).
- **Preconditions**: a scope URI — a local directory, or `s3://bucket/memory/<scope>/<id>` for a
  scope shared by several units.
- **Main Flow**:
  1. `OpenMemoryTable(ctx, uri)` opens the store WRITABLE and ensures the `memories` table.
  2. `Put` upserts records on the `key` column: the bare id for a live memory,
     `<id>/<revision_id>` for an archived revision, so one chain holds many rows without collision.
  3. `Get` / `Live` / `Revisions` / `List` read it. `Live` excludes archived revisions, which is what
     every catalogue surface wants; `Revisions` orders by `revision_id`, preserving what the
     lexicographic order of file names used to give.
  4. `Maintain` folds new rows into the indexes, compacts, and prunes versions older than the
     configured retention.
- **Error Scenarios**: a record with no id is refused; `Delete` of an absent key is a no-op, because
  the caller's intent is that it must not be there.
- **Postconditions**: the record round-trips with every field, including the six that had no column
  anywhere before.
- **Affected Files**: `internal/memory/table.go`.

### UC-07: The raw store is migrated, and the migration proves itself
- **Actor**: an operator, once per scope, before the raw store is retired (T2.4).
- **Preconditions**: the raw markdown store exists.
- **Main Flow**:
  1. `MigrateRawStoreToTable(ctx, rawDir, targetURI)` lists the same two sets the compiler lists —
     the live memories and every `history/**` revision — so it cannot see a different corpus than the
     wiki does.
  2. Each file is parsed with `ParseMemoryFrontmatterOK`. A false result SKIPS and REPORTS.
  3. Records are written in batches of 100, then the scalar indexes are built.
  4. Every written record is read back and compared by canonical rendering.
- **Alternative Flows**: re-running produces the same table, because `Put` upserts on the key.
- **Error Scenarios**: a skipped file, a hash mismatch, an unreadable file, or a row count that
  disagrees each make `Verified()` false — and `Verified()` is what T2.4 must gate the deletion on.
- **Postconditions**: the raw store is UNTOUCHED. A migration that cleaned up after itself could not
  be re-run to prove itself again.
- **Affected Files**: `internal/memory/migrate.go`.

## Test Cases & Acceptance Criteria

### Feature: A row holds a memory without loss
Ref: UC-06

#### Scenario: every field survives the round trip
```gherkin
Given a memory record with all fifteen fields set
When it is written to the store and read back
Then each field equals what was written
  And the record renders to the same markdown
```

#### Scenario: a chain is several rows
```gherkin
Given a live memory and two archived revisions of it
When all three are written
Then the store holds 3 rows
  And Live() returns only the head
  And Revisions() returns 0001 then 0002
```

### Feature: The migration proves itself
Ref: UC-07

#### Scenario: a file whose frontmatter cannot be read is skipped, not migrated
```gherkin
Given a raw store with one good memory and one whose `tags` is an unclosed flow mapping
When the migration runs
Then the good memory is migrated
  And the broken file is listed in Skipped
  And the report does NOT verify
```

#### Scenario: the verification can fail
```gherkin
Given a record written to the store
When verification is asked about a record carrying an extra tag the store never saw
Then it reports that record as mismatched
But it reports nothing for the record actually written
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/memory/table.go` | Created | `MemoryRecord`, the `memories` schema, `MemoryTable` with Put/Delete/Get/List/Live/Revisions/Maintain |
| `internal/memory/migrate.go` | Created | the one-shot migration and its verification |
| `internal/memory/table_lancedb_test.go` | Created | round trip per field, chain keying, upsert/delete, and the four migration properties |

## Trade-offs & Decisions (T2.1)

- **Shape B: the store is its own table, not rows of the wiki's `chunks`.** Recorded in the design
  entry above. The deciding fact is that `wiki.RebuildDB` drops and recreates its tables on every
  compile — correct for a derived artifact, fatal for the only copy of something — so Shape A needs
  T4 first, and T2 comes before T4.
- **`Previous` and `Next` keep holding PATHS**, not row keys. Every archive already on disk points at
  `history/<id>/<rev>.md`, and rewriting those on migration would be a second transformation to get
  wrong. The values cross verbatim and whoever walks them interprets them; T2.2 decides whether new
  writes switch to keys.
- **`tags` is JSON in one column.** A column store has no natural place for a variable-length list of
  strings that is only ever read whole, and modelling them as rows would make a memory a join — the
  same reasoning the wiki's `sync_log` uses for its three slug lists.
- **The `embedding` column exists from the start, unused.** It is what lets `shardsync.go` retire: that
  file mirrors the wiki's shard cache into the raw prefix so a colleague's memory arrives with its
  vector already computed. Without the column, retiring it would silently make every unit recompute
  every vector. Adding it later would be a second schema change.
- **The temporary proof program was deleted.** It lived in `tmp-migrate-proof/` for one run against
  the real store and is gone; the migration has no CLI surface yet, which T2.4 needs and will add.

## Technical Debt
- [ ] **The migration has no CLI or MCP surface.** It was run for T2.1 through a throwaway
  `go run` program, which is fine for a proof and not fine for an operator. T2.4 needs
  `graphit memory migrate` with a dry run, because it is the command that gates deleting data.
- [ ] **`project_id` may be dead, and this store cannot tell.** It is written only for USER-scoped
  memories and read by nothing, so the project scope's 0/409 is expected rather than evidence. Check
  the user scope before concluding; if it is genuinely unread, it should go rather than be carried
  into a new schema.
- [ ] **21 memories carry no `type` and 1 carries no `tags`** in this store. The record holds that
  faithfully, which is right for a migration, but it means the type-derived surfaces have a real
  untyped population to handle.
- [ ] **`MemoryTable` has no scope-URI resolver yet.** `OpenMemoryTable` takes a URI, and nothing
  decides what a scope's URI IS — local directory versus `s3://…/memory/<scope>/<id>`. That is T2.2's
  first job, and it is where the remote-writable intent proved in T0 finally gets used.
- [ ] `Live()` and `List()` cap at 100000 rows, like every other read in this project. Fine now;
  it is a limit rather than a paging strategy.

## System Knowledge

- **The raw store was a TRANSPORT as much as a store.** `Publish` uploads every file in the scope
  directory to `s3://…/memory/<scope>/<id>/` in the background; `Pull` merges and never deletes
  locally. It is safe today only because each memory is its own object named by a ULID, so two units
  ADDING memories cannot collide — which is precisely the property T0 re-established for a table.
- **`repair.go` becomes unnecessary rather than ported.** All four of its passes heal one corruption:
  a write path that recovered a memory id from the FILE NAME, forking 184 memories into twins under
  ids like `<ulid>_important_`. A table keyed by the declared id cannot express that defect.
- **Three structural facts the filesystem encoded** and a table has to reproduce deliberately: the
  `# <Title>` H1 that `renderMemoryFile` emits and `extractBodyAfterFrontmatter` strips back off; the
  LOCATION under `history/`, which `buildMemDoc` trusts over the frontmatter when deciding
  `superseded` (a location cannot be wrong, which is what let legacy archives compile correctly); and
  the lexicographic order of names within `history/<id>/`, which is how successors were derived — the
  zero-padded counter and the ULID are each ordered by age, and `"0001"` precedes every ULID.
  `recordFromMarkdown` is the LAST place the location rule applies, because a row has no location:
  the migration resolves it once and `superseded` is explicit from then on.
- **A canonical-form hash is the right tool for "did anything get lost", and a source-byte hash is
  the wrong one.** Whenever two representations of the same data differ by normalisation, compare
  them through the normaliser.

## Progress Log

### 2026-09-01 (T2.1)

- Mapped `internal/memory` in full with a sub-agent before specifying, because three of T2's
  constraints — the transport role, `repair.go`'s reason to exist, and the six file-only fields —
  are only visible in the details.
- Chose Shape B over Shape A on a sequencing argument, and recorded it as a deliberate deviation
  from reading the objective as "one table, full stop" rather than burying it.
- Built the table and the migration, then found my own verification was wrong before trusting it:
  hashing against the source file's bytes would have failed all 409 records for normalisation and
  proved nothing.
- Proved it on the real store: 409/409 verified, and the field census above is what makes that
  meaningful rather than merely green.

**Next: T2.2** — the five write paths write the table. Its first job is the one recorded as debt
above: decide what a scope's URI is, which is where T0's remote-writable intent gets used for the
first time. T2.3 moves the readers, and T2.4 retires the raw store, `repair.go` and `shardsync.go` —
gated on `MigrationReport.Verified()`, which is why that method exists.

### 2026-09-02 (legacy removal — everything identified as residue is gone)

Asked to remove all the residue identified while answering "is everything working, with no
backwards compatibility, since we are in dev". Green after: `go build ./...`,
`go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`, `make lint` (0 issues).

**Deleted, with the reason each one was residue rather than design:**

| What | Where | Why it was residue |
|---|---|---|
| `legacySQLiteIndexName = "wiki.db"` and its branch in `IsDerivedFile` | `internal/wiki/pipeline.go` | Guarded against publishing a pre-Lance SQLite index. `find ~/.graphit -name 'wiki.db*'` returns nothing, and nothing creates one. |
| `WikiDB.Checkpoint()` + its only call site | `internal/wiki/store.go`, `internal/wiki/embedder.go` | A documented no-op kept "to show the concern was considered". Lance has no log to fold: a write is already a complete immutable version. The reasoning moved into `Compact`'s comment, which is where it is still true. |
| `ladybugstore.Store.Checkpoint()` | `internal/ladybugstore/store.go` | **Not previously identified.** Issues a real `CHECKPOINT`, and has ZERO callers — confirmed by graph query and grep. Dead since the wiki left this engine. |
| `LintConfig.Deep`, `LintConfig.Fix`, `LintReport.FixesApplied` | `internal/wiki/lint.go` | `Deep` was never wired to anything. `Fix` became inert when pages stopped existing — there is no page to inject backlinks into, and `xrefs` cannot be missing an edge the graph is built FROM. An inert flag that promises repair is worse than no flag. |
| `--deep` / `--fix` CLI flags, and the `deep` / `fix` MCP parameters | `cmd/graphit/commands/knowledge.go`, `internal/mcpstdio/tools_knowledge.go` | The surfaces that advertised the two inert fields. This is a deliberate, user-visible schema change, taken because "documented" was the only argument for keeping them. |
| `gitDirName = ".git"` and both skip blocks | `internal/memory/memory_s3_store.go` | Tolerated a `.git` left by the worktree era in `uploadDir` and `copyDirRecursive`. |

**The `.git` removal was gated on checking the data first, and the check changed the risk.** The
one stray was `~/.graphit/memory-raw/memory-project-01KTJFYWJDD3PXYHV1CE10QC8D/.git` — a **file**,
not a directory: a one-line gitlink pointing at `~/.graphit/memory/.git/worktrees/…`, which does
not exist. So it held no recoverable data at all, and `git log` in that directory fails with *not
a git repository*. Backed up to `/tmp/graphit-legacy-gitlink-backup/` anyway, removed, then
verified `find ~/.graphit -name '.git'` is empty and the scope's 14 memories are untouched. Only
then was the skip code deleted — **in that order, because removing the skip while a `.git` existed
would have published git internals as memories.**

Also confirmed the skip could not become necessary again: `copyDirRecursive` here is reached only
from `ExtractScopeDir`, whose `src` is always `<scope dir>/<relDir>` — the store's own tree, which
nothing puts a `.git` into.

**Stale claims corrected — comments that described a file or a behaviour that no longer exists.**
These are not cosmetic: each one would send the next reader looking for something absent.

- `internal/wiki/search.go` — `searchCompiledWiki`'s doc justified its second return value by "only
  the markdown can answer, and scanning it is why a memory is findable the moment it is written".
  That fallback was deleted in T1 slice 1. The return value is still live and still meaningful
  (`bm25PreFilter` uses it to emit no block rather than an empty one), so the value stayed and the
  justification was rewritten.
- `internal/wiki/statcheck.go` — "wiki.db exists" → the index holds content; "directory containing
  wiki.db and the generated .md pages" → **doubly wrong**, there is no `wiki.db` and no generated
  pages.
- `internal/wiki/fastpath.go` — the `IndexHasContent` rationale said generation was skipped "because
  the pages were generated", and that search "silently fell back to scanning the markdown".
- `internal/wiki/process_cache.go` (3), `internal/knowledge/wiki.go`,
  `internal/mcpstdio/tools_knowledge.go`, `cmd/graphit/commands/runners.go` — `wiki.db` by name.
- `internal/memory/rule.go` — **agent-facing text in the mandate**, telling every agent that
  `memory_search` runs "through SQLite FTS5, falling back to an in-memory BM25 index over the wiki".
  Both halves false. This one mattered most: it is instruction, not a comment.
- `internal/ast/resources.go` (2) — "the full-text index moved to the SQLite sidecar"; it moved to
  the Lance search index, which `internal/ast/search_lance.go` says replaced that sidecar.

**Kept deliberately, so the next pass does not delete them thinking they were missed:** the
recovering frontmatter parse (`quoteUnquotedScalars` — it is what makes 172 real memories readable);
the one-shot migration (the declared exception); `repair.go` (retires in T2.4, and until the raw
store is gone it is still what heals the 184 forked twins); `RevisionIDFromHistoryPath`'s handling
of `0001.md` names (real data on disk uses them). Historical rationale that says "this replaced
SQLite, which is why the shape is what it is" also stayed — that is explanation, not residue.

**Three tests broke, which is the signal working.** Each was read before being touched:

- `TestIsDerivedFileNamesOnlyTheDatabase` asserted `wiki.db`/`-wal`/`-shm`/`WIKI.DB` are derived.
  Rewritten as `TestIsDerivedFileNamesOnlyTheIndex` over `WikiIndexDirName`, keeping the
  case-insensitive and nested-path cases and the "shards must survive the filter" half, which is
  the assertion that actually protects a publish.
- `TestMemoryStoreSkipsLeftoverGitMetadata` and `TestMemoryPublishSkipsLeftoverGitMetadata` —
  see the correction below.

#### Correction from the user, mid-pass: the raw store is itself the legacy

I first **converted** the two `.git` tests into honest tests of raw-store extract/publish, to avoid
losing coverage. The user corrected the premise: *"memoria não tem mais raw dir, porque as memorias
já vão direto para o lancedb em s3"*. Salvaging coverage for a mechanism that T2.2–T2.4 delete is
wasted work, and it signals that the path is supported. **Both tests were deleted outright.**

The general rule, recorded because it will recur in T3 and T4: **when removing residue from a
subsystem that is itself being retired, check whether the SUBJECT of a broken test survives the
plan before rescuing the test.** The signal was already in those tests' own comments — "the worktree
this replaces", "anyone upgrading in place". A test that justifies itself by retrocompat dies with
the retrocompat.

T2.2's spec above was updated with what the correction settles: the scope URI is
`s3://…/memory/<scope>/<id>`, and the markdown surface is deleted rather than adapted.

**State check, so this is not misread later.** The directive describes the agreed target, not the
current code. As of this pass the raw store is STILL the live path: `MemoryTable`/`MemoryRecord`
have no caller outside `table.go`, `migrate.go` and tests, every write still goes through
`RawDirFor`, and there are 425 `.md` files across 5 scopes in `~/.graphit/memory-raw/`. Confirm with
a grep for `RawDirFor` before assuming the switch happened.

#### Not a deletion — carried into T3 as an explicit item

Knowledge has **two live publish mechanisms**, and collapsing them is a mechanism swap rather than
residue, so it is not part of this pass:

- **Hub**: the index travels. `internal/hub/registry.go` `prepareKnowledgePublish` → `ExportToParquet`;
  the consumer side is `internal/hub/service.go` `HasBundle` → `ImportFromParquet`.
- **MCP context**: the shards travel. `installKnowledgeContext` → `knowledge.IndexContextWiki` →
  `wiki.BuildDBFromCache`, with export via `exportWikiToWorktree` using
  `paths.SyncCopyDirExcept(wikiDir, dest, wiki.IsDerivedFile)`.

Both work. D4 already says the Parquet bundle dies and publishing becomes "write the table to the
published prefix", which resolves the pair — that belongs to T3.

**Still uncommitted.** The working tree now holds this pass on top of T1 (4 slices), T0, the two
incremental gate fixes, T1.4 and T2.1. The natural commit boundaries are those six.

### 2026-09-02 (asked "falta algo?" — a second sweep, and it found four more, two of them mine)

Verified instead of answering. Green after: `go build ./...`, `go build -tags lancedb ./...`,
`go test -tags lancedb -count=1 ./...` (46 packages), `make lint` (0 issues), `make install`.

**1. I broke the knowledge rule text by removing the MCP parameters** —
`internal/knowledge/rule.go:468-476` still instructed agents to call
`graphit_knowledge_lint(project_dir: …, fix: true)` and documented `deep: true`. Both parameters
had just been deleted, so an agent following its own mandate would pass unknown fields. Rewritten
to say `stale_days` is the only knob, and to state plainly that the audit reports and never
repairs — with the reason, so nobody re-adds it: there is no page to repair, and `xrefs` cannot be
missing an edge the graph is built FROM.

**2. A test was PINNING a false claim, which is why my earlier fix looked complete and was not.**
`internal/memory/rule_gc_test.go` asserted the memory skill text *contains* `"FTS5"`. I had removed
one FTS5 mention from `internal/memory/rule.go` and the suite still passed — because a second
mention survived at line 144 ("FTS5-ranked over the compiled wiki") and the test was satisfied by
it. The index is LanceDB BM25; SQLite FTS5 has not been the engine for some time.

Fixed both ends: the text now says BM25, and the test asserts the *current* truth — `FTS5` and
`SQLite` must be ABSENT and `BM25` present. Its comment records that this text has now been wrong
twice, each time telling the agent to expect behaviour the code does not have. **A green suite is
only evidence when the assertion is about what is true now; an assertion that pins the old truth
converts a regression into a passing test.**

**3. Four more agent-facing rows named artifacts T1 deleted**, and they contradicted the same
documents' own "there is no wiki inside the project" section:
`internal/memory/rule.go` — "Reading a memory `index.md` directly" and "NEVER read index.md files
directly / Reading raw .md files is slower"; `internal/knowledge/rule.go` ×2 — "Reading all .md
files in wiki/ sequentially". All reframed to the mistake that is still possible (opening page
after page instead of searching) rather than one that no longer is.

**4. Dead exported code, found by doing on purpose what `ladybugstore.Checkpoint` was found by
accident.** `.golangci.yml` documents that `unused` reports UNEXPORTED identifiers only, and that
for exported ones "there is no linter at all, only a query against the AST graph, run by hand" —
so the manual sweep is the documented procedure, not improvisation. Scanned all 365 exported
funcs/methods in `internal/wiki`, `internal/knowledge`, `internal/memory`, `internal/ladybugstore`
and `internal/lancestore` for references outside their declaring file. Zero were unreferenced
everywhere; 12 were referenced only within their own file, and of those, two were genuinely dead
and both are T1 residue:

- **`wiki.ReadFrontmatterField(path, field)`** — read frontmatter *from a `.md` file on disk*,
  which is precisely what T1 removed. Deleted. `FrontmatterField`/`FrontmatterBlock` stay: they
  take content, not a path, and `docutil.go` and `internal/memory/memory.go` use them on source
  documents.
- **`wiki.YAMLScalar` + `needsYAMLQuoting`** — hand-rolled YAML scalar quoting, written because
  frontmatter blocks "are assembled by string concatenation rather than by a YAML encoder". They
  are now assembled by `yaml.Marshal` (`internal/wiki/export.go:240,249`), which cannot emit an
  unparseable block by construction. Deleted **because** the replacement is strictly safer, not
  merely because it was unreferenced — this is the exact bug class that once destroyed 20 memory
  files, so the ordering of that argument matters.

Each deletion orphaned an import (`os` in `helpers.go`, `strconv` in `okf.go`), which is
independent confirmation that nothing else used them.

**One false positive worth recording, because the heuristic will be reused:** `wiki.WikiQueryText`
looked dead by the same measure and is not — its two callers are in the same file
(`store.go:664,686`). Had I trusted the scan, I would have deleted the function that keeps query
expansion in step with document expansion, which would have degraded search silently rather than
failing a build. **"Referenced only in its own file" is a candidate list, never a verdict.**

Also caught: deleting `ladybugstore.Store.Checkpoint()` left `internal/ladybugstore/store.go`
unformatted (a lost blank line). `make lint` does NOT enforce gofmt here, so it reported 0 issues
either way — checked every file I touched with `gofmt -l` against its HEAD version to tell my
damage from the four files that were already unformatted before this work.

**Not changed, and deliberately: `internal/wiki/transfer.go` (`ExportToParquet`/`ImportFromParquet`)
is still LIVE** — `internal/hub/registry.go:981` and `internal/hub/service.go:262` call it. D4 says
the Parquet bundle dies, so removing it now would break Hub publish with nothing in its place.
It goes with the publish-mechanism swap in T3.

### 2026-09-02 (the vacuous assertion was hiding a SHIPPING bug: the whole index was being published)

Fixing the `wiki.db` assertion in `internal/knowledge/publish_test.go` to name
`wiki.WikiIndexDirName` instead made it **fail immediately**. It was not a test problem.

**The bug.** `wiki.IsDerivedFile(rel)` tested `filepath.Base(rel)` only. That was sufficient when
the index was `wiki.db`, a single FILE. The index is a DIRECTORY now, and the callers walk
recursively and ask per entry — so `index.lance/chunks.lance/_indices/<uuid>/part_0_invert.lance`
has base `part_0_invert.lance`, is not derived, and gets copied. `paths.SyncCopyDirExcept` compounds
it: on a skip it `return nil` rather than `filepath.SkipDir`, so the directory entry is skipped and
the walk descends into it anyway.

Net effect, measured by walking a real published directory: **the entire Lance index travelled** —
every FTS fragment under `_indices/`, every `_transactions/*.txn`, every `_versions/*.manifest` —
in the exact artifact whose stated purpose is to leave it behind. Both
`internal/mcpstdio/tools_knowledge.go` `exportWikiToWorktree` and `cmd/graphit/commands/runners.go`
carry comments claiming the index is excluded because "it is the largest thing in the directory".
They were describing intent, not behaviour.

**Why it was invisible, and it is the same lesson as the FTS5 test above.** The guard was
`os.Stat(filepath.Join(published, "wiki.db"))` asserting `IsNotExist`. Once the index was renamed
and reshaped, that assertion became true for the wrong reason and stayed green through the entire
regression. **An assertion pinned to a literal artifact name does not fail when the artifact
changes — it stops testing.**

**The fix** is in `IsDerivedFile`, not in the walkers: it now matches ANY component of the relative
path, so every caller is fixed at once regardless of whether it honours `SkipDir`. Splitting on both
`/` and `filepath.Separator`, since `SyncCopyDirExcept` passes `filepath.ToSlash(rel)` and other
callers do not.

`TestIsDerivedFileNamesOnlyTheIndex` gained the cases that actually reach it — four paths INSIDE the
index directory, including a `_transactions` entry and a `_versions` manifest — because those are
what a recursive walk asks about, and none of them was covered before.

**Checked that excluding it is safe, rather than assuming.** `TestAPublishedWikiInstallsWithoutItsSources`
installs the published artifact with no source tree anywhere and indexes it from the shards alone;
it passes. So the consumer was never depending on the index arriving.

**A near-miss worth recording, because it would have broken Hub publish.**
`internal/wiki/transfer.go:30` declares `BundleDir = WikiIndexDirName` — the Hub bundle is written
to a directory with **the same name** as the wiki index. A probe test confirmed
`IsDerivedFile(BundleDir + "/…")` is now true, so if the Hub staging directory were filtered, the
fix would have deleted the artifact's payload. It is not: `prepareKnowledgePublish`
(`internal/hub/registry.go:976`) creates a fresh temp dir containing only the bundle and hands it
straight to `PublishArtifact`, with no `IsDerivedFile` anywhere on that path. Verified end to end
against **real MinIO** — `TestPublishedWikiIsReadDirectlyFromObjectStorage` green, reading
`artifacts/knowledge/acme/1.0.0/index.lance` on the fly.

That coupling is a trap for T3, which is going to change both mechanisms: **the exclusion predicate
and the bundle directory currently share a name, and they mean opposite things.** One says "never
carry this", the other says "this IS what we carry". Recorded as debt below.

Also fixed here: `internal/daemon/memorysyncmodule.go:107` referred to "the note on gitDirName in
internal/memory/memory_s3_store.go", a note deleted earlier in this pass; and
`internal/hub/registry.go:481` still described the publish fallback as carrying "the pages and the
shards".

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues), plus the MinIO E2E above.

## Technical Debt (added 2026-09-02)

- [ ] **`BundleDir == WikiIndexDirName`, and `IsDerivedFile` now excludes that name at any depth.**
  `internal/wiki/transfer.go:30`. The two are only compatible because the Hub staging path never
  applies the predicate. Anything in T3 that unifies the publish mechanisms must not put a bundle
  inside a directory the copy filter rejects — give the bundle its own name, or drop the predicate
  when the table becomes the artifact.
- [ ] **`paths.SyncCopyDirExcept` returns `nil` instead of `filepath.SkipDir` for a skipped
  directory.** `internal/paths/copy.go:69`. Harmless now that `IsDerivedFile` matches every
  component, but it means the walk still descends into a tree it has decided to exclude, and the
  next predicate written for it will hit the same trap. Fix when that file is next touched.
- [ ] **Four files were already unformatted before this work** — `internal/ignorer/ignorer.go`,
  `internal/ladybugstore/icebug_canonical.go`, `internal/memory/rule.go`, `internal/store/store.go`.
  `make lint` does not enforce gofmt, which is why they drifted. Left alone to keep this diff
  honest; worth a separate formatting-only commit plus a gofmt check in the lint target.

### 2026-09-02 (asked "está completo?" — verified the blast radius of my own fix, and it holds)

The `IsDerivedFile` fix BROADENED a predicate, and I had only validated the two publish paths I
already knew about. That is the wrong order, so this pass checked the fix instead of the residue.

**Every caller enumerated.** Two in production, both `paths.SyncCopyDirExcept(wikiDir, <dest>,
wiki.IsDerivedFile)`: `internal/mcpstdio/tools_knowledge.go:466` and
`cmd/graphit/commands/runners.go:2183`. Both destinations are `os.MkdirTemp` with
`defer os.RemoveAll` — never a live wiki. So the broadened predicate cannot reach a wiki that is
being read.

**A real hazard in the delete half, which turned out to be inert here.**
`internal/paths/copy.go:78-96` applies the skip to the DESTINATION walk as well and `return nil`s
on a match, so a skipped entry already present in the destination is never deleted. If the
destination were persistent, my fix would have frozen a previously-published index in place: no
longer updated, never removed, diverging from the shards beside it. It is inert only because both
destinations are fresh temp dirs — recorded as debt above, since the next predicate written for
this function meets the same trap.

**The remote is mirrored, so the already-published index does get cleaned up.**
`S3Store.PublishContextDir` (`internal/hub/s3_store.go:425`) calls `DeletePrefix` on the target
prefix BEFORE `UploadDir`. So the `index.lance/**` objects that the bug pushed to existing branches
are removed by the next export rather than lingering.

**And even if a stale index did arrive, the consumer overrides it — now pinned by a test.**
`installKnowledgeContext` → `knowledge.IndexContextWiki` → `wiki.BuildDBFromCache`, which passes a
`logEntry` specifically to disable `RebuildDB`'s hash fast path. That is a comment, so I tested the
claim rather than trusting it: new `TestStaleIndexIsOverriddenByTheShards`
(`internal/wiki/stale_index_test.go`) builds an index from one set of shards, replaces the shards
with different content while leaving the index in place, rebuilds, and asserts the page reads the
NEW title. It passes. **That test is what makes the fix safe to reason about — without it, "the
rebuild always wins" was an assumption in a comment.**

**A near-miss re-verified end to end.** `BundleDir == WikiIndexDirName` means the Hub bundle sits in
a directory the predicate now rejects at any depth. Confirmed again that `prepareKnowledgePublish`
never applies the predicate, and re-ran the MinIO E2E after all of today's changes:
`TestPublishedWikiIsReadDirectlyFromObjectStorage` green against a real object store.

**Stale user-facing and comment text found in the same sweep** — all of it describing artifacts or
storage that no longer exist:

| Where | Was | Why wrong |
|---|---|---|
| `cmd/graphit/commands/runners.go:2189` | `"wiki/ → …/wiki (pages + shards, without the rebuildable database)"` | **Printed to the user.** No pages travel, and it is an index, not a database |
| `cmd/graphit/commands/memory.go:83` | "The memories live in their own git worktree" | **CLI help text.** Memory left git |
| `cmd/graphit/commands/runners.go:1271` | "the pages and the embedding vectors arrive together" | shards arrive, not pages |
| `cmd/graphit/commands/runners.go:1671` | "a wiki that is empty while its pages and shards are present" | pages again |
| `internal/mcpstdio/tools_memory.go:433`, `runners.go:1865` | "a branch of the shared memory repository … its worktree" | a prefix of the store and a raw directory |
| `internal/uiserver/wiki_handler.go:210` | "the worktree set is the record" | the set of raw directories is |
| `internal/memory/wiki.go:274` | "Search still answered — it falls back to a BM25 scan over the .md files" | present tense for a fallback T1 deleted; now says the fallback is gone, which is what makes reporting the error the only signal |

**Renamed `exportWikiToWorktree` → `stageWikiForPublish`** (`internal/mcpstdio/tools_knowledge.go`).
Its destination is a temp staging directory uploaded to S3; there has been no worktree in this path
since the Hub left git, and the name was the last thing still asserting otherwise.

Left alone on purpose: the `"wiki.db"` decoy fixture in `internal/memory/shardsync_test.go:121` —
`ExportShards` does not use `IsDerivedFile`, it mirrors only the `shards` subtree, so the decoy's
name is irrelevant to what the test proves, and `shardsync` retires in T2.4. Every other `worktree`
mention in the tree is past-tense history of how something used to work, which is the kind of
comment worth keeping.

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues), MinIO E2E, and a CLI smoke of index / lint / export / memory.

### 2026-09-02 (T2.2 — the five write paths write the table: PLAN, written before the first edit)

Investigated first, because all five writes share one shape and the shape is what gets replaced:

```
scope, _ := m.store.OpenScopeLocal(m.ScopePrefix())   // a raw markdown directory
data := scope.ReadFile(MemoryFileName(id))            // read the file
scope.WriteFile(rel, []byte(renderedMarkdown))        // rewrite the file
scope.Publish(msg)                                    // upload the WHOLE directory, async
m.syncToLocalFast()                                   // recompile the wiki
```

becomes

```
tbl, _ := OpenMemoryTable(ctx, MemoryTableURI(scope, scopeID))  // the store itself
rec, ok, _ := tbl.Get(ctx, id)                                  // read the row
tbl.Put(ctx, rec)                                               // write the row — this IS the remote write
m.syncToLocalFast()                                             // recompile the wiki
```

**`Publish` disappears rather than being ported.** It existed because the truth was local and had to
be pushed; a table opened at an `s3://` URI is written where it lives. That also removes the async
goroutine and `WaitForPendingPushes`, which is what made a write's durability something a test had
to wait for.

#### D5 — what a scope's URI IS. DECIDED, and it is adjacent to the no-fallback constraint, so it is stated rather than assumed

```
MemoryTableURI(scope, scopeID):
  bucket configured  →  s3://<bucket>/<cfg.Prefix>/memory/<scope>/<scopeID>
  no bucket          →  <global>/memory-table/memory-<scope>-<scopeID>
```

The remote form is the user's directive, verbatim: the memories go straight to LanceDB in S3, and
`cfg.Prefix` must be spelled out because `s3store.Store.Key` applies it internally while LanceDB
talks to S3 directly — a URI built without it would silently address a different prefix than every
other object this project writes.

**The local form is NOT a fallback, and the distinction is the reason this is written down.** The
constraint forbids a second READ PATH, tolerance for an old schema, and a flag that restores old
behaviour. This is none of those: there is one store, one writer, one reader, one schema, and one
code path. What varies is the URI string, which is configuration. `NewMemoryStore` has always
treated a missing bucket as local-only mode rather than an error, and a table that can only exist in
S3 would take memory away from every user without a hub bucket — a regression, not a simplification.
`memory-table/` is also NOT the raw store returning: it holds the Lance table, never markdown, and
nothing reads a file out of it.

#### Sub-slices, in dependency order

- [ ] **T2.2a — `MemoryTableURI` and the scope handle.** Spec: new `MemoryTableURI(scope, scopeID)`
  in `internal/memory/paths.go` using `s3store.URI` + `s3store.JoinKey`, and
  `store.MemoryTableRoot()`/`MemoryTableDir()` beside the existing `MemoryRawRoot`/`MemoryRawDir`
  in `internal/store/store.go`. Done when a unit test pins both forms, including that the
  configured prefix appears in the remote one. Constraint: the scope path normalisation must match
  `remotePrefix` exactly — `memory/project/x` and `project/x` are the same scope, and a second
  normalisation rule would put two tables where there is one scope.
- [ ] **T2.2b — `AddMemory` writes a row.** Spec: build a `MemoryRecord` directly instead of
  `buildMemoryFile` + `WriteFile`; `Revision: 1`, `Superseded: false`, `RevisionID: ""`. Done when
  a memory inserted through the service is readable with `tbl.Get`. Constraint: `CreatedAt` and
  `UpdatedAt` are both set here — they are the same on a first write and they diverge later, and
  the wiki cannot reconstruct either.
- [ ] **T2.2c — `updateMemory` and the revision chain as rows.** Spec: `archiveRevision` becomes a
  `Put` of the OLD record with `Superseded: true` and a fresh `RevisionID`; `repointArchiveNext`
  becomes a `Get`+`Put` on the previous archive. Done when an update leaves exactly two rows —
  one live, one superseded — with `previous`/`next` linked. Constraint: **`Previous`/`Next` keep
  holding `history/<id>/<rev>.md` PATHS.** They are what 84/86 migrated records already carry, and
  rewriting them to row keys here would split the chain across two conventions. The path is an
  identifier now, not a location.
- [ ] **T2.2d — `RemoveMemory`.** Spec: archive the live record as superseded, then
  `tbl.Delete(ctx, id)`. Done when the live row is gone and the chain under that id is still
  readable by `tbl.Revisions`. Constraint: the archive's `Next` stays EMPTY, which is what
  distinguishes the last state of a deleted memory from a superseded revision.
- [ ] **T2.2e — `changeRelevance`.** Spec: `Get`, flip `Important`, `Put`. Done when promote and
  demote survive a round trip. Constraint: it must NOT archive a revision — importance is not a
  content change, and the current code deliberately returns early when the flag already matches.
- [ ] **T2.2f — the write path no longer needs `ScopeStore`.** Spec: `MemoryService` holds a
  `MemoryTable` instead of reaching for `OpenScopeLocal`. Done when no write path calls
  `WriteFile`, `ReadFile`, `RemoveFile` or `Publish`. Constraint: the READERS still read the raw
  store until T2.3, so `localDir` stays for now — this slice is writes only, and the two must not
  be entangled or a half-finished session leaves the store unreadable.

#### The risk this slice carries, and how it is contained

Between T2.2 and T2.3 the writes go to the table and the readers still read markdown, so **a memory
written in that window is invisible to search until T2.3 lands.** That is a real intermediate state,
not a hypothetical. Two things make it acceptable: the raw store is not deleted (T2.4 does that,
separately and last), and the migration is re-runnable — so a memory written to the table in the gap
is not lost, it is merely not yet compiled. Anyone stopping mid-slice should record where they
stopped here, because the symptom is "my new memory does not come back from search" and the cause
is not a bug.

### 2026-09-02 (T2.2 — T2.2a LANDED; T2.2b–f written, PROVED, and DELIBERATELY REVERTED)

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues).

#### What landed

- [x] **T2.2a — the scope URI and the path helpers.** `MemoryTableURI(scopePath, localDir)` and
  `TableDirFor(scope, scopeID)` in `internal/memory/paths.go`;
  `MemoryTableRoot`/`MemoryTableDir`/`memoryScopeSegment` in `internal/store/store.go`; five tests in
  `internal/memory/table_uri_test.go`. D5 above is now encoded in code, and the tests pin the two
  traps a future implementer would otherwise hit:
  - **`cfg.Prefix` must be in the URI.** `s3store.Store.Key` prepends it internally; LanceDB is
    handed a URI and talks to S3 directly. Omitting it does not fail — it addresses a different
    prefix and answers as an empty store.
  - **The scope→remote mapping is NOT the identity.** An imported context lives under
    `memory/project/<name>`, so the URI is built from `ScopePrefix()` through `remotePrefix`, which
    is the one existing normaliser. A second rule would put two tables where there is one scope.

  **These two helpers currently have no production consumer**, which is the residue smell this
  session spent three passes removing. It is stated rather than hidden: they are the foundation of
  T2.2b, they are tested, and they carry the two findings above, which is the part worth keeping
  across a session boundary. If T2.2b is abandoned rather than resumed, delete them.

#### What was written, verified to work, and then reverted — and why

T2.2b–f was implemented in full and **the mechanism was proved**: writes went to the table,
`Publish` and the async push goroutine left the write path, and the chain tests passed once their
assertions read rows instead of files. It was reverted because **it cannot ship without T2.3**, and
T2.3 did not fit in the session with the care the user's real store deserves — 324 live memories.

The evidence that it worked, and the evidence that it cannot stand alone, are the same test run:

| Test group | Result after T2.2b–f | Meaning |
|---|---|---|
| chain semantics — revision 1, update archives, chain walks back and forward, remove archives | **PASS**, reading rows | the write path and the chain are correct on the table |
| 5 of 7 history tests stopped using `*ScopeStore` entirely | compiler flagged `w` unused | the raw store genuinely left the write path |
| `ListMemories`, wiki compile, search, `repair` | **FAIL, 10 tests** | the readers still read markdown, and the markdown is no longer written |

That last row is the intermediate state this slice's plan predicted in writing. Predicting it did not
make it shippable: a tree where a memory is written and then cannot be found is worse than one where
the move has not started.

#### The design, recorded so resuming is re-typing rather than re-deciding

The shape that worked, and the one judgement call in it:

- `MemoryService` gains a `tableURI` field, set in `newMemorySvcInternal` (which takes a
  `tableDir` parameter alongside `localDir`) via `MemoryTableURI(svc.ScopePrefix(), tableDir)`.
- `openTable(ctx)` opens **per operation**, not held on the service: a long-lived handle pins a
  dataset version for the life of the process, which is how a reader ends up answering from a
  snapshot taken before the writes it is being asked about.
- **The four content transforms stay on text, and the markdown becomes a TRANSIT FORMAT.**
  `buildMemoryFile`, `updatedMemoryContent`, `archivedRevisionContent` and `withImportantFlag` keep
  operating on strings; two new helpers bracket them — `putMarkdown(ctx, tbl, rel, content)` which
  goes through `recordFromMarkdown` and `Put`, and `readMarkdown(ctx, tbl, key)` which is `Get` plus
  `MemoryRecord.Markdown()`. Nothing is written to disk.

  **This is the judgement call, and it was deliberate.** Rewriting all four to operate on
  `MemoryRecord` is the cleaner end state, but they carry the revision / previous / next /
  classification bookkeeping that a data-loss bug was found in, and `recordFromMarkdown` enforces
  the guard that came out of it — a frontmatter that does not parse is REFUSED, never re-rendered
  from an empty struct. Moving storage and rewriting that bookkeeping in one slice puts both at risk
  at once. The rewrite belongs in its own slice, with tests.
- `archiveRevision(ctx, tbl, id, content, nextPath)` passes `HistoryPath(id, revisionID)` as the
  record's `rel`, which is what makes `recordFromMarkdown` mark the row superseded and recover its
  revision id — **the same derivation the migration used, so a native archive and a migrated one are
  indistinguishable rows.**
- `repointArchiveNext` needs `archiveKeyFromPath(rel)` — `chainIDFromHistoryPath(rel) + "/" +
  RevisionIDFromHistoryPath(rel)` — because `previous` holds a PATH, in the 84 migrated records that
  already carry one and in every archive written since. The path is an identifier now.
- `changeRelevance` must NOT archive a revision, and its early return when the flag already matches
  matters more than before: a no-op promote that wrote would add a dataset version to a store whose
  version history IS the recovery path D2 accepted.
- `repair.go` needs its own `archiveRevisionInRawStore(scope, …)`, because it is the only thing that
  still writes markdown and its corruption is not expressible in a table keyed by the declared id.
  Both retire together in T2.4.
- Tests: `newLocalService` sets `tableURI` to a temp dir; `readStored(t, svc, rel)` /
  `mustReadStored` replace `w.ReadFile(rel)`, still taking a PATH; `archivePaths(t, svc, id)`
  replaces `w.ListDir(HistoryDirFor(id))` and preserves the ORDER, because `Revisions` sorts by
  `revision_id` which is what the lexicographic file order gave.

#### 🔒 THE ACTUAL BLOCKER FOR T2.3, measured rather than assumed

`GenerateMemoryWiki(ctx, rawDir, wikiDir, …)` in `internal/memory/wiki.go:106` is coupled to the
filesystem in **four** places, not one, so there is no seam to swap:

1. `ImportShards(rawDir, wikiDir)` — pulls a colleague's precomputed vectors out of the raw prefix.
   The `embedding` column on `memories` is what replaces it (that is why the column exists).
2. `wiki.StatPreCheck(ctx, rawDir, wikiDir, processCache, …)` with
   `CurrentSourceFiles: memorySourceFileNames(rawDir)` — the incremental skip gate, keyed on file
   stat and file names.
3. `os.ReadDir(rawDir)` + `isMemorySourceFile(e.Name())` — the document enumeration.
4. `buildMemDoc(rawDir, e.Name(), processCache, validPaths)` — per-document load, keyed by relPath.

**So T2.3 is not "point the readers at the table": it is replacing the memory wiki's incremental
machinery with one keyed on rows and content hashes, which is T4's subject.** Whoever resumes should
decide between two orders, and the second is probably right:

- **T2.3 then T4** — compile the wiki from the table with a full rebuild every time, accept the cost,
  then make it incremental. Simple, and slow on a large scope.
- **T4 first, for memory only** — make the incremental gate hash-based over rows, then move the
  readers onto it. Reverses the plan's numbering, and is the order the coupling actually implies.

`ListMemories`, `important.go:33`, `consolidate.go:161` and `dream.go:320` are all straightforward by
comparison — `Live(ctx)` answers each of them directly. The wiki generator is the whole of the risk.

#### Also fixed in this session, unrelated to T2

`internal/daemon/memorysyncmodule.go:107` referenced "the note on gitDirName in
internal/memory/memory_s3_store.go", deleted earlier in the legacy pass; and
`internal/hub/registry.go:481` described the publish fallback as carrying "the pages and the shards".

## Technical Debt (added 2026-09-02, T2.2)

- [ ] **`MemoryTableURI` and `TableDirFor` have no production caller.** `internal/memory/paths.go`.
  They are T2.2b's foundation and are covered by `table_uri_test.go`. Resume T2.2b or delete them —
  do not leave them indefinitely, since an unused export is exactly what this session removed
  elsewhere.
- [ ] **A CLI hazard hit again, and it is now twice in three weeks.** `graphit config get X` parses
  `get` as the KEY and writes `{"get": "X"}` into the tracked `graphit.lock.json`, printing
  `✓ Set get = …`. Memory `01KZV193K3KDXH5EBF66TKMHCD` documented it in August; it recurred here.
  Unknown key + two positional args should fail, not write. Also worth noting: `config unset` drops
  the now-empty `config` object, so cleaning up dirties the tracked lockfile — `git checkout
  graphit.lock.json` afterwards.

### 2026-09-02 (T2.3's blocker was WRONG, and the seam is now landed and proved)

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues).

**I misdiagnosed the blocker in the entry above, and the correction matters more than the code.** I
wrote that T2.3 required replacing the memory wiki's incremental machinery, and therefore that T4 had
to come first. That was wrong, and the thing that disproves it is work this same session had already
done: **T1 slice 2 changed `wiki.FastPathCheck` to compare `content_hash` BY QUERY against the index**
rather than by `os.ReadDir`. Its signature is
`FastPathCheck(ctx, wikiDir, entries []DocHashEntry, cache)` — the entries are supplied by the
CALLER, so the gate never cared whether they came from a directory or from a table.

The lesson is specific enough to reuse: **when a coupling looks structural, check whether an earlier
slice of the same task already dissolved it.** I read `GenerateMemoryWiki` top-down, saw four
filesystem calls, and concluded "no seam" — without checking what the gate it feeds actually compares
now. One read of `FastPathCheck` would have saved the wrong conclusion, and that conclusion had
already been written into the log and into a memory as a plan revision.

#### What landed

- [x] **T2.3a — the generator is source-agnostic, and the table is a source.**
  `internal/memory/wiki.go`:
  - `compileMemoryWiki(ctx, docs, wikiDir, processCache, logger…)` — everything from sorting onwards
    factored out: slug resolution, the incremental gate, chunk building, `RebuildDB`. It takes
    documents and never asks where they came from.
  - `GenerateMemoryWiki` keeps the markdown enumeration and calls it. **Behaviour-preserving** — the
    whole existing memory suite is green with no test changed.
  - `GenerateMemoryWikiFromTable(ctx, tbl, wikiDir, logger…)` — the new entry point.
  - `memDocFromRecord(rec)` — the row in the shape the compiler expects, with `filename` holding the
    PATH form (`<id>.md`, `history/<id>/<rev>.md`) so a migrated scope compiles to the same slugs,
    the same cache keys and the same order as it did from files.

  **Four mechanisms are absent from the table path rather than ported**, each because the table makes
  it unnecessary: `ImportShards` (the `embedding` column is why it exists), `StatPreCheck` (a gate
  that avoided READING files, and reading rows IS the enumeration), `dedupMemoryDocsByID` (a table
  keyed by the declared id cannot hold two rows for one id — the same reason `repair.go` retires),
  and `ExportShards` (there is no publish step; the table is written where it lives).

  Three tests in `internal/memory/wiki_from_table_test.go`: the chain compiles with its columns
  (`entity_id`, `revision_id`, `previous`, `next`, `current_id`) and the two revisions do not collide
  on a slug; a second compile over an unchanged table writes 0; and a record whose content hash
  changed IS recompiled — that last one matters because a gate that always skips is a bug this
  project has shipped before.

#### What remains, and why it has to be ONE atomic slice

Nothing is wired to `GenerateMemoryWikiFromTable` yet, and wiring it alone would be the failure this
session already refused once: compiling from an empty table empties the wiki. The remaining switch is
atomic and its order inside the commit does not matter, but its parts do:

1. **The writes** — T2.2b–f, whose design is recorded in the entry above in enough detail to re-type.
2. **`IndexMemories` / `RunCycle`** → `GenerateMemoryWikiFromTable`.
3. **The listing readers** → `Live(ctx)`: `ListMemories` (`memory.go:410`), `important.go:33`,
   `consolidate.go:161`, `dream.go:320`. These are direct.
4. **🔒 THE MIGRATION NEEDS A SURFACE FIRST, and this is the real gate.** The 324 live memories on
   this machine are in the raw store; the project scope's TABLE does not exist yet, because T2.1 ran
   the migration through a throwaway harness — recorded as debt above ("The migration has no CLI or
   MCP surface"). Switching reads and writes before the migration has run into the REAL scope URI
   makes every existing memory invisible. So the order is: give the migration a surface, run it into
   `MemoryTableURI(...)`, verify with `MigrationReport.Verified()`, and only then switch.

That reordering is the concrete next step, and it is smaller than what was planned for T2.3.

### 2026-09-02 (the migration has a surface, and it ran on the REAL store — 417 rows verified)

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues), `make install`.

#### What landed

- [x] **T2.3b — `graphit memory migrate`.** `newMemoryMigrateCmd` in
  `cmd/graphit/commands/memory.go`, `runMemoryMigrate` in `cmd/graphit/commands/runners.go`, and
  `MemoryService.TableURI()` as the accessor that gives it the target. Flags mirror the rest of the
  memory commands: default project scope, `--user`, `--context <name>`.

  It **reports rather than decides**. A NOT VERIFIED result is not turned into a non-zero exit for
  its own sake, because a pass may have moved 415 of 417 and what the operator needs is WHICH two did
  not — so the report names them. What it refuses to do is imply success: the summary says NOT
  VERIFIED and says the raw store must stay.

  `MemoryService` regained its `tableURI` field, and `tableScope(scope, scopeID)` now derives the
  (scope, scopeID) pair the LOCAL directories are named from — an imported context names both halves
  after itself, the project and user scopes use their scope word and their id. One rule, so a scope
  cannot own two differently-named directories.

#### Proved against the real store, not a fixture

```
graphit memory migrate
  /home/…/.graphit/memory-raw/memory-project-01KSH1CRFFG8Z74B5ZS78WW808
  → s3://graphit/memory/project/01KSH1CRFFG8Z74B5ZS78WW808
  325 live + 92 archived = 417 rows — verified          (3.6s)
```

- **Verified by row count AND content hash**, which is the check T2.1 built and the reason it exists.
- **Idempotent, measured**: the second run reported 417 again, not 834. Upsert on the row key holds.
- **The user scope migrated to 0 rows**, correctly — its raw directory holds no memories on this
  machine.
- Written to the project's own configured hub (`hub.bucket = graphit` on a local MinIO). Nothing
  reads the table yet, so this could not affect memory in use, and the migration deletes nothing.
- 417 rows here against 409 at T2.1 — 325 live vs 320, 92 archived vs 89 — which is this session's
  own writes, and is the consistency check on the count.

#### 🔒 A finding from listing the bucket, which belongs in T2.4's spec and was not written anywhere

The remote memory prefix holds **2316 objects**, and the breakdown says what T2.4 actually has to
clean up:

| Objects | What |
|---|---|
| 1816 | under `.wiki/shards/` — the shard cache `shardsync.go` mirrors into the prefix |
| **567** | **`_important_`-suffixed twins — residue of the 184-memory fork corruption** |
| 82 | the new `memories.lance` table, holding all 417 memories |

Two things follow. First, **`repair.go` healed the twins LOCALLY and the published copies were never
deleted**: `Publish` removes only the objects a caller tracked as removed, so a twin that was
mirrored up before the repair is still there weeks later. Second, the compaction is dramatic — 82
objects for 417 memories against 1816 for the shard cache of the same data.

So T2.4 is not only "delete the raw store": it must **delete the remote `.wiki/` prefix** when
`shardsync.go` retires. Left alone, a scope keeps paying for 1816 objects that nothing reads, 567 of
which were already dead.

- [ ] **T2.4 addition — delete the remote `.wiki/` prefix with shardsync.** `s3store` already has
  `DeletePrefix`, which `PublishContextDir` uses. Measured on this machine: 1816 objects, 567 of them
  twin residue.

#### The remaining switch, now down to one atomic slice with its gate cleared

The migration gate is OPEN: the project scope's table exists at its real URI and is verified. What is
left is a single commit that must not be split:

1. **The writes** — T2.2b–f, design recorded above in enough detail to re-type.
2. **`IndexMemories` / `RunCycle`** → `GenerateMemoryWikiFromTable` (landed, tested).
3. **The listing readers** → `Live(ctx)`: `ListMemories` (`memory.go`), `important.go:33`,
   `consolidate.go:161`, `dream.go:320`.

Run `graphit memory migrate` once more immediately before that commit, because memories written
between now and then go to the raw store and would otherwise be left behind.

### 2026-09-02 (T2.2 + T2.3 DONE — the memory store IS the Lance table, proved end to end)

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues), `make install`.

- [x] **T2.2 — the five write paths write the table.** `AddMemory`, `updateMemory`, `RemoveMemory`
  with `archiveBeforeDelete`/`archiveRevision`/`repointArchiveNext`, and `changeRelevance`.
- [x] **T2.3 — every reader reads the table.** `IndexMemories` and `RunCycle` compile through
  `GenerateMemoryWikiFromTable`; `ListMemories` and `ListImportantMemories` read `tbl.Live(ctx)`;
  `SyncContextFromMemoryRepo` reads the context's table instead of downloading it.

#### Proved on the real store, through the installed binary

```
graphit memory migrate   →  326 live + 92 archived = 418 rows — verified
graphit memory index     →  Indexing memories from s3://graphit/memory/project/<id>
graphit memory list      →  326 memorys
graphit memory important →  248
```

Then the round trip that actually settles it: a memory written through
`graphit_memory_insert` **is in the table** (`memory list` finds it), **is NOT in the raw store**
(`grep` over `memory-raw/` finds nothing), and `memory index` + `memory search` return it. Write →
table → compile → search, with the raw store out of the path.

#### The four decisions in this slice that are not obvious

1. **`Publish` disappeared rather than being ported.** It existed because the truth was local and had
   to be pushed; a table opened at an `s3://` URI is written where it lives. That took the async push
   goroutine and `WaitForPendingPushes` with it — the thing that made a write's durability something
   a test had to wait for.
2. **The markdown is a TRANSIT FORMAT, not a file.** The four content transforms keep operating on
   text and `putMarkdown`/`readMarkdown` bracket them through `recordFromMarkdown` /
   `MemoryRecord.Markdown()`. Nothing reaches disk. Deliberate: those four carry the
   revision/previous/next bookkeeping a data-loss bug was found in, and `recordFromMarkdown` enforces
   the guard that came out of it — a frontmatter that does not parse is REFUSED, never re-rendered
   from an empty struct. Rewriting them onto `MemoryRecord` is the cleaner end state and is a slice
   of its own, with tests.
3. **`repair.go` was DELETED, not ported** — ~370 lines and 7 tests. It healed 184 memories forked
   into twins by an id recovered from a FILE NAME; a row is keyed by the id the memory declares, so
   two twins collapse on upsert and the defect is no longer expressible. `sameMemoryBody` moved to
   `memory.go`, because the question it answers is not about repair: two revisions differ in their
   frontmatter on every write, so "did the text change" needs the frontmatter excluded.
4. **`resolveTableURI()` derives the URI when the field is empty, and the field is an OVERRIDE.** Not
   a fallback — it is the same value from the same two inputs the constructor uses. It exists so a
   test can point a scope at a temporary directory instead of the machine's real store; without it,
   the ~20 tests that build `&MemoryService{…}` as a struct literal answered "not configured" about a
   perfectly well-defined scope.

#### A deliberate behaviour change, and a hazard the compiler cannot catch

**`RunCycle` with an unopenable URI now ERRORS.** The raw-store version treated a missing directory
as "nothing to compile" and returned no error. Opening a table CREATES it, so a location that cannot
be opened is not an empty scope — it is broken, and reporting success would hide a whole scope
silently stopping.

**`RunCycle(ctx, scope, tableURI, wikiDir)` changed its third parameter from `rawDir` to `tableURI` —
same type.** Both external callers (`internal/daemon/memorysyncmodule.go:124`,
`cmd/graphit/commands/lifecycle.go:1179`) kept compiling while passing the wrong value. Found by grep,
not by the build. **When a parameter's meaning changes but its type does not, grep the call sites —
the compiler is not going to help.**

#### The tests were the signal, again

24 failed after the switch, and every one asserted the old source. What they became:

- 7 history tests → read rows; `readStored`/`archivePaths` replace `w.ReadFile`/`w.ListDir`, still
  taking PATHS because that is what `previous`/`next` hold.
- 6 `ListMemories` tests → 2. Their surviving subject is the catalogue and its classification; a
  missing directory and an `.md` extension to skip are subjects that no longer exist. The replacement
  proves something the directory version could not: **a superseded row is excluded**, and the entry
  now carries `Type` and `Tags`, which a listing never had.
- 4 `RunCycle` + 2 `IndexMemories` → 4, including the unopenable-URI case above.
- 7 repair tests deleted with `repair.go`; `twinFile` went too — caught by `unused`, which is the half
  of dead-code detection `.golangci.yml` says is automated.

#### Also fixed

`graphit memory index` printed `Indexing memories → <raw dir>` while compiling from the table.

## Remaining

- **T2.4** (destructive, needs explicit confirmation): delete the raw store, `shardsync.go`, and the
  remote `.wiki/` prefix — 1816 objects, 567 of them twin residue. Also drops `ExtractScopeDir` and
  the now-inert `provider` parameter of `SyncContextFromMemoryRepo`/`OnHubImport`.
- **T3, T4, T5.**

### 2026-09-02 (T2.4a — the markdown compile path and the shard mirror retired. CODE only; no data deleted)

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues), `make install`, and the real cycle still answers —
`memory index` compiles from `s3://`, `memory list` returns 327, search resolves.

**T2.4 was split, and the split is the point.** Retiring the CODE is reversible and belongs with the
work that made it dead. Deleting the DATA — 433 local markdown files and 1816 remote objects — is
not, so it waits for an explicit go-ahead and is tracked separately below.

#### What was retired, and how it was found to be dead

The trigger was a query, not a guess: **`GenerateMemoryWiki` had ZERO production callers** after
T2.3 — only tests kept it alive. That made the whole markdown compile path dead, and everything that
existed to serve it:

| Deleted | Why it was dead |
|---|---|
| `GenerateMemoryWiki`, `buildMemDoc`, `dedupMemoryDocsByID` (`internal/memory/wiki.go`) | the file-source compile; the table source replaced it |
| `internal/memory/shardsync.go` — `ImportShards`, `ExportShards`, `shardMirrorDirName` | only the markdown path called them; the `embedding` column is what replaced the vector mirror |
| `MemoryStoreProvider`, `MemoryStore.ExtractScopeDir` | the download-then-compile interface; a context's table is read where it lives |
| the `provider` and `projectDir` parameters of `SyncContextFromMemoryRepo` and `OnHubImport` | inert since T2.3, and both now take only the context NAME |

`dedupMemoryDocsByID` deserves its own line: it resolved two files claiming one id, which is the
same defect `repair.go` healed. Neither can exist in a table keyed by the declared id, so both went
for one reason.

**What SURVIVES, and why, because this is the part a later pass could get wrong:**
`memorySourceFileNames` and `isMemorySourceFile` stay because **the migration still reads the raw
store** (`migrate.go:111`), and `isHistorySource` stays because `recordFromMarkdown` derives
supersession from the path. They retire with the data, not with the code.

#### 21 test functions went with it, and the linter found the last five

The tests deleted are the ones whose SUBJECT was the file source: 13 `GenerateMemoryWiki_*`, 6
shard-sync tests, and the provider variants. Three collapsed instead of dying —
`TestSyncContextFromMemoryRepo_{NilStore,WithStore,WithStoreError}` differed only in the provider
they passed, so they became one test that proves compiling a context needs only its name.

`TestMemoryBranch` went too: it pinned the scope→prefix mapping, which now lives in
`ScopePrefix`/`TableURIFor`/`ContextTableURI` and is covered by
`TestAContextResolvesToTheProjectPrefixRemotelyAndADoubledNameLocally`.

**Then `unused` caught five leftovers a human sweep would have missed** — `mockStoreProvider`,
`mockStoreProviderCov`, their `ExtractScopeDir` methods, and `assertIndexedChunks` — and removing
those emptied `wiki_reindex_completeness_test.go`, which was deleted. That is the automated half of
dead-code detection doing exactly what `.golangci.yml` says it does, on the half it can see.

## Remaining

- [ ] **T2.4b — DESTRUCTIVE, needs explicit confirmation.** Delete the raw markdown store
  (`~/.graphit/memory-raw/`, 433 files across 5 scopes) and the remote `.wiki/` prefix (1816 objects,
  567 of them forked-twin residue). Gated on `graphit memory migrate` reporting verified, which it
  does: 418 rows. Retires `memorySourceFileNames`, `isMemorySourceFile`, `isHistorySource`,
  `migrate.go` itself, and `MemoryStore`'s remaining file surface.
- [ ] **T3** — knowledge builds into a local table and publishes by writing it to S3. Includes the
  two-live-publish-mechanism swap and D4 (the Parquet bundle dies), plus the `BundleDir ==
  WikiIndexDirName` trap recorded above.
- [ ] **T4** — incremental by `Upsert`/`DeleteByKey` instead of `RebuildDB` dropping four tables.
- [ ] **T5** — rules, specs, ADR, and the memory that still says a wiki has three artifacts.

### 2026-09-02 (T2.4b DONE — the raw store is gone, locally and remotely. T2 is closed)

Green: `go build ./...`, `go build -tags lancedb ./...`, `go test -tags lancedb -count=1 ./...`
(46 packages), `make lint` (0 issues), `make install`. The full cycle answers with no raw store on the
machine: `memory index` compiles from `s3://`, `memory list` returns 327, `memory important` 249,
search resolves.

#### 🔒 The check that had to happen first, and it found 15 memories about to be destroyed

The raw store root is **GLOBAL — shared by every project on the machine.** It held five scopes, and
`graphit memory migrate` resolves ONE scope from the current project's lockfile. So the four other
scopes had never been migrated, and `graphit_hub_projects` returned empty — the global lock no longer
knows those projects, so their directories cannot be resolved to migrate them project by project.

Two of them held real memories: **1 and 14**. Deleting the store after migrating only this project
would have destroyed 15 memories belonging to projects nobody can check out.

**`graphit memory migrate --all` is the fix**, and it works because the migration never needed the
project: a table's URI is built from the SCOPE ID, so a directory name is enough.
`memory.RawScopesIn(root)` parses the flattened `memory-<scope>-<id>` names the same way
`ContextNamesFrom` does, and `TableURIForRawScope` maps each to its prefix. All five scopes migrated
and verified before anything was deleted.

#### A bug in the verification itself, found by running it

The first `--all` pass reported this project's scope **NOT VERIFIED: row count 419, expected 418**.
Nothing was lost: 418 files migrated into a table that already held 419 rows, because one memory had
been written straight to the table earlier in the session.

`MigrationReport.Verified()` required `Rows == Live+Archived`, which assumed the table holds ONLY what
the pass migrated — true while the raw store was the source of truth, **false from the first native
write.** It would have said NOT VERIFIED forever, and the retirement it gates would never have been
allowed. The count is now a FLOOR: fewer rows than sources still fails, because that means rows went
missing; more means the store is ahead of the files, which is the normal state. The report names the
surplus — `(1 written natively)` — rather than leaving a reader to reconcile two numbers. The count
was never the fidelity proof anyway; the content hash is, and it is unchanged.

#### What was deleted

| Where | Before | After |
|---|---|---|
| `~/.graphit/memory-raw/` | 433 markdown files, 5 scopes | gone |
| `s3://graphit/memory/` | **2454 objects** | **217** — only the `memories.lance` tables |

The remote breakdown that went: 1818 under `.wiki/shards/` (the mirror `shardsync.go` maintained,
567 of them forked-twin residue), 327 loose memory `.md` objects, and the whole `history/` tree.
A 91% reduction, for the same memories. Backup of the local store at
`/tmp/graphit-rawstore-final-backup` before deletion.

#### The reader T2.3 missed, and why it was silent

**`RunConsolidation` still read `RawDir(scope)`.** With the directory deleted,
`loadMemorySnapshots` returns `os.ErrNotExist`, which that path converts to "nothing to consolidate"
— so consolidation reported a clean run over zero memories. **A no-op that looks like success**, and
the same failure class as the incremental gate bug earlier in this task.

Fixed by the same move as the wiki generator: `consolidateSnapshots(ctx, memories, aiClient, vecs)` is
now the source-agnostic analysis, and `RunConsolidation` loads snapshots from `tbl.Live(ctx)`.

#### Everything else that still reached for the raw store

- `internal/dream/dream.go` gated consolidation on `RawDir(scope) != ""`. The real gate is the
  scope's IDENTITY — `TableURIForScope(scope) == ""` means there is no id to resolve.
- `internal/livesearch/prep/reclaim.go` and `internal/mcpstdio/tools_memory.go` deleted raw
  directories when reclaiming a session and un-importing a context. Both now drop the local TABLE
  directory. The context case gained a caveat worth stating: with a bucket configured the table is
  REMOTE and is not dropped — it belongs to the project that published it, and un-importing is not
  deleting.
- **`SyncToLocal` had nothing left to sync.** It pulled the remote prefix into a local directory and
  compiled from those files, with a fast path that skipped the network and a slow one that did not —
  a distinction that meant something while the truth was a directory that could be behind a bucket.
  It only recompiles now, and `syncToLocalInternal` is gone.
- **`MemoryStore.EnsureInitialised` stopped creating the raw root.** It kept recreating an empty
  `memory-raw/` on every run after the deletion — which is not harmless: an empty directory with that
  name tells the next reader the raw store is still a thing, and that is how a retired mechanism gets
  restored instead of finished.

#### Two contracts changed, and the tests say why

`SyncToLocal` **no longer errors** when the store is unusable. Three tests asserted that it does, and
they were right while it also synced: a failed pull meant the local copy was wrong. It runs AFTER a
write that already succeeded now, so erroring would report a stored memory as unstored. The failure is
logged. `TestMemoryService_NoGitStore_Errors` keeps every write in its list and deliberately drops
`SyncToLocal` from it, with the reason in the test.

`EnsureInitialised` asserted the raw root exists afterwards; it now asserts the opposite.

## Remaining

- [ ] **`ScopeStore` has ZERO production callers** — `OpenScope`, `OpenScopeLocal`, `WriteFile`,
  `ReadFile`, `RemoveFile`, `ListDir`, `Pull`, `Publish`, `uploadDir`, `copyDirRecursive`,
  `HasLocalScope`, `ScopeDir`, `remoteRevision`. Only comments name them. `unused` cannot see it
  because they are exported methods. Deleting them is mechanical and was left out of this slice to
  keep it reviewable.
- [ ] **`migrate.go` and its surface still exist, deliberately.** This machine's raw store is gone,
  but another unit's is not, and the migration is the only path across. It retires when the team has
  no raw store left — a decision that belongs to whoever knows that, not to this pass.
  `memorySourceFileNames`, `isMemorySourceFile` and `RawDirFor` retire with it.
- [ ] **T3** — knowledge builds into a local table and publishes by writing it to S3: the
  two-live-mechanism swap, D4 (the Parquet bundle dies), and the `BundleDir == WikiIndexDirName` trap.
- [ ] **T4** — incremental by `Upsert`/`DeleteByKey` instead of `RebuildDB` dropping four tables.
- [ ] **T5** — rules, specs, ADR, and the memories that still describe a wiki as three artifacts.

### 2026-09-02 (the migration retired, and T0+T1+T2 committed as 832e5bd)

The user settled the one thing holding the migration open: the other scopes in the raw store are
dev test data, so nothing needed carrying across. `migrate.go`, `graphit memory migrate` (both forms)
and its five tests are deleted, and the linter then found the three file enumerators that only it
used — `isMemorySourceFile`, `memorySourceFileNames`, `historySourceFileNames`.

**One test was rescued rather than deleted, and the distinction is the one this task kept meeting.**
`TestAMemoryRecordSurvivesTheRoundTripWithEveryField` was swept up because it compared canonical
hashes through the migration's helper — but its SUBJECT is the `memories` schema, which is live and
is the whole of T2.1's guarantee. Rewritten to compare field by field, which is stricter than the
hash: it names WHICH field was lost instead of only that one was.

`internal/ast` failed once in a full run and passed alone and on re-run — a flake, noted rather than
chased.

Committed as **832e5bd**, 102 files, +8092/−6371. One commit, deliberately: the boundaries are
interleaved in the same files — `wiki/pipeline.go` carries both a legacy deletion and the publish bug
fix, `internal/memory/*` carries the legacy sweep and all of T2 — so splitting them would produce
intermediate commits that may not build.

## Remaining after 832e5bd

- [ ] **The last raw-store threads.** Four are load-bearing and were left out to keep the commit
  reviewable, each with its own decision:
  - `ScopeStore` and its whole file surface have ZERO production callers. Mechanical deletion;
    `unused` cannot see it because they are exported methods.
  - `memory.ContextNames()` lists imported contexts from `store.MemoryRawRoot()`, which no longer
    exists — **it answers empty now**, so context listing is silently broken until it reads
    `MemoryTableRoot()`.
  - `internal/daemon/memorysyncmodule.go` watches the raw root for file changes. There are no files;
    a write recompiles inline through `syncToLocalFast`, so the module is vestigial.
  - `consolidate_apply.go` keeps its apply state under `LocalDir()`. That needs a home — the table
    directory.
- [ ] **T3** — knowledge builds into a local table and publishes by writing it to S3: the two live
  publish mechanisms collapse, D4 (the Parquet bundle dies), and the `BundleDir == WikiIndexDirName`
  trap where the exclusion predicate and the bundle directory share a name and mean opposites.
- [ ] **T4** — incremental by `Upsert`/`DeleteByKey` instead of `RebuildDB` dropping four tables.
- [ ] **T5** — rules, specs, ADR, and the memories that still describe a wiki as three artifacts.

### 2026-09-02 (session opened on the remainder after 832e5bd)

Resumed from `## Remaining after 832e5bd`. Order for this session, chosen so the one DEFECT lands
before any planned work — a silently-empty listing gets read as "memory contexts do not work", and
the longer it sits the more likely someone concludes that:

- [ ] **R1 — `memory.ContextNames()` points at `MemoryTableRoot()`.** Spec: `internal/memory`
  (`paths.go`/wherever `ContextNames` lives) plus a regression test. Done means listing an imported
  memory context answers with its name again. Constraint: `ContextNamesFrom` recognises a context by
  the flattened `memory-<scope>-<id>` name with `scope == id`; the table directory reuses that
  segment, so the PARSE must not change — only the root it enumerates.
- [ ] **R2 — delete the `ScopeStore` file surface.** Spec: `OpenScope`, `OpenScopeLocal`,
  `WriteFile`, `ReadFile`, `RemoveFile`, `ListDir`, `Pull`, `Publish`, `uploadDir`,
  `copyDirRecursive`, `HasLocalScope`, `ScopeDir`, `remoteRevision`. Done means they are gone and
  both builds are green. Constraint: `unused` cannot see exported methods, so the zero-caller claim
  is verified against the graph, per name, before each deletion — not assumed from the earlier note.
- [ ] **R3 — delete `internal/daemon/memorysyncmodule.go`.** Done means the daemon still registers
  its remaining modules and no test references the removed one.
- [ ] **R4 — `consolidate_apply.go` apply state moves to the table directory.**
- [ ] **T3**, **T4**, **T5** as specified above. T3 starts by separating `BundleDir` from
  `WikiIndexDirName`, before either mechanism is touched.

### 2026-09-02 (R1–R4 done — and the defect was bigger than one root, in both directions)

**R1 was not a one-line fix, and the fix named in the plan would have been wrong.** The plan said to
point `AllContextDirs` at `MemoryTableRoot()` because the table directory reuses the flattened
`memory-<scope>-<id>` segment, so the parse would not have to change. Running the installed CLI
against the real global directory is what disproved it: **with a bucket configured — which is the
normal configuration, and the one this machine runs in — a scope has NO local table directory at
all.** The table is `s3://<bucket>/<prefix>/memory/<scope>/<id>` and nothing is written locally for
it. `~/.graphit/memory-table/` does not exist here, while `graphit memory list` answers perfectly.
So keying the listing on the table root fixes the undefined root and still answers empty.

The record of an imported memory context is its **compiled wiki**, `wiki/memory/<name>/<name>`,
because being local is what that artifact is for — it is what a search opens, so it exists in both
configurations. `ContextNamesFrom` now enumerates `store.MemoryWikiRoot()` and recognises a context
as the scope whose two path segments are equal, which is the same doubling test in the nested layout
instead of the flattened one. `cutPrefix` and `doubledName` went with the flattened parse.

**And the same defect existed on the WRITE side, which is why it was worth chasing past the plan.**
`newMemorySvcInternal` derived `wikiDir` from `string(scope)`, and for a context that is the literal
word `"context"` — so `NewMemoryServiceForContext(name, …).SyncToLocal()` compiled into
`wiki/memory/context/<name>` while every reader of a context's memories looks in
`wiki/memory/<name>/<name>`: the UI's picker, `SyncContextFromMemoryRepo`, and the removal path.
Three call sites compile through the service — `runASTImport`, the MCP `ast_install`, and the MCP
memory-context sync — so all three produced a wiki nobody opens. The fingerprint was an **empty
`~/.graphit/wiki/memory/context/` directory** in the global store, which is what `ensureWikiDir`
left behind. `tableScope` is now `localScope` and names BOTH local artifacts, so the compile and the
listing cannot disagree again.

**R4 was a silent no-op of the same family, not a homeless state file.** The plan described
`consolidate_apply.go` as keeping apply state under `LocalDir()` and needing a new home. It is not
state that is kept: `loadApplyState` LOADED the plan's starting state by enumerating markdown files
in that directory. With no files, every action was refused with "memory no longer exists" — a
consolidation that applied nothing and reported a clean run. Exactly the failure `RunConsolidation`
had before T2.3, one layer down, and the reason it survived T2.3 is that the reader is behind an
interface. `MemoryWriter.LocalDir() string` is now `LiveMemories(ctx) ([]MemoryRecord, error)`,
`ApplyConsolidation` takes a `context.Context`, and `MemoryService.LiveMemories` is the service-level
read. `snapshotsFromRecords` is now the ONE mapping from rows to the analysis's view, shared by the
pass that plans and the pass that applies.

**A fourth silent breakage found on the way:** `pruneLocalScope` removed the raw directory, so
reclaiming a scope deleted its bookkeeping and left its table — the only copy of its memories with
no bucket — behind. It removes the table directory now.

**And one trap removed rather than found:** `SyncToLocal` re-derived `m.wikiDir` on every call. That
was a no-op for a service built by the constructor, which sets the same value, and it silently
discarded the field as an override for anything that set it directly — a test pointing a scope at a
chosen wiki directory got the machine's real one from the first write onwards. It cost me a
debugging round on a converted test; the assignment is gone.

Deleted with R2/R3: `ScopeStore` and its entire file surface, `MemoryStore`'s S3 client (a scope's
table reaches storage through its own URI, so the second client could only answer "is a bucket
configured", which is `config.HubS3Config().Configured()`), `remoteRevision` and its listing cache,
`ExtractScopeDir`, `copyDirRecursive`/`copyFileData`, `pendingPushes`/`WaitForPendingPushes` and its
two call sites — a table commit finishes before the write path returns, so there is nothing left to
wait for on exit. `internal/daemon/memorysyncmodule.go` and the free functions only it used
(`anyUnder`, `scopeDir`, `parseScopePath`). `memory_s3_store.go` became `memory_store.go`, which is
what it now is. In `internal/memory`: `RawDir`, `RawDirForScope`, `RawDirFor`, `RawScope`,
`RawScopesIn`, `TableURIForRawScope`, `listImportantInDir`, `parseMemoryMeta`, `parseMemoryHeader`,
`ParseMemoryMetaPublic`, `loadMemorySnapshots`, `consolidateDir`, `consolidateDirWithVectors`,
`MemoryService.localDir`/`LocalDir`. In `internal/store`: `MemoryRawRoot`, `MemoryRawDir`.

**Tests: 56 deleted, 8 converted, 3 added.** The deletions are all coverage tests whose SUBJECT left
with the subsystem — the ScopeStore file surface, the directory-based consolidation loader, the
markdown-directory readers, and four `ListRecentMemories`/`RenderImportantBlock`/`RenderRecentBlock`
tests that turned out to exercise **test-local reimplementations of production functions that no
longer exist at all**. The conversions kept their subject: `consolidateSnapshots` is handed a corpus
directly, `fakeWriter` holds markdown in a map and produces records through the same
`recordFromMarkdown` the service stores through, and the store-layout test asserts
`MemoryTableDir` **plus the absence of `memory-raw`** in every resolved path. The new
`TestAllContextDirs` seeds both wrong roots fully populated and requires them to yield nothing
before it seeds the right one.

Verified: `go build ./...` and `-tags lancedb`, `go test -tags lancedb -count=1 ./...` (all packages),
`make lint` 0 issues, `gofmt` clean, `make install`, and `graphit memory list` against the real store.
Note for the next session: `go test` WITHOUT `-tags lancedb` fails 23 tests in `internal/memory` on
`main` as well — the package's tests need the tag, and that is pre-existing, not a regression.

### 2026-09-02 (resumed after the R1–R4 implementation stopped before T3)

The preceding session completed and verified R1–R4, updated this log, and then stopped during the
read-only investigation of T3. Its working tree is intentionally preserved. No T3 source change had
landed when this session resumed.

The investigation established the two live knowledge-publish mechanisms that T3 must collapse:

- the versioned Hub artifact path calls `prepareKnowledgePublish` → `wiki.ExportToParquet`, but the
  function no longer converts anything — it copies `index.lance` under the artifact prefix, and a
  `lancedb` build already mounts that index directly from S3;
- the legacy context-export path calls `stageWikiForPublish` → `PublishContextDir`, publishes process
  cache shards, downloads them, and runs `BuildDBFromCache`. This violates the established invariant
  that shards are local build artifacts and LanceDB is the published queryable artifact;
- the MCP form of that legacy path stages a `wiki/` child and then publishes it under a prefix that
  already ends in `wiki`, yielding `contexts/knowledge/<id>/wiki/wiki/...`; the CLI form stages the
  contents directly. The collapse removes this double-`wiki` defect rather than preserving either
  spelling;
- `BundleDir == WikiIndexDirName` currently gives one name two opposite meanings: the directory that
  must be copied as the artifact and the directory excluded as derived output. T3 separates those
  concepts before deleting the obsolete transfer/import surface.

Next: revalidate callers and tests against the current AST graph, commit the completed R1–R4 slice as
its own reviewable unit, then implement T3 without mixing the memory cleanup with the publish rewrite.

### 2026-09-02 (R1–R4 committed; T3 implementation opened without compatibility work)

R1–R4 were rechecked with `go test -tags lancedb ./internal/memory ./internal/daemon
./internal/livesearch/prep`, `go build ./...`, and `go build -tags lancedb ./...`, then committed as
**95b2a36** (`refactor(memory): remove the last raw-store paths`).

The user clarified that this project is still in development: existing data is test data and may be
discarded. T3 therefore has no backward-compatibility or migration requirement. The legacy
unversioned knowledge-context transport is removed rather than adapted.

T3 now proceeds as a destructive simplification to the single target mechanism:

1. rename the neutral operation around a published wiki from Parquet import/export to publishing or
   staging a Lance index for a versioned Hub artifact;
2. delete the legacy CLI/MCP `knowledge export` and `knowledge install` context transport instead of
   preserving it as an alias, adapter, fallback, or migration path; Hub publish/install is the sole
   publication surface;
3. delete context-directory publish/fetch, shard rebuild, bundle import, and old compatibility tests
   once their callers disappear; no existing local context data will be migrated;
4. preserve the non-`lancedb` refusal as an explicit capability error — such a binary cannot search a
   Lance index locally or remotely, so retaining a shard fallback would only preserve a second store;
5. make installed versioned knowledge artifacts discoverable and readable by the remaining knowledge
   surfaces through their immutable S3 mount, then cover that shape with unit tests plus the existing
   MinIO E2E.

### 2026-09-02 (T3 legacy context transport removed)

The destructive simplification requested by the user is now implemented in the production surface:

- CLI `knowledge install` and `knowledge export` were deleted; `hub install --type knowledge` and
  `hub submit --type knowledge` are the only installation/publication route;
- MCP `graphit_knowledge_install` and `graphit_knowledge_export` were deleted for the same reason;
- context-specific `knowledge sync` was deleted. Sync now only rebuilds the current project's wiki;
- `S3Store.ContextPrefix`, `PublishContextDir`, and `FetchContextDir` were deleted, so the unversioned
  `contexts/knowledge/<project>` object layout no longer exists in code;
- the shard-based imported-context compiler and its compatibility tests were deleted. No existing
  local context data is migrated.

The remaining T3 work is consumer-side: make versioned Hub knowledge claims listable and ensure every
knowledge reader that accepts a context opens the mounted Lance index directly, then remove stale
surface assertions and documentation.

### 2026-09-02 (T3 consumer paths now mount the versioned index)

The remaining hidden copy was removed from multi-wiki Hub references:

- `HubService.EnsureKnowledgeAvailable`, which cloned a knowledge artifact for an ad-hoc search,
  became `ResolveKnowledgeMount` and now returns the immutable Lance mount only;
- `wiki.WikiSource` can carry a runtime `lancestore.Config`, so single-source and multi-source AI
  consultation, catalogue reads, page reads, and BM25 all use the mounted database handle;
- CLI knowledge search/query/lint and MCP knowledge search/list/lint now open installed Hub contexts
  through that same mount rather than resolving a nonexistent local directory;
- `InstalledContextsIn` treats a versioned Hub claim as installed without demanding local bytes;
- a build without LanceDB refuses knowledge install/mount explicitly instead of falling back to a
  download or another store.

Targeted LanceDB-tagged tests pass for `internal/wiki`, `internal/hub`, `internal/knowledge`,
`internal/mcpstdio`, `internal/wikisvc`, and `cmd/graphit/commands`. T3 still needs the final dead-code
and generated-surface sweep, full build/test gates, and the task's direct-S3 E2E when an endpoint is
available.

### 2026-09-02 (T4 incremental table synchronization implemented)

The destructive wiki rebuild has been removed. `WikiDB.Sync` now reads the table's current
slug/content projection, deletes only slugs absent from the desired corpus, and `Upsert`s only new
or changed slugs. An unchanged row is never rewritten, so its embedding remains in the sole LanceDB
store without an embedding shard or process cache. Cross-references follow the same rule: only
sources whose normalized edge set changed are deleted/reinserted. New/deleted rows are folded into
the existing indexes, while compaction and version pruning run on a timestamped maintenance
schedule instead of a rebuild counter.

The generators now derive all incremental evidence from LanceDB itself:

- `FastPathCheck` compares the desired slug/hash projection directly with `chunks`;
- knowledge staleness derives its previous source manifest from stored chunks, so
  `manifest.json`, `StatPreCheck`, and the process cache are gone;
- memory and knowledge call `SyncDB`; the old `RebuildDB`, cache-replay builder, shard embedding
  export, and cache-only tests have been removed;
- regression coverage proves that additions, changes and deletions defeat the fast path, that
  deleted vector rows disappear, and that an unchanged row keeps its embedding across a sync.

The first post-conversion checkpoint passes with `-tags lancedb` for wiki, knowledge, memory, Hub,
MCP, CLI, wikisvc, daemon, and UI server. Remaining work in this slice: remove stale comments/dead
helpers, add the explicit one-document delta assertion, and run the repository-wide gates.

### 2026-09-02 (T5 current contracts aligned; false compatibility surfaces removed)

The one-document delta is now pinned directly: the fixture changes row A and requires row B's
existing embedding to survive. This would fail under a drop/recreate implementation. Dead
`CheckAllHashesMatch`, `createTables`, `EmbeddingCache`, the stat/process-cache tests, and the
file-name repair guard were removed rather than retained as compatibility helpers.

The current architecture, storage, wiki, memory, Hub/S3, daemon, retrieval, MCP, ignore-file, and
brand documents now describe the LanceDB-only system. Historical task/changelog records were left
chronological on purpose. In particular:

- memory is an authoritative local-or-S3 Lance table plus a local Lance wiki projection;
- a local wiki contains only `index.lance/`; catalogue/page/log Markdown is rendered on demand;
- a Hub knowledge artifact is a versioned `index.lance/` opened at its derived `s3://` URI;
- the removed process cache, manifests, shards, raw-memory directory, and unversioned knowledge
  context prefix no longer appear as current behavior.

Two user-facing operations that had become lies were deleted instead of aliased: CLI `memory
export` and MCP `graphit_memory_export` only recompiled the wiki and then claimed to have exported
to Git. CLI `memory watch` watched a filesystem that no longer exists. The residual method name
`SyncToLocal` became `SyncWiki`, and both CLI/MCP memory schema output now describes the table
instead of a nonexistent graph. The obsolete `BuildDBFromCache` backlog item was removed through
the backlog registry because the function and its entire mechanism are gone.

The persistent project memories were reconciled in the same pass. The current “one wiki artifact”
memory now names `index.lance` as the only persisted representation and no longer lists the removed
process cache. The earlier per-file-cache decision was superseded with the direct incremental
LanceDB synchronization model. Both memories explicitly record that no format migration or
compatibility reader survives this development-stage reset.

After formatting, the affected modules pass their LanceDB-tagged test suites: memory, MCP, CLI,
daemon, wiki, knowledge, Hub, wikisvc, and UI server. Repository-wide build, test, lint, direct-S3
E2E availability, final index sync, and commit were still pending at that checkpoint.

### 2026-09-02 (T3–T5 verified and Graphit indexes synchronized)

The repository-wide gates pass:

- `git diff --check`;
- `go build ./...`;
- `go build -tags lancedb ./...`;
- `go test -tags lancedb -count=1 ./...` (all packages);
- `make lint` (0 issues).

The first lint pass identified three uncalled local-reader wrappers left behind by the transport
cleanup: `noKnowledgeToSearch`, `loadWikiPageFromIndex`, and `bm25PreFilter`. The AST graph confirmed
that none had callers, and they were deleted; their focused MCP/wiki tests and lint then passed.

The direct publication/mount test also passes explicitly together with read-only refusal and index
transport coverage. This host has no `GRAPHIT_LANCE_S3_ENDPOINT`/`GRAPHIT_LANCE_S3_BUCKET`, so the E2E
ran the identical publish → resolve mount → open → search → read → xref wiring against the local URI
transport. A live MinIO network run was therefore unavailable, not failed.

`graphit_sync` then completed successfully after the code, current documentation, task log, and
persistent-memory corrections were in place. The AST, project wiki, and memory wiki therefore all
describe the LanceDB-only state before the final commit.
