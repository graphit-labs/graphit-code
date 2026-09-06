---
title: Filesystem, State, and Watchers
type: guide
updated: 2026-09-03
tags: [filesystem, configuration, daemon, watchers, state, adapters]
---

# Filesystem, State, and Watchers

Graphit separates repository-owned configuration from machine-owned compiled state. Understanding
that boundary explains what to commit, what the daemon watches, what can be deleted, and which files
Graphit updates when it connects an agent environment.

For storage-engine internals and the complete compiled-store tree, see
[Storage Layout](../architecture/storage_layout.md). For every configuration key and environment
override, see the [Configuration Reference](configuration.md).

## Project-owned files

| Path | Purpose | Commit it? |
|---|---|---|
| `graphit.lock.json` | Project identity, project config, installed artifacts, contexts, and adapter membership | Yes |
| `.gitignore` | Receives a marked Graphit block for machine-owned directories | Yes |
| `.astignore` | Adds code-index exclusions; nested files apply to their subtree | Usually |
| `.wikiignore` | Adds knowledge-index exclusions; nested files apply to their subtree | Usually |
| `.graphit/rules/<module>.md` | Project mandate overrides shared by the team | Yes |
| `.graphit/rules/<module>_skill.md` | Project override for the full managed Task, Memory, AST, Hub, or Knowledge skill; may include the default-content placeholder | Yes |
| `.graphit/ast/queries/*.yaml` | Project grammar-query overrides, unless `ast.queries_dir` moves them | Yes |
| `.graphit/grammars/{treesitter,antlr}/` | Platform-specific local parser libraries | No |
| `.graphit/runtime/` | Generated caches, locks, stamps, exports, Dream output, and per-project logs | No |

The first operation that must persist project state creates a minimal lockfile with an immutable
project ULID and a mutable discovery name. `graphit init` then creates or reconciles the remaining
lockfile fields and the managed `.gitignore` block without changing that ULID. Graphit edits only
its marked block and leaves user-owned ignore rules intact. `graphit remove` removes the files or
sections it owns without treating the rest of an agent configuration as disposable. See
[Project Identity](../specs/project_identity.md).

The generated ignore block covers `**/.graphit/runtime/` and `**/.graphit/grammars/` at every depth.
The broader `.graphit/` directory is intentionally not ignored: rules and AST query overrides are
repository knowledge.

## Generated project runtime

The default generated tree is:

```text
.graphit/runtime/
├── ast/export/                    default AST export destination
├── dream/                         reports, state markers, and last-seen data
├── daemon/daemon.log              project supervision log
├── daemon/dream.state             current Dream timing/session state
├── cache/skills/<adapter>/<name>  managed-skill content and mtime cache
├── cache/artifacts/...            managed-artifact synchronization cache
├── sync.stamp                     recent-sync debounce marker
├── sync.lock                      regular synchronization lock
└── sync-heavy.lock                heavyweight synchronization lock
```

This tree is disposable machine state, except for an export or Dream report that you deliberately
want to preserve. Use an explicit AST export destination or `dream.reports_dir` when output should
live in a versioned or externally managed location.

## Global state

Graphit uses `~/.graphit/` by default. `GRAPHIT_GLOBAL_DIR` moves the whole global root; it is an
environment-only bootstrap setting because the global config file lives inside the directory it
selects. A relative override is resolved from the process start directory, but an absolute path is
recommended for daemons and containers.

Important global paths include:

| Path under the global root | Purpose |
|---|---|
| `config.json` | Global configuration |
| `global.lock.json` | Registered projects and global artifact metadata |
| `memory.lock.json` | Registered external memory context mappings |
| `hub/cache/<hub-fingerprint>/<subject-fingerprint>/` | Bounded, lazy, non-authoritative Hub metadata and discovery cache |
| `daemon/daemon.pid` | Single-daemon lock and process metadata |
| `daemon/daemon.log` | Daemon log |
| `daemon/mcp.port` | Actual MCP port, including an OS-selected port |
| `daemon/mcp.key` | Active MCP bearer key; generated per start unless `mcp.api_key` is configured; mode `0600` |
| `daemon/embed.sock` | Local embedding service socket on supported platforms |
| `logs/graphit.log` | Process-wide structured log, truncated after 5 MiB |
| `sync.log` | Errors that prevented a detached lifecycle-triggered full sync from starting |
| `runtime/<version>/` | Extracted, version-scoped framework runtime |
| `models/` | Local embedding and optional reranker models |
| `frameworks/`, `artifacts/modules/` | Installed framework content and managed file-artifact materializations |
| `rules/<module>.md`, `rules/<module>_skill.md` | User-global mandate and managed-skill overrides |
| `hub/rules/` | Installed Hub overrides used after project and user-global rules |
| `grammars/{treesitter,antlr}/`, `ast/queries/` | Globally installed parser binaries and user/Hub grammar profiles |
| `ast/`, `wiki/`, `memory-table/`, `task-table/` | Authoritative compiled graphs, indexes, memories, and task state |
| `sessions/<project-hash>/{meta,messages}/` | Saved AI chat metadata and JSONL messages |
| `sessions/<session-id>/` | Ephemeral multi-artifact Live Search workspaces and event history |

Do not commit global state or read compiled stores directly. Use Graphit tools so project identity,
scope, version, remote storage, and pagination remain correct. The narrower `GRAPHIT_MODEL_CACHE`
override can place model weights on a different volume without moving the other state.

The Hub cache is isolated by both Hub and authenticated subject and may be deleted at any time. It
does not contain an eager registry mirror, cannot establish a permission, and is not used after a
failed authorization refresh. Cached discovery may be displayed as explicitly stale while offline,
but it cannot authorize content or a remote mount. See
[Hub Access Control](../specs/hub_access_control.md).

In the supplied container, `GRAPHIT_GLOBAL_DIR=/opt/graphit`, so daemon files are under
`/opt/graphit/daemon/`, not under a project `.graphit/runtime/` tree.

The default Dream directory contains `<session-id>.md` reports, an optional
`<session-id>.exhausted` deep-sleep marker, and `dream_last_seen.json`. When
`dream.reports_dir` moves the report directory, those files move with it; `daemon/dream.state`
remains generated runtime state.

## Agent adapter files

`graphit sync` materializes installed rules, commands, agents, skills, MCP definitions, and lifecycle
hooks in the selected adapter's native layout. Existing user configuration is reconciled; Graphit
tracks and removes only managed entries.

| Adapter | Managed root | Hook file | MCP file |
|---|---|---|---|
| Antigravity | `.agents/` | `.agents/hooks.json` | `.agents/mcp_config.json` |
| Claude Code | `.claude/` | `.claude/settings.json` | `.mcp.json` |
| Codex | `.codex/` | `.codex/hooks.json` | `.codex/config.toml` |
| Cursor | `.cursor/` | `.cursor/hooks.json` | `.cursor/mcp.json` |
| Gemini | `.gemini/` | `.gemini/settings.json` | `.gemini/settings.json` |
| Kiro | `.kiro/` | `.kiro/hooks/graphit-memory.json` | `.kiro/settings/mcp.json` |
| OpenCode | `.opencode/` | `.opencode/plugins/graphit-memory-session-start.js` | `opencode.json` |

Within those roots, the normal destinations are `rules/`, `commands/`, `skills/<name>/SKILL.md`,
and `agents/`; Kiro uses `steering/` for rules and `hooks/` for commands, Antigravity uses
`workflows/` for commands, and OpenCode uses `agents/` for rules. The adapter reference is the
source of truth when an upstream client changes its layout.

## Git and agent lifecycle hooks

When the repository has Git and `modules.hooks=true`, Graphit reconciles marked shell blocks in
`post-commit`, `pre-push`, and `post-merge` under the resolved Git directory. Each block dispatches
a silent, non-blocking `graphit sync --debounce 60s`. Existing shell-hook content remains in place;
a hook with a non-shell shebang is left untouched. Worktree `.git` pointer files are resolved to
their real Git directory.

Agent lifecycle files are separate from Git hooks. Supported adapters use the events their host
offers to load mandatory memory and routing context at session start, reassert current mandates on
later turns, request Task checkpoints after work units, and dispatch a full asynchronous sync at
stop. A host that lacks a particular event receives the nearest supported subset; the
[Capability and Surface Matrix](capability_matrix.md) does not imply hooks that an agent host cannot
express.

## What the daemon watches

With `modules.sync=true`, the daemon registers an operating-system filesystem watch recursively over
each active project. It does not poll `git status`.

Set `modules.sync=false` in a project's configuration to create no watcher for that project. Set it
globally to suppress watchers for every project, or use `GRAPHIT_MODULES_SYNC=false` in the daemon
environment. Restart the daemon after changing the setting; manual `graphit sync` and direct index
commands continue to work because this switch controls only daemon orchestration.

- `.git/` is always skipped.
- AST and Knowledge each apply their own ignore rules; a directory is excluded from the shared watch
  only when both consumers reject it.
- AST always excludes `.graphit/` and the root `graphit.lock.json`; it also excludes the configured
  documentation tree unless `ast.index_docs=true`.
- Knowledge watches `knowledge.docs_dir` plus the root README when
  `knowledge.include_readme=true`, filtered by `knowledge.extensions` and `.wikiignore`.
- Events are coalesced after one second of quiet and flushed after at most five seconds.
- A newly created directory is registered and scanned immediately, closing the creation/watch race.
- A kernel event overflow requests a full rescan, restoring correctness rather than trusting a
  partial event set.

Projects without filesystem activity are parked after `daemon.activity_window` (default `30m`) to
release watchers, embedding work, and Dream work. Set it to `0` to keep every registered project
supervised. Any event under the watched tree refreshes activity, even when no indexable file changed.

On Linux, a large repository or many active projects can exhaust inotify watches. Raise
`fs.inotify.max_user_watches` and `fs.inotify.max_user_instances` when Graphit reports `no space left
on device` or `too many open files` while registering directories.

## Change detection and manual synchronization

The watcher keeps active projects current, while lifecycle hooks dispatch a full asynchronous sync
when supported agents stop. Run `graphit sync` explicitly after changing configuration, artifacts,
ignore rules, grammar queries, or when you need a deterministic freshness boundary before a query.

Changing `GRAPHIT_GLOBAL_DIR` does not migrate existing state. Move the directory yourself or accept
a fresh store and run `graphit sync`. Deleting `.graphit/runtime/` only clears project-local generated
state; deleting global stores removes authoritative compiled data, memories, or task history and
should be done only through the corresponding Graphit command or tool.
