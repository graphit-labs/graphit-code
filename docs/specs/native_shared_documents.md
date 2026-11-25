---
title: Shared native host document updates
type: reference
status: implemented
updated: 2026-11-07
references:
  - target: {type: task, id: tsk-bc28}
    relation: supports
  - target: {type: task, id: tsk-aa20}
    relation: implements
  - target: {type: task, id: tsk-be6e}
    relation: uses
  - target: {type: task, id: tsk-427f}
    relation: derived_from
  - target: {type: task, id: tsk-c29b}
    relation: derived_from
  - target: {type: task, id: tsk-89e0}
    relation: implements
  - target: {type: task, id: tsk-d6e4}
    relation: uses
  - target: {type: task, id: tsk-5975}
    relation: uses
---

# Shared native host documents

`graphit_hub::native::mcp_config` builds typed native MCP entries.
`graphit_hub::native::shared_document` applies them to an ordinary provider document. The host supplies its parent
`&Path`, filename, the logical ArtifactStore owner, request and document limits. The writer
does not resolve personal home directories or select profiles implicitly.

## Native plans

`NativeServerSpec` represents a named stdio, HTTP or SSE server. Its
`plan(NativeBinding)` selects the client's actual keys and format; unsupported
transports or options return an error. `NativeServerSpec::graphit(name)`
launches `graphit mcp --stdio` through PATH. Arguments retain their boundaries,
and environment values are separate from command text. Stdio `env_vars` is a
typed list of names allowed through Codex's environment filter. Names are
validated with the same environment-name grammar as `env`; existing explicit
`env` values remain separate. A nonempty list on another binding returns
`UnsupportedOption`, rather than copying a Codex option into an unsupported
schema. This generic option does not reserve environment names for Graphit;
custom native server names remain valid. The builtin generator supplies only
its installation and transient Role/Live transport names, while launch-time
values remain in memory. The [Codex MCP contract](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
defines names as values read from the local host environment; no shell
interpolation is assumed.

`NativeMcpPlan::project_config_path()` supplies the canonical selected placement
to the App installer, avoiding a second path table there. It follows the native
binding and selects `opencode.jsonc` when that explicit plan format is requested.
Kimi returns no PROJECT placement: its selected Graphit integration uses the
captured global `mcp.json` described below.

Codex uses TOML `mcp_servers`; OpenCode 2 uses `mcp.servers`; other bindings use
`mcpServers`. OpenCode 2 local commands are arrays with `type: local`;
Claude and Cursor use `type: stdio`. Remote URL/header keys vary by client.
A plan writes only the launch fields and explicitly supplied environment or
header keys. Timeout, disabled/enabled, auto-approval and other personal settings
remain untouched. The native document codec preserves JSONC comments and
foreign literals, and TOML comments and native values.

Kiro has one current binding, `NativeBinding::KiroV3` (`kiro-v3`). Its agent
profile is Markdown with YAML frontmatter. The launcher version does not select
the binding. Profile-scoped hooks and standalone hook documents share typed
V3 tuples but use distinct codecs and owned-document operations.

## Applying changes

`SharedDocumentStore::open` receives an explicit state directory common to all
Graphit brands editing the provider document. Paths follow normal OS link and
volume resolution. `writer(parent: &Path, name, core, limits, request)` resolves
the destination and uses its path as the routing key for a document lock. The
ArtifactStore supplies only the logical owner and brand; no Core writer, physical
scope binding or ancestor identity is retained.

The lock serializes cooperating field edits. The writer reads the document,
parses it with the same request's cancellation/deadline, and compares the current
bytes immediately before atomic replacement. External editors do not participate
in this lock and may still save after that comparison. This is not a universal
filesystem compare-and-swap or a power-loss durability guarantee.

- `sync` creates absent managed entries or updates fields belonging to the same
  logical owner and brand.
- `remove` releases the selected binding and removes its fields only after the
  final reference, preserving personal extensions and other entries.
- `forget` changes registration only, even if native syntax is now invalid.
- A foreign entry with the same name is a conflict, including equal launch fields.
  Modified managed values and incompatible shared-binding updates also conflict.
- New environment/header fields cannot overwrite a foreign value.

The state serializes a destination `NativePath` and an `entries` array containing
logical ownership, managed values and reference sets. The resolved destination's
serialized path is hashed with SHA-256 for the state and lock filenames. It is
document routing data; filesystem inode or ancestor identities are not retained.
Managed values may contain environment/header credentials, so state is
written with the OS-private file helper and temporary in-memory byte buffers are
zeroized. State and secret fields must never enter logs, Task or Knowledge.
A private temporary file is synced and atomically persisted with its handle
closed for the Windows rename. Ordinary provider paths need no generic private
ancestor qualification.

## Kiro hook tuples

The same writer consumes the typed [Kiro hook plan](rust_kiro_hook_plans.md).
`sync_hook(plan, format)` and `remove_hook(plan, format)` manage standalone
JSON or JSONC; TOML fails before publication. `sync_kiro_profile_hook` and
`remove_kiro_profile_hook` instead edit the hook array in an existing Coordinator
Markdown profile. They validate its YAML metadata and preserve its literal body
and foreign metadata values; YAML presentation may normalize. `forget_hook(plan)`
removes only the private registration and leaves native bytes unchanged, including
malformed documents. Every operation checks the original request and current document bytes.
The renderer composes a new profile through `KiroProfileRuntime { mcp, hooks }`;
actual installation and native execution remain separate consumers.

Both placements use `hooks[]` with PascalCase triggers. Standalone documents
require `version: "v1"`; profile YAML contains the tuples directly. An existing
different standalone version, incompatible container, binding, format or registered
event is an error. MCP entries and their references keep their own operations.

Creation never adopts a foreign tuple: an existing hook name collides. The private
registration stores the logical identifier, owner, binding, format, array path
and critical fields. Missing matcher is recorded as absence. It does not store
an array index or the full provider document. Critical values required after a
restart use the existing private state protection and are never diagnostics.

Update/removal requires exactly one tuple matching all last-owned critical
fields; a changed, missing or duplicate tuple is a conflict. Reordering leaves
this identity intact. Update also rejects collision with the desired native
identity. It changes only critical fields and preserves the current timeout,
enabled flag, comments and extensions; a new input timeout does not overwrite
the user's current value. Creation uses the full generated tuple, including an
explicit initial timeout. Removing the owned tuple removes that entire object,
including its extensions, and retains foreign tuples and containers.

For standalone JSON/JSONC, the existing jsonc-parser AST compares selected values without converting
foreign nulls or numbers such as `1e1000` through JSON serialization. Its CST
edits only the selected object/array. A bounded isolated candidate is rendered,
reparsed and checked against the same request before it replaces the in-memory
document. Publication uses the byte comparison and atomic replacement below, with typed
MCP-or-hook registration.

## Other native command hooks

`native::hooks::plan_command_hook` accepts a binding and `CommandHookInput`
(`identifier`, native event, command, optional matcher and timeout), the existing
shared-document limits and the original request. Commands come from the App's
one typed invocation producer. The SDK validates a closed native event/schema
matrix; accepting an event does not make it a default installed hook.

Codex, Claude and Qwen group commands under `hooks[event][]` and a nested
`hooks[]` array. Cursor uses flat command arrays and numeric `version: 1`.
Antigravity uses a named block, with flat lifecycle handlers and grouped tool
handlers. The plan exposes its project configuration path and document format.
Kiro retains the separate typed operations above; OpenCode uses a plugin below.

`sync_command_hook(plan, project_reference)`, `remove_command_hook` and
`forget_command_hook` use the same `SharedDocumentWriter`. Project command hosts
require their PROJECT ArtifactStore and no global reference. The private entry
records binding, logical identifier, event, native path/group and last-owned
critical fields. Positions observed while holding the lock are never persisted.
The editor matches the handler and its native matcher group, preserves foreign
siblings and fields, and updates only critical command fields. User timeout and
disabled settings remain intact. Removing an action cannot remove a wrapper
with foreign fields or actions. Changed or duplicate identities conflict.

`NativeDocument::get` decodes native TOML arrays of tables, including `[[hooks]]`,
as `FieldValue::Array`. Each table uses the existing recursive field conversion
and original request context. Reading retains the native document and its
formatting for a subsequent render.

### Kimi global references

Kimi hooks use global `config.toml` under captured `KIMI_CODE_HOME`, defaulting to
`~/.kimi-code`; global MCP uses `mcp.json` in that same root. These are separate
documents. The SDK receives their configured parent and filename. The official [hook schema](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/hooks.html)
allows only `event`, `matcher`, `command`, `timeout` in each `[[hooks]]` rule.
The [MCP schema](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/mcp.html)
uses `mcpServers`.

Kimi command-hook operations require the stable Installation ArtifactStore and
an explicit `ProjectId` reference. The typed entry's reference set is protected
by the same document lock and saved as ordinary managed-field metadata.
Retries are idempotent; one project's removal retains the tuple while another
reference exists. Only the final removal removes the owned rule. Incompatible
commands while other references remain are conflicts. `forget_command_hook`
releases only the selected reference and never changes native bytes.
`command_hook_has_references(plan)` lets the caller retain the shared global MCP
entry until all relevant hook references end.

The caller generates one global dispatcher with no fixed project or project
version. At execution, official stdin cwd/workspace selection resolves the
nearest Graphit lock and passes through current authorization. A host session ID
is never a Graphit session ID. TOML edits use existing `toml_edit` native tables
or inline-table arrays, retaining foreign rules, comments and user timeout.

## OpenCode 2 plugin publication

The native CLI catalog contains Codex, Claude, Cursor, Qwen, OpenCode 2,
Kiro V3, Antigravity/AGY and Kimi. Gemini CLI and its `gemini-code` alias are
rejected. Gemini inference providers/models/API and Antigravity remain supported.
Only the OpenCode 2 contract is emitted; no V1 compatibility emitter or importer
is retained.

`native::opencode_plugin::render_opencode_plugin` receives typed
`std::process::Command` invocations named `context` and `compaction`, optional
idle/deleted invocations and explicit resource bounds. The existing limited
serde JSON writer validates program/argv before copying into
`.opencode/plugins/<identifier>.js`. No shell string is reparsed; no runtime
context, credential environment or ambient process cwd is persisted.

The file exports the public V2 object `{id, async setup(ctx)}` directly.
`Plugin.define` in the inspected 2.0.31 SDK is an identity helper, so generated
JavaScript needs no SDK import, npm dependency, loader or second package payload.
Setup registers `ctx.session.hook('context', ...)` and `'compaction'`; each
appends a complete `{type:'text', text}` SystemPart to the event's `system`
array, preserving messages, tools, options and existing parts. A foreign
compaction `result` makes this context boundary unavailable and remains
untouched. Title/generate hooks are separate boundaries, not implied coverage.

The hook resolves the session's actual `location.directory` through the public
SDK client with AbortSignal. A location outside this plugin instance is rejected;
`ctx.location` alone does not establish a session's project. Lookup and child
execution share a deadline. Node `execFile` receives bounded stdin/EOF, output
limit and cancellation. Failed, oversized, invalid UTF-8, cancelled or late
results cannot inject partial context. Errors retain causal code/signal without
copying commands, SDK payloads, stderr or private stdout. Missing executables
retain `ENOENT`; pipe errors settle before abort can replace their cause.

The V2 asynchronous subscription reads `data.sessionID`. Session deletion and
`session.execution.interrupted` cancel matching calls. Optional idle/deleted
operations require an event location matching this instance; another project's
event cannot run this project's commands. A returned cleanup function aborts
the subscription and cancels/awaits children through their actual `close`.
Cancellation sends `SIGKILL` before abort because AbortSignal alone may ignore
`execFile.killSignal`. The first stream failure survives cancellation errors from sibling lifecycle
commands; cleanup reports that original cause. Expected per-session cancellation
does not poison the subscription or prevent the subsequent deletion operation.
No private context is logged. Host IDs only correlate cancellation and never become Graphit identities.
No per-tool callback is installed by default.

See the [V2 plugin API](https://dev.opencode.ai/v2/docs/build/plugins/),
[migration contract](https://opencode.ai/v2/docs/migrate-v1/) and
[published 2.0.31 types](https://unpkg.com/@opencode/plugin@2.0.31/dist/promise/plugin.d.ts)
inspected on 2026-10-16. Controlled peers exercise this public contract; they do
not prove that a separately installed vendor host loaded the plugin.

`publish_opencode_plugin` and `remove_opencode_plugin` use the ordinary Core
generated-file writer and content hash checks. The package supplies the
canonical plugin and publication writes directly from its validated plan, using
only transient staging. Installed hashes detect user edits without a second copy. JavaScript is
an exclusive owned file, never a JSON/TOML shared document. A preexisting foreign
file is a collision even with identical bytes; unrelated plugins are retained.
The caller supplies the PROJECT scope and rechecks authorization around effects.
`PluginPublicationError::AfterPublication` retains the actual change and later
guard/cleanup error instead of reporting that no publication happened.

## Publication and partial effects

The shared writer validates the prospective registration's encoded size before
changing provider bytes. It compares current document bytes, atomically replaces
the provider document, then saves registration. There is no persistent operation
ID, intent, receipt, phase journal, backup or replay API.

A registration-save error after provider replacement is an `io::Error` whose
source is `SharedDocumentPublicationError`; `effect()` returns `Applied`. The
provider document remains applied. A normal later operation reads actual bytes
and registration again. A newly created entry with missing registration remains
a collision rather than being adopted from equal content. The caller must retain
and expose the partial effect, instead of reporting rollback or automatic repair.

Core publishes exclusive generated Role and plugin files through its ordinary
content catalogue. Their post-publication authorization callbacks retain actual
changes in `AfterPublication` failures. Host authentication and runtime grants
remain caller responsibilities.

## Validation and native integration

Focused tests use temporary provider/state directories and the real document writer. They cover native schemas, Unicode arguments, comments and personal
options, install/update/no-op/remove, conflicts, Forget, Kiro references and
reopening current registration and ordinary explicit reruns. Formatting and strict Clippy apply to the
affected crates.

These tests validate library behavior. Actual client discovery, MCP connection,
hook execution, credentials and platform-specific runtime integration remain
the responsibility of the native host integration suites. Personal profiles
are never test fixtures. See
[Native host bindings](../development/native-host-bindings.md) for the selected
schemas and dated native observations.
