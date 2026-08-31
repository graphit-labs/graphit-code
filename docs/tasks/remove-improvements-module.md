---
title: Remove improvements module and decouple backlog from Dream
status: completed
created: 2026-08-31
updated: 2026-08-31
tags: [improvements, backlog, dream, configuration]
---

# Remove improvements module and decouple backlog from Dream

## Objective

Remove the Improvements module as a framework module, including its compiled rules, generated mandates, skills, MCP/CLI surfaces, configuration namespace, and module documentation. Preserve the backlog as task-recording infrastructure independent of Dream, remove every rule or mandate that makes Dream a prerequisite for backlog usage, and change the default network host to `127.0.0.1`.

The backlog already lives in `docs/tasks/backlog` and is implemented in `internal/backlog`; the safe boundary is therefore to retain that package and move its public surfaces to a neutral backlog/task namespace while deleting only the Improvements methodology module. The original implementation incorrectly retained Dream as an optional backlog consumer. The corrected boundary is strict: Dream improves and consolidates knowledge and never selects, executes, completes, reports, or exposes backlog items.

## Plan & Task Breakdown

- [x] **T1 — Map the complete Improvements surface** — Identify all source entities, callers, generated artifacts, configuration keys, tests, and docs. Done means the removal scope is complete and the retained backlog boundary is explicit.
- [x] **T2 — Remove Improvements rules and mandates** — Delete the module implementation and eliminate its rule/skill/mandate generation and registration. Done means no agent-facing instruction requires or exposes the Improvements module.
- [x] **T3 — Preserve and re-home backlog interfaces** — Keep `internal/backlog`, expose it independently of Improvements, and remove Dream prerequisites from rules/mandates/docs. Done means add/list/remove work regardless of Dream state and Dream is not a consumer.
- [x] **T4 — Set the default host** — Change the relevant default bind/listen host to `127.0.0.1` and update tests/docs. Done means the compiled default and generated guidance agree.
- [x] **T5 — Regenerate artifacts and verify** — Regenerate tracked rules/skills, run focused and full validation, update docs and memories, then synchronize indexes. Done means no stale Improvements references remain except historical task records and all tests pass.
- [x] **T6 — Remove runtime Dream/backlog consumption** — Removed backlog selection from `internal/dream`, assigned-item prompt content, pending/done semantics, Dream status fields, command output, and tests. No Dream session can mark a backlog item complete.
- [x] **T7 — Remove Dream/backlog presentation coupling** — Removed backlog state and controls from the Dream dashboard and Dream status API; backlog HTTP routes now have a neutral handler. Dream UI and status surfaces describe knowledge improvement only.
- [x] **T8 — Correct active documentation and verify the boundary** — Rewrote specs, guides, generated guidance, and tests to state that Dream never consumes backlog work; audited code relationships through the reindexed AST and text-only references after regeneration. Active code and docs contain no Dream/backlog coupling beyond explicit negative assertions and legacy-file compatibility.

## Implementation Details

- Removed the Improvements package, CLI command, MCP registration, generated skill, module specification, and `<imp_rule>` mandate.
- Preserved `internal/backlog` and exposed it through top-level `graphit backlog` commands and `graphit_backlog_list`, `graphit_backlog_add`, and `graphit_backlog_remove` MCP tools.
- Moved backlog configuration to `backlog.dir` with the `GRAPHIT_BACKLOG_DIR` environment override.
- Moved backlog agent guidance into the Knowledge rule and explicitly states that Dream never consumes backlog tasks.
- Removed `backlog.Pick`/`Pending`, `.done.md` result semantics, assigned backlog prompt sections, and backlog data from Dream CLI/MCP/HTTP status.
- Split backlog HTTP routes into `DaemonBacklogHandler` and reduced the Dream dashboard to knowledge-improvement status and reports.
- Added lifecycle cleanup for retired Improvements skills and mandates so existing installations migrate cleanly.
- Changed `DefaultUIHost` to `127.0.0.1` and updated tests, guides, specs, and the network decision record.

## Use Cases

### UC-01: Record and manage deferred tasks without Dream
- **Actor**: Developer or agent.
- **Preconditions**: A Graphit project directory exists; Dream may be enabled or disabled.
- **Main Flow**:
  1. The actor adds, lists, or removes an item through the retained backlog interface.
  2. The operation reads or writes the documentation-backed backlog.
- **Alternative Flows**: Dream state has no effect on backlog behavior.
- **Error Scenarios**: Invalid titles or missing items return validation errors without requiring Dream state.
- **Postconditions**: The task record is updated independently of Dream.
- **Affected Files**: `internal/backlog`, `internal/mcpstdio/tools_backlog.go`, `cmd/graphit/commands/backlog.go`, `internal/uiserver/daemon_backlog_handler.go`.

### UC-02: Improve knowledge without task execution
- **Actor**: Dream runner.
- **Preconditions**: Dream is enabled and an idle session starts; backlog items may or may not exist.
- **Main Flow**:
  1. The runner consolidates memories and builds a prompt from project knowledge, conversations, artifacts, and prior reports.
  2. The agent improves knowledge artifacts and writes a Dream report.
- **Alternative Flows**: An empty knowledge opportunity may put Dream into deep sleep; backlog contents are never consulted.
- **Error Scenarios**: Agent failure is reported without changing backlog files.
- **Postconditions**: Existing backlog items are unchanged and absent from the Dream prompt and status.
- **Affected Files**: `internal/dream`, `cmd/graphit/commands/dream.go`, `internal/mcpstdio/tools_dream.go`, `internal/uiserver/daemon_dream_handler.go`, `internal/ui/src/components/dream/DreamDashboard.tsx`.

### UC-03: Start services on the loopback default host
- **Actor**: CLI user or daemon startup.
- **Preconditions**: No explicit host override is supplied.
- **Main Flow**:
  1. Configuration resolves the host default.
  2. The service binds to `127.0.0.1`.
- **Alternative Flows**: An explicit configuration or environment override takes precedence.
- **Error Scenarios**: Invalid explicit hosts follow existing validation behavior.
- **Postconditions**: Default service exposure is loopback-only.
- **Affected Files**: To be determined by T1.

## Test Cases & Acceptance Criteria

### Feature: Dream-independent backlog
Ref: UC-01

#### Scenario: Add and list while Dream is disabled
```gherkin
Given a project where modules.dream is not enabled
When a backlog item titled "Document pending migration" is added
Then the item is persisted under the task backlog
  And listing returns the item
  And no Dream status check is required
```

#### Scenario: Generated guidance does not gate backlog on Dream
```gherkin
Given the generated rules, skills, and mandates
When their text is inspected
Then backlog recording is described as task registration
  And no instruction says Dream must be enabled to use the backlog
```

#### Scenario: Dream does not consume a registered task
```gherkin
Given a backlog item exists before a Dream session
When Dream builds its context, reports status, and completes the session
Then the backlog item is absent from the Dream prompt and status
  And the backlog item remains registered and unchanged
  And no done-result file is created
```

### Feature: Loopback host default
Ref: UC-03

#### Scenario: Default host is loopback IPv4
```gherkin
Given no explicit host configuration
When the host is resolved
Then the result is "127.0.0.1"
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/remove-improvements-module.md` | Created | Open the task with scope, rationale, plan, and acceptance criteria |
| `internal/improvements/`, generated `graphit-improvements` skills, `docs/specs/improvements_module.md` | Removed | Retire the Improvements module, its rule, mandate, and documentation surfaces |
| `internal/mcpstdio/tools_backlog.go`, `cmd/graphit/commands/backlog.go` | Added/reworked | Expose task-backlog CRUD independently of Dream and the removed module |
| `internal/knowledge/rule.go` | Updated | Own backlog guidance and state the Dream-independent invariant in generated skills and mandates |
| `internal/dream/dream.go`, `internal/dream/prompt.go` | Updated | Remove backlog selection, assignment, completion protocol, and report metadata from Dream runtime |
| `internal/backlog/backlog.go` | Updated | Keep registry CRUD only; remove pending/pick/done execution semantics |
| `internal/uiserver/daemon_backlog_handler.go`, `internal/uiserver/daemon_dream_handler.go` | Added/updated | Separate backlog routes from Dream status and reports |
| `internal/ui/src/components/dream/DreamDashboard.tsx` | Updated | Remove backlog CRUD from the Dream dashboard and focus it on knowledge reports |
| `internal/config/config.go`, `internal/config/ui_server.go` | Updated | Move configuration to `backlog.dir` and make `127.0.0.1` the default host |
| `cmd/graphit/commands/lifecycle.go`, `internal/mcpstdio/tools_lifecycle.go` | Updated | Remove retired skills and mandate blocks from existing installations |
| Active guides, specifications, READMEs, and generated IDE artifacts | Updated | Keep public behavior and agent instructions consistent with the implementation |

## Trade-offs & Decisions

- Preserve `internal/backlog`: it models versioned task records and already has no implementation dependency on Dream.
- Dream is not a backlog consumer. Its only work-selection inputs are knowledge, memories, conversations, existing artifacts, and prior Dream reports.
- The strict ownership boundary is recorded in [Dream Never Consumes the Task Backlog](../decisions/2026-08-31-dream-never-consumes-task-backlog.md).
- Historical task logs may retain the word “Improvements” as provenance; active code, generated artifacts, specs, and references must not expose the removed module.

## Technical Debt

None identified yet.

## System Knowledge

- The corrected memory records that Dream improves knowledge and is never a backlog executor.
- Tracked IDE rules and skills are generated from Go rule builders; generators must be changed before artifacts are regenerated.

## Progress Log

### 2026-08-31
- Opened the task before source edits.
- Consulted project memory and knowledge; confirmed the backlog/Dream ownership distinction and identified existing Improvements and backlog specifications.
- Next: map source and generated-artifact impact through the AST graph.
- Completed the first implementation slice: `DefaultUIHost` now resolves to `127.0.0.1`; the backlog configuration moved from `improvements.backlog_dir`/`GRAPHIT_IMPROVEMENTS_BACKLOG_DIR` to `backlog.dir`/`GRAPHIT_BACKLOG_DIR`; and the CLI backlog is now a top-level `graphit backlog` command whose text states that Dream is optional.
- Removed the Improvements source module and all active registrations, then added migration cleanup for its retired skill and mandate.
- Re-homed backlog MCP tools and agent guidance under neutral backlog/Knowledge surfaces, with tests that reject a Dream prerequisite.
- Updated active documentation and the UI copy to describe a task backlog rather than an improvement backlog.
- Regenerated tracked IDE rules and skills from the new source. The generated `<imp_rule>` block and `graphit-improvements` skill are absent; Knowledge guidance contains the Dream-independent backlog contract.
- Passed `go test -tags fts5 ./...` and CLI help smoke tests; `graphit improvements` is absent and `graphit backlog` is present.
- Audited active code, generated artifacts, and documentation for the removed module, old tool/config names, Dream gating, and the former host default. Only intentional migration-cleanup strings, historical task/changelog provenance, and explicit remote-host examples remain.
- `graphit sync` completed. It reported that the local development build lacks the optional `lancedb` tag while indexing memory wikis; fallback scanning remains available and the sync completed successfully.
- The already-running MCP stdio process still held the pre-change rule builders and briefly reintroduced retired generated artifacts during the mandatory MCP sync. The post-sync audit caught this immediately; a final sync from the updated source removed them again. A fresh MCP process will expose the new backlog tool names and rule set.
- User correction: treating Dream as an optional backlog consumer was still wrong. Dream exists to improve knowledge, not to execute registered tasks. Reopened memory, Knowledge, and AST; consolidated the outdated memory; and added T6–T8 before further code changes.
- Removed the real runtime coupling: Dream no longer imports the backlog package, picks items, embeds assigned tasks or completion protocols in prompts, or exposes backlog entries in CLI, MCP, or HTTP status.
- Removed execution semantics from the backlog package itself (`Pick`, `Pending`, done/result fields); legacy `.done.md` files are ignored rather than treated as state.
- Split backlog HTTP routes into a dedicated neutral handler, removed backlog controls from the Dream dashboard, and updated focused UI/server tests to reflect the ownership boundary.
- Corrected the active backlog, Dream, CLI, MCP, troubleshooting, and Knowledge-rule contracts. Validation and regenerated-artifact audit remain.
- The first focused test run passed Dream, backlog, MCP, and Knowledge packages, then exposed one stale CLI completion check for `Item.Done` and two misconfigured HTTP test fixtures. Corrected all three before rerunning validation.
- Focused Go tests now pass for Dream, backlog, MCP, UI server, Knowledge rules, and CLI commands. The frontend production build and ESLint also pass after removing the Dream backlog panel.
- Reindexed all 848 AST files. Hybrid AST search no longer finds `Pick`, `Pending`, `PendingBacklog`, assigned-backlog prompt content, or the deleted Dream/backlog test; direct active-source audit finds the word `backlog` in Dream code only in negative regression assertions.
- The complete `go test -tags fts5 ./...` suite passes. CLI smoke tests show Dream as autonomous knowledge improvement with no backlog content and backlog as an independent task registry. `git diff --check` is clean.
- Regenerated IDE skills and mandates from the updated source. The Improvements module/skill/mandate remains absent, and every generated Knowledge skill states that Dream never consumes backlog items.
- Completed the mandatory MCP sync, then regenerated with the updated source because the long-lived MCP process still carried pre-removal rule builders. The final audit confirms that retired Improvements skills/mandates did not survive and all generated Knowledge skills carry the strict Dream/backlog boundary. The source sync reported the known optional-LanceDB limitation while rebuilding search indexes; the earlier forced MCP AST reindex of all 848 files and direct audits remain current and successful.

## Status

Completed — Improvements is removed, Dream is knowledge-only, backlog is a neutral task registry, and the default host is `127.0.0.1`.
