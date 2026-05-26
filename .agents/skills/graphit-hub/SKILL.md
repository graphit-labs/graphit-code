---
name: graphit-hub
description: Centralized registry of knowledge, AST, rules, skills, commands, agents, MCPs, and powers. Use when working with external libraries, APIs, or frameworks to find pre-built knowledge artifacts. Check the hub BEFORE implementing integrations with unfamiliar systems. Also use to install/update artifacts and discover reusable components.
---

# Hub Discovery Rule

## Objective

The Hub is a centralized registry of shareable artifacts that enrich your development
environment with pre-built knowledge bases, code analysis contexts, reusable rules,
skills, commands, agents, MCP integrations, and power bundles.

You MUST leverage the Hub BEFORE assuming or hallucinating how any framework,
library, or domain concept works.

## Artifact Types

The Hub provides these artifact types — each serves a different purpose:

| Type | What it provides | After installation |
|---|---|---|
| `knowledge` | Pre-indexed documentation wiki for a framework/library | Read the wiki at `.graphit/knowledge/<id>/index.md` |
| `ast` | Pre-indexed code graph of a framework's source code | Query with `graphit ast query "..." --context <id>` |
| `rule` | Coding conventions, style guides, governance rules | Auto-injected into IDE rules file |
| `skill` | Detailed methodology for specific tasks (e.g. testing, migration) | Available as an on-demand skill |
| `command` | Reusable CLI workflows/commands | Available in IDE's commands directory |
| `agent` | Pre-configured agent personas with specific expertise | Available in IDE's agents directory |
| `mcp` | MCP server configurations for external tool integrations | Auto-configured in IDE's MCP settings |
| `power` | Curated bundle combining multiple artifacts as a cohesive package | Installs all bundled artifacts at once |

## How to use the Hub

If you encounter a framework, module, or domain concept you are not fully certain
about, DO NOT guess its API or structure. Check if it is available in the Hub.

### 1. Discovery
To see all available artifacts or filter by type:
```bash
graphit hub list
graphit hub list --type <knowledge|ast|rule|skill|command|agent|mcp|power>
```

### 2. Inspection
To see the details, tags, and description of a specific artifact:
```bash
graphit hub show <artifact-id>
```

### 3. Installation
To download and install the artifact into the current project:
```bash
graphit hub install <artifact-id> --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>
```

### 4. Updates
To keep all installed artifacts up to date with the latest versions:
```bash
graphit hub update
```

## Using Installed Artifacts

Once installed, artifacts enhance your capabilities automatically:

- **Knowledge**: Read the wiki `.graphit/knowledge/<id>/index.md` to understand
  a framework's API, architecture, and patterns — never guess.
- **AST**: Query the code graph to find functions, classes, and relationships
  in the framework's source: `graphit ast query "..." --context <id>`
- **Rules**: Automatically injected — follow the conventions they define.
- **Skills**: Read the skill when the task matches its domain. Skills appear
  in the IDE's skills directory.
- **Commands**: Execute pre-built workflows from the IDE's commands directory.
- **Agents**: Delegate specialized tasks to agent personas with domain expertise.
- **MCPs**: External tool integrations are auto-configured — use them as available tools.
- **Powers**: All bundled artifacts are installed — use each by its individual type.

## Installed Artifacts

> No hub artifacts are currently installed in this project.

Run `graphit hub install <artifact-id> --ide <ide>` to install one.

## 🌐 Ecosystem Project Discovery

**When you need to find other projects in the work ecosystem** (e.g., to understand
cross-project dependencies, shared libraries, related services, or sibling projects),
**consult the project lock file:**

```
.graphit/cluster.lock.json
```

This file is **automatically generated** during `graphit sync` and contains only the
sibling projects that belong to the **same cluster** as the current project.
Clusters are managed via `graphit cluster <key> <value>` — projects sharing at
least one identical cluster label are grouped together. Projects without any labels
form their own default group.

Each sibling project entry includes:

| Field | Description |
|---|---|
| `projects.<id>.dir` | Absolute path to the project root directory |
| `projects.<id>.name` | Human-readable project name |
| `projects.<id>.description` | Project description |
| `projects.<id>.cluster` | Cluster labels (key→value map) |
| `projects.<id>.registeredAt` | When the project was registered |

**With the project paths from this file you can:**

- **Discover and navigate** — find sibling project directories and read their source, docs, or lockfile
- **Query code in another project** — run AST or full-text search against a sibling:
  ```bash
  cd /path/to/other-project && graphit ast query "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'handler' RETURN f.name, f.path" --ai-optimized
  ```
- **Read another project's knowledge wiki** — understand its architecture without grepping:
  ```bash
  cat /path/to/other-project/.graphit/knowledge/project/index.md
  ```
- **Make cross-project changes** — if the user asks to modify code in another project,
  use the path from `cluster.lock.json` to locate, read, and edit files there directly

**Example workflow:** The user asks "how does the auth service validate tokens?".
You read `.graphit/cluster.lock.json`, find the auth service project path,
then run `cd /path/to/auth-service && graphit ast query "MATCH (f:Function) WHERE toLower(f.name) CONTAINS 'validate' RETURN f.name, f.path, f.line_number" --ai-optimized`
to locate the validation logic, and read the relevant source files.

## ⚠️ Rule

Rely entirely on the official artifacts from the Hub rather than generic internet knowledge.
When in doubt: `graphit hub list` → `graphit hub show <id>` → `graphit hub install <id>`.
