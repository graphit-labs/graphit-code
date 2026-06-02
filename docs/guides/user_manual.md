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
The Wiki Explorer indexes the markdown files in your codebase:
- **Default Indexing Path**: By default, it scans the entire project root directory (respecting ignore rules). You can customize this by setting `knowledge.docs_dir` in your configuration to point to a specific directory (like `docs/`).
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

## Customizing AST Tree-sitter Queries

The AST module extracts code entities (functions, classes, imports, etc.) from source files using **Tree-sitter query patterns**. These patterns are defined in **YAML files** that you can fully customize — adding new extraction patterns, removing defaults, or replacing the entire query set for a language — all without recompiling.

### How It Works

When Graphit Code parses a source file, it resolves query patterns using a **4-level priority chain**:

1. **Project** (`.graphit/ast/queries/`) — Highest priority. Applies only to this project.
2. **User Global** (`~/.graphit/ast/queries/`) — Your personal customizations. Applies to all projects. **Never written by the framework.**
3. **Runtime** (`~/.graphit/runtime/<version>/ast/queries/`) — Factory defaults extracted from the binary. **Automatically updated on each version upgrade.**
4. **Embedded** — Compiled into the binary. Used only if the runtime directory is unavailable.

For each language, the **first source that provides queries wins**. If you create a `go.yaml` in your project, only Go queries use the project version — all other languages continue resolving from user → runtime → embedded.

### Viewing the Defaults

After your first `graphit sync` or `graphit ast index`, the runtime defaults are extracted to:
```
~/.graphit/runtime/<version>/ast/queries/
```

Browse these files to see every Tree-sitter pattern used for each language:
```bash
ls ~/.graphit/runtime/*/ast/queries/
# c.yaml  cpp.yaml  csharp.yaml  dart.yaml  go.yaml  java.yaml
# javascript.yaml  kotlin.yaml  php.yaml  python.yaml  ruby.yaml
# rust.yaml  sql.yaml  swift.yaml  tsx.yaml  typescript.yaml
```

### Customizing Globally (All Projects)

To modify queries for all your projects, copy the default file to the user global directory and edit it:

```bash
# Create the user global directory
mkdir -p ~/.graphit/ast/queries/

# Copy the runtime default as a starting point
cp ~/.graphit/runtime/*/ast/queries/go.yaml ~/.graphit/ast/queries/go.yaml

# Edit to add your custom patterns
$EDITOR ~/.graphit/ast/queries/go.yaml
```

### Customizing Per Project

To customize queries for a single project, create the file in the project's `.graphit/` directory:

```bash
mkdir -p .graphit/ast/queries/
cp ~/.graphit/runtime/*/ast/queries/python.yaml .graphit/ast/queries/python.yaml
$EDITOR .graphit/ast/queries/python.yaml
```

### Example: Adding Custom Patterns

To track goroutines as function entities in Go, add a new query entry to `go.yaml`:

```yaml
language: go
extensions: [".go"]
queries:
  # ... keep existing queries ...

  # Custom: track goroutine launches
  - data_key: goroutines
    graph_label: Function
    pattern: '(go_statement (call_expression function: (identifier) @fn))'
    name_capture: fn
```

### Example: Completely Replacing Queries

Set `replace: true` to discard all lower-priority queries and use only your definitions:

```yaml
language: sql
extensions: [".sql"]
replace: true   # Ignore runtime/embedded defaults entirely
queries:
  - data_key: tables
    graph_label: Table
    pattern: '(create_table_statement name: (identifier) @name)'
  - data_key: procedures
    graph_label: Function
    pattern: '(create_procedure_statement name: (identifier) @name)'
```

### YAML Reference

| Field | Required | Description |
|---|---|---|
| `language` | ✅ | Tree-sitter language name (e.g., `go`, `python`, `typescript`) |
| `extensions` | ❌ | File extensions filter (e.g., `[".ts"]`). Omit to match all extensions |
| `replace` | ❌ | `true` = replace lower-priority queries; `false` = append (default) |
| `queries[].data_key` | ✅ | Entity category: `functions`, `classes`, `imports`, `calls`, `fields`, etc. |
| `queries[].graph_label` | ❌ | LadybugDB node label (e.g., `Function`, `Class`). Empty = relational data |
| `queries[].pattern` | ✅ | Tree-sitter S-expression query |
| `queries[].name_capture` | ❌ | Capture group name for the entity. Default: `name` |

> For the full technical specification and implementation details, see `docs/specs/ast_module.md`.

---

## Customizing Module Rules and Skills

Both the on-demand IDE agent and the background Dream agent follow **rules** and **skills** defined per module (AST, Knowledge, Memory, Hub, and Improvements).
Rules are the compact instructions injected into the global rules file (e.g., `AGENTS.md`). Skills are the detailed instruction files that agents read on-demand.
Graphit Code provides a **multi-layer override system** so you can customize both at different scopes.

### Override Hierarchy (highest to lowest priority)

1. **Project-Level** — `.graphit/rules/<module>.md` / `<module>_skill.md` in the project directory. Applies only to that project.
2. **Global CLI** — `~/.graphit/rules/<module>.md` / `<module>_skill.md`. Applies to all projects on your machine.
3. **Hub Main Branch** — `rules/<module>.md` / `rules/<module>_skill.md` on the `main` branch of the Hub Git repository. Applies to all team members automatically.
4. **Compiled-In Default** — Built into the Graphit Code binary.

The first source found wins. This means a project-level override always takes precedence, followed by the user's global override, then the team's Hub-distributed override, and finally the built-in default.

### Managing Rules via CLI

Every module exposes a `rule` subcommand:

```bash
# Output resolved rules (respecting the full override hierarchy)
graphit improvements rules

# Show only the compiled-in default (ignore all overrides)
graphit improvements rules --default

# Set a custom global CLI override from a file
graphit improvements rules my-rules.md

# Restore default ruleset (removes the global CLI override)
graphit improvements rules --unset
```

This works for all modules: `graphit ast rule`, `graphit knowledge rule`, `graphit memory rule`, `graphit hub rule`, `graphit improvements rules`.

### Embedding the Default with Placeholders

Custom rule and skill files support placeholders that embed the compiled-in default content, allowing you to **wrap** the defaults with additional instructions:

**For rules** — use `{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}`:

```markdown
# My Custom Security Requirements
- All endpoints must enforce mTLS in production.

## Standard Analysis
{{_GRAPHIT_DEFAULT_RULE_CONTENT_}}
```

**For skills** — use `{{_GRAPHIT_DEFAULT_SKILL_CONTENT_}}`:

```markdown
# Extra Skill Instructions
- Always check for race conditions in concurrent code.

## Standard Skill
{{_GRAPHIT_DEFAULT_SKILL_CONTENT_}}
```

The placeholders are replaced at runtime with the full default content. This lets you extend the defaults rather than completely replacing them.

### Team-Wide Rules and Skills via Hub

To enforce standards across your entire team, commit rule and/or skill files to the `rules/` directory on the `main` branch of the Hub Git repository. For example:

```
hub-repo (main branch)
└── rules/
    ├── improvements.md          # team-wide improvements rule override
    ├── ast.md                   # team-wide AST rule override
    └── memory_skill.md          # team-wide memory skill override
```

Every team member will receive these overrides automatically on `graphit sync` or `graphit update` — they are distributed via git pull, without needing each developer to manually configure their machine.

> For the full technical specification, see `docs/specs/rule_override.md`.


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
