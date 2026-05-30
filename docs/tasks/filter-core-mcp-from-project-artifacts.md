---
title: Filter Core MCP and Framework Skills from Project Artifacts Listing
status: done
created: 2026-05-30
updated: 2026-05-30
---

# Filter Core MCP and Framework Skills from Project Artifacts Listing

## Objective
The project artifacts page in the Hub UI was listing two categories of framework-internal
artifacts that should never be user-visible:

1. **Core MCP stdio server** (`graphit-code-stdio-mcp`) — the framework's internal
   communication channel with AI agents
2. **Framework-managed skills** (`graphit-ast`, `graphit-hub`, `graphit-knowledge`,
   `graphit-memory`, `graphit-improvements`) — built-in skills auto-installed by the
   framework

Both types were filtered at the **data layer** (Go backend), not hidden in the UI.

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/hub/ui_server.go` | Modified | Added filter in `scanMCPArtifacts` to skip core MCP server; added `brand.ManagedSkillPrefix()` filter in `handleProjectArtifacts` to skip framework skills from imported artifacts |

## Key Decisions
- **Prefix-based filter, NOT origin-based**: Initially considered filtering by `origin == "managed"`, but `origin: "managed"` means "artifact published by this project and auto-reconciled by the reconcile system". Those are legitimate user artifacts and MUST remain visible. Framework skills are identified by their name prefix (`brand.ManagedSkillPrefix()` = `"graphit-"`), consistent with `ScanLocal` in `base.go` line 266.
- **Two filtering points for skills**: `ScanLocal` already filters managed skills from `project_artifacts` (local disk scan). The new filter addresses `imported_artifacts` (lockfile scan) where managed skills can also appear.
- **Core MCP filter by exact name**: Uses `brand.MCPServerName("code-stdio")` for the MCP server, adapting automatically if the brand changes.

## Notes
- `origin: "managed"` in lockfile = artifact published by this project, auto-tracked by reconciliation (`internal/hub/reconcile.go`). NOT the same as "framework-internal skill".
- `brand.ManagedSkillPrefix()` = `"graphit-"` identifies framework-internal skills by naming convention. This is the canonical way to identify them, used consistently across `ScanLocal` and now `handleProjectArtifacts`.
