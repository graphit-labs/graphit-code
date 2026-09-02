---
title: LanceDB becomes the only store for knowledge and memory
status: planning
created: 2026-09-01
updated: 2026-09-01
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
- [ ] **D2 — is Lance version retention accepted as memory's recovery mechanism**, replacing the
  raw store? Still open, and blocked on T0 proving a rollback actually works through
  `lancestore`. The concurrency/retry cost is accepted.

## Plan & Task Breakdown

- [ ] **T0 — Prove the two properties the rest depends on.** Spec: a test, not a document.
  (a) a remote (`s3://`) Lance store can be written with retry on commit conflict, with two
  concurrent writers, against MinIO; (b) an earlier table version is readable after a
  destructive write, through `lancestore`, and `PruneVersions` retention is configurable.
  Done means both are covered by tests. **If (a) or (b) fails, T2 does not start** — the raw
  store stays until they hold.

- [ ] **T1 — Stop writing compiled pages; move everything that read them onto the table.**
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

- [ ] **T2 — Memory writes go straight to the table; the raw store retires.** Spec:
  `internal/memory`. `AddMemory`/`UpdateMemory`/`RemoveMemory`/`Promote`/`Demote` become
  `Upsert`/`DeleteByKey` on the scope's table, with retry. The revision chain stays exactly as
  it is — `entity_id`/`revision_id`/`superseded`/`current_id` are already columns, so an
  archived revision is just another row and `history/` stops being a directory.
  `ListMemories`, `ListImportantMemories`, `consolidate`, `repair` and `dream` read the table.
  `shardsync.go` dies with the raw store. `ScopeStore` becomes a thin remote-writable Lance
  handle. Constraint: T0 must have passed; the raw store is deleted only after a migration
  that reads it and writes the table, verified by count and by content hash.

- [ ] **T3 — Knowledge builds into a local table and publishes by writing it to S3.** Spec:
  `internal/knowledge`, `internal/wiki/transfer.go`, `internal/hub`. Local build unchanged in
  shape but with no page output; export writes the table to the published prefix instead of a
  Parquet bundle; a Hub context is opened read-only at its `s3://` URI and never downloaded.
  Constraint: depends on D1 for whether `docs/` remains the input.

- [ ] **T4 — Incremental, on the table.** Spec: `Upsert` by `slug` for changed documents,
  `DeleteByKey` for removed ones, `FoldNewRowsIntoIndexes` after a batch, `Compact` and
  `PruneVersions` on a schedule. Done means a one-document change does not rewrite the table
  and does not drop four tables, which is what `RebuildDB` does today.

- [ ] **T5 — Rules, specs, ADR, and the memory that says the wiki has three artifacts.**
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
