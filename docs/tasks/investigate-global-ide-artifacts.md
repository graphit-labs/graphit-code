---
title: Investigate IDE artifacts in the global brand directory
status: complete
created: 2026-08-24
updated: 2026-08-24
tags: [investigation, ide, storage]
---

# Investigate IDE artifacts in the global brand directory

## Objective

Determine why IDE-oriented files, including `CLAUDE.md`, are being created under the
global brand directory (`~/.graphit`), identify the exact producer and intended storage
contract, and report whether the behavior is deliberate or a defect. The investigation
starts from the project's memory and knowledge indexes, then traces the responsible code
through the AST graph. On 2026-08-24 the user explicitly requested the fix after reviewing
the diagnosis, so the task now also carries the project directory through IDE adapter sync,
adds regression coverage, and removes the corresponding deferred backlog item.

## Plan & Task Breakdown

- [x] **T1 — Inventory the observed global artifacts** — Spec: inspect only the relevant
  paths under the resolved global brand directory; done when the suspicious files and
  their locations are known without mutating them.
- [x] **T2 — Trace each artifact to its producer** — Spec: use the AST graph to locate the
  functions that calculate and write IDE rule paths, plus their callers and tests; done
  when the complete write flow and trigger are established.
- [x] **T3 — Compare behavior with the documented storage contract** — Spec: use indexed
  project knowledge and provenance to decide whether the global placement is intentional;
  done when the expected location and any mismatch are explicit.
- [x] **T4 — Record and report the diagnosis** — Spec: update this log with evidence,
  system knowledge, and any technical debt; done when another agent can reproduce the
  finding and the user receives a concise root-cause explanation.
- [x] **T5 — Make IDE sync project-explicit** — Spec: change `hub.SyncIDEAdapter` and every
  production caller so the project directory is supplied through the full call chain and
  resolved with `paths.GetPathsForProject`; done when the MCP path cannot fall back to the
  daemon cwd and CLI behavior remains scoped to its requested `wd`.
- [x] **T6 — Add and run regression coverage** — Spec: add tests with cwd and target project in
  different temporary directories; done when the target receives its adapter files while
  cwd remains untouched, and every updated caller compiles.
- [x] **T7 — Verify the fix** — Spec: run focused and package tests, then exercise adapter
  sync with an isolated global root/project and prove no IDE output lands at the global root.
- [x] **T8 — Close records and indexes** — Spec: update this log and memory, remove the
  resolved backlog item, and refresh the AST, knowledge, and memory indexes without invoking
  the pre-fix daemon's unsafe full sync path.

## Implementation Details

The diagnosis found a project-directory context loss in the MCP sync path:

1. `internal/mcpstdio/tools_lifecycle.go` resolves the input `project_dir` and loads that
   project's lockfile, whose IDE list is `antigravity`, `kiro`, `claude`, and `codex`.
2. Its adapter phase calls `hub.SyncIDEAdapter(targetIDE, lf)` without passing the resolved
   directory.
3. `internal/hub/lifecycle.go:SyncIDEAdapter` calls `paths.GetPaths(ide, false)`, which
   discovers a project from git/cwd.
4. The daemon deliberately parks its cwd in `brand.GlobalDir()`, so this fallback resolves
   `ActiveProjectDir` to `~/.graphit`.
5. `FolderBasedAdapter.Sync` creates all configured type directories beneath that active
   directory even when there are no installed artifacts. The four adapters therefore create
   `.agents/`, `.kiro/`, `.claude/`, and `.codex/` trees.
6. `ClaudeAdapter.syncClaudeMD` unconditionally writes `<ActiveProjectDir>/CLAUDE.md`, which
   produced the observed managed block containing `@AGENTS.md`.

The global placement is therefore a correctness bug, not part of the documented storage
contract. The correction and its regression test are now implemented; the temporary backlog
item used during diagnosis was removed after completion.

The correction changes `SyncIDEAdapter` to accept `projectDir` and resolve paths with
`paths.GetPathsForProject`. The CLI passes its `wd`; the MCP sync handler passes its already
resolved `projectDir`; the exported coverage test passes its temporary directory. A dedicated
regression test changes cwd to one temporary directory, targets another, syncs Claude and
Codex, and asserts that only the target receives `.claude/`, `.codex/`, and `CLAUDE.md`.

## Use Cases

### UC-01: Diagnose unexpected IDE artifacts in global storage

- **Actor**: maintainer inspecting a Graphit installation.
- **Preconditions**: the global brand directory exists and contains IDE-looking files.
- **Main Flow**:
  1. Inventory the relevant files without changing them.
  2. Trace their path construction and write calls through the code graph.
  3. Compare the implementation with the documented storage layout.
  4. Explain the producer, trigger, and intended or defective placement.
- **Alternative Flows**:
  - If the files are stale artifacts from an older version, identify the historical producer.
- **Error Scenarios**:
  - If an index is stale or locked, check the daemon and synchronize before concluding.
- **Postconditions**: the origin and status of the files are known; no runtime artifact is modified.
- **Affected Files**: `docs/tasks/investigate-global-ide-artifacts.md`.

### UC-02: Synchronize IDE adapters for an explicit project

- **Actor**: MCP `graphit_sync` handler or CLI sync command.
- **Preconditions**: a target project directory with a Graphit lockfile and one or more
  supported IDEs; the process cwd may be unrelated to that project.
- **Main Flow**:
  1. The caller resolves the target project directory.
  2. The caller passes it to `hub.SyncIDEAdapter` with the target IDE and lockfile.
  3. `SyncIDEAdapter` constructs paths with `paths.GetPathsForProject`.
  4. The adapter writes only beneath the target project and IDE-owned user config paths.
- **Alternative Flows**:
  - CLI supplies its resolved working directory explicitly.
- **Error Scenarios**:
  - Unsupported IDE returns the existing adapter-resolution error without creating paths.
- **Postconditions**: no IDE project scaffolding or `CLAUDE.md` is written beneath the global
  brand directory merely because the daemon is parked there.
- **Affected Files**: `internal/hub/lifecycle.go`, `internal/mcpstdio/tools_lifecycle.go`,
  `cmd/graphit/commands/lifecycle.go`, and regression tests under `internal/hub/`.

## Test Cases & Acceptance Criteria

### Feature: Diagnose global IDE artifacts
Ref: UC-01

#### Scenario: Every observed artifact has a traced producer
```gherkin
Given IDE-oriented files exist under the resolved global brand directory
When their names and paths are traced through the indexed source graph
Then each current artifact is associated with a path resolver and writer
  And the invocation trigger is identified
```

#### Scenario: Documented placement is compared with implementation
```gherkin
Given the storage layout and IDE integration behavior are indexed in project knowledge
When the implementation path is compared with those documents
Then the diagnosis states whether global placement is intentional or a defect
  And any stale historical artifact is distinguished from current behavior
```

### Feature: IDE adapter sync honors the explicit project
Ref: UC-02

#### Scenario: Sync ignores an unrelated cwd
```gherkin
Given the process cwd is a temporary directory representing the daemon global root
  And a different temporary project directory is the adapter target
When Claude and Codex adapters are synchronized for the explicit project directory
Then the project's .claude and .codex directories are created
  And the project's CLAUDE.md contains the Graphit-managed block
  And the process cwd receives no .claude directory, .codex directory, or CLAUDE.md
```

#### Scenario: Unsupported IDE remains side-effect free
```gherkin
Given cwd and the explicit project directory are different temporary directories
When adapter synchronization is requested for an unsupported IDE
Then an unsupported IDE error is returned
  And neither directory receives adapter artifacts
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/investigate-global-ide-artifacts.md` | Created | Open and track the investigation before inspecting implementation details |
| `internal/hub/lifecycle.go` | Modified | Require an explicit project directory for exported adapter sync |
| `cmd/graphit/commands/lifecycle.go` | Modified | Pass the CLI sync working directory through the Hub boundary |
| `internal/mcpstdio/tools_lifecycle.go` | Modified | Preserve the MCP tool's resolved `project_dir` through adapter sync |
| `internal/hub/coverage_extra_test.go` | Modified | Adapt exported-function coverage to the explicit directory contract |
| `internal/hub/lifecycle_projectdir_test.go` | Modified | Regress cwd/global-root pollution with a distinct target project |
| `docs/tasks/backlog/graphit-memory-update-reports-success-but-compiled-memory-wi.md` | Created | Defer the independently discovered stale-memory-index defect with reproduction details |

## Trade-offs & Decisions

- The initial investigation remained read-only. The existing global files remain untouched
  because deleting user data requires explicit authorization; the implementation fix only
  prevents subsequent syncs from targeting the daemon cwd.
- The user subsequently authorized the correction. Keep the API change small and explicit:
  pass `projectDir` into the existing `SyncIDEAdapter` function rather than introduce another
  wrapper whose cwd semantics could be selected accidentally by server code.

## Technical Debt

- [x] Fix `SyncIDEAdapter` so server-side callers must pass an explicit project directory and
  use `paths.GetPathsForProject`; add a regression test with cwd distinct from `project_dir`.
  The resolved improvement-backlog item was removed.
- [ ] Investigate why `graphit_memory_update` and `graphit_memory_index` report success while
  `graphit_wiki_source` still serves the prior content. Tracked in
  `docs/tasks/backlog/graphit-memory-update-reports-success-but-compiled-memory-wi.md`.

## System Knowledge

- The global brand directory defaults to `~/.graphit` and may be relocated with
  `GRAPHIT_GLOBAL_DIR`; all inspection must use the resolved root rather than assuming the default.
- The current global root contains IDE trees `.agents/`, `.claude/`, `.codex/`, and
  `.kiro/`, plus a root-level `CLAUDE.md`; these were present together at the time of inspection.
- The directories were created at approximately 12:37 on 2026-08-24 and contained only
  empty adapter scaffolding at the inspected depth. The root `CLAUDE.md` was rewritten at
  17:01 and contained only the Graphit-managed `@AGENTS.md` block.
- `internal/hub/coverage_extra_test.go:TestSyncIDEAdapter_Exported` now supplies its temporary
  project explicitly instead of relying on a temporary cwd.
- This is a recurrence of the project-directory loss previously fixed in
  `OnInit`/`OnUpdate`/`OnRemove`; any empty project-dir argument or late call to
  `paths.GetPaths(..., false)` can reopen the cwd dependency.

## Progress Log

### 2026-08-24

- Opened the investigation after consulting project memory and knowledge about the global directory.
- Inventoried the global root: `.agents/`, `.claude/`, `.codex/`, `.kiro/`, and
  root-level `CLAUDE.md` are present. No runtime artifact was changed.
- User confirmed the IDE directories are part of the unexpected behavior, expanding T2
  from the root `CLAUDE.md` to the complete adapter output.
- Traced the exact call chain from the MCP sync handler through `hub.SyncIDEAdapter`,
  `paths.GetPaths`, `FolderBasedAdapter.Sync`, and `ClaudeAdapter.syncClaudeMD`.
- Compared the implementation with the documented global storage contract and classified
  the placement as a defect caused by lost project context.
- Extended existing memory `01KZQP4FM716BSNHDGKEKSJ3BM` with this recurrence and queued
  the code fix plus regression-test specification in the improvement backlog.
- Investigation complete. No runtime artifact was deleted or modified.
- User requested the fix. Reopened the task, reread its record, reapplied memory/wiki/AST
  lookups, and expanded the plan with implementation, regression, verification, and closure.
- Reran the pre-edit graph checks: three direct callers (`runSyncPhase1`, the MCP lifecycle
  registration, and exported coverage) plus existing indirect test reachability.
- T5 landed: `SyncIDEAdapter` now requires `projectDir`, uses `GetPathsForProject`, and both
  production callers pass the directory they already resolved.
- Added the T6 regression test with separate cwd and target directories. Next: format and run
  the focused tests, then expand verification in proportion to the cross-package signature change.
- Focused regression passed. The three-package run compiled and passed `internal/mcpstdio` and
  `cmd/graphit/commands`; `internal/hub` reached unrelated `TestIcebugArtifactMountsAndAnswers`
  and failed because the command omitted the repository's required `lancedb` build tag. No
  IDE-sync assertion failed. Next: consult the recorded test/build convention and rerun through
  the repository-supported test entry point or with the required tag.
- Reran the affected packages with `-tags lancedb`; `internal/hub`, `internal/mcpstdio`, and
  `cmd/graphit/commands` all passed. The cwd-versus-target regression is the isolated smoke
  proof: Claude and Codex outputs land only in the explicit target project.
- The full `make test` run passed the changed packages and the complete AST suite, then reported
  one unrelated transient failure in `TestPrepareASTPublishProducesOnlyIcebug` because its
  temporary Ladybug graph was absent. Repeating that exact test with the same `lancedb` and race
  settings passed, confirming it is not an IDE-sync regression.
- Updated existing memory `01KZQP4FM716BSNHDGKEKSJ3BM` with the recurrence, root cause, fix,
  and safety guidance; removed the now-resolved improvement-backlog item.
- Deliberately did not invoke the running daemon's full `graphit_sync`: that installed process
  still contains the pre-fix adapter call and would recreate the exact global IDE artifacts under
  investigation. Refreshed the AST, knowledge, and memory indexes independently instead.
- AST and knowledge verification reflected the current source and completed task log. The memory
  update and forced memory-index rebuild both reported success, but the compiled memory page still
  exposed its previous content; queued that independent indexing defect with its exact reproduction
  instead of expanding this fix.
