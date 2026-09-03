---
title: "Agent Hook Activation"
description: "Required trust, activation, reload, and verification steps for every Graphit adapter."
content-type: guide
audience: developers
updated: 2026-09-02
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

`graphit init` and `graphit sync` write the project hook and MCP configuration for the selected adapter. A hook cannot grant trust to itself: when an agent requires approval for project code, each user must approve the checkout on that machine. This approval is intentionally stored by the host outside the repository.

Hook activation and MCP availability are separate checks:

- the hook injects the current Graphit mandates, installed Hub rules, and memory bootstrap;
- the MCP configuration exposes the `graphit_*` tools;
- a subagent can receive the instructions but still have a restricted tool allowlist.

After `graphit sync`, follow the section for your adapter and start a new session. Re-run the check after changing adapter, cloning to another path, changing a hook command, or changing a custom agent's tool permissions.

## Codex

**User action.** Start Codex in the repository and run `/hooks`. Review the project hooks from `.codex/hooks.json` and mark the Graphit hooks as trusted. Codex binds trust to the hook content, so repeat this review after a Graphit update changes the generated command. Do not use `--dangerously-bypass-hook-trust` for routine interactive use.

**Verify.** `/hooks` must show the Graphit `SessionStart` and `SubagentStart` hooks as enabled and trusted. Start a new task and confirm that the `graphit_*` MCP tools are present. For a custom subagent, do not remove Graphit MCP tools from its tool allowlist; otherwise the subagent intentionally uses its native tools.

See the [Codex hooks documentation](https://learn.chatgpt.com/docs/hooks).

## Claude Code

**User action.** Accept Claude Code's workspace-trust dialog for the repository (or a trusted parent). Interactive Claude sessions withhold every settings-file hook until that approval. No separate Graphit registration is required after trust is granted.

**Verify.** Run `/hooks` and confirm that Graphit's hooks come from Project Settings in `.claude/settings.json`. Start a new session so `SessionStart` runs. `SubagentStart` supplies the child instructions; custom agents must not exclude Graphit through `tools` or `disallowedTools` if they are expected to use its MCP tools.

See the [Claude Code hooks documentation](https://code.claude.com/docs/en/hooks#workspace-trust).

## Cursor

**User action.** No hook-specific approval is documented for local Cursor project hooks. Open the repository as the workspace and start a new Composer conversation. Cursor watches `.cursor/hooks.json`; if a synced hook is not reloaded, restart Cursor.

**Verify.** Open **Customize → Hooks** and the **Hooks** output channel. Confirm that the project `sessionStart` hook ran and that Graphit MCP tools are available. Local subagents inherit the parent's tools and Graphit augments `Task` creation with its subagent protocol.

**Cloud limitation.** Cursor Cloud does not run `sessionStart`, and local MCP configuration is not available in its VM. Project hooks still run at the cloud-supported boundaries, but they cannot inject Graphit at the true start of the first read-only turn. Configure Graphit MCP at the Cursor team/enterprise layer, or use a local/self-hosted agent when first-turn Graphit bootstrap is required.

See the [Cursor hooks documentation](https://prod.cursor.com/docs/hooks#cloud-agent-support).

## Gemini CLI

**User action.** If folder trust is enabled, choose **Trust folder** when Gemini prompts for the repository. If it was previously rejected, run `/permissions` and change the current folder to trusted. An untrusted folder ignores `.gemini/settings.json`, which disables both the project hooks and project MCP servers. Do not use `--skip-trust` or `GEMINI_CLI_TRUST_WORKSPACE=true` for routine interactive use; those are intended for reviewed headless automation.

**Verify.** Run `/hooks list` or `/hooks panel` and confirm that the Graphit hooks from `.gemini/settings.json` are enabled. Start a new session so `SessionStart` runs. Graphit's `BeforeAgent` hook reapplies resident context on agent turns; if a custom agent declares an explicit tool list, ensure that it includes the Graphit MCP tools.

See [Gemini CLI trusted folders](https://geminicli.com/docs/cli/trusted-folders/) and the [hooks command reference](https://geminicli.com/docs/cli/commands/#hooks).

## Kiro

**User action.** No manual registration is required. Kiro automatically activates `.kiro/hooks/*.json` at session start, and Graphit writes its entries with `enabled: true`. If a user disabled the hook in Kiro, open **Agent Hooks** and enable `graphit-memory-session-start` and `graphit-memory-session-start-cli` with the eye toggle or **Hook Enabled** switch.

**Verify.** Confirm both Graphit entries are enabled in **Agent Hooks**, then start a new IDE session or CLI agent. Custom agent profiles that must use Graphit need project MCP inclusion enabled; a profile that disables `includeMcpJson` continues with its native tools by design.

See [Kiro hooks](https://kiro.dev/docs/hooks/) and [hook management](https://kiro.dev/docs/hooks/management/).

## Google Antigravity

**User action.** No separate registration or trust prompt is documented for workspace hooks. Open the repository as an Antigravity workspace and start a new conversation. The project hook in `.agents/hooks.json` is enabled by default unless the user explicitly disables it.

**Verify.** Confirm that `.agents/hooks.json` contains the enabled `graphit-memory-session-start` entry; in Antigravity CLI, run `/hooks` to inspect loaded hooks. Start a new conversation. Graphit's `PreInvocation` hook injects the bootstrap before the first model call and refreshes resident context on later calls. A static custom agent must retain the project MCP server/tool configuration if it is expected to use Graphit.

See the [Antigravity hooks documentation](https://antigravity.google/docs/hooks) and [CLI hook management](https://antigravity.google/docs/cli/plugins/#managing-hooks).

## OpenCode

**User action.** No separate hook trust or registration step is documented. OpenCode automatically loads JavaScript and TypeScript files under `.opencode/plugins/` at startup. Fully restart the OpenCode CLI or desktop app after the first sync or after the Graphit plugin file changes.

**Verify.** Start OpenCode from the repository and confirm that `graphit_*` tools from `opencode.json` are available. If the plugin does not load, start with `opencode --log-level DEBUG` and inspect the newest log under `~/.local/share/opencode/log/`. Custom agent permissions must not deny Graphit MCP tools when Graphit-first behavior is required.

See [OpenCode plugins](https://opencode.ai/docs/plugins/) and [OpenCode troubleshooting](https://opencode.ai/docs/troubleshooting/).

## End-to-end check

In a fresh main-agent session, ask the agent to summarize the `Graphit session bootstrap` from its current instructions and list its `graphit_*` tools. Then ask it to delegate the same check to a subagent. The first answer verifies hook delivery; the tool list verifies MCP visibility; the delegated answer verifies the child boundary and its tool policy.

If any one of the three fails, use the matching adapter section above and [Troubleshooting](troubleshooting.md). Do not treat the presence of generated files alone as proof that the host loaded them.

## Expected fallback

If a host does not expose a required lifecycle boundary, the workspace is not trusted, hooks are disabled, or a child agent cannot see Graphit MCP tools, Graphit cannot manufacture that capability from project files. The agent or subagent must continue with the native tools that its host exposes. This is a compatibility fallback, not proof that installation succeeded: use the adapter-specific verification above when Graphit was expected to be active.
