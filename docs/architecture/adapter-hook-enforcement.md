---
title: Adapter hook enforcement
type: architecture
status: active
updated: 2026-09-03
tags: [adapters, hooks, mandates, skills, enforcement]
---

# Adapter hook enforcement

## Objective

Graphit uses hooks for observable guarantees and reserves instructions for decisions that require interpretation. The boundary is deliberate:

- **hook**: the event and input are objective, so the action can run or be blocked without judgment;
- **mandate**: a resident router dynamically composed by the hook only for enabled modules;
- **skill**: teaches the decision workflow only after its domain becomes relevant;
- **tool schema**: remains the argument reference and is not copied into the skill.

More prose does not turn an obligation into a guarantee. Likewise, a hook must not block legitimate work based on a classification it cannot prove.

## Guarantees executed by hooks

### Memory bootstrap

The hidden `_session-hook` command reads the authoritative `project` and `user` memory tables directly. Mandatory content is injected into the agent's first context, so the initial `graphit_memory_mandatory` call no longer depends on the model. If the table cannot be opened, the payload falls back to the MCP protocol and declares the required call.

Contextual recall remains semantic: the skill requires a focused search for the current request, selection by title, and reading only the relevant pages. The hook does not choose memories by score as though relevance were mechanically certain.

### Dynamic resident context

In the same event, `_session-hook` resolves the project root at runtime from the host's native field: `cwd` for Claude, Codex, Gemini, and Kiro; `workspace_roots` for Cursor; and `workspacePaths` for Antigravity. It walks upward from every candidate to the nearest Graphit lockfile, with the process working directory as the final candidate. OpenCode starts the subprocess with its runtime `directory` as the working directory. `.git` never defines a Graphit root: projects without Git work through the lockfile, while a missing lockfile leaves the root unresolved and enables only the compact fallback. No absolute checkout path from the machine that ran sync is serialized into the hook. The `--project-dir` flag remains an explicit diagnostic starting point and must also reach a lockfile.

After resolution, the command reads that project's configuration and lockfile. In stable order, it composes only the mandates for enabled modules and the bodies of installed Hub `rule` artifacts. Rules are read from the authoritative artifact (`RULE.md`), including local links; they are not copied into IDE rule directories.

Graphit does not create or update `AGENTS.md`, `CLAUDE.md`, or equivalents to deliver these instructions. Those files belong to the user when present. Skills remain physical in native host directories because hosts must discover and load them on demand.

External agents can retrieve only global mandates through parameterless `graphit_mandates`. The tool does not resolve a project or read its lockfile. On every call, the canonical configuration schema resolves environment, global configuration, defaults, and global rule overrides through the same builder used by the hook. Mandatory memories, bootstrap instructions, installed Hub rules, and project configuration are excluded from this response.

### Invariant reinjection

Full resident context is reserved for a real session or subagent start and for exceptional reconstruction after compaction. Recurring prompt or invocation boundaries receive only `CoreInvariant`, the short Graphit-first priority reminder; they do not rebuild or repeat mandatory memory, mandates, rules, or the initial bootstrap. When an adapter has a compensable gap, only that adapter's format appends its specific instruction to the invariant or checkpoint. Post-action boundaries without a gap receive only `UnitCompletionReminder`. If a required Graphit MCP tool is unavailable in the current agent, the agent continues with its native tools. The only prohibited substitution is using the Graphit CLI as though it were MCP.

Resuming, re-entering, or continuing interrupted work reapplies this priority before the next action. The hook only restores the router; the agent still classifies the domain and loads the corresponding skill only when the next action matches a trigger.

### Task state, checkpoints, and finalization

Graphit treats the smallest semantically reportable unit as a checkpoint, not as a mechanical synonym for every tool call. At available post-action events, the hook asks the agent to decide whether the unit finished and, if so, immediately update the active Graphit task with what landed and the exact next step. No state is written to Markdown task logs. Kiro also exposes `PostTaskExec`, which provides an objective specification-task boundary.

The same hook performs deterministic reconciliation in the LanceDB tables: it restores dependency, check, comment, and event projections interrupted after the authoritative CAS; expires leases; releases the stopped agent's claim when the host supplies an identity; and reopens any completion that violates a flag, evidenced checks, or subtask completion. Post-tool events renew the single task claim owned by the agent. These transitions do not depend on model interpretation.

After the final task update, the final event dispatches `graphit sync` in the background. `_session-hook --sync` starts the active Graphit executable with the `sync` argument, releases the child process, and immediately returns the native completion payload. It does not wait for indexing, a lock, or process completion. Failure to start the dispatcher is a hook error, but an already-started sync does not control or delay the agent's final response.

### Subagents: three separate guarantees

A subagent is covered correctly only when three independent layers hold:

1. **Instruction delivery** — the isolated context receives the Graphit protocol. Conversation inheritance or a rules file is never assumed when the host provides a dedicated boundary.
2. **Tool visibility** — the runtime includes Graphit MCP servers in the child's tool registry. This layer belongs to the host and can be restricted through `tools`, `disallowedTools`, permissions, `includeMcpJson`, or cloud configuration.
3. **Usage routing** — hooks deliver Graphit-first priority. A prompt cannot create a tool that is not exposed, so the child falls back to the host's native tools instead of blocking.

`SubagentProtocol` is self-contained and marked with `GRAPHIT_SUBAGENT_PROTOCOL_V1`. Claude and Codex receive it through `SubagentStart`. Cursor cannot add context at that event, so its adapter attempts to inject the protocol into the `Task` input through `preToolUse`. This hook is deliberately fail-open: if a host version does not apply `updated_input`, the child still starts and uses its native tools.

Graphit preserves user-owned subagent allowlists. Changing them silently could grant access the user intentionally removed. When an allowlist excludes Graphit, the subagent continues with the tools permitted by the host.

An external limit remains: when a host requires trust or allows hooks to be disabled, a project file cannot approve itself. Cursor Cloud also runs read-only exploratory turns before loading repository hooks. No project file can enforce Graphit-first routing before the host loads it; during that interval, the agent operates with its default capabilities. To use Graphit in cloud execution, MCP must be configured at the host's team or enterprise layer. Concrete trust, activation, reload, and verification steps are documented per adapter in [Activate Graphit Hooks in Each Agent](../guides/agent_hook_activation.md).

### Native fallback

Adapters do not block native tools. A tool-use hook payload does not prove that `graphit_ast_*` is exposed in the current agent or subagent; denying `Grep`, `Glob`, `rg`, or equivalents could block all work. The mandate and skill prioritize Graphit when available and permit the host's standard tools when it is not.

For supported local code, the skill still requires AST-first discovery. Unindexed content, unsupported formats, or unavailable tools use native discovery directly. Imported contexts have no native fallback because their source is not present in the agent's workspace.

## Adapter matrix

### Work lifecycle

| Adapter | Resume/reinjection | Smallest available unit | Asynchronous finalization |
|---|---|---|---|
| Claude Code | `SessionStart`/`SubagentStart` load bootstrap; `UserPromptSubmit` reinjects only the compact invariant | `PostToolUse` requests evaluation and immediate progress | `SubagentStop`, `Stop`, and `SessionEnd` dispatch sync |
| Codex | `SessionStart`/`SubagentStart` load bootstrap; `UserPromptSubmit` reinjects only the compact invariant | `PostToolUse` requests evaluation and immediate progress | `SubagentStop`, `Stop`, and `SessionEnd` dispatch sync |
| Cursor | `sessionStart`; `preToolUse(Task)` initializes the child | `postToolUse` requests evaluation and immediate progress | local `subagentStop`, `stop`, and `sessionEnd` dispatch sync |
| Gemini CLI | `SessionStart` loads bootstrap; `BeforeAgent` reinjects only the compact invariant | `AfterTool` requests evaluation and immediate progress | `AfterAgent` and `SessionEnd` dispatch sync |
| Kiro | `SessionStart`, `UserPromptSubmit`, and `AgentSpawn` | `PostToolUse` evaluates the unit; `PostTaskExec` covers a specification task | `Stop` dispatches sync |
| Antigravity | `PreInvocation` loads bootstrap at invocation zero and only the invariant afterward | `PostInvocation` requests evaluation and immediate progress | `Stop` dispatches sync |
| OpenCode | per-session system transform and a compaction hook | `tool.execute.after` requests evaluation and immediate progress | `session.idle` and `session.deleted` use `Bun.spawn(...).unref()` |

All final dispatches are fire-and-forget. The dispatcher uses runtime process APIs; OpenCode uses an argument array, while hosts whose schema requires a command string receive an executable escaped for the current operating system. There is no shell-dependent helper script, and generated configuration contains no checkout path. This is the common Linux, Windows, and macOS contract.

An event appearing in an API does not automatically count as coverage. Antigravity exposes `PostToolUse`, but its output accepts only `{}` and cannot reinject task-management guidance. The adapter therefore uses `PostInvocation`, which supports `injectSteps[].ephemeralMessage`, and does not install an empty `PostToolUse`. Likewise, Cursor's `beforeSubmitPrompt` cannot replace a reinjection boundary because its output cannot add context. Native limitations remain explicit instead of being hidden behind hooks with no effect.

Compensation is strictly adapter-specific. Cursor receives guidance through `sessionStart` and `postToolUse` to reapply Graphit-first routing on every prompt because `beforeSubmitPrompt` cannot inject context and Cloud may omit `sessionStart`; a late reminder governs only subsequent actions. Antigravity receives guidance through `PreInvocation` to apply the checkpoint after each tool despite the context-free `PostToolUse`, and to include the Graphit protocol in delegated prompts because no dedicated subagent-start boundary exists. Neither instruction enters the common `CoreInvariant`, the global mandate preamble, or the other five adapters. Compensation does not create the missing event or turn instruction compliance into native enforcement.

### Subagents and visibility

| Adapter | Subagent instruction | MCP tool visibility | Fallback and limit |
|---|---|---|---|
| Claude Code | `SubagentStart` injects `SubagentProtocol`; it does not depend on `CLAUDE.md`, which some built-ins do not load. | The child inherits parent tools except for background restrictions and custom-agent `tools`/`disallowedTools`. | An allowlist that removes MCP leaves the child with its permitted native tools. |
| Codex | `SubagentStart` injects `SubagentProtocol` as developer context. | The adapter installs MCP in the project; child availability still depends on the Codex surface that performed the spawn. | Without Graphit in the child registry, that surface's standard tools remain available. |
| Cursor | `preToolUse(Task)` attempts protocol injection without blocking the spawn. | A local child inherits all parent tools. A cloud child uses team-configured MCP, not local MCP configuration. | Missing Graphit or failed input rewriting leaves the native subagent path open; `subagentStart` has no context gate. |
| Gemini CLI | `BeforeAgent` reapplies compact context on each turn; `SessionStart` provides the main bootstrap. | A custom agent without `tools` inherits its parent; an explicit list can exclude `mcp_*`/`mcp_server_*`. | A restricted configuration continues with its configured built-ins. |
| Kiro | Steering is shared; `SessionStart` covers the IDE and `AgentSpawn` covers the CLI. | Subagents share project MCP and permissions; a custom profile can disable `includeMcpJson` or declare its own MCP servers. | Without project MCP, the profile continues with native tools. |
| Antigravity | `PreInvocation` injects bootstrap at invocation zero and compact context afterward; no dedicated subagent event is exposed. | Dynamic clones can inherit the toolset; a static agent controls `tools` and `mcpServers`, which default to empty. | Executions outside project hooks or without MCP continue with the host-defined toolset. |
| OpenCode | `experimental.chat.system.transform` initializes every `sessionID`, including child sessions, and `experimental.session.compacting` preserves context during compaction. | MCP configuration belongs to the project, but agent-specific permissions can deny MCP tools. | Denied MCP permissions leave the tools permitted for that agent; the plugin does not block alternatives. |

Each concrete adapter owns synchronization, removal, format, and path behavior for its host. `FolderBasedAdapter` knows nothing about hook events or formats.

## Synchronization contract

`graphit sync` reconciles each adapter as one lifecycle unit: physical skills, commands, and agents; project-local MCP configuration; and native IDE hooks. This happens on every synchronization, not only during `init`. The writer replaces previous Graphit entries with current state, preserves user-owned entries, and must be idempotent. `rule` artifacts remain in the authoritative cache and lockfile and are consumed by the next hook; sync does not materialize them in the IDE.

There are two separate wait contracts. An explicit CLI or MCP invocation reports its result to the caller. Automatic finalization only dispatches synchronization and does not wait. The agent therefore finishes without being blocked by indexing, while every supported final boundary starts a complete reconciliation attempt.

A partial update cannot be reported as success. MCP resolution or write failures and hook parse or write failures propagate through `SyncIDEAdapter`; both the CLI and `graphit_sync` report the error. The integration test changes the Graphit executable between two synchronizations and verifies that MCP and hooks for all seven adapters receive the new value and discard the old one.

## What remains semantic

The following cannot be automated safely without a model:

- building an appropriate Cypher query, selecting a source, and assessing edit impact;
- deciding which Hub artifact is relevant or when external documentation is required;
- selecting which memory or wiki results must be read;
- deciding whether a discovery is durable, duplicate, contradictory, or important;
- deciding when to record progress or a typed decision, problem, lesson, or knowledge comment on the active task;
- recognizing whether an action completed the smallest independently reportable unit before updating the task;
- deciding when freshness must be proven before a conclusion.

These responsibilities remain in mandates and skills without duplicating schema manuals, generic justification, or examples.

## Context budget

- The resident preamble has a tested 1,600-byte limit and appears only once in composed context.
- Recurring payloads that add Cursor- or Antigravity-specific compensation have a tested 1,200-byte limit; adapters without those gaps retain the previous `CoreInvariant` and payloads without additional text.
- Each module mandate has its own limit and contains only request shapes and entry tools.
- Each compact skill has an absolute tested limit between 5 and 6.5 KB.
- Every module tool appears once in the `Tool index`; argument details come from the tool's published schema.
- Memory bootstrap and full dynamic context occur only at a real session or subagent start. Recurring prompt or invocation boundaries receive only `CoreInvariant`; post-action checkpoints receive only `UnitCompletionReminder`. Compaction may reconstruct dynamic context because it represents actual context loss, not an ordinary turn event.

Bytes are the deterministic regression metric because token counts vary by host tokenizer.

## Verified official sources

- [Claude Code hooks](https://code.claude.com/docs/en/hooks)
- [Claude Code subagents](https://code.claude.com/docs/en/sub-agents)
- [OpenAI Codex hooks](https://learn.chatgpt.com/docs/hooks)
- [Cursor hooks](https://prod.cursor.com/docs/hooks) and [Cursor subagents](https://prod.cursor.com/docs/subagents)
- [Gemini CLI hooks reference](https://geminicli.com/docs/hooks/reference/) and [Gemini CLI subagents](https://geminicli.com/docs/core/subagents/)
- [Kiro hooks](https://kiro.dev/docs/hooks/), [triggers](https://kiro.dev/docs/hooks/types/), and [Kiro subagents](https://kiro.dev/docs/chat/subagents/)
- [Google Antigravity hooks](https://antigravity.google/docs/ide/hooks/) and [Antigravity subagents](https://www.antigravity.google/docs/subagents/)
- [OpenCode plugins/hooks](https://opencode.ai/docs/plugins/) and [OpenCode agents](https://opencode.ai/docs/agents/)

These capabilities were verified on 2026-09-03. Vendor changes must update this matrix and its adapter tests first; they must never be simulated through a shared abstraction the host does not support.
