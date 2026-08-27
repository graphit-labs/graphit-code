---
title: Comment cleanup and dead code removal across the Go and TypeScript tree
status: done
created: 2026-08-15
updated: 2026-08-15
tags: [refactor, cleanup, comments, dead-code, ci]
---

# Comment cleanup and dead code removal across the Go and TypeScript tree

## Objective

Sweep the whole codebase for comments that carry no information — decorative rules,
section banners that repeat the declaration below them, narration that restates the
next statement — and remove them, keeping every comment that states a *why* the code
cannot state itself. In the same pass, remove dead code (unreferenced exports,
abandoned subsystems) and leave `make build` and `make ci` green.

The rule applied throughout: **a comment survives when it says something the code
cannot.** A comment that names what the next line already names does not.

## Implementation Details

### 1. Comment inventory built from the parser, not from regex

A throwaway scanner (`go/parser` with `ParseComments`) produced an exact index of every
comment group in the 547 non-generated Go files: file, start line, end line, text, and
whether the group is a doc comment and of which declaration. Regex over raw lines was
rejected because a line inside a raw string literal (`internal/dream/prompt.go` builds
markdown with `//`-looking content) is not a comment, and only the parser knows that.

Baseline: **5,276 comment groups / 14,470 comment lines**.
After the sweep: **4,642 groups / 12,946 lines** — 1,524 lines removed, 10.5%.
Go source went from 147,917 to 145,475 lines across the same file set, minus the
two files deleted outright.

### 2. Decorative banners (870 lines)

The dominant pattern was a three-line header:

```go
// ---------------------------------------------------------------------------
// Multi-pass search engine
// ---------------------------------------------------------------------------
```

The two rule lines carry nothing. They were stripped everywhere, and the title line was
kept as a plain `// Multi-pass search engine`. The same treatment was applied to
`// ═══`, `// ─── Title ───` and `// --- Title ---` variants. 65 files, 870 lines, zero
information lost — the words that were between the rules are still there.

Empty banners (rule immediately followed by rule) were deleted whole.

### 3. Section titles and narration that restate the code (532 lines)

A second pass compared every free-floating one-line comment against the next
non-blank, non-comment line, tokenising both (splitting camelCase and snake_case) and
keeping only content words. When ≥60% of the comment's content words already appear in
the line below, the comment was a restatement.

Dominant shapes removed:

| Shape | Example |
|---|---|
| Section title above the test of the same name | `// safeMemFilename` above `func TestSafeMemFilename` |
| Numbered step above the call it numbers | `// 5. RunOutput` above `logOut, err := g.RunOutput(...)` |
| Category label above the function it labels | `// Deduplication` above `func deduplicationKey` |
| Test narration | `// Test config get` above `executeCommand("config", "--get", "ide")` |

21 candidates were **kept** by an explicit exclusion list: any comment containing
`should`, `must`, `no-op`, `never`, `always`, `because`, `why`, `note`, `safe`,
`convention`, `expected`, `avoid`, `cannot`, `only`, `instead`, `otherwise`, `unlike`,
`deliberate`, `intentional` states an expectation or a rationale, not a restatement.

### 4. Test-setup narration (67 lines)

A third pass took one-line in-function comments opening with a setup verb (`Create`,
`Write`, `Set up`, `Register`, `Now`, `Then`, …) of at most seven words. 51 of the 118
candidates were kept because the phrase carries the point of the fixture — an adjective
(`malformed`, `unreadable`, `ignored`, `unsupported`, `fake`, `outside`), a
parenthetical, a quoted string, a number, or a `so …` clause. The remaining 67
(`// Create a file`, `// Now remove`, `// Append another entry`) were removed.

Two comments in `internal/ast/antlr_sidecar.go` were explicitly preserved for this
reason — `// Build request frame: [grammar\0][source]` and
`// Write request: [4 bytes length LE][payload]` document a wire format the code cannot
state.

### 5. Doc comments were NOT touched

Go doc comments (1,881 groups) are this project's documentation mechanism for
declarations — they are what `go doc` renders — so they were left in place, including
the long rationale blocks (`internal/ast/incremental_rebuild.go`,
`internal/ast/query_loader.go`, `Makefile`). Those blocks were evaluated for migration
to `docs/` and deliberately kept: they explain a *local* decision at the exact place
the reader needs it, and several are the record of measured behaviour (the copy+swap
vs in-place table, the ORT API version, the `-s -w` measurement) that loses its force
when detached from the code it constrains.

### 6. Four doc comments were attached to the wrong declaration

Go attaches a comment group to the declaration that immediately follows it. In four
places, two doc blocks had been written back to back with no blank line, so the *first*
block — documenting a function declared further down — became the doc comment of the
function in between. `go doc` rendered the wrong documentation for both.

| File | Comment for | Was attached to | Fix |
|---|---|---|---|
| `internal/ast/rebuild_index.go` | `resolveCallee` | `resolveNamed` | moved down to its function |
| `internal/ast/query_loader.go` | `effectiveProjectQueryFiles` | `allEffectiveQueryFiles` | moved down to its function |
| `internal/mcpstdio/tools_knowledge.go` | `installKnowledgeContext` | `noKnowledgeToSearch` | moved down to its function |
| `internal/ast/server.go` | `defaultGraphEdgeQuery` | `graphEdgeSampleQuery` | retitled: it describes the function it is on |

### 7. Dead code

`unused` is disabled in `.golangci.yml`, so nothing was catching this. Running
`golangci-lint run --enable unused` reported **0 issues** — there is no unreferenced
*unexported* code. Everything found below is exported, which `unused` does not report
in package mode; since every package here is under `internal/`, an export nothing
references is unreachable from anywhere.

Candidates came from the AST graph (exported declarations with no inbound `CALLS`),
then each was verified by a whole-repo identifier count that excluded its own
declaration and doc comment.

**Removed — zero references anywhere, including tests:**

| Symbol | File | Note |
|---|---|---|
| `ASTRoot`, `KnowledgeRoot`, `MemoryRoot` | `internal/store/store.go` | unused root accessors |
| `ContextDBPath` | `internal/ast/config.go` | cwd-based variant its own doc told callers to avoid |
| `SetAntlrGrammarProjectDir` | `internal/ast/antlr_adapter.go` | racy exported setter; `setAntlrGrammarProjectDirIfUnset` is the live path |
| `HasAntlrForExtension` | `internal/ast/antlr_adapter.go` | `""`-projectDir wrapper over the `…In` variant |
| `HasTreeSitterForExtension` | `internal/ast/treesitter_adapter.go` | idem |
| `HasNativeGrammar` | `internal/ast/treesitter_native.go` | — |
| `LauncherStampPath`, `ReadLauncherStamp` | `internal/daemonctl/daemonctl.go` | pair; only the pair referenced each other |
| `SkillHashCachePath` | `internal/hub/adapters/ide/adapters.go` | exported wrapper over `skillHashCachePath` |
| `ManagedSkillHashCachePath` + `adapterRootDir` | `internal/hub/adapters/ide/adapters.go` | `adapterRootDir` had no other caller |
| `LoadInstalledArtifacts` + `InstalledArtifactInfo` | `internal/hub/global_lock.go` | superseded by `hub.InstalledArtifacts` |
| `RawDirFor` | `internal/memory/paths.go` | — |
| `NodeRecord`, `RelRecord`, `Releaser` | `internal/ast/types.go` | types nothing declares or asserts |
| `AllModuleNames` | `internal/config/config.go` | `OptInModules` beside it is used; this one was not |
| `AppliedCount` | `internal/memory/consolidate_apply.go` | — |
| `EnsureASTAvailable`, `ASTArtifactFor` | `internal/hub/ast_store.go` | see Trade-offs |
| whole file `internal/hub/appsvc.go` | | see below |
| whole file `internal/ast/jobs.go` | | see below |

**`internal/hub/appsvc.go` (deleted).** `HubAppService` announced itself as
"hub operations shared across views (CLI, MCP, UI)" and no view used it: `ListSummary`
and `SearchSummary` had zero references, `ResolveIDE` duplicated `config.ResolveIDE`
and was called only from `lifecycle_test.go`, and the `projectDir` field was never
read. The file and the two tests that existed only to exercise it were removed.

**`internal/ast/jobs.go` (deleted) and the `/api/jobs` endpoints.** `JobManager` held a
`map[string]*Job` with a `Get` and a `List` and **no way to add a job** — no
constructor argument, no setter, no writer anywhere. The five `JobStatus` constants
were never assigned. Two HTTP routes were wired to it, so `GET /api/jobs` always
returned `[]` and `GET /api/jobs/{id}` always returned 404; nothing in the UI called
either. Removing it took the `jobs` field off `ast.Server`, a parameter off
`ast.NewServerOnPort` and off `uiserver.NewUnifiedServer`, and the `ast.NewJobManager()`
line out of `cmd/graphit/commands/runners.go`.

### 8. Comments that describe code the store centralisation removed

`docs/tasks/backlog/comentarios-e-nomes-que-ainda-descrevem-os-artefatos-removid.md`
asked for this sweep; the first two of its three items are now closed.

- **`internal/livesearch/prep/index.go`** — the doc comment of `prepareUserMemory`
  described `MemoryService.ensureProjectCopy` (removed), claimed "the copy is done here
  rather than by the memory service", and documented an accepted side effect of
  `EnsureInitialised` performing "its own project copy". The body of the same function
  says *"Nothing is copied"*. Rewritten to state what happens now, keeping the one
  transferable lesson: a write destination must never be derived from the process
  working directory, because a server holds several sessions and the cwd names one
  arbitrary project.
- **`internal/memory/memory_full_coverage_test.go`** — section header
  `// memory.go — ensureProjectCopy` over tests of `MemoryWikiGlobalDir` / `WikiDir` /
  `RawDir`. Removed.
- **`internal/memory/memory_coverage_test.go`** — headers `// ProjectLinkDir` and
  `// GlobalBaseDir, ProjectReplicaDir` named removed functions; a comment claimed
  `EnsureWikiIndexExists` "uses ProjectReplicaDir". All three corrected.

The remaining mentions of *replica* in `internal/store/store.go`,
`internal/memory/paths.go`, `internal/knowledge/paths.go`,
`internal/daemon/memorysyncmodule.go` and `internal/daemon/adapters.go` are the
deliberate history the backlog item says to keep — they state, in the past tense, why
the current single-copy design exists, and they are what stops the replica being
reintroduced.

**One of them was not history but dead code.** `wiki.EmbedTarget.OnEmbedded` was a
callback field documented as "the memory wiki is compiled once and copied into every
project that reads it, so vectors that land here have to be pushed out again" —
present tense, about the removed fan-out. `EmbedTarget` is constructed in exactly one
place (`daemon.WikiEmbedTargets`) and never sets it, so the four
`if target.OnEmbedded != nil` guards in `internal/wiki/embed_loop.go`,
`internal/mcpstdio/tools_wiki.go`, `internal/mcpstdio/tools_lifecycle.go` and
`cmd/graphit/commands/runners.go` were unreachable. Field, guards and comment removed.

### 9. TypeScript / CSS

192 comment lines across 53 files, nearly all of them substantive. One duplicated
header block in `internal/ui/src/components/wiki/WikiExplorerPage.tsx` — two adjacent
comments saying the same thing about the live search — was folded into one.

### 10. `make ci` was not green before this task — the `vulncheck` step was broken

`make ci` is `ui vet lint vulncheck test ui-lint`. The `vulncheck` target ran
`govulncheck ./...` with **no build tags**, so `internal/ast` and `internal/wiki`
resolved to their `//go:build !fts5` guard files instead of the real packages and the
load aborted:

```
/home/…/internal/ast/fts5_required.go:33:11: undefined: graphit_requires_the_fts5_build_tag____run_make_test_or_pass_tags_fts5
make: *** [Makefile:538: vulncheck] Error 1
```

Broken since `8a8fdc4` (2026-08-12), the commit that introduced the guard files —
`vet` and `lint` were given the tag, `vulncheck` was not. It never surfaced as a
vulnerability report; it read as a broken tool, and everything after it in the CI chain
(`test`, `ui-lint`) never ran.

Two changes:

- **`Makefile`** — `vulncheck` now passes `-tags $(BUILD_TAGS)`, matching `vet` and
  `.golangci.yml`.
- **`go.mod`** — `go 1.26.5` → `go 1.26.6`. With the tag fixed, govulncheck ran for the
  first time and reported **5 standard-library vulnerabilities**, all of them fixed in
  the Go 1.26.6 patch release (GO-2026-5972 `encoding/asn1` recursion depth, GO-2026-5026
  `net/http`/`x/net/idna` Punycode, and three others reachable from
  `internal/updater/updater.go:155`, `internal/mcpproxy/proxy.go:306` and the generated
  PL/SQL lexer). Every workflow in `.github/workflows/` reads `go-version-file: go.mod`,
  so the single directive moves local builds, CI and release together. After the bump:
  `No vulnerabilities found.`

### 11. Formatting

`gofmt -w` over the project Go files. This also closed a pre-existing drift: 55
files carried a trailing blank line that `gofmt` removes and that nothing in `make ci`
was checking.

## Files Changed

| File | Change | Reason |
|---|---|---|
| 156 files across `cmd/` and `internal/` | Modified | banner, narration and restatement comments removed; `gofmt` |
| `internal/ast/jobs.go` | Deleted | job manager nothing could ever write to |
| `internal/hub/appsvc.go` | Deleted | service no view used |
| `internal/ast/server.go` | Modified | `jobs` field, two handlers and two routes removed |
| `internal/uiserver/unified_server.go` | Modified | `astJobs` parameter removed |
| `cmd/graphit/commands/runners.go` | Modified | `ast.NewJobManager()` call removed |
| `internal/ast/server_file_handler_test.go` | Modified | `NewServerOnPort` signature |
| `internal/hub/lifecycle_test.go` | Modified | tests of the deleted `HubAppService` |
| `internal/ast/rebuild_index.go`, `internal/ast/query_loader.go`, `internal/mcpstdio/tools_knowledge.go`, `internal/ast/server.go` | Modified | doc comments moved to the declaration they document |
| `internal/ui/src/components/wiki/WikiExplorerPage.tsx` | Modified | duplicated header comment folded |
| `internal/wiki/embed_loop.go` | Modified | dead `EmbedTarget.OnEmbedded` field and its stale doc removed |
| `internal/mcpstdio/tools_wiki.go`, `internal/mcpstdio/tools_lifecycle.go`, `cmd/graphit/commands/runners.go` | Modified | unreachable `OnEmbedded` guards removed |
| `internal/livesearch/prep/index.go` | Modified | doc comment described `ensureProjectCopy`, which the body contradicts |
| `internal/memory/memory_coverage_test.go`, `internal/memory/memory_full_coverage_test.go` | Modified | section headers naming removed functions |
| `Makefile` | Modified | `vulncheck` gets `-tags $(BUILD_TAGS)`; the step could not run without it |
| `go.mod` | Modified | `go 1.26.6` — closes the 5 stdlib vulnerabilities govulncheck reported once it could run |
| `docs/tasks/comment-cleanup-and-dead-code-removal.md` | Created | this log |

## Trade-offs & Decisions

- **Go doc comments were kept, including the long ones.** The project improvement rules
  say documentation belongs in `docs/`, not in comments. That rule is about prose
  explaining *what* code does; a Go doc comment is the language's documentation
  mechanism and `go doc` is how it is read. Stripping them would remove documentation,
  which is the opposite of the request. The long rationale blocks were read individually
  and kept for the same reason: they constrain the code they sit on.

- **`internal/ast/types.go` label and relation constants were kept**, even though 32 of
  them are unreferenced. They are one enumerated vocabulary of the graph schema, and the
  memory *Lista fixa de labels no tradutor de Cypher* records that such a list can never
  be complete anyway (grammars declare labels at runtime, from YAML). Deleting the
  unused half of an enum makes the remaining half look authoritative when it is not.

- **`EnsureASTAvailable` and `ASTArtifactFor` were removed even though they are recent.**
  Both landed in `fbaaa10` (live search, 2026-08-13) and neither was ever called. Their
  docs describe a path — "consult a system's code graph without installing the artifact
  into a project, which is what the live search does" — that the prep design no longer
  takes: prep installs artifacts into the ephemeral project and `hub.Install` builds the
  store. If the intent was to wire them later, this is the commit to revert.

- **Exports used only by their own tests were kept**: `LockPath`,
  `ListInstalledInProject`, `ValidateProjectDirs` on `GlobalLockManager`. Removing them
  means deleting passing tests as well, which is a different decision from removing code
  nothing references at all.

- **Comment language was left as it is.** 63 comment groups are written in Portuguese
  (concentrated in `internal/ast/rebuild_index.go`, `cache_convert.go`,
  `file_reference_source_test.go`, `sql_treesitter_test.go`, `embedded_*_test.go`) in an
  otherwise English tree — `internal/ast/rebuild_index.go` even carries the same
  rationale twice, once in each language, at lines 820 and 888. Translating them is a
  separate decision with real risk of blunting carefully worded reasoning, so it was not
  done here. See Technical Debt.

## Technical Debt

- [ ] **Comment language is mixed.** 63 Portuguese comment groups in an English
  codebase, in a public repository. Either translate them to English or record the
  choice as a convention — the current state is neither.
- [ ] **`unused` is disabled in `.golangci.yml`.** It currently reports 0 issues, so
  turning it on would cost nothing and would stop unexported dead code from
  accumulating again. Left off because changing the lint configuration was outside this
  task.
- [ ] **Exported-but-unused symbols are not caught by anything.** `unused` does not
  report exported identifiers in package mode, and every package here is `internal/`,
  where an unreferenced export is unreachable. The AST-graph query used in this task
  (`MATCH ()-[:CALLS]->(s) WITH collect(DISTINCT s.name) AS called MATCH (f:Function)
  WHERE f.is_exported AND NOT f.name IN called …`) is the check; it is not automated.
- [ ] **Exports referenced only by their own tests** — `GlobalLockManager.LockPath`,
  `ListInstalledInProject`, `ValidateProjectDirs`. Decide whether they are API or
  leftovers.
- [ ] **A stale git worktree lives at `.claude/worktrees/vigilant-ritchie-17429d/`**, a
  full untracked copy of the repository. It is not project code, but it makes every
  whole-tree grep return each hit twice.

### Backlog

Two items were filed and one was closed:

- **Filed** `decidir-o-idioma-dos-comentarios-63-blocos-em-portugues-numa` — the
  language decision above, with the full file list and both options written out.
- **Filed** `ligar-o-linter-unused-no-golangci-yml-hoje-custa-zero-0-issu` — turning
  `unused` on while it costs nothing.
- **Closed and removed** `comentarios-e-nomes-que-ainda-descrevem-os-artefatos-removid`
  — its three items are done: the `prepareUserMemory` doc block, the two stale test
  section headers, and the broad sweep for replica-era wording (which also turned up
  the dead `OnEmbedded` field). Everything it asked for is recorded in §8 above, so
  nothing was lost by removing it.

Nothing will pick these up automatically: `graphit_dream_status` reports the dream
module disabled in this repository, so the backlog is a record for a human, not a
queue.

## System Knowledge

- **`go/parser` is the only safe way to enumerate Go comments here.**
  `internal/dream/prompt.go` builds markdown containing `// ── Phase 1 ──`-shaped text
  inside raw string literals. A line-based regex sweep edits those strings and changes
  program output without failing the build.

- **A doc comment silently binds to the next declaration.** Two doc blocks written back
  to back with no blank line between them make the first one the documentation of the
  wrong function. Nothing warns — not `go vet`, not `golangci-lint`, and the code reads
  correctly top to bottom. Four instances existed. When adding a doc block above an
  existing one, check what `go doc` then renders.

- **`golangci-lint`'s `exclusions.presets: comments`** disables the staticcheck rules
  that require doc comments on exported identifiers (ST1020–ST1022), which is why
  removing or adding them does not move the lint result either way.

- **A CI step can be broken for months without looking broken.** `vulncheck` failed on
  a package-load error, not on a finding, so the output was a compiler diagnostic about
  a deliberately-undefined symbol rather than "vulnerabilities found". Anything that
  type-checks the tree — `vet`, `lint`, `govulncheck`, and any future analyser — needs
  `-tags fts5` here, because without it two packages *are* their guard file.

- **The `/api/jobs` endpoints were live and always empty**, which is worse than absent:
  a client integrating against them would have found a working route returning a
  well-formed empty list. Dead code behind an HTTP route does not look dead from
  outside.

## Progress Log

### 2026-08-15

- Verified `make build` green before touching anything (exit 0).
- Built the comment index with `go/parser`; measured 5,276 groups / 14,470 lines.
- Removed 870 lines of banner decoration across 65 files.
- Removed 532 restatement comments and 67 setup-narration comments, with explicit
  keep-lists for comments carrying an expectation, a rationale, or fixture-defining
  detail.
- Fixed 4 doc comments bound to the wrong declaration.
- Ran `golangci-lint run --enable unused`: 0 issues, so dead code hunting moved to
  exported symbols via the AST graph, each verified by a whole-repo identifier count.
- Removed 14 dead exported symbols, 2 dead files, and the `/api/jobs` subsystem;
  re-ran `unused` after each cascade — still 0 issues.
- `gofmt -w` over the project's Go files, which also cleared 55 files of pre-existing
  trailing-blank-line drift.
- First full `make ci` run failed at `vulncheck` — a pre-existing break unrelated to the
  cleanup (missing `-tags fts5`, since 2026-08-12). Fixed the target, which then reported
  5 stdlib vulnerabilities; bumped `go.mod` to `go 1.26.6` and they cleared.
- `make build` and `make ci` re-run to completion, green.
