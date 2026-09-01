---
title: Investigate Hub framework artifact
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [hub, artifacts, framework]
---

# Investigate Hub framework artifact

## Objective

Determine whether `framework` is still a supported Graphit Hub artifact type, explain its behavior, and remove any obsolete presentation controls after confirming that backend support is gone. The cleanup must leave the canonical supported artifact types unchanged and add regression coverage for the visible UI list.

## Plan & Task Breakdown

- [x] **T1 — Establish the documented status** — Read the relevant wiki pages and the live Hub catalogue; confirmed that the `framework` type was removed with framework detection and entry-point scoring.
- [x] **T2 — Verify runtime behavior** — Inspected the indexed implementation and tests; confirmed that no `TypeFramework` exists and unsupported registry entries fail installation.
- [x] **T3 — Report the result** — Reconciled documentation, registry, implementation, and remaining UI references.
- [x] **T4 — Remove stale UI controls** — Removed the `framework` filter, icon mapping, style mapping, and now-unused `Blocks` imports from the two components.
- [x] **T5 — Add regression coverage** — Added `Sidebar.test.tsx` covering filters, icons, and styles; the focused Vitest test passes.
- [x] **T6 — Verify and close** — The production UI build passes, memory was updated, the completed backlog item was removed, and all indexes were synchronized.
- [x] **T7 — Commit the completed cleanup on `main`** — Verified the branch and worktree, staged only the four files belonging to this task, and created a focused commit without including unrelated user changes.

## Implementation Details

Current backend artifact types are `agent`, `rule`, `workflow`, `skill`, `knowledge`, `ast`, `mcp`, `command`, `power`, and `language`. `framework` is absent from the canonical constants, folder map, and valid-type list. `HubService.Install` rejects a registry entry whose type is absent from the folder map with `unknown artifact type`.

Historically, a framework artifact installed an AST framework YAML consumed by framework/ecosystem detection and framework-specific entry-point scoring. Those enrichment paths had no runtime consumer beyond an agent-facing sample query and were removed on 2026-08-09.

The user subsequently authorized the cleanup of the remaining UI references. The implementation removed the obsolete filter plus both ArtifactCard presentation mappings (`TYPE_ICONS` and `TYPE_STYLES`) without changing backend artifact definitions. The three metadata constants are exported for a focused regression test; they remain internal to the UI package.

## Use Cases

### UC-01: Check a legacy Hub artifact type
- **Actor**: Graphit user
- **Preconditions**: The project and Hub registry are available.
- **Main Flow**:
  1. Query current project knowledge for the `framework` artifact type.
  2. Inspect the live Hub catalogue and implementation evidence.
  3. Explain whether the type is supported and what it does or used to do.
- **Alternative Flows**:
  - If documentation and implementation disagree, report both and identify the effective runtime behavior.
- **Error Scenarios**:
  - If an index is unavailable, retry and check daemon status before falling back.
- **Postconditions**: The user has an evidence-backed description of the type's current status.
- **Affected Files**: `docs/tasks/investigate-hub-framework-artifact.md`

### UC-02: Browse supported Hub artifact filters
- **Actor**: Graphit Hub UI user
- **Preconditions**: The Hub UI is loaded.
- **Main Flow**:
  1. Open the Hub sidebar.
  2. Review the artifact-type filters rendered from `TYPE_FILTERS`.
  3. Select only types supported by the backend.
- **Alternative Flows**:
  - Select `Any Type` to clear the type constraint.
- **Error Scenarios**:
  - An unsupported historical type must not be presented as a selectable filter.
- **Postconditions**: The sidebar contains no `Framework` filter and artifact cards do not carry unused `framework` styling.
- **Affected Files**: `internal/ui/src/components/layout/Sidebar.tsx`, `internal/ui/src/components/hub/ArtifactCard.tsx`, frontend test files identified by the AST impact check.

## Test Cases & Acceptance Criteria

### Feature: Hub framework artifact status
Ref: UC-01

#### Scenario: Current status is verified

```gherkin
Given the current Graphit knowledge index and Hub registry are available
When the framework artifact type is investigated
Then the answer identifies whether it is accepted by the current Hub
And explains its behavior or replacement
And distinguishes live behavior from historical documentation
```

#### Scenario: Obsolete Framework filter is absent

```gherkin
Given the Hub backend no longer supports a framework artifact type
When the Hub sidebar renders its artifact-type filters
Then no filter labeled "Framework" is present
And no filter has the value "framework"
```

#### Scenario: Supported filters remain available

```gherkin
Given the Hub sidebar is rendered
When the user reviews the artifact-type filters
Then the existing supported filters remain selectable
And "Any Type" remains available to clear filtering
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/investigate-hub-framework-artifact.md` | Created | Preserve the investigation plan and evidence trail. |
| `internal/ui/src/components/layout/Sidebar.tsx` | Modified | Removed the stale `Framework` filter and unused `Blocks` import; exported `TYPE_FILTERS` for regression coverage. |
| `internal/ui/src/components/hub/ArtifactCard.tsx` | Modified | Removed obsolete `framework` icon/style mappings and unused `Blocks` import; exported the maps for regression coverage. |
| `internal/ui/src/components/layout/Sidebar.test.tsx` | Created | Assert filters, icons, and styles do not expose `framework`. |

## Trade-offs & Decisions

- Prefer live MCP registry and indexed implementation evidence over assumptions based on older terminology.
- Treat the remaining `Framework` sidebar filter and `ArtifactCard` color mapping as stale presentation code, not compatibility support.

## Technical Debt

- None introduced. The stale Hub `framework` presentation controls were removed and their completed backlog item was cleared.

## System Knowledge

- The current Hub search returned no artifact registered under the term `framework`; this alone does not establish whether the type is accepted by the implementation.
- The canonical backend definitions contain ten artifact types and no `TypeFramework`.
- `HubService.Install` checks `TypeFolderMap` and returns `unknown artifact type` for an unrecognized registry type.
- The Hub UI no longer advertises a `Framework` filter or carries icon/style mappings for `framework`.

## Progress Log

### 2026-08-31

- Searched project memory, project knowledge, and the live Hub for `framework`.
- Found relevant documentation about filtering framework skills and removing framework detection; no directly relevant project memory was found.
- Read the removal task, which records that the Hub `framework` type was deleted because installed framework YAML files no longer had a consumer.
- Verified the canonical constants, valid-type list, installation guard, CLI artifact list, and remaining UI references through the AST index.
- Recorded the stale UI cleanup in the documentation-backed backlog.
- Investigation complete; no source changes were made.
- User requested implementation of the recorded cleanup. Reopened the task, reread the memory/wiki evidence, and added T4–T6 before editing source.
- AST impact analysis found no test reaching `Sidebar` or `ArtifactCard`, and no dependency beyond local containment for the metadata constants.
- Removed all three obsolete presentation references and added a focused Vitest regression; next step is frontend verification.
- Focused Vitest verification passed: 1 test.
- Production UI build passed.
- Updated the existing project memory and removed the completed backlog item.
- User requested that the completed cleanup be committed directly on `main`; reopened the task for a scoped Git review and commit.
- Confirmed `main`, left the unrelated `docs/tasks/redesign-github-site.md` untracked, and committed only this task's four files.
