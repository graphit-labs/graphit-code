---
title: Rename dream subjects to the improvement backlog and move it into the docs tree
status: done
created: 2026-08-12
updated: 2026-08-12
tags: [refactor, naming, config, improvements, dream, backlog]
---

# Rename dream subjects to the improvement backlog and move it into the docs tree

## Objective

Deferred improvement suggestions were stored at `.graphit/dream/subjects/<slug>.md`. Two
problems with that, both raised by the Engineer:

1. **Location.** `graphit init` gitignores the brand directory, so work a review deliberately
   deferred was invisible to every other checkout, to code review, and to anyone not sitting at
   the machine that recorded it. The requested default was `docs/tasks/<something>`, overridable
   through the existing configuration mechanism (project / user / env precedence).
2. **Naming.** "dream" no longer describes the *suggestions* — the dream module is only the
   thing that consumes them — and "subjects" says nothing about what the files contain. The
   Engineer explicitly delegated the naming decision ("veja qual o melhor para o contexto
   atual") and asked for every code and documentation reference to be updated. No backward
   compatibility required: the project is pre-release.

## Implementation Details

### Naming decision

"dream subject" → **backlog item**, collectively the **improvement backlog**.

`backlog` is standard engineering vocabulary for work identified but not started, and
`docs/tasks/backlog/` reads correctly beside `docs/tasks/<task-log>.md`: the backlog holds what
has not been done, the task logs hold what has. An item's lifecycle is legible from the layout
alone.

### Scope boundary — what kept the "dream" name, and why

The rename was applied to the **queue**, not to the **runner**. `graphit dream status`,
`graphit dream reports`, `graphit_dream_status`, `graphit_dream_reports`, `modules.dream`,
`dream.idle_timeout`, `dream.max_duration`, `.graphit/daemon/dream.state`, `.graphit/dream/`
reports and `internal/dream` all keep their names: "dream" is an accurate and deliberate
metaphor for an autonomous session that runs while the developer is away. What was wrong was
calling the *work queue* a dream artifact when the queue is produced by review and merely
consumed by the session.

That boundary is why the MCP tools moved namespace rather than just changing suffix: the
Improvements module produces items, so the tools belong to it.

### Configuration

`internal/config/config.go` gained `DefaultBacklogDir` / `ResolveBacklogDir` for
`improvements.backlog_dir`, following the `ast.queries_dir` pattern (`TrimSpace` →
`filepath.Clean(filepath.FromSlash(...))`, returning a path relative to the project root).

The default is **composed, not fixed** — `filepath.Join(ResolveDocsDir(...), "tasks", "backlog")`
— so a project that relocates `knowledge.docs_dir` gets its backlog in the matching place with
no second key to set. This is the only `*_dir` key in the codebase whose default depends on
another key; every other one is a constant or a `brand.DotDir()` join.

No key registry or allow-list exists (`newConfigCmd` in `cmd/graphit/commands/lifecycle.go:307`
has no `ValidArgs`), so nothing had to be registered and shell completion needed no change. The
env var `GRAPHIT_IMPROVEMENTS_BACKLOG_DIR` is derived automatically by `ResolveConfig`.

### New package

`internal/dream/subjects.go` was deleted and became `internal/backlog/backlog.go`:

| Before (`dream`) | After (`backlog`) |
|---|---|
| `Subject` | `Item` |
| `SubjectsDir(projectDir)` | `Dir(projectDir)` — now config-aware |
| `AddSubject` | `Add` |
| `ListSubjects` | `List` |
| `PendingSubjects` | `Pending` |
| `RemoveSubject` | `Remove` |
| `PickSubject` | `Pick` |
| `subjectExt` / `resultExt` (private) | `ItemExt` / `ResultExt` (exported) |

A separate package rather than a file inside `internal/dream`: the Improvements module produces
items and the Dream module consumes them, and neither should import the other to reach the
queue. `Dir` resolves configuration itself via `config.LoadProjectConfig(projectDir)`, matching
how `internal/ast` reads project config without importing `internal/hub`.

`ResultExt` had to be exported because `internal/dream/prompt.go` builds the completion protocol
path from it.

### Serialization fixed on the way through

The old `Subject` struct had **no JSON tags**, so both the MCP tools and the HTTP API emitted
capitalized Go field names (`Slug`, `CreatedAt`, …) and `internal/ui/src/api/dream.ts` mirrored
them. `Item` carries proper snake_case tags (`slug`, `title`, `body`, `path`, `created_at`,
`done`, `result_path`), consistent with every other result struct in the codebase, and the
TypeScript interface was rewritten to match. `DreamStatusResult.PendingSubjects` →
`PendingBacklog` (`pending_backlog`) in both `internal/mcpstdio/tools_dream.go` and
`internal/uiserver/daemon_dream_handler.go`.

### Latent bug fixed

`DreamDashboard.tsx` hardcoded `` `${activeProjectDir}/.graphit/dream/subjects` `` and
`` `${activeProjectDir}/.graphit/dream` ``. The first would have broken the moment
`improvements.backlog_dir` was overridden; both assumed the literal `.graphit` rather than the
configured brand directory. Both now derive the directory and filename from the absolute path
the API already returns (`item.result_path`, `report.path`), so neither can drift again.

### Renamed surfaces

| Surface | Before | After |
|---|---|---|
| MCP | `graphit_dream_subject_list` | `graphit_improvements_backlog_list` |
| MCP | `graphit_dream_subject_add` | `graphit_improvements_backlog_add` |
| MCP | `graphit_dream_subject_remove` | `graphit_improvements_backlog_remove` |
| CLI | `graphit dream subject list\|add\|rm` | `graphit improvements backlog list\|add\|rm` |
| HTTP | `GET /api/dream/subjects` | `GET /api/backlog` |
| HTTP | `POST /api/dream/subject` | `POST /api/backlog/item` |
| HTTP | `DELETE /api/dream/subject/{slug}` | `DELETE /api/backlog/item/{slug}` |
| Prompt | `## 🎯 Assigned Subject` | `## 🎯 Assigned Backlog Item` |
| Prompt | `subject: <slug>` frontmatter | `backlog_item: <slug>` |

### Agent-facing text

`internal/improvements/rules.go`: the section became
`## Work You Are Not Going to Do Now — The Improvement Backlog`, now naming the default
directory (rendered from `config.DefaultBacklogDir`, so the prose cannot drift from the code)
and the override key. The reflection template line became `Backlog items left: <count>`.

The precondition warning was rewritten rather than just renamed. It used to imply a queued item
was worthless without the daemon; now that items are committed, the accurate statement is that
recording always has value (a human finds it in review) and only *automatic action* is
conditional on `modules.dream` plus a running daemon.

`internal/improvements/rule.go`: mandate tool list and skill frontmatter description.
`internal/hub/rule.go`: a new row in the configuration troubleshooting table. Both files now
import `internal/config`.

Regenerated artifacts (git-tracked): `AGENTS.md`, and the `graphit-improvements` + `graphit-hub`
`SKILL.md` under `.agents/`, `.claude/`, `.kiro/`. `.claude/settings.local.json` permission
entries were remapped to the new tool names.

## Use Cases

### UC-01: An agent defers an out-of-scope finding during review
- **Actor**: AI agent following the improvements methodology
- **Preconditions**: Project initialised; a review is in progress; the agent has found a real problem outside the scope it was given
- **Main Flow**:
  1. The agent calls `graphit_improvements_backlog_list` to check the item is not already queued
  2. The agent calls `graphit_improvements_backlog_add` with a one-line `title` and a `body` naming paths, symptom, what was ruled out, and how to verify
  3. `backlog.Add` resolves `Dir(projectDir)` through `config.ResolveBacklogDir`, creates it with `MkdirAll` (0o755), slugifies the title, and writes `# <title>\n\n<body>` at 0o644
  4. The agent calls `graphit_dream_status` and reports both the item left **and** whether `enabled` + `daemon_running` mean anything will action it
- **Alternative Flows**:
  - `body` omitted → the file contains only the `# <title>` heading and a blank line
  - `improvements.backlog_dir` set → the item is written there instead of `docs/tasks/backlog`
  - `knowledge.docs_dir` set and `improvements.backlog_dir` unset → the default follows the docs tree
- **Error Scenarios**:
  - Title slugifies to empty → `title produces an empty slug — use alphanumeric characters`
  - Slug already exists → `backlog item %q already exists at %s`; nothing is overwritten
  - Directory not creatable → `creating backlog dir: %w`
  - Directory not writable → `writing backlog item: %w`
- **Postconditions**: `<slug>.md` exists in the backlog directory, is tracked by git, and is indexed into the knowledge wiki by the docs watcher
- **Affected Files**: `internal/backlog/backlog.go`, `internal/mcpstdio/tools_improvements.go`, `internal/config/config.go`

### UC-02: A dream session picks up the oldest pending item
- **Actor**: Dream runner (`internal/dream.Runner`), triggered by the daemon
- **Preconditions**: `modules.dream` is `"true"`; daemon running; project idle beyond `dream.idle_timeout`; state not `Exhausted`
- **Main Flow**:
  1. `tick` passes its precondition checks and launches the session goroutine
  2. `executeDream` creates `.graphit/dream/`, runs memory consolidation, then calls `backlog.Pick(projectDir)`
  3. `Pick` returns the oldest item with no `.done.md` counterpart, or nil
  4. `buildDreamPrompt(projectDir, ulid, ide, item)` emits the `## 🎯 Assigned Backlog Item` section, the item body, and a completion protocol naming `<backlog dir>/<slug>.done.md`
  5. The session writes its report to `.graphit/dream/<ulid>.md` with `backlog_item: <slug>` in the frontmatter
- **Alternative Flows**:
  - Backlog empty → `item` is nil, no item sections are emitted, and the session does general conversation mining
  - `Pick` errors (unreadable directory) → error swallowed, treated as "no item", session proceeds
- **Error Scenarios**:
  - Artifact dir not creatable → `creating dream artifact dir: %w`, session aborts
  - Agent execution fails → `executing dream agent: %w`, logged, no report written
- **Postconditions**: A report exists; if the agent wrote `<slug>.done.md`, the item is no longer pending
- **Affected Files**: `internal/dream/dream.go`, `internal/dream/prompt.go`, `internal/backlog/backlog.go`

### UC-03: A developer manages the backlog from the CLI
- **Actor**: Developer
- **Preconditions**: Shell in the project root
- **Main Flow**:
  1. `graphit improvements backlog list` prints totals and, per item, status, slug, creation time, and the path relative to the project root
  2. `graphit improvements backlog add "<title>" [--body "<brief>"]` writes the item and echoes its slug and relative path
  3. `graphit improvements backlog rm <slug>` removes `<slug>.md` and, best-effort, `<slug>.done.md`
- **Alternative Flows**:
  - Empty backlog → guidance to add one instead of an empty table
  - Tab-completion on `rm` → `completionBacklogSlugs` offers pending slugs only
- **Error Scenarios**:
  - Unknown slug → `backlog item %q not found`
  - Unreadable directory → `listing the backlog: %w`
- **Postconditions**: The backlog directory reflects the operation
- **Affected Files**: `cmd/graphit/commands/backlog.go`, `cmd/graphit/commands/improvements.go`, `cmd/graphit/commands/completions.go`

### UC-04: The UI dashboard reads a completed item's result
- **Actor**: Developer in the web dashboard
- **Preconditions**: uiserver running; a done item exists
- **Main Flow**:
  1. `DreamDashboard` fetches `GET /api/backlog` and renders the "Improvement Backlog" list
  2. Clicking a done item calls `viewItemResult`, which splits `item.result_path` into directory and filename
  3. It fetches `/api/wiki/page?dir=<dir>&path=<file>` and renders the markdown
- **Alternative Flows**:
  - Pending item clicked → the queued view with its instructions, no fetch
  - Session report clicked → same split applied to `report.path`
- **Error Scenarios**:
  - `result_path` empty → `this item has no result file yet` toast
  - Non-2xx → `HTTP <status>` toast
- **Postconditions**: Content rendered; no path was reconstructed from a hardcoded directory
- **Affected Files**: `internal/ui/src/api/dream.ts`, `internal/ui/src/components/dream/DreamDashboard.tsx`, `internal/uiserver/daemon_dream_handler.go`

## Test Cases & Acceptance Criteria

### Feature: Backlog location resolution
Ref: UC-01

#### Scenario: Default location sits inside the documentation tree
```gherkin
Given a project at "/tmp/testproj" with no backlog configuration
When Dir is called for that project
Then it returns "/tmp/testproj/docs/tasks/backlog"
```

#### Scenario: An explicit override wins over the default
```gherkin
Given GRAPHIT_IMPROVEMENTS_BACKLOG_DIR is set to "custom/queue"
When Dir is called for the project
Then it returns the project root joined with "custom/queue"
  And the docs tree is not consulted
```

#### Scenario: The default follows the documentation tree
```gherkin
Given GRAPHIT_KNOWLEDGE_DOCS_DIR is set to "documentation"
  And improvements.backlog_dir is not set
When Dir is called for the project
Then it returns the project root joined with "documentation/tasks/backlog"
```

### Feature: Backlog item lifecycle
Ref: UC-01, UC-02, UC-03

#### Scenario: Add, list, pick, complete, remove
```gherkin
Given an empty backlog
When an item titled "My Backlog Item" is added
Then its slug is "my-backlog-item"
  And List returns exactly one item
  And Pending returns exactly one item
  And Pick returns that item
When a file "my-backlog-item.done.md" is written beside it
Then List reports the item as done
  And Pending returns no items
When the item is removed
Then List returns no items
```

#### Scenario: Duplicate titles are refused rather than overwriting
```gherkin
Given an item titled "My Backlog Item" already exists
When another item with the same title is added
Then an error is returned
  And the existing file is left untouched
```

#### Scenario Outline: Titles that cannot produce a slug are rejected
```gherkin
Given an empty backlog
When an item is added with the title "<title>"
Then the result is "<outcome>"

Examples:
  | title           | outcome |
  |                 | error   |
  | "   "           | error   |
  | "abc"           | success |
```

#### Scenario Outline: Slug boundary lengths
```gherkin
Given a title of "<length>" alphanumeric characters
When it is slugified
Then the slug is "<slug_length>" characters long

Examples:
  | length | slug_length |
  | 59     | 59          |
  | 60     | 60          |
  | 61     | 60          |
  | 100    | 60          |
```

#### Scenario: Removing an item also removes its result file
```gherkin
Given a done item with both "<slug>.md" and "<slug>.done.md"
When the item is removed
Then both files are gone
```

#### Scenario: A missing backlog directory is empty, not an error
```gherkin
Given a project whose backlog directory does not exist
When List is called
Then it returns no items
  And no error
```

#### Scenario: An unreadable item falls back to its slug as the title
```gherkin
Given a backlog containing "unreadable.md" with mode 0o000
When List is called
Then no error is returned
  And the item's title is "unreadable"
```

#### Scenario: An unreadable backlog directory is an error
```gherkin
Given a backlog directory with mode 0o000
When List is called
Then an error is returned
```

### Feature: HTTP contract
Ref: UC-04

#### Scenario: The wire format is snake_case
```gherkin
Given an item was added through "POST /api/backlog/item"
When "GET /api/backlog" is requested
Then the response is 200
  And each element has a string "slug" key
  And each element has a "created_at" key
When "DELETE /api/backlog/item/{slug}" is requested for that slug
Then the response is 200
```

#### Scenario: project_dir is mandatory
```gherkin
Given no project_dir query parameter
When "GET /api/backlog" is requested
Then the response is 400
```

#### Scenario: dream status reports the pending backlog
```gherkin
Given one pending item titled "Test Item"
When "GET /api/dream/status?project_dir=<dir>" is requested
Then "pending_backlog" contains exactly one entry
  And that entry is "Test Item"
```

### Feature: Agent-facing skill text
Ref: UC-01

#### Scenario: The skill teaches the current tool names
```gherkin
Given the resolved improvements skill content
Then it mentions graphit_improvements_backlog_add, _list and _remove
  And it mentions graphit_dream_status and graphit_dream_reports
  And it does not mention graphit_dream_subject_add, _list or _remove
```

#### Scenario: The skill names where the backlog lives
```gherkin
Given the resolved improvements skill content
Then it contains the default backlog directory
  And it contains "improvements.backlog_dir"
```

#### Scenario: The skill warns about the preconditions
```gherkin
Given the resolved improvements skill content
Then it warns that the next agent inherits no conversation history
  And it warns that the dream module is opt-in
  And it warns that reading reports marks them as read
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/backlog/backlog.go` | Created | The concept's new home: `Item`, `Dir`, `Add`, `List`, `Pending`, `Remove`, `Pick` |
| `internal/backlog/backlog_test.go` | Created | Ported lifecycle/error tests plus three new location-resolution tests |
| `internal/dream/subjects.go` | Deleted | Relocated to `internal/backlog` |
| `internal/config/config.go` | Modified | `DefaultBacklogDir`, `ResolveBacklogDir`, `backlogParentDirName`, `backlogDirName` |
| `internal/dream/dream.go` | Modified | `executeDream` uses `backlog.Pick`; log message renamed |
| `internal/dream/prompt.go` | Modified | Takes `*backlog.Item`; item vocabulary; `backlog.Dir`/`backlog.ResultExt` in the completion protocol |
| `internal/dream/dream_test.go` | Modified | Subject tests removed (moved); prompt tests use `backlog.Item` |
| `internal/mcpstdio/tools_dream.go` | Modified | Three subject tools removed; `PendingSubjects` → `PendingBacklog` |
| `internal/mcpstdio/tools_improvements.go` | Modified | Registers the three `improvements_backlog_*` tools |
| `internal/improvements/rules.go` | Modified | Backlog section, location prose, reflection template |
| `internal/improvements/rule.go` | Modified | Mandate tool list, skill description |
| `internal/improvements/rule_dream_test.go` | Deleted | Replaced by `rule_backlog_test.go` |
| `internal/improvements/rule_backlog_test.go` | Created | Asserts new tools present, old tools absent, location named |
| `internal/hub/rule.go` | Modified | Config table row for `improvements.backlog_dir` |
| `cmd/graphit/commands/backlog.go` | Created | `graphit improvements backlog {list,add,rm}` |
| `cmd/graphit/commands/dream.go` | Modified | Subject subtree removed; help text; status shows pending backlog |
| `cmd/graphit/commands/improvements.go` | Modified | Wires `newBacklogCmd` |
| `cmd/graphit/commands/completions.go` | Modified | `completionDreamSubjectSlugs` → `completionBacklogSlugs` |
| `internal/uiserver/daemon_dream_handler.go` | Modified | Backlog routes and handlers; `pending_backlog` |
| `internal/uiserver/daemon_dream_handler_test.go` | Modified | New routes; asserts snake_case wire keys |
| `internal/uiserver/daemon_dream_extended_test.go` | Modified | New routes and package |
| `internal/uiserver/uiserver_final_coverage_test.go` | Modified | New routes |
| `internal/ui/src/api/dream.ts` | Modified | `BacklogItem` with snake_case; backlog endpoints |
| `internal/ui/src/components/dream/DreamDashboard.tsx` | Modified | Backlog vocabulary; hardcoded paths replaced with API-provided paths |
| `internal/brand/brand_test.go` | Modified | Example tool updated to a live one |
| `docs/specs/backlog.md` | Created | Specification of the concept |
| `docs/specs/dream_module.md` | Modified | Consumes the backlog; queue no longer listed as a dream artifact |
| `docs/specs/config_module.md` | Modified | Key table + a section on the composed default; added the previously undocumented `dream.*` and `daemon.activity_window` keys |
| `docs/specs/improvements_module.md` | Modified | Backlog section; reflection step 4 |
| `docs/specs/mcpstdio_module.md` | Modified | Tool tables and dependency list |
| `docs/guides/mcp_tools_reference.md` | Modified | Tools moved to the Improvements section |
| `docs/guides/cli_reference.md` | Modified | `dream` and `improvements` subcommands |
| `docs/guides/user_manual.md` | Modified | "Queueing Work — the Improvement Backlog" |
| `docs/guides/troubleshooting.md` | Modified | Fixed the non-existent `dream.enabled` key; new backlog-location entry |
| `docs/README.md`, `README.md` | Modified | Index the new spec |
| `AGENTS.md`, `.agents/.claude/.kiro` improvements+hub `SKILL.md` | Regenerated | Generated from the rule sources, git-tracked |
| `.claude/settings.local.json` | Modified | Permission entries remapped to the new tool names |

## Trade-offs & Decisions

- **Renamed the queue, not the runner.** Renaming `internal/dream`, `modules.dream`,
  `dream.state`, the reports directory and `graphit_dream_status`/`_reports` would have been a
  far larger diff for no gain: "dream" describes the idle autonomous session accurately. The
  Engineer's objection was to calling the *suggestions* a dream artifact, which the move out of
  `.graphit/dream/` resolves on its own.
- **`improvements.backlog_dir`, not `dream.backlog_dir`.** The producing module owns the key, so
  the key does not carry the name that was just rejected.
- **Composed default over a constant.** `docs/tasks/backlog` hardcoded would silently contradict
  a project that moved `knowledge.docs_dir`. The cost is the only composed default in the config
  package, which the spec now calls out explicitly.
- **Own package rather than a file in `internal/dream`.** Slightly more structure, but it stops
  the improvements-facing tools from importing the dream package to reach a queue that dream
  does not own.
- **Added JSON tags, accepting a wire-format change.** The old capitalized keys were an accident
  of a missing struct tag. Since no backward compatibility was required, fixing it now was
  cheaper than carrying the inconsistency; a test asserts the new contract.
- **Accepted that backlog items are indexed into the knowledge wiki.** A consequence of the
  location the Engineer chose. Judged a feature — the backlog becomes searchable next to the task
  logs — but it does mean pending *proposals* now appear in wiki results alongside descriptions
  of what the system actually does. Documented rather than suppressed with a `.wikiignore` entry.
- **Fixed the `viewGeneralReport` hardcoded path too.** Strictly adjacent to the request, but it
  was the same three-line bug in the same function being edited, and it removed a second
  hardcoded `.graphit` assumption.

## Technical Debt

All three items opened by the first pass were closed the same day — see the 2026-08-12
follow-up in the Progress Log.

- [x] `scanDreamReportsLocal` / `extractFrontmatterTitleLocal` / the dream last-seen file helpers
      existed in **three** copies — `cmd/graphit/commands/dream.go`, `internal/mcpstdio/tools_dream.go`,
      `internal/uiserver/daemon_dream_handler.go`. Extracted into `internal/dream/reports.go`.
- [x] The dream reports directory staying in the gitignored brand directory is now an explicit,
      documented decision rather than inertia — see *Why the reports stay in the brand directory*
      in `docs/specs/dream_module.md`.
- [x] `internal/ui/src/api/dream.ts` no longer hosts the backlog: it split into `api/backlog.ts`
      (`backlogApi`, `BacklogItem`) and `api/dream.ts` (`dreamApi`, `DreamStatus`, `DreamReport`).

## System Knowledge

- **The FTS5 build tag is mandatory.** `go build ./...` fails with
  `undefined: graphit_requires_the_fts5_build_tag____run_make_test_or_pass_tags_fts5` in
  `internal/ast` and `internal/wiki`. Always `-tags fts5` (`BUILD_TAGS` in the Makefile).
- **`go vet ./...` is noise without `-unreachable=false`.** The generated ANTLR parsers emit
  thousands of unreachable-code warnings; the Makefile's `vet` target disables that check and
  skips `internal/ast/antlr`.
- **Rules and skills are generated *and* committed.** `AGENTS.md` and the three IDE `SKILL.md`
  trees are git-tracked but produced by `InstallRule`/`InstallSkill`, invoked through
  `installAllRules` from `graphit sync` / `graphit update`. Editing the `.md` by hand is wrong;
  editing the Go builder and regenerating is right. Registered IDEs come from `ides` in
  `graphit.lock.json` (here: antigravity, kiro, claude).
- **There is no config key registry.** `newConfigCmd` accepts free-form dotted keys with no
  validation, so adding a key is purely additive — but also means a typo is silently accepted
  and simply never read.
- **`ResolveConfig` derives env vars automatically**: `improvements.backlog_dir` →
  `GRAPHIT_IMPROVEMENTS_BACKLOG_DIR`, and an env var outranks both config files while appearing
  in neither `graphit config --list` output.
- **File permissions are deliberately split.** A 2026-05-31 security pass tightened
  machine-state files (AST config with API keys, daemon logs, PID and port files) to `0o600`.
  Backlog items stay `0o644`/`0o755`: they are documentation, committed and meant to be read.
- **`ladybug`/LOWER can fail on the AST graph.** `toLower()` in a Cypher query over all nodes
  returned `Runtime exception: Failed calling LOWER: Invalid UTF-8` — some node name in this
  repository's graph is not valid UTF-8. Pin labels and use case-sensitive `CONTAINS` instead.

## Progress Log

### 2026-08-12
- Mapped the full surface: `internal/dream`, every caller, the agent-facing text, the generated
  artifacts, and all documentation.
- Chose "improvement backlog" / "backlog item" and set the scope boundary at queue-vs-runner.
- Added `improvements.backlog_dir` with a default composed from `knowledge.docs_dir`.
- Created `internal/backlog`; deleted `internal/dream/subjects.go`; split the tests between the
  two packages.
- Renamed the MCP, CLI, HTTP and UI surfaces; added JSON tags; removed two hardcoded
  `.graphit/dream…` paths from the dashboard.
- Updated the skill/rule builders and regenerated `AGENTS.md` plus the three IDE skill trees.
- `go build -tags fts5 ./...`, `go vet`, `golangci-lint` (0 issues), `npx tsc --noEmit`,
  `npm run lint` and `make test` all green.
- Rewrote the documentation and wrote this log.
- Note: the graphit MCP tools went offline partway through the session
  (`MCP server "graphit-code-stdio-mcp" is not connected`); the remaining work used native
  tooling, which was disclosed to the Engineer at the time.

### 2026-08-12 — follow-up: the three debt items closed

Committed the rename as `b7b67a0`, then cleared all three items above.

**1. The triplicated reports scanner → `internal/dream/reports.go`.**

The Decision Validation Gate found a precedent that had to be ruled out first:
`docs/tasks/security-fix-cors-headers.md` records a deliberate choice to **duplicate**
`isAllowedOrigin` across three packages rather than extract it, on the grounds that it was
"small (10 lines), stable, and the packages are in different domains", so "extracting to a
shared package would add coupling for minimal benefit".

That precedent does not cover this case, and its own criteria are why:

| Criterion from the CORS decision | The reports scanner |
|---|---|
| Small — 10 lines | ~100 lines per copy, ~300 total |
| Different domains | Same domain: all three read the dream reports vault |
| Extraction would add coupling | Adds **none** — all three callers already import `internal/dream` |

There was also no `DECISION`/`NOTE` comment protecting the duplication. So the extraction went
ahead, and the precedent is respected rather than contradicted: it is a rule about small
cross-domain helpers, not about three packages reimplementing one module's own file format.

New API in `internal/dream/reports.go`: `Report`, `LastSeen`, `ReportsDir`, `ListReports`,
`ReportsSince`, `LoadLastSeen`, `MarkReportsSeen`, plus private `lastSeenPath` and `reportTitle`.
Two behaviours were promoted into the contract so callers stop re-implementing them:

- **`ListReports` returns newest-first.** All four call sites sorted descending afterwards; one
  of them (`runDreamStatus`) only sorted to take the head.
- **A missing reports directory yields `(nil, nil)`.** Two call sites pre-checked with `os.Stat`
  purely to avoid the error.

It also uses the existing `exhaustedSentinel` constant and `fileExists` helper instead of
re-hardcoding `".exhausted"`, and `dream.go` now calls `ReportsDir` in the three places that
rebuilt `filepath.Join(projectDir, brand.DotDir(), "dream")` inline.

Net: **-343 lines** across the three copies, **+163** in one owner. `internal/dream` coverage
92.7%.

The three duplicated *test* suites collapsed into `internal/dream/reports_test.go`. Two cases
that existed only in the uiserver copy — a title appearing after other frontmatter fields, and
frontmatter with tags but no title — were carried over rather than dropped. A new case asserts
that `dream_last_seen.json`, which lives in the same directory, is not mistaken for a report.

**2. The reports vault stays in the brand directory — deliberately.**

Recorded in `docs/specs/dream_module.md`. The backlog moved because it is *intent*; the vault
holds *output* (`<ulid>.md`) and *machine state* (`<ulid>.exhausted`, `dream_last_seen.json`).
The last-seen marker decides it on its own: it records which reports **this developer** has
read and is rewritten on every `graphit dream reports`, so versioning it would have one
developer's reading position overwrite another's with a merge conflict on every read. Splitting
the directory is left as a future `dream.reports_dir` key, following the
`improvements.backlog_dir` pattern, for the day someone actually wants committed reports.

**3. `api/dream.ts` split.**

`api/backlog.ts` now owns `BacklogItem` and `backlogApi` (`list`/`add`/`remove`); `api/dream.ts`
keeps `DreamStatus`, `DreamReport` and `dreamApi` (`getStatus`/`getReports`). The dashboard
imports both. The file name no longer misdescribes its contents, and the method names lost the
redundant `Backlog` prefix now that the module says it.

Verified: `go build -tags fts5 ./...`, `golangci-lint` (0 issues), `gofmt` clean on every
touched file, `npx tsc --noEmit`, `npm run lint`, full `make test`. The report behaviour was
also exercised against a real binary: newest-first ordering, deep-sleep sentinel detection,
non-`.md` files and the last-seen marker excluded, frontmatter titles parsed, "1 new of 2 total"
after adding a report, and a missing directory reported as "the dream module has not run yet"
rather than an error.
