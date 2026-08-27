---
title: Centralize AST, knowledge and memory stores in the global directory
status: done
created: 2026-08-14
updated: 2026-08-14
tags: [storage, ast, knowledge, memory, hub, mcp, skills, refactor]
---

# Centralize AST, knowledge and memory stores in the global directory

## Objective

Every compiled artifact of the framework was stored twice: once in the global brand
directory and once inside each project that read it. AST stores had a project-side
symlink per imported context, knowledge contexts were **copied** into
`<project>/.graphit/knowledge/<name>`, the project's own graph and wiki were compiled
into `<project>/.graphit/`, and the memory wiki was compiled globally and then
replicated into every project — with a daemon fan-out pass whose only job was to stop
the copies from disagreeing.

The goal: **one location per artifact, in the global directory, identified by an id.**
No replication, no sync logic to keep copies in step, no duplicated disk. Since a
store is never read as files by an agent, moving it out of the project costs nothing
and removes a whole class of bug — a project answering from a replica nobody
refreshed, indistinguishable from a project with less data.

Two obligations came with it, both from the user:

1. The MCP surface must cover every read, so the agent never needs a file outside its
   workspace. Reading a wiki page in particular had to become a tool with the same
   slicing as the code-source tool (`head`, `tail`, line ranges, `pattern` with
   `before`/`after`).
2. The AST skill must instruct the agent to call the AST index tool once at the end of
   a session that changed code, so the next session opens a current graph.

No backward compatibility was required — the project is in development.

## Implementation Details

### The new resolver: `internal/store`

A leaf package (imports only `internal/brand`) that owns every store path. It is the
single place these paths are composed; nothing else builds them.

- `store.go` — `Root`, `ProjectID`, `ProjectStoreID`, `SanitizeName`,
  `DefuseReservedName`, `SanitizeSegment`, and the resolvers:
  `ASTRoot`/`ASTProjectDir`/`ASTProjectDirByID`/`ASTProjectDBPath`/`ASTContextDir`/
  `ASTContextDBPath`/`ASTHubRoot`/`ASTHubDir`/`ASTHubDBPath`,
  `KnowledgeRoot`/`KnowledgeProjectDir`/`KnowledgeProjectDirByID`/
  `KnowledgeContextDir`/`KnowledgeContextDocsDir`,
  `MemoryRoot`/`MemoryWikiDir`/`MemoryWorktreeRoot`/`MemoryWorktreeDir`.
- `registry.go` — the per-project context registry at `<project>/.graphit/contexts.json`:
  `ListContexts`, `ContextNames`, `LookupContext`, `HasContext`, `AddContext`,
  `RemoveContext`, over `ContextRecord{Name, SourcePath, DBPath, ImportedAt}` keyed by
  sanitised name under a kind (`KindAST`, `KindKnowledge`).

It is a leaf package on purpose: `ast`, `knowledge` and `memory` all need it, and
`hub` imports all three, so the lockfile is parsed directly here rather than through
`internal/hub`.

The layout it produces is documented in
[docs/architecture/storage_layout.md](../architecture/storage_layout.md).

### AST

| File | Change |
|---|---|
| `internal/ast/hubstore.go` | delegates to `store.ASTHubRoot/Dir/DBPath`; local `defuseReservedName`, `sanitizePathSegment`, `windowsReservedNames`, `hubContextsDirName` deleted |
| `internal/ast/config.go` | deleted `Config`, `DefaultConfig`, `LoadConfig`, `SaveConfig`, `configDir`, `configFile`, `globalASTContextDir`, `astContextProjectLinkDir`, `reservedASTDirName`. `ContextDBPathIn` = lockfile → registry `DBPath` → `store.ASTContextDBPath`. `AddImportedContext(projectDir, name, sourcePath)` no longer symlinks; new `LinkImportedContext(projectDir, name, dbPath)`; `RemoveImportedContext(projectDir, name)`; `ListImportedContextsIn` reads lockfile + registry with no directory scan |
| `internal/ast/ladybug.go` | new `LadybugConfigFor(projectDir)` returning an **absolute** path from `store.ASTProjectDBPath`; `DefaultLadybugConfig` is the cwd form |
| `internal/ast/server.go` | four hard-coded `<root>/.graphit/ast/project/ladybugdb` literals replaced; legacy `.graphit/ast/imports` scan removed; `handleDeleteContext` passes the requested root |
| `internal/daemon/syncmodule.go`, `cmd/graphit/commands/daemon.go` | use `ast.LadybugConfigFor` / `store.ASTProjectDir` |

`.graphit/ast/queries/` **stays in the project**: grammar query files are source, not
a compiled store, and a project versions its own.

### Knowledge

`internal/knowledge/paths.go` was rewritten:

- `WikiDirFor(projectDir)` / `WikiDir()` — the project's wiki, in the global store.
- `WikiDirForContextIn(projectDir, name)` — resolves a context, preferring the
  `wiki/` subdirectory when that is the one holding an `index.md`. A published Hub
  artifact carries `docs/` and `wiki/`; a locally built context compiles straight into
  the context directory. Both layouts read, resolved once here instead of probed again
  by every reader.
- `ContextWriteDir(name)` / `ContextDocsDir(name)` — where a context is compiled and
  where its markdown is extracted first.
- `InstalledContextsIn(projectDir)` — from the registry, filtered by "its wiki exists".
- `EnsureContextCopy` and `globalKnowledgeContextDir` deleted.

`internal/knowledge/wiki.go` gained `WikiScope.Subdirs`: a whitelist of directories to
walk under one root, with `walkRoots` replacing the single `walkRoot`. This is what
lets the live search compile exactly the documentation sets a user selected out of a
shared wiki root without copying them somewhere it can point a single root at.

`ContextDocsDir` also fixes a latent bug: the extraction directory used to be one
shared `<global>/knowledge/docs` for **every** context, so each import compiled the
previous import's pages.

### Memory

- `internal/memory/replicate.go` **deleted** (`ReplicateWikiToProjects`, `HasReplica`,
  `walSidecar`, `caseInsensitivePaths`).
- `internal/memory/paths.go` rewritten: `WikiDirFor(projectDir, scope)`,
  `RawDirFor`, `resolveScopeIDIn` (project id via `store.ProjectID`),
  `ContextNamesFrom` / `AllContextDirs` reading the worktree root. Deleted
  `ProjectReplicaDir`, `ProjectLinkDir`, `EnsureContextCopy`.
- `internal/memory/memory.go`: `MemoryService` lost `projectLinkDir`;
  `ensureProjectCopy` → `ensureWikiDir`; `MemoryLocalDir`, `MemoryProjectLinkDir`,
  `MemoryGlobalContextDir` deleted.
- `internal/memory/cycle.go`: `CycleResult.ReplicaErr` gone; `runScopeCycle` just
  compiles.
- `internal/daemon/memorysyncmodule.go`: `fanOut` and `ReplicateMemoryScope` deleted.
- `internal/daemon/adapters.go`: `WikiEmbedTargets` no longer needs `OnEmbedded`
  callbacks to push fresh vectors outward.

### Hub

- Knowledge install copies the clone into `store.KnowledgeContextDir` once per machine
  and records the claim with `store.AddContext`.
- AST install no longer removes a legacy project-side link (there is none);
  `removeLegacyASTContextLink` deleted.
- `Link` for AST and knowledge records a **pointer** to the source project's global
  store instead of creating a symlink or copying pages, so a link never goes stale
  when the source reindexes. It now requires the source project to have been indexed,
  and says so.
- `Unlink` and `preUninstallHook` drop registry claims; shared stores are collected
  only when the global lock reports the artifact orphaned.

### MCP

- `resolveWikiDir` no longer chdirs or anchors: every wiki is resolved from an
  explicit `projectDir`.
- `wiki_browse`, `wiki_log`, `wiki_xrefs` gained a `context` parameter, routed through
  new `resolveWikiScopeDirContext` / `openWikiForReadContext`.
- `wiki_source` already carried the full slicing set (`head`, `tail`, `start_line`,
  `end_line`, `line_numbers`, `pattern`, `regex`, `before`, `after`) plus `wiki` and
  `context`; its description now states that it is the **only** way to read a page.
- `ast_index`'s description names the end-of-session call; `ast_source`'s says it is
  the only way to read the source of an imported context or another project.

### Skills (`rule.go`)

- **AST**: a new mandatory section — one `graphit_ast_index` at the end of a session
  that changed code, with the distinction spelled out (mid-session: nothing, the
  watcher has it; end of session: one call, because the watcher is reliable but is not
  a check you performed). Plus a section on where the graph lives and why it cannot be
  opened as a file. Two new mandate triggers.
- **Knowledge**: "There is no wiki inside the project — the page file is not yours to
  open", covering the global layout, `wiki_source` as the only reader with its slicing
  options, registry-based membership, and the fact that *writing* documentation is
  unchanged.
- **Memory**: "There is no memory directory in the project", plus corrected
  anti-pattern and retrieval steps.

## Use Cases

### UC-01: Index a project's code graph
- **Actor**: developer (CLI `graphit ast index`) or agent (`graphit_ast_index`)
- **Preconditions**: the project directory exists. A lockfile is not required.
- **Main Flow**:
  1. The caller resolves the store with `ast.LadybugConfigFor(projectDir)`, which
     returns `store.ASTProjectDBPath(projectDir)` — absolute.
  2. `store.ProjectStoreID` reads the lockfile ULID, or falls back to
     `path-<16 hex of the absolute path>`.
  3. The pipeline writes the graph, `manifest.json` and `shards/` into that directory.
- **Alternative Flows**:
  - `LADYBUGDB_PATH` set: overrides the resolved path wholesale.
  - `--reset`: `os.RemoveAll` of the store directory before indexing.
- **Error Scenarios**:
  - No home directory: `globalOr` degrades to `<projectDir>/.graphit/…`. Nothing is
    shared in that state, which is the honest outcome, but indexing still works.
- **Postconditions**: exactly one store exists for the project; nothing was written
  into the project except logs.
- **Affected Files**: `internal/store/store.go`, `internal/ast/ladybug.go`,
  `cmd/graphit/commands/runners.go`, `internal/daemon/syncmodule.go`

### UC-02: Import a local repository as an AST context
- **Actor**: developer (`graphit ast install`) or agent (`graphit_ast_install`)
- **Preconditions**: the source path exists on this machine.
- **Main Flow**:
  1. `ast.AddImportedContext(projectDir, name, absSource)` creates
     `store.ASTContextDir(name)` and records the claim in the project's registry.
  2. The pipeline indexes the source into that store.
- **Alternative Flows**:
  - Another project imported the same name already: the store is reused, and only the
    new project's registry entry is added.
  - `reset: true`: the store directory is removed first.
- **Error Scenarios**:
  - The store cannot be created: the error is returned and nothing is registered.
- **Postconditions**: one store per name; each project that may query it holds a
  registry entry.
- **Affected Files**: `internal/ast/config.go`, `internal/store/registry.go`,
  `internal/mcpstdio/tools_ast.go`, `cmd/graphit/commands/runners.go`

### UC-03: Read a wiki page
- **Actor**: agent (`graphit_wiki_source`)
- **Preconditions**: the wiki has been compiled.
- **Main Flow**:
  1. `resolveWikiDir(module, projectDir, contextName)` resolves the global directory.
  2. `wiki.ReadPage` resolves the slug inside it — refusing anything that escapes —
     and applies the `textslice.Request`.
- **Alternative Flows**:
  - `wiki: "memory"` reads a memory scope; `context` names an imported context (for
    knowledge) or a memory scope other than `project`.
  - A different `project_dir` reads a sibling project's page.
- **Error Scenarios**:
  - Unknown slug: the error lists the pages that do exist (capped at 40).
  - A reference escaping the wiki directory: refused, with its own reason, and
    deliberately **not** accompanied by a page list.
  - Wiki never built: "wiki directory not found — the wiki may not have been built yet".
- **Postconditions**: none; the read is pure.
- **Affected Files**: `internal/mcpstdio/tools_wiki.go`, `internal/wiki/source.go`,
  `internal/mcpstdio/context.go`

### UC-04: Record and recall a memory
- **Actor**: agent (`graphit_memory_insert`, `graphit_memory_search`)
- **Preconditions**: for the project scope, the project has a lockfile id; for the
  user scope, git has a `user.email`.
- **Main Flow**:
  1. The memory is written to the git worktree at `store.MemoryWorktreeDir` and
     committed.
  2. The wiki is compiled into `store.MemoryWikiDir(scope, id)` — once, in place.
  3. `memory_search` opens that same directory.
- **Alternative Flows**:
  - The daemon's `MemorySyncModule` recompiles a scope when its worktree changes,
    including memories arriving from the remote. No fan-out follows.
- **Error Scenarios**:
  - No lockfile id: `WikiDirFor(projectDir, "project")` returns `""` and the caller
    reports that there is no project scope. This is the only case that legitimately
    resolves to nothing.
- **Postconditions**: one worktree and one wiki per scope; no project holds a copy.
- **Affected Files**: `internal/memory/paths.go`, `internal/memory/memory.go`,
  `internal/memory/cycle.go`, `internal/daemon/memorysyncmodule.go`

### UC-05: Link a sibling project's graph or wiki
- **Actor**: developer (`graphit hub link --type ast|knowledge`)
- **Preconditions**: the source project has been indexed, so its global store exists.
- **Main Flow**:
  1. The source store is resolved with `store.ASTProjectDBPath(absSource)` or
     `store.KnowledgeProjectDir(absSource)` and checked for existence.
  2. A registry record with an explicit `DBPath` is written for the consuming project.
- **Alternative Flows**: none — a link is always a pointer now.
- **Error Scenarios**:
  - Source not indexed: "source AST not found at … — index the source project first".
- **Postconditions**: the consuming project queries the sibling's store in place, and
  the link cannot go stale when the sibling reindexes.
- **Affected Files**: `internal/hub/service.go`, `internal/ast/config.go`

### UC-06: Prepare a live-search session
- **Actor**: the live search runtime
- **Preconditions**: Hub knowledge artifacts have been installed into the ephemeral
  workspace.
- **Main Flow**:
  1. `knowledge.InstalledContextsIn(ws)` lists the session's selected contexts from
     the workspace registry.
  2. One `RunIndexPipeline` pass over `store.KnowledgeRoot()` with
     `Scope.Subdirs` = each context's source directory, compiling into
     `store.KnowledgeProjectDir(ws)`.
  3. `prepareUserMemory` reports availability; it no longer copies anything.
- **Alternative Flows**:
  - A published artifact keeps its markdown under `docs/`, which
    `contextSourceDir` prefers so its shipped `wiki/` pages are not compiled a second
    time.
- **Error Scenarios**:
  - Nothing selected: no wiki is created and no compilation is announced.
- **Postconditions**: the session's wiki covers exactly the selected sets, and no
  documentation another project installed leaks into it.
- **Affected Files**: `internal/livesearch/prep/index.go`,
  `internal/knowledge/wiki.go`

## Test Cases & Acceptance Criteria

### Feature: Global store layout
Ref: UC-01, UC-04

#### Scenario: every store resolves under the global directory
```gherkin
Given a project directory whose lockfile carries the id "01ACME"
  And HOME points at a scratch directory
When each store path is resolved
Then every one of them is under "<HOME>/.graphit"
  And none of them is under the project directory
  And the AST graph is at "<HOME>/.graphit/ast/project/01ACME/ladybugdb"
  And the documentation wiki is at "<HOME>/.graphit/wiki/knowledge/project/01ACME"
```

#### Scenario: an uninitialised project still gets a stable store id
```gherkin
Given two different directories with no lockfile
When their store ids are resolved
Then each id begins with "path-"
  And resolving the same directory twice gives the same id
  And the two directories do not collide on one id
```

#### Scenario Outline: a version string survives as a directory name
```gherkin
Given a version "<input>"
When it is sanitised for use as a path segment
Then the result is "<output>"

Examples:
  | input         | output        |
  | 1.2.3         | 1.2.3         |
  | v1.0.0-beta.1 | v1.0.0-beta.1 |
  | 1.0.0+build7  | 1.0.0+build7  |
  | ../../etc     | etc           |
  | a\b           | a-b           |
  |               | unversioned   |
  | ...           | unversioned   |
```

### Feature: Per-project context membership
Ref: UC-02, UC-05

#### Scenario: a context is listed only where it was claimed
```gherkin
Given project A has imported the context "real-context"
  And that context's store has been built
  And project B has imported nothing
When each project lists its imported contexts
Then project A lists "real-context"
  And project B lists nothing
```

#### Scenario: a registered context whose store was never built is not offered
```gherkin
Given a project has registered the context "never-built"
  And no store exists for it
When the project lists its imported contexts
Then "never-built" is absent
```

#### Scenario: dropping a claim leaves the shared store alone
```gherkin
Given project A has imported the context "real-context"
When project A removes that context
Then project A no longer lists it
  And the store still exists on disk
```

#### Scenario: a linked context resolves to the store it points at
```gherkin
Given a project has linked "sibling" to the graph "/elsewhere/ladybugdb"
When the context "sibling" is resolved
Then the answer is "/elsewhere/ladybugdb"
```

#### Scenario: linking a project that was never indexed fails with a reason
```gherkin
Given a source project whose global store does not exist
When it is linked as an AST artifact
Then the error names the store that was looked for
  And it says to index the source project first
```

### Feature: One project's request never resolves to another's store
Ref: UC-01, UC-03

#### Scenario: the requested project's graph is used, not the working directory's
```gherkin
Given the process is running in project B
When the AST config for project A is resolved
Then the DBPath is project A's store
  And it is not project B's store
  And it is an absolute path
```

#### Scenario: a write lands in the requested project's store
```gherkin
Given the process is running in project B
When a read-write graph is opened for project A and a table is created
Then the database exists in project A's store
  And no store was created for project B
  And no dot directory was created inside project A
```

#### Scenario: a missing database names the store that was looked for
```gherkin
Given project B has a graph and project A does not
  And the process is running in project B
When a read-only graph is opened for project A
Then the call fails
  And the error names project A's store path
```

### Feature: Memory has no project-local copy
Ref: UC-04

#### Scenario: a scope's wiki is global
```gherkin
Given a project whose lockfile carries an id
When its project memory wiki is resolved
Then the path is under the global directory
  And it is not under the project directory
```

#### Scenario: a project with no identity has no project scope
```gherkin
Given a project directory with no lockfile
When its project memory wiki is resolved
Then the result is empty
```

#### Scenario: an unknown scope name is a context and resolves anyway
```gherkin
Given the scope name "some-context"
When its wiki and raw directories are resolved
Then both resolve to paths
  And the raw path is under the memory worktree root
```

#### Scenario: imported memory contexts are recovered from the worktree names
```gherkin
Given worktrees named "memory-project-01ACME", "memory-user-abc123" and "memory-my-context-my-context"
When the imported memory contexts are listed
Then the result is exactly ["my-context"]
```

### Feature: Live search compiles only what was selected
Ref: UC-06

#### Scenario: a selected documentation set is searchable
```gherkin
Given a session workspace with the artifact "chosen-docs" installed and registered
When the session's indexes are prepared
Then wiki.db exists in the session's global wiki directory
  And it contains the pages of "chosen-docs"
  And nothing was compiled inside the workspace
```

#### Scenario: documentation nobody selected is not compiled in
```gherkin
Given the global wiki root also holds "someone-elses-docs", claimed by no one in this session
When the session's indexes are prepared
Then the compiled pages include "Chosen"
  And they do not include "Private"
```

#### Scenario: nothing selected produces no wiki and no complaint
```gherkin
Given a session workspace with no documentation artifacts
When the session's indexes are prepared
Then no wiki directory is created
  And no compilation is announced
```

#### Scenario: the user memory is read in place
```gherkin
Given a session workspace
When the user memory step runs
Then no memory directory is created inside the workspace
  And the step reports either the memory's availability or why it is absent
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/store/store.go` | Created | The single resolver for every store path |
| `internal/store/registry.go` | Created | Per-project context membership, replacing symlinks and copies |
| `internal/store/store_test.go` | Created | Layout, ids and sanitisers (cases moved from `ast/hubstore_test.go`) |
| `internal/store/registry_test.go` | Created | Registry round-trip, kind isolation, linked paths |
| `docs/architecture/storage_layout.md` | Created | The canonical description of the layout |
| `internal/ast/config.go` | Modified | Registry-based context resolution; the config-file machinery deleted |
| `internal/ast/hubstore.go` | Modified | Delegates to `store`; local sanitisers removed |
| `internal/ast/ladybug.go` | Modified | `LadybugConfigFor(projectDir)`, absolute paths |
| `internal/ast/server.go` | Modified | Global store paths; legacy imports scan removed |
| `internal/ast/rule.go` | Modified | End-of-session index call; where the graph lives |
| `internal/knowledge/paths.go` | Rewritten | Global wiki resolution; `EnsureContextCopy` deleted |
| `internal/knowledge/wiki.go` | Modified | `WikiScope.Subdirs` + `walkRoots` |
| `internal/knowledge/rule.go` | Modified | The page file is not the agent's to open |
| `internal/memory/paths.go` | Rewritten | Project-scoped resolvers; replica paths deleted |
| `internal/memory/memory.go` | Modified | `ensureWikiDir` replaces `ensureProjectCopy` |
| `internal/memory/cycle.go` | Modified | One compile, one destination |
| `internal/memory/replicate.go` | Deleted | Replication is the mechanism being removed |
| `internal/memory/rule.go` | Modified | There is no memory directory in the project |
| `internal/daemon/memorysyncmodule.go` | Modified | Fan-out deleted |
| `internal/daemon/adapters.go` | Modified | Embed targets are the only copy; no `OnEmbedded` |
| `internal/daemon/syncmodule.go` | Modified | Global AST and knowledge paths |
| `internal/hub/service.go` | Modified | Install copies once; link records a pointer; uninstall drops claims |
| `internal/hub/ast_store.go` | Modified | Legacy link cleanup deleted |
| `internal/hub/ui_server.go` | Modified | Lists the project's global stores |
| `internal/mcpstdio/context.go` | Modified | `resolveWikiDir` without chdir; `astConfigForProject` always project-scoped |
| `internal/mcpstdio/tools_wiki.go` | Modified | `context` on browse/log/xrefs; `wiki_source` description |
| `internal/mcpstdio/tools_knowledge.go` | Modified | Install/sync write the global context dir and register the claim |
| `internal/mcpstdio/tools_memory.go` | Modified | `memory_remove` drops the global worktree and wiki |
| `internal/mcpstdio/tools_ast.go` | Modified | Project-scoped embed config; tool descriptions |
| `internal/wikisvc/wikisvc.go` | Modified | Local and ecosystem sources resolve through `store` |
| `internal/livesearch/prep/index.go` | Modified | Whitelist compile; the user-memory copy removed |
| `cmd/graphit/commands/runners.go` | Modified | Project-scoped wiki scopes; registry-based context removal |
| `cmd/graphit/commands/daemon.go` | Modified | `store.ASTProjectDir` cache dir |
| `cmd/graphit/commands/hub.go` | Modified | Post-uninstall project cleanup removed |
| `internal/memory/consolidate.go` | Modified | Pre-existing `ineffassign` fixed |

Tests were updated across `internal/ast`, `internal/knowledge`, `internal/memory`,
`internal/daemon`, `internal/hub`, `internal/mcpstdio`, `internal/wikisvc` and
`internal/livesearch/prep`; `internal/memory/replicate_test.go` was deleted and
`internal/wikisvc/seed_test.go` added (a package `TestMain` isolating `HOME`, since a
test that seeds a wiki would otherwise write into — and read from — the developer's
real store).

## Trade-offs & Decisions

**A project keeps a membership record, not zero files.** The instinct was to derive
everything from the global directory, but a listing there cannot answer "which of
these may *this* project query" without exposing every context anybody on the machine
installed. Membership is genuinely per-project, so it stays per-project — as a few
hundred bytes of JSON, not as a copy of the data.

**Two records, not one.** Hub artifacts are claimed by the lockfile and locally
imported ones by `contexts.json`. Unifying them would mean either teaching the
lockfile about local imports (it is `hub`'s format, and `hub` imports `ast`) or moving
Hub claims out of it (breaking the install/uninstall refcounting). Two records with a
clean split by origin beat one record with a dependency inversion.

**A path-hash fallback for the store id, rather than requiring `init`.** `ast index`
has never required `init` and breaking that would be a worse regression than the cost
accepted: running `init` later re-keys the store and the next query reindexes once.
The `path-` prefix makes the fallback unmistakable in a directory listing.

**`WikiScope.Subdirs` instead of staging copies for the live search.** With the
artifacts now sharing one global root, a single-root walk would compile every context
on the machine. The alternatives were a per-session copy of the markdown — the exact
thing being removed — or a whitelist. The whitelist is ~20 lines in the walker and
costs nothing at runtime.

**The `wiki/` subdirectory probe moved into the resolver.** A published artifact and a
locally compiled context have different shapes, and three call sites used to probe for
it independently (two of them appending a `wiki` segment that did not exist, which
`OpenWikiDB` then happily created as an empty database). Resolving it once in
`WikiDirForContextIn` removes the divergence.

**`storePathForRequest` keeps its relative-path anchor.** Every resolver now returns
an absolute path, so the anchor is dead code in production — but it is three lines
that stop a hand-constructed relative `DBPath` from resolving against the server's
working directory, and the test that pins it is still meaningful.

## Technical Debt

- [ ] `internal/ast/query_loader.go` still keeps `projectASTDir` at
      `<project>/.graphit/ast` for grammar query files. That is correct (they are
      source), but it means `.graphit/ast` exists in a project for a reason unrelated
      to stores, which reads as a leftover. Consider renaming the key to
      `.graphit/grammars/queries` so the directory's purpose is unambiguous.
- [ ] Ephemeral live-search sessions now leave a wiki under
      `<global>/wiki/knowledge/project/path-<hash>` after the workspace is deleted.
      Session teardown should remove it; nothing does yet, so a long-lived server
      accumulates one small wiki per search.
- [ ] `store.ContextRecord.DBPath` is the "explicit store path" for both AST graphs
      and knowledge wikis. The name says database, which is wrong for the wiki case.
      Rename to `StorePath` when nothing else is in flight.
- [ ] No migration exists for a machine carrying the old layout. That is deliberate —
      the project is in development and stores rebuild — but the old
      `<project>/.graphit/{ast,knowledge,memory}` directories are left behind rather
      than cleaned up. A one-shot pruner in `graphit sync` would be cheap.
- [ ] `internal/memory/paths.go` recovers a context name from a worktree directory by
      splitting `<x>-<x>` in half. It is exact for contexts, but the underlying
      ambiguity (a flattened branch name) would be better solved by asking the git
      store for its active branches.

## System Knowledge

- **`OpenWikiDB` creates whatever it opens.** A wrong wiki path therefore yields a
  perfectly healthy *empty* index whose every answer is "no results" for a reason that
  has nothing to do with the query. This is why `openWikiForRead` refuses an index
  with no content, and why resolving the wiki directory in one place matters more than
  it looks.
- **The Ladybug backend connects lazily.** A wrong `DBPath` is invisible until the
  first query, long after any `chdir` would have been undone — which is exactly why
  the old chdir-and-restore approach looked correct and was not, and why the
  regression tests for it force a write.
- **`astConfigForProject` had the bug the tests were written to catch.** It called
  `ast.DefaultLadybugConfig()` (working directory) for a project's own graph while
  resolving *contexts* against the requested project. Making both go through
  `LadybugConfigForContextIn` fixed it; the test caught it within a minute of being
  rewritten.
- **A property binds in Cypher when *any* candidate label has it**, so a partial
  schema during a rebuild makes a correct query look wrong. Unrelated to this change,
  but it is why a reindex mid-session is worth avoiding and why the end-of-session
  index call is one call rather than one per edit.
- **The repository is not `gofmt`-clean** by the current toolchain: blank lines have
  been stripped project-wide. Running `gofmt -w` on an existing file produces an
  enormous unrelated diff. New files should be written already-formatted instead.

## Progress Log

### 2026-08-14
- Mapped the full storage surface (AST, knowledge, memory, MCP, daemon, hub, CLI,
  live search) before changing anything.
- Built `internal/store` as a leaf resolver plus the per-project context registry.
- Moved AST, knowledge and memory stores to the global directory; deleted the
  replication path, the project-side symlinks and the context copies.
- Extended the MCP surface (`context` on three wiki tools; descriptions stating that
  `wiki_source` and `ast_source` are the only readers).
- Rewrote the three skills' storage sections and added the end-of-session
  `graphit_ast_index` mandate.
- Repaired the suite: `go test -tags fts5 ./...` green, `make vet` clean, `make lint`
  0 issues. Four real bugs surfaced and were fixed in the process (see System
  Knowledge and Files Changed).
- Documented the layout in `docs/architecture/storage_layout.md` and updated the
  affected specs and guides.

### 2026-08-14 (follow-up)

- `withProjectDir` (`internal/mcpstdio/context.go`) refused to run when `os.Getwd()`
  failed, which took down every memory tool on an MCP server whose working directory
  had been deleted from under it — a directory the server inherited rather than chose
  and never needed, since the function exists to move *into* `projectDir`. It now
  falls back to restoring into `projectDir`, which at least exists. Found by hitting
  it: `graphit_memory_insert` failed with `failed to get current directory: getwd: no
  such file or directory` right after the full test suite ran, while
  `graphit_daemon_status` (which never calls `getwd`) answered normally.

  This is the same failure mode recorded in the project memory "MCP falhando com
  getwd" — previously diagnosed as an environmental accident. It is also a code
  defect, and this is the fix.

### 2026-08-14 (cross-platform audit)

Asked whether the change holds on Windows and macOS. It did not, in two places, and
both were introduced by this task:

1. **`ProjectStoreID` hashed the absolute path without folding case.** Windows always,
   and macOS by default, treat two paths differing only in letter case as the same
   directory — so one project reached by `C:\Proj` and `C:\proj` would have got two
   store ids, hence two graphs and two wikis, each looking healthy while holding half
   the answers. Fixed by folding on those platforms, reusing the `caseInsensitivePaths`
   predicate that the deleted `replicate.go` already had for the same reason. The
   hashing is now `pathStoreID(abs, fold)` with the fold as a parameter, so **both**
   behaviours are asserted on every host — a Linux-only CI would otherwise never
   exercise the folding branch at all.
2. **`ASTProjectDirByID` and `KnowledgeProjectDirByID` did not defuse Windows device
   names**, while `ProjectStoreID` was about to start doing so. Two resolvers keyed by
   the same id must agree on the path or a project's own store and the same store seen
   from the ecosystem become two directories. Both now go through `storeIDSegment`,
   which defuses device names and changes nothing else — case and characters are left
   alone, because an id is an identity.

Four tests were added in `internal/store/store_test.go`: the case-fold behaviour on
both kinds of filesystem, agreement between the `ByID` resolvers and `ProjectStoreID`,
device names defused across every resolver, and a sweep asserting that no path this
package builds carries a character Windows forbids, escapes the global root, or ends
in a space or a dot.

`internal/store` is CGO-free, which `go vet` confirms for `windows/amd64`,
`windows/arm64`, `darwin/amd64` and `darwin/arm64`. It has therefore been added to the
CI job that runs on all three operating systems — renamed from `watcher-cross-platform`
to `platform-semantics`, since the store resolver is now the second component in it
whose correctness is supplied by the operating system.

**What could not be verified here.** A Windows or macOS *binary* cannot be produced on
this machine: the cross build dies inside a third-party dependency
(`sqlite-vec-go-bindings`) for want of `sqlite3.h` for the mingw target, before
reaching any code from this repository. The release workflow builds those targets on
native runners (`macos-14`, `windows-2022`), which is where a real end-to-end check on
them happens. What is verified from here is the layer where this change actually lives:
path composition and identity, on all four target platform/arch pairs, plus the full
suite on Linux.

### 2026-08-14 (documentation audit)

Asked whether everything was documented. It was not. Five gaps, found by checking each
changed surface against the docs rather than by assuming:

1. **`WikiScope.Subdirs` was only in this task log.** It is a public field of
   `internal/knowledge` and the `Indexed Scope` section of `docs/specs/wiki_module.md`
   documents the struct — so the new field belonged there, with why it is not reachable
   from configuration and why the compile has to be one pass.
2. **`graphit_wiki_source` was absent from `docs/guides/mcp_tools_reference.md`
   entirely.** It is now the only way to read a wiki page, and it was the one wiki tool
   with no entry. Added with its full parameter table and the lookup/refusal behaviour.
3. **`wiki_browse`, `wiki_xrefs` and `wiki_log` did not list the `context` parameter**
   this task added to them, in either their own tables or the differentiation matrix.
4. **`docs/specs/hub_collaboration.md` said nothing about where an installed artifact
   lands, and nothing about `Link()`.** Both changed: install places a knowledge wiki
   once per machine and an AST store once per version, and link now records a pointer
   into the context registry instead of creating a symlink or copying pages. Added a
   per-type placement table and a section on the pointer, including the Windows junction
   failure mode the symlink carried.
5. **`docs/architecture/storage_layout.md` predated the cross-platform fixes.** It
   described the path-hash fallback without the case fold and without the device-name
   rule, so it documented a layout the code no longer produced. It now covers both, and
   gained a section on what deleting a store does on Windows.

`docs/specs/ast_module.md` also gained a pointer to the layout document — it never
stated where the graph lives, so nothing there was stale, but a reader had no way to
find out.

Checked and found already correct, needing no change: `docs/specs/daemon_module.md`
(its `MemorySyncModule` entry never mentioned the fan-out) and
`docs/specs/config_module.md` (`ast.queries_dir` still points into the project, which is
right — grammar queries are source, not a store).

### 2026-08-14 (superseded: the per-project registry is gone)

The trade-off recorded above — *"Membership per-project in `contexts.json`, not unified
into the lockfile"* — has been reversed. `contexts.json` no longer exists; every context
a project can reach is an artifact entry in `graphit.lock.json`.

The argument that justified the split was dependency inversion: `hub` owns the lockfile
format and `hub` imports `ast`, `knowledge` and `memory`, so none of those could read the
lockfile through `hub`. That obstacle turned out to be already bypassed — `HubContextsForProject`
was parsing the lockfile with an anonymous struct, the same trick `store.ProjectID` uses.
The structural fix was to stop working around it: the format moved to `internal/projectlock`,
a leaf package, and `hub` keeps type aliases so no caller changed.

What the split had been hiding is the part worth remembering: because the two registries
were chosen by *store path* rather than by principle, a Hub **knowledge** artifact was
installed into an unversioned directory. Two projects pinned to different versions shared
one copy and the last install silently won, while both lockfiles recorded a version
nothing enforced.

Full detail: [One registry for context membership](one-registry-for-context-membership.md).
