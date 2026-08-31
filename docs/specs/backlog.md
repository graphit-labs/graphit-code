---
title: "Task Backlog Specification"
description: "Specification of the documentation-backed task backlog, which records future work independently of any executor."
tags:
  - specification
  - backlog
  - tasks
---

# Task Backlog Specification

The **task backlog** records work that should survive the conversation or session that
identified it. It is project documentation and has no runtime dependency on Dream.

## Purpose

A backlog item is a self-contained future task. It may originate from a review, a user request,
an interrupted session, or any other workflow. Adding, listing, and removing items works when
Dream is enabled, disabled, running, or absent.

Dream never reads, selects, executes, completes, or reports backlog items. Its responsibility is
to improve project knowledge. Backlog work is performed explicitly by a user or agent through a
separate task workflow.

## Storage

Each item is a Markdown file. The directory resolves through the standard configuration chain:

| Key | Default | Meaning |
|---|---|---|
| `backlog.dir` | `docs/tasks/backlog` | Directory holding backlog tasks, relative to the project root |

The default is composed from `knowledge.docs_dir`, so setting the documentation directory to
`documentation` moves the default backlog to `documentation/tasks/backlog`.

Precedence is inline configuration → `GRAPHIT_BACKLOG_DIR` → project `graphit.lock.json` →
global config → compiled default.

The backlog belongs in the documentation tree because each item is versioned, reviewable,
searchable through the knowledge wiki, and adjacent to the task logs that eventually record its
execution.

## File Layout

| File | Meaning |
|---|---|
| `<slug>.md` | Task title and self-contained brief |

Items are listed oldest-first by modification time. Legacy `.done.md` files from the former Dream
integration are ignored. Slugs are NFKD-normalised, stripped of combining marks, lower-cased, collapsed to
hyphenated alphanumeric runs, and truncated to 60 characters.

## Interfaces

### MCP tools

| Tool | Description |
|---|---|
| `graphit_backlog_list` | List task backlog items |
| `graphit_backlog_add` | Record a task with a title and optional self-contained body |
| `graphit_backlog_remove` | Remove an item by slug |

These operations never consult Dream status.

### CLI

```bash
graphit backlog list
graphit backlog add "Refactor the auth module"
graphit backlog add "Fix API error handling" --body "Focus on /api/v2 endpoints"
graphit backlog rm refactor-the-auth-module
```

### HTTP API

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/backlog?project_dir=<dir>` | List the backlog |
| `POST` | `/api/backlog/item?project_dir=<dir>` | Add an item with `{"title":"...","body":"..."}` |
| `DELETE` | `/api/backlog/item/{slug}?project_dir=<dir>` | Remove an item |

### Item schema

| Field | Type | Description |
|---|---|---|
| `slug` | string | Filename without extension |
| `title` | string | First `# ` heading, falling back to the slug |
| `body` | string | Full file contents, omitted when empty |
| `path` | string | Absolute item path |
| `created_at` | timestamp | File modification time |

## Writing a Good Task

The future reader has no conversation history. A useful body names the relevant paths, the
problem, constraints, what was already ruled out, and how completion will be verified. Clients
should list before adding to avoid duplicate work.

## Execution Boundary

The backlog is a registry, not a scheduler or executor. Its API intentionally has no pending/done
state and does not assign items to Dream. A separate task workflow may read an item, perform the
work, record the result in its task log, and remove the item when appropriate.

## Implementation

| Component | Location |
|---|---|
| Package | `internal/backlog` |
| Config resolution | `internal/config/config.go` — `DefaultBacklogDir`, `ResolveBacklogDir` |
| MCP tools | `internal/mcpstdio/tools_backlog.go` |
| CLI | `cmd/graphit/commands/backlog.go` |
| HTTP handlers | `internal/uiserver/daemon_backlog_handler.go` |
