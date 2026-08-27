---
title: UI wiki module discovery resolves from the global store instead of the project
status: done
created: 2026-08-15
updated: 2026-08-15
tags: [uiserver, wiki, knowledge, memory, store, regression]
---

# UI wiki module discovery resolves from the global store instead of the project

## Objective

In the web UI, Knowledge → "All Contexts" rendered the empty state ("No contexts
available"), the project's own knowledge wiki was missing from it, and the Memory section
of the sidebar — the project and user memory wiki entries — had disappeared entirely.

All three wikis existed and were current. `graphit_knowledge_search` and
`graphit_memory_search` answered normally from them. The fault was entirely in
`GET /api/wiki/modules`, which still looked for wikis where they lived before the storage
centralization.

## Root cause

`discoverModules` in `internal/uiserver/wiki_handler.go` probed four locations, and after
commit `db73e41` (*centralize storage layer with registry*) none of them holds a wiki:

| What it probed | What is there now |
|---|---|
| `<project>/.graphit/knowledge/project` | nothing — the per-project replica was deleted |
| `<project>/.graphit/memory/project` | nothing — same |
| `<project>/.graphit/memory/user` | nothing — same |
| `<global>/knowledge`, `<global>/memory` (directory scan) | moved to `<global>/wiki/knowledge`, `<global>/wiki/memory` |

Measured on this machine: `<project>/.graphit/` contains only `runtime/`, while
`~/.graphit/wiki/knowledge/project/01KSH1CRFFG8Z74B5ZS78WW808` held 182 pages and
`~/.graphit/wiki/memory/project/01KSH1CRFFG8Z74B5ZS78WW808` held 120.

Two properties of the failure are worth keeping:

- **It answered 200 with `[]`.** Nothing errored, nothing logged. An empty list is
  indistinguishable, from the frontend, from a project that has never indexed anything —
  so the UI correctly rendered "index project documentation first" over a fully indexed
  project.
- **The AST side of the same UI had already been migrated.** `internal/ast/server.go`
  `handleContexts` uses `store.ASTProjectDBPath` and `ListImportedContextsIn`, which is why
  the AST pages kept working and made the wiki pages look like data loss rather than a
  resolution bug.

## Implementation Details

`discoverModules` now resolves every path instead of walking anything:

- project documentation: `knowledge.WikiDirFor(projectDir)` → `store.KnowledgeProjectDir`
- project memory: `memory.WikiDirFor(projectDir, "project")` → `store.MemoryWikiDir("project", <lockfile id>)`
- user memory: `memory.WikiDirFor(projectDir, "user")` → keyed by the git identity hash
- imported documentation contexts: `store.ContextNames(projectDir, store.KindKnowledge)`,
  each resolved with `store.KnowledgeContextDirIn` (which handles the three origins: Hub
  version-keyed, link to a sibling's own wiki, local import)
- imported memory contexts: `memory.AllContextDirs()` → `memory.MemoryWikiGlobalDir(name, name)`

`discoverContexts` and `scanDir` were **deleted**. Context membership is a per-project
record now; a scan of the global wiki root would report every context anybody on this
machine ever installed, for every project. This mirrors the decision already documented in
`internal/ast/config.go` `ListImportedContextsIn` ("Two records are merged, and neither is a
directory scan"). Imported *memory* contexts stay a directory listing on purpose — a memory
context is a branch of the shared memory repository, so the worktree set is its only record,
per `memory.ContextNamesFrom`.

Two helpers were extracted: `projectDisplayName` (lockfile name via `projectlock.Load`,
falling back to the directory basename) and `contextLabel` (prefers a readable project name
from `hub.registry.json` over the ULID a Hub context is keyed by).

The JSON contract is unchanged, deliberately — the frontend depends on the exact shape:
`id` (`knowledge`, `knowledge/<ctx>`, `memory-project`, `memory-user`, `memory/<ctx>`),
`label`, `path`, `context`, `pages`, `hasLog`.

### Visibility rule

The project's own three wikis are listed when their directory **exists**, even with zero
pages; imported contexts require at least one page. This preserves the previous behavior
exactly. It is why `memory-user` appears here with `pages=0`: this machine genuinely has no
user memories, and hiding the entry would read as "the module is gone" rather than "nothing
is indexed yet".

## Use Cases

### UC-01: Browse the knowledge contexts of a project
- **Actor**: user, via Knowledge → All Contexts (`/knowledge/contexts`)
- **Preconditions**: the project has a lockfile identity; its documentation wiki has been compiled at least once
- **Main Flow**:
  1. `WikiContextsPage` calls `fetchModules(activeProjectDir)` → `GET /api/wiki/modules?project_dir=`
  2. `handleModules` calls `discoverModules(projectDir)` and sorts the result by `id`
  3. `discoverModules` resolves the project wiki through `knowledge.WikiDirFor` and the imported ones through `store.ContextNames` + `store.KnowledgeContextDirIn`
  4. The page keeps ids equal to `knowledge` or prefixed `knowledge/`, splitting them into Project Scope (`context` is `project`/`user`) and Imported Scope
  5. "Explore" navigates to `/knowledge/explorer` for the project, or `/knowledge/explorer/<context>` for an import
- **Alternative Flows**:
  - No `project_dir` in the query: the handler falls back to the server's working directory
  - A context claimed in the lockfile whose wiki was never compiled is omitted, not shown as empty
- **Error Scenarios**:
  - No lockfile: the project has no id, so `memory.WikiDirFor(dir, "project")` returns `""` and the module is skipped; the knowledge wiki is still keyed by a path hash and resolves
  - The global store is unreachable (no home directory): `store.globalOr` degrades to the project's runtime dir, where nothing exists, and the list is empty
- **Postconditions**: the response lists exactly the wikis the store resolves for that project
- **Affected Files**: `internal/uiserver/wiki_handler.go`, `internal/ui/src/components/wiki/WikiContextsPage.tsx`

### UC-02: Reach the project and user memory wikis from the sidebar
- **Actor**: user, via the sidebar Memory section
- **Preconditions**: at least one memory scope has a compiled wiki directory
- **Main Flow**:
  1. `Sidebar` calls `fetchModules(activeProjectDir)` and keeps ids prefixed `memory-`
  2. Each becomes a nav item routed to `/memory/explorer/<context>`, labelled `User` for `context === 'user'` and the project name otherwise
  3. `WikiExplorerPage moduleFilter="memory"` selects the module whose `context` matches the URL segment and lists its pages from `module.path`
- **Alternative Flows**: with zero memory modules the whole section is hidden — which was the reported symptom
- **Error Scenarios**: no git `user.email` configured → the user scope has no id, `WikiDirFor` returns `""`, and only the project entry appears
- **Postconditions**: both scopes are reachable and their pages are served from the global wiki directory
- **Affected Files**: `internal/uiserver/wiki_handler.go`, `internal/ui/src/components/layout/Sidebar.tsx`, `internal/ui/src/components/wiki/WikiExplorerPage.tsx`

### UC-03: Read the pages of a discovered module
- **Actor**: the UI, immediately after discovery
- **Preconditions**: a module was returned by UC-01 or UC-02
- **Main Flow**:
  1. The frontend passes `module.path` verbatim as `dir` to `GET /api/wiki/pages`
  2. `handlePages` resolves symlinks, absolutises, stats the directory and walks it for `.md` files, skipping `shards/`
- **Error Scenarios**: a `dir` that does not exist returns 400 — which is why discovery stats before reporting a module
- **Postconditions**: the page list matches the `pages` count reported by discovery
- **Affected Files**: `internal/uiserver/wiki_handler.go`

## Test Cases & Acceptance Criteria

### Feature: Wiki module discovery
Ref: UC-01, UC-02, UC-03 — implemented in `internal/uiserver/wiki_modules_test.go`

#### Scenario: The project's wikis are found in the global store
```gherkin
Given a HOME this test owns
  And a project whose lockfile id is "01TESTPROJECT0000000000000" and name is "acme"
  And a compiled knowledge wiki at store.KnowledgeProjectDir with "index.md", "overview.md" and "log.md"
  And a compiled memory wiki at store.MemoryWikiDir("project", that id) with "index.md"
When discoverModules runs for that project
Then a module with id "knowledge" is returned
  And its path is the store's knowledge directory, under the isolated HOME
  And its label is "acme"
  And its context is "project"
  And its pages count is 3
  And hasLog is true
  And a module with id "memory-project" is returned pointing at the store's memory directory
```

#### Scenario: A wiki left at the pre-centralization path is ignored
```gherkin
Given a HOME this test owns
  And an initialised project
  And a wiki written at "<project>/.graphit/knowledge/project" containing "index.md"
When discoverModules runs for that project
Then no returned module has a path inside the project directory
```

#### Scenario: The user memory wiki is listed for a project with no identity
```gherkin
Given a HOME this test owns
  And a project directory with no lockfile
  And a compiled wiki at the user memory scope directory
When discoverModules runs for that project
Then a module with id "memory-user" and context "user" is returned
  And its path is the user scope's wiki directory
```

#### Scenario: Only contexts the project claimed are reported
```gherkin
Given a HOME this test owns
  And an initialised project
  And a compiled context wiki named "claimed" with two pages
  And a compiled context wiki named "somebody-elses" that this project never claimed
  And the project's lockfile claims "claimed" as a knowledge context
When discoverModules runs for that project
Then a module with id "knowledge/claimed" is returned with 2 pages and context "claimed"
  And no module for "somebody-elses" is returned
```

#### Scenario: A project with nothing compiled reports nothing
```gherkin
Given a HOME this test owns
  And an initialised project with no wiki compiled in any scope
When discoverModules runs for that project
Then zero modules are returned
```

#### Scenario: The endpoint serves the resolved wikis, sorted by id
```gherkin
Given a HOME this test owns
  And an initialised project with a knowledge wiki, a project memory wiki and a claimed context "vendor-api"
When GET /api/wiki/modules?project_dir=<project> is requested
Then the status is 200
  And the response contains ids "knowledge", "knowledge/vendor-api" and "memory-project"
  And the ids are in ascending order
```

#### Scenario: The pages of a discovered module are readable through /api/wiki/pages
```gherkin
Given a HOME this test owns
  And an initialised project whose knowledge wiki holds "index.md" and "billing.md"
When discoverModules returns the "knowledge" module
  And GET /api/wiki/pages?dir=<that module's path> is requested
Then the status is 200
  And two pages are returned
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/uiserver/wiki_handler.go` | Modified | `discoverModules` resolves through `internal/store`; `discoverContexts` and `scanDir` deleted; `projectDisplayName` and `contextLabel` extracted |
| `internal/uiserver/wiki_modules_test.go` | Created | Regression coverage for discovery and the endpoint, against an isolated global store |
| `internal/uiserver/uiserver_test.go` | Modified | Removed the four `scanDir` tests and the `discoverModules` tests that built wikis inside the project |
| `internal/uiserver/coverage_boost_test.go` | Modified | Same removal; dropped the now-unused `strings` and `brand` imports |
| `internal/uiserver/wiki_handler_extended_test.go` | Modified | Replaced `TestHandleModules_WithWikiContent`, which asserted the project-local layout |
| `docs/specs/ui_dashboard.md` | Modified | Corrected the wiki endpoint list and documented the resolution rule and the id/context contract |

## Trade-offs & Decisions

- **Resolvers over a directory scan.** A scan of `<global>/wiki/knowledge` would have been a
  two-line fix and would have listed every context on the machine under every project. The
  registry is the record of membership; the AST side already made this call, and having the
  two disagree is how the split that produced `contexts.json` started.
- **Imported memory contexts remain a directory listing.** Not an inconsistency: they have no
  per-project registry by design, so the worktree set is the only record. Giving them one
  would mean a second record of the same fact.
- **The visibility asymmetry was kept** (own wikis need only exist, imports need a page)
  rather than unified. Unifying either way changes what the sidebar shows, and this fix is
  about restoring the previous behavior, not redefining it.
- **The `WikiModule` JSON shape was left alone.** `id`, `context` and `path` are load-bearing
  in three frontend components; changing them here would have turned a server fix into a
  frontend change with no benefit.
- **Stale tests were deleted rather than adapted.** They created wikis inside the project, so
  adapting them would have meant rewriting every assertion anyway — and they were the reason
  the regression shipped green.

## Technical Debt

- [ ] `GET /api/wiki/pages` and `/api/wiki/page` accept an arbitrary absolute `dir` and read
      any `.md` under it. Pre-existing (the server is localhost-only, and `handlePage` blocks
      traversal *within* the given dir), but now that every legitimate `dir` is a store path,
      these could be constrained to directories `discoverModules` would report.
- [ ] The UI reads wiki pages as `.md` files from disk while search reads the compiled
      `wiki.db`. A wiki whose pages were pruned but whose database is current would list
      nothing in the explorer and still answer searches. Not reachable through the current
      publication paths — a published knowledge artifact carries pages and shards and never
      the database (`internal/knowledge/publish_test.go`) — but the two readers can drift.
- [ ] `docs/specs/ui_dashboard.md` still describes other parts of the dashboard that have
      moved on (the chat endpoint became the live search). Only the wiki handler section was
      corrected here.

## System Knowledge

- **`<project>/.graphit/` holds only `runtime/`.** Every compiled artifact — code graph,
  knowledge wiki, memory wiki — lives once under `~/.graphit`, keyed by
  `store.ProjectStoreID`: the lockfile ULID, or `path-<16 hex of sha256 of the absolute path>`
  for a project that was never initialised.
- **The memory scope id is not the project id in general.** `project` is keyed by the lockfile
  id, `user` by the first 16 hex of the sha256 of `git config user.email`, and an imported
  context by its own name used as both scope and id.
- **Anything that resolves a global store must isolate `HOME` in tests.** `store.Root()` is
  `brand.GlobalDir()`, which is `$HOME/.graphit`. Without `t.Setenv("HOME", …)` a test reads
  the developer's real store — the deleted `TestDiscoverModules_EmptyProject` would have
  started passing or failing depending on whether that developer happens to have a user
  memory wiki. `t.Setenv` and `t.Parallel` are mutually exclusive, which is why the new tests
  are serial.
- **The uiserver package needs `-tags fts5`** to compile at all; without it `internal/wiki`
  and `internal/ast` resolve to their guard files. Tests also need
  `LD_LIBRARY_PATH` pointing at the go-ladybug `lib` directory.

## Progress Log

### 2026-08-15
- Reproduced the report by resolving the store paths on disk: project `.graphit/` had only
  `runtime/`, while the global wiki directories held 182 and 120 pages.
- Traced `GET /api/wiki/modules` → `discoverModules` and confirmed all four probed locations
  were pre-centralization paths; confirmed the AST handler had already been migrated.
- Rewrote discovery against `internal/store`, deleted `discoverContexts`/`scanDir`, and
  replaced the tests that had been passing against the project-local layout.
- Verified on live data: `knowledge` (182 pages, label from the lockfile), `memory-project`
  (120 pages) and `memory-user` (0 pages — this machine has no user memories) all resolve
  under `~/.graphit/wiki/`.
- `go vet -tags fts5 ./internal/uiserver/` clean; package tests pass.
