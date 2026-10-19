---
title: Native host bindings during the Rust rewrite
type: reference
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-cab0}
    relation: relates_to
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-01eb}
    relation: derived_from
  - target: {type: task, id: tsk-a2bb}
    relation: supports
  - target: {type: task, id: tsk-4b26}
    relation: derived_from
  - target: {type: task, id: tsk-84f3}
    relation: relates_to
  - target: {type: task, id: tsk-bc28}
    relation: relates_to
  - target: {type: task, id: tsk-aa20}
    relation: relates_to
  - target: {type: task, id: tsk-97d9}
    relation: relates_to
  - target: {type: task, id: tsk-9bd4}
    relation: relates_to
  - target: {type: task, id: tsk-ad05}
    relation: supports
---

# Native host bindings during the Rust rewrite

This research selects native schemas for the Rust producers. The MCP planner and
shared document writer are implemented as library APIs; actual native-client
integration is tracked separately. The fixture and observation packet (`evidence/native-host-bindings-2026-10-07.json`; historical local artifact unavailable)
contains generic synthetic policies, native configuration inputs and sanitized
observations. It contains no live bootstrap, mandatory-memory snapshot, account,
credential or personal configuration. Version observations are dated; they are
not invented minimum supported versions.

## MCP library implementation

`native::mcp_config::{NativeServerSpec, NativeServerTransport}` selects typed
stdio/HTTP/SSE fields per binding, rejecting unsupported options. Codex uses
TOML `mcp_servers`, OpenCode 2 uses `mcp.servers`, and other bindings use `mcpServers`.
The plan owns only explicit launch/environment/header fields; personal timeout,
disabled/enabled and approval settings are preserved. Current Kiro has one
`kiro-v3` binding; the native MCP entry remains `mcpServers`.

`native::shared_document` applies those plans with the existing Core Writer.
Focused Linux tests exercise real publication/reopening, comments, foreign
entries, personal overrides and independent native binding removal. The
[shared-document contract](../specs/native_shared_documents.md) describes this
API and its cooperative concurrency boundary. These library tests do not
substitute for client discovery or an actual MCP connection.

## Evidence boundaries

Keep these proofs separate: codec parse, native configuration load, native
discovery, event execution, model-visible context, tool execution/denial and OS.
An exit code, help screen, native permission declaration or crosscompile does
not prove the remaining boundaries. Current observations are Linux only.
The former aggregate checks in `tsk-6b5f`, `tsk-9a3d` and `tsk-c3c2` were retired;
current functional contracts remain in `tsk-be6e` and their productive platform
composition is tracked by `tsk-cab0`. Each OS retains its own execution evidence.
Acceptance uses the actual Rust producers and controlled consumers, without a
vendor executable, login, account, plan, remote catalogue or model prerequisite.
The following dated observations retain their historical proof limits; they
require no new installed-vendor campaign.

| Host / observed executable | Version | Actual isolated observation | Historical proof limits |
|---|---|---|---|
| Codex / `codex` | 0.160.1 | `debug prompt-input` discovered the fixture skill; typed `-c` overrides made `mcp list --json` report the fixture stdio server | Custom-role context, trusted project config, events and dispatch were not exercised |
| Claude Code / `claude` | 2.1.280 | Version/help | Native role discovery/context/events; `agents` manages background sessions and is not a definition validator |
| Cursor / `cursor-agent` | 2026.09.02-c22c1a3 | Version/help | Local CLI, IDE and cloud have separate lifecycle boundaries; these observations did not exercise them |
| Historical Gemini CLI / `gemini` (retired) | 0.42.0 | Version/help; untrusted fixture returned no skills | Owned-fixture `--skip-trust` required auth; a synthetic noncredential with network disabled timed out after 25 seconds. No native load pass |
| Historical OpenCode 1 / `opencode` (retired) | 1.18.23 | `--pure debug agent graphit-conformance` loaded the role, Unicode description, prompt, subagent mode and edit/bash denial declarations | Declaration load did not exercise execution denial. Other built-in tools remained enabled; the full selected catalog and server profile were not exercised |
| Kiro / `kiro-cli` | Initial 2.20.2; recaptured 2.28.0 | V2 validator reported invalid tool types **with exit 0**. Explicit ACP V2 loaded canonical JSON and executed `agentSpawn`; ACP V3 selected the Markdown fixture mode | Historical dual-engine receipt, superseded by current V3-only selection. Both returned protocol 1 and session creation; this does not identify the engine or prove model context/permissions. V3 SessionStart marker was absent |
| Qwen / `qwen` | 0.25.0 | Official npm package installed in isolated runtime; version/help | `hooks --help` returned no output; no role/context/event discovery was observed |
| Kimi / `kimi` | 2.1.1 | Official npm package; `doctor config` accepted a hook with timeout 30 and rejected timeout 0 | Configuration validation only; discovery/context/events and permission execution were not exercised |
| Antigravity / `agy` | 1.2.6 | Version/help; `agents --help` identifies agent listing | `agents` returned no fixture metadata; empty successful output did not establish discovery. CLI and IDE have separate proof limits |

Qwen was installed from `@qwen-code/qwen-code`. Current Kimi is
`@moonshot-ai/kimi-code`, not the archived Python CLI. It requires Node >=22.19;
Node 22.14 failed before startup. Official isolated Node 22.19 resolved startup;
the Linux archive SHA-256 was checked against the official release manifest.
See [Qwen installation](https://qwenlm.github.io/qwen-code-docs/en/users/quickstart/),
[Kimi installation](https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html),
[Node release](https://nodejs.org/en/blog/release/v22.19.0) and
[Antigravity installation](https://www.antigravity.google/docs/cli/install/).

## Role discovery and canonical emitted fields

Paths below are project-relative. User roots are resolved on the executing
host; never serialize a checkout root. The table selects current canonical
fields rather than upstream compatibility aliases. Markdown entries consist
of YAML frontmatter plus a generic UTF-8 policy body. TOML and JSON entries are
typed native documents. Skill aliases require matching folder/frontmatter
identity and keep referenced assets; copying bytes is insufficient.

| Host | Native role path | Required identity/body and selected controls | Project skill path |
|---|---|---|---|
| [Codex](https://learn.chatgpt.com/docs/agent-configuration/subagents) | `.codex/agents/<id>.toml` | `name`, `description`, `developer_instructions`; inherits parent sandbox/permissions/MCP; native `features.shell_tool = false` where selected. A profile cannot promise its own readonly sandbox | `.agents/skills/<id>/SKILL.md` |
| [Claude](https://code.claude.com/docs/en/sub-agents) | `.claude/agents/<id>.md` | `name`, `description`, body; explicit `tools` list, e.g. `Read`, `Grep`, `Glob`; optional `permissionMode` | `.claude/skills/<id>/SKILL.md` |
| [Cursor](https://prod.cursor.com/docs/subagents) | `.cursor/agents/<id>.md` | Filename identity; description/body, `readonly: true` when selected; `model: inherit` | `.cursor/skills/<id>/SKILL.md` |
| [OpenCode 2](https://opencode.ai/v2/docs/agents/) | `.opencode/agents/<id>.md` | Filename identity; `description`, body, `mode: subagent`; ordered `permissions` array of `{action, resource, effect}` | `.opencode/skills/<id>/SKILL.md` |
| [Kiro V3](https://kiro.dev/docs/custom-agents/) | `.kiro/agents/<id>.md` | YAML config and body prompt; explicit tags `tools: [read]`, `excludedTools`, `includeMcpJson`, `includePowers`, `permissions.rules` | `.kiro/skills/<id>/SKILL.md` |
| [Qwen](https://qwenlm.github.io/qwen-code-docs/en/users/features/sub-agents/) | `.qwen/agents/<id>.md` | `name`, description/body; explicit lowercase native tools; canonical `approvalMode: plan` | `.qwen/skills/<id>/SKILL.md` |
| [Kimi](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/agents.html) | `.kimi-code/agents/<id>.md` | `name`, description/body; `tools: [Read, Grep, Glob]`; explicit empty list means no tools | `.kimi-code/skills/<id>/SKILL.md` |
| [Antigravity](https://antigravity.google/docs/subagents?tab=cli) | `.agents/agents/<id>.md` | `name`, description/body; `subagent: true`, explicit tools such as `view_file`, `grep_search`; `commandExecutionPolicy: off` | `.agents/skills/<id>/SKILL.md` |

Current Rust Kiro support is solely `NativeBinding::KiroV3`, canonical `kiro-v3`,
with Markdown agents. The coordinated decision in `tsk-01eb` supersedes the
former V2/default and V3/preview contract. ACP uses explicit
`kiro-cli acp --agent-engine=v3`. The launcher version and ACP protocol number
do not select the engine. No V2 importer, personal settings migration or legacy
alias is supplied. Current profile, hook and adapter acceptance follows official
contracts and controlled Graphit integrations. Agent tests require no vendor
executable, login, account, plan or remote catalogue. The dated probe commands,
authentication requirements and flag statements below describe the historical
investigation only; they define no current test instruction or acceptance gate.

## MCP transport contracts

The Rust shared writer (`tsk-bc28`) must consume a typed transport and emit
only the selected native schema. Do not copy arbitrary Hub JSON fields. Exactly
one transport is selected; mixed executable/remote fields, empty executable or
endpoint, invalid field type, unsupported fields and out-of-range units are
Graphit validation errors before any write. These are Graphit admission rules;
they do not claim every native parser rejects the same inputs. Preserve foreign
fields and user overrides through the shared-document ownership contract
`tsk-ced2`; unreadable/invalid files are errors, not empty configurations.

| Host / project document | Stdio | HTTP / remote | Native timeout and absence semantics |
|---|---|---|---|
| [Codex](https://learn.chatgpt.com/docs/extend/mcp), `.codex/config.toml`, `mcp_servers` | `command`, `args`, `env`, `cwd` | `url`, `http_headers`, `bearer_token_env_var` | `startup_timeout_sec` default 10; `tool_timeout_sec` default 60; `enabled_tools` allowlist, then `disabled_tools` |
| [Claude](https://code.claude.com/docs/en/mcp), `.mcp.json`, `mcpServers` | `type: stdio`, `command`, `args`, `env` | Explicit `type: http`, `url`, `headers` | URL without type is invalid. Do not invent per-server timeout units: qualification of additional options remains blocked in MC |
| [Cursor](https://prod.cursor.com/docs/mcp), `.cursor/mcp.json`, `mcpServers` | `type: stdio`, `command`, `args`, `env`, optional `envFile` | `url`, `headers`; no remote `envFile` | Timeout/default and CLI-specific schema qualification remain blocked in MC; no borrowed Claude fields |
| [OpenCode 2](https://dev.opencode.ai/v2/docs/mcp-servers/), `opencode.json` or `opencode.jsonc`, `mcp.servers` | `type: local`, `command: [exe, ...args]`, `environment`, `cwd` | `type: remote`, `url`, `headers` | Optional `disabled` and structured native timeout fields are preserved when foreign; Graphit emits required launch fields only, without V1 `enabled` |
| [Kiro](https://kiro.dev/docs/custom-agents/configuration-reference/), `.kiro/settings/mcp.json`, `mcpServers` | `command`, `args`, `env`, `disabled` | Current agent schema documents `type: http`, `url`, `headers`, `oauth`, sibling `oauthScopes` | Agent schema: handshake `timeout` default 60000 ms, `requestTimeout` default 120000 ms. Shared-document CLI 3/IDE scope needs its own documented schema and controlled Graphit integration; historical CLI 2 validation does not prove it |
| [Qwen](https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/), `.qwen/settings.json`, `mcpServers` | `command`, `args`, `env`, `cwd` | `httpUrl` for HTTP; `url` for SSE; `headers` | `timeout` milliseconds, default 600000; `trust` default false; `excludeTools` wins over `includeTools` |
| [Kimi](https://moonshotai.github.io/kimi-code/en/customization/mcp), `.kimi-code/mcp.json`, `mcpServers` | `command`, `args`, `env`, `cwd` | `url` means HTTP by default; `transport: sse` explicitly selects legacy SSE; `headers`, `bearerTokenEnvVar` | `startupTimeoutMs` default 30000; per-call `toolTimeoutMs`; 1..2147483647 ms. Global/env defaults need MC qualification; no invented per-call default |
| [Antigravity](https://antigravity.google/docs/mcp), `.agents/mcp_config.json`, `mcpServers` | `command`, `args`, `env`, `cwd` | `serverUrl`, `headers`; `url`/`httpUrl` unsupported | `disabled`, `disabledTools`; timeout fields/defaults remain unavailable until documented/qualified |

Emit only the required Graphit options; absence means the verified native
default. No timeout is invented by the renderer. Credentials are referenced through
the future authenticated profile; fixtures have none. Per-role allowlists,
server selection, OAuth issuer/endpoint binding and direct-call denial belong
to PF/MC and remain prerequisites for permission qualification.

## Lifecycle and context output

This table records documented mappings and their unavailable boundaries. Hook
delivery requires functional HK/RT consumers with real authorization and lifecycle,
qualified independently on each OS. Controlled Graphit integrations require no
vendor executable or account access. Codec parse or hook-config validation alone
does not establish delivery. Fixtures return only bounded generic context.

| Host | Bootstrap/resume and recurring context | After mutation / completion | Compaction / delegation / end |
|---|---|---|---|
| [Codex](https://learn.chatgpt.com/docs/hooks) | `SessionStart`, `UserPromptSubmit`: JSON `hookSpecificOutput.additionalContext` where supported | `PostToolUse`, `Stop` | `PreCompact`/`PostCompact`, `SubagentStart`/`SubagentStop`, `SessionEnd`; exact event output qualification remains HK work |
| [Claude](https://code.claude.com/docs/en/hooks) | `SessionStart` including resume/compact sources, `UserPromptSubmit`: `hookSpecificOutput.additionalContext` | `PostToolUse`; `Stop` for sync/side effects | `SubagentStart` context; `SubagentStop` and `SessionEnd` side effects. Launch `SessionStart` precedes MCP availability, so use a command hook |
| [Cursor](https://prod.cursor.com/docs/hooks) | Local `sessionStart`: `additional_context`; `preToolUse`/`subagentStart` need event-specific qualified outputs | `postToolUse`, `stop`/`subagentStop` | `preCompact` exists; `beforeSubmitPrompt` supports continue/user_message, not arbitrary context. Cloud lacks first-turn `sessionStart`, IDE-lifetime `sessionEnd` and MCP hooks; self-hosted worker claims have separate session boundaries |
| [OpenCode 2](https://dev.opencode.ai/v2/docs/build/plugins/) | `ctx.session.hook("context", cb)` appends text SystemPart to `event.system` | Optional `session.idle` operational refresh; no default per-tool callback | `compaction` appends to `system` unless a foreign `result` overrides the boundary; subscription uses `data.sessionID`, deletion/interruption cancel, returned cleanup joins children |
| [Kiro V3](https://kiro.dev/docs/hooks/) | Current Markdown profile `hooks` array, PascalCase `SessionStart`/`UserPromptSubmit`, native command action; successful stdout is context | Other native triggers remain event-specific SDK capabilities | Documented V3 adapter and controlled Graphit integration proof; no vendor executable/login/account/model prerequisite. Compact/delegation/stop retain separate functional obligations |
| [Qwen](https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/) | `SessionStart`, `UserPromptSubmit`: event-specific `hookSpecificOutput.additionalContext` | `PostToolUse` additionalContext; `Stop` | `SubagentStart`/`SubagentStop`, `SessionEnd`; compact output must be qualified rather than copied from Claude |
| [Kimi](https://moonshotai.github.io/kimi-code/en/customization/hooks) | `UserPromptSubmit` returned **text** is appended to context; `SessionStart` startup/resume is observation-only | `PostToolUse` observation only; `Stop` can block/continue; sync emits no unsolicited continuation | `SubagentStart`/`SubagentStop`, `SessionEnd` observation; `PreCompact` output ignored. Runtime compensation at the next prompt boundary must be qualified |
| [Antigravity](https://antigravity.google/docs/hooks) | `PreInvocation`, `invocationNum == 0` bootstrap, later calls invariant: `injectSteps: [{ephemeralMessage: ...}]` | `PostInvocation` injectSteps; `PostToolUse` returns `{}` and cannot deliver unit context; `Stop` side effects | No separately documented session-end/compact/subagent-context event; qualify recurring PreInvocation compensation. Do not invent event names |

Hook configuration units differ: Kimi seconds (1..600, default 30); Antigravity seconds (default 30);
Cursor seconds with platform default; Kiro current seconds (default 60, zero
disables timeout). Codex command hooks use seconds: most default to 600, while SessionEnd and
Interrupt default to 1 and allow at most 3. Claude command hooks usually default
to 600 seconds, UserPromptSubmit to 30, and SessionEnd shares a 1.5-second
budget with documented bounded overrides. Qwen command hooks default to 60
seconds; values >=1000 have a native millisecond interpretation. Graphit must
validate the intended duration against that native rule before emission, without
adding a compatibility emitter. Qwen permits an explicit `shell` of bash or
powershell; execution and quoting still need actual OS qualification.
Codex `additionalContextLimit` defaults to approximately 2500 tokens;
zero removes the host threshold, but never removes Graphit's resource budget.

Kimi global `[[hooks]]` permits only `event`, `matcher`, `command`, `timeout`.
Unknown metadata fields fail native config validation. Its global config root
honors `KIMI_CODE_HOME`; shared project references and external writers belong
to S/MC. Never use a command substring as ownership. Hook failure can be
fail-open; context delivery and permission authority are separate contracts.

Matcher policy selects actual mutating host tools, excluding read and Graphit
bookkeeping operations. Matchers use the documented native tool identities:
Claude/Codex uppercase names, Qwen lowercase names, current Kiro V3 tool tags and
Cursor `Write|Delete|Shell` must each be qualified against actual tools. Kimi
matches prompt text for UserPromptSubmit, tool name for tool events and role
name for subagent events. Antigravity payload uses camelCase `workspacePaths`,
Codex/Claude/Kiro/Kimi use event-specific cwd fields, and Cursor uses
workspace roots. RT resolves candidates to the nearest Graphit lockfile;
`.git` is never the Graphit project boundary.

## Command-hook and plugin SDK publication

`native::hooks::{CommandHookInput, NativeHookPlan, plan_command_hook}` and
`SharedDocumentWriter::sync_command_hook/remove_command_hook/forget_command_hook`
implement the command-host document boundary. The App supplies native event names
from its Runtime codec and commands from its one invocation producer. A codec's
supported event list does not decide installation defaults: publish functional
bootstrap/turn/restore boundaries and authorized operational refresh only.
Per-tool/per-task reminders are not defaults.

| Host | Selected hook document | Native structure |
|---|---|---|
| Codex | `.codex/hooks.json` | `hooks[event][]` matcher group with nested command `hooks[]` |
| Claude | `.claude/settings.json` | Grouped commands under `hooks[event][]` |
| Cursor | `.cursor/hooks.json` | Numeric `version: 1`, flat `hooks[event][]` |
| Qwen | `.qwen/settings.json` | Grouped commands; native timeout interpretation retained |
| Antigravity | `.agents/hooks.json` | Named block; lifecycle handler arrays are flat |
| Kimi | Captured global `$KIMI_CODE_HOME/config.toml`, default `~/.kimi-code/config.toml` | `[[hooks]]` with exactly the four documented native fields |
| OpenCode | `.opencode/plugins/<identifier>.js` | Real plugin callbacks, no hooks JSON |
| Kiro V3 | Existing Coordinator Markdown profile | Current typed inline hooks, unchanged |

The editor preserves foreign configuration, nested handlers, user timeout and
disabled settings, and rejects ambiguous or changed owned identities. Kimi uses
one Installation owner with PROJECT references under the existing common lock;
the final reference controls removal. The global dispatcher and global MCP launch
do not fix the last installed project's path/version. The caller captures the
official home/override before the operation and recovery uses that destination.
Current Kimi project agents and skills use `.kimi-code`, not an alternate alias.
See the current [Kimi hooks](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/hooks.html)
and [MCP locations](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/mcp.html).

OpenCode 2 uses typed `context`/`compaction` program/argv from
`native_invocation`, bounded Node `execFile`, stdin EOF and a shared
lookup/execution deadline. The generated ESM exports `{id, setup}` directly,
without an SDK identity-helper import or npm package. Native hooks append text
SystemParts to `event.system`; a foreign compaction `result` remains intact
and returns unavailable context. The actual session location is checked before
execution. V2 `data.sessionID` deletion/interruption cancel matching calls;
returned cleanup aborts the subscription and awaits child close. Optional
lifecycle commands run only for the matching event location. Private runtime
output is never logged or stored. The exclusive JS file reuses Core publication,
foreign collisions, recovery and actual after-publication errors.

The current native CLI catalog has eight bindings; Gemini CLI aliases/config/
rendering are removed, while Gemini API/model providers and AGY remain.
Historical OpenCode 1/Gemini executable observations above are not current
Rust/OpenCode 2 qualification.

The [shared-document contract](../specs/native_shared_documents.md) specifies these SDK operations.

## Quoting, errors and producer gates

Keep executable and argv separate for Rust subprocesses and stdio MCP. Native
hook command fields are shell strings. HK must select the actual host shell
contract, use a mature serializer for that shell and test spaces, quotes,
Unicode, metacharacters and platform PATH resolution. POSIX quoting is not a
Windows proof; guessing `cmd.exe` versus PowerShell is blocked. No personal
absolute path, credential interpolation or shell-built JSON is permitted.

RC now implements the documented pure role/skill formats with bounded codecs
in `graphit_hub::native`; its acceptance and publication integration are separate.
See [the library contract](../specs/hub_collaboration.md#templates-and-native-host-customizations).
NativeBinding contains only current Kiro V3. The pure API does not select a
live harness. Current Kiro hooks require actual RC/HK composition before the
complete Markdown profile is published. Trusted
builtin catalogs are admitted; MCP catalogs await MC/PF authority. OpenCode
edit/write/patch must be granted together because the V2 edit action gates
that family; shell is the V2 command action, and read covers directory listing.
Permission rules deny broadly before explicit allows/asks, with exact native
MCP server_tool actions. The Root readonly guard remains enforced. Cursor strict no-shell is explicitly unsupported by its
qualified readonly field, which still permits non-mutating shell commands.
Unproved host behavior remains disabled in product capability approval. MC
must qualify the explicitly blocked shared-scope/default/role-selection fields;
HK must resolve exact outputs, mutation catalogs and shell contracts; PF must
establish real endpoint/authenticated capabilities; RT must demonstrate live
bootstrap, resume, compact and delegation behavior. Native gaps are necessary
delivery work and are not waived by completing this research packet.

## Current Rust installation identity

The [B library contract](../specs/hub_native_installations.md) selects eight
current bindings, including only `kiro-v3`. Its local
Agent keys/files/pins are independent; common Kiro skills have one family target.
Recovery retains the selected binding. RC-produced Derived Agent payloads are
required; raw role copying and generic Kiro Markdown hooks are unsupported.
Library wiring alone does not qualify runtime context, events or permissions.
MC/HK consumers require their affected functional Graphit integration using
documented contracts and controlled peers, without installed-vendor, account or
model prerequisites. Evidence applies only to the OS and consumers exercised;
other platforms do not impose a blanket gate on an independent delivery.

### Product hook and MCP publication

The Rust CLI selects a canonical binding explicitly: `coordinator install --binding <binding>` publishes supported Coordinator artifacts, workspace MCP and hooks; `hooks install --binding <binding>` installs the native hook consumer. Only Kiro requires an existing `--profile <name>.md`. The shared native SDK supplies provider paths and document formats. Tests read the published configuration, run the actual Graphit CLI against authorized Source and apply OpenCode callbacks, using controlled peers rather than vendor accounts or binaries.

| Host | Coordinator MCP profile contract | Workspace transport |
|---|---|---|
| Codex | Inherits parent MCP and permissions; no profile sandbox override | `.codex/config.toml` |
| Claude | Native inline `mcpServers` array and `mcp__server__*` tool pattern | `.mcp.json` |
| Cursor | Inherits local parent tools/MCP; no invented profile MCP field | `.cursor/mcp.json` |
| Qwen | Native `mcpServers` object and `mcp__server__*` tool pattern | `.qwen/settings.json` |
| Kiro V3 | Current inline MCP, `@server` and typed profile hooks | `.kiro/settings/mcp.json` |
| Kimi | Native `mcp__server__*` pattern; global transport | `$KIMI_CODE_HOME/mcp.json`, default `~/.kimi-code/mcp.json` |
| OpenCode | Native server tool permission pattern with ask policy | `opencode.json` and owned plugin `.opencode/plugins` |
| Antigravity | Customprofile MCP returns unsupported: object fields are not documented; no fabricated selector or inheritance claim | Documented `.agents/mcp_config.json` plus PreInvocation hook |

The renderer rejects `read_only` with a native MCP plan outside Kiro V3: those bindings do not supply a representable readonly MCP catalog. Kiro Live retains its qualified exact tool grants. The Coordinator publication path uses its existing full tool grants; a server wildcard is not a readonly catalog.

Kimi uses one global `$KIMI_CODE_HOME/config.toml` hook without fixed
project/version argv. The CLI captures its final parent once; SDK journals keep
that parent and per-project references. OpenCode 2 registers `context` and
`compaction` callbacks through `ctx.session.hook`, appending text SystemParts to
`event.system` while preserving foreign messages, tools and options. Optional
`session.idle` and `session.deleted` operations match the project location;
deletion/interruption and returned cleanup cancel pending calls, and cleanup
disposes registrations, aborts the subscription and awaits child close. Host
session IDs remain operational data.

Sources: [Claude inline MCP](https://code.claude.com/docs/en/sub-agents), [Qwen subagents](https://qwenlm.github.io/qwen-code-docs/en/users/features/sub-agents/), [Kimi agents](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/agents.html), [Kimi hooks](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/hooks.html), [Kimi MCP](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/mcp.html), [OpenCode 2 agents](https://opencode.ai/v2/docs/agents/), [Antigravity subagents](https://antigravity.google/docs/subagents?tab=cli) and [workspace MCP](https://antigravity.google/docs/mcp).

Native withdrawal uses `hooks remove --binding <binding>` for shared tuples or `coordinator remove --binding <binding>` for the Hub-owned profile and its hooks/MCP. A field editor rejects a complete Core-owned Kiro profile. Kimi drops the selected ProjectId reference and preserves its global MCP while SDK hook references remain. Supported Stop/SessionEnd defaults use quiet Configure-authorized rescan; Codex SessionEnd uses the documented maximum native timeout of 3s. No Task completion, claim release or authentication TTL change is attached to these events.
