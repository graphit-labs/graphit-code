---
title: project_dir ignored in the lifecycle tools, instrumented writes and cleanups
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [mcp, hub, ast, escrita, performance, ui, concorrencia]
---

# project_dir ignored in the lifecycle tools, instrumented writes and cleanups

## Objective

Five fronts, coming out of a performance analysis and of a real incident:

1. `graphit_remove` accepted `project_dir` and removed files of ANOTHER project. Fix it and
   audit whether any other tool does the same.
2. Attribute the index write phase — 80% of the time, on a single core — and act according to the
   measurement.
3. The write did not report progress, and for that reason it looked stuck.
4. Audit the `select`s with the shape that let the gate accept a cancelled context.
5. Bring the 26 `ui-lint` warnings to zero.

## Implementation Details

### 1. The `project_dir` that was not honoured

`hub.OnInit`, `hub.OnUpdate` and `hub.OnRemove` resolved every path with
`paths.GetPaths(ide, false)`, which walks up from the working directory of the PROCESS. That is
correct for the CLI, typed inside the project, and wrong for the MCP server: a long-lived
process parked in one project while its tools receive a `project_dir` that names
another. `graphit_remove` called with a temporary directory removed the IDE adapter, the
rules, `CLAUDE.md` and the global registration **of the repository where the server was running**.

The three functions now receive `projectDir` and use `paths.GetPathsForProject`, which already
existed. An empty `projectDir` preserves the old behaviour, which is what the CLI passes.

The first pass fixed the main path and **was not enough**: `OnRemove` still called
`svc.UninstallAll(ctx, ide, "")` and `OnUpdate` still called `svc.UpdateAll(ctx, ide, "")` — and
an empty string is exactly what makes `GetPathsForProject` fall back to the working
directory. That is how the repository's `graphit.lock.json` kept being deleted, which
only showed up because the new test deleted it again.

`graphit_remove` also used `git.NewHookManager("")`, now `git.NewHookManager(projectDir)`.

**Audit** (what the task asked for): the only points that accepted a project and resolved
by another path were those. In the CLI (`cmd/graphit/commands/lifecycle.go`) the working
directory IS the project, so `NewHookManager("")` is still correct there and was left as it is.

**Side effect of the same bug:** the `internal/hub` test suite mutated the user's
REAL `~/.graphit/global.lock.json`. Fixing the forwarding of `projectDir` resolved
both — verified by hashing the file before and after running the whole package.

### 2. The write, measured before being optimized

`RebuildFromJSON` now accounts separately for serialization (`writeJSON`) and
ingestion (`COPY`), and reports `serialize_s`, `copy_s` and `rows` in the `COPY complete` log.

Measured over this repository's real cache (842 files, 216.099 lines):

| part | time | fraction |
|---|---|---|
| COPY (database) | 6.50 s | 58% |
| serialization to disk | 1.03 s | 9% |
| building the in-memory maps | ~1.0 s | 9% |
| schema, cache load, swap, post-processing | ~2.6 s | 24% |
| **total of the rebuild** | **11.16 s** | |

Per call: 124 COPYs, and **a COPY of a single row takes 21–132 ms**. COPYs with fewer than
50 rows cost 26 ms on average; with 50 or more, 59 ms. It is a floor per call, not volume.

**Two optimizations were tested and discarded, with measurement:**

- **Wrapping the COPYs in a transaction.** `copy_s` fell from 6.50 s to 3.00 s, but the total
  rebuild came out at 10.90 s against 11.16 s — the time merely moved to the COMMIT, which was not
  being accounted for. Besides gaining nothing, it would change the semantics of a partial error: today a COPY that
  fails is counted and the swap is aborted, keeping the previous database.
- **Parallelizing the COPYs.** `LadybugBackend.execQuery` takes a mutex over a single connection,
  so concurrency on the Go side would only produce a queue on the same lock. Doing it for real would require
  multiple connections on the same database, which this project already knows to be dangerous ground.

What stayed was the instrumentation, which is what allows deciding this again with data.

### 3. Progress during the write

`RebuildFromJSONWithProgress` reports rows as they go in; `RebuildFromJSON` delegates to it
with `nil`, so the nine existing call sites remain intact. The pipeline wires the callback to
`OnProgress("writing", rows, files, errors)` and the CLI reporter now prints
`Writing graph: N row(s) from M file(s)`.

### 4. The `select`s where cancellation loses the draw

Three candidates, handled by what each one does:

- `internal/fswatch/fswatch.go:272` — **fixed**. The batch goes on to the `SyncModule`, which is
  the path of the heavy work; a `ctx.Err()` before the select costs one branch.
- `internal/ast/pipeline.go:407` and `:431` — **left as they are**. Both loops already test
  `ctx.Err()` at the top, and the consequence of losing the draw is one extra path or one extra result
  processed. Extra code there would be noise.

### 5. `ui-lint`

26 → 0. Twenty-two were `no-unused-vars` (imports, arguments, dead variables, and four
`catch (err)` that became `catch {}`). One `no-explicit-any` became `isValidElement` with the type
declared. Of the three `exhaustive-deps`, two asked for `containerRef`/`getNodeColor`, which are
stable and already appeared as a dependency of another hook in the same file — included. The
third, in `SubmitModal`, seeds the form when the modal opens: including
`gitAuthor`/`isUpdate`/`existingScope` would make the effect run again with the modal open and
overwrite what the user typed, so there goes an `eslint-disable-next-line` with the
reason written down.

## Use Cases

### UC-01: Remove Graphit from a project through the MCP tool
- **Actor**: `graphit_remove` MCP tool.
- **Preconditions**: the MCP server runs in project A; the call names project B.
- **Main Flow**:
  1. The handler resolves `projectDir` = B.
  2. `git.NewHookManager(B)` removes B's hooks.
  3. `RemoveGitignore` acts on B's `.gitignore`.
  4. `hub.OnRemove(..., B)` assembles the paths with `GetPathsForProject(ide, B)`.
  5. The IDE adapter removes under `pp.ActiveProjectDir`, which is B.
  6. Being the last IDE, `UninstallAll(ctx, ide, B)` uninstalls B's artifacts and the project is deregistered from the global lock.
- **Alternative Flows**:
  - `project_dir` absent: resolves to the working directory, like the CLI.
- **Error Scenarios**:
  - Registry unavailable: the local removal goes on; the event is not tracked.
- **Postconditions**: nothing outside B is touched — in particular the project where the server runs.
- **Affected Files**: `internal/mcpstdio/tools_lifecycle.go`, `internal/hub/lifecycle.go`.

### UC-02: Follow the graph write
- **Actor**: `graphit ast index`, the daemon's `SyncModule`.
- **Preconditions**: parse completed with entries in the cache.
- **Main Flow**:
  1. The pipeline emits `OnProgress("writing", 0, files, errors)` on entering the phase.
  2. `RebuildFromJSONWithProgress` calls the callback on each COPY, with the accumulated rows.
  3. The CLI rewrites the progress line with rows and files.
- **Error Scenarios**:
  - `onProgress` nil: the rebuild goes on the same — it is the path of the other nine callers.
- **Postconditions**: the longest phase stops being silent.
- **Affected Files**: `internal/ast/json_rebuild.go`, `internal/ast/pipeline.go`, `cmd/graphit/commands/runners.go`.

## Test Cases & Acceptance Criteria

### Feature: the lifecycle tools honour the project they receive
Ref: UC-01

#### Scenario: remove acts on the project given
```gherkin
Given a temporary project with a lockfile registering the IDE "claude"
  And a process whose working directory is another project
When OnRemove is called with the temporary directory
Then the IDE no longer appears in the lockfile of the temporary directory
```

#### Scenario: remove does not uninstall from the working directory
```gherkin
Given a temporary project whose only registered IDE is "claude"
  And an existing graphit.lock.json in the working directory
When OnRemove is called with the temporary directory
Then the graphit.lock.json of the working directory still exists
```
Note: this scenario reproduces the real damage. Without the fix in `UninstallAll`, running the test
deleted the lockfile of the repository itself.

#### Scenario: the paths stay inside the project given
```gherkin
Given a temporary project directory
When the paths are resolved for that project
Then the lockfile is inside it
  And it does not coincide with the working directory
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/hub/lifecycle.go` | Modified | `OnInit`/`OnUpdate`/`OnRemove` receive `projectDir`; `UpdateAll`/`UninstallAll` stop receiving an empty string |
| `internal/mcpstdio/tools_lifecycle.go` | Modified | Forwards the resolved `project_dir`; `NewHookManager(projectDir)` |
| `cmd/graphit/commands/lifecycle.go` | Modified | Passes `""`, preserving the CLI's behaviour |
| `internal/hub/lifecycle_projectdir_test.go` | Created | Pins the three properties above |
| `internal/ast/json_rebuild.go` | Modified | serialize/copy/rows instrumentation; variant with progress |
| `internal/ast/pipeline.go` | Modified | Wires the write progress to `OnProgress` |
| `cmd/graphit/commands/runners.go` | Modified | Shows rows written during the phase |
| `internal/fswatch/fswatch.go` | Modified | Cancellation wins over sending the batch |
| `internal/ui/src/**` (12 files) | Modified | 26 lint warnings → 0 |

## Trade-offs & Decisions

- **A new parameter instead of a new variant in the lifecycle hooks.** They are three functions with
  few callers; a `WithProject` variant for each would triple the surface.
- **A new variant instead of a parameter in the rebuild.** Here it is the inverse: nine callers, of
  which only one needs progress.
- **The instrumentation stays, the two optimizations do not.** Transaction and parallelism were measured and
  did not pay off — the first only shifted the time, the second runs into a single connection.
- **An `eslint-disable` with a reason, and not complete deps.** In `SubmitModal` the lint rule and
  the intent of the effect disagree, and obeying the lint would break the user's typing.

## Technical Debt

- [ ] The floor of ~25 ms per `COPY` call remains the largest component of the write and
  was not explained — knowing whether it is query planning, an implicit transaction or a flush would decide
  whether there is anything to gain by reducing the number of tables per rebuild.
- [ ] `IncrementalRebuild` does not report progress; only the full rebuild started reporting.
- [ ] No entity of this project fails tokenization today, so the warning that would name them
  had nothing to collect. If the crash-loop reappears in another corpus, that is what will give the names.

## System Knowledge

- **`paths.GetPaths` walks up from the process's working directory; `GetPathsForProject`
  accepts the project.** In an MCP server the first is almost always the wrong one, and the error is
  silent: the operation works, on the wrong project.
- **Passing `""` as the project is not neutral** — it falls back to the working directory. That is how
  the first half of the fix went unnoticed.
- **Tests that call the hub hooks touch the user's real environment** when the project is not
  forwarded: `~/.graphit/global.lock.json` is resolved by `brand.GlobalDir()`, with no
  path injection.
- **The graph write is dominated by per-COPY-call cost, not by volume** — a COPY of
  one row costs tens of milliseconds.

## Progress Log

### 2026-08-11
- `project_dir` bug reproduced, fixed, audited at the remaining points and covered by a test.
- Incomplete fix discovered by the test itself, which deleted the repository's lockfile;
  `UninstallAll`/`UpdateAll` were also receiving an empty string. Closed and covered by a second test.
- Write instrumented and measured; transaction and parallelism tested and discarded with numbers.
- Write progress wired end to end.
- `fswatch` aligned with the gate; the pipeline's two selects evaluated and left as they are.
- `ui-lint` from 26 to 0, with the UI recompiling.
