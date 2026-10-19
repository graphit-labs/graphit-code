---
title: "Agent Hook Activation"
description: "Required trust, activation, reload, and verification steps for every Graphit adapter."
content-type: guide
audience: developers
updated: 2026-10-30
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
references:
  - target: {type: task, id: tsk-4b26}
    relation: supports
  - target: {type: task, id: tsk-a2bb}
    relation: supports
  - target: {type: task, id: tsk-aa20}
    relation: relates_to
  - target: {type: task, id: tsk-9bd4}
    relation: relates_to
  - target: {type: task, id: tsk-ced2}
    relation: relates_to
  - target: {type: task, id: tsk-1c5c}
    relation: supports
  - target: {type: task, id: tsk-d0fb}
    relation: derived_from
  - target: {type: task, id: tsk-356b}
    relation: implements
---

# Agent Hook Activation

## Native qualification during the Rust rewrite

These instructions describe the current Rust native installer. Kiro V3 bindings, the
[hook planner](../specs/rust_kiro_hook_plans.md), central
[instruction templates](../architecture/rust-agent-instruction-templates.md),
[shared writer](../specs/native_shared_documents.md) and
[authenticated PROJECT hook caller](../specs/rust_project_hook_context.md)
are implemented and have focused library/caller qualification. Compatibility tests
follow the documented host contracts with controlled peers; they do not require
a vendor executable, account or paid model plan. Actual host trust, enablement and
reload still apply when using an installed agent.

The shared SDK also implements selected command-hook schemas for Codex, Claude,
Cursor, Qwen and Antigravity, Installation-owned Kimi global references,
and owned OpenCode plugin publication. Their full installed-configuration and
Graphit CLI/Source qualification is tracked jointly in `tsk-aa20`/`tsk-be6e`.
See the [SDK contract](../specs/native_shared_documents.md); Kiro's existing
Markdown profile path is preserved. Supporting a codec event does not install
per-tool/per-task reminders by default.

The following executable observations are historical: OpenCode 1 and Gemini CLI
do not qualify the current Rust integration. Gemini providers/models/API and
Antigravity remain supported; the current native CLI catalog contains eight bindings.

The 2026-10-07 inspection found Codex 0.160.1, Claude Code 2.1.280,
Cursor agent 2026.09.02-c22c1a3, Kiro CLI 2.20.2, Gemini CLI 0.42.0 and
OpenCode 1.18.23 on PATH. Antigravity is also present as `agy` 1.2.6; the
earlier search for an `antigravity` executable did not establish absence.
Qwen Code 0.25.0 and current Kimi Code 2.1.1 were installed in isolated runtimes
for configuration probes. See the [native binding contract](../development/native-host-bindings.md)
for exact observations, fixture inputs and pending runtime boundaries.
Kiro V3 is the current Rust target, selected by the `kiro-v3` binding. Graphit's
ACP consumers start `kiro-cli acp --agent-engine=v3` explicitly. The launcher
version does not identify the engine. Profiles, permissions, hooks and native
delivery are covered by the [native binding contract](../development/native-host-bindings.md).

`tsk-a2bb` must fix each host's surface, version, discovery path, event, matcher,
timeout unit and context output before the dependent producer starts. For
[Kimi](https://moonshotai.github.io/kimi-code/en/customization/hooks), a start
notification must not be treated as context delivery; the prompt boundary has
its own delivery contract. Hook errors can be advisory, so hooks alone do not
enforce read-only MCP access.

The Rust `_session-hook` caller (`tsk-1c5c`) uses an existing Source service
from the same installation, real Runtime/Local authentication and PROJECT
ReadMemory when Hooks and Memory are enabled. It returns complete plaintext
through the supported forms below; these are caller entrypoints, not evidence
of an installed Kiro workflow:

```text
graphit _session-hook --engine kiro --event SessionStart
graphit _session-hook --engine kiro --event UserPromptSubmit
```

Both forms use the current V3 event names. The recipient defaults to Coordinator;
Role hooks use `--recipient role` and their existing verified Role scope.
The caller does not start the daemon, create credentials or grant access from
project/version selectors. Source failure or an invalid Original prevents
starting stdout delivery; operating-system write/flush errors can follow a
partial write. See the [caller contract](../specs/rust_project_hook_context.md).

The [PROJECT hook contract](../specs/rust_project_hook_context.md) and
[managed Role contract](../specs/managed_role_context.md) describe the current
installation, recipients and runtime delivery boundaries. ReadMemory and
rendered aliases do not authorize forwarding or prove MCP tool availability.
SDK or controlled-process proofs do not qualify delivery by the installed host.
Native Kiro receiving the context and model perception remain unqualified.
Platform qualification is tracked separately in `tsk-6b5f` (Linux),
`tsk-9a3d` (Windows) and `tsk-c3c2` (macOS). A leaf uses its affected Rust
producers and controlled native consumers with real Graphit authorization and
Source delivery; it does not require a vendor executable, account, model or a
three-OS campaign. Record each proof boundary and executed OS. SDK/local caller
success alone does not prove that an installed vendor host delivered context,
and Linux execution does not qualify another operating system.

## Does every agent require approval?

No. The supported agents use different controls, and only some of them present a trust decision. Do not treat these controls as interchangeable:

- **workspace trust** decides whether project configuration may load at all;
- **hook-definition trust** approves a specific command or script;
- **enablement** turns an already discovered hook on or off;
- **reload** starts a process or conversation after the hook was installed or changed.

| Adapter | Manual approval? | When is user action required again? | How |
| --- | --- | --- | --- |
| Codex | **Yes, through Codex CLI.** Trust the current published Graphit hook definitions. | On first use for that user and machine, and whenever the generated hook definition changes. Project trust is a separate prerequisite. | In a terminal, enter the repository, start `codex`, run `/hooks`, and review the current Graphit definitions in `.codex/hooks.json` listed in the Codex section below. Verify delivery in a new task. |
| Claude Code | **Yes, for interactive workspace trust.** There is no separate Graphit-hook approval. | For a new untrusted folder or parent, or after the stored trust decision is changed. | Accept the workspace-trust dialog, then inspect `/hooks`. |
| Cursor Local | **No separate hook approval is documented.** | After the first sync or a hook change, only if Cursor has not reloaded it; also re-enable a hook that a user disabled. | Open the workspace and a new Composer conversation; inspect **Customize → Hooks** and restart Cursor if necessary. |
| Cursor Cloud | **No approval can add the missing boundary.** | Whenever true first-turn `sessionStart` delivery is required. | Use local/self-hosted Cursor, or configure Graphit MCP at the team/enterprise layer; Cloud does not run `sessionStart`. |
| Kiro | **No separate hook approval is documented.** | Only when a user disabled a hook, or after sync when a new session has not loaded it yet. | Enable all Graphit entries in **Agent Hooks**, then start a new IDE or CLI session. |
| Google Antigravity | **No separate hook approval is documented.** | Only when the hook was explicitly disabled, or a conversation predates the sync. | Ensure the `.agents/hooks.json` entry is enabled and start a new conversation. |
| OpenCode | **No separate hook approval is documented.** | After the first sync or whenever the plugin file changes. | Fully restart the CLI or desktop app so startup loading runs again. |
| Qwen Code | **No separate hook approval is documented.** | After the first sync or a `.qwen/settings.json` hook change. | Start a new Qwen Code session and inspect its loaded hooks. |
| Kimi Code | **No separate hook approval is documented.** | After the first sync or a global `~/.kimi-code/config.toml` hook change. | Restart Kimi Code; Graphit keeps the shared global hooks while any synchronized project references them. |

The Rust CLI publishes the selected adapter's Coordinator, MCP and hooks through `graphit coordinator install --binding <binding>`; `graphit hooks install --binding <binding>` publishes just the native hooks. Core project discovery walks upward from the supplied directory and recognizes the nearest valid `.graphit/project.json` (or the configured brand directory) or Git marker. An invalid identity fails closed; Git is not required when the project identity exists. A hook cannot grant trust to itself: when an agent requires approval for project code, each user must approve the checkout on that machine. This approval is intentionally stored by the host outside the repository.

Published Graphit commands resolve the branded executable through the host's
PATH. PROJECT hooks use `--project-dir .`, relative to the workspace; they do
not store this machine's checkout or executable paths. For a custom
installation, launch the host with the branded `GLOBAL_DIR` variable. Codex's
MCP entry forwards this variable by name with `env_vars`; other native hosts
must inherit it through their supported process environment. Literal `~` or
`$HOME` in JSON/TOML arguments is not shell expansion. A working terminal PATH
alone does not establish the environment visible to an already running host.

Hook activation and MCP availability are separate checks:

- the hook injects the current Graphit mandates, installed Hub rules, and memory bootstrap;
- the MCP configuration exposes the `graphit_*` tools;
- a subagent can receive the instructions but still have a restricted tool allowlist.

The Task mandate and lifecycle reminders instruct delivery agents to install and run
missing commands needed to verify quality, conformance and tests. The Task skill
describes recurring project validation. In this repository, `.graphit/project.json` declares `githooks`;
`graphit git install` registers the native dispatcher. See [Git hooks](git-hooks.md).
Pre-commit runs `git diff --cached --check`, `actionlint`, then `cargo xtask check`;
pre-push runs `cargo xtask gate`. No project lockfile supplies these commands.
A check explicitly requested by the user, or
an error the user identifies as caused by missing validation, is necessary for
the project. The agent may reassess other checks, but cannot count an unavailable
command as a passed check. The lifecycle hook supplies this instruction; the
agent performs and records the installation and validation. For a persisted
project check, the agent chooses one invocation that works unchanged on Windows,
macOS and Linux. It calls an executable owned by the project with a path relative
to the project root; every other executable is called by name through the system
`PATH`. Machine-dependent absolute paths are forbidden. A versioned project
runner can handle OS differences behind the same invocation. The agent installs
missing external tools, verifies `PATH` resolution and the exact command on
available platforms, and records any untested platform instead of claiming
portability from one OS.

After the matching native installation, follow the section for your adapter and verify delivery in a new session. Re-run the check after changing adapter, cloning to another path, changing a hook command, or changing a custom agent's tool permissions.

## Codex

**Approval.** Required. Codex must first trust the project layer, then it separately reviews every non-managed project hook. Approval is bound to the current hook-definition hash, not granted permanently to every future Graphit command.

**When.** Approve on first use when that user and machine have no stored decision. Review again whenever native installation or a Graphit update changes the generated hook definition; Codex skips the changed definition until it is trusted.

**Action.** Approval must be performed through **Codex CLI**:

1. Open a terminal and enter the repository directory.
2. Start Codex CLI with `codex`.
3. Run `/hooks` inside the CLI.
4. Review the four Graphit hooks published for Codex in `.codex/hooks.json`: `SessionStart`, `UserPromptSubmit`, `Stop`, and `SessionEnd`. Trust their current definitions. The first two supply authorized project context; `Stop` and `SessionEnd` request a quiet, Configure-authorized project rescan, without completing Tasks or releasing claims.
5. In a new task, verify that `SessionStart` delivers context with the approved definition. Approval does not refresh an existing MCP connection.

Do not use `--dangerously-bypass-hook-trust` for routine interactive use. The desktop app can consume the synchronized project configuration, but the approval procedure documented here is the Codex CLI `/hooks` flow.

**Verify.** `/hooks` must show these four published Graphit hooks as enabled and trusted. The current Codex installer does not publish the legacy `PostToolUse`, `SubagentStart`, or `SubagentStop` callbacks. Check `graphit_*` MCP tool availability separately; hook approval alone does not prove a connection. For a custom subagent, do not remove Graphit MCP tools from its tool allowlist; otherwise the subagent intentionally uses its native tools. Other adapters publish their own event sets, as described below.

See the [Codex hooks documentation](https://learn.chatgpt.com/docs/hooks).

## Claude Code

**Approval.** Required for workspace trust in an interactive session, but not separately for each Graphit hook. Interactive Claude sessions withhold every settings-file hook until the repository or a parent is trusted.

**When.** Approve the first time Claude Code opens a folder not covered by an existing trusted parent, and repeat only after moving to another untrusted path or changing/revoking that decision. Non-interactive `-p` and SDK sessions do not display this dialog and treat the folder as trusted, so review repository hooks before running unattended code.

**Action.** Accept Claude Code's workspace-trust dialog for the repository or a trusted parent. No separate Graphit registration is required after trust is granted.

**Verify.** Run `/hooks` and confirm that the current Graphit installer publishes `SessionStart`, `UserPromptSubmit`, `Stop`, and `SessionEnd` in Project Settings in `.claude/settings.json`. Verify context in a new session; `Stop` and `SessionEnd` request the authorized project rescan. The installer does not publish a separate `SubagentStart` callback. Custom agents must not exclude Graphit through `tools` or `disallowedTools` if they are expected to use its MCP tools.

See the [Claude Code hooks documentation](https://code.claude.com/docs/en/hooks#workspace-trust).

## Cursor

**Approval.** No separate hook-specific approval is documented for local Cursor project hooks.

**When.** After the first sync or a change to `.cursor/hooks.json`, start a new conversation. Restart Cursor only if it did not reload the synchronized hook; re-enable the entry if a user disabled it.

**Action.** Open the repository as the workspace. If the Graphit entry was disabled, open **Customize → Hooks** and enable it. Start a new Composer conversation; if the synchronized hook still does not appear, fully restart Cursor and try a new conversation again. Cursor watches `.cursor/hooks.json`.

**Verify.** Open **Customize → Hooks** and the **Hooks** output channel. Confirm that the project `sessionStart` and local `sessionEnd` hooks are loaded and that Graphit MCP tools are available. Local subagents inherit the parent's tools and Graphit augments `Task` creation with its subagent protocol. Cursor's `beforeSubmitPrompt` cannot add context, so it is not presented as a reinjection boundary.

**Cloud limitation.** Cursor Cloud does not run `sessionStart`, and local MCP configuration is not available in its VM. Project hooks still run at the cloud-supported boundaries, but they cannot inject Graphit at the true start of the first read-only turn. Configure Graphit MCP at the Cursor team/enterprise layer, or use a local/self-hosted agent when first-turn Graphit bootstrap is required.

See the [Cursor hooks documentation](https://prod.cursor.com/docs/hooks#cloud-agent-support).

## Kiro

**Configuration.** Select `kiro-v3` during setup. `graphit coordinator install --binding kiro-v3`
publishes the current Markdown agent profile, MCP configuration and inline
`SessionStart`/`UserPromptSubmit` hooks. Hook definitions use version `v1`,
PascalCase triggers and the native action schema. To attach context hooks to an
existing Markdown profile, use `graphit hooks install --binding kiro-v3 --profile
coordinator.md`. Rust qualification is tracked in the
[native binding contract](../development/native-host-bindings.md).

**Approval.** Current V3 documentation requires trust for project agents. Hook enablement and agent trust are separate from tool permissions and server authorization. The native probe's generic trust banner does not establish a trust request. Avoid changing personal trust or tool policies to make a validation pass.

**When.** Start a new agent session after publishing or changing its profile.
Re-enable a disabled hook only when it is part of the intended configuration.

**Action.** Use the installed profile returned by Graphit and select V3 in the
native CLI. Complete any native project-agent trust step required by the host.

**Verify.** Inspect the published Markdown profile for the two context hooks and
the intended MCP servers and tool permissions. Start a fresh session and verify
context delivery and authorized MCP access. Profile bytes alone do not prove
that the installed host executed either hook.

See [Kiro hooks](https://kiro.dev/docs/hooks/) and [hook management](https://kiro.dev/docs/hooks/management/).

## Google Antigravity

**Approval.** No separate registration or trust prompt is documented for workspace hooks. The project hook in `.agents/hooks.json` defaults to enabled unless its `enabled` field is explicitly set to `false`.

**When.** User action is needed only after an explicit disablement, or when the current conversation started before the synchronized hook was available.

**Action.** Open the repository as an Antigravity workspace. If the Graphit entry in `.agents/hooks.json` contains `"enabled": false`, change it to `true` or remove the field so the default applies. Then start a new conversation.

**Verify.** Confirm that `.agents/hooks.json` contains the enabled `graphit-memory-session-start` entry; in Antigravity CLI, run `/hooks` to inspect loaded hooks. Start a new conversation. Graphit's `PreInvocation` hook injects the bootstrap before the first model call and only the compact invariant on later calls. `PostInvocation` carries the task checkpoint. Antigravity's `PostToolUse` is intentionally absent because its output contract accepts only `{}` and cannot inject the checkpoint. A static custom agent must retain the project MCP server/tool configuration if it is expected to use Graphit.

See the [Antigravity hooks documentation](https://antigravity.google/docs/hooks) and [CLI hook management](https://antigravity.google/docs/cli/plugins/#managing-hooks).

## OpenCode 2

**Approval.** Only OpenCode 2 is supported by the Rust integration. No separate hook trust or registration step is documented. OpenCode 2 automatically loads JavaScript and TypeScript files under `.opencode/plugins/` at startup.

**When.** Reload after the first sync and whenever the generated Graphit plugin file changes.

**Action.** Fully restart the OpenCode CLI or desktop app so startup loading runs again.

**Verify.** Start OpenCode from the repository and confirm that `graphit_*` tools from `opencode.json` are available. The V2 plugin registers `context` and `compaction`, adding a text SystemPart to the native `system` array. A foreign compaction result makes that boundary unavailable. Optional `session.idle` and `session.deleted` operations run only for this project location; interruption/deletion and cleanup cancel pending calls. If the plugin does not load, start with `opencode --log-level DEBUG` and inspect the newest log under `~/.local/share/opencode/log/`. Custom agent permissions must not deny Graphit MCP tools when Graphit-first behavior is required.

See [OpenCode 2 plugins](https://dev.opencode.ai/v2/docs/build/plugins/) and the [V2 migration guide](https://opencode.ai/v2/docs/migrate-v1/). The generated plugin exports the native `{id, setup}` object without a dependency installation step.

## Qwen Code

**Approval.** No separate project-hook approval is documented.

**When and action.** After `graphit coordinator install --binding qwen` (or `graphit hooks install --binding qwen` when only hooks are needed), start a new `qwen` session from the project.
Graphit reconciles lifecycle entries in `.qwen/settings.json` without replacing user hooks or settings.

**Verify.** Confirm that the current Qwen installer publishes `SessionStart`, `UserPromptSubmit`,
`Stop`, and `SessionEnd`, with one owned Graphit command per event. Context events use the native
`--engine qwen --event <event>` invocation; `Stop` and `SessionEnd` request the authorized project
rescan without the legacy `--sync` flag. When the Coordinator was installed, inspect the same file's
`mcpServers` object for the Graphit server independently of hook delivery.

## Kimi Code

**Configuration.** Use `graphit coordinator install --binding kimi` or `graphit hooks install --binding kimi`. The current native data root is `$KIMI_CODE_HOME`, defaulting to `~/.kimi-code`; the CLI captures that selection once. Graphit publishes one global `config.toml` with one owned tuple per event and global MCP in `mcp.json`, using the shared SDK ownership and per-project references. Invocations resolve the native cwd without a fixed project/version.

**Verify.** The default UserPromptSubmit command supplies current authorized PROJECT context as text. Stop and SessionEnd dispatch existing authorized rescan and do not read Memory, Task or USER. No PostToolUse reminder, parallel runtime registry or receipt of model perception is created. Do not require an installed Kimi executable, account or plan for adapter tests.

A project reference is not authority. Source authenticates the actual caller and captures the selected project/version; native session IDs cannot select a Graphit task session. Removing a reference must preserve shared configuration while another project still uses it. Use `coordinator remove --binding kimi` for the product artifacts or `hooks remove --binding kimi` for native hook/MCP ownership. The shared configuration is removed only after the last project reference.

## Durable session continuity

After activation, ask the agent to work on a concrete request. With Task enabled, it must first look for a related active Task Session and read its description, checkpoints and linked tasks. It resumes a matching session or creates a detailed one recording the request, scope, constraints and strategy. All agent-created tasks belong to that durable session. The native conversation ID is not a substitute for Graphit's session ID.

At meaningful progress, expect the coordinator to record a session checkpoint with evidence, problems, decisions, strategy and the exact next step. Add a requirement or change direction: the current session description/strategy and affected tasks must be revised before the changed work begins. An ordinary next prompt or compaction must reuse the durable session rather than create a duplicate.

Delegate one task with the session ID and Task ID. The worker should read both and claim only its own task. Stopping the worker must not release or close the coordinator's session. Before reporting a delivered request, the coordinator must explicitly complete the session after linked tasks are terminal. Stopping a turn or closing the host conversation is not that completion: unfinished work remains open and recoverable. See [Task sessions](task-sessions.md) for the tool and CLI workflow.

The delegation boundary is intentionally narrow. Do not create a subagent unless the user or an applicable mandate, `AGENTS.md`, loaded skill, or higher-priority instruction explicitly assigns bounded work to a delegated role. That instruction is itself authorization and an obligation, but only for that role and work. Graphit's mandate assigns recall to the scout, impact review to the tracker, and transcription of decided content to the scribe; a performer already acting in that role works directly instead of delegating recursively. Without an applicable explicit instruction, no subagent is created. If the host cannot run the assigned role separately, read its installed role document and perform only that bounded role in the coordinator.

A delegated agent delivers its complete result with evidence and may finish its turn. It does not need to keep a turn running in a waiting loop. The coordinator may later send the same delegate another instruction and continue from its context through the host's follow-up or resume mechanism when supported; this reuse is optional, not a requirement to keep the delegate alive, reuse it, or dismiss it explicitly. For example, a host exposing `followup_task` may restart an idle delegate, whereas `send_message` may only deliver a message. Inspect the host tool contract instead of assuming these operations are interchangeable. Availability and resource lifetime depend on that host, but the guidance must not prevent a later supported follow-up. None of these host operations completes or releases another agent's Graphit Task or coordination Session; acceptance and session closure remain the coordinator's responsibility.

When the host cannot provide an identity matching MCP ownership, explicitly checkpoint and release before stopping. Automatic hooks cannot guess the right owner. OpenCode forwards native identity where its events provide it. Hook presence alone does not prove correlation with a proxy-generated fallback identity.

## End-to-end check

In a fresh main-agent session, ask the agent to summarize the `Graphit session bootstrap` from its current instructions and list its `graphit_*` tools. Confirm that project work triggers focused non-mandatory Memory recall and direct retrieval of an assigned task (or focused Task search), followed by selected authoritative source reads. Reuse recalled context for the same scope; load a module skill immediately before its first matching action. Then ask it to delegate the same check to a subagent. The first answer verifies hook delivery; the tool list verifies MCP visibility; the delegated answer verifies the child boundary and its tool policy. Cursor's `preToolUse(Task)` injection and Antigravity's delegated-prompt compensation must carry the complete protocol when no dedicated subagent context hook exists. On the next normal prompt, only the compact Graphit-first invariant should be added (Kimi retains its documented prompt bootstrap fallback); the searches must not be requested again merely because a recurring boundary ran. Cursor and Antigravity receive only their own compact gap compensation at their documented boundaries, and no other adapter should contain those instructions. Except for that Kimi fallback, mandatory memory, full mandates, rules, and the startup bootstrap must not be reinjected on every turn.

The maintained automated checks use the published native configuration, the real Graphit CLI and authorized Source with controlled host inputs and callbacks. They require no vendor installation, account, plan or model response. Validate the affected context channel and lifecycle, including cancellation, expiry and joins. A supported Stop/SessionEnd requests the existing Configure-authorized project rescan; accepted dispatch is not completed indexing. It never completes a Task or Session, releases claims, creates reminders or changes another owner. Progress checkpoints and completion remain explicit task operations.


If any one of the three fails, use the matching adapter section above and [Troubleshooting](troubleshooting.md). Do not treat the presence of generated files alone as proof that the host loaded them.

## Expected fallback

If Cursor or Antigravity lacks the documented lifecycle boundary but loads the adapter's other context-capable Graphit hook, that adapter-specific instruction makes the missing action a semantic obligation. For a child without a dedicated context event, the fallback carries the complete `SubagentProtocol`, so enabled Memory and Task retrieval are not reduced to a generic Graphit-first slogan. It does not change the common mandate or any unaffected adapter, is not native enforcement, and cannot manufacture the absent event. If the workspace is not trusted, hooks are disabled, no context-capable boundary runs, or a child agent cannot see Graphit MCP tools, the project has no delivery channel for compensation. The agent or subagent must continue with the native tools that its host exposes. This is a compatibility fallback, not proof that installation succeeded: use the adapter-specific verification above when Graphit was expected to be active.


## Rust shared-hook writer status

The [Rust shared-document contract](../specs/native_shared_documents.md) records
exact hook ownership, preserved timeout/disabled/foreign fields and Kimi
last-reference recovery. Kiro V3 profile hooks use the dedicated Markdown writer
and current schema. A lock and atomic
replacement alone do not protect an external edit made after the last read;
S selects coordinated Graphit writers with optimistic conflict detection and an
explicit residual window, without claiming CAS. Staging/effect proof and native
runtime remain delivery gates. These writers do not change personal trust
configuration.
