---
title: "User Manual"
description: "User guide on navigating the 3D dashboard, managing memories, deploying rule templates, and utilizing knowledge wikis."
content-type: guide
audience: developers
keywords:
  - user manual
  - dashboard
  - 3D graph
  - memory
  - wiki
  - dream
  - idle
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/cli_reference.md"
  - "docs/architecture/architecture_overview.md"
---

# User Manual

This manual explains how to interact with the Graphit Code system.
It covers how to use the visual dashboard, manage the memory database, set up autonomous code improvements, and work within the docs-as-code collaborative flow.

---

## Navigating the Visual Dashboard

To open the unified web application, run:
```bash
graphit ui
```
This launches a browser window (default: `http://localhost:8080`).
The interface contains a sidebar with three main modules:

### 1. Abstract Syntax Tree (AST) Explorer
The AST Explorer features an interactive **3D force-directed node canvas** that visualizes your codebase:
- **3D Canvas Navigation**: Drag with your mouse to rotate the graph, scroll to zoom, and right-click to pan.
- **Node Selections**: Click on a node (representing a function, file, or class) to highlight its connections. The sidebar displays its properties (e.g., source file path, cycle complexity, and docstring).
- **Cypher Queries**: Execute custom Cypher queries in the query bar. For example:
  ```cypher
  MATCH (fn:Function) WHERE fn.cyclomatic_complexity > 10 RETURN fn.name, fn.path
  ```
  The canvas renders the matching subset of nodes, while the results panel lists data in a tabular format.

### 2. Wiki and Knowledge Explorer
The Wiki Explorer indexes the files in your `docs/` folder:
- **Index & Logs**: Read a compiled list of all registered documents, categorized into community graphs.
- **Search**: Perform unified searches combining Full-Text Search (FTS) and semantic keyword matching.
- **Wikilinks**: Click on highlighted links to explore adjacent topics and track inbound back-references.

### 3. Collaboration Hub Manager
The Hub Manager allows you to review rules and agent configurations shared across the team:
- **Registry View**: Inspect available plugins, skills, and MCP tools in the registry.
- **Installs**: Click to deploy templates or commands into your local project structure.

---

## Managing the Memory Lifecycle

AI agents often suffer from "session amnesia"—forgetting your preferences, style guidelines, and corrections as soon as a conversation ends.
Graphit Code solves this by dividing memory into two scopes:
- **Project Memory**: Stored under `.graphit/memory/project/`. Shared across the team using a central Git repository. Best for database architectures, API contracts, and design conventions.
- **User Memory**: Stored under `.graphit/memory/user/`. Kept local to the machine or private repo. Best for personal coding preferences.

### Memory Categories

Memory cards are structured around four types:
1. **Conventions**: Rules or patterns the agent must conform to (e.g., "Use HSL tailored colors for UI components").
2. **Corrections**: Important directives logged when the agent makes a mistake (e.g., "Do not call OS stdout directly, use output package printer").
3. **Decisions**: Architectural decisions or ADRs explaining why the system was built a certain way.
4. **Skills**: Reusable discoveries or scripts to solve recurrent debugging challenges.

### Modifying Memories via CLI
You can insert, delete, or list memories directly using the CLI:
```bash
# Add a new convention memory
graphit memory insert --title "API_Response_Format" --type "convention" --content "All API endpoints must wrap responses in a standard JSON metadata envelope."

# List active memories
graphit memory list

# Delete a memory
graphit memory delete --title "API_Response_Format"
```

---

## Autonomous Idle Improvements (Dreaming)

The Dream module allows AI agents to refactor and optimize your code autonomously when you are away:
1. **Preconditions**: The daemon process must be running (`graphit daemon`).
2. **Idle Inactivity**: The system monitors file changes. If no modifications occur within the idle timeout (default: 2 hours), a dream session is triggered.
3. **Worktree Isolation**: The agent operates inside a temporary, isolated git worktree branch. It will never block or interfere with your unstaged active edits.
4. **Committing Changes**: Approved improvements are committed to a `dream/session-id` branch with special author tags.

### Submitting Refactoring Subjects
You can queue tasks or instructions for the background agent to solve during its next dream cycle:
```bash
# Register a subject for the next dream cycle
graphit dream subject add "Refactor database connection pool" --body "Check internal/db/ for connection leaks and optimize active pool sizing"

# Check subjects queue
graphit dream subject list
```

### Reviewing Dream Reports
After the session finishes, it produces a markdown report detailing its reflection, modifications, and findings:
```bash
# List recent dream reports
graphit dream reports

# Read a report
cat .graphit/dream/<session-id>.md
```

---

## Customizing Code Improvement Rules

Both the on-demand IDE agent and the background Dream agent follow a set of strict engineering analysis rules (Clean Code, Security, Concurrency, and Observability).
You can customize these rules for your project or user environment:

```bash
# Output resolved rules
graphit improvements rules

# Set a custom rules file override for this project
graphit improvements rules my-rules.md

# Restore default ruleset
graphit improvements rules --unset
```

---

## Managed Rules and Sentinel Blocks

When you initialize a project using `graphit init`, the CLI installs instructions inside your IDE config files (such as `.cursorrules`, `.codeagent`, or `AGENTS.md`).
To keep these instructions up-to-date without overwriting your custom rules, Graphit uses **Sentinel Blocks**:

```html
<!-- GRAPHIT MEMORY BLOCK -->
# 🧠 Memory Management
...
<!-- END GRAPHIT MEMORY BLOCK -->
```

> [!WARNING]
> Do not modify the text inside these sentinels manually.
> The framework automatically overwrites their content during `graphit sync` or `graphit update`. Put your custom rules outside these blocks.

---

## Docs-as-Code Synchronous Flow

To keep the AST database, memories, and wiki indexes in sync with your source code updates:
1. Make code modifications in your editor.
2. Run a sync:
   ```bash
   graphit sync &
   ```
   The background daemon will pick up files, reindex, compute embeddings, run memory GC, and recompile the local Obsidian index.
3. Your AI agent can immediately query the updated files without needing to reload its context window.
