---
title: Filter Core MCP Stdio Server from Project Artifacts Listing
status: done
created: 2026-05-30
updated: 2026-05-30
---

# Filter Core MCP Stdio Server from Project Artifacts Listing

## Objective
The project artifacts page in the Hub UI was listing the framework's own MCP stdio server
(`graphit-code-stdio-mcp`) alongside user-installed MCP artifacts. This server is the
framework's internal communication channel with AI agents and should never appear as a
user-visible project artifact. The fix filters it at the data layer (Go backend), not
by hiding it in the UI.

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/hub/ui_server.go` | Modified | Added filter in `scanMCPArtifacts` to skip the core MCP server |

## Key Decisions
- **Filter at data level, not UI level**: The user explicitly requested the server not be "just hidden in the UI". The filter was added in `scanMCPArtifacts` (Go backend) so the core MCP server is excluded from the API response entirely, never reaching the frontend.
- **Used `brand.MCPServerName("code-stdio")` for the name**: This ensures the filter stays consistent with the brand configuration and will automatically adapt if the brand name changes.

## Use Cases

### UC-01: Listing Project MCP Artifacts
- **Actor**: User browsing the Hub UI project artifacts page
- **Preconditions**: A project is initialized with `graphit init` and at least one MCP artifact is managed
- **Main Flow**:
  1. User navigates to the project artifacts page in the Hub UI
  2. Frontend calls `GET /api/project-artifacts?project_dir=<path>`
  3. `handleProjectArtifacts` in `ui_server.go` reads the lockfile and scans local artifacts
  4. `scanMCPArtifacts` reads the IDE's MCP config file
  5. It iterates over managed MCP servers, **skipping** the core framework server (`graphit-code-stdio-mcp`)
  6. Only user-installed MCP artifacts are returned in the `project_artifacts` array
  7. UI renders only user-relevant MCP artifacts
- **Alternative Flows**:
  - No MCP artifacts installed: the MCP section is empty, core server is still excluded
  - Multiple user MCP artifacts: all are listed except the core server
- **Error Scenarios**:
  - MCP config file unreadable: `scanMCPArtifacts` returns nil, no crash
  - MCP config file malformed: `scanMCPArtifacts` returns nil
- **Postconditions**: The core framework MCP server does not appear in the project artifacts list
- **Affected Files**: `internal/hub/ui_server.go`

## Test Cases & Acceptance Criteria

### Feature: Core MCP Server Filtering in Project Artifacts
Ref: UC-01

#### Scenario: Core MCP server is excluded from project artifacts listing
```gherkin
Given a project with the MCP config containing "graphit-code-stdio-mcp" and "user-custom-mcp" servers
  And both servers are listed under the managed MCP keys
When the user requests the project artifacts API
Then the response contains "user-custom-mcp" in the project_artifacts array
  And the response does NOT contain "graphit-code-stdio-mcp"
```

#### Scenario: Only core MCP server exists in managed keys
```gherkin
Given a project with the MCP config containing only "graphit-code-stdio-mcp"
  And "graphit-code-stdio-mcp" is the only managed MCP key
When the user requests the project artifacts API
Then the response contains zero MCP-type entries in project_artifacts
```

#### Scenario: No MCP config file exists
```gherkin
Given a project where the IDE adapter returns no MCP config path
When the user requests the project artifacts API
Then no MCP artifacts are listed in project_artifacts
  And no error occurs
```

## Notes
- The `scanMCPArtifacts` function only returns MCP servers that are in the `_graphitManagedMcpKeys` section of the MCP config file. The core server is always added there by the framework's `syncAllMCP` function in `internal/hub/adapters/ide/base.go` (line 301).
- This is a data-layer fix: the core server name is computed using `brand.MCPServerName("code-stdio")` which produces `graphit-code-stdio-mcp`. If the brand changes, the filter adapts automatically.
