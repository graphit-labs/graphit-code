---
title: Enforce agent lifecycle hooks for resume, unit completion, and final sync
status: done
created: 2026-09-02
updated: 2026-09-03
tags: [adapters, hooks, agents, tasks, sync]
---

# Enforce Agent Lifecycle Hooks

## Objective

Strengthen every supported agent adapter at the lifecycle boundaries its host actually exposes so that:

1. resumed work re-injects the Graphit-first routing and reminds the agent to prefer framework MCP tools over native discovery tools;
2. completing the smallest independently reportable unit of work prompts an immediate task-management update;
3. finishing the overall work always dispatches `graphit sync` asynchronously without delaying completion.

The implementation must preserve each host's native hook/event format, remain fail-open when Graphit MCP is unavailable, and avoid claiming an enforceable boundary where a host does not provide one.

## Reasoning

The current mandate covers these responsibilities semantically, but a session can resume at a host boundary without the model reopening the right skill, and task-log/final-sync instructions can be forgotten during long or interrupted work. Objective lifecycle events should therefore carry the reminder at the adapter layer; semantic classification remains in the agent instructions where the host cannot prove that a unit is complete.

## Plan & Task Breakdown

- [x] **T1 — Map adapter lifecycle coverage.** Identify each adapter's generated hook events, payload composition, and tests; document which native boundaries correspond to resume, unit completion, and final completion.
- [x] **T2 — Define lifecycle protocol content.** Add compact, shared content for Graphit-first resume, smallest-unit task-management updates, and mandatory asynchronous final-sync dispatch without adding IDE-name switches to shared code.
- [x] **T3 — Wire concrete adapters.** Emit the protocol through every host's strongest native events while preserving user-owned hook/config entries and fail-open behavior. Final sync is fire-and-forget on every host, never awaited.
- [x] **T4 — Test behavior and generated artifacts.** Add or update unit/integration assertions for every supported adapter and regenerate versioned artifacts through the supported build/install flow when required.
- [x] **T5 — Verify and close.** Run targeted and broader tests, update documentation/task state after each completed unit, then perform the explicit contributor-workflow `graphit_sync` required before completion.
- [x] **T6 — Close the lifecycle audit gaps.** Add compact per-prompt routing where the host can inject it, add final session fallbacks, and preserve full bootstrap only at true agent/session start boundaries.
- [x] **T7 — Reverify native limitations.** Keep unsupported/no-output events documented instead of installing no-op hooks, update coverage tests and generated artifacts, then close with asynchronous sync.

## Acceptance Criteria

- Each supported adapter has an explicit, tested mapping for resume, smallest-unit task update, and final sync.
- Resume-boundary output explicitly restores Graphit MCP preference over native tools.
- The task-management reminder names the smallest independently reportable completed unit and requires the task log/manager to be updated immediately.
- Final-completion hooks always dispatch a full sync asynchronously and never delay the agent's completion.
- Production behavior and generated hooks work on Linux, Windows, and macOS without requiring a POSIX shell.
- Unsupported lifecycle boundaries are documented rather than emulated with a false guarantee.
- Existing user-owned hook and MCP configuration remains preserved.
- Relevant tests pass and the final task log reflects the actual state.

## Affected Files

- `internal/sessionhook/sessionhook.go` and tests: common resume/unit/final lifecycle payloads.
- `cmd/graphit/commands/session_hook.go` and tests: portable asynchronous final-sync dispatch owned by the hook process.
- `internal/hub/adapters/ide/{claude,codex,cursor,gemini,kiro,antigravity,opencode}.go`: native event wiring per host.
- `internal/hub/adapters/ide/session_hooks.go` and tests: reusable managed-command reconciliation and lifecycle coverage assertions.
- `internal/hub/adapters/ide/mandate_compact.go` and tests: unconditional finalization ordering in resident instructions.
- `docs/architecture/adapter-hook-enforcement.md` and `docs/guides/agent_hook_activation.md`: lifecycle matrix and verification instructions.
- Generated project-local hook artifacts after install/sync.

## Trade-offs & Constraints

- Host-specific lifecycle behavior stays in concrete adapters; shared code may only compose common protocol text.
- Hooks enforce only objective boundaries. Determining that a semantic sub-step is the smallest complete unit may still require model judgment, but every available post-action/stop boundary should reassert the requirement.
- Graphit MCP remains preferred when available; native tools remain the fail-open fallback.
- Every implementation in the framework must support Linux, Windows, and macOS; this is now mandatory project memory.
- No backward-compatibility layer is required for the in-development generated hook format.

## Technical Debt

None identified yet.

## System Knowledge

The existing architecture separates instruction delivery, MCP tool visibility, and routing. A hook can inject routing but cannot manufacture tool visibility. Generated hook files are owned by concrete adapters and must be regenerated through the installed runtime rather than edited by hand.

## Progress Log

### 2026-09-02

- Consulted mandatory project memory, focused memory, and project wiki before implementation.
- Confirmed the prior task added semantic resume/task-log/sync guidance, while the present request requires lifecycle-level reinforcement for every agent.
- Opened this task log before code discovery or edits.
- Completed T1 after AST-first discovery and validation against current official host documentation. The strongest available boundaries are: Codex/Claude (`SessionStart`, `PostToolUse`, `SubagentStop`, `Stop`); Cursor (`sessionStart`, `postToolUse`, `subagentStop`, `stop`); Gemini (`SessionStart`, `BeforeAgent`, `AfterTool`, `AfterAgent`); Kiro (`SessionStart`/`AgentSpawn`, `PostToolUse`, `PostTaskExec`, `Stop`); Antigravity (`PreInvocation`, `PostInvocation`, `Stop`); OpenCode (system transform/compaction, `tool.execute.after`, `todo.updated`, `session.idle`).
- Design consequence: resume routing and smallest-unit reminders can be injected at native model/tool/task boundaries, while final sync must be a deterministic synchronous hook action rather than a best-effort instruction to the model.
- Completed T2 in `internal/sessionhook/sessionhook.go`: the resident invariant now explicitly reapplies Graphit-first routing on resumed/interrupted work; a compact unit-completion checkpoint tells each agent to update the active task manager/task log immediately; native payload formats cover Claude/Codex, Cursor, Gemini, Kiro, Antigravity, and silent finalization; controllable stop formats turn sync failures into a continuation instead of silently accepting completion.
- Began T3 with the deterministic finalizer in `cmd/graphit/commands/session_hook.go`. Managed completion hooks can now pass `--sync`; the hidden hook resolves the project from native input, runs `graphit sync --no-background`, captures all child output so host JSON remains valid, waits and retries when another sync owns the lock, and converts failures into the host-specific continuation payload where the host supports one.
- Extended the adapter reconciliation helpers for managed final-sync commands. Final hooks are distinguishable and removable through their format, preserve existing user hook groups, and receive a 600-second timeout without inflating ordinary context-hook timeouts.
- Wired the standard JSON-hook adapters. Claude and Codex now checkpoint after every successful tool and perform awaited sync at both subagent and main-agent stop; Cursor does the equivalent with its native lowercase events and payloads; Gemini checkpoints in `AfterTool` and synchronizes in `AfterAgent`. Existing start/resume and subagent-start routing remains intact.
- Wired Kiro through its current v1 lifecycle: `UserPromptSubmit` reasserts Graphit-first on every resumed turn, `PostToolUse` emits the smallest-unit checkpoint, `PostTaskExec` explicitly prompts the agent to update task management at the native spec-task boundary, and `Stop` runs the awaited final sync with no timeout. IDE `SessionStart` and CLI `AgentSpawn` bootstraps remain separate.
- Wired Antigravity inside its adapter-owned lifecycle object: every `PreInvocation` still receives dynamic Graphit routing, `PostInvocation` now injects the task checkpoint after a model/tool step, and `Stop` runs the synchronous finalizer; a failed sync returns native `decision: continue` feedback rather than accepting completion.
- Wired OpenCode in its generated project plugin: system transforms continue to cover new/resumed/child sessions and compaction, `tool.execute.after` appends the task checkpoint to successful tool results, and every `session.idle` synchronously invokes the silent final-sync hook. A sync failure throws from the event handler so it is visible instead of being reported as success.
- Completed T3 by strengthening the resident mandate as the universal fallback: every smallest completed unit updates task management immediately, and every agent/subagent completion orders final task state before an awaited `graphit_sync`. Native stop hooks enforce the sync where the host exposes the boundary; adapter-specific wiring remains in concrete adapters.
- Formatted all modified Go sources before beginning T4 verification.
- First targeted test run exposed a package-boundary mistake: `getGraphitExecutable` belongs to the IDE adapter package and is not available to the hidden command. The finalizer will use `os.Executable()` instead, which is more accurate here because `_session-hook` must spawn the exact installed runtime currently handling the event.
- Corrected and formatted the finalizer to spawn the exact running Graphit executable returned by `os.Executable()`.
- Split the final-sync runner into a production resolver plus an executable-specific helper so tests can verify the real synchronous subprocess contract without trying to recursively invoke the Go test binary as the CLI.
- Added renderer contract tests for resumed-work routing, smallest-reportable-unit wording, every native checkpoint payload, successful silent completion, and host-specific continuation behavior on final-sync failure.
- Added hidden-command tests proving that an unresolved project blocks completion on controllable hosts and that the finalizer invokes `sync --no-background` synchronously from the resolved project directory.
- Expanded adapter integration assertions so all seven generated configurations must contain their native resume/checkpoint/final-sync boundaries, including dual parent/subagent finalizers where supported; strengthened the mandate test to require task management before final sync.
- Second targeted test run caught a local variable scoping error introduced while extracting the executable-specific sync helper; adapter and renderer suites remained green. The helper needs its own command-run error binding.
- Corrected the extracted helper's local command error binding.
- Targeted tests then exposed a test-fixture mistake, not a product hang: a nonexistent `cwd` falls through to the process cwd by design, so the failure test resolved this real project and launched a real full sync. The run was interrupted; the test must pass the nonexistent path through the explicit diagnostic flag, which intentionally does not fall back.
- Corrected the failure fixture to use explicit project discovery, preventing process-cwd fallback and any real sync during the unit test.
- Cleanup: two earlier test processes were still running the pre-fix binary; because a Go test binary accepts positional arguments, each attempted `sync --no-background` reran the test suite and recursively spawned children. Terminated only that temporary `/tmp/go-build.../commands.test` tree. The current fixture uses the explicit unresolved-project path, while subprocess behavior is tested only with a standalone fake executable.
- User correction changed finalization semantics: final sync must be asynchronous and completion must not wait. Reopened T3 and T4; the synchronous subprocess/lock retry and completion-blocking failure payloads will be removed rather than retained as compatibility paths.
- Recorded mandatory project memory `01M1JJCRFD74YHTP80S5WN5MS8`: every Graphit implementation, including hooks, subprocesses, paths, quoting, generated scripts, and tests, must work on Linux, Windows, and macOS.
- Reworked the finalizer around a portable `os/exec` command builder: it starts `graphit sync` in the resolved project, releases the child process, and returns immediately without a shell, POSIX quoting, or a wait path.
- Removed long final-hook timeouts and synchronous wording from the shared helpers, Kiro, and Antigravity. OpenCode now uses `Bun.spawn(...).unref()` for the dispatcher, and the resident mandate explicitly orders the final task update before a fire-and-forget full sync.
- Replaced the old blocking/failure-continuation tests with asynchronous completion contracts: stop payloads now accept completion immediately, the hidden command is tested without launching a child, command arguments contain only `sync`, and generated configurations reject 600-second wait timeouts. The subprocess test is now OS-neutral and has no shell script or Windows skip.
- First asynchronous targeted run compiled all changed packages. It found two assertion-only mismatches: capitalization in the new mandate sentence and Cobra's expected usage text on command failure; production behavior itself compiled and the renderer suite passed.
- Corrected the two test fixtures: mandate matching now respects the emitted capitalization, and the unresolved-project command test silences Cobra usage so it can assert that no host completion payload was rendered.
- The rerun passed the renderer and command suites; the adapter suite exposed one remaining case-sensitive assertion for the sentence-final `Do not sync after every edit`, which has now been aligned with the emitted text.
- Targeted lifecycle verification is green across `internal/sessionhook`, all IDE adapters, and `cmd/graphit/commands` with the `fts5` build tag.
- Documented the complete lifecycle matrix for all seven adapters, including resume/reinjection, smallest-unit checkpoints, parent/subagent completion boundaries, fire-and-forget sync semantics, and the portable no-shell process contract. Activation guidance now asks users to enable/trust every generated lifecycle entry and verifies prompt return without waiting for indexing.
- T3 is complete under the corrected contract: each concrete adapter owns its native events, every available final boundary dispatches without waiting, and no shared helper switches on IDE names.
- Broader verification is green across all `internal/hub/...` packages plus `internal/sessionhook` and `cmd/graphit/commands` with `fts5`; generated artifacts remain to be refreshed with the installed binary.
- Confirmed through the authoritative project memory list that `01M1JJCRFD74YHTP80S5WN5MS8` is present with both `mandatory=true` and `important=true`.
- Cross-target compilation passed for the changed renderer and adapter packages on Windows/amd64, but the commands package could not be cross-compiled with `CGO_ENABLED=0`: pre-existing tree-sitter grammar packages exclude all files in that mode. This is an existing repository build constraint, not a failure in the new hook subprocess code; the project build targets must be used for full platform binaries.
- Re-ran the portable subset explicitly for both Windows/amd64 and macOS/arm64: `internal/sessionhook` and `internal/hub/adapters/ide` compile cleanly on both targets with `CGO_ENABLED=0`.
- Hardened executable-path serialization for generated hook command strings: Linux/macOS use safe single-quote escaping, while Windows uses CommandLineToArgvW-compatible backslash/quote escaping. Added OS-independent table tests for spaces, apostrophes, and trailing Windows backslashes; the adapter suite remains green.
- Refined the architecture wording to match the host contracts precisely: OpenCode receives an argument array, command-string hosts receive OS-specific executable escaping, and the Graphit dispatcher itself never relies on an auxiliary shell script.
- Reconfirmed clean Windows/amd64 and macOS/arm64 compilation of the renderer and adapter packages after introducing platform-aware command quoting.
- Full Linux verification passed: `go test -tags fts5 ./... -count=1` is green across the repository.
- Built and installed the updated Linux runtime to `/usr/local/bin/graphit` through `make install`; the UI/build pipeline completed successfully. npm reported existing dependency-audit findings (32 total), which are outside this hook change.
- The first required MCP `graphit_sync` completed, but its already-resident server still contained the pre-build adapter code and therefore left the generated artifacts with the obsolete 600-second/synchronous forms. The freshly installed runtime must perform the project-generation step before artifact verification; a final MCP sync will still close the contributor workflow.
- Regenerated project artifacts with the freshly installed runtime. Claude, Codex, Kiro, Antigravity, and OpenCode now contain the corrected fire-and-forget hooks, OS-specific executable quoting, no 600-second final timeouts, and `Bun.spawn(...).unref()` in OpenCode.
- Executed the installed `_session-hook --format no-output --sync` against this project as an end-to-end smoke test: it emitted no payload and returned successfully in effectively zero wall time while dispatching the full sync child.
- T4 is complete: unit/integration contracts cover all seven adapters, generated artifacts were refreshed through the installed runtime, and the real finalizer smoke test confirmed non-blocking behavior.
- Final targeted rerun passed for `internal/hub/...`, `internal/sessionhook`, and `cmd/graphit/commands`; `git diff --check` is clean and generated artifacts contain none of the obsolete 600-second timeout, awaited-sync description, or synchronous OpenCode finalizer.
- T5 and the task are complete. The required MCP sync was executed after the code changes; the final task-log write is followed by the installed hook's asynchronous sync dispatcher so completion itself does not wait.

### 2026-09-03

- Reopened the task after the lifecycle audit correction. Repeated boundaries must inject only `CoreInvariant` or `UnitCompletionReminder`; full bootstrap, mandatory memory, mandates, and Hub rules remain restricted to true session/subagent start.
- Confirmed from the current Antigravity contract that `PostToolUse` exists but accepts only `{}` and cannot inject agent context. `PostInvocation` remains the meaningful delivery boundary; a no-op `PostToolUse` will not be installed merely for nominal coverage.
- Planned meaningful additions: compact `UserPromptSubmit` for Claude/Codex, `SessionEnd` sync fallbacks for Claude/Codex/Cursor/Gemini, `session.deleted` sync for OpenCode, and removal of full dynamic context from repeated Gemini/Antigravity model boundaries.
- Added native `user-prompt` and `session-end` output formats. `UserPromptSubmit`, repeated Gemini `BeforeAgent`, and Antigravity invocations after invocation zero now emit only `CoreInvariant`; startup/subagent formats remain the only paths that carry the full bootstrap. The hidden command also skips project mandate/rule construction entirely for compact repeated formats.
- Wired compact `UserPromptSubmit` into Claude and Codex. Added asynchronous `SessionEnd` sync fallbacks to Claude, Codex, Cursor local, and Gemini, plus an OpenCode `session.deleted` fallback. Sync/remove paths remain symmetric and preserve user-owned entries.
- Expanded renderer and adapter coverage for the new boundaries. Tests now reject mandatory memory, dynamic mandates/rules, and startup bootstrap text at recurring prompt/model hooks; Antigravity coverage also asserts that a context-incapable `PostToolUse` entry is not generated.
- Formatted the changed sources and completed the first verification unit: `internal/sessionhook`, all IDE adapters, and `cmd/graphit/commands` pass their targeted `fts5` test suites with the compact-context and lifecycle-fallback assertions enabled. Documentation and full-platform verification are next.
- Updated the architecture matrix and activation guide with the new prompt/session-end boundaries, the strict compact-context budget, OpenCode's deletion fallback, and the native limitations that rule out Antigravity `PostToolUse` and Cursor `beforeSubmitPrompt` as context-injection hooks.
- Full Linux verification passed: `go test -tags fts5 ./... -count=1` is green across the repository. Cross-target adapter/renderer compilation and generated-artifact refresh are next.
- The portable hook subset (`internal/sessionhook` and `internal/hub/adapters/ide`) cross-compiles cleanly for Windows/amd64 and macOS/arm64 with `CGO_ENABLED=0`, preserving the mandatory Linux/Windows/macOS contract.
- Built the full Linux distribution and installed the updated runtime at `/usr/local/bin/graphit` through `make install`. The build succeeded; npm again reported 32 pre-existing audit findings outside this hook change. Generated project artifacts will now be refreshed with this installed runtime.
- Ran the required Graphit MCP sync, then regenerated project-local hooks with the freshly installed runtime so the artifacts reflect the new adapter code rather than the already-resident MCP process. Artifact inspection and final smoke verification are next.
- Verified all generated JSON hook files, confirmed Antigravity contains no ineffective `PostToolUse`, and exercised the installed compact renderers. `UserPromptSubmit` is 499 bytes and a resumed Antigravity invocation is 458 bytes; neither contains bootstrap, mandatory-memory search, nor system-mandate text.
- Removed the temporary Cursor/Gemini activation created solely for artifact inspection and restored the repository's original adapter selection. Their implementations remain covered by adapter integration tests; project-local generated files now stay limited to the IDEs selected by the user.
- Re-synchronized the restored five-adapter project configuration and reran the targeted lifecycle suites; sync, `git diff --check`, renderer tests, adapter tests, and hidden-command tests are all green. Temporary Cursor/Gemini generated files and lockfile churn were removed.
- T6 and T7 are complete. Full Linux tests, Windows/macOS cross-compilation, generated-hook validation, compact-payload smoke checks, documentation, and the required Graphit MCP sync all passed. The final task-manager update is followed by the installed stop hook's fire-and-forget sync dispatcher.
- Before committing on `main`, reran the focused renderer, adapter, and hidden-command suites; all passed and `git diff --check` remained clean.
