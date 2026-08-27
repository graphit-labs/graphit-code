---
title: An ephemeral session owns no store
status: done
created: 2026-08-14
updated: 2026-08-14
tags: [livesearch, store, knowledge, memory, ast, cleanup]
---

# An ephemeral session owns no store

## Objective

A live search session must have **no code graph of its own, no documentation wiki of its
own, and no memory scope of its own**. It reads the contexts it selected and the user's
memory; it owns nothing.

That was already the stated intent — the previous task log says in as many words *"There
is no project wiki, project memory or project AST"* — but it was an intention, not a
fact. Every one of the three was being created, and nothing reclaimed them.

## Implementation Details

### The root cause, which is a single thing

`prep/workspace.go` writes a lockfile with `Project.ID = <session id>`, for a good
reason: the agent CLI discovers instructions, tool configuration and identity from its
working directory, and without a lockfile it finds an empty folder.

Every store resolver keys on exactly that. **Having a lockfile with an identity is what
entitles a directory to stores**, and this directory has one for an unrelated reason, so
it was indistinguishable from a project someone works in.

### What was actually happening

| | when | how |
|---|---|---|
| documentation wiki | eagerly, during preparation | `compileKnowledgeWikis` compiled into `store.KnowledgeProjectDir(ws)` |
| memory scope | on first use | see the chain below |
| code graph | on first use | `openASTDBReadWrite`, reached by the end-of-session `ast_index` the AST skill mandates |

The memory one was the worst, and it was triggered by the framework's own instructions:
the mandate tells an agent to search project memory **before its first response**, so
every session asked.

```
newMemorySvc(false, ws)              mcpstdio/context.go   scopeID = lf.Project.ID
  EnsureInitialised                  memory/memory.go
    syncToLocalFast → syncToLocalInternal(true)
      MemoryWorktreeLocal → memoryWorktreeInternal   memory_git_store.go
        createOrphanBranch  →  memory/project/<session id>  in the SHARED repo
        addWorktree         →  <global>/memory-wt/memory-project-<session id>/
        RegisterBranch      →  an entry in the branch lock
      ensureWikiDir + IndexMemories  →  <global>/wiki/memory/project/<session id>/
```

So each discarded search left three stores, a git branch in a shared repository, and a
lock entry.

### The fact is recorded, not inferred

`hub.ProjectIdentity` gained `Ephemeral bool` (`json:"ephemeral,omitempty"`), and
`store.IsEphemeralProject(projectDir)` reads it. It reads the lockfile directly with an
anonymous struct, for the same reason `store.ProjectID` does: `ast`, `knowledge` and
`memory` are imported **by** `hub`, so `store` cannot import `hub`.

Inference was rejected. "Has no source or documentation of its own" also describes a real
project on its first day.

Anything unreadable answers `false`: an empty path, a missing lockfile, an unparseable
one. The flag grants a restriction, so the safe direction is to deny the restriction and
give a real project its stores.

### Knowledge: preparation compiles nothing, and there is no project-level read

`compileKnowledgeWikis` is deleted, along with `contextSourceDir` and `dirExists`. It was
both unnecessary and harmful:

- **unnecessary** because a knowledge context now arrives already compiled — the Hub
  install runs `wiki.BuildDBFromCache`, so each context has a searchable index before
  preparation would have run;
- **harmful** because the only place it could compile to was the wiki keyed by the
  workspace's project id.

`reportKnowledge` replaced it and only names what the session can search, so that "nothing
was selected" and "nothing resolved" stay distinguishable.

New `knowledge.ReadDirIn(projectDir, contextName)`: the context's directory when one is
named, the project's wiki for a normal project, and **nothing** for an ephemeral session
with no context named. `resolveWikiDir` calls it, so every knowledge and wiki tool
inherits the rule from one place — the same shape the memory branch of that function
already had.

`WikiScope.Subdirs` was removed. It had exactly one production caller, the compile above,
and `walkRoots` is now one line.

### Memory: the project scope is served from the user's

`memoryScopeFor(userScope, projectDir)` is the single scope resolver, replacing logic
duplicated in `newMemorySvc` and `newMemorySvcDetails`. For an ephemeral workspace it
redirects a project-scope request to the user scope and reports that it did.

Redirecting rather than refusing, because refusing is more literal and less useful: it
would fail the first call of every session and lose any memory the search was about to
record. The user's memory is the only memory such a session legitimately has — it is
about the user, applies everywhere, and is often the only place a constraint was written
down.

`resolveWikiDir`'s memory branch applies the same redirect. That is a correctness
requirement, not symmetry for its own sake: without it a search returns user-memory slugs
and reading one back resolves to a directory that does not exist.

`memoryScopeNotice` supplies the sentence, and `noticeResult` (new, in `server.go`) puts
it in front of a payload.

### AST: the write path is refused

`openASTDBReadWrite` refuses when no context is named and the project is ephemeral. That
is the structural guard, because opening read-write is what **creates** the store, and it
covers `ast_index` and a contextless `ast_remove` at once.

`ast_index` also checks at the top of its handler, because its `reset` option deletes the
store directory *before* the open.

Imported contexts are untouched — `ast_install` opens read-write with a context name, and
installing and querying contexts is most of what a live search does.

### `live remove` reclaims what earlier versions left

`livesearch.ReclaimFunc` and `Manager.SetReclaim`, injected for the same reason as
`PrepareFunc`: the runtime does not import `ast`, `knowledge` or `memory`, which pull in
the generated ANTLR parsers.

`prep.Reclaim(sessionID)` deletes the three stores plus the memory worktree, then calls
`memory.MemoryGitStore.PruneScope`, new and deliberately **unconditional** —
`DeregisterBranch` only prunes when the last reference goes away, which is right for a
scope someone may still be using and wrong for one that should never have existed.

`Manager.Remove` calls the hook **after** deleting the session directory and never
instead of it, and a failure to reclaim does not turn a successful removal into an error.
Wired in `live remove` and in the server that answers the UI's DELETE. The interactive
`graphit live` manager does not get it: it calls `CloseAll`, and closing is not removing.

## Use Cases

### UC-01: Prepare a live search session
- **Actor**: the live search runtime
- **Preconditions**: artifacts selected for the session have been installed
- **Main Flow**:
  1. `writeLockfile` records the session identity **and** `Ephemeral: true`
  2. `prepareIndexes` calls `reportKnowledge`, which names the installed contexts
  3. `reportGraphs` names the resolvable code graphs
  4. `prepareUserMemory` opens the user scope only
- **Alternative Flows**: nothing selected — each report is silent, which is not an error
- **Error Scenarios**: no git identity means no user memory, reported and survivable
- **Postconditions**: no store is keyed by the session id
- **Affected Files**: `internal/livesearch/prep/workspace.go`, `internal/livesearch/prep/index.go`

### UC-02: The session searches documentation
- **Actor**: the agent inside the session
- **Preconditions**: at least one knowledge context installed
- **Main Flow**:
  1. `graphit_knowledge_search` with `context: "<name>"`
  2. `resolveWikiDir` → `ReadDirIn` → that context's store
  3. BM25 runs over the index the Hub install compiled
- **Alternative Flows**: no context named → refused, and the message lists what is
  installed and says to pass one
- **Error Scenarios**: nothing installed → the message says the session selected nothing
- **Postconditions**: no wiki was compiled and none was created
- **Affected Files**: `internal/knowledge/paths.go`, `internal/mcpstdio/context.go`, `internal/mcpstdio/tools_knowledge.go`

### UC-03: The session recalls memory
- **Actor**: the agent, following the session-start mandate
- **Preconditions**: the user has a git identity
- **Main Flow**:
  1. `graphit_memory_search` with the default project scope
  2. `memoryScopeNotice` reports the redirect; `resolveWikiDir` resolves the user wiki
  3. results come back with the notice in front, via `noticeResult`
- **Alternative Flows**: `scope: "user"` explicitly — same data, no notice
- **Error Scenarios**: the user wiki has not been built → a sentence saying so, not
  "memory wiki not found for project scope"
- **Postconditions**: no branch, worktree, lock entry or wiki was created for the session
- **Affected Files**: `internal/mcpstdio/context.go`, `internal/mcpstdio/tools_memory.go`, `internal/mcpstdio/server.go`

### UC-04: The session finishes and the agent indexes
- **Actor**: the agent, following the AST skill's end-of-session obligation
- **Preconditions**: an ephemeral workspace
- **Main Flow**:
  1. `graphit_ast_index` with the workspace as `project_dir`
  2. The handler refuses before touching anything, naming contexts as the way in
- **Alternative Flows**: `ast_install` with a context still works
- **Error Scenarios**: `reset: true` is refused before the directory removal runs
- **Postconditions**: no graph exists for the session
- **Affected Files**: `internal/mcpstdio/tools_ast.go`, `internal/mcpstdio/context.go`

### UC-05: A session is removed
- **Actor**: a developer, via `live remove` or the UI
- **Preconditions**: the session exists
- **Main Flow**:
  1. `Manager.Remove` closes the session and deletes its directory
  2. The reclaim hook runs `prep.Reclaim(id)`
  3. The three stores, the memory worktree, the branch and its lock entry go
- **Alternative Flows**: nothing to reclaim, which is the normal case now
- **Error Scenarios**: an invalid id is rejected before the hook runs
- **Postconditions**: nothing on the machine is keyed by that session id
- **Affected Files**: `internal/livesearch/session.go`, `internal/livesearch/prep/reclaim.go`, `internal/memory/memory_branch_lock.go`, `cmd/graphit/commands/live.go`, `internal/uiserver/unified_server.go`

## Test Cases & Acceptance Criteria

### Feature: The ephemeral flag
Ref: UC-01

#### Scenario: Only a project that says so is ephemeral
```gherkin
Given a lockfile whose project block sets "ephemeral" to true
When the project is checked
Then it is ephemeral

Given a lockfile with no "ephemeral" field
When the project is checked
Then it is not ephemeral
```

#### Scenario Outline: Anything unreadable is not ephemeral
```gherkin
Given a project directory described as "<state>"
When it is checked for the ephemeral flag
Then the answer is false

Examples:
  | state                    |
  | an empty path            |
  | no lockfile at all       |
  | an unparseable lockfile  |
  | ephemeral set to false   |
```

#### Scenario: Store paths still resolve for an ephemeral id
```gherkin
Given an ephemeral project whose lockfile id is "01SESSION"
When its store paths are resolved by id
Then each one resolves
  And the reclaim can therefore name what an older version created
```

### Feature: Preparation owns nothing
Ref: UC-01, UC-02

#### Scenario: A prepared session has acquired no store
```gherkin
Given a session with one knowledge artifact installed
When preparation finishes
Then no code graph, documentation wiki, memory wiki or memory worktree exists for its id
  And preparation still named the documentation set it can search
```

#### Scenario: Each selected set is read where it was installed
```gherkin
Given a session that selected two documentation sets
When each is resolved by name
Then each resolves to its own context store
  And an unqualified read resolves to nothing
```

#### Scenario: A set nobody selected is not claimed
```gherkin
Given another project's context present in the shared knowledge root
  And a session that selected a different one
When the session's installed contexts are listed
Then only the selected one appears
```

### Feature: Documentation reads require naming a context
Ref: UC-02

#### Scenario: A normal project is unaffected
```gherkin
Given a normal project with a context installed
When an unqualified knowledge read is resolved
Then it resolves to the project's own wiki
```

#### Scenario: A session has nothing to read unqualified
```gherkin
Given an ephemeral session with two contexts installed
When an unqualified knowledge read is resolved
Then it resolves to nothing
```

#### Scenario: Naming a context works for either kind of project
```gherkin
Given a project, ephemeral or not, with a context installed
When that context is named
Then the read resolves to the context's store
```

### Feature: Memory is redirected, not refused
Ref: UC-03

#### Scenario: A project-scope request on a session is reported as redirected
```gherkin
Given an ephemeral session
When project-scope memory is requested
Then the result carries a notice that the user scope was used instead

Given the same session
When user-scope memory is requested explicitly
Then there is no notice, because nothing was redirected
```

#### Scenario: The wiki redirect agrees with the scope redirect
```gherkin
Given an ephemeral session
When the memory wiki directory is resolved for the project scope
  And it is resolved for the user scope
Then both resolve to the same directory
```

### Feature: The code graph write path is refused
Ref: UC-04

#### Scenario: A session cannot open a graph of its own read-write
```gherkin
Given an ephemeral session
When its own graph is opened read-write
Then the call is refused
  And the message names "context" as the way in
```

#### Scenario: An imported context stays writable
```gherkin
Given an ephemeral session
When a named context's graph is opened read-write
Then it succeeds, because installing and querying contexts is what a session is for
```

#### Scenario: A real project is unaffected
```gherkin
Given a project that is not ephemeral
When its own graph is opened read-write
Then it succeeds
```

### Feature: Removal reclaims
Ref: UC-05

#### Scenario: Every store keyed by the session is collected
```gherkin
Given residue from an older version for session "01OLDSESSION"
When the reclaim runs for that id
Then the graph, wiki, memory wiki and memory worktree for it are gone
```

#### Scenario: A neighbour's residue is untouched
```gherkin
Given residue for two different session ids
When the reclaim runs for one of them
Then that one's residue is gone
  And the other's is intact
```

#### Scenario: Reclaiming nothing is not a failure
```gherkin
Given a session that created no stores, which is the normal case
When the reclaim runs
Then it completes without error
```

#### Scenario: The hook runs after the session directory is gone
```gherkin
Given a session and a reclaim hook
When the session is removed
Then the hook is called once with the session id
  And the session directory no longer existed when the hook ran
```

#### Scenario: An id that was rejected never reaches the hook
```gherkin
Given a reclaim hook
When removal is called with something that is not a session id
Then removal fails
  And the hook was not called
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/lockfile.go` | Modified | `ProjectIdentity.Ephemeral` |
| `internal/store/store.go` | Modified | `IsEphemeralProject` |
| `internal/livesearch/prep/workspace.go` | Modified | the workspace marks itself ephemeral |
| `internal/livesearch/prep/index.go` | Modified | `compileKnowledgeWikis` deleted; `reportKnowledge` added |
| `internal/knowledge/paths.go` | Modified | `ReadDirIn` |
| `internal/knowledge/wiki.go` | Modified | `WikiScope.Subdirs` removed; `walkRoots` collapsed |
| `internal/wiki/multi_search.go` | Modified | `BM25SearchMerged` added then removed with the fan-out |
| `internal/mcpstdio/context.go` | Modified | `memoryScopeFor`, `memoryScopeNotice`, `errEphemeralHasNoGraph`, both resolver rules |
| `internal/mcpstdio/tools_knowledge.go` | Modified | `noKnowledgeToSearch` |
| `internal/mcpstdio/tools_memory.go` | Modified | scope resolution delegated; notices |
| `internal/mcpstdio/tools_ast.go` | Modified | early refusal in `ast_index` |
| `internal/mcpstdio/server.go` | Modified | `noticeResult` |
| `internal/memory/memory_branch_lock.go` | Modified | `PruneScope` |
| `internal/livesearch/session.go` | Modified | `ReclaimFunc`, `SetReclaim`, the hook in `Remove` |
| `internal/livesearch/prep/reclaim.go` | Created | `Reclaim` |
| `cmd/graphit/commands/live.go` | Modified | `live remove` installs the hook |
| `internal/uiserver/unified_server.go` | Modified | the server installs the hook |
| `internal/store/ephemeral_test.go` | Created | the flag and its failure modes |
| `internal/knowledge/readdir_test.go` | Created | the three resolution shapes |
| `internal/mcpstdio/ephemeral_test.go` | Created | resolver rules, memory redirect, graph refusal |
| `internal/livesearch/prep/reclaim_test.go` | Created | reclaim, isolation, no-op |
| `internal/livesearch/reclaim_hook_test.go` | Created | the hook contract |
| `internal/livesearch/prep/index_test.go` | Modified | rewritten for the new properties; `newBareSession` writes the lockfile |
| `docs/architecture/storage_layout.md` | Modified | identity is not entitlement |
| `docs/specs/wiki_module.md` | Modified | `Subdirs` removed |
| `docs/tasks/live-search-as-its-own-subsystem-sessions-sse-and-an-ephemeral-project.md` | Modified | Progress Log entry |
| `docs/tasks/per-file-wiki-cache-and-compiled-knowledge-publication.md` | Modified | debt closed and its record corrected |
| `docs/tasks/an-ephemeral-session-owns-no-store.md` | Created | this log |

## Trade-offs & Decisions

**A recorded flag over an inferred one.** Inference from "has no source of its own" would
misclassify a real project on its first day. The cost is a lockfile field and one more
thing preparation must remember to set — mitigated by `newBareSession` in the tests now
writing the lockfile, so a test workspace behaves like a real session.

**No project-level knowledge read, rather than a fan-out over the contexts.** The first
implementation had `knowledge_search` merge the session's contexts into one ranking, with
`BM25SearchMerged` to rank across sources. It was reversed on the user's instruction, in
favour of symmetry with the code graph: a session names what it wants. The merged search
was deleted rather than left unused. The honest downside: an agent must now issue one call
per context, and BM25 scores from different corpora were never really comparable anyway —
which is an argument the reversal happens to strengthen.

**Memory redirected rather than refused.** Refusing is more literal and worse: the mandate
makes the first call of every session a project-memory search, and refusing loses whatever
the search was about to record. The cost is that a session's "project" memory writes land
in the user's store; the notice is what keeps that from being silent.

**`PruneScope` is unconditional and destructive.** It cannot verify that a scope is
disposable, because a branch holding a session's memories and one holding a team's are the
same shape. The doc comment says so and the caller is responsible. Collision would require
two identical ULIDs.

**The reclaim exists although nothing should need it.** "Should not create" is a property
of the current code, not of a machine that has been running earlier versions.

## Technical Debt

- [ ] **The reclaim is only reachable through `live remove`.** Residue from a session that
  was deleted by hand, or before this change, is collected only if that same id is removed
  again — which it cannot be. A `live gc` that walks the stores for ids with no session
  would close it.
- [ ] **`prep.Reclaim` names the four locations literally.** A future store kind will not
  be reclaimed until someone remembers to add it. A single "every store keyed by this id"
  helper in `store` would be the structural fix.
- [ ] **The redirect notice is on `memory_search` and `memory_insert` only.** `memory_list`,
  `memory_important` and the update/delete family serve the user scope silently on an
  ephemeral session.
- [ ] **Nothing prevents a *real* project from being marked ephemeral.** The flag is
  writable by anything that writes a lockfile, and doing so would quietly deny that project
  its stores.
- [ ] **The skills still tell a session agent to do things it will be refused.** The AST
  skill's end-of-session `graphit_ast_index` obligation, and the knowledge skill's
  unqualified `graphit_knowledge_search`, are both installed into the ephemeral workspace.
  The refusals name the way out, so an agent self-corrects, but a clause in `rule.go` saying
  the obligation does not apply to a session would avoid the round trip. Not done here
  because regenerating the rendered `SKILL.md` files requires running the installer, and
  doing that with a stale installed binary overwrites current text with older text.
- [ ] Inherited, still open: `ContextRecord.DBPath` is misnamed; there is no migration from
  the old layout; `ContextNamesFrom` splits `<x>-<x>` in the middle; `memory/wiki.go` stamps
  `updated: now`.

## System Knowledge

- **A live search workspace has a lockfile with `Project.ID = <session id>`**, so its store
  key is that ULID and `ProjectStoreID` never reaches the `path-<hash>` fallback. An earlier
  debt note in the phase-2 log said otherwise; it was wrong and is corrected there.
- **`memoryWorktreeInternal` creates on read.** Asking for a scope's worktree creates the
  orphan branch, the worktree and the lock entry. There is no read-only way to ask.
- **`syncToLocalInternal` also builds the wiki**, via `ensureWikiDir` + `IndexMemories`, so
  "just opening" a memory scope produces a compiled wiki as well.
- **The framework's own mandate is what triggered the memory leak.** The session-start
  protocol requires `memory_search` before the first response. When instructions drive tool
  calls, a lazily-created store is created on every session, not occasionally.
- **`ast_index`'s `reset` deletes the store directory before the database is opened**, so a
  guard inside the open function is not sufficient for it.
- **`internal/livesearch` deliberately imports none of `ast`, `knowledge`, `memory`** — they
  pull the generated ANTLR parsers, which cost minutes to link. Anything the runtime needs
  from them arrives as an injected function; `ReclaimFunc` follows `PrepareFunc`.
- **`store.ProjectID` and `store.IsEphemeralProject` parse the lockfile with anonymous
  structs on purpose.** `hub` owns the format but imports `ast`, `knowledge` and `memory`,
  so `store` importing `hub` would invert the dependency.

## Progress Log

### 2026-08-14
- Measured the three violations before changing anything, including the exact chain from
  `memory_search` to `createOrphanBranch` in the shared repository.
- Added the flag, the predicate, and the mark in `writeLockfile`.
- Deleted `compileKnowledgeWikis` and `WikiScope.Subdirs`; first implemented the
  context fan-out for knowledge search, then reversed it on the user's instruction so that
  documentation behaves like the code graph, and deleted `BM25SearchMerged` with it.
- Unified memory scope resolution and added the redirect plus its notice; applied the same
  redirect in `resolveWikiDir` so search results stay readable.
- Refused the graph write path structurally and early.
- Added the reclaim seam, `prep.Reclaim` and `PruneScope`, wired into `live remove` and the
  UI server.
- Rewrote the prep tests whose premise was the compile; `newBareSession` now writes the
  lockfile, without which the tests exercised a workspace no real session is. Added five
  new test files.
- Full suite green, `make vet` clean, `make lint` 0 issues.
