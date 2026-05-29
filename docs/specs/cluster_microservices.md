---
title: "Cluster Discovery Specification"
description: "Technical specification of the Cluster Discovery Module, microservice siblings, and cross-project query delegations."
content-type: reference
audience: developers
keywords:
  - cluster
  - microservices
  - siblings
  - active projects
  - cross-project query
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/ast_module.md"
  - "docs/specs/hub_collaboration.md"
---

# Cluster Discovery Specification

In modern enterprise architectures, applications are composed of multiple distributed services.
The Cluster Discovery module enables Graphit Code to map and discover sibling services in the local environment, allowing AI agents to query code across boundaries.

---

## 🗃️ Global Project Registration

The discovery mechanism uses the **Global Lock Manager**:

- **Active Projects DB**:
  The daemon monitors registered paths listed inside a global database:
  `~/.graphit/daemon/active_projects.json`.
- **Automatic Registration**:
  When you run `graphit init` or any command in a project, the CLI registers the workspace absolute path, project ULID, name, and description.
- **Deregistration**:
  On `graphit remove`, the CLI unregisters the workspace.
  Stale paths are cleaned up by the daemon if the directory no longer exists.

---

## 🔗 Sibling Discovery Commands

Developers can inspect and link sibling projects manually:

```bash
# Add a sibling project to the cluster
graphit cluster add /path/to/auth-service

# List active cluster siblings
graphit cluster list
```

---

## 📡 Cross-Project Query Delegation

When an AI agent is invoked (e.g., inside Claude Code or Cursor), it can discover sibling projects by calling the `graphit_cluster_projects` tool.
Once the agent has the absolute directory path of a sibling service, it can delegate queries to it:

### 1. Cross-Project AST Queries
The agent can query the AST of a sibling service by setting the `project_dir` parameter in the AST tool call:
```json
{
  "ServerName": "graphit-code-stdio-mcp",
  "ToolName": "graphit_ast_query",
  "Arguments": {
    "project_dir": "/path/to/auth-service",
    "query": "MATCH (fn:Function) WHERE toLower(fn.name) CONTAINS 'validate' RETURN fn.name, fn.path",
    "ai_optimized": true
  }
}
```

### 2. Sibling Wiki Exploration
Similarly, the agent can search the compiled documentation of a sibling microservice:
- It uses the `view_file` tool to read the index:
  `/path/to/auth-service/.graphit/knowledge/project/index.md`.
- It executes targeted searches to understand the API contracts, DTO structs, and routes of the other service.

This cross-project visibility prevents **integration hallucinations**—ensuring the agent writes upstream client calls that exactly match the downstream backend parameters.
