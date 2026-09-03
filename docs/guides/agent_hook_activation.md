---
title: "Agent Hook Activation"
description: "Required trust, activation, reload, and verification steps for every Graphit adapter."
content-type: guide
audience: developers
updated: 2026-09-03
keywords:
  - hooks
  - trust
  - adapters
  - mcp
  - subagents
prerequisites:
  - "docs/guides/getting_started.md"
related:
  - "docs/guides/troubleshooting.md"
  - "docs/architecture/adapter-hook-enforcement.md"
---

# Agent Hook Activation

## Does every agent require approval?

No. The supported agents use different controls, and only some of them present a trust decision. Do not treat these controls as interchangeable:

- **workspace trust** decides whether project configuration may load at all;
- **hook-definition trust** approves a specific command or script;
- **enablement** turns an already discovered hook on or off;
- **reload** starts a process or conversation after the hook was installed or changed.

| Adapter | Manual approval? | When is user action required again? | How |
| --- | --- | --- | --- |
| Codex | **Yes, through Codex CLI.** Trust each non-managed Graphit hook definition. | On first use for that user and machine, and whenever the generated hook definition changes. Project trust is a separate prerequisite. | In a terminal, enter the repository, start `codex`, run `/hooks`, review every Graphit entry in `.codex/hooks.json`, and trust it. Then start a new task. |
| Claude Code | **Yes, for interactive workspace trust.** There is no separate Graphit-hook approval. | For a new untrusted folder or parent, or after the stored trust decision is changed. | Accept the workspace-trust dialog, then inspect `/hooks`. |
| Cursor Local | **No separate hook approval is documented.** | After the first sync or a hook change, only if Cursor has not reloaded it; also re-enable a hook that a user disabled. | Open the workspace and a new Composer conversation; inspect **Customize → Hooks** and restart Cursor if necessary. |
| Cursor Cloud | **No approval can add the missing boundary.** | Whenever true first-turn `sessionStart` delivery is required. | Use local/self-hosted Cursor, or configure Graphit MCP at the team/enterprise layer; Cloud does not run `sessionStart`. |
| Gemini CLI | **Conditional workspace approval.** It applies when Trusted Folders is enabled. | The first time an untrusted folder is used, for a new path outside a trusted parent, or after its trust decision changes. | Choose **Trust folder**, or run `/permissions` to change an earlier decision. |
| Kiro | **No separate hook approval is documented.** | Only when a user disabled a hook, or after sync when a new session has not loaded it yet. | Enable all Graphit entries in **Agent Hooks**, then start a new IDE or CLI session. |
| Google Antigravity | **No separate hook approval is documented.** | Only when the hook was explicitly disabled, or a conversation predates the sync. | Ensure the `.agents/hooks.json` entry is enabled and start a new conversation. |
| OpenCode | **No separate hook approval is documented.** | After the first sync or whenever the plugin file changes. | Fully restart the CLI or desktop app so startup loading runs again. |

`graphit init` and `graphit sync` write the project hook and MCP configuration for the selected adapter. At runtime, Graphit finds the project by walking upward from the directory reported by the host until it finds the Graphit lockfile. Git is not required and `.git` is never used as the project boundary. A hook cannot grant trust to itself: when an agent requires approval for project code, each user must approve the checkout on that machine. This approval is intentionally stored by the host outside the repository.

Hook activation and MCP availability are separate checks:

- the hook injects the current Graphit mandates, installed Hub rules, and memory bootstrap;
- the MCP configuration exposes the `graphit_*` tools;
- a subagent can receive the instructions but still have a restricted tool allowlist.

After `graphit sync`, follow the section for your adapter and start a new session. Re-run the check after changing adapter, cloning to another path, changing a hook command, or changing a custom agent's tool permissions.

## Codex

**Approval.** Required. Codex must first trust the project layer, then it separately reviews every non-managed project hook. Approval is bound to the current hook-definition hash, not granted permanently to every future Graphit command.

**When.** Approve on first use when that user and machine have no stored decision. Review again whenever `graphit sync` or a Graphit update changes the generated hook definition; Codex skips the changed definition until it is trusted.

**Action.** Approval must be performed through **Codex CLI**:

1. Open a terminal and enter the repository directory.
2. Start Codex CLI with `codex`.
3. Run `/hooks` inside the CLI.
4. Review the project hooks from `.codex/hooks.json` and trust all Graphit lifecycle entries: start/resume, compact per-prompt reinjection, post-tool checkpoint, subagent completion, main-agent completion, and session-end fallback.
5. Exit that task and start a new task so `SessionStart` runs with the approved definitions.

Do not use `--dangerously-bypass-hook-trust` for routine interactive use. The desktop app can consume the synchronized project configuration, but the approval procedure documented here is the Codex CLI `/hooks` flow.

**Verify.** `/hooks` must show all Graphit lifecycle hooks as enabled and trusted. Start a new task and confirm that the `graphit_*` MCP tools are present. For a custom subagent, do not remove Graphit MCP tools from its tool allowlist; otherwise the subagent intentionally uses its native tools.

See the [Codex hooks documentation](https://learn.chatgpt.com/docs/hooks).

## Claude Code

**Approval.** Required for workspace trust in an interactive session, but not separately for each Graphit hook. Interactive Claude sessions withhold every settings-file hook until the repository or a parent is trusted.

**When.** Approve the first time Claude Code opens a folder not covered by an existing trusted parent, and repeat only after moving to another untrusted path or changing/revoking that decision. Non-interactive `-p` and SDK sessions do not display this dialog and treat the folder as trusted, so review repository hooks before running unattended code.

**Action.** Accept Claude Code's workspace-trust dialog for the repository or a trusted parent. No separate Graphit registration is required after trust is granted.

**Verify.** Run `/hooks` and confirm that Graphit's hooks come from Project Settings in `.claude/settings.json`, including `UserPromptSubmit` and `SessionEnd`. Start a new session so `SessionStart` runs. `SubagentStart` supplies the child instructions; custom agents must not exclude Graphit through `tools` or `disallowedTools` if they are expected to use its MCP tools.

See the [Claude Code hooks documentation](https://code.claude.com/docs/en/hooks#workspace-trust).

## Cursor

**Approval.** No separate hook-specific approval is documented for local Cursor project hooks.

**When.** After the first sync or a change to `.cursor/hooks.json`, start a new conversation. Restart Cursor only if it did not reload the synchronized hook; re-enable the entry if a user disabled it.

**Action.** Open the repository as the workspace. If the Graphit entry was disabled, open **Customize → Hooks** and enable it. Start a new Composer conversation; if the synchronized hook still does not appear, fully restart Cursor and try a new conversation again. Cursor watches `.cursor/hooks.json`.

**Verify.** Open **Customize → Hooks** and the **Hooks** output channel. Confirm that the project `sessionStart` and local `sessionEnd` hooks are loaded and that Graphit MCP tools are available. Local subagents inherit the parent's tools and Graphit augments `Task` creation with its subagent protocol. Cursor's `beforeSubmitPrompt` cannot add context, so it is not presented as a reinjection boundary.

**Cloud limitation.** Cursor Cloud does not run `sessionStart`, and local MCP configuration is not available in its VM. Project hooks still run at the cloud-supported boundaries, but they cannot inject Graphit at the true start of the first read-only turn. Configure Graphit MCP at the Cursor team/enterprise layer, or use a local/self-hosted agent when first-turn Graphit bootstrap is required.

See the [Cursor hooks documentation](https://prod.cursor.com/docs/hooks#cloud-agent-support).

## Gemini CLI

**Approval.** Conditional. Gemini CLI's Trusted Folders feature is disabled by default, but when enabled it requires folder trust before loading project configuration. An untrusted folder ignores `.gemini/settings.json`, which disables both project hooks and project MCP servers.

**When.** Approve the first time Gemini CLI opens a folder not covered by a trusted parent, and repeat only for a different untrusted path or after changing the stored decision.

**Action.** Choose **Trust folder** in the prompt. If the folder was previously rejected, run `/permissions` and change its trust level. Do not use `--skip-trust` or `GEMINI_CLI_TRUST_WORKSPACE=true` for routine interactive use; those are temporary mechanisms for reviewed headless automation.

**Verify.** Run `/hooks list` or `/hooks panel` and confirm that the Graphit hooks from `.gemini/settings.json` are enabled, including `SessionEnd`. Start a new session so `SessionStart` runs. Graphit's `BeforeAgent` hook reapplies only the compact invariant on agent turns; if a custom agent declares an explicit tool list, ensure that it includes the Graphit MCP tools.

See [Gemini CLI trusted folders](https://geminicli.com/docs/cli/trusted-folders/) and the [hooks command reference](https://geminicli.com/docs/cli/commands/#hooks).

## Kiro

**Approval.** No separate trust or manual registration is documented. Kiro automatically discovers `.kiro/hooks/*.json`, and Graphit writes its entries with `enabled: true`.

**When.** User action is needed only if a Graphit entry was disabled, or after synchronization when the current IDE or CLI session predates the new hooks.

**Action.** If necessary, open **Agent Hooks** and enable every `graphit-*` entry with the eye toggle or **Hook Enabled** switch. Then start a new IDE session or CLI agent.

**Verify.** Confirm all Graphit entries are enabled in **Agent Hooks**, then start a new IDE session or CLI agent. Custom agent profiles that must use Graphit need project MCP inclusion enabled; a profile that disables `includeMcpJson` continues with its native tools by design.

See [Kiro hooks](https://kiro.dev/docs/hooks/) and [hook management](https://kiro.dev/docs/hooks/management/).

## Google Antigravity

**Approval.** No separate registration or trust prompt is documented for workspace hooks. The project hook in `.agents/hooks.json` defaults to enabled unless its `enabled` field is explicitly set to `false`.

**When.** User action is needed only after an explicit disablement, or when the current conversation started before the synchronized hook was available.

**Action.** Open the repository as an Antigravity workspace. If the Graphit entry in `.agents/hooks.json` contains `"enabled": false`, change it to `true` or remove the field so the default applies. Then start a new conversation.

**Verify.** Confirm that `.agents/hooks.json` contains the enabled `graphit-memory-session-start` entry; in Antigravity CLI, run `/hooks` to inspect loaded hooks. Start a new conversation. Graphit's `PreInvocation` hook injects the bootstrap before the first model call and only the compact invariant on later calls. `PostInvocation` carries the task checkpoint. Antigravity's `PostToolUse` is intentionally absent because its output contract accepts only `{}` and cannot inject the checkpoint. A static custom agent must retain the project MCP server/tool configuration if it is expected to use Graphit.

See the [Antigravity hooks documentation](https://antigravity.google/docs/hooks) and [CLI hook management](https://antigravity.google/docs/cli/plugins/#managing-hooks).

## OpenCode

**Approval.** No separate hook trust or registration step is documented. OpenCode automatically loads JavaScript and TypeScript files under `.opencode/plugins/` at startup.

**When.** Reload after the first sync and whenever the generated Graphit plugin file changes.

**Action.** Fully restart the OpenCode CLI or desktop app so startup loading runs again.

**Verify.** Start OpenCode from the repository and confirm that `graphit_*` tools from `opencode.json` are available. The plugin dispatches final sync at both `session.idle` and the deletion fallback `session.deleted`. If the plugin does not load, start with `opencode --log-level DEBUG` and inspect the newest log under `~/.local/share/opencode/log/`. Custom agent permissions must not deny Graphit MCP tools when Graphit-first behavior is required.

See [OpenCode plugins](https://opencode.ai/docs/plugins/) and [OpenCode troubleshooting](https://opencode.ai/docs/troubleshooting/).

## End-to-end check

In a fresh main-agent session, ask the agent to summarize the `Graphit session bootstrap` from its current instructions and list its `graphit_*` tools. Then ask it to delegate the same check to a subagent. The first answer verifies hook delivery; the tool list verifies MCP visibility; the delegated answer verifies the child boundary and its tool policy. On the next normal prompt, only the compact Graphit-first invariant should be added. Cursor and Antigravity also receive their own compact gap compensation at their documented boundaries; no other adapter should contain those instructions. Mandatory memory, full mandates, rules, and the startup bootstrap must not be reinjected on every turn.

Complete one small, independently reportable work unit and verify that the next injected checkpoint tells the agent to update task management immediately. Finally, end the agent or subagent and verify that the completion hook returns promptly while a background `graphit sync` is dispatched. Waiting for indexing is not expected at this boundary; an explicit CLI/MCP sync remains the diagnostic path when its result must be observed.

If any one of the three fails, use the matching adapter section above and [Troubleshooting](troubleshooting.md). Do not treat the presence of generated files alone as proof that the host loaded them.

## Expected fallback

If Cursor or Antigravity lacks the documented lifecycle boundary but loads the adapter's other context-capable Graphit hook, that adapter-specific instruction makes the missing action a semantic obligation. It does not change the common mandate or any unaffected adapter, is not native enforcement, and cannot manufacture the absent event. If the workspace is not trusted, hooks are disabled, no context-capable boundary runs, or a child agent cannot see Graphit MCP tools, the project has no delivery channel for compensation. The agent or subagent must continue with the native tools that its host exposes. This is a compatibility fallback, not proof that installation succeeded: use the adapter-specific verification above when Graphit was expected to be active.
