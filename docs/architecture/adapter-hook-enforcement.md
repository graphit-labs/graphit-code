---
title: Adapter hook enforcement
type: architecture
status: active
updated: 2026-10-17
tags: [adapters, hooks, mandates, skills, enforcement]
references:
  - target: {type: task, id: tsk-0c26}
    relation: supports
  - target: {type: task, id: tsk-aa20}
    relation: implements
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-97d9}
    relation: relates_to
  - target: {type: task, id: tsk-ced2}
    relation: relates_to
  - target: {type: task, id: tsk-9bd4}
    relation: relates_to
---

# Adapter hook enforcement

## Runtime authority and publication

The Rust policy classifies maintained tool effects and uses authenticated
capabilities for catalog emission and direct dispatch. Native allowlists
supplement that policy. A wildcard MCP selection includes mutations and cannot
establish a read-only grant. Role text and user overrides cannot expand the
authenticated capability.

Shared MCP/settings/hook documents require field or exact-tuple ownership;
a name, substring or marker does not authorize adopting the whole file.
User fields, disabled hooks and foreign entries survive publication and removal.
The [shared-document SDK](../specs/native_shared_documents.md) implements
native command handlers and Kimi's Installation-owned PROJECT reference set
through the existing Core operation and common lock. This serializes Graphit
writers and detects observed changes before replacement, while retaining the
external-after-comparison window. The lock and atomic replacement do not claim
universal CAS. OpenCode's plugin uses Core exclusive-file ownership and receipts.

The managed-role/runtime producer persists generic policy. Mandatory memory
and other live context are loaded at the qualified runtime boundary. Static
files contain no current `Standing context`, memory titles or memory snapshot.
The [Runtime context contract](../specs/rust_project_hook_context.md) records
selection, authority, capture, cancellation and qualified consumers.

## Native bindings and OpenCode 2

The native catalog has eight bindings: Codex, Claude, Cursor, Qwen, OpenCode 2,
Kiro V3, Antigravity/AGY and Kimi. Kiro uses Markdown agents, YAML metadata and
PascalCase inline hooks. Its typed profile writer preserves the literal policy
body. Other command hosts use their selected JSON/TOML schemas; OpenCode uses
plugin callbacks. The [native binding contract](../development/native-host-bindings.md)
records the supported schemas, event outputs and actual isolated observations.
An enum entry or successful parser does not establish native event delivery.

The OpenCode file exports the native `{id, async setup(ctx)}` object. Setup
registers `context` and `compaction` session hooks; only text SystemParts are
appended to `event.system`. Existing messages/options/tools remain intact.
A foreign compaction `result` stays intact and makes Graphit context delivery
unavailable. Title/generate and per-tool callbacks are not inferred coverage.

The public session client resolves the actual session location before execution;
the plugin instance location is insufficient for cross-project subscriptions.
The lookup and child share a deadline and cancellation. OpenCode 2 stream events use
`data.sessionID` only for correlation: deletion/interruption cancels the
matching call, operational idle/deleted commands require the matching location,
and returned cleanup aborts the subscription and awaits child close. Full output
is applied only on success. Errors preserve causal codes without private output.
Core remains the exclusive-file publisher and recovery authority.

MCP configuration is `mcp.servers.<name>`. Agent `permissions` are ordered
`{action,resource,effect}` rules, with broad denial followed by selected grants.
Native actions use `shell`, `edit` and MCP `server_tool`; edit/write/patch
must be representable as a complete mutation family. Existing Root readonly
admission remains intact. These declarations do not replace server authorization. See the [shared-document contract](../specs/native_shared_documents.md).

## Hooks, instructions and runtime context

Hooks act on concrete events and authenticated inputs. Mandates route enabled
modules; skills teach decisions when their domain becomes relevant. Published
tool schemas remain the argument reference. A hook cannot decide which evidence
answers a question or whether a semantic work unit is complete.

The hidden `_session-hook` command resolves the active PROJECT from the host's
runtime input and nearest Graphit lockfile. OpenCode resolves the actual session
location before starting the child. Project identity and version follow the
captured checkout, not an agent file's publication path. Generated instructions
and shared records retain logical identity and repository-relative references;
each host resolves its own runtime `project_dir`.

The authenticated Source consumer captures the selected PROJECT/configuration, Original request, deadline and cancellation. At a context-delivery boundary it uses current admitted sources and the selected rendering format. Bootstrap and `CoreInvariant` remain distinct formats; the invariant retains the compact Graphit-first router and persistence boundary. Those observations do not imply that every native event delivers a bootstrap.

With Task enabled, bootstrap tells the agent to read the assigned Task or locate
relevant work, read its parent and prerequisites, and resume the matching durable
session. The coordinator records the request and strategy before claiming it.
A native conversation ID cannot supply that description or substitute for a
Graphit session. Memory and Task follow their own module switches.

Recall remains semantic during exploration, implementation, debugging and
review. Reuse sufficient context; when a new question exceeds it, select Memory
facts/lessons, Task decisions or Knowledge contracts and read the relevant
sources. Known IDs bypass search. Neighboring checkouts returned by cluster
discovery remain Graphit-managed and use their returned target for module reads.
Unavailable Graphit MCP tools permit native tools; the Graphit CLI never
substitutes for MCP.

The resident preamble carries the persistence boundary for every enabled module
and delegated role. Shared records contain only non-sensitive, project-inherent
content needed for confirmed contracts, decisions, evidence, operations or
technical continuity. Personal/sensitive data, secrets, credentials, security-risk
material and transient feelings or speculation are not persisted there.
Durable user-only preferences belong only in private user memory. Necessary
technical hypotheses remain labelled and are not promoted to facts.

### Task state, checkpoints, and finalization

A semantic checkpoint follows the smallest independently reportable work unit.
The responsible agent explicitly calls `graphit_task_progress` with its result,
evidence and exact next step; the coordinator explicitly calls
`graphit_task_session_checkpoint` with evidence, problems, decisions/rationale,
strategy and next action. These calls use the real owner, fence and lease
required by their Task/session producers. A tool invocation, file write or MCP
read is not itself a checkpoint, and no hook can infer that semantic boundary.
Task state is not recorded in Markdown task logs.

Before delivery, the agent checks implementation and documentation in both
directions and records the actual inspected targets and evidence. Added scope
or changed direction requires the coordinator to revise the current session
and affected Tasks before execution. Completion, cancellation, release and
recovery retain their explicit Task/session contracts and authority. Stop,
SessionEnd, silence or lease expiry cannot reconstruct a missing handoff or
prove delivery. See [Task sessions](../guides/task-sessions.md).

The current Rust installation plan consists of supported context events and
supported Stop/SessionEnd boundaries. It does not install post-tool or
specification-task checkpoint callbacks. An event such as `PostToolUse` in a
codec enum does not prove installation, native delivery or semantic checkpoint
execution.

For a supported Stop/SessionEnd with Hooks enabled, the actual path is:

1. `_session-hook` enters the [Graphit CLI](../../crates/graphit-app/src/session_hook_entry.rs)
   and the authenticated [Source hook](../../crates/graphit-app/src/source_server/project_hook.rs).
   Source retains the captured PROJECT/version and Original, authenticates the
   actual Runtime/Local caller, revalidates the physical capture and requires
   `Configure` before requesting rescan.
2. [RoleToolServer::request_project_rescan](../../crates/graphit-app/src/role_tool_server.rs)
   selects the retained owner by exact PROJECT ID, checkout and version.
   A missing or stopped owner is an error; Hooks disabled produces no hint.
3. The existing [supervisor](../../crates/graphit-daemon/src/supervisor.rs)
   accepts a `Notify` hint, runs its callback serially with producer work and
   retains its real job through shutdown/join.

The response is native Quiet before Memory, Task or USER corpus admission and
without Task/session lifecycle work. Success means the existing owner accepted
a rescan hint, not that indexing completed. Pending hints coalesce; there is no
one-execution-per-turn guarantee. The hook starts no `graphit sync` child,
watcher or new scheduler and does not wait for indexing.

Rust final hooks do not reconcile Task CAS projections, expire or renew leases,
release or adopt claims, force takeover, reopen completion or turn Stop into a
handoff. A host conversation/session ID is correlation data, not a Graphit
owner/fence/lease. Existing Task tools remain responsible for their real
lifecycle contracts; quiet rescan supplies none of that authority.

Lifecycle coverage is maintained in `stop_and_session_end_dispatch_exact_owned_rescans_with_configure_authority` and `explicit_rescan_coalesces_during_jobs_without_parallel_work`.

### Subagents and tool visibility

A delegated role requires three separate guarantees:

1. The isolated context receives its complete routing and role protocol.
2. The host exposes the admitted Graphit tools in the child's registry.
3. The instructions route their use when the next action matches a module.

Conversation inheritance or an installed file does not establish delivery to a
child. A prompt cannot create a missing MCP tool. Native trust, activation and
allowlists remain host-controlled; Graphit preserves user-owned restrictions.
Use the qualified binding and [activation instructions](../guides/agent_hook_activation.md)
for the actual host. A project cannot approve its own trust or grant itself
permissions before the host loads it.

## Delegated roles

A delegated performer receives the assigned session and task IDs. The protocol is scoped by role: it carries the routing invariant, the mandate rules for the modules that role actually uses, and five contracts. The coordinator owns acceptance, checkpoints, Task/session claims, revisions, completion and lifecycle. The performer reads and reports evidence, or transcribes only expressly decided content within its assigned role. If a write requires a coordinator claim, it returns a draft instead of acquiring that claim. It may finish its turn; finishing that turn does not complete the Graphit work, which remains the coordinator's decision. It releases only host resources it was asked to manage, never another agent's Graphit records. And each role states what it must return.

The delegate mandate routes reading assigned work and returning anchored evidence or a draft to the coordinator. Task progress and checkpoints remain with the coordinator; the delegate does not acquire claims or perform lifecycle mutations.

The default and its exception are explicit. An agent does not create subagents unless the user or an applicable mandate, `AGENTS.md`, loaded skill, or higher-priority instruction explicitly assigns bounded work to a delegated role. That instruction is itself both authorization and an obligation, but only for that role and work; it is not permission for unrelated delegation. The Graphit mandate makes three such assignments: recall goes to the scout, impact review to the tracker, and transcription of decided content to the scribe. A performer already acting in the assigned role does the bounded work directly instead of delegating recursively.

After a delegate answers, it may finish its turn. The coordinator may later send that same delegate another instruction through the host's follow-up or resume mechanism when the host supports that operation, continuing from its recorded evidence. This is a capability, not a lifecycle rule: no waiting loop, keep-alive, reuse, or explicit dismissal is required, and the guidance does not prevent a later supported follow-up. Availability and resource lifetime remain host-specific. Acceptance, checkpointing, Task/session claims, revisions, completion, and session lifecycle always remain with the coordinator.

The protocol has two readers: a subagent when the role is delegated, and the main agent performing the role itself where the host has no subagent. It is written for the first, and the second is served by an explicit exception clause at the end of the contracts. Where the host cannot run the named role separately, the coordinator reads that role's installed document and performs only that bounded role itself. Performing the role locally retains its bounded scope and coordinator lifecycle ownership.

### Managed role documents

Graphit maintains scout for recall, tracker for impact review and scribe for
transcribing decided Task/Memory content. They are published as native role
documents alongside module skills. Each host receives only its supported fields;
server authorization remains authoritative even where a native schema can
express a restriction.

The role document supplies generic routing and the bounded role contract. Live
context is admitted at runtime; no current memory snapshot is serialized into
that document. On hosts without a separate role runtime, the coordinator reads
the installed document and performs only the assigned role itself.

## Publication, reads and bounded output

The [compiled-skill publisher](../../crates/graphit-app/src/compiled_skills.rs)
renders the maintained bundles and references through current captured sources
and the existing Core writer. Publication preserves foreign files and reports
partial effects, pending cleanup and delivery failures. Removal and recovery
retain the writer's real ownership and receipts. See
[Global instructions](../development/native-global-instructions.md).

The [Root module-skill reader](../specs/rust_root_module_skill.md) returns the
entrypoint and advertised companion names, or one exact selected companion.
Omitted PROJECT selects neutral Global guidance; explicit PROJECT/version uses
its current admitted configuration and observed overrides. Role consumers use
their announced scoped tools and authenticated input contract. Reading guidance
does not grant publication, remote access or access to another PROJECT.

The [instruction renderer](../../crates/graphit-runtime/src/instructions.rs)
enforces current template, variable, output, fuel and recursion limits under
the original request's deadline and cancellation. Long examples and authoring
procedures remain in selectively loaded references. Tool schemas supply their
arguments; instruction text need not repeat a catalog. Qualification records
identify their actual sources, environment and executed checks; fixture or
Graphit-consumer success does not prove installed vendor/model behavior or
execution on another operating system.
