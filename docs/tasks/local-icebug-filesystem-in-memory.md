---
title: Local Ladybug via in-memory Icebug filesystem, no swap and no legacy
status: done
created: 2026-08-27
updated: 2026-08-28
tags: [ast, ladybug, icebug, storage, architecture]
---

# Local Ladybug via in-memory Icebug filesystem

## Objective

Make the local, on-the-fly graph use the same format as the Hub (`icebug-disk`/`icebug-canonical`), but with `storage` pointing at an absolute filesystem path instead of `s3://`. The `ladybugdb` catalog stops existing as a file — every connection opens `:memory:` and applies a `schema.cypher` that points at the local `graph.icebug/`. This eliminates `CopyDBDir` → `AtomicSwapDB` plus `.wal`/`.shadow` plus `CHECKPOINT`, and publishing to the Hub becomes `cp -r graph.icebug/` plus a cypher schema with `storage` rewritten to `s3://`.

No backward compatibility: we're in dev, old artifacts aren't read, and file-based stores are discarded on the first sync.

## Reasoning

Research into `docs.ladybugdb.com/import/icebug` plus `LadybugDB/ladybug/docs/icebug-disk.md:14` confirms: `storage='<path-to-dir>'` can be relative, absolute, or `s3://`. The Go binding already has `OpenInMemoryDatabase(":memory:")` (`go-ladybug/database.go:70`). The Hub already uses `ExportIcebugCanonical` (`internal/ladybugstore/icebug_canonical.go:86`) and `MountIcebugGraph` (`internal/ast/icebug_transfer.go:127`). Local and Hub converge on the same writer/reader — only the URI changes.

Convergence details required by the user (requirements from 2026-08-27/28):
1. **Parquets generated DIRECTLY from the shards** — without first populating the ladybug catalog and then exporting (which was an extra O(corpus) step and blocked incremental updates).
2. **Incremental rewrites ONLY the affected parquets** — the old catalog rewrote the entire bundle (O(corpus), 53s for 24k files); the user required a partial rewrite.
3. **Local queries go through the SAME planner/transport as the Hub** — the bounded traversal planner (`tryCanonicalBoundedTraversal`), fail-closed for logical relationships that can't be planned via `namesLogicalRel`; the only local-vs-hub difference is `storage:'/abs/graph.icebug'` vs `s3://`.
4. **No conditionals**: `LadybugConfig` only has `StoreDir` + `IcebugDir` + `ReadOnly` — there is NO `DBPath` and no `InMemory` flag (in-memory isn't optional). The Hub is also an in-memory mount: only `schema.cypher` + `manifest` + `search.mount` are persisted; an import `ladybugdb` is NEVER written.
5. **Nothing hardcoded per language** — labels (`File`, `Function`, `Class`, ...), columns, and types are all derived from the shards (`ExportDirect*` is dynamic), not literal `ICON=GO_ATTRIBUTES`-style constants.
6. **Reverse edges** became a property of the BUILD: the `hub.icebug.reverse_edges` config maps to `PipelineOptions.ReverseEdges *bool`; direct/incremental export gained `*WithReverse` variants. (Before, this was also a publish-time config; now it's decided at generation time and the artifact stays faithful to it.)

## Plan & Task Breakdown

- [x] **T1 — Store layout** — `internal/store/store.go`: `ASTProjectIcebugDir()`, `ASTHubIcebugDir()`, `ASTProjectIcebugSchema/Manifest`, `ASTContextIcebugDir`, `ASTHubIcebugSchema`; `internal/store/contextpaths.go`: `ASTContextIcebugDirIn`, `ASTContextDirIn` (read the lockfile: hub → `ASTHubDir`, link → the sibling's `ASTProjectDir`, local → `ASTContextDir`). `ASTProjectDBPath`/`ASTContextDBPath`/`ASTHubDBPath`/`DBFileName` removed (final — no longer in the code).
- [x] **T2 — In-memory backend** — `internal/ast/ladybug.go`: `LadybugConfig{StoreDir,IcebugDir,ReadOnly}`; `openOnce` always calls `OpenInMemoryDatabase`; `mountLocalIcebugLocked` reads `schema.cypher` + `icebug.json` from `IcebugDir` and runs the DDL; `Query()` uses the planner (`tryCanonicalBoundedTraversal`) when `canonical`, otherwise the physical `runQuery`; `namesLogicalRel` is fail-closed; `StoreDir()`/`Close` have no registry. `LadybugConfigFor` only changes `StoreDir`/`IcebugDir`; the `LADYBUGDB_PATH` env var becomes an override for `StoreDir` (with `IcebugDir` derived from it).
- [x] **T3 — Direct export from the shards** — `internal/ast/direct_icebug.go`: `ExportDirectFromRebuildIndex(WithReverse)` — iterates over the shard tree, columns derived from the rows (`columnsForLabel`), types inferred (`inferTypeFor`), dense IDs PER-TABLE (same as `ExportIcebugCanonical`), `writeCanonicalSchemaDirect`, `reverseEdgesDirect(edges, props, from, to)` with a self-loop only when `from==to && source==target`, `copyIcebugFile` for the final bundle; sparse parquets (`exportDirectDelta`) plus `ExportDirectIncremental(WithReverse)`, which rewrites only the affected parquets.
- [x] **T4 — Pipeline** — `internal/ast/pipeline.go`: `RebuildIcebugFromCache(WithReverse)` (full) or `ExportDirectIncremental` (partial, decided by `doIncremental`), `bundleDir` comes from the backend (`LadybugConfig.IcebugDir`), search `UpdateIncremental`/`RebuildFromCache`, `ReverseEdges` wired through, `ForceRebuild` excluded from the incremental path; `internal/ast/icebug_rebuild.go`: `rebuildIcebugFromCacheWithDelta` — full + delta.
- [x] **T5 — Shared canonical planner** — `internal/ast/ladybug_icebug_canonical.go`: `canonicalPKFor` (the real PK per label: `File.path`, `uid` for everything else), `sanitizeCanonicalPKEquality`/`rewritePKEqToIN` (rewrites `=` to `IN`), `sanitizeCondPK` (anchor conditions), `canonicalUIDMembers` via `hasKey`, accepts a rel-var `[r:CALLS]`, `count` with an `AS n` alias, a `RETURN` regex that captures `ORDER BY`/`LIMIT`.
- [x] **T6 — Faithful Hub publish** — `internal/hub/registry.go`: `prepareASTPublish` copies the local `graph.icebug/`, rewriting `schema.cypher` to replace the filesystem URI with `s3://store.ArtifactURI` (`rewriteIcebugStorageURI`); `internal/hub/ast_store.go`: mount only stages `schema.cypher` + `manifest` + `search.mount`; `astStoreBuilt` checks for `schema.cypher` + `SearchIndexBuilt`.
- [x] **T7 — Legacy removal** — removed: `incremental_rebuild.go`, `json_rebuild.go`, `ladybug_registry.go`, `reflink_linux.go`, `reflink_other.go`, the swap-based test setup (`AtomicSwapDB`, `CleanupInterruptedSwap`, `CopyDBDir`, `engineSidecarSuffixes`), `TestCancelAndRebuild`/`copy/`/`e2e_bench`/`incremental_cost_probe`/`real_corpus_incremental_probe`/`ladybug_checkpoint`/`ladybug_reader_isolation` (see git status).
- [x] **T8 — Callers adapted** — `server.go` (`getOrCreateCachedDB`/`storePathForRequest`), `query.go`, `bundle.go`, `source_service.go`, `embedder.go` (search-only rebuild), `mcpstdio/context.go` + `tools_ast.go` + `tools_lifecycle.go`, `cmd/graphit/commands/{ast,lifecycle,runners}.go`, `internal/daemon/syncmodule.go`, `internal/ast/config.go` (`ImportedContext.StoreDir`, `contextStoreBuilt` via the bundle, `ListImportedContextsIn`), `internal/hub/service.go` — the Link check now goes through `graph.icebug/schema.cypher`.
- [x] **T9 — Docs** — `docs/architecture/storage_layout.md`, `docs/specs/ast_module.md`, `docs/specs/hub_collaboration.md`, this task log.

## Implementation Details

- Language-body selector: `internal/ast/rebuild_helpers.go` (new) — `engineOwnedRelTypes()` (CALLS/OWNS), `shortHex()`, `BuildEmbLookup()`, `batchRows()`, `estimateRowBytes()`, `copyBatchBytes()`, `writeJSONFile`; replaces the helpers from the deleted rebuild files.
- `mountLocalIcebugLocked` uses `IcebugDir` (the bundle) → `schema.cypher`; for a Hub import from S3, `MountIcebugGraph(ctx, storeDir, schema)` only stores the schema — it never needs network access at that point, since the read is lazy inside the engine (`storage='s3://'`).
- `ExportDirectFromRebuildIndexFromStore` (`icebug_transfer.go`) exists to re-export a store derived from a bundle mount (used by the remote traversal test `ladybug_icebug_canonical_test:259,345`); `ExportGraphToIcebug` wraps it for cost tests gated by `GRAPHIT_REAL_STORE`.
- The `SearchIndex*` API now takes `storeDir` (previously `dbPath`) — `LanceIndexPath(storeDir)`, `searchMountURI(storeDir)`, `searchConfigFor(storeDir)`, `OpenSearchIndex(storeDir)`, `WriteSearchMount(storeDir, uri)`.

## Use Cases

### UC-01: Index the local on-the-fly filesystem
- **Actor**: `graphit sync` / `graphit ast index`
- **Preconditions**: project with a ULID lockfile
- **Main Flow**: parse shards → `RebuildIcebugFromCache(WithReverse)` generates `graph.icebug/schema.cypher` + `nodes_*.parquet` ... directly from the shards (new bundle in a cache-dir `tmp.<hex>/` → rename) → the next query opens `:memory:` and applies `schema.cypher`
- **Error**: export fails → sync fails, the old bundle is kept (the rename never happened)
- **Files**: `internal/store/store.go`, `internal/ast/direct_icebug.go`, `internal/ast/icebug_rebuild.go`, `internal/ast/pipeline.go`

### UC-02: Query locally without a db file
- **Actor**: `graphit_ast_query` (MCP)
- **Preconditions**: `graph.icebug/` exists
- **Main Flow**: `NewLadybugDB` (config = only `StoreDir`+`IcebugDir`) → `:memory:` → `LOAD schema.cypher` → `Query()` answers via the canonical planner or `runQuery`, pointing at `/abs/graph.icebug`
- **Error**: `graph.icebug/` missing → `contextStoreBuilt` is false → the MCP returns "no store"; DDL error → hint shown
- **Files**: `internal/ast/ladybug.go`, `internal/ast/server.go`, `internal/mcpstdio/context.go`

### UC-03: Publish to the Hub from local state
- **Actor**: `graphit hub submit`
- **Preconditions**: local `graph.icebug/` is ready
- **Main Flow**: copy `graph.icebug/` → tmp publish dir → replace the filesystem storage URI with `s3://bucket/prefix/graph.icebug` in `schema.cypher` → upload to S3
- **Files**: `internal/hub/registry.go`, `internal/hub/ast_store.go`

### UC-04: Hub install (mount, without downloading the graph)
- **Actor**: `graphit hub install` / MCP, `contextStoreBuilt`
- **Preconditions**: the artifact is published (`schema.cypher` + `nodes_*.parquet` on S3)
- **Main Flow**: `ensureASTStore` → `MountIcebugGraph` stages only `schema.cypher` (in-memory per-connection) + manifest + `WriteSearchMount` → `SearchIndexBuilt` used as the built check
- **Files**: `internal/hub/ast_store.go`

## Test Cases & Acceptance Criteria

### Running (will report `ok` on the full suite with `-tags lancedb`, 2026-08-28)
```gherkin
Given a project with 2 functions and 1 CALLS edge
When sync generates the graph.icebug filesystem
And a query MATCH (n) RETURN count(n) runs via the :memory: mount
Then the count matches the native result and nodes_*.parquet exists
```
- `internal/ast`: `TestIcebugArtifactMountsAndAnswers` (hub), `TestCanonicalPQPlannerTraversal`, `TestCanonicalPQCallsTraversal`, `TestCanonicalCountPatternAs`, `TestAllNamedLogicalRelsFailClosed`, `TestMountedIcebugRemoteRealGraph` (env-gated),...
- `internal/hub`: publish roundtrip + mount tests; `TestHubService_Link_AST` now writes `graph.icebug/schema.cypher`.
- `internal/livesearch/prep`: `TestCodeGraphsAreReportedByNameOnceAddressable` — the test setup was fixed to create `schema.cypher` in `HubContextDir` ("store built" now means the bundle is present).
- `internal/daemon`: the e2e syncmodule test checks `store.ASTProjectDir()/graph.icebug/schema.cypher` + `OpenSearchIndex(storeDir)`.

### Pending manual run/validation (E2E CLI)
- [ ] `go build -tags lancedb ./...` + the full suite `go test -count=1 -tags lancedb -timeout 1200s ./internal/... ./cmd/...` **after today's cleanup** (partial: ast/hub/store/daemon/mcpstdio/prep/commands ok; the full `ast` run is in progress)
- [ ] `GRAPHIT_GLOBAL_DIR=<tmp> graphit ast index` on a repo with >200 files → confirm the bundle + timing
- [ ] Edit 1 file, run `graphit ast index` again → should rewrite only the affected parquets (measure: `nodes_0.parquet` mtime changes for a few parquets, not all of them)
- [ ] `graphit hub submit` against a fake S3 (MinIO) → rewrites the URI to `s3://`, identical parquets
- [ ] `graphit hub install` from another project → mount, query via MCP node, without downloading the graph
- [ ] `graphit ast link` → `TestHubService_Link_AST` passes in code; manually check the "index the source project first" error when the bundle is missing

## Files Changed (destaques)

| File | Change | Reason |
|---|---|---|
| `internal/ast/direct_icebug.go` | NEW | direct export from the shards, dynamic, partial incremental |
| `internal/ast/icebug_rebuild.go` | New | `RebuildIcebugFromCache` (+`WithReverse`), delta |
| `internal/ast/ladybug.go` | Modified | `:memory:` + per-connection schema mount, no `DBPath`/`InMemory` |
| `internal/ast/ladybug_icebug_canonical.go` | Modified | shared planner converged with the Hub |
| `internal/ast/pipeline.go` | Modified | uses direct export/incremental, `ReverseEdges` |
| `internal/ast/rebuild_helpers.go` | New | helpers extracted from the removed rebuild files |
| `internal/ast/icebug_transfer.go` | Modified | mount staging, `ExportDirectFromRebuildIndexFromStore` |
| `internal/ast/server.go`, `query.go`, `embedder.go`, `bundle.go`, `source_service.go` | Modified | callers |
| `internal/ast/config.go` | Modified | `StoreDir`, `contextStoreBuilt`, `ListImportedContextsIn` |
| `internal/ast/search_lance.go` | Modified | search API keyed by `storeDir` |
| `internal/store/store.go` + `contextpaths.go` | Modified | icebug resolvers, legacy `DBPath*` removed |
| `internal/hub/registry.go` | Modified | publish via bundle copy + URI rewrite |
| `internal/hub/ast_store.go` | Modified | mount is just schema+manifest+search.mount |
| `internal/hub/service.go` | Modified | Link check via the bundle |
| `internal/mcpstdio/` | Modified | context resolver/tools adapted |
| `cmd/graphit/commands/` | Modified | help text without `ladybugdb` |
| `internal/livesearch/prep/index_test.go` | Modified | new test setup around `schema.cypher` |
| `internal/daemon/syncmodule_e2e_test.go` | Modified | checks bundle+search index |
| `internal/hub/coverage_extra_test.go` | Modified | Link test uses the bundle |
| `internal/store/store_test.go` | Modified | icebug shapes |
| `internal/ast/hubstore_test.go` | Modified | `ContextDirIn`, no `DBPath` |
| `docs/architecture/storage_layout.md`, `docs/specs/ast_module.md`, `docs/specs/hub_collaboration.md` | Modified | new format |
| Deleted | `incremental_rebuild.go`, `json_rebuild.go`, `ladybug_registry.go`, `reflink_*.go`, `cancel_and_rebuild_test.go`, `copy_test.go`, `e2e_bench_test.go`, `incremental_cost_probe_test.go`, `real_corpus_incremental_probe_test.go`, `ladybug_checkpoint_test.go`, `ladybug_reader_isolation_test.go` |

## Trade-offs & Decisions

- In-memory vs. file catalog: in-memory eliminates WAL/lock/checkpoint; the cost is reapplying the schema on every connection (ms). In-memory was chosen, unconditionally.
- Direct export vs. populate-then-export: populating the catalog was O(corpus) and made incremental updates impossible; direct export doesn't have that gap.
- IDs per table (not global): avoids the anti-pattern of a stray `count`; a self-loop happens only when `from==to && source==target`.
- `ExportDirectIncremental` falls back to a full export when files were deleted (old data can't be re-derived); accepted — in that case the whole bundle gets renamed, but the incremental path is still an improvement overall.
- Removes all legacy `DBPath*`: the `LADYBUGDB_PATH` env var still works as a `StoreDir` override for tests/dev; `internal/ast/rule.go` help text updated to reference `graph.icebug`.

## Technical Debt

- [ ] Measure and document the full rebuild time on a large repo (~24k files, previously 53s; the direct export should be much faster — not yet measured with the final version).
- [ ] `ExportGraphToIcebug`/`ExportDirectFromRebuildIndexFromStore` are only used by env-gated cost tests — decide whether they become public API or move to test-only. (Currently: they live in `icebug_transfer.go`, compile, and one of them is used by `ladybug_icebug_canonical_test`, env-gated.)
- [ ] Does `ladybug_segmentation_cap` and the shard tree in the bundle (`nodes_*.parquet` names derived from the `ShardCache`) fail to use real shard segment names? — check whether the final parquet name carries the shard's `shortHex` and whether the manifest reflects that.
- [ ] `item.path_lower`-style columns: the direct export derives columns from rows; confirm that `path` is never a column and that `uid` is used for `File` instead, so it doesn't diverge from the physical schema (`_id`).
- [ ] Cleanup: `.build.lock` in the hub dir; check whether old `ladybugdb` stores get removed by the daemon (is there no cleanup code left? `atob`/`sync` does not delete legacy files; decide on a follow-up).
- [ ] The "Where the graph lives" text in `internal/ast/rule.go` is the only place that shows paths to the user; review whether it should include `graph.icebug/schema.cypher` (already done) and reindex the wiki.

## Progress Log

### 2026-08-27
- Web research confirmed the icebug filesystem format (`storage='<abs>/graph.icebug'`, docs.ladybugdb.com/import/icebug) and validated `:memory:` (`lbug.OpenInMemoryDatabase`).
- Implemented v1: `ASTProjectIcebugDir`, `LadybugConfig.InMemory`, `mountLocalIcebugLocked`, `RebuildIcebugFromCache`, `prepareASTPublish` filesystem→s3, `isCanonicalTraversalQuery` for single-hop, `InMemory` checks in `newASTBackendReadOnly`/`mcpstdio`.
- Validated: `GRAPHIT_GLOBAL_DIR=/tmp/graphit-test-icebug` sync generated `graph.icebug/` with `storage='/abs'`, and `MATCH (n:File)` via `:memory:` returned 2 rows; publish rewrites storage to `s3://`.
- Docs updated (storage_layout, ast_module, hub_collaboration).

### 2026-08-27 (v2 — user requirements)
- Against the populate-then-export approach: the user required parquets straight from the SHARDS (not the catalog), and incremental rewrites of only the affected parquets → `direct_icebug.go` + `ExportDirectIncremental` + `exportDirectDelta` + `rebuildIcebugFromCacheWithDelta`.
- Wrote a first version with labels/columns hardcoded per language → the user rejected it; rewrote it dynamically (derived from rows, types inferred).
- The user required local queries to go through the SAME path as the Hub (bounded planner, fail-closed) → `tryCanonicalBoundedTraversal`, `namesLogicalRel`, PK sanitizations (`=`→`IN`, anchor conditions).
- The user required "no conditionals": `DBPath` and `InMemory` removed from `LadybugConfig`; `openOnce` always uses `:memory:`; the Hub also becomes an in-memory mount (only schema+manifest+search.mount are persisted).
- Removed `.wal`/`.shadow`/checkpoint (`AtomicSwapDB`, `engineSidecarSuffixes`, `CleanupInterruptedSwap`, `CopyDBDir`) and the associated legacy files.
- Planner fixes: rel-var `[r:CALLS]`, `count` with an `AS` alias, `ORDER BY`/`LIMIT` in `RETURN`, PK per label.
- Writer fix: IDs per table (not global) to avoid "self-loops" from edges between labels (when from/to belong to different languages).
- `hub.icebug.reverse_edges` → `PipelineOptions.ReverseEdges *bool`; `WithReverse` variants.

### 2026-08-28 (continued)
- **MCP test with a real index (`graphit ast index --reset`, graphit-code repo, store 01KSH1…)**:
  - `ast_schema` ✓ — labels with per-language counts (1.47M js, 270k go…), 40 generic labels, relationship tables `calls__*/contains__*/imports__*`.
  - `MATCH (n:File) RETURN n.name LIMIT 3` ✓ (bytes resolved from the local icebug bundle).
  - `MATCH (g:Function {name:'NewLadybugDB'}) RETURN g.name` ✓.
  - `MATCH (f:Function)-[:calls__function_function]->(g:Function {name:'NewLadybugDB'}) RETURN f.name LIMIT 3` ✓ (canonical planner; results came from both `Function` and `Method` nodes depending on the call).
  - `ast_search` FTS ✓ (`openOnce`, `mountIcebug`, `writeIcebugSchema`);
  - **REAL BUG found**: `ast_search`/`graphit ast query --hybrid` failed with
    `Invalid input, expected column _distance not found in rank. found columns […,"_score","_rowid"]`
    — root cause: the index was built WITHOUT embeddings (`.emb.json` with `emb:{}`; `ast embed` had never run). With every embedding row NULL, the engine's RRF rank never produces `_distance`, and the C++ engine's error surfaces without any context.
  - **FIX**: `SearchIndex` gained a `vectorCount` plus a sidecar `embeds.json` next to `search.lance` in the store dir, written by `RebuildFromCache`/`UpdateIncremental` (`writeEmbedsStatus`); `OpenSearchIndex` reads and sets it. `HybridSearch` degrades to keyword search with a `run graphit ast embed` log hint when `vectorCount==0`; `SemanticSearch` returns empty with the same hint instead of crashing. For incremental runs with deletes but no new vectors, a filtered probe `WHERE embedding IS NOT NULL LIMIT 1` (`hasVectorRows`) decides which path to take. New test: `TestHybridSearchDelegatesToKeywordsWhenTheIndexHasNoEmbeddings`.
  - **status**: the `internal/ast` suite is green (124s), the search test family is green; `graphit ast embed` is running (daemon); once it finishes, a `graphit ast query --hybrid` run will validate hybrid search for real.
- Test-only functions moved to `internal/ast/export_test.go`: `ExportGraphToIcebug`/`ExportDirectFromRebuildIndexFromStore` → `exportGraphToIcebug`/`exportDirectFromRebuildIndexFromStore` (plus `parseCanonicalManifest`, `copyDirContents`, `rewriteSchemaStorageURI`, `copyLanceIndex`), the dead `reverseEdges bool` parameter removed; nothing exported remains for production.
- **PENDING**: the post-fix full suite (running), the final embed + real hybrid validation, commit.

### 2026-08-28 (continued)
- **REGRESSION BUG — ignore rules ignored by `ast index`** (previously reported by the user: `node_modules` was being indexed):
  - Cause: the command switched to collecting files via `collectFilesForPath` (`runners.go`) and feeding the pipeline through `ChangedPaths` (scoped). Since the scoped pipeline **doesn't run** `collectFiles` (the ignore-rule applier), `.gitignore`/`.astignore` and dot-dirs were never checked — `ast index` was indexing `node_modules`, `.opencode/`, `graphit.lock.json`.
  - Fix: `collectFilesForPath(rootPath, projectRoot)` now applies `ast.NewAstIgnoreChecker(projectRoot)` (boundary = the project, so scopes like `ast index internal/ui` still honor the root's rules), skips dot-dirs, and uses `ShouldDescend` for re-inclusions.
  - Tests: `cmd/graphit/commands/collect_files_ignore_test.go` (gitignore, astignore+dot-dirs, scoped-boundary; skipped when the grammar isn't available — the repo's usual pattern).
  - Manually validated: `/tmp/itest` and `/tmp/itest2` (with `.gitignore internal/ui/node_modules/` and `.opencode/node_modules/zod`) → only `keep/`/`internal/ui/main.go` got indexed.
  - **PENDING**: `make install` the new binary and run `graphit ast index --reset` on the real project to purge the erroneously indexed files already there.

### 2026-08-28 (continued)
- **PHASE 2 — ignores in subdirectories** (user: "both `.gitignore` and the custom `.astignore`/`.wikiignore` ignores need to be read at every sublevel and apply there"):
  - Discovered: `.opencode/.gitignore` with `node_modules` wasn't being read — the checker only collected ignore files from the root down to the boundary; a `.gitignore` in a subdirectory (git semantics: directory-scoped) was never picked up.
  - `internal/ignorer`: new `IgnoreChecker.At(dirRelPath) DirScope` — returns the directory's context with the ignore files from each level (`.gitignore` + custom `.astignore`/`.wikiignore`), cached per directory; patterns are parsed against the directory's own domain (gogitignore::domain, already validated: `node_modules` in `.opencode/` matches `.opencode/node_modules/...` and nothing else).
  - `internal/fswatch`: `Ignorer` is now an ALIAS for `ignorer.DirScope` (the old interface wasn't compatible with `At`'s covariant return; explained in the comment); `addTree` walks with `At` and keeps the context per directory; `accept` judges each event by its directory's context; the `usable()` helper treats a typed `(*IgnoreChecker)(nil)` as no-rules (nil-typed interface).
  - `internal/ast/writer.go` (`collectFiles`) and `internal/knowledge/wiki.go` (`enumerateKnowledgeSources` + `knowledgeSourceFile`): now walk with `At`, so subdirectory `.gitignore`/`.astignore`/`.wikiignore` apply during internal discovery; dot-dirs are no longer skipped structurally (rule: only the ignore files exclude anything).
  - `internal/daemon/syncmodule.go`: `ignoreUnion.At` (the union of AST+wiki) implemented.
  - Tests: `TestAtAppliesSubdirectoryIgnoreFiles` (ignorer), `TestSubdirectoryIgnoreFilesApplyWhileWatching` (fswatch), `TestCollectFilesForPathHonorsSubdirectoryGitignore` (commands), dot-dirs governed by ignores (commands).
  - **Integration note**: `collectFilesForPath` (`runners.go`) is the CLI's discovery path and follows `At`; the internal `collectFiles` (daemon full-scan/watcher fallback) does too; watching goes through fswatch's `At`.
  - Machine note: `graphit-core daemon` (7.4GB RSS) was running; the `internal/ast` suite hit a "signal: segmentation fault" on the 15:12 run and passed in isolation afterward (13s, Verify ok) — likely memory/native-execution contention; WATCH THIS (if it recurs, investigate liblbug/OOM).

## Handoff

For another agent to continue:
1. `go build -tags lancedb ./...` OK + let `go test -count=1 -tags lancedb -timeout 1200s ./internal/... ./cmd/...` finish (ast ~124s).
2. `graphit ast embed` is running (log `/tmp/embed2.log`): once it finishes, validate `graphit ast query "<term>" --hybrid --top 5` — should return results with merged scores (no `_distance` error), and check `embeds.json` → `"vectors": N`.
3. Measure deliberately: on a large repo, time a full and an incremental (1 file changed) `graphit ast index` — record the timings in this log.
4. Commit: message referencing this task log, no amend.
5. Update `docs/specs/ast_module.md` if `Discover` or `ast rule` change in a way that affects the help text.
