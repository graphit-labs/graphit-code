---
title: Project-less Hub installs, project-less MCP queries, and artifact content retrieval
status: done
created: 2026-09-01
updated: 2026-09-01
tags: [hub, mcp, store, ast, knowledge, memory, global]
---

# Project-less Hub installs, project-less MCP queries, and artifact content retrieval

## Objective

Two capabilities, both asked for so that an agent **outside any checkout** — a web agent
reaching this framework over MCP HTTP — can use the Hub at all.

1. **Install a Hub artifact with no project.** The artifact installs into the place that
   already centralises Hub installs per version, and no project directory is involved.
   The MCP tools may then be called **without `project_dir`**, addressing the artifact by
   its **qualified identifier with version** (`id@version`) instead of by project. This
   must cover `ast` (install and queries), `knowledge`, and `memory`.
2. **Retrieve the content of a `rule`, `skill`, `command` or `agent` artifact over MCP.**
   An artifact may hold **more than one file** — a skill is exactly that case — so the
   response is a structure **keyed by path**: `path -> content`.

### Reasoning — what the codebase already gives us for free

Read before planning; this is why the change is small rather than a new subsystem.

- **Every compiled store is ALREADY global and keyed by id + version.** `store.ASTHubDir(contextID, version)`,
  `store.KnowledgeHubDir(contextName, version)`, and the file-artifact clone cache at
  `hub.ArtifactCacheDirIn(cacheBase, type, id, version, projectID)` are all under the global
  brand directory with no project in the path. Nothing about the *data* is per-project.
- **What ties an artifact to a project is membership, and only membership.** One lockfile
  entry, read back by `store.LookupContext` → `store.ASTContextDirIn` /
  `store.KnowledgeContextDirIn`. That single lookup is the whole reason a query needs a
  project today.
- **The global lock already records installs per artifact version.**
  `hub.GlobalLockManager` writes `<global>/global.lock.json` with
  `Artifacts[type/id@version]` carrying id, name, version, hash, type, `CachePath`, and a
  `Projects` map. It is already the machine-wide registry of installs; it just has no
  notion of an install that belongs to no project.
- **`_global` is already the reserved project key** (`hub.globalProjectKey`), used by
  `ArtifactPrefix` for artifacts published outside any project.
- **Memory needs no project for two of its three scopes.** `memory.UserScopeID()` derives
  from the machine unit id, and a named context scope is its own id
  (`resolveScopeIDIn` default branch). Only `project` scope requires a lockfile.

### Justification — why this approach, and what was dropped

**Chosen: a global membership registry read by `internal/store`, written by `internal/hub`.**
`store.LookupContext` gains a fallback: with an empty `projectDir` it resolves from
`global.lock.json` instead of a project lockfile. Every existing resolver
(`ASTContextDirIn`, `ASTContextIcebugDirIn`, `KnowledgeContextDirIn`) then works
project-lessly with no change of its own, because they all funnel through that one lookup.
`internal/store` is a leaf package and may not import `internal/hub`, so it reads the
global lock with an anonymous struct — the same asymmetry `store.ProjectID` already has
with the project lockfile, and the same trick `ast.HubContextsForProject` used before the
registries were unified.

Dropped alternatives:

- **A synthetic/ephemeral project per MCP session.** This is what live search does
  (`internal/livesearch/prep`), and it is right *there* because a live search needs a
  workspace an agent CLI can be pointed at. Here it would be pure overhead: a lockfile,
  an identity, and a directory to reclaim, all to hold membership records that the global
  lock can hold directly. It also re-opens the leak `TestPrepareLeavesNoTraceInTheEcosystem`
  exists to prevent.
- **A separate `global.contexts.json`.** Rejected for the reason `contexts.json` was
  deleted in the first place (see `docs/tasks/one-registry-for-context-membership.md`): a
  second registry beside the first drifts from it, and the first one already carries
  version, hash, type and cache path.
- **Making `project_dir` a no-op that silently falls back to the global scope.** Rejected:
  a project that never installed an artifact must not be able to query it. The global
  fallback fires only when `project_dir` is genuinely absent.
- **Reading artifact content off the IDE directory of a project.** That is the *materialised
  copy*, and it does not exist without a project. The clone cache is the artifact.

## Plan & Task Breakdown

- [x] **T1 — Global context registry (read side) in `internal/store`** — Spec: new
  `internal/store/globalcontexts.go`. Reads `<global>/global.lock.json` with an anonymous
  struct (no import of `internal/hub`). Exposes `ListGlobalContexts(kind)` and
  `LookupGlobalContext(kind, name)` returning the existing `ContextRecord`, matching on
  artifact id, sanitised id, `ContextNameFor(id, projectID)`, and the qualified `id@version`.
  `LookupContext(projectDir, kind, name)` falls back to it **only when `projectDir == ""`**.
  Done when `ASTContextDirIn("", "x@1.0.0")` and `KnowledgeContextDirIn("", "x@1.0.0")`
  resolve to the same directories a project-scoped install would. Constraint: `internal/store`
  must stay a leaf (imports `brand` + `projectlock` only).

- [x] **T2 — `hub.Install` with no project** — Spec: `internal/hub/service.go`. When
  `projectDir == ""`: do not call `paths.GetPathsForProject` (with both arguments empty it
  walks up from the process cwd and would bind the install to whatever project the server
  happens to sit in), do not load or save a project lockfile, skip the IDE-materialisation
  branch, and register in the global lock under the `_global` project key with an empty
  project dir. Store work (`ensureASTStore`, knowledge wiki placement, clone) and recursive
  dependency installs run unchanged. Done when installing each of `ast`, `knowledge`,
  `rule`, `skill`, `command`, `agent` with no project leaves the global stores populated,
  a `global.lock.json` entry present, and **no** file written inside any project.
  Constraint: an install that fails halfway must not leave a global-lock entry claiming a
  store that was never built.

- [x] **T3 — `hub_install` / `hub_uninstall` MCP tools accept an absent `project_dir`** —
  Spec: `internal/mcpstdio/tools_hub.go` + a `resolveProjectDirOptional` in `context.go`.
  Empty `project_dir` means the global scope; the IDE parameter is ignored there and the
  result says so. Uninstall in global scope drops the `_global` claim and cleans the shared
  store only when no project still references it (existing orphan logic). Done when both
  tools round-trip an artifact with no `project_dir`.

- [x] **T4 — AST tools without a project** — Spec: `internal/mcpstdio/tools_ast.go`,
  `context.go`. `ast_query`, `ast_search`, `ast_source`, `ast_schema`, `ast_list` accept an
  empty `project_dir` **provided `context` is given** as `id` or `id@version`; without a
  context there is nothing to answer about, and the error must say that rather than opening
  a graph keyed by a path hash. `anchorToProject` must not turn a global absolute path into
  a relative one. Write paths (`ast_index`, `ast_embed`, read-write open) are refused in
  global scope, for the reason `openASTDBReadWrite` already refuses an ephemeral project:
  opening read-write creates a store. Done when a query against a globally installed AST
  artifact returns rows with no `project_dir`.

- [x] **T5 — Knowledge and wiki tools without a project** — Spec:
  `internal/mcpstdio/tools_knowledge.go`, `tools_wiki.go`, `resolveWikiDir` in `context.go`.
  Same rule: empty `project_dir` requires `context`, resolved through T1. `knowledge_search`,
  `wiki_search`, `wiki_browse`, `wiki_source`, `wiki_xrefs` covered. Indexing, linting,
  export and install-into-project stay project-only. Done when a globally installed
  knowledge artifact is searchable and one of its pages readable with no `project_dir`.

- [x] **T6 — Memory tools without a project** — Spec: `internal/mcpstdio/tools_memory.go`,
  `memoryScopeFor`. Empty `project_dir` serves the `user` scope by default and a named
  context scope when asked; `scope: "project"` with no project is refused with a sentence
  that names why. Done when `memory_search`/`memory_list`/`wiki_source(wiki: "memory")`
  work with no `project_dir`. Constraint: must not create a memory scope keyed by a
  path hash of a directory nobody owns.

- [x] **T7 — New MCP tool: artifact content, keyed by path** — Spec: new tool in
  `internal/mcpstdio/tools_hub.go`, `brand.MCPToolName("hub", "content")`. Input: `id`
  (accepts `@version`), optional `type` to disambiguate, optional `project_dir` (when given,
  the project's claim pins the version), optional `path` to fetch a single file. Output: a
  map **keyed by the artifact-relative path** with the file's text as the value, so a
  multi-file artifact (skill) arrives whole in one call. Supported types: `rule`, `skill`,
  `command`, `agent`; `ast` and `knowledge` are refused by name with a pointer to
  `ast_source` / `wiki_source`, because those are mounted, not downloaded, and have no file
  tree to hand back. Non-text files are listed with a marker instead of their bytes.
  Done when a skill artifact returns every one of its files in one response.

- [x] **T8 — Documentation and skill/mandate updates** — Spec: this log kept current;
  `docs/` updated for the new tool and the project-less mode; the `graphit-hub` and
  `graphit-ast` skill text updated so an agent knows `project_dir` is optional in these
  cases and how the qualified identifier is passed. Done when the skills describe the new
  calls without contradicting the existing "always pass absolute project_dir" wording.

- [x] **T9 — Verification** — Spec: `go build ./...`, the package test suites for
  `store`, `hub`, `mcpstdio`, `golangci-lint`. New tests: global lookup resolution, a
  project-less install leaving no project file, a project-less query, and the content tool
  on a multi-file artifact. Constraint: every test isolates `HOME` **and** `USERPROFILE`
  (`t.Setenv`) — the existing convention in these packages.

## Implementation Details

### T1 — the global membership registry (`internal/store/globalcontexts.go`)

`global.lock.json` is read with an anonymous struct — id, version, type, `projectId`, and the
keys of the `projects` map — because `internal/store` must stay a leaf (`brand` + `projectlock`
only), and `internal/hub` imports `ast`/`knowledge`/`memory`, which import `store`. The same
asymmetry `store.ProjectID` already has with the project lockfile it does not own.

- `GlobalOwnerKey = "_global"` is the reserved owner. It matches the key the Hub already uses
  for artifacts published outside any project, so both senses of "global" read alike in the file.
- `ownedGlobally()` is the discriminator: an entry whose only owners are real projects is in the
  global lock because **every** install is recorded there, not because anyone installed it
  globally. Answering a project-less query from one would hand out a store nobody shared.
- `SplitQualified` splits on the **last** `@` with `i > 0`, so a scoped name (`@scoped@2.0.0`)
  keeps its leading `@` instead of yielding an empty id.
- `ListGlobalContexts` / `LookupGlobalContext` sort candidates, so an unqualified reference
  resolving to "the highest installed version" does not depend on Go's random map order.
- `LookupContext(projectDir, ...)` and `ListContexts` fall back to the global registry **only**
  when `projectDir == ""`. Every existing resolver — `ASTContextDirIn`,
  `ASTContextIcebugDirIn`, `KnowledgeContextDirIn`, and therefore
  `ast.ListImportedContextsIn` and `hub.MountedWikiFor` — inherits the project-less mode with
  no change of its own.

### T2 — `hub.Install` with no project (`internal/hub/service.go`)

`globalInstall := projectDir == ""` gates three things and nothing else:

1. **`paths.GetPathsForProject` is not called.** With both arguments empty it falls through to
   `paths.GetPaths`, which walks **up from the process working directory** — a server sitting
   inside a checkout would have bound the install to that project, silently and successfully.
   `pp` stays nil in the global path, which is why the IDE branch is gated too.
2. **No project lockfile is read or written**, so the "project not initialized" refusal no
   longer applies; membership goes to the global lock under `GlobalOwnerKey` with an empty
   owner directory.
3. **The IDE-materialisation branch is skipped.** There is no project directory to copy into,
   and the clone in the shared cache is the artifact — which is what T7 then serves.

Everything else is the same code: version resolution, `ensureASTStore`, the versioned knowledge
wiki placement, the language-artifact global install, and recursive dependencies. The
"already installed?" check that dependencies use was reading the project lockfile; it is now a
pre-built `alreadyClaimed` set filled from the lockfile **or** from the global installs.

`RegisterInstall`'s ten positional parameters became `InstallRecord`. The eleventh value was
the publishing project, and a mis-ordered pair among four look-alike ids and paths does not
fail — it registers against the wrong owner or addresses a store that was never built.
`GlobalArtifact` gained `ProjectID` (the **publisher**, same meaning as in
`LockfileArtifactMeta`) because it is half of a store's address: a project-scoped install has it
in its own lockfile, a project-less one has only this entry.

`UninstallGlobal` is a separate method rather than a branch: `Uninstall` reads type, version and
members from the project lockfile, decrements `InstalledBy` there, and removes an IDE copy —
none of which exists globally. `Uninstall` with an empty `projectDir` delegates to it.
`findGlobalInstall` **refuses** an ambiguous reference and lists the candidates; two versions are
two stores, and dropping the wrong one is invisible afterwards.

**A latent bug fixed on the way:** `ValidateProjectDirs` pruned any owner whose
`ProjectDir + lockfile` did not stat, and `filepath.Join("", "<lockfile>")` is **relative** — so
it resolved against the process's working directory and deleted exactly the owners that have no
directory. That already affected the pre-existing `__transient__` owner; it would have deleted
every global install. The guard is `if proj.ProjectDir == "" { continue }`.

### T3–T6 — the MCP layer (`internal/mcpstdio`)

Three resolvers, deliberately separate from `resolveProjectDir`, which keeps failing loudly for
the tools that genuinely need a project:

| resolver | rule |
|---|---|
| `resolveProjectDirOptional` | `""` is the global scope; a non-empty path is still validated |
| `resolveArtifactScope` | global scope **requires** a `context`, or there is nothing to answer about |
| `resolveWikiScope` | as above, except `wiki: "memory"` needs neither — its user scope is machine-keyed |

Read tools that accept an absent `project_dir`: `ast_query`, `ast_schema`, `ast_search`,
`ast_source`, `ast_list`, `knowledge_search`, `wiki_browse`, `wiki_log`, `wiki_xrefs`,
`wiki_source`, `wiki_search` (restricted to `wikis: ["memory"]`), and the memory tools
`insert`, `update`, `delete`, `list`, `search`, `important`, `promote`, `demote`, `index`.

Refused in the global scope: `ast_index`, `ast_embed`, `ast_install`, `ast_remove`,
`ast_export`, `knowledge_index`/`lint`/`install`/`remove`/`sync`/`export`/`list`, `wiki_embed`,
`memory_export`/`sync`/`remove`, and anything opening a store read-write.

Four functions needed guards for the empty path, and each was a silent-wrong-answer rather than
an error:

- `anchorToProject("", p)` returned `filepath.Join("", p)` — a no-op that leaves the path
  **relative**, the exact state the function exists to remove.
- `withProjectDir("")` would have chdir'd to `""`.
- `loadProjectConfig("")` / `loadProjectLockInfo("")` joined to a **relative** lockfile path and
  would have applied the working-directory project's Hub bucket and module switches to a request
  that named no project.
- `openASTDBReadWrite("")` refuses: opening read-write creates the store, and with no identity it
  would be filed under the hash of an empty path.

`memoryScopeFor` redirects an absent project to the `user` scope and reports it, mirroring what
it already did for an ephemeral workspace; `resolveWikiDir` performs the same redirect, because
if the two disagree a search returns user slugs that read back as a missing directory.

`ast.ImportedContext` gained `Version` and `Reference`, so `ast_list` with no `project_dir` tells
the caller exactly what to pass as `context` — otherwise the version is only inferable from the
store path.

### T7 — `hub_content` (`internal/hub/content.go`, `internal/mcpstdio/tools_hub.go`)

Serves `rule`, `skill`, `command`, `agent`. Returns `files` as a **map keyed by the
artifact-relative path** in slash form, plus `canonical` (the `SKILL.md`/`RULE.md`/`COMMAND.md`/
`AGENT.md` entry point) and a `notice` naming the version when the reference did not.

- **Nothing is downloaded.** An artifact that is not installed is reported as such, naming
  `hub install` — fetching silently would turn a read into a network write against a Hub the
  caller may not have configured.
- `project_dir` **narrows** rather than enables: with one, that project's claim pins the version
  and a mismatched `@version` is refused; a linked artifact is read from its source directory,
  because the clone would answer with what was published before the link.
- `ast`/`knowledge` are refused **by name**, pointing at `ast_source`/`wiki_source`. An empty
  answer would read as "the artifact is empty".
- Path safety is lexical **and** post-`EvalSymlinks`: a symlink inside a clone must not turn this
  into a way to read arbitrary files off the machine running the server.
- A non-UTF8 file is listed with a marker instead of its bytes — its presence is information, its
  bytes are not. Files over 512 KiB are marked too, escapable with `path`, which has no cap.

## Use Cases

### UC-01: Install a Hub artifact with no project
- **Actor**: An external agent over MCP HTTP, with no checkout on this machine.
- **Preconditions**: The Hub is configured and reachable; the artifact exists in the registry.
- **Main Flow**:
  1. Agent calls `graphit_hub_install` with `id: "<artifact>@<version>"` and **no** `project_dir`.
  2. `hub.Install` resolves the version against the registry entry.
  3. For `ast`, `ensureASTStore` mounts the published graph into `store.ASTHubDir(contextID, version)`.
     For `knowledge`, the wiki lands in `store.KnowledgeHubDir(contextName, version)`.
     For `rule`/`skill`/`command`/`agent`, `EnsureArtifactClone` populates the global clone cache.
  4. The install is recorded in `<global>/global.lock.json` under the `_global` project key.
  5. No project lockfile is read or written, and nothing is placed in any project directory.
- **Alternative Flows**:
  - `project_dir` supplied: the pre-existing project-scoped behaviour, unchanged.
  - No version in `id`: the registry entry's latest is resolved, exactly as today.
- **Error Scenarios**:
  - Hub unreachable → the existing "is the registry accessible?" error.
  - Store build fails → error returned and no global-lock entry is left behind.
- **Postconditions**: The artifact's global store exists and is addressable by `id@version`.
- **Affected Files**: `internal/hub/service.go`, `internal/hub/global_lock.go`, `internal/mcpstdio/tools_hub.go`.

### UC-02: Query a globally installed AST artifact with no project
- **Actor**: An external agent over MCP HTTP.
- **Preconditions**: UC-01 completed for an `ast` artifact.
- **Main Flow**:
  1. Agent calls `graphit_ast_schema` with `context: "<id>@<version>"` and no `project_dir`.
  2. `store.LookupContext("", "ast", name)` resolves from the global lock.
  3. `store.ASTHubDir` yields the mounted catalog; the graph opens read-only.
  4. Agent calls `graphit_ast_query` / `graphit_ast_search` / `graphit_ast_source` the same way.
- **Alternative Flows**: `project_dir` given → project lockfile resolution, unchanged.
- **Error Scenarios**:
  - No `project_dir` **and** no `context` → refused, naming that a project-less call must
    address an artifact by its qualified identifier.
  - `context` not in the global lock → refused, listing what is globally installed.
  - Write-path tool in global scope → refused (opening read-write would create a store).
- **Postconditions**: No store is created; nothing is written.
- **Affected Files**: `internal/store/globalcontexts.go`, `internal/store/registry.go`, `internal/mcpstdio/tools_ast.go`, `internal/mcpstdio/context.go`.

### UC-03: Search and read a globally installed knowledge artifact with no project
- **Actor**: An external agent over MCP HTTP.
- **Preconditions**: UC-01 completed for a `knowledge` artifact.
- **Main Flow**:
  1. `graphit_knowledge_search` with `context: "<id>@<version>"`, no `project_dir` → ranked titles.
  2. `graphit_wiki_source` with the chosen slug and the same `context` → page text.
- **Alternative Flows**: `graphit_wiki_browse` for the catalogue; `graphit_wiki_xrefs` for provenance.
- **Error Scenarios**: As UC-02 — missing context refused; write/index operations project-only.
- **Postconditions**: Read-only; nothing compiled or written.
- **Affected Files**: `internal/mcpstdio/tools_knowledge.go`, `internal/mcpstdio/tools_wiki.go`, `internal/mcpstdio/context.go`.

### UC-04: Use memory with no project
- **Actor**: An external agent over MCP HTTP.
- **Preconditions**: None beyond a resolvable machine identity.
- **Main Flow**:
  1. `graphit_memory_search` with no `project_dir` → the `user` scope is served.
  2. `graphit_wiki_source` with `wiki: "memory"` and no `project_dir` → the picked memory.
- **Alternative Flows**: A named memory context is served when passed as the scope/context.
- **Error Scenarios**: `scope: "project"` with no `project_dir` → refused, naming that a
  project scope is keyed by a project identity that a project-less call does not have.
- **Postconditions**: No scope is created for a directory nobody owns.
- **Affected Files**: `internal/mcpstdio/tools_memory.go`, `internal/mcpstdio/context.go`.

### UC-05: Retrieve the content of a rule, skill, command or agent artifact
- **Actor**: Any agent, with or without a project.
- **Preconditions**: The artifact is installed (globally, or in the given project).
- **Main Flow**:
  1. Agent calls `graphit_hub_content` with `id: "<artifact>@<version>"`.
  2. The clone directory is resolved — from the project's claim when `project_dir` is given,
     from the global lock otherwise.
  3. Every file under it is read and returned in a map **keyed by its artifact-relative path**.
- **Alternative Flows**:
  - `path` given → that one file only, same map shape with a single entry.
  - No version → the installed version is used; when several are installed globally, the
    latest is chosen and the choice is stated.
- **Error Scenarios**:
  - Type is `ast` or `knowledge` → refused by name, pointing at `ast_source` / `wiki_source`.
  - Artifact not installed → refused, naming `hub_install` as the missing step.
  - Non-text file → listed with a marker instead of its bytes.
- **Postconditions**: Read-only.
- **Affected Files**: `internal/mcpstdio/tools_hub.go`, `internal/hub` (clone-dir resolution helper).

## Test Cases & Acceptance Criteria

### Feature: Project-less Hub install
Ref: UC-01

#### Scenario: Installing a file artifact with no project leaves nothing in any project
```gherkin
Given an isolated HOME with a configured hub registry
  And an artifact "demo-skill" published at version "1.0.0" of type "skill"
When hub.Install is called with projectDir "" and id "demo-skill@1.0.0"
Then the global clone cache for "skill/_global/demo-skill/1.0.0" is populated
  And "<global>/global.lock.json" holds an artifact entry keyed "skill/demo-skill@1.0.0"
  And that entry's projects map contains the key "_global"
  And no lockfile is created or modified anywhere under the working directory
```

#### Scenario: A failed store build leaves no global-lock claim
```gherkin
Given an isolated HOME with a hub registry whose published schema object is missing
When hub.Install is called with projectDir "" and id "demo-ast@1.0.0"
Then the call returns an error naming the unreadable published schema
  And "<global>/global.lock.json" holds no artifact entry for "ast/demo-ast@1.0.0"
```

### Feature: Global context resolution
Ref: UC-02, UC-03

#### Scenario Outline: A qualified identifier resolves to the shared versioned store
```gherkin
Given a global lock recording an install of "<id>" at version "<version>" of kind "<kind>"
When LookupContext is called with an empty projectDir, kind "<kind>" and name "<name>"
Then it reports found
  And the resolved record's version is "<version>"

Examples:
  | kind      | id       | version | name           |
  | ast       | demo-ast | 2.1.0   | demo-ast       |
  | ast       | demo-ast | 2.1.0   | demo-ast@2.1.0 |
  | knowledge | demo-kb  | 1.0.0   | demo-kb        |
  | knowledge | demo-kb  | 1.0.0   | demo-kb@1.0.0  |
```

#### Scenario: A project that never installed an artifact cannot reach it globally
```gherkin
Given a global lock recording an install of "demo-ast" at version "2.1.0"
  And a project whose lockfile claims no ast context
When LookupContext is called with that project's directory, kind "ast" and name "demo-ast"
Then it reports not found
```

### Feature: Project-less MCP queries
Ref: UC-02, UC-03, UC-04

#### Scenario: An AST query with no project and a qualified context returns rows
```gherkin
Given a globally installed ast artifact "demo-ast" at version "2.1.0"
When graphit_ast_query is called with no project_dir and context "demo-ast@2.1.0"
Then the query executes against the shared versioned store
  And the result is the rows of that graph
```

#### Scenario: An AST query with neither project_dir nor context is refused
```gherkin
Given a server with no project_dir supplied
When graphit_ast_query is called with no project_dir and no context
Then the call fails
  And the error states that a project-less call must name an artifact by its qualified identifier
```

#### Scenario: A write-path AST tool is refused in global scope
```gherkin
Given a globally installed ast artifact "demo-ast" at version "2.1.0"
When graphit_ast_index is called with no project_dir
Then the call fails
  And no store directory is created
```

#### Scenario: Memory with no project serves the user scope
```gherkin
Given an isolated HOME with a resolvable machine identity
When graphit_memory_search is called with no project_dir and no scope
Then the user scope is searched
  And the response states that the user scope was served
```

#### Scenario: Project-scoped memory with no project is refused
```gherkin
Given an isolated HOME
When graphit_memory_search is called with no project_dir and scope "project"
Then the call fails
  And the error states that a project scope needs a project identity
```

### Feature: Artifact content retrieval
Ref: UC-05

#### Scenario: A multi-file skill returns every file keyed by path
```gherkin
Given a globally installed skill artifact "demo-skill" at version "1.0.0"
  And that artifact holds "SKILL.md" and "reference/patterns.md"
When graphit_hub_content is called with id "demo-skill@1.0.0"
Then the response is a map with exactly the keys "SKILL.md" and "reference/patterns.md"
  And each value is the text of that file
```

#### Scenario: A single path returns one entry
```gherkin
Given a globally installed skill artifact "demo-skill" at version "1.0.0" holding "SKILL.md" and "reference/patterns.md"
When graphit_hub_content is called with id "demo-skill@1.0.0" and path "SKILL.md"
Then the response is a map with exactly the key "SKILL.md"
```

#### Scenario Outline: The four content types are served and the mounted ones are refused
```gherkin
Given a globally installed artifact "demo" at version "1.0.0" of type "<type>"
When graphit_hub_content is called with id "demo@1.0.0" and type "<type>"
Then the call "<outcome>"

Examples:
  | type      | outcome                                              |
  | rule      | returns a map keyed by path                          |
  | skill     | returns a map keyed by path                          |
  | command   | returns a map keyed by path                          |
  | agent     | returns a map keyed by path                          |
  | ast       | fails naming ast_source as the tool to use instead   |
  | knowledge | fails naming wiki_source as the tool to use instead  |
```

#### Scenario: An artifact that is not installed is refused with the missing step named
```gherkin
Given an isolated HOME with no artifacts installed
When graphit_hub_content is called with id "demo-skill@1.0.0"
Then the call fails
  And the error names hub_install as the step that has not been performed
```

#### Scenario: A non-text file is listed rather than returned as bytes
```gherkin
Given a globally installed skill artifact "demo-skill" at version "1.0.0"
  And that artifact holds "SKILL.md" and a binary file "assets/logo.png"
When graphit_hub_content is called with id "demo-skill@1.0.0"
Then the value for "SKILL.md" is its text
  And the value for "assets/logo.png" is a marker stating the file is not text
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/projectless-hub-installs-and-artifact-content.md` | Created | This log, opened before the work |
| `internal/store/globalcontexts.go` | Created | The global membership registry, read side: `GlobalOwnerKey`, `ListGlobalContexts`, `LookupGlobalContext`, `SplitQualified` |
| `internal/store/globalcontexts_test.go` | Created | Qualified-identifier resolution, the highest-version rule, and the two negatives: a project-only install is not global, and a project cannot borrow one |
| `internal/store/registry.go` | Modified | `LookupContext`/`ListContexts` fall back to the global registry when `projectDir == ""` — the single change that gives every resolver the project-less mode |
| `internal/hub/global_lock.go` | Modified | `GlobalArtifact.ProjectID` (the publisher); `RegisterInstall` takes `InstallRecord`; `ValidateProjectDirs` no longer prunes owners that have no directory |
| `internal/hub/global_lock_test.go` | Modified | Updated to the `InstallRecord` parameter |
| `internal/hub/service.go` | Modified | `globalInstall` path in `Install`; `UninstallGlobal`, `findGlobalInstall`, `GlobalInstalls`; `transientOwnerKey`; dependency check reads whichever record applies |
| `internal/hub/content.go` | Created | `ArtifactContentFor` — the path-keyed read of a rule/skill/command/agent, with the project optional |
| `internal/hub/content_test.go` | Created | Multi-file skill, single `path`, escape attempts, the four served types, mounted-type refusals, ambiguity, binary marker, version reporting |
| `internal/hub/globalinstall_test.go` | Created | A project-less install records globally and touches no project; a project-scoped one does not claim the global owner; uninstall; the `ValidateProjectDirs` guard; the cross-package lock shape |
| `internal/hub/rule.go` | Modified | Hub skill: the project-less install, the qualified identifier, `hub_content`'s path-keyed shape, and the global scope's limits |
| `internal/hub/rule_test.go` | Modified | `content` added to the tool-coverage list; three new assertions on the text above |
| `internal/ast/config.go` | Modified | `ImportedContext.Version` and `.Reference`, so `ast_list` tells a project-less caller what to pass as `context` |
| `internal/ast/rule.go` | Modified | AST skill: the project-less query row and section, and the three constraints that come with it |
| `internal/knowledge/rule.go` | Modified | Knowledge skill: reading a Hub artifact with no project, and the memory-wiki exception |
| `internal/memory/rule.go` | Modified | Memory skill: `project_dir` optional, the user-scope redirect, and what still needs a project |
| `internal/mcpstdio/context.go` | Modified | `resolveProjectDirOptional`, `resolveArtifactScope`, `resolveWikiScope`, `errGlobalScopeIsReadOnly`; empty-path guards on `anchorToProject`, `withProjectDir`, `loadProjectConfig`, `loadProjectLockInfo`, `openASTDBReadWrite`, `memoryScopeFor`, `resolveWikiDir` |
| `internal/mcpstdio/tools_hub.go` | Modified | `hub_content`; `hub_install`/`hub_uninstall` accept an absent `project_dir` |
| `internal/mcpstdio/tools_ast.go` | Modified | `ast_query`/`schema`/`search`/`source` use `resolveArtifactScope`; `ast_list` uses the optional resolver |
| `internal/mcpstdio/tools_knowledge.go` | Modified | `knowledge_search` uses `resolveArtifactScope` |
| `internal/mcpstdio/tools_wiki.go` | Modified | `wiki_browse`/`log`/`xrefs`/`source` use `resolveWikiScope`; `wiki_search` restricted to memory sources in the global scope |
| `internal/mcpstdio/tools_memory.go` | Modified | Nine scope tools accept an absent `project_dir`; schema descriptions updated |
| `internal/mcpstdio/globalscope_test.go` | Created | The resolvers, the read-only refusal, the config-leak guard, the user-scope redirect, and the `anchorToProject`/`withProjectDir` guards |

## Trade-offs & Decisions

- **The global fallback fires only on an absent `project_dir`, never as a second chance for
  a project.** A project that did not install an artifact must not be able to query it;
  membership is the whole point of the per-project record. Accepting: an agent working in a
  project cannot reach a globally installed artifact without installing it there too.
- **`internal/store` reads `global.lock.json` with an anonymous struct** rather than
  importing `internal/hub`. Keeps `store` a leaf, which is what lets `ast`, `knowledge` and
  `memory` depend on it. Accepting: the shape is described in two places, and a change to
  the global lock's artifact fields has to be mirrored. Mitigated by keeping the read
  narrow — id, version, type, and the projects map keys.
- **Global scope is read-only for the AST graph.** Opening read-write creates a store, and
  a project-less caller has no identity to key one by. Same reasoning that already refuses
  an ephemeral project a graph of its own.
- **`hub_content` covers exactly `rule`, `skill`, `command` and `agent`.** `mcp`, `workflow`
  and `language` are file-based too and would technically work, but they were not asked for
  and each has a different consumer; `ast` and `knowledge` are refused by name because they
  are mounted, not downloaded, and have no file tree.

## Technical Debt

- [x] The global lock's `Projects` map now carries a `_global` pseudo-key. Checked:
  `RegisterUninstall` takes the owner as a parameter and is fine; `GCOrphans` keys on an empty
  owner set and is fine; `ValidateProjectDirs` was **broken** and is fixed (it pruned owners with
  no directory, which already affected `__transient__`); `hub_projects` reads `lock.Projects`, a
  different map, and is unaffected.
- [ ] **A query against a genuinely published artifact has not been run** — see the completion
  entry in the Progress Log. Publish something to the local hub and repeat the project-less
  `ast_query`/`knowledge_search`, expecting rows rather than a path in an error.
- [ ] `hub_update` has no global scope. A globally installed artifact is updated by installing the
  newer version, which leaves the older one installed alongside it — after which an unqualified
  reference silently means the newer one. Deliberate for now (the highest version is the sane
  default), but `hub_update` with no `project_dir` should eventually replace rather than accumulate.
- [ ] `hub_content` serves four types. `mcp` and `workflow` are file-based too and would work;
  they were not asked for and each has a different consumer, so they are refused by the generic
  branch. Revisit if a caller needs to read an `mcp` artifact's server declaration.
- [ ] The global-lock artifact shape is described in two places — `hub.GlobalArtifact` and the
  anonymous struct in `store.globalArtifactRecord`. `TestTheGlobalLockShapeIsWhatTheStoreReaderExpects`
  fails if the field names drift, which is the mitigation, not a fix. Collapsing them needs the
  shape moved to a leaf package (as `projectlock` was for the project lockfile).
- [ ] `wiki_search` in the global scope accepts only `wikis: ["memory"]`. A globally installed
  knowledge artifact is searchable through `knowledge_search` with a `context`, but not through
  the multi-source tool alongside memory. `hub_refs` looks like the right seam for it.

## System Knowledge

- `paths.GetPathsForProject("", "")` falls through to `paths.GetPaths`, which **walks up
  from the process working directory**. Calling it on a project-less path silently binds
  the operation to whatever project the server happens to sit in. This is the same trap
  recorded in project memory about MCP tools and `paths.GetPaths`.
- `hub.ArtifactPrefix` omits the artifact id for mountable types (`ast`, `knowledge`)
  because a project publishes exactly one of each; file types keep the id in the path.
- `store.ContextNameFor(artifactID, projectID)` is why a Hub context is usually known by
  the **publishing project's id**, not by the artifact id — an artifact published outside
  any project falls back to the artifact id. A global lookup must accept both.
- The IDE-materialisation branch in `hub.Install` is the only place a file artifact is
  copied into a project. Without an IDE (or without a project) the clone cache is the only
  copy — which is exactly what makes `hub_content` possible project-lessly.
- **`filepath.Join("", x)` is the recurring hazard in this whole area**, and it never errors.
  It yields a RELATIVE path, which then resolves against the server's working directory. Four
  separate call sites had it (`anchorToProject`, `loadProjectConfig`, `loadProjectLockInfo`,
  `ValidateProjectDirs`), and in every one the symptom would have been a confident answer about
  the wrong project rather than a failure. Any new code path that accepts an empty `projectDir`
  needs this checked explicitly.
- **`__transient__` was already a project-less owner in the global lock** before this task —
  written by `EnsureKnowledgeAvailable` — and `ValidateProjectDirs` was quietly deleting its
  entries on every validation pass. The reserved-owner pattern was therefore not new; it was
  new only in being correct.
- A Hub AST context's `IcebugDir` **is** its store directory, not `<store>/graph.icebug` as for a
  project's own graph: what is staged there is the mounted DDL. A test asserting the resolved path
  has to expect `ASTHubDir(...)`, not `ASTHubIcebugDir(...)`.
- `RegistryManager` can be constructed in tests with `entries`/`projects` maps and no S3 store, in
  which case `IsReady()` is false and `Install` skips every clone — which is what makes the
  membership half of an install testable without a network or a published artifact.
- `graphit init` in an isolated `HOME` requires `graphit setup` to have run, and with only
  `config.json` copied in it hangs rather than failing. Worth knowing before reaching for the CLI
  to build an end-to-end fixture; driving `mcp --stdio` directly is faster and does not need init.

## Progress Log

### 2026-09-01 — completion

All nine tasks landed. Verification actually performed:

- `go build ./...` clean; `go test ./internal/... ./cmd/...` fully green; `golangci-lint run ./...`
  reports **0 issues**.
- **End-to-end against the real MCP server** (`.build/graphit-local mcp --stdio`, isolated
  `HOME`/`USERPROFILE`, global install seeded on disk because the local hub registry has nothing
  published): 24 checks, all passing — `hub_content` registered; a multi-file skill returned with
  exactly the three expected path keys and their text; the binary file marked rather than
  returned; `canonical` and the version reported; a single `path` returning one entry; an escape
  path refused; an `ast` id refused; an uninstalled id refused; `ast_list` working project-lessly;
  `ast_query`/`knowledge_search` refused with neither project nor context; `wiki_search` refused
  over a project source; `ast_index` refused; `memory_insert`/`list`/`search` working with the
  user-scope redirect reported; and no lockfile left in the working directory.
- **Store resolution confirmed through the server too**: with a global lock entry present but the
  store deliberately unbuilt, `ast_schema` for `demo-ast@2.1.0`, `demo-ast`, `01PUBLISHER` and
  `01PUBLISHER@2.1.0` all name the absolute shared path
  `<global>/ast/hub/01publisher/2.1.0`, while an uninstalled version and a project-only install
  do not resolve there.

**What was NOT verified, and why:** a query returning rows from a genuinely published AST or
knowledge artifact. The local hub has no published artifacts, and `graphit init` in an isolated
`HOME` hangs before an artifact could be published to it. The two halves either side of that gap
are covered — which directory a global reference resolves to (above, through the server) and
opening a graph from a directory (unchanged code, existing tests) — but the composed path has not
been exercised against real published bytes. Worth doing once something is published to the local
hub.

### 2026-09-01 — planning
- Read the relevant code before planning: `internal/hub/service.go` (`Install`),
  `internal/hub/global_lock.go`, `internal/hub/ast_store.go`, `internal/hub/registry.go`
  and `internal/hub/s3_store.go` (clone cache and artifact prefixes), `internal/store/store.go`,
  `internal/store/registry.go`, `internal/store/contextpaths.go`, `internal/mcpstdio/context.go`,
  `internal/mcpstdio/server.go`, `internal/mcpstdio/tools_hub.go`, `internal/ast/hubstore.go`,
  `internal/ast/ladybug.go`, `internal/knowledge/paths.go`, `internal/memory/paths.go`.
- Established that no data is per-project — only membership is — which is what makes the
  global fallback a single-lookup change instead of a new store layout.
- Opened this log with the plan above. Next: T1, the global context registry read side.
- Note for whoever resumes: the IDE's stdio MCP connection was down for this session, so
  the graphit MCP tools were reached through the daemon's HTTP endpoint
  (`~/.graphit/daemon/mcp.port` + `mcp.key`, streamable-HTTP handshake). Same tools, same
  arguments — not a fallback to native tooling.
