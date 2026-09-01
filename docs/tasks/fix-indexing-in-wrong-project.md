---
title: Indexing by MCP was recording on the wrong project graph.
status: done
created: 2026-07-28
updated: 2026-07-28
tags: [ast, mcpstdio, paths, bug, indexacao]
---

Indexing by MCP was written to the wrong project graph.

## Objective

Correcting the bug reported by Engineer on July 28, 2026: `graphit_ast_index(project_dir:
"/tmp/probe")` reported success, did not create any database at `/tmp/probe`, and placed 16 sensor nodes in the ___INLINE_1__ graph. Two open projects on the same MCP server could silently contaminate each other's graphs.

Together, and since it was part of the same correction: _INLINE_2__ was a stub that returned
`nil` without erasing anything, which prevented __INLINE_4___ from removing obsolete elements.

Registered before in `docs/tasks/revisar-skills-e-mandates.md`, section **What's left**.

## Implementation Details

The root cause is broader than the reported site.

The constructor for all path module builders returns a string literal — the root of the project. Every constructor is consequently relative to the project's root: `ast.DefaultLadybugConfig()`, `knowledge.WikiDir()`, `memory.ProjectLinkDir(scope)`.

Note: The inline code blocks and markdown are preserved as they are in the original text.

The handlers MCP resolved this by using `os.Chdir(projectDir)` + `defer os.Chdir(origWd)`, allowing the relative path to escape the block. `LadybugBackend` opens the bank in a lazy manner (`sync.Once` in `connect()`, `internal/ast/ladybug.go:104`), so the resolution happens after `defer`, against the server process's cwd.

**Discovery that Changed the Correction:** The constructors are *pure with respect to cwd*—they only concatenate constants, never read `os.Getwd`. The `chdir` was never necessary for them; it served merely to resolve the relative result. Therefore, the correction was not implemented within the `chdir` (as suggested in the reproduction), but rather anchored explicitly at ___INLINE_24__, eliminating the dependence on cwd—thus removing the concurrent request window dependency.

Note: The code block and markdown formatting have been preserved as requested.

### Sites corrigidos

All in `internal/mcpstdio` through the two new helpers `anchorToProject` and `astConfigForProject`:

| Site | Defect |
|---|---|
| Inline 28 (Inline 29) | The reported defect |
| Inline 30 (Inline 31) | Idem; the Inline 32 within Inline 33 masked in reading |
| Inline 34 (Inline 35) | Relative path relative **without** any inline 37 — erased the AST database of the cwd project |
| Inline pipeline (Inline 38, Inline 39) | Cache of parse recorded in another project |
| Inline 41 (Inline 42) | Returned _INLINE_43_ outside _Inline_44_

The reported defect
Idem; the inline 32 within inline 33 masked in reading
Relative path relative **without** any inline 7 — erased the AST database of the cwd project
Cache of parse recorded in another project
Returned _inline_ 31 outside _inline_ 44

In `resolveWikiDir` the `chdir` **remained**: `memory.WikiDir` → `GlobalScopeDir` decides whether the project link exists, and this survey must run on `projectDir`. Only the result is anchored before returning.

The code block is performing all the work **inside** the `withProjectDir` — it was already correct and untouched.

It was verified as an order: absolute on the normal path (via __INLINE_55__), relative only in the fallback __INLINE_56__, which the anchor covers.

### `DeleteRepository`

Scope by `File.path`, the pipeline writes relative to the indexed root. `w.rel(repoPath)` provides `"."` for the root, a prefix for a subdirectory, and `""` outside of the root (in which case nothing is deleted).

Three phases:
1. For each active label with column __INLINE_62__: __INLINE_63__.
2. We __INLINE_64__ and __INLINE_65__ under the prefix.
3. Orphan search for labels without column __INLINE_66__.

The phase 3 exists due to a detail in the DDL (`internal/ast/ladybug.go:208-214`): when the grammar does not declare `Parameter`/`Field` as labels, they receive a minimum schema (__INLINE_70___) **without __INLINE_71__**, and hang on their owner via __INLINE_72__/__INLINE_73__, not in __INLINE_74__. It is impossible to couple them by file.

The error began to rise in `tools_ast.go` — it was discarded with `_ =`.


## Use Cases

### UC-01: Index a project from an MCP server located in another directory

- **Actor**: agent, via `graphit_ast_index`
- **Preconditions**: the MCP server is running with CWD different from `project_dir`
- **Main Flow**:
  1. __INLINE_79__ transforms __INLINE_80__ into an absolute path
  2. __INLINE_81__ builds the config and attaches it to __INLINE_82__
  3. __INLINE_84__ returns backend with an absolute path
  4. __INLINE_85__ saves; lazy opening resolves at the right place

- **Alternative Flows**:
  - `context` filled → `LadybugConfigForContext`, normally already absolute, goes directly
  - `LADYBUGDB_PATH` absolute → respected without change

- **Error Scenarios**:
  - `project_dir` does not exist → __INLINE_90__ fails before any opening

- **Postconditions**: database in ___INLINE_91__; no other project touched
- **Affected Files**: `internal/mcpstdio/context.go`, `internal/mcpstdio/tools_ast.go`

### UC-02: Reindexing without removing obsolete nodes

**Actor**: Agent, via `graphit_ast_index(reindex: true)`

**Preconditions**: Graph already populated; `reset` is false

**Main Flow**:
1. `NewGraphWriter(db, absPath, true)`

2. `DeleteRepository(ctx, absPath)` deletes the subgraph under `absPath`

3. `RunPipeline` reinserts with `ForceRebuild: true`

**Alternative Flows**:
- `path` points to a subdirectory → only the prefix is deleted
- Path outside the root → nothing is deleted

**Error Scenarios**:
- Failure during deletion → error **returns** to caller; it was previously swallowed

**Postconditions**: Graph without entity file deleted

**Affected Files**: `internal/ast/writer.go`, `internal/mcpstdio/tools_ast.go`

### UC-03: Resetting the AST Index of a Project

- **Actor**: Agent, via Inline 104
- **Preconditions**: None
- **Main Flow**: ___Inline 105___ within the directory of the project requested
- **Error Scenarios**: Before this correction, it would delete the database from the current working directory on the server
- **Postconditions**: Only the requested project's database disappears
- **Affected Files**: ___Inline 106___

## Test Cases & Acceptance Criteria

Feature: Project Path Resolution
Ref: UC-01, UC-03

Scenario: The bank is born in the project request, not in the server's current working directory.
```gherkin
Given that it's in the directory "bystander"
And "target" is an empty project distinct
When `openASTDBReadWrite` is called
And an opening is forced by lazy writing of the bank
Then o banco existe em "target/.graphit/ast/project/ladybugdb"
And "bystander" remains without directory ".".
```

The scenario is that an absent bank is reported to the project request.
```gherkin
Given "bystander" already has an AST bank
The process is in "spectator mode."
When `openASTDB("target")` is called
Then a chamada falha
  And a mensagem nomeia o banco de "target"
```

#### Scenario: caminho absoluto passa direto
```gherkin
Given LADYBUGDB_PATH aponta para um caminho absoluto fora do projeto
When `astConfigForProject` is called
Then DBPath is exactly that absolute path
```

Scenario: The knowledge wiki is anchored in the project
```gherkin
Given that the process is in "spectator mode"
When `resolveWikiDir("knowledge", "target", "")` is called
The result is "target/.graphit/knowledge/project"
```

### Feature: DeleteRepository
Ref: UC-02

Scenario: Removing the root vertex empties the graph
```gherkin
Given um grafo com File, Function, Struct, Method, Parameter e Field
And Parameters and Fields do not have column "Path"
When `DeleteRepository` is called with indexed root
Then no node remains in any label.
```

Scenario: Deleting a Subdirectory Preserves Everything
```gherkin
Given um grafo com "sonda.go", "pedreira/veio.go" e "pedreira/xisto.go"
When `DeleteRepository` is called with the subdirectory "pedreira"
There is exactly one knot on each label.
  And o Parameter de "sonda.go" sobrevive, porque seu dono sobreviveu
```

Scenario: The path outside the root does not delete anything
```gherkin
Given um grafo povoado com raiz "root"
When `DeleteRepository` is called with a directory outside of "root"
Then a contagem de cada label permanece inalterada
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/mcpstdio/context.go` | Modified | `anchorToProject` and `astConfigForProject`; `openASTDB`/`openASTDBReadWrite` stop using `chdir`; `resolveWikiDir` still returns |
| `internal/mcpstdio/tools_ast.go` | Modified | `reset` and `CacheDir` use the anchored config; error of `DeleteRepository` rises |
| `internal/mcpstdio/tools_lifecycle.go` | Modified | `CacheDir` from `sync` is anchored |
| `internal/ast/writer.go` | Modified | `DeleteRepository` implemented; `pathsUnder` and `labelHasPath` new |
| `internal/ast/ladybug.go` | Modified | accessor `DBPath()` — the backend is lazy, there was no way to inspect the destination |
| `internal/mcpstdio/context_projectdir_test.go` | Created | regression of paths by project |
| `internal/ast/writer_delete_repository_test.go` | Created | regression stub, including case without `path` |
| `Makefile` | Modified | `test` without `-tags fts5` and swallowing the exit code; it ran `ci` before `vet` |

The file has been modified. The changes are as follows:

- Inline 107: The inline is updated, and Inline 108 and 109 use a different configuration that no longer requires Inline 112.
- Inline 114: The inline is updated, and Inline 115 and 116 use an anchored config; there's an error in Inline 117 rising.
- Inline 118: The inline has been modified, and the configuration from Inline 119 uses a different version of Inline 120.
- Inline 121: The inline is updated, and Inline 122 and 123 are new implementations.
- Inline 125: The accessor Inline 126 has been modified; the backend is lazy, so there's no way to inspect the destination.
- Inline 127: A regression of paths by project has been created.
- Inline 128: A regression stub with a case without Inline 129 has been created.
- Inline 130: The inline has been modified, and Inline 131 is missing Inline 132; it runs Inline 133 before Inline 134.

## Trade-offs & Decisions

Anchor in **__INLINE_136__** instead of **__INLINE_137__** within **__INLINE_138___.** The suggestion works but keeps the cwd as an intermediary. Since constructors do not read the cwd, anchoring is equivalent and eliminates concurrent request competition. Cost: **__INLINE_139__** requires that __INLINE_140__ already be absolute — always comes from **__INLINE_141__**, and is documented in the helper.

---

Note: The provided inline codes are placeholders for actual code blocks, which should be replaced with the appropriate content.

Do not serialize ___INLINE_142__ with mutex. There are ~45 handlers that do ___INLINE_143__ — including ___INLINE_144__, which holds the cwd throughout the embedding cycle. A global mutex would serialize long handlers. The correct fix is to remove ___INLINE_145__ from these paths as shown here, one module at a time; it remains as credit.

---

Note: I've translated "___" into underscores for clarity and replaced "embedding" with "embedding cycle" as that's the closest equivalent in English for the Portuguese term.

Sow the graph by hand in the test of ___INLINE_146_. The first version ran ___INLINE_147__ in a temporary directory and indexed zero files — ___INLINE_148__ filters for ___INLINE_149__, and in a directory without grammar, it returns empty. Sowing is hermetic and allows to construct exactly the difficult case (___INLINE_150__/___INLINE_151__ without ___INLINE_152__), which a real `path` might not produce.

We `Directory` are being purged by prefix. They could be left for the pipeline to reprocess, but removing an
tree would leave the folders standing—exactly the same type of residue that the task corrects.

## Technical Debt

- [ ] **`withProjectDir` continues to mutate the process's cwd** in ~45 handlers (`tools_memory.go`,
      `tools_knowledge.go`, `tools_hub.go`, `tools_cluster.go`, `tools_dream.go`). Concurrent requests can interfere with each other. Correction: propagate `projectDir` explicitly through the path constructors, as was done in the module AST.
- [ ] **`ast_embed` securely holds the cwd throughout the embedding cycle**, which is long. While this happens, any handler that depends on the cwd sees the wrong directory.
- [ ] **`LADYBUGDB_PATH` relative has a new semantics**: resolves against `projectDir`, not against the cwd. This is the consistent and the one that makes override work project-wide, but it's a behavior change. Absolute — the expected use case — does not change.
- [x] **`make ci` was green and lying** — resolved in this task. Check `## System Knowledge`.
- [x] The 16 sonar nodes from the original report continue to be part of this repository's graph until someone runs `ast_index(reset: true)` or `reindex: true` — this latter one now works.
    - **CLOSED on August 5, 2026, due to fact and not action:** the database in `.graphit/ast/project/ladybugdb` did not exist at the beginning of that session — `ast_schema` and `ast_search` responded *in AST database found* — and the graph was built from scratch. A new index based on an absent database is stronger than `reset: true`. Audited after:
    - The only occurrences of "sonar" in the graph are `seedSondaGraph`, `newSondaGraph`, and `sondaSchema`, which are real helpers of `internal/ast/writer_delete_repository_test.go`.

Note: Inline comments have been removed for brevity.

## System Knowledge

- **INLINE_178** is a relative string literal, not a resolved path. Every derived path from it is relative to the project root and must be anchored by the consumer outside of the project. This is the origin of all these bug classes.
- **INLINE_179** connects sloppily (INLINE_180 in INLINE_181). An incorrect configuration does not fail during construction — fails silently and incorrectly, at the first query.
- Not every node carries **INLINE_182**. **INLINE_183** and **INLINE_184** receive DDL minimum when the grammar does not declare them. Queries that assume **INLINE_185** in all labels fail with *Cannot find property path*.
- The pipeline indexes only extensions installed by grammar. A temporary directory INLINE_186 returns an empty list, and INLINE_187 returns INLINE_188 without error. A test that depends on real indexing needs the grammar or seeds a graph.
- **INLINE_189** is relative to the root passed to the pipeline, not the project’s root. Indexing a subdirectory writes relative paths to that subdirectory.
- **INLINE_190** is not versioned (INLINE_191). A **INLINE_192** in a new worktree fails with *pattern dist/\*: no matching files found* until it runs **INLINE_194**. This is not regression.
- The target **INLINE_195** of the Makefile ran without **INLINE_196**, despite **INLINE_197** being present on line 39 and used by build targets. Without the tag, SQLite does not have module FTS5 and 30 search index tests fail with *no such module: fts5*. The tag is documented convention of the project — the target simply did not reference it.
- The target **INLINE_198** consumes the output code from **INLINE_199**. Invocations were followed by **INLINE_200** and a block **INLINE_201** of coverage, so the status was that of the last command. Result: INLINE_202 printed *"✅ All CI checks passed"* with 30 tests failing on the screen and exiting 0. In an encumbered recipe **INLINE_203** by **INLINE_204**, every **INLINE_205** needs **INLINE_206** and a **INLINE_207** at the end — or the failure disappears.
- The entire suite passes with **INLINE_208** in both sets that the **INLINE_209** runs: project packages (with **INLINE_210**) and generated parser packages. Both exit 0.

## Progress Log

### July 28, 2026
- The root cause was confirmed by reading ___INLINE_211__, ___INLINE_212__, and ___INLINE_213__.
- Four additional sites were raised with the same defect; two of them are worse than the reported (___INLINE_214__ destructive, wiki of knowledge).
- Fixed using ___INLINE_215__/___INLINE_216__; ___INLINE_217__ removed from the opening path.
- ___INLINE_218__ implemented, with orphan scanning for cases without ___INLINE_219__.
- Written tests and **verified against old code**: fail there, pass here. Also verified that orphan scanning is necessary — without it, remnants are ___INLINE_220__.
- ___INLINE_221__ passes;
  ___INLINE_222__ issues-free in these packages.
- `make ui` + `make ci` executed on request of the Engineer.
