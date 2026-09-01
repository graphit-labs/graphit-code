# Task: The package spec parameter belongs to the subroutine, and a failed copy no longer publishes an incomplete graph

**Date:** 2026-08-03 → 2026-08-04
**Status:** ✅ Complete

## Problem

Reported from a concrete question about the project's test corpus — a private Oracle corpus of 36,823 files: *why is parameter `P_LOG_TX` linked to package `PCK_EXEMPLO` instead of to procedure `ATLZ_EXEMPLO`?*

The investigation found **two independent bugs**, plus **two gaps of the same family in other grammars**.

### 1. `procedure_spec` / `function_spec` missing from `context_types` (PL/SQL)

`internal/ast/queries/plsql.yaml` declares Procedure and Function from three forms each:

| form | where it appears | was it in `context_types`? |
|---|---|---|
| `create_procedure_body` | standalone `CREATE PROCEDURE` | yes |
| `procedure_body` | body inside `CREATE PACKAGE BODY` | yes |
| `procedure_spec` | **declaration inside `CREATE PACKAGE`** (spec) | **no** |

`resolveParentContextAntlr` (`internal/ast/antlr_adapter.go:598`) walks up the `match.Context.Outer` chain and returns the **first** ancestor whose rule is in `context_types`. In a package spec, the chain for a parameter is `parameter → procedure_spec → create_package`; since `procedure_spec` was not a context, the walk skipped over it and stopped at `create_package`.

Measured in the corpus graph before the fix — Parameter by `context_type`:

| owner | parameters |
|---|---|
| Procedure | 13,542 |
| Function | 10,558 |
| **Package** | **9,052** |

And 100% of the 9,052 `Package` cases are under `schema/packages/` (zero outside it), which closes the diagnosis: it is exactly the set of specs. Beyond assigning the wrong owner, this **collides uids**: two subprograms of the same package sharing a parameter name (`E_TX_ERRO` is the typical case in this corpus) produced the same `path::PACKAGE.E_TX_ERRO`.

Making it worse: `ConvertToCache` **discards** parameters and fields with an empty context — this is the loss of 967 out of 2,732 entities documented in `internal/ast/oracle_parameter_loss_test.go`. A callable that is not a context does not just file the parameter under the wrong owner: it can cost the parameter entirely.

### 2. A failed COPY was logged and ignored, and the rebuild swapped the database anyway

In the same project, `MATCH (f:File) RETURN count(f)` returned **0**, with `Directory` = 17 and all entities present. Symptoms:

- `graphit ast source` / `graphit_ast_source` failed with `source not found for path` for **any** path in the project (verified against `schema/packages/PCK_EXEMPLO.sql` and `AGENTS.md`) — it wasn't a specific path, it was the entire project;
- `MATCH (f:File {path:...})-[:CONTAINS]->(e)` returned zero rows **silently**;
- the `CONTAINS File→entity` and `Directory→File` edges (`internal/ast/json_rebuild.go`) simply did not exist.

Cause: in `RebuildFromJSON`, the `copyNode` helper only logged the error and incremented `copyErrors`; nothing checked `copyErrors` before `lb.AtomicSwapDB(tempDBPath)`. The rebuild kept going, ran enrichment, and **published** the half-loaded database. The error log goes to the rebuild's slog, not to `.graphit/daemon/daemon.log` — confirmed: no COPY/error line appeared in either the global or the project daemon logs.

Why the File COPY is the one that blows up: `ri.fileNodeJSON` builds **one** in-memory slice with `"source": fe.entry.Source` for every file, writes it to a single JSON, and loads it with a single `COPY File(...) FROM '<file>'`. In this corpus that is 36,823 files / 2.4 GB of shards, with individual shards of up to 133 MB (a single XML file).

The source was **not** lost during extraction: the shard `schema/packages/PCK_EXEMPLO.sql.nodes.json` has the keys `v,h,lang,src,file_row,dir_paths,entities` with `src` intact. The loss happens only at load time.

Note: the **incremental** path already handled this correctly — `internal/ast/incremental_rebuild.go` falls back to a full rebuild when `insertErrors > 0`, with a comment explaining exactly this class of bug. It was the full rebuild that still needed to be closed.

### 3. `html.yaml` declared contexts but no `context_name_paths`

Direct continuation of [Entities know their parent](../changelogs/20260802_entities_know_their_parent_and_ascent_is_memoized.md), which added `context_name_paths` to `xml`, `json`, `yaml_lang`, and `svelte`. `html.yaml` was left out: it declares `element`, `script_element`, and `style_element` as contexts, but tree-sitter-html's `element` does not expose a `name` field — the name lives in the `start_tag`. Without the path, `nameNodeOf` returns nil, the container becomes **transparent**, and every HTML entity falls back onto File — precisely the silent failure that piece of work existed to kill.

### 4. Other grammars: callables that were not contexts

An audit of all 44 grammars cross-referencing "rule that the query turns into a callable" against `context_types`. Real gaps found beyond PL/SQL:

| grammar | rule | effect |
|---|---|---|
| `dart` | `function_signature`, `method_signature` | Dart declares the signature as its own node; **every** method parameter went to the class |
| `objc` | `function_definition`, `method_declaration`, `method_definition` | same issue: parameters went to the class/protocol |
| `java` | `constructor_declaration` | constructor parameters went to the class |
| `javascript`, `typescript`, `tsx` | `generator_function_declaration` | `function* foo()` is its own kind, not `function_declaration` |

## Solution

### Grammar `context_types`

`internal/ast/queries/plsql.yaml` — added `function_spec: Function`, `procedure_spec: Procedure`, `create_materialized_view: MaterializedView`, and the family of package-level type declarations (`record_type_definition`, `table_type_definition`, `varray_type_definition`, `ref_cursor_type_definition`, `subtype_declaration` → `Type`), with a comment in the file explaining the invariant: *every rule that a query turns into a container must be listed here*.

Also `java.yaml` (`constructor_declaration`), `javascript.yaml` / `typescript.yaml` / `tsx.yaml` (`generator_function_declaration`), `dart.yaml` (`function_signature`, `method_signature`), `objc.yaml` (3 rules), `svelte.yaml` (`script_element`, `style_element`, for parity with `html.yaml`), and `html.yaml` (`context_name_paths` for the three contexts).

### The rebuild no longer publishes an incomplete graph

`internal/ast/json_rebuild.go`:

- `copyErrors > 0` now **aborts**: it closes the temporary backend and returns an error. The previous database stays in place, and the temporary one is removed by the defer (because `swapped` stays `false`). Keeping the old database is strictly better than publishing an incomplete one.
- `copyNode` now iterates over `batchRows(data, copyBatchBytes)` — 64 MiB per COPY. The new `batchRows` and `estimateRowBytes` functions measure **bytes**, not rows: sizes here vary across six orders of magnitude, so a row count would not express the limit. A row larger than the budget gets its own batch — the goal is to bound the document, not reject content.

`internal/ast/incremental_rebuild.go` — `insertNodes` batches by the same criterion, so that one large file does not force an unnecessary full rebuild.

### File text moves out of the graph: the search index is the sole owner

The source was stored **three times**, through different code paths:

| copy | where | written by |
|---|---|---|
| parse cache shard | `<DB dir>/shards/<path>.nodes.json`, field `src` | parse pipeline |
| graph property | `File.source` inside `ladybugdb` | `COPY File` in `RebuildFromJSON` |
| search index column | `file_fts.source` in `ladybugdb.search.sqlite` | `fts_sqlite.go` |

`file_fts` declares `source` as an **indexed** column (`fts_sqlite.go:95`, unlike `name`, which is `UNINDEXED`), with a BM25 weight of 1.0 in `bm25(2.0, 0.0, 8.0, 1.0)`, and `queryFTS("file_fts", ...)` uses table-level `MATCH` — so the source is genuinely searchable, and FTS5 keeps the original text retrievable via `SELECT`. Verified on the real corpus: 36,823 rows in `file_fts`, and `schema/packages/PCK_EXEMPLO.sql` with 12,973 bytes of source — in the **same** database whose graph has zero File nodes.

Of the three, only the search-index copy is actually queryable — the other two were dead weight that could still drift apart. The graph copy was the most expensive: it was the one forcing the entire repository through a single `COPY`, and it was the one that failed. So **the text moves out of the graph**:

- `internal/ast/rebuild_index.go` — `fileNodeJSON` no longer emits `"source"`.
- `internal/ast/json_rebuild.go` — `fileCols` loses the column, and with it the 2.4 GB `COPY` that caused the incident disappears. The incremental path adjusts itself: `insertNodes` derives its keys from `data[0]`.
- `internal/ast/ladybug.go` — the `source` **column** survives in the File table DDL, and for one reason only: the synthetic `'__config__'` node, where `RunEnrichment` stores the project's detected configuration (`enrichment.go:413`) and which the skill documents querying (`MATCH (c:File {path: '__config__'}) RETURN c.source AS configs`). For a real file, it stays empty.
- `internal/ast/source_service.go` — `SourceService` no longer queries the graph for text: `WithStore(dbPath)` and a single `FileSourceAt`.
- `internal/ast/server.go` — the file-content HTTP endpoint likewise, via `storePathFor(context)`.
- `internal/ast/rule.go` — the skill's property table loses `source` for **File**, and the "Quick source peek" section — which used to teach `RETURN file.source` — now states that text is not a graph property: the query gives the location, the `ast source` tool gives the text.

New function `FileSourceAt` in `internal/ast/fts_sqlite.go`: a single `SELECT source FROM file_fts WHERE path = ?`, opening sqlite with `mode=ro`. It **deliberately does not** use `OpenSearchIndex` — that path calls `migrateSearchSchema`, which does `DROP TABLE IF EXISTS file_fts` when the schema version differs (`fts_sqlite.go:74`). Reading a source must never destroy the index it reads from; there is a test for this.

### Imported contexts become searchable

With text living only in the index, a context without one isn't a degraded context — it's unusable. And that was exactly the case for the Hub install: `internal/hub/service.go` (`case TypeAST`) called `RebuildFromJSON` and stopped there, never opening a `SearchIndex`. The context stayed navigable via Cypher but was neither searchable nor readable, and the shards it was built from remain in the Hub clone, not next to the store — so nothing else could serve the text.

New function `ast.BuildSearchIndexFor(dbPath, cache, embCache)` wraps `OpenSearchIndex` + `RebuildFromCache` + `BuildEmbLookup`, and the Hub install now calls it, failing the install if it fails. All three context types become equivalent:

| source | graph | search index |
|---|---|---|
| own project | pipeline | pipeline (`pipeline.go:544`) |
| `ast_install` from a local path | pipeline with `CacheDir: filepath.Dir(ictx.DBPath)` | pipeline |
| Hub artifact | `RebuildFromJSON` | **`BuildSearchIndexFor`** (new) |

Context resolution already worked on both sides and didn't need to change: `NewQueryService` derives `lb.cfg.DBPath + searchIndexSuffix` (`query.go:36-38`), and `openASTDB(projectDir, context)` returns the context's backend. All that was missing was for the file to exist.

### `ast.index_source: false` no longer degrades semantic search

A different flag from the previous one, and the problem is about signal, not surface. With `ast.index_source` false — or `--no-source` / `no_source` on the index command — the text is not persisted anywhere: `antlr_adapter.go:272` and `treesitter_adapter.go:309` don't even materialize `result.Source`, and `ConvertToCache` writes an empty `src` to the shard. Not having FTS over the source is an accepted consequence: `entity_fts` keeps indexing name, `name_split`, and docstring, so search by name keeps working.

What was **not** acceptable is what the embedding lost. `scanPending` built the snippet with `embedSourceSnippet(entry.Source, ...)` — the **persisted** source — which under the flag is empty. The embedded text ended up reduced to label, path, context, name, and docstring: exactly missing the part that describes what an entity **does** rather than what it's called. The flag was silently degrading semantic search.

The distinction that fixes it: the flag says *don't keep a copy of the source*, not *don't look at the source*. An embedding is a vector, not retrievable text — it can be computed from the file and persisted while the text itself never is.

- `EmbeddingConfig.RepoRoot` — the indexed tree, read **only** when the parse cache has no text for that file. Empty turns off the read, and the embedding falls back to name, docstring, and context instead of failing.
- `Embedder.sourceFromDisk(relPath, shardHash)` — **SAFETY**: requires the file's `fileContentHash` to match the shard's hash. The embedding cache is keyed on that hash, so embedding newer text under the old key would store a vector describing code the graph doesn't contain, and it would survive until the file changed again. A mismatch ⇒ no snippet, which is better than the wrong snippet.
- Lazy resolution per shard: a cache that **has** text never touches disk, so the read is a path taken only under the flag, not a new cost on the default path. At most one read per shard, in the same streaming fashion as the scan.
- `RunEmbeddingLoop` gained a `repoRoot` parameter, propagated through the daemon's embedding module (which already had `rootPath`). `RepoRoot` is also wired into `ast embed`, the sync path, and the MCP tool — the latter resolving `ListImportedContexts()[ctx].SourcePath` for an imported context, and leaving it empty for a Hub artifact, which has no local tree.

### `--no-sources` finally actually exists

The flag was declared on two surfaces — `--no-sources` in the CLI (`ast.go:289`) and `no_sources` in the MCP tool — and it was **accepted and discarded**: `runASTExport` received the parameter and never read it, and the success message said "(with sources)" regardless. It was also unimplementable as written, because it described omitting a node property where the text no longer lives.

With the text coming from the index, including it is a real action and omitting it is a real choice:

- `internal/ast/fts_sqlite.go` — `EachFileSource(indexPath, fn)` walks `file_fts` row by row. **Streaming, not collecting**: the entire corpus sits on the other side of the cursor (2.4 GB for 36k files), and whatever writes to a zip needs one file at a time. Rows with an empty `source` are skipped.
- `internal/ast/bundle.go` — `ExportBundle` gains `BundleOptions{StorePath, NoSources}`, and writes each file as a `sources/<path>` member of the zip instead of one giant map: the extracted bundle becomes a tree, and reading one file doesn't require parsing all of them. A path that would escape the archive root is skipped. The manifest gains `source_count`, to distinguish a structural bundle from a truncated one without guesswork.
- Wired into all three call sites: the CLI, the MCP tool, and `POST /api/export/bundle` (which gained `no_sources` in its body).

Requesting sources and failing to get them is an **error**, not a silent structural export — a bundle nobody can use must not look like a success. Without `StorePath`, the bundle is structural by definition, and the manifest says so.

Error messages now distinguish the two cases, which is useful for diagnosis:

| message | meaning |
|---|---|
| `source not found for path %q (no File node in the graph; ...)` | no File node exists — incomplete index |
| `file source is empty: %s (indexed without source, see ast.index_source)` | File node exists without source — configuration |

Wired in `internal/mcpstdio/tools_ast.go` and `cmd/graphit/commands/runners.go` via `WithShardCache(filepath.Dir(cfg.DBPath))`.

## Use Cases

### UC-01: Index a PL/SQL package spec and navigate a subprogram's signature
- **Actor**: Agent or developer querying the graph.
- **Preconditions**: Project indexed with `ast.grammar .sql=antlr-plsql`; a file containing `CREATE PACKAGE ... AS` with `PROCEDURE`/`FUNCTION` declared in the spec.
- **Main Flow**:
  1. The indexer matches `//procedure_spec/identifier` and creates the Procedure entity.
  2. It matches `//parameter/parameter_name` for each parameter of the signature.
  3. `resolveParentContextAntlr` walks up from `parameter` and finds `procedure_spec`, now in `context_types`, returning `(subprogram name, "Procedure")`.
  4. The Parameter entity is written with `context` = the subprogram, `context_type` = `Procedure`, and the uid becomes `path::PACKAGE.SUBPROGRAM.PARAM`.
  5. `MATCH (p:Procedure {name:'X'})-[:CONTAINS]->(a:Parameter)` returns the signature.
- **Alternative Flows**:
  - In `CREATE PACKAGE BODY`, the context found is `procedure_body`/`function_body` — unchanged behavior.
  - In a standalone `CREATE PROCEDURE`, it is `create_procedure_body` — unchanged.
  - Types declared in the package (`TYPE ... IS RECORD`) are now the context for their own fields instead of leaving them on the package.
- **Error Scenarios**:
  - A container rule missing from `context_types` → parameter assigned to the wrong ancestor or discarded by `ConvertToCache`. Caught by the `TestEveryCallableContainerIsDeclaredAsAContext` test.
- **Postconditions**: No spec Parameter has `context_type = "Package"`; uids of same-named parameters in different subprograms no longer collide.
- **Affected Files**: `internal/ast/queries/plsql.yaml`, `internal/ast/antlr_adapter.go`.

### UC-02: A full rebuild with a COPY failure does not publish the graph
- **Actor**: `graphit ast index`, or the daemon via sync.
- **Preconditions**: Parse cache populated; a previous database may or may not exist.
- **Main Flow**:
  1. `RebuildFromJSON` creates the temporary database and initializes the schema.
  2. `copyNode` splits each table into batches of up to 64 MiB and runs one COPY per batch.
  3. No error → enrichment → `AtomicSwapDB` → `swapped = true`.
- **Alternative Flows**:
  - Small table → a single batch, zero cost relative to the previous behavior.
  - A row larger than the budget → gets its own batch, nothing is discarded.
- **Error Scenarios**:
  - Any COPY fails → `copyErrors > 0` → temporary backend closed, error returned, **previous database preserved**, temporary one removed by the defer.
  - On the incremental path, `insertErrors > 0` still falls back to a full rebuild — which now fails explicitly instead of publishing an incomplete graph.
- **Postconditions**: The published database is either complete, or it was not published at all.
- **Affected Files**: `internal/ast/json_rebuild.go`, `internal/ast/incremental_rebuild.go`.

### UC-03: Read the text of an indexed file
- **Actor**: Agent via `graphit_ast_source`, user via `graphit ast source`, or the file-content HTTP endpoint.
- **Preconditions**: File indexed with `ast.index_source` enabled; search index at `<DBPath>.search.sqlite`.
- **Main Flow**:
  1. The call site resolves the store — the project's or a context's — and calls `WithStore(dbPath)`.
  2. `fetchFileSource` calls `FileSourceAt`, which opens the index in `mode=ro` and runs `SELECT source FROM file_fts WHERE path = ?`.
  3. The text is returned and sliced according to `head`/`tail`/`entity`/`pattern`.
- **Alternative Flows**:
  - `entity` given → the **location** comes from the graph (`line_number`/`end_line`) and the text from the index; they are separate responsibilities by design.
  - Imported context → same path, with that context's `DBPath`.
- **Error Scenarios**:
  - Service without a store → error stating there's no way to reach the index, not that the file doesn't exist.
  - Path missing from the index → error citing both possible causes: not indexed, or `ast.index_source` set to false.
  - Outdated index schema version → the read still works and does **not** migrate or drop `file_fts`.
- **Postconditions**: The text comes from a single store; the index stays intact after the read; the graph is never queried for text.
- **Affected Files**: `internal/ast/source_service.go`, `internal/ast/fts_sqlite.go`, `internal/ast/server.go`, `internal/mcpstdio/tools_ast.go`, `cmd/graphit/commands/runners.go`.

### UC-04: Install an AST context from the Hub and search it
- **Actor**: User or agent via `graphit_hub_install` of an `ast` artifact.
- **Preconditions**: Hub artifact carrying parse cache shards.
- **Main Flow**:
  1. `internal/hub/service.go` (`case TypeAST`) loads the shard cache from the clone.
  2. `CreateGraphSchema` + `RebuildFromJSON` build the graph at `~/.graphit/ast/<project-id>/ladybugdb`.
  3. `ast.BuildSearchIndexFor(dbPath, shardCache, embCache)` builds `ladybugdb.search.sqlite` from the same shards.
  4. `ast_search` and `ast_source` with `context: "<name>"` resolve that store and respond.
- **Alternative Flows**:
  - Empty shard cache (`Count() == 0`) → nothing is built, as before.
- **Error Scenarios**:
  - Failure to build the index → the install **fails** with an error, instead of leaving a context that's navigable but not searchable.
- **Postconditions**: All three context origins (project, local `ast_install`, Hub) have both a graph and a search index.
- **Affected Files**: `internal/hub/service.go`, `internal/ast/fts_sqlite.go`.

### UC-05: Export a bundle with or without file text
- **Actor**: User via `graphit ast export --format bundle`, agent via the export MCP tool, or `POST /api/export/bundle`.
- **Preconditions**: Project indexed; search index present when the text is wanted.
- **Main Flow**:
  1. The call site builds `BundleOptions{StorePath, NoSources}`.
  2. `ExportBundle` collects nodes and edges from the graph.
  3. With `NoSources` false and `StorePath` set, `writeBundleSources` walks the index via `EachFileSource` and streams a `sources/<path>` member per file.
  4. The manifest records `node_count`, `edge_count`, and `source_count`.
- **Alternative Flows**:
  - `--no-sources` / `no_sources` → step 3 is skipped; `source_count` = 0.
  - No `StorePath` → structural bundle by definition.
- **Error Scenarios**:
  - Sources requested but the index is missing or unreadable → **error**, naming the sources step; nothing is reported as a successful export.
  - An index path that would escape the zip root → skipped, not written.
- **Postconditions**: The bundle's content matches what the flag requested, and the manifest describes what it carries.
- **Affected Files**: `internal/ast/bundle.go`, `internal/ast/fts_sqlite.go`, `internal/ast/server.go`, `internal/mcpstdio/tools_ast.go`, `cmd/graphit/commands/runners.go`.

### UC-06: Index without persisting the source and still have semantic search
- **Actor**: User with `ast.index_source: false` in the config, or `--no-source` on the index command.
- **Preconditions**: Indexed tree present on disk; `RepoRoot` configured on the embedder.
- **Main Flow**:
  1. Parsing doesn't materialize `result.Source`, and `ConvertToCache` writes an empty `src` to the shard.
  2. `scanPending` finds `entry.Source` empty and calls `sourceFromDisk(relPath, hash)`.
  3. The file's hash is compared to the shard's; if they match, the file is read.
  4. The snippet is sliced to the entity's line range and capped by `MaxSourceChars`; the vector is computed and persisted in the embedding cache.
- **Alternative Flows**:
  - Shard **with** text → uses the cached text, without touching disk.
  - Empty `RepoRoot` (a Hub artifact, for example) → no snippet; the entity is still embeddable by name, docstring, and context.
  - FTS over the source doesn't exist under this flag, by definition; `entity_fts` keeps indexing name, `name_split`, and docstring.
- **Error Scenarios**:
  - File hash different from the shard hash → no snippet, so the cache doesn't end up storing a vector for code the graph doesn't contain.
  - File unreadable or missing → no snippet, no error: the shard will be reprocessed.
- **Postconditions**: A vector exists for the entity including the body's signal, and no persisted artifact contains the text.
- **Affected Files**: `internal/ast/embedder.go`, `internal/daemon/adapters.go`, `internal/mcpstdio/tools_ast.go`, `cmd/graphit/commands/ast.go`, `cmd/graphit/commands/lifecycle.go`.

## Test Cases & Acceptance Criteria

### Feature: Parameter ownership in a package spec
Ref: UC-01

#### Scenario: a parameter of a procedure declared in the spec belongs to the procedure
```gherkin
Given a file with "CREATE PACKAGE PCK_COBRANCA AS" declaring the procedure "ATUALIZAR_INTEGRACAO"
  And that procedure declares the parameter "P_LOG_TX"
When the file is parsed with the antlr-plsql grammar
Then the Parameter entity "P_LOG_TX" has context "ATUALIZAR_INTEGRACAO"
  And has context_type "Procedure"
  But does not have context "PCK_COBRANCA"
```

#### Scenario: a parameter of a function declared in the spec belongs to the function
```gherkin
Given a package spec declaring the function "SALDO_DEVEDOR" with the parameter "P_ID_CONTRATO"
When the file is parsed
Then the Parameter "P_ID_CONTRATO" has context "SALDO_DEVEDOR" and context_type "Function"
```

#### Scenario: the package body keeps assigning ownership correctly
```gherkin
Given a "CREATE PACKAGE BODY" with the procedure "ATUALIZAR_INTEGRACAO" and the parameter "P_LOG_TX"
When the file is parsed
Then the Parameter "P_LOG_TX" has context "ATUALIZAR_INTEGRACAO" and context_type "Procedure"
```

#### Scenario: spec parameters survive conversion to cache
```gherkin
Given a package spec declaring the procedure "P" with parameters "P_A" and "P_B"
When the parse result goes through ConvertToCache
Then the Parameter entities "P_A" and "P_B" are present in the cache
```

### Feature: Grammar safety net
Ref: UC-01

#### Scenario: a callable declared from a non-context rule fails the build
```gherkin
Given a grammar whose query creates a Function, Method, Procedure, or Constructor from a rule
  And that rule is not in context_types
  And the language is not in flatLanguages
  And the rule is not in callableContainerExemptions
  And the pattern does not mention a kind from anon_func_types
When TestEveryCallableContainerIsDeclaredAsAContext runs
Then the test fails, naming the grammar, the label, and the rule
```

#### Scenario: removing procedure_spec from plsql.yaml reintroduces the bug and is caught
```gherkin
Given plsql.yaml without "procedure_spec: Procedure" in context_types
When the suite runs
Then TestEveryCallableContainerIsDeclaredAsAContext fails, pointing at "procedure_spec"
  And TestPackageSpecParametersBelongToTheirSubprogram fails with the parameter assigned to the Package
```

### Feature: COPY batching
Ref: UC-02

#### Scenario: large rows are split across several batches
```gherkin
Given 3 rows of approximately 900 bytes of source each
When batchRows is called with a budget of 2000 bytes
Then more than one batch is produced
  And no batch is empty
  And the sum of rows across batches equals the total input
```

#### Scenario: a row larger than the budget is not discarded
```gherkin
Given a row with 10000 bytes of source between two small rows
When batchRows is called with a budget of 100 bytes
Then the large row is present in the output
  And the sum of rows across batches equals the total input
```

#### Scenario Outline: batching edge cases
```gherkin
Given the input "<input>"
When batchRows is called with budget "<budget>"
Then the result is "<result>"

Examples:
  | input          | budget    | result                      |
  | nil            | 64MiB     | no batches                  |
  | empty slice    | 64MiB     | no batches                  |
  | 2 rows         | 0         | one batch with both rows    |
  | 2 rows         | 64MiB     | one batch                   |
```

### Feature: The search index is the sole owner of the text
Ref: UC-03

#### Scenario: source comes from file_fts and the graph is never queried
```gherkin
Given a graph that returns zero rows for every query
  And a search index containing "schema/packages/PCK_X.sql" with the file's text
When GetSource is called for "schema/packages/PCK_X.sql"
Then the returned text matches what is stored in file_fts
  And the graph was never queried
```

#### Scenario: a File table row carries no text
```gherkin
Given a parse cache with "a/b.go" and its source
When fileNodeJSON builds the File table rows
Then the row for "a/b.go" has no "source" key
  And has path equal to "a/b.go"
```

#### Scenario: reading the source does not migrate or destroy the index
```gherkin
Given a search index whose search_meta.schema_version was downgraded to "0"
When FileSourceAt is called for a path present in file_fts
Then the source is returned
  And a second call still returns the source, proving file_fts was not dropped
```

#### Scenario: the error names the missing store, not the file
```gherkin
Given a SourceService built without a store path
When GetSource is called for "a/b.sql"
Then an error is returned
  And the message says the service has no store, not that the file doesn't exist
```

#### Scenario: the error cites ast.index_source when the path is not in the index
```gherkin
Given a search index containing only "a/b.sql"
When GetSource is called for "not/indexed.sql"
Then an error is returned
  And the message cites "ast.index_source" as one of the possible causes
```

#### Scenario Outline: inputs that do not resolve
```gherkin
Given a search index containing only "a/b.sql"
When FileSourceAt is called with index "<index>" and path "<path>"
Then the read is refused

Examples:
  | index           | path              |
  | valid           | not/indexed.sql   |
  | empty           | a/b.sql           |
  | valid           |                   |
  | missing file    | a/b.sql           |
```

#### Scenario: an empty source does not count as found
```gherkin
Given a search index whose file_fts has "empty.sql" with source equal to an empty string
When FileSourceAt is called for "empty.sql"
Then the read is refused, so the caller still knows the source is unavailable
```

### Feature: Embedding under ast.index_source false
Ref: UC-06

#### Scenario: the embedding keeps the source's signal without persisting the source
```gherkin
Given a shard without text, with the function "ChargeCard" on lines 3 to 6
  And the real file on disk whose hash matches the shard's hash
  And RepoRoot pointing at that file's root
When scanPending builds the pending rows with a snippet
Then the entity's snippet contains the function body
  And the embedded text contains both "ChargeCard" and the body
```

#### Scenario: no text is persisted in the shard
```gherkin
Given the same scenario, after scanPending has produced the snippet
When the shard is written to disk and read back
Then the shard's src field is empty
  And the file's body does not appear anywhere in the shard's JSON
```

#### Scenario: a changed file is not read, so it does not poison the cache
```gherkin
Given a shard whose hash was computed over the original content
  And the file on disk was rewritten after that
When scanPending tries to build the snippet
Then no snippet is produced, because the hash does not match
```

#### Scenario: without RepoRoot the entity is still embeddable
```gherkin
Given a shard without text and an empty RepoRoot
When scanPending builds the pending rows
Then no snippet is produced
  But the embedded text still contains the entity's name
```

#### Scenario: a cache with text never touches disk
```gherkin
Given a shard carrying text containing "CACHED_MARKER"
When scanPending builds the snippet
Then the snippet comes from the cache, containing "CACHED_MARKER"
```

### Feature: Sources in the bundle and the --no-sources flag
Ref: UC-05

#### Scenario: the bundle carries text coming from the search index
```gherkin
Given a search index containing "svc/handler.go" with the file's text
When ExportBundle is called with StorePath pointing at that store
Then the zip has a member "sources/svc/handler.go" with exactly that text
  And the manifest declares source_count equal to 1
```

#### Scenario: --no-sources omits the text
```gherkin
Given the same search index
When ExportBundle is called with NoSources true
Then the zip has no member under "sources/"
  And the manifest declares source_count equal to 0
```

#### Scenario: without a store the bundle is structural and declares it
```gherkin
Given BundleOptions without StorePath
When ExportBundle is called
Then the zip has no source members
  And the manifest declares source_count equal to 0
```

#### Scenario: requesting sources with no index available is an error
```gherkin
Given BundleOptions with StorePath pointing at a store that does not exist
  And NoSources false
When ExportBundle is called
Then an error is returned naming the sources step
  And no bundle is presented as successfully exported
```

#### Scenario: the scan skips rows without text and reports the missing index
```gherkin
Given an index with "has/text.go" holding text and "no/text.go" with an empty source
When EachFileSource walks the index
Then "has/text.go" is visited with its text
  But "no/text.go" is not visited
```

### Feature: Imported context is searchable
Ref: UC-04

#### Scenario: an index built from the shard cache serves both text and search
```gherkin
Given a shard cache containing "svc/handler.go" with a function "HandlePayment"
When BuildSearchIndexFor is called for a new store
Then FileSourceAt returns the text of "svc/handler.go" from that store
  And a search for "HandlePayment" in that index returns at least one result
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/queries/plsql.yaml` | Modified | `function_spec`/`procedure_spec` + matview + package-level types in `context_types`; comment documenting the invariant |
| `internal/ast/queries/java.yaml` | Modified | `constructor_declaration` as a context |
| `internal/ast/queries/javascript.yaml` | Modified | `generator_function_declaration` as a context |
| `internal/ast/queries/typescript.yaml` | Modified | Same |
| `internal/ast/queries/tsx.yaml` | Modified | Same |
| `internal/ast/queries/dart.yaml` | Modified | `function_signature`/`method_signature` as contexts |
| `internal/ast/queries/objc.yaml` | Modified | `function_definition`/`method_declaration`/`method_definition` as contexts |
| `internal/ast/queries/html.yaml` | Modified | `context_name_paths` for `element`/`script_element`/`style_element` |
| `internal/ast/queries/svelte.yaml` | Modified | `script_element`/`style_element` as contexts, with name paths |
| `internal/ast/queries/sql.yaml` | Modified | Comment only: points to `flatLanguages` and explains why it is deliberately flat |
| `internal/ast/json_rebuild.go` | Modified | Aborts on `copyErrors > 0`; `copyNode` now batches; adds `batchRows`/`estimateRowBytes`/`copyBatchBytes` |
| `internal/ast/incremental_rebuild.go` | Modified | `insertNodes` batches by the same criterion |
| `internal/ast/rebuild_index.go` | Modified | `fileNodeJSON` no longer emits `source` |
| `internal/ast/ladybug.go` | Modified | Comment in the DDL: the `source` column survives only for `'__config__'` |
| `internal/ast/source_service.go` | Modified | `WithStore`; text now comes only from the search index; error messages distinguish a missing store from a non-indexed path |
| `internal/ast/fts_sqlite.go` | Modified | `FileSourceAt`, `EachFileSource` (streaming), and `BuildSearchIndexFor` |
| `internal/ast/bundle.go` | Modified | `BundleOptions`, streamed `sources/<path>` members, `source_count` in the manifest |
| `internal/ast/server.go` | Modified | Content endpoint reads from the index; `storePathFor`; export accepts `no_sources` |
| `internal/ast/rule.go` | Modified | Property table without `File.source`; "quick source peek" section rewritten |
| `internal/hub/service.go` | Modified | AST artifact install builds the search index, and fails if it cannot |
| `internal/mcpstdio/tools_ast.go` | Modified | Passes the store to `SourceService` |
| `cmd/graphit/commands/runners.go` | Modified | Same on the CLI; bundle export message no longer promises sources |
| `internal/ast/containment_coverage_test.go` | Modified | `TestEveryCallableContainerIsDeclaredAsAContext` + helpers + `callableContainerExemptions` |
| `internal/ast/oracle_package_spec_owner_test.go` | Created | Regression test for the reported bug (spec, body, survival through the cache) |
| `internal/ast/copy_batch_test.go` | Created | `batchRows`/`estimateRowBytes` |
| `internal/ast/source_search_index_test.go` | Created | Text only from the index, File without `source`, non-migration, edge cases, searchable context |
| `internal/ast/bundle_sources_test.go` | Created | Sources in the bundle, `--no-sources`, structural bundle, error when the index is missing, the scan |
| `internal/ast/embedder.go` | Modified | `RepoRoot`, `sourceFromDisk` with a hash guard, lazy per-shard resolution, `repoRoot` in `RunEmbeddingLoop` |
| `internal/daemon/adapters.go` | Modified | Propagates `rootPath` to the embedding loop |
| `cmd/graphit/commands/ast.go` | Modified | `RepoRoot` in `ast embed` |
| `cmd/graphit/commands/lifecycle.go` | Modified | `RepoRoot` on the sync path |
| `internal/ast/embedder_no_source_test.go` | Created | Source signal under `index_source: false`, shard without text, hash guard, degradation without a root, preference for the cache |

## Trade-offs & Decisions

- **`sql.yaml` was reverted on purpose.** The first attempt added `create_function`/`create_table`/`create_view` to `context_types`, and `TestEveryShippedGrammarDeclaresItsContainment` failed: `sql` is in `flatLanguages` with a written justification. The tree-sitter-sql grammar only captures top-level CREATEs and has no column or parameter query — there is nothing nested to assign. What remained was just a comment pointing to the existing decision. A guard test with a written reason beat an assumption, which is exactly what it exists to do.
- **Batching by bytes, not by rows.** An entity row is dozens of bytes; a File row is the entire file. A count-based limit would be useless in the one case that matters.
- **Abort instead of publishing partially.** Aligned with the decision already made on the incremental path (`internal/ast/incremental_rebuild.go`). Cost: an outdated index instead of a wrong one — and a visible error instead of silence.
- **One queryable copy, not a chain of fallbacks.** The design went through three versions in sequence, and the first two were wrong: (1) read the parse-cache shard when the graph fails; (2) read `file_fts` when the graph fails, with the shard as a further fallback. Both treated redundancy as robustness. The third removes the problem instead of working around it: the text has a single owner — the search index, the only one of the three copies that is actually queryable — and the graph stops holding it. With no fallback, there's no possible drift between copies, and the `COPY` that caused the incident stops existing.
- **Choosing which copy to read from was never about saving space.** All three were already paid for on disk. The saving came from **eliminating** one: `File.source` leaves `ladybugdb`.
- **The `source` column stays in the File table's DDL.** Only because of the synthetic `'__config__'` node, which is not a file at all — it's where `RunEnrichment` stores the detected config, and the skill documents querying it. Alternative considered and rejected: moving that payload to another property, which would break a documented query for the sake of cosmetic cleanliness.
- **Search index mandatory on Hub install, not best-effort.** If the text lives only there, a context without an index isn't a degraded context, it's a useless one — so the install fails instead of delivering something half-done. Same principle as the rebuild abort.
- **Reading the file on disk to embed, instead of persisting the snippet.** The alternative was to write the per-entity snippet to the shard even under `index_source: false`. Rejected for two reasons: it contradicts the flag's purpose for anyone enabling it for confidentiality, and it's expensive — `entity_source_cost_test` measures the per-entity body at 1.31x the size of the file itself. Reading the file costs one read per shard, in the same streaming fashion the scan already uses, and persists nothing. The accepted price is depending on the tree being present on disk at embedding time, which is true for the project's own tree and for a local-path context, and is not true for a Hub artifact — where the degradation is explicit.
- **A hash guard instead of trusting the file.** Without it, embedding a newer file under the old cache key would store a vector describing code the graph doesn't contain, and it would survive until the file changed again. Having no snippet is preferred over having the wrong one.
- **`FileSourceAt` does not go through `OpenSearchIndex`.** That path migrates the schema and drops `file_fts` on a version mismatch. Accepted cost: opening and closing one sqlite connection per read, on a path that only runs once the graph has already failed.
- **Guard test restricted to callables.** The full audit (every container label) flags ~30 cases, almost all benign (imports, `package_declaration`, wrapper nodes covered by a closer context) and would need an exception list large enough to become noise. Callables are the principled cut: they are the ones that own Parameters, and a Parameter without an owner is **discarded**, not just filed under the wrong one.
- **Callable exceptions are explicit and justified** (`callableContainerExemptions`), in the same spirit as `flatLanguages`: "I didn't think about this" and "there's nothing to think about" must be distinguishable in the file.

## Technical Debt

The six open debt items were closed on 2026-08-04. Two of them, once verified with a test instead of reasoning, revealed bugs that the original justification had denied.

- [x] **The corpus index was repaired** — `make install` + `ast index --reset`. Verified: 36,823 File nodes (was 0); parameters sit under the procedure that declares them, with a per-subprogram uid, so the collision is gone; Parameter by owner: Procedure 33,472 / Function 12,432 / **Package 0**; `ast source` and `ast_search` respond.
- [x] **Building the search index no longer just logs.** This was the third instance of the "log and continue" pattern, and the worst of the three once the text came to live only there: failing would cost `ast source` for the entire project. `pipeline.go` now counts it as a write error, `fullRebuildWithSearch` returns the error, and both incremental paths capture the goroutine's error and return it to the caller.
- [x] **Rebuild logs now actually exist.** The cause was simpler and worse than "not persisted": `PipelineOptions.Logger` stayed nil, and `slogutil.Resolve(nil)` returns a **NOP handler that discards every record**. The lines that did appear in `.graphit/daemon/daemon.log` came from the supervisor, not the pipeline — which made the logging look functional. `projectRebuildLogger` writes to the same file, wired into both the sync module and the embedding module.
- [x] **Per-entity reads became per-shard reads.** The whole file is still read, because the hash covers the entire file and there's no way to verify it over a line range — memory usage ends up on par with the default path, where the shard already carries the same text. What was genuine waste, and got fixed: `embedSourceSnippet` was doing `strings.Split` over the whole file once per ENTITY. A file with 500 embeddable entities was split 500 times. Now `sliceLines` receives the lines already split, once per shard.
- [x] **Degradation without text is now reported.** Investigating closed this debt item a different way than expected: the embedding cache **travels** with the Hub artifact and feeds the index, so an installed context inherits the origin's vectors and doesn't need the text. The real gap is narrow — an entity whose vector the origin never computed — and nothing can be done locally about that. `scanPending` now counts shards without text and warns, instead of degrading silently.
- [x] **`clojure`, `julia`, and `r` were verified with a fixture, and the justification was wrong.** The exception claimed that "parameters resolve to a closer, already-declared context". They didn't: **julia** left the context EMPTY — and `ConvertToCache` discards ownerless parameters, meaning it lost all of them; **r** assigned them to a Function literally named `function`, the keyword itself, because `function_definition` has no `name` field and r names the function via the surrounding assignment. Both were changed to declare containment through `parent_capture` in the pattern; `function_definition` was removed from r's `context_types`, where it could only ever produce the ghost. clojure doesn't declare a Parameter query, so the question doesn't arise — and this is now an assertion.
- [x] **The non-callable container audit got a test** (`TestEveryNonCallableContainerIsDeclaredAsAContext`, 108 declarations verified, 23 justified exceptions). It found the same bug as `html` in **`toml`**: `table` was a declared context without `context_name_paths`, so the node was transparent and every pair fell onto File instead of belonging to its table.

### New findings, fixed right after

- [x] **Four of the five grammars with an inert context were fixed** — `protobuf` (`service: service_name`, `message: message_name`, `enum: enum_name`), `graphql` (the four types by `name`, which is a KIND rather than a field), `markdown` (`section: atx_heading/heading_content`), and `elixir` (`call: arguments/alias`, plus `parent_capture` on the Parameter query, because a `def` has no alias to name it and its parameters would go to the module instead of the function). Verified: `card_id` belongs to `Charge`, the `rpc` to `Payments`, GraphQL fields to their type, code blocks to their heading, elixir parameters to their function.
- [x] **The markdown Heading entity was renamed.** It used to be called `"# Title\n"` — the whole `atx_heading` node, marker and newline included — while the context resolved to `"Title"`. A name that doesn't match its own context is a ghost parent. The query now captures `heading_content`.
- [x] **`hcl` still had an inert context, and the first fix attempt was reverted.** **CLOSED further below, within this same task** — see "A name path learned to pick the nth child of a kind" and the `hcl` item under "### Closed subsequently". The checkbox was left open by a bookkeeping oversight and was corrected on 2026-08-05, with no code change: `hcl.yaml` already carries `block: string_lit[1]/template_literal|string_lit[0]/template_literal`, and `TestHCLAttributesBelongToTheirBlock` asserts that `bucket` belongs to `logs` — the INSTANCE, not the type — with `assertNoDanglingContains` at the end. An open debt item that was already resolved is worse than none: it sends the next agent to redo finished work. The original diagnosis text follows below because it explains *why* the index was needed.

      A path only descends, so the only reachable name is the FIRST label of the block: the TYPE in `resource "aws_s3_bucket" "logs"`, not the instance. And the type is not an entity — `TestHCLBlockLabelsAreNotAllEntities` asserts it must never be — so naming the context after it made `ConvertToCache` **synthesize** a parent UID and emit an edge to a node that's never created: measured as `Resource(a.tf::aws_instance) -> Attribute(...)` with `parentExists=false`. An attribute stuck on File is wrong; one pointing at a ghost is worse, and with the rebuild now aborting on a failed COPY, it risks bringing down the entire index. It needed a path that could say "the second child of this kind".
- [x] **The class of error I made twice myself got a guard.** `assertNoDanglingContains` converts to cache and checks that every CONTAINS edge's `ParentUID` exists among the entities. `ConvertToCache` does **not** validate this: when it can't find the parent, it synthesizes the UID (`entityUID(relPath, e.Context, "")`) and emits the edge anyway — the same shape as the historical "Table does not exist" failure that used to bring down the rebuild. Applied to the tests for r, julia, toml, protobuf, graphql, markdown, and elixir.

### Closed subsequently

- [x] **A name path learned to pick the nth child of a kind and to carry alternatives.** `contextSpec` holds a list of `namePath` entries, each segment with an optional zero-based index (`kind[n]`), and the value accepts alternatives separated by `|`, tried in order. `parsePathSegment` has a unit test because a malformed segment fails OPEN — it degrades to "the first child of that kind", meaning it resolves to the wrong node instead of to none, and a wrong owner is exactly the failure mode this area produces. An empty alternative and an index on a field (a field holds only one node) both fail validation.
- [x] **`hcl`** — `block: string_lit[1]/template_literal|string_lit[0]/template_literal`. The index is what makes this correct: `resource "aws_s3_bucket" "logs"` has two labels and the entity is the SECOND one; the first is the type, which is deliberately not an entity. The second alternative covers a single-label block — variable, output, module, provider — whose only label IS the name. Verified: `ami` belongs to `logs`, `default` to `region`, no dangling edge.
- [x] **TOML table with a dotted key** — `table: bare_key|dotted_key`. `[server.http]` is now the owner of its pairs.
- [x] **An r anonymous-function parameter got an owner.** `binary_operator: Function` with path `lhs` — the same node the Function entity takes its name from, so the owner always exists. `function_definition` stays out, since it only ever produced the ghost `function`. Verified: inside a lambda within `charge_card`, the parameter `item` belongs to `charge_card` — not to the lambda, which r doesn't name, but to the function whose body it is.
- [x] **elixir stopped turning arguments into Functions.** Every declaration in elixir is a `call`, so a pattern without a predicate on the target also matches ordinary calls: `def charge(amount, currency)` was producing a Function `amount` and a Function `currency`, and `alias Other.Helper` was producing a Module. Predicates `#match? "^(def|defp|defmacro|defmacrop)$"` were added to the Function and Parameter queries, `#eq? "defmodule"` to Module, and `#eq? "defstruct"` to Field.

### Open

Nothing in this task. The `transparentContextGrammars` registry is **empty** — the five grammars that used to be there are gone — and `TestNamelessContextsDeclareANamePath` fails if a sixth one shows up.

## System Knowledge

- **`resolveParentContextAntlr` returns the first ancestor that IS a context, and ignores the ones that aren't** — there's no fallback to "the nearest named ancestor". A rule outside `context_types` is *invisible*, not "used as a last resort". That's what makes the bug silent: the result is a plausible owner, just the wrong one.
- **`resolve`/`owner` skip the self-name.** An entity whose container has the same name and label as itself is not contained by itself; the search continues upward. This is why `create_type` as a context doesn't create a self-loop on the attributes of a `CREATE TYPE`.
- **In tree-sitter, an arrow function assigned to a variable is resolved by `anonHit`** (`internal/ast/treesitter_context.go:246`), via `anon_func_types` + `variable_declarator` — **not** via `context_types`. Declaring `variable_declarator` as a context would be unnecessary and misleading, because `variable_declarator` also serves ordinary variables.
- **An explicit `context_types: {}` is different from an absent one.** Absent (nil) falls back to `defaultContextTypes`; `{}` means "this grammar is deliberately flat". For a data-format grammar, falling back to the default is the pathological case documented in `treesitter_context.go`: 74% of parse time spent walking up to the root testing JavaScript kinds.
- **`context_types` without `context_name_paths` can be inert.** If the context node has no `name` field, `nameNodeOf` returns nil and the container becomes transparent. Declaring a context doesn't guarantee it gets used — that was the case for `html.yaml`.
- **PL/SQL entities have `line_number == end_line`** (the span comes from the name node, not the declaration), so containment here **cannot** be derived from a line range — only from the context chain. A query that tries to infer ownership by line interval within a package will not work.
- **`graphit_ast_schema` is fixed text, not introspection.** It listed `File(... source)` normally even on a database with zero File nodes. Counting nodes requires `count()`.
- **`UNION ALL` in LadybugDB scrambled the correspondence between branch and row** in a count query (`MATCH (d:Directory) RETURN count(d) UNION ALL MATCH (p:Procedure) ...` returned numbers that didn't match the same counts run separately). For diagnostic counts, run separate queries.
- **`split()` doesn't exist in LadybugDB** — grouping by directory requires `STARTS WITH`.
- **The shard cache lives at `<DB dir>/shards/`**, i.e. `.graphit/ast/project/shards/`, and **not** at `.graphit/cache/` (that one holds skills). Each `.nodes.json` shard carries the full `src`.
- **The source used to live in three places, written by different code paths**: the shard (`src`), `File.source` in the graph, and `file_fts.source` in `<DBPath>.search.sqlite`. That's why the disaster was partial — one copy was lost, not the text itself. After this task, two remain: the shard, which is the parse cache, and the index, which is the owner. `file_fts.source` is **indexed** and takes part in search with a BM25 weight of 1.0; `name` in the same table is `UNINDEXED`.
- **Text and location are now different stores, by design.** The graph answers *where* (`path`, `line_number`, `end_line`); the index answers *what*. `ast source` with `entity` uses both, and that is the only seam between them.
- **`OpenSearchIndex` is destructive on a schema version mismatch**: `migrateSearchSchema` does `DROP TABLE IF EXISTS file_fts/entity_fts/entity_trigram` when `search_meta.schema_version != ftsSchemaVersion`. Never use that path just to read.
- **A context installed from the Hub had no search index — fixed here — and still has no shards next to the database.** `internal/hub/service.go` (`case TypeAST`) loads the shard cache from the **Hub clone**, not from `filepath.Dir(dbPath)`, so nothing next to the store can serve text: the search index is the only route, which is why building it became mandatory there. A context installed via `ast_install` from a local path is different: the pipeline runs with `CacheDir: filepath.Dir(ictx.DBPath)` and produces both shards and the index normally.
- **Per-context store resolution was already uniform** and didn't need changing: `NewQueryService` derives the index from `lb.cfg.DBPath + searchIndexSuffix` (`query.go:36-38`), and `openASTDB(projectDir, context)` returns the context's backend. What was missing was the file existing, not the code knowing how to find it.
- **`ExportBundle` is export-only: there is no `ImportBundle`.** A bundle exists to carry the graph outward (to a human or another tool); the entry path for a context is always shards, via `ast_install` or the Hub.

## Verification

```
go build ./...                                          # ok
go vet ./internal/ast ./internal/mcpstdio ./cmd/graphit/commands   # ok
go test -tags fts5 ./internal/ast/ ./internal/mcpstdio/ ./cmd/...  # ok
```

The `fts5` tag is mandatory (`BUILD_TAGS` in the Makefile); without it, two search-index tests fail with `no such module: fts5` for environmental reasons.

Negative tests run to prove the safety nets catch the bug — removing `procedure_spec: Procedure` from `plsql.yaml`:

- `TestEveryCallableContainerIsDeclaredAsAContext` fails, naming `procedure_spec`;
- `TestPackageSpecParametersBelongToTheirSubprogram` fails with `P_LOG_TX owned by Package "PCK_COBRANCA"` — the exact reproduction of the reported symptom.

## Progress Log

### 2026-08-03
- Investigated the case reported in the `corpus-privado` graph: `P_LOG_TX` under `PCK_EXEMPLO`, with the pattern measured across 9,052 parameters, all under `schema/packages/`.
- Traced the cause to `plsql.yaml`'s `context_types` versus `resolveParentContextAntlr`.
- Discovered, while investigating why `ast source` wasn't responding, the second bug: zero File nodes in the graph, with the source intact in the shards, caused by a failed COPY that didn't abort the swap.
- Audited all 44 grammars; found gaps in `dart`, `objc`, `java`, `javascript`, `typescript`, `tsx`, and the `context_name_paths` gap in `html`.
- First attempt on `sql.yaml` failed by `TestEveryShippedGrammarDeclaresItsContainment` and reverted.
- Implemented the abort, the batching, and the alternative source read; wrote the four test suites; confirmed with negative tests.
- Revised the design of the source read: the first version read the parse-cache shard. Measured that the corpus's `<DBPath>.search.sqlite` has all 36,823 files with full source (`file_fts`, an indexed column), in the same database whose graph has zero File nodes. Switched to `file_fts` and dropped the shard read. Discovered along the way that `OpenSearchIndex` drops tables on a version mismatch, hence the dedicated read-only read, and that a Hub context has neither a search index nor shards next to the database.
- Second revision, the one that settles the design: instead of chaining fallbacks, **a single owner**. `File.source` leaves the graph (the 2.4 GB `COPY` that caused the incident disappears with it), `SourceService` and the HTTP endpoint now read exclusively from the index, the skill stops advertising `File.source`, and the Hub's AST artifact install now builds the index — without which an imported context would end up with neither text nor search. Skill, DDL, and error messages all aligned.

### 2026-08-04
- Commit `74b634f8` landed on main with the three previous axes of work.
- Implemented `--no-sources`. It existed on two surfaces and was accepted and discarded (`runASTExport` received it and never read it; the message always promised "(with sources)") — and after `File.source` left the graph, it also lost its object. Now the bundle streams `sources/<path>` members read from the index, the manifest declares `source_count`, requesting sources with no index is an error, and all three surfaces pass the flag through.
- `ast.index_source: false` no longer degrades semantic search. The embedding's snippet used to come from the **persisted** source, which is empty under the flag, so the vector lost exactly the part describing what the entity does. Now it comes from the file on disk, guarded by a hash check, without persisting anything. `RepoRoot` propagated through the daemon, the CLI, sync, and the MCP tool.
- `corpus-privado` reindexed with the new binary. Both reported bugs are closed in the real graph, with the numbers in the Technical Debt section.
- **Wrong prediction, recorded because it changes disk-size expectations:** I said the reindex would shrink `ladybugdb` by removing `File.source`. It **grew** instead, from 571 MB to 1078 MB. The reason is obvious in hindsight and invalidates the comparison: the 571 MB was a **mutilated** graph — zero File nodes, and therefore no `CONTAINS File→entity` or `Directory→File` edges — and still 12,752 fewer parameters (45,904 vs. 33,152). The new database is larger because it's complete. How much `File.source` would cost on top of that isn't measurable without reintroducing it.
