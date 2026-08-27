---
title: "Improvement Backlog Specification"
description: "Specification of the improvement backlog — deferred work recorded in the documentation tree instead of dropped."
content-type: reference
audience: developers
keywords:
  - backlog
  - improvements
  - deferred work
  - review
  - specs
prerequisites:
  - "docs/specs/improvements_module.md"
related:
  - "docs/specs/dream_module.md"
  - "docs/specs/config_module.md"
---

# Improvement Backlog Specification

The **improvement backlog** is where work that was identified but deliberately not done gets
written down. It is produced by the Improvements module's review methodology and consumed by
the Dream module's autonomous sessions.

## Purpose

Every review turns up more than the current change should carry. Without a backlog, such a
finding has two exits and both are bad: widen the change until it is unreviewable, or mention
it once in prose and lose it. The backlog is the third exit.

An item belongs in the backlog when it is:

| Finding | Why it is deferred rather than folded in |
|---|---|
| A real problem outside the scope that was given | Widening scope without being asked is its own defect |
| A refactor too large to do safely alongside the fix in flight | Two unrelated risks in one diff is how a revert loses the good half |
| An audit worth running across the whole codebase | Breadth is what an idle machine is good at |
| Something deliberately **not** changed, that a human should still see | Otherwise the decision dies with the author's context |

## Storage

Each item is a markdown file. The directory resolves through the standard configuration chain:

| Key | Default | Meaning |
|---|---|---|
| `improvements.backlog_dir` | `docs/tasks/backlog` | Directory holding the backlog, relative to the project root |

The default is **composed from `knowledge.docs_dir`** rather than hardcoded, so a project that
keeps documentation elsewhere gets its backlog in the matching place: setting
`knowledge.docs_dir` to `documentation` moves the default to `documentation/tasks/backlog`.

Precedence is the framework's usual order: inline → `GRAPHIT_IMPROVEMENTS_BACKLOG_DIR` →
project `graphit.lock.json` → global `~/.graphit/config.json` → compiled-in default.

### Why the documentation tree and not the brand directory

The backlog used to live at `.<brand>/dream/subjects/`, and `graphit init` gitignored the whole
brand directory at the time, which meant deferred work was invisible to every other checkout of
the project, to code review, and to anyone not sitting at the machine that recorded it. Only
`.<brand>/runtime/` and `.<brand>/grammars/` are the two ignored ownership trees today,
so that particular trap is gone — but the destination was never only about visibility.

A backlog item is a project artifact, not machine state. Keeping it under `docs/` has three
consequences, all intended:

1. **It is versioned.** An item is committed, reviewed, and travels with the repository.
2. **It is indexed into the knowledge wiki.** `docs/` is the wiki's scope, so the backlog is
   searchable through `graphit_knowledge_search` alongside the task logs.
3. **It sits beside the task logs it becomes.** `docs/tasks/` holds the record of completed
   work; `docs/tasks/backlog/` holds the work not started yet.

## File Layout

| File | Meaning |
|---|---|
| `<slug>.md` | The item. First `# ` heading is its title; the rest is the brief. |
| `<slug>.done.md` | The result. Its presence is what marks the item done. |

The slug is derived from the title: NFKD-normalised, combining marks stripped, lower-cased,
every run of non-alphanumeric characters collapsed to `-`, trimmed, truncated to 60
characters. An item whose title slugifies to the empty string is rejected.

Items are listed oldest-first by file modification time. **Pending** means no `.done.md`
counterpart exists; the Dream module always picks the oldest pending item.

Files are written `0o644` inside a `0o755` directory — the permissions of documentation, not
of the machine-state files under the brand directory.

## Interfaces

### MCP tools

| Tool | Description |
|---|---|
| `graphit_improvements_backlog_list` | List the backlog |
| `graphit_improvements_backlog_add` | Add an item (`title`, optional `body`) |
| `graphit_improvements_backlog_remove` | Remove an item by `slug` |

The tools live in the `improvements` namespace because the Improvements module owns the
concept. Whether anything will *act* on an item is a Dream question, answered by
`graphit_dream_status` (`enabled`, `daemon_running`, `pending_backlog`).

### CLI

```bash
graphit improvements backlog list
graphit improvements backlog add "Refactor the auth module"
graphit improvements backlog add "Fix API error handling" --body "Focus on /api/v2 endpoints"
graphit improvements backlog rm refactor-the-auth-module
```

### HTTP API

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/backlog?project_dir=<dir>` | List the backlog |
| `POST` | `/api/backlog/item?project_dir=<dir>` | Add an item — JSON body `{"title": "...", "body": "..."}` |
| `DELETE` | `/api/backlog/item/{slug}?project_dir=<dir>` | Remove an item |

### Item schema

Serialised identically by the MCP tools and the HTTP API:

| Field | Type | Description |
|---|---|---|
| `slug` | string | Filename without extension; the item's identifier |
| `title` | string | First `# ` heading, falling back to the slug |
| `body` | string | Full file contents (omitted when empty) |
| `path` | string | Absolute path to the item file |
| `created_at` | timestamp | File modification time |
| `done` | bool | Whether a `.done.md` counterpart exists |
| `result_path` | string | Absolute path to the result file (omitted when pending) |

## Writing a Good Item

The agent or session that picks an item up inherits **no conversation history**: not the files
the author was in, not what the user said, not why it mattered. An item that says "fix the
duplication we discussed" gets nothing done.

A usable `body` names:

- the paths involved,
- the symptom, concretely,
- what was already ruled out,
- how to tell it worked.

This is the same standard the task logs are held to.

## Preconditions — recording versus acting

Adding an item always succeeds. Whether anything picks it up is a separate question, because
the Dream module is **opt-in** (`modules.dream` must be `"true"`) and needs the daemon
running. With either missing, the item still has value — it is committed, so a human finds it
in review — but nothing will action it automatically. Reporting an item as "handled tonight"
without checking `graphit_dream_status` is incorrect.

## Implementation

| Component | Location |
|---|---|
| Package | `internal/backlog` — `Item`, `Dir`, `Add`, `List`, `Pending`, `Remove`, `Pick` |
| Config resolution | `internal/config/config.go` — `DefaultBacklogDir`, `ResolveBacklogDir` |
| MCP tools | `internal/mcpstdio/tools_improvements.go` |
| CLI | `cmd/graphit/commands/backlog.go` |
| HTTP handlers | `internal/uiserver/daemon_dream_handler.go` |
| Consumer | `internal/dream/dream.go` (`backlog.Pick`), `internal/dream/prompt.go` |

`internal/backlog` is a package of its own rather than part of `internal/dream`: the
Improvements module produces items, the Dream module consumes them, and neither should have to
import the other to reach the queue.
