---
title: Ignored project directory in lifecycle tools, written instrumented, and cleans
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [mcp, hub, ast, escrita, performance, ui, concorrencia]
---

Ignored in tools of the lifecycle cycle, scripted, and cleans

## Objective

Five fronts, exits from a performance analysis and a real incident:

1. The project accepted __INLINE_0__ and removed files from another project. Correct and audit if any other tool does the same.
2. Assign the indexing phase — 80% of time on a single node — and act according to measurement.
3. Writing did not report progress, so it seemed stuck.
4. Audit the `select` with the form that gate accepted an abandoned context.
5. Reset the 26 warnings from __INLINE_3__.

## Implementation Details

The **__INLINE_4__** that was not honored

The processes `hub.OnInit`, `hub.OnUpdate`, and `hub.OnRemove` resolved the entire path with
`paths.GetPaths(ide, false)`, which starts from the working directory of the PROCESS. This is correct for CLI within the project,
and wrong for the MCP server: a long-running process stopped in a project while its tools received an `project_dir` that names another one.
The `graphit_remove` called with a temporary directory removed the IDE adapter, rules, the `CLAUDE.md` and the global registration **of the repository where the server was running**.

The three functions began to receive __INLINE_12__ and started using __INLINE_13__, which already existed. __INLINE_14__ preserves the old behavior, which is what the CLI passes.

The first fixed the main path and **not enough**: `OnRemove` still called
`svc.UninstallAll(ctx, ide, "")` and `OnUpdate` still called `svc.UpdateAll(ctx, ide, "")` — and an empty string is exactly what causes `GetPathsForProject` to fall back into the working directory. That was where the `graphit.lock.json` of the repository continued being deleted, which only appeared because the new test erased it again.

---

Note: The inline code blocks (e.g., `OnRemove`) are placeholders and should be replaced with actual content or removed if not needed for translation.

Also used `git.NewHookManager("")` now `git.NewHookManager(projectDir)`.

**Audit** (what the task required): The only points that accepted a project and resolved it were these. In CLI (`cmd/graphit/commands/lifecycle.go`), the working directory is the project, so `NewHookManager("")` remains correct there and was left as it is.

**Side effect of the same bug:** The test suite for __INLINE_26__ mutated the __INLINE_27__ REAL user data. Correcting the transfer of __INLINE_28__ resolved both — verified by hash before and after running the entire package.

2. The writing was done before optimization.

It passed to separately count the serialization (__INLINE_30__) and ingestion (__INLINE_31__), and report __INLINE_32__, __INLINE_33__, and __INLINE_34__ in the log of __INLINE_35__.

Measured against this repository's actual cache (842 files, 216,099 lines):

The total rebuild time is 11.16 seconds.

By call: 124 COPS, and **a single line copy takes between 21-132 milliseconds**. Copies with fewer than 50 lines cost on average 26 milliseconds; copies with 50 or more take 59 milliseconds. This is by call, not volume.

Two optimizations were tested and discarded, with measurement:

- **Wrap the COPS in a transaction.** `copy_s` dropped from 6.50 seconds to 3.00 seconds, but the total rebuild time remained at 10.90 seconds compared to 11.16 seconds — only time migrated to COMMIT, which was not counted. In addition, it would no longer gain by changing partial error semantics: today, a failed COPY is counted and the swap is aborted, preserving the old database.
- **Parallelize the COPS.** `LadybugBackend.execQuery` acquires a mutex on a single connection, so concurrency on the Go side only produces a queue in the same lock. To do it truly would require multiple connections to the same database, which this project already knows is perilous terrain.

What remains is the instrumentation, which allows this to be decided anew with data.

Progress during writing

The report is inline 38 and reports as entries come in; inline 39 delegates to it.
With inline 40, the nine existing call sites remain intact. The pipeline connects the callback to inline 41, and the CLI relater now prints inline 42.

### 4. The `select` where cancellation loses the draw

Three candidates, treated according to what each one does:

- `internal/fswatch/fswatch.go:272` — **corrected**. The batch proceeds to the `SyncModule`, which is
  the path of heavy work; an `ctx.Err()` before the select costs a branch.
- `internal/ast/pipeline.go:407` and `:431` — **as they are**. Both loops already test `ctx.Err()` at the top, and losing the draw results in either a path or more processed result. Extra code there would be noise.

### 5. `ui-lint`

26 → 0. Twenty-two were `no-unused-vars` (imports, arguments, dead variables, and four
`catch (err)` that turned into `catch {}`). One `no-explicit-any` became `isValidElement` with the declared type. Of the three `exhaustive-deps`, two asked for `containerRef`/`getNodeColor`, which are stable and already depended on by another hook in the same file — included. The third, in `SubmitModal`, seeds the form when the modal opens: including
`gitAuthor`/`isUpdate`/`existingScope` would cause the form to run again with the modal open and overwrite what the user had entered, so there is a `eslint-disable-next-line` with the reason written.

## Use Cases

### UC-01: Remove Graphit from a Project via the MCP Tool

**Actor**: MCP tool `graphit_remove`

**Preconditions**: The MCP server runs in project A; the call names project B.

**Main Flow**:
1. Handler resolves `projectDir` = B.
2. `git.NewHookManager(B)` removes hooks from B.
3. `RemoveGitignore` ages in `.gitignore` of B.
4. `hub.OnRemove(..., B)` builds paths with `GetPathsForProject(ide, B)`.
5. IDE adapter removes `pp.ActiveProjectDir`, which is B.
6. Since it's the last IDE, `UninstallAll(ctx, ide, B)` uninstalls artifacts from B and deregisters the project from the global lock.

**Alternative Flows**:
- `project_dir` absent: resolves to work directory, as CLI.

**Error Scenarios**:
- Registry unavailable: local removal follows; event not tracked.

**Postconditions**: nothing outside of B is touched — particularly the project where the server runs.
- Affected Files: `internal/mcpstdio/tools_lifecycle.go`, `internal/hub/lifecycle.go`.

### UC-02: Monitor the Graph Writing Process

**Actor:** The `graphit ast index` actor of the daemon.
**Preconditions:** Parsing completed with input in cache.
**Main Flow:**
1. The pipeline emits `OnProgress("writing", 0, files, errors)` as it enters the phase.
2. `RebuildFromJSONWithProgress` calls the callback at each COPY, with the accumulated lines.
3. The CLI rewrites the progress line with lines and files.

**Error Scenarios:**
- `onProgress` is nil: the rebuild proceeds identically — this is the path of the other nine callers.

**Postconditions:** The longest phase becomes silent.

**Affected Files:** `internal/ast/json_rebuild.go`, `internal/ast/pipeline.go`, `cmd/graphit/commands/runners.go`

## Test Cases & Acceptance Criteria

Feature: The tools honor the project received

Ref: UC-01

Scenario: Remove Age from Project Informed
```gherkin
Given a temporary project with a lockfile registering the IDE "Claude"
And a process whose working directory is another project
When `OnRemove` is called with the temporary directory
Then it is removed from the lock file in the temporary directory.
```

Scenario: Remove without uninstalling from work directory
```gherkin
Given an interim project with only one registered ID, "Claude"
And an existing graphit.lock.json file in the working directory
When `OnRemove` is called with the temporary directory
The graphit.lock.json file in the working directory remains present.
```
Note: This scenario reproduces the actual damage. Without correction in `UninstallAll`, running the test would erase the lockfile of its own repository.

Scenario: The paths are within the project specified
```gherkin
Given a temporary project directory
When the paths are resolved for this project
Then o lockfile fica dentro dele
And it does not match with the working directory
```

## Files Changed

File | Change | Reason  
--- | --- | ---  
Inline 85 | Modified | Inline 86/87/88 receive Inline 89; Inline 90/91 stop receiving empty string  
Inline 92 | Modified | Passes resolved Inline 93 to Inline 94  
Inline 95 | Modified | Passes Inline 96 while preserving CLI behavior  
Inline 97 | Created | Fixes the three properties above  
Inline 98 | Modified | Instrumentation for serialize/copy/rows; variant with progress  
Inline 99 | Modified | Links progress of writing to Inline 100  
Inline 101 | Modified | Shows lines written during phase  
Inline 102 | Modified | Cancels batch send when inline 103 (12 files) is modified  
Inline 103 (12 files) | Modified | 26 lint warnings → 0

## Trade-offs & Decisions

- **New parameter instead of new variant in lifecycle hooks.** There are three functions with few callers; one variant `WithProject` for each tripled surface.
- **New variant instead of parameter in rebuild.** This is the opposite: nine callables, only one needs progress.
- **Instrumentation remains, but optimizations do not.** Transactions and parallelism were measured and did not pay—first improved time, second hit a single connection issue.
- **An `eslint-disable` with reason, not complete dependencies.** In `SubmitModal` the lint rule and intent of effect disagree, and following the lint would break user input integrity.

Note: The code blocks (`WithProject`, `eslint-disable`, `SubmitModal`) are placeholders for specific lines or sections that should be replaced with actual code snippets.

## Technical Debt

- [ ] The call latency of ~25 ms per invocation of `COPY` remains the largest component of writing and is not explained — whether it's query planning, implicit transaction, or flush would decide if there’s anything to gain by reducing the number of tables rebuilt.
- [ ] `IncrementalRebuild` does not report progress; only the complete rebuild reports.
- [ ] No entity in this project fails today with tokenization, so the warning that names were not collected. If the crash-loop reappears in another corpus, it will be him who gives the names.

## System Knowledge

- **`paths.GetPaths` starts from the working directory of the process; `GetPathsForProject`
  accepts the project.** On an MCP server, the first is almost always wrong, and the error is
  silent: the operation works on the wrong project.
- **Passing `""` as a project is not neutral** — it falls back to the working directory. This was how the first half of the correction went unnoticed.
- **Tests that call the hub hooks touch the user's real environment when the project is not passed: the `~/.graphit/global.lock.json` is resolved by `brand.GlobalDir()`, without path injection.
- **Graph writing is dominated by cost per COPY call, not volume** — a COPY of one line costs hundreds of milliseconds.

## Progress Log

### August 11, 2026
- Bug _INLINE_114 reproduces and is fixed across other points; covered by tests.
- Partial fix found in the test itself, which removed the lockfile from the repository;
  `UninstallAll`/`UpdateAll` also received an empty string. Closed with a second test coverage.
- Instrumented write and measured; transactions and parallelism tested and discarded using numbers.
- Progress linked point-to-point.
- _INLINE_117__ aligned to the gate; both pipeline selects evaluated and left as is.
- _INLINE_118__ changed from 26 to 0, with UI recompiling.
