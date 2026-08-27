---
title: "Cluster Discovery Specification"
description: "Technical specification of the Cluster Discovery Module, project grouping via labels, and cross-project query delegations."
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
The Cluster Discovery module enables Graphit Code to map and discover related projects in the local environment, allowing AI agents to query code across boundaries.

---

## 🗃️ Global Project Registration

The discovery mechanism uses the **Global Lock Manager**:

- **Global Lock File**:
  All registered projects are stored in `~/.graphit/global.lock.json`.
- **Automatic Registration**:
  When you run `graphit init` or any command in a project, the CLI registers the workspace absolute path, project ULID, name, and description.
- **Deregistration**:
  On `graphit remove`, the CLI unregisters the workspace.
  Stale paths are cleaned up by the daemon if the directory no longer exists.

---

## 🏷️ Cluster Labels

Projects are grouped into clusters via key-value labels. Projects sharing at least one
identical label value for the same key are considered part of the same cluster.
Projects without any labels form their own default group.

```bash
# Set a cluster label
graphit cluster domain backend

# Get a specific label
graphit cluster --get domain

# List all labels
graphit cluster --list

# Remove a label
graphit cluster --unset domain
```

---

## 🔗 Cluster Project Discovery

List all projects in the same cluster (including the current project):

```bash
graphit cluster projects
```

Optionally filter by a specific label key:

```bash
graphit cluster projects domain
```

Via MCP:

```json
{
  "tool": "graphit_cluster_projects",
  "arguments": { "project_dir": "/path/to/project" }
}
```

With label filter:

```json
{
  "tool": "graphit_cluster_projects",
  "arguments": { "project_dir": "/path/to/project", "label": "domain" }
}
```

The result includes all projects sharing at least one cluster label with the
current project. Each entry contains:

| Field          | Description                                  |
|----------------|----------------------------------------------|
| `dir`          | Absolute path to the project root directory  |
| `name`         | Human-readable project name                  |
| `description`  | Project description                          |
| `cluster`      | Cluster labels (key→value map)               |
| `registeredAt` | When the project was registered              |

---

## 📡 Cross-Project Query Delegation

When an AI agent is invoked (e.g., inside Claude Code or Cursor), it can discover cluster projects by calling the `graphit_cluster_projects` tool.
Once the agent has the absolute directory path of a project, it can delegate queries to it:

### 1. Cross-Project AST Queries
The agent can query the AST of another project by setting the `project_dir` parameter in the AST tool call:
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

### 2. Project Wiki Exploration
Similarly, the agent can search the compiled documentation of another project:
- It calls `graphit_knowledge_search` and `graphit_wiki_source` with the sibling's
  directory as `project_dir`. There is no file to open: the sibling's wiki lives once
  in the global brand directory, keyed by its project id, which is precisely what lets
  these tools serve it without the agent leaving its own workspace.
- It executes targeted searches to understand the API contracts, DTO structs, and routes of the other service.

This cross-project visibility prevents **integration hallucinations**—ensuring the agent writes upstream client calls that exactly match the downstream backend parameters.
