---
title: Specs described the git polling status that fswatch had already replaced
status: done
created: 2026-08-08
updated: 2026-08-08
tags: [documentation, daemon, watcher, fswatch, lag]
---

# Specs described the polling of `git status` that the `fswatch` had already replaced

## Objective

When documenting the resource gate between pipelines
([[daemon-cross-pipeline-resource-gate]]) appeared that `docs/specs/daemon_module.md`
described a change detection mechanism that the code no longer uses: a poll of
`git status --porcelain` + `git rev-parse HEAD` every few seconds, with a hash of
combined state. This drawing has been replaced by operating system notifications
(`internal/fswatch`, about `fsnotify`), and the documentation never followed up.

The lag was recorded as a debit in that task and corrected here, at the request of the
user. The audit showed that it was not contained in a section: four documents and
an orphaned comment in the code stated the same thing wrong.

## Implementation Details

### What the code actually does

All four observers use `internal/fswatch`:

| Consumer | Root observed | Debounce / ceiling |
|---|---|---|
| `daemon.SyncModule` | project tree | 1s / 5s |
| `daemon.MemorySyncModule` | `~/.graphit/memory-wt/` | 1s / 10s |
| `ast.Watcher` | project tree | 500ms / 5s |
| (package defaults) | — | 400ms / 3s |

The decisive gain of the exchange is not latency, it is that a notification **names the paths**, the
which allows the indexer to skip the entire discovery (`ast.RunPipelineForPaths`) — measured
in ~350 ms of an incremental ~1.07 s in a repository of 35,000 files.

What the new documentation had to say, and did not say anywhere before:

- **`fswatch.Batch`** has three fields, and the third is what matters to those who consume:
  `Rescan` indicates that the kernel event queue has overflowed, `Changed`/`Removed` are a
  partial portrait, and only a full scan restores consistency. - ** Ignore rules apply on two levels**: an ignored directory is never watched (it is
  which keeps the inotify budget under control, since in Linux each directory
  observed costs a watch), and an event that escapes to an ignored path is
  discarded. `.git` is never observed. - **One note, two ignore files.** The daemon feeds the AST index and the wiki the
  from a single watch, so what is observed is the *union* of what the two want, and
  each consumer applies their own file to what arrives. - ** Newly created directory ** is observed *and* swept, closing the race between the `mkdir`
  and the watch goes into effect. - ** Watches limit overflow ** is reported as such: the raw error is` no space left on
  device`, which tells the person to look at disk space. - **The accepted trade-off **: the old poll spent zero descriptors and earned `.gitignore`
  for free from git. The watcher spends one watch per directory and needs to apply the rules of
  ignore it on your own.

### Documentos corrigidos

- **`docs/specs/daemon_module.md` §2** — `SyncModule` bullet said "Polls 'git
  status --porcelain -unormal` + `git rev-parse head` every 5 seconds"Rewritten for the
  recursive watch, with `classifyBatch` routing and activity reporting to the
  supervisor, which were also undocumented. - **`docs/specs/daemon_module.md` §3** — the `MemorySyncModule` bullet said "every 10
  seconds ... via `git status` + `git rev-parse HEAD` combined hash". Rewritten for the
  single watch on the basis of worktrees, with selective recompilation per branch played. - **`docs/specs/daemon_module.md` §4** — the entire section, "Git-Based Change Detection",
  described the combined hash and listed "Advantages over `fsnotify`". Replaced by
  "Filesystem Change Detection", covering batching, ignore on two levels, cases of
  edge and the accepted trade-off. - **`docs/specs/ast_module.md`** — "It uses **git-based polling** instead of filesystem
  notifications (`fsnotify`)" was the exact inversion of what the code does. - **`docs/specs/memory_module.md`** — same 10-second poll described in spec.
  memory module. - **`docs/architecture/architecture_overview.md`** — the node of the Mermaid diagram
  `SyncModule (Git Polling)`. - **`internal/daemon/memorysyncmodule.go`** — comment orphaned at the end of the file,
  documenting a `dirtyFileMtimes` function that no longer exists and stating that "the
  memory module still polls its worktrees in ~/.graphit", contradicting the logo code
  above him.

Every historical mention of the poll was kept deliberately, marked as what was
replaced and why — erasing it would leave the next person unsure why the drawing
currently spends inotify watches.

## Use Cases

### UC-01: Someone reads the spec to understand how a change gets to the index
- **Actor**: agent or person reading `docs/specs/daemon_module.md`
- **Preconditions**: none. - **Main Flow**:
  1. Reads §2 and sees that the `SyncModule` maintains a recursive watch, not a timer. 2. Go to §4 and find the batch format, including `Rescan`. 3. Correctly concludes that the reindex receives named paths and skips discovery. - **Alternative Flows**:
  - Arrives by spec from AST or memory: both point to §4 instead of repeating the
    engine. - **Error Scenarios**:
  - [SOLVED] Previously, the person concluded that there was a poll of `git status` and that
    `.gitignore` was applied by git — and drew on top of it. - **Postconditions**: the spec reading corresponds to what `internal/fswatch` does. - **Affected Files**: `docs/specs/daemon_module.md`, `docs/specs/ast_module.md`,
  `docs/specs/memory_module.md`, `docs/architecture/architecture_overview.md`

## Test Cases & Acceptance Criteria

Documentation has no automated testing; verification is a search audit, and it is
reproducible.

### Feature: Documentation no longer affirms a non-existent mechanism
Ref: UC-01

#### Scenario: No spec describes poll as current mechanism
```gherkin
Given the specs and the architecture documentation in docs/
When se busca por "git status", "git polling", "git-based", "rev-parse HEAD" e "fsnotify"
The remaining occurrence is marked as the previous drawing, which was replaced.
  And nenhuma descreve o comportamento atual
```

#### Scenario: The code doesn't contradict itself
```gherkin
Given internal/daemon/memorysyncmodule.go
When the file is read from beginning to end
Then there is no comment affirming that the module performs polling
  And there is no comment documenting a function that doesn't exist
```

#### Scenario: The compilation and tests remain green after removing the comment
```gherkin
The orphaned comment was removed.
When "go build -tags fts5 ./..." e "go vet -tags fts5 ./internal/daemon/" rodam
Then ambos terminam sem erro
And an internal/daemon suite continues passing.
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/specs/daemon_module.md` | Modified | §2, §3 and §4: the change detection mechanism |
| `docs/specs/ast_module.md` | Modified | The File Watcher section stated the opposite of the code |
| `docs/specs/memory_module.md` | Modified | The `MemorySyncModule` 10s poll |
| `docs/architecture/architecture_overview.md` | Modified | Diagram node `SyncModule (Git Polling)` |
| `internal/daemon/memorysyncmodule.go` | Modified | Orphaned comment from a removed role, factually wrong |
| `docs/tasks/daemon-cross-pipeline-resource-gate.md` | Modified | Debt marked as resolved; reference sanitation |

## Trade-offs & Decisions

- ** Separate task log instead of attaching to the previous one.** The correction was born from a debt
  registered on [[daemon-cross-pipeline-resource-gate]], but plays `ast_module.md` and
  `memory_module.md`, which have nothing to do with the gate. Separate keeps each log with a
  objective only; the debt there points here. - **Preserve the historical mention of the poll.** The alternative was to delete all reference to the
  old design. Rejected: Without it, the cost of inotify watches seems like a choice
  arbitrary, and the next person rehashes the discussion from scratch. - ** Default gap dealt with after the statement gap.** Paragraph1 described the
  project discovery not to mention parking by activity window — wrong by
  silence, not for affirming something false, and so separated into a second step. O
  user asked for the correction next and she entered this same task.

## Technical Debt

- [x] **`docs/specs/daemon_module.md` §1 does not mention parking per window of
  activity.** Discovery was described as "launches supervisors / stops supervisors
  for deleted paths", without the `parked` or `ProjectActivityWindow` states. Corrected in
  same session, at the request of the user: §1 went on to document the three states, the two
  transitions (one pushed by `ActivityReporter`, the other measured by
  `dream.LastModifiedTime`), the `daemon.activity_window` configuration, and the fact that
  `ListActiveProjects()` be misnamed - it filters only for the existence of the lockfile, not
  per activity. - [ ] **No mechanism prevents the next lag.** The specs describe mechanisms that
  the code can be changed without anything breaking. A test that verifies that spec cites the
  package actually imported would be fragile; a lint of "spec older than the module that
  describes" would be more honest. Not implemented.

## System Knowledge

- **Documentation lag spreads by copy.** The same wrong fact was in
  four files and a code comment, because each was written repeating the
  another instead of pointing to a source. AST and memory specs now **point**
  for `daemon_module.md` § *Filesystem Change Detection* instead of repeating the mechanism. - **An orphaned comment survives `go vet` and the compiler.** The comment of
  `dirtyFileMtimes` continued in the file after the function exited, with no sign of
  tool, and it was the most misleading statement of all: it was *inside* the file that the
  contradicted. - **The compiled wiki faithfully reflects the lag.** The pages
  `Watch_Strategy-_Git_Polling`, `File_Watcher_internal-ast-watcher.go` and
  `4._Git-Based_Change_Detection` existed because the sources said so. The daemon
  recompiles the wiki on its own when watching files change — there is no step from reindex to
  run, but the old pages only disappear in the next cycle.

## Progress Log

### 2026-08-08
- Inherited debit from [[daemon-cross-pipeline-resource-gate]]; correction requested by the user. - Audited the mentions in `docs/specs/`, `docs/architecture/` and `docs/guides/`: a
  lag was in four documents, not one section. - Read `internal/fswatch/fswatch.go` integer, plus `daemon/syncmodule.go`,
  `daemon/memorysyncmodule.go` and `ast/watcher.go`, to write from code and not
  than the previous spec stated. - Found and removed the orphaned comment of `dirtyFileMtimes` in
  `internal/daemon/memorysyncmodule.go`, which stated that the module was still polling. - `go build -tags fts5 ./...`, `go vet -tags fts5 ./internal/daemon/` and the suite of
  `internal/daemon` green after removal. - Final search audit: the only remaining occurrences of "git status" / "polling" /
  "fsnotify" in the specs are the deliberate historical notes. - At the request of the user, also corrected §1 (default lag): three states of the
  project, the two transitions, `daemon.activity_window`, and the notice that
  `ListActiveProjects()` does not filter by activity despite the name.
