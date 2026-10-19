---
title: Rust Kiro hook plans
type: reference
status: partial
updated: 2026-10-15
references:
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-01eb}
    relation: derived_from
  - target: {type: task, id: tsk-d6e4}
    relation: implements
  - target: {type: task, id: tsk-03bf}
    relation: derived_from
  - target: {type: task, id: tsk-e643}
    relation: uses
  - target: {type: task, id: tsk-289f}
    relation: uses
  - target: {type: task, id: tsk-8c6f}
    relation: relates_to
  - target: {type: task, id: tsk-89e0}
    relation: uses
  - target: {type: task, id: tsk-1c5c}
    relation: relates_to
  - target: {type: task, id: tsk-d0fb}
    relation: derived_from
  - target: {type: task, id: tsk-356b}
    relation: implements
---

# Rust Kiro hook plans

`graphit_hub::native::kiro_hooks::plan_kiro_hook` produces owned hook data for an
`NativeBinding::KiroV3` (`kiro-v3`). It does not discover a host,
capture overrides, publish a file, execute a command or grant tool permissions.
The caller passes `KiroHookInput`, existing `SharedWriteLimits` and the original
borrowed `policy::RequestContext`.

The input contains a logical `identifier`, typed `trigger`, optional `matcher`,
`Command(&str)` or `AgentPrompt(&str)`, and optional `Duration` timeout. The
identifier uses the existing Graphit name validator: 1–64 ASCII lowercase
letters, digits or hyphens, without leading, trailing or repeated hyphens.
It becomes the native hook `name`.

## Native surfaces

| Binding | Placement | Generated tuple |
|---|---|---|
| Kiro V3 | Coordinator or Role Markdown profile `hooks[]` | `name`, PascalCase `trigger`, `action`, optional `matcher` and command `timeout` |

Runtime publishers compose `SessionStart` and `UserPromptSubmit` into one
profile-scoped array before publishing the complete Markdown profile. They do
not generate a parallel standalone copy.

The planner represents `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
`PostToolUse`, `Stop`, `SessionEnd`, `PostFileCreate`, `PostFileSave`,
`PostFileDelete`, `PreTaskExec`, `PostTaskExec` and `Manual`. Representation
does not establish native dispatch. See the official
[trigger reference](https://kiro.dev/docs/hooks/types/).

Actions use `{"type":"command","command":...}` or
`{"type":"agent","prompt":...}`. Only tool and file events accept a matcher.
An omitted matcher remains omitted; an explicitly empty matcher is preserved.
The planner does not rewrite tool IDs, regexes or selectors into permissions.

The SDK also exposes schema version `v1` for standalone native documents.
That capability does not change the Runtime placement above. No V2 importer,
converter or compatibility surface is provided.

Timeouts are explicit integer seconds representable by the native signed
integer codec. Zero disables the native command timeout. Omission does not
insert a default. Fractional or overflowing values fail; a timeout on an agent
action is rejected rather than silently ignored.

## Bodies, plan data and errors

The caller supplies an already rendered Markdown body or command string.
Quotes, dollar/percent signs, backticks, newlines, Unicode and literal template
tokens survive JSON encoding unchanged. The planner neither interprets Jinja
nor serializes shell argv. Generic policy comes from the central
[Markdown producers](../architecture/rust-agent-instruction-templates.md);
live Memory/session snapshots do not belong in their templates.

`KiroHookPlan` exposes `binding`, `identifier`, `trigger`, `native_trigger`,
borrowed `array_path`, `version`, borrowed `tuple`, `profile_value` and
`critical_fields`. Its fields are private. `array_path()` is `["hooks"]`;
`version()` returns `"v1"`. `profile_value()` returns native tuple data for
profile rendering, rather than the tagged SDK enum encoding.

Critical fields are `name`, `trigger`, `action` and `matcher`. Missing matcher
is a critical absence; timeout and extensions are not identity. The registered
trigger remains part of the owned tuple identity. These values do not prove
ownership of an existing tuple. The actual
[shared writer](native_shared_documents.md) owns publication and recovery.

The planner admits supplied text lengths against existing input/scalar byte
limits before scanning or copying the body and matcher. The identifier first
passes the existing name validator, which limits it to 64 bytes. The planner
then uses `NativeDocument` and `FieldValue` to validate its generated JSON,
including codec output/structure limits and the same cancellation/deadline.
Temporary validation documents are discarded. No resource reservation,
cumulative ledger or new parser is introduced.

Unsupported bindings, actions or matchers and invalid identifiers,
empty actions, NUL or invalid timeouts produce typed `KiroHookError` values.
`Document(DocumentError)` preserves the codec/request cause. Display/Debug do
not echo bodies; plan Debug exposes binding, trigger and identifier length.

## Qualification boundary

The earlier focused Hub suite checks native schemas/events, literals, absence/units,
unsupported inputs, codec limits and original cancellation/deadline. It proves
plan generation and JSON encoding.

The canonical `policy_prompt → render_instruction → plan_kiro_hook` consumer
(`tsk-8c6f`, under `tsk-aa20`) is completed and qualified by five focused
library tests at its recorded Source. It preserves effective caller brands and aliases, literal bodies,
UTF-8 byte bounds and original cancellation. Its deadline case establishes
admission-time checking only; it does not prove mid-render interruption. See the
[canonical instruction templates](../architecture/rust-agent-instruction-templates.md).

The [shared writer](native_shared_documents.md), delivered in `tsk-89e0`,
provides `SharedDocumentWriter::sync_hook`, `remove_hook` and `forget_hook`
through the real Core writer, owned-tuple lifecycle and reopening recovery.
Its qualification covers 45 Hub tests and strict Clippy exit 0; these tests
validate library behavior.

The authenticated Rust [PROJECT hook caller](rust_project_hook_context.md)
and `_session-hook` CLI entrypoint are implemented under `tsk-1c5c`, with six
focused Linux Source HTTP/entry and CLI checks. The supported current forms are:

```text
graphit _session-hook --engine kiro --event SessionStart
graphit _session-hook --engine kiro --event UserPromptSubmit
```

Both commands select current Kiro V3. The entrypoint has no V2 or preview
selector. It uses the existing daemon's actual authentication and retained
PROJECT/version selection, with the recipient's Memory admission when required.
It preserves Original, cancellation and connection cleanup before plaintext
delivery. It neither installs hooks nor starts a daemon. A returned payload
does not prove that a native host or model received it.

Runtime installation composes hooks with MCP and roles. Explicit
`--recipient role` forwarding consumes the real verified Role transport;
Tracker remains Quiet without Memory admission. Native session-ID/Graphit
session and claim correlation, compact-only repeated prompts, post-tool
checkpoint/stop/session-end lifecycle and installed dispatch remain separate
obligations under Runtime/plugin lifecycle. Planner triggers and selectors
do not implement or authorize them.
