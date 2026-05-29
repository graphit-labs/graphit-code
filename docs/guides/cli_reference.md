---
title: "CLI Command Reference"
description: "Reference manual detailing every subcommand, flag, and configuration override option in the Graphit CLI."
content-type: reference
audience: developers
keywords:
  - CLI
  - commands
  - reference
  - arguments
  - flags
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/user_manual.md"
---

# Command Line Interface Reference

The `graphit` command-line tool acts as the control center for configuring project registries, indexing code structure, query-tuning, and running background daemon tasks.
This document lists every command and flag available in the CLI.

---

## Global Flags

These flags can be appended to any command:

| Flag | Shorthand | Description | Default |
|---|---|---|---|
| `--verbose` | `-v` | Enables detailed debug logs on stdout. | `false` |
| `--config` | `-c` | Override a configuration value inline (e.g. `-c ide=cursor`). | `nil` |

---

## Project Lifecycle Commands

These commands initialize and maintain Graphit Code environments:

### `setup`
Initializes global workspaces, global directories (`~/.graphit`), configuration paths, and links repositories.
```bash
graphit setup
```

### `init`
Initializes a new project workspace.
It creates a project-local `graphit.lock.json` file, registers the project under the global active tracker, and generates rules or skills files in your `.graphit/` directory.
```bash
graphit init --ide <ide_name> [flags]
```
**Flags:**
- `--ide <string>`: Targets a specific model coding assistant (e.g. `cursor`, `claude`, `kiro`, `gemini`, `antigravity`).
- `--id <string>`: Explicitly overrides or sets the project ULID metadata.
- `--name <string>`: Sets the human-readable project name.
- `--description <string>`: Sets the project description inline to skip prompts.

### `sync`
Forces a complete synchronization of the project state.
```bash
graphit sync [flags]
```
**Flags:**
- `--no-background`: Prevents spawning background tasks asynchronously. Both phases (sync and heavy indexing/processing) execute synchronously inside the terminal process.
- `--heavy`: Runs only Phase 2 tasks (generating embeddings and memory consolidation).

### `update`
Checks for updates to registered artifacts (rules, skills, prompts) in the decentralized Hub registry and pulls down fresh copies.
```bash
graphit update --ide <ide_name>
```

### `remove`
Deletes project-local configuration, rules files, git hooks, and ignorer configurations. Does not affect source code or the global database.
```bash
graphit remove --ide <ide_name>
```

### `uninstall`
Cleans up all global parameters, schedules, background daemons, and deletes the `~/.graphit` workspace directory.
```bash
graphit uninstall
```

### `self-update`
Self-downloads and replaces the current `graphit` CLI binary with the latest release published on GitHub.
```bash
graphit self-update
```

---

## Configuration Management

### `config`
Gets, sets, or unsets parameters inside your active configuration file.
By default, targets your project-local `graphit.lock.json`.
```bash
graphit config [key] [value] [flags]
```
**Flags:**
- `--global`: Reads or updates parameters in `~/.graphit/config.json` instead.
- `--get`: Retrieves the value of a key.
- `--unset`: Deletes a key from the configuration.
- `--list`: Prints all keys and values.
- `--secret`: Reads a value securely from stdin without echoing it on terminal logs.

**Examples:**
```bash
graphit config ide cursor
graphit config --global ide cursor
graphit config --get ide
graphit config --unset ide
graphit config --list
```

---

## Dashboard Interface

### `ui`
Launches the local Vite unified web application server.
Allows you to explore the AST code database in 3D, chat with the wiki knowledge, and view memories.
```bash
graphit ui [flags]
```
**Flags:**
- `--port <int>`: Port for the local UI server (defaults to `8080` or `8080+` if in use).

---

## MCP Integration

### `mcp`
Starts an MCP (Model Context Protocol) server.
Allows AI tools to consume AST querying, memory search, and wiki indexes via standardized MCP actions.
```bash
graphit mcp [flags]
```
**Flags:**
- `--stdio`: Starts the MCP server over standard input/output (used by Claude Code).
- `--port <int>`: Starts the MCP server over HTTP/SSE.

---

## Subsystem Commands

### `ast`
Directly indexes, queries, and manages the abstract syntax tree.
```bash
graphit ast <subcommand> [flags]
```
**Subcommands:**
- `index [path]`: Parses source code and builds the AST knowledge graph.
  - `--reset`: Wipe database before indexing.
  - `--reindex`: Wipe only this repo's data before re-indexing.
  - `--cluster <name>`: Logical cluster tag for queries.
  - `--workers <int>`: Worker thread count.
  - `--no-source`: Skip storing raw source code inside nodes.
- `watch [path]`: Watch directory for file changes and re-index incrementally.
  - `--cluster <name>`: Logical cluster tag.
  - `--workers <int>`: Worker thread count.
- `query <cypher-query | natural-language-question>`: Execute graph query.
  - `--ai`: Generate Cypher from natural language via AI.
  - `--cypher`: Print generated Cypher without executing (requires `--ai`).
  - `--ai-optimized`: Output tabular representation for AI agent tokens.
  - `--hybrid`: Perform combined BM25 + semantic vector search (RRF).
  - `--top <int>`: Limit results count.
  - `--context <name>`: Query an imported context instead of project.
- `schema`: Print the AST node properties and labels schema.
  - `--context <name>`: Context name.
- `install <path> --context <name>`: Import external AST database into named context.
  - `--reset`: Wipe context before importing.
  - `--list`: List imported contexts.
  - `--workers <int>`: Thread count.
- `remove`: Wipe project AST or context.
  - `--context <name>`: Context name.
- `sync`: Re-sync imported context from cache.
- `export`: Export AST graph to Obsidian vault or `.ast` bundle.
  - `--format <format>`: Format: `obsidian` or `bundle`.
  - `--output <dir>`: Output path.
  - `--no-sources`: Exclude source code contents.
- `list`: List all installed AST contexts.
- `source <relative-path>`: Show stored source code for a file.
  - `--entity <name>`: Extract specific entity range.
  - `--entity-type <type>`: Class context type.
  - `--head <int>`: Show first N lines.
  - `--tail <int>`: Show last N lines.
  - `--start <int>`: Start line.
  - `--end <int>`: End line.
  - `--pattern <string>`: Grep-like match.
  - `--regex`: Treat pattern as regex.
  - `--before <int>`: Context before match.
  - `--after <int>`: Context after match.
  - `--line-numbers`: Print line prefixes.
  - `--context <name>`: Context name.
- `embed`: Generate vector embeddings for semantic search.
- `rule`: Manage the AST module rule.
  - `--unset`: Remove customization.
  - `--default`: Show default rule.

### `knowledge`
Manages project documentation wiki and contexts.
```bash
graphit knowledge <subcommand> [flags]
```
**Subcommands:**
- `index [path]`: Scan docs/ and compile wiki index.
  - `--reset`: Clear knowledge graph first.
  - `--louvain`: Detect community structures.
  - `--workers <int>`: Thread count.
  - `--context <name>`: Re-index context.
- `watch [path]`: Watch docs/ and incrementally compile.
  - `--louvain`: Detect community structures.
- `query <text>`: Search the knowledge wiki.
  - `--context <name>`: Search context.
- `lint`: Audit wiki files for link defects.
  - `--fix`: Fix broken backlinks.
  - `--deep`: Enable AI contradiction audit.
  - `--stale-days <int>`: Age threshold.
  - `--context <name>`: Context name.
- `schema`: Print knowledge graph schema.
- `install <name>`: Fetch knowledge context.
- `remove`: Clear knowledge graph or context.
  - `--context <name>`: Context name.
- `sync`: Re-sync context.
- `export`: Export wiki DB.
- `list`: List contexts.
- `rule`: Manage knowledge rule.

### `memory`
Manipulates persistent agent memories.
```bash
graphit memory <subcommand> [flags]
```
**Subcommands:**
- `index`: Scan memories and regenerate wiki.
  - `--user`: Target user scope.
  - `--louvain`: Run clustering.
  - `--context <name>`: Re-index context.
- `watch`: Watch memory changes.
  - `--user`: Target user scope.
  - `--louvain`: Run clustering.
- `query <question>`: Query memories using AI.
  - `--user`: Target user scope.
  - `--context <name>`: Context name.
- `schema`: Show memory graph schema.
- `install <project-id-or-name>`: Fetch external memory context.
- `remove`: Remove memory graph.
  - `--context <name>`: Context name.
- `sync`: Sync cache.
- `export`: Export memory wiki database.
- `list`: List memories.
  - `--user`: List user scope.
- `insert <title>`: Add new memory entry.
  - `--content <body>`: Memory details.
  - `--user`: Save to user scope.
  - `--project`: Link user memory to active project.
  - `--important`: Surface in IDE rules.
  - `--type <type>`: convention, correction, decision, tension, fact, or skill.
  - `--tags <list>`: Comma-separated tags.
  - `--context <name>`: Target context.
- `update <id>`: Modify existing memory.
  - `--content <body>`: New body.
  - `--title <title>`: New title.
  - `--user`: User scope.
- `delete <slug>`: Remove memory.
  - `--user`: User scope.
  - `--context <name>`: Context name.
- `search <term>`: Grep-like memory search.
  - `--user`: Target user scope.
- `important`: List important memories.
  - `--user`: User scope.
- `promote <id>`: Mark memory as important.
  - `--user`: User scope.
- `demote <id>`: Remove important status.
  - `--user`: User scope.
- `consolidate`: Run AI review for duplicates.
  - `--user`: User scope.
  - `--apply`: Save corrections.
- `gc`: Clean stale/empty memories.
  - `--user`: User scope.
  - `--dry-run`: Simulation mode.
  - `--stale-days <int>`: Expiry threshold.
- `rule`: Manage memory rule.

### `wiki`
AI multi-wiki explorer.
```bash
graphit wiki <subcommand> [flags]
```
**Subcommands:**
- `search <query>`: Search multiple wiki sources.
  - `--wiki <refs>`: Sources to search: `project`, `memory`, or project IDs.
  - `--hub <refs>`: Registry knowledge artifacts.
  - `--session <name>`: Session identifier.
  - `--continue`: Resume last session.
  - `--top-k <int>`: BM25 filter limit.
- `chat`: Multi-turn chat over wiki context.
  - `--session <id>`: Session ID.
  - `--continue`: Resume last session.
- `sessions`: List or delete wiki sessions.
  - `--delete <id>`: Delete session ID.

### `daemon`
Controls background service lifecycle.
```bash
graphit daemon <subcommand> [flags]
```
**Subcommands:**
- Foreground daemon start: `graphit daemon`
  - `--no-embedding`: Disable embedding server.
  - `--no-dream`: Disable dream runner.
  - `--log <path>`: Log file.
- `stop`: Terminate running daemon.
- `status`: Show daemon status and log tail.
- `restart`: Restart daemon.
- `scheduler <install|remove|status>`: Manage OS system launchers (cron, launchd, task scheduler).

### `dream`
Controls autonomous idle improvement agents.
```bash
graphit dream <subcommand> [flags]
```
**Subcommands:**
- `status`: Show dream state (active/idle/exhausted).
- `reports`: List dream session reports.
  - `--all`: Show all reports.
- `subject <list|add|rm>`: Manage dream subjects queue.
  - `subject list`: List subjects.
  - `subject add [title]`: Add new subject.
    - `--body <body>`: Subject details.
  - `subject rm [slug]`: Remove subject.

### `improvements`
Manages code improvement rules.
```bash
graphit improvements <subcommand> [flags]
```
**Subcommands:**
- `rules [file]`: Output resolved rules or set custom rule override.
  - `--default`: Output built-in defaults.
  - `--unset`: Remove override.
- `rule [file]`: Customize IDE rule block.
  - `--default`: Output built-in defaults.
  - `--unset`: Remove override.

### `cluster`
Manages project grouping.
```bash
graphit cluster [key] [value] [flags]
```
Without subcommands:
- `graphit cluster <key> <value>`: Set cluster label.
- `--get <key>`: Read label.
- `--list`: List all labels.
- `--unset <key>`: Remove label.

**Subcommands:**
- `projects`: List sibling projects.
