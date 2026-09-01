---
title: Indexing through MCP wrote to the wrong project's graph
status: done
created: 2026-07-28
updated: 2026-07-28
tags: [ast, mcpstdio, paths, bug, indexacao]
---

# Indexing through MCP wrote to the wrong project's graph

## Objective

Fix the bug reproduced by the Engineer on 2026-07-28: `graphit_ast_index(project_dir:
"/tmp/probe")` reported success, created no database at all in `/tmp/probe`, and put 16 probe nodes
in the graph of `graphit-code`. Two projects open on the same MCP server
could contaminate each other's graph, silently.

Along with it, and because it is part of the same fix: `GraphWriter.DeleteRepository` was a stub that
returned `nil` without deleting anything, which prevented `ast_index(reindex: true)` from removing
obsolete nodes.

Recorded earlier in `docs/tasks/review-skills-and-mandates.md`, section **O que sobrou**.

## Implementation Details

### The root cause is broader than the reported site

`brand.DotDir()` (`internal/brand/brand.go:22`) returns `"." + Brand` — the literal string
`".graphit"`. Every path constructor in the modules is, as a consequence, **relative to the project
root**: `ast.DefaultLadybugConfig()`, `knowledge.WikiDir()`, `memory.ProjectLinkDir(scope)`.

The MCP handlers resolved that with `os.Chdir(projectDir)` + `defer os.Chdir(origWd)`, letting the
relative path escape the block. `LadybugBackend` opens the database lazily (`sync.Once`
in `connect()`, `internal/ast/ladybug.go:104`), so the resolution happened after the `defer`,
against the server process's cwd.

**The discovery that changed the shape of the fix:** the constructors are *pure with respect to the
cwd* — they only concatenate constants, they never read `os.Getwd`. The `chdir` was never needed to
build them; it only served to resolve the relative result. That is why the fix was not
`filepath.Abs` inside the `chdir` (as suggested in the reproduction), but anchoring explicitly to
`projectDir`, eliminating the dependency on the cwd — and with it the race window between concurrent
requests.

### Sites fixed

All in `internal/mcpstdio`, through the two new helpers `anchorToProject` and `astConfigForProject`:

| Site | Defect |
|---|---|
| `openASTDBReadWrite` (`context.go`) | the reported bug |
| `openASTDB` (`context.go`) | same; the `os.Stat` inside the `chdir` masked it on reads |
| `ast_index(reset: true)` (`tools_ast.go`) | `os.RemoveAll` on a relative path **with no `chdir` at all** — it deleted the AST database of the cwd's project |
| the pipeline's `CacheDir` (`tools_ast.go`, `tools_lifecycle.go`) | parse cache written into another project |
| `resolveWikiDir` (`context.go`) | returned `.graphit/knowledge/project` outside the `chdir` |

In `resolveWikiDir` the `chdir` **stayed**: `memory.WikiDir` → `GlobalScopeDir` does an `os.Stat` to
decide whether the project's link exists, and that probe has to run in `projectDir`. Only the result
is anchored before returning.

`ast_embed` (`tools_ast.go:431`) does all its work **inside** `withProjectDir` — it was already
correct, and was not touched.

`LadybugConfigForContext` was checked as requested: absolute on the normal path (through
`brand.GlobalDir()`), relative only in the `GlobalDir() == ""` fallback, which the anchor covers.

### `DeleteRepository`

Scoped by `File.path`, which the pipeline writes relative to the indexed root. `w.rel(repoPath)`
gives `"."` for the root, the prefix for a subdirectory, and `""` for anything outside the root (in
which case it deletes nothing).

Three phases:
1. For each active label with a `path` column: `MATCH (n:Label) WHERE n.path IN $paths DETACH DELETE n`.
2. `File` and `Directory` nodes under the prefix.
3. An orphan sweep for the labels **without** a `path` column.

Phase 3 exists because of a detail of the DDL (`internal/ast/ladybug.go:208-214`): when the
grammar does not declare `Parameter`/`Field` as labels, they get a minimal schema
(`uid, name, lang, is_stub`) **without `path`**, and hang off the owner through
`HAS_PARAMETER`/`HAS_FIELD`, not off `File`. There is no way to match them by file.

The error now propagates in `tools_ast.go` — it was being discarded with `_ =`.

## Use Cases

### UC-01: Index a project from an MCP server that is in another directory
- **Actor**: agent, via `graphit_ast_index`
- **Preconditions**: MCP server running with a cwd different from `project_dir`
- **Main Flow**:
  1. `resolveProjectDir` turns `project_dir` into an absolute path
  2. `astConfigForProject(projectDir, "")` builds the config and anchors `DBPath` to `projectDir`
  3. `openASTDBReadWrite` returns a backend with an absolute path
  4. `ast.RunPipeline` writes; the lazy open resolves in the right place
- **Alternative Flows**:
  - `context` filled in → `LadybugConfigForContext`, normally already absolute, passes through
  - absolute `LADYBUGDB_PATH` → respected unchanged
- **Error Scenarios**:
  - nonexistent `project_dir` → `resolveProjectDir` errors before any open
- **Postconditions**: database in `<project_dir>/.graphit/ast/project/ladybugdb`; no other project touched
- **Affected Files**: `internal/mcpstdio/context.go`, `internal/mcpstdio/tools_ast.go`

### UC-02: Reindex removing obsolete nodes
- **Actor**: agent, via `graphit_ast_index(reindex: true)`
- **Preconditions**: graph already populated; `reset` false
- **Main Flow**:
  1. `NewGraphWriter(db, absPath, true)`
  2. `DeleteRepository(ctx, absPath)` deletes the subgraph under `absPath`
  3. `RunPipeline` with `ForceRebuild: true` reinserts
- **Alternative Flows**:
  - `path` points at a subdirectory → only the prefix is deleted
  - path outside the root → nothing is deleted
- **Error Scenarios**:
  - failure while deleting → the error **returns** to the caller; before it was swallowed
- **Postconditions**: graph with no entity from a deleted file
- **Affected Files**: `internal/ast/writer.go`, `internal/mcpstdio/tools_ast.go`

### UC-03: Reset a project's AST index
- **Actor**: agent, via `graphit_ast_index(reset: true)`
- **Preconditions**: none
- **Main Flow**: `os.RemoveAll` over the database directory **of the requested project**
- **Error Scenarios**: before this fix, it deleted the database of the project in the server's cwd
- **Postconditions**: only the requested project's database disappears
- **Affected Files**: `internal/mcpstdio/tools_ast.go`

## Test Cases & Acceptance Criteria

### Feature: per-project path resolution
Ref: UC-01, UC-03

#### Scenario: the database is born in the requested project, not in the server's cwd
```gherkin
Given the process is in the directory "bystander"
  And "target" is a distinct empty project
When openASTDBReadWrite("target") is called
  And a write forces the lazy open of the database
Then the database exists at "target/.graphit/ast/project/ladybugdb"
  And "bystander" still has no ".graphit" directory
```

#### Scenario: a missing database is reported for the requested project
```gherkin
Given "bystander" already has an AST database
  And the process is in "bystander"
When openASTDB("target") is called
Then the call fails
  And the message names the database of "target"
```

#### Scenario: an absolute path passes straight through
```gherkin
Given LADYBUGDB_PATH points to an absolute path outside the project
When astConfigForProject is called
Then DBPath is exactly that absolute path
```

#### Scenario: the knowledge wiki is anchored to the project
```gherkin
Given the process is in "bystander"
When resolveWikiDir("knowledge", "target", "") is called
Then the result is "target/.graphit/knowledge/project"
```

### Feature: DeleteRepository
Ref: UC-02

#### Scenario: deleting the root empties the graph
```gherkin
Given a graph with File, Function, Struct, Method, Parameter and Field
  And Parameter and Field have no "path" column
When DeleteRepository is called with the indexed root
Then no node remains under any label
```

#### Scenario: deleting a subdirectory preserves the rest
```gherkin
Given a graph with "sonda.go", "pedreira/veio.go" and "pedreira/xisto.go"
When DeleteRepository is called with the subdirectory "pedreira"
Then exactly one node of each label remains
  And the Parameter of "sonda.go" survives, because its owner survived
```

#### Scenario: a path outside the root deletes nothing
```gherkin
Given a graph populated with root "root"
When DeleteRepository is called with a directory outside "root"
Then the count for each label stays unchanged
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/mcpstdio/context.go` | Modified | `anchorToProject` and `astConfigForProject`; `openASTDB`/`openASTDBReadWrite` stop using `chdir`; `resolveWikiDir` anchors its return |
| `internal/mcpstdio/tools_ast.go` | Modified | `reset` and `CacheDir` use the anchored config; the `DeleteRepository` error propagates |
| `internal/mcpstdio/tools_lifecycle.go` | Modified | `sync`'s `CacheDir` anchored |
| `internal/ast/writer.go` | Modified | `DeleteRepository` implemented; `pathsUnder` and `labelHasPath` new |
| `internal/ast/ladybug.go` | Modified | `DBPath()` accessor — the backend is lazy, there was no way to inspect the destination |
| `internal/mcpstdio/context_projectdir_test.go` | Created | regression for the per-project paths |
| `internal/ast/writer_delete_repository_test.go` | Created | regression for the stub, including the case without `path` |
| `Makefile` | Modified | `test` without `-tags fts5` and swallowing the exit code; `ci` ran `vet` before `ui` |

## Trade-offs & Decisions

**Anchoring to `projectDir` instead of `filepath.Abs` inside the `chdir`.** The suggestion from the
reproduction would work, but it keeps the cwd as an intermediary. Since the constructors do not read
the cwd, anchoring is equivalent and eliminates the race between concurrent requests. Cost:
`anchorToProject` needs `projectDir` to be absolute already — it is, it always comes from
`resolveProjectDir`, and that is documented in the helper.

**Not serializing `withProjectDir` with a mutex.** There are still ~45 handlers that do `chdir` —
including `ast_embed`, which holds the cwd for the whole embedding cycle. A global mutex would
serialize long handlers. The right fix is to remove the `chdir` from those paths as was done here, one
module at a time; it was left as debt.

**Seeding the graph by hand in the `DeleteRepository` test.** The first version ran `RunPipeline` in
a temporary directory and indexed zero files — `collectFiles` filters by
`HasParserForExtensionIn`, and in a directory with no grammar installed
`TreeSitterSupportedExtensions()` returns empty. Seeding is hermetic and makes it possible to build
exactly the hard case (`Parameter`/`Field` without `path`), which a real `.go` might not produce.

**`Directory` nodes deleted by prefix.** They could be left for the pipeline to remerge, but a
removed tree would leave the folders standing — the same kind of residue this task fixes.

## Technical Debt

- [ ] **`withProjectDir` still mutates the process's cwd** in ~45 handlers (`tools_memory.go`,
      `tools_knowledge.go`, `tools_hub.go`, `tools_cluster.go`, `tools_dream.go`). Two concurrent
      requests can interfere with each other. Fix: propagate `projectDir` explicitly
      through the path constructors, as was done in the AST module.
- [ ] **`ast_embed` holds the cwd for the whole embedding cycle**, which is long. Meanwhile,
      any handler that depends on the cwd sees the wrong directory.
- [ ] **Relative `LADYBUGDB_PATH` changed semantics**: it resolves against `projectDir`, not against
      the cwd. That is the coherent semantics and the one that makes the override work per project,
      but it is a behavior change. Absolute — the expected use — does not change.
- [x] **`make ci` was green and lying** — resolved in this task. See `## System Knowledge`.
- [x] **The 16 probe nodes** from the original report stay in this repository's graph until someone
      runs `ast_index(reset: true)` or `reindex: true` — the latter now works.
      **CLOSED on 2026-08-05, by fact and not by action:** the database in
      `.graphit/ast/project/ladybugdb` did not exist at the start of that session — `ast_schema` and
      `ast_search` answered *no AST database found* — and the graph was built from scratch. A
      fresh index from an absent database is stronger than `reset: true`. Audited afterwards:
      the only occurrences of "sonda" in the graph are `seedSondaGraph`, `newSondaGraph` and
      `sondaSchema`, which are real helpers in `internal/ast/writer_delete_repository_test.go`.

## System Knowledge

- **`brand.DotDir()` is a relative string literal**, not a resolved path. Every path
  derived from it is relative to the project root and needs to be anchored by whoever consumes it
  from outside the project. It is the origin of this whole class of bug.
- **`LadybugBackend` connects lazily** (`sync.Once` in `connect()`). A wrong config does not
  fail at construction — it fails, silently and in the wrong place, on the first query.
- **Not every node carries `path`.** `Parameter` and `Field` get a minimal DDL when the grammar does
  not declare them. A query that assumes `n.path` on every label fails with *Cannot find property
  path*.
- **The pipeline only indexes extensions with an installed grammar.** In a temporary directory
  `TreeSitterSupportedExtensions()` returns an empty list and `RunPipeline` returns `TotalFiles: 0`
  without an error. A test that depends on real indexing needs a grammar, or needs to seed the graph.
- **`File.path` is relative to the root passed to the pipeline**, not to the project root. Indexing a
  subdirectory writes paths relative to that subdirectory.
- **`internal/ui/dist` is not versioned** (`.gitignore:29`). A `go build ./...` in a new worktree
  fails in `internal/ui/embed.go` with *pattern dist/\*: no matching files found* until `make ui` is
  run. It is not a regression.
- **The Makefile's `test` target ran without `-tags fts5`**, even though `BUILD_TAGS := fts5` exists
  on line 39 and is used by the build targets. Without the tag, SQLite has no FTS5 module and 30
  search index tests fail with *no such module: fts5*. The tag is a documented project convention —
  the target simply did not reference it.
- **The `test` target swallowed `go test`'s exit code.** The invocations were followed by a `;` and a
  coverage `if` block, so the recipe's status was the last command's. Result:
  `make ci` printed *"✅ All CI checks passed"* with 30 tests failing on screen and exited 0. In a
  `make` recipe chained with `\`, every `go test` needs `|| status=1` and an `exit $$status`
  at the end — or the failure disappears.
- **The whole suite passes with `-tags fts5`**, in both sets that `make test` runs: project
  packages (with `-race`) and generated parser packages. Both exit 0.

## Progress Log

### 2026-07-28
- Root cause confirmed by reading `context.go`, `ladybug.go` and `config.go`.
- Four additional sites of the same defect turned up, two of them worse than the reported one
  (destructive `reset`, knowledge wiki).
- Fixed with `anchorToProject`/`astConfigForProject`; `chdir` removed from the open path.
- `DeleteRepository` implemented, with the orphan sweep for the case without `path`.
- Tests written and **verified against the old code**: they fail there, they pass here. Also
  verified that the orphan sweep is necessary — without it `map[Field:1 Parameter:1]` is left over.
- `go test -race -tags fts5 -p 4 ./internal/ast/... ./internal/mcpstdio/...` passes;
  `golangci-lint` with no issues in those packages.
- `make ui` + `make ci` run at the Engineer's request.
