---
title: Rust OpenCode completion
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-e6ff}
    relation: implements
  - target: {type: task, id: tsk-be6e}
    relation: uses
  - target: {type: task, id: tsk-28ee}
    relation: derived_from
  - target: {type: task, id: tsk-9db8}
    relation: derived_from
---

# Rust OpenCode completion

Graphit supports OpenCode 2 for Knowledge Ask and Incoming Conversation. OpenCode 1 is removed; its earlier qualification receipts remain historical and do not qualify version 2. No session importer, compatibility adapter or fallback is provided.

## Stable native contract

The current [OpenCode 2 migration guide](https://opencode.ai/v2/docs/migrate-v1/) distinguishes the new plugin and server APIs. Graphit uses the documented [ACP command and protocol](https://opencode.ai/v2/docs/cli/acp/), through the existing Rust agent-client-protocol SDK. OpenCode 2 continues to use ACP protocol version 1; application and protocol versions are separate.

The captured executable receives only the literal argument `acp`, with captured cwd and environment. Version 1 listener flags are removed. The documented command starts its private server, exposes no ACP network port and exits when stdin closes. Graphit retains the actual process owner and observes EOF, process/group retirement and cleanup. Metadata or a completed SDK future alone does not prove termination.

## Native process ownership and platforms

The cab0 transport sources target Linux, macOS and Windows. Linux and macOS retain the existing Unix process group, TERM/grace/KILL and pipe retirement, including escalation after the leader exits. Windows uses the pinned process-wrap 10.0.1 Job Object through the small native child boundary shared with Kiro in `acp_process/windows_child.rs`. The protocol driver, byte/frame audit, Original and PROJECT admissions remain unchanged.

The caller registers the actual owner synchronously before its next await on
success or a native-spawn failure. Process's existing `spawn_owned` boundary
preserves its reason and birth phase. BeforeNative with no child is a confirmed
pre-spawn failure; NativeAttempted without a handle is unknown birth. A recovered
RAW child is retained and drained but its leader wait does not prove ownership
or termination of a Job tree. These unknown states retain custody and admission
and never become a clean join or an eligible answer.

On Windows the interruptible wait observes only the native leader. Termination
then controls the same owned Job, and one complete Job wait stays inside the
retained ACP task while stdout and stderr drain. Borrowing observer timeouts
cannot cancel or recreate that wait. A failed Job wait retains its owner and is
not repaired by a cached leader status. SDK stream drop and actual pipe EOF are
also required before release. Windows executes only a captured native executable,
with no implicit batch shell.

Compilation of the composed consumers and native host execution are separate
proofs. The cab0 platform receipt records their actual state; Windows and macOS
native execution remain unqualified by Linux results. Unix fixtures remain in
`acp_process::tests`; Windows-only cases remain in
`acp_process::windows::tests`. Both consume the Cargo-emitted Rust
`app-agent-fixture`, including a child of that same executable with inherited
pipes and process group or Job membership. Private PID/readiness markers and
release gates preserve duplex bytes, normal EOF, leader exit, cancellation,
deadline and service stop. The batch-rejection case still denies the literal
untrusted batch path before birth. No interpreter, vendor binary or account is
required. A selector that runs no cases on another OS is not platform evidence.

## Driver boundary

[The driver](../../crates/graphit-ai/src/cli_opencode.rs) validates ACP protocol 1 and observed agent name OpenCode with major version 2 before creating or loading a session. This is protocol selection, not binary attestation or authorization. Load additionally requires advertised loadSession. No SSE capability is required: this ACP driver passes no per-request MCP servers or additional directories. It advertises no filesystem, terminal or authentication capability and never performs automatic login.

`OpencodeRequest.model` is optional. With `None`, the driver preserves the native default or loaded session selection and sends no ACP model configuration request; model options need not be present. With `Some(model)`, the native option must have id/category model, select type and the requested literal provider/model among offered values. Set configuration must confirm the same value before prompting. An explicitly empty, missing, ambiguous or unconfirmed selection fails. Permission requests receive Cancelled and prevent an eligible answer, including requests from child sessions; no approval or trust bypass is introduced.

`OpencodeRequest.agent: Option<String>` is independent of the optional model.
For `Some(agent)`, the driver requires the exact published identifier among the
announced `mode` option's values, with id/category `mode`. It sends the existing
SDK's `session/set_config_option` request and requires an acknowledgement
confirming that value before prompting. Missing, ambiguous, unoffered, empty,
oversized or unconfirmed selections fail without a prompt; no default-agent
fallback replaces the requested Role. `None` sends no agent configuration request.
With both selections present, the driver validates the original Load model
binding before any agent configuration. Agent selection then precedes model
selection: its acknowledgement must offer the requested model, and the final
model acknowledgement must preserve the selected agent. The agent acknowledgement
is validated and discarded before the next await; the receipt retains no second
configuration response. With no model selected, the native model remains unchanged
even when an agent is selected.

The [official ACP config options](https://opencode.ai/v2/docs/cli/acp/#config-options)
expose visible primary agents through `mode`. A generated Role used here must be
primary-capable: the [agent modes contract](https://opencode.ai/v2/docs/agents/#modes)
allows `all` for both primary use and delegation. Publishing a body alone does not
select it. The Role renderer and managed caller own that publication/capture;
this driver selects only an actually announced value. Knowledge Ask and
Conversation keep `agent: None` and their existing explicit model selection.
Admission, Original and native process retirement remain the caller's duties.

[The wire classifier](../../crates/graphit-ai/src/cli_opencode_wire.rs) decodes updates after the caller's bounded JSON/frame audit and SDK envelope decoding. Only textual agent_message_chunk for the exact active session during prompting enters the candidate. Child updates marked with update._meta[`opencode/child-session`] retain native progress and metadata but never contribute to the principal answer. Child id, parentID and positive depth are validated; malformed metadata fails. Foreign outer session IDs fail. Thoughts, tools, unknown updates and native usage remain progress, without invented tokens, cost or attribution.

Answer eligibility requires EndTurn, new principal text, completed SDK transport and a valid Original. App delivery additionally requires current PROJECT admission, actual process success, both EOFs, cleanup and retained custody through join. Errors, bounds, cancellation, deadlines, permission requests and sink failures preserve the first cause and suppress the answer. Debug/Display expose categories and counts, not prompt, answer, session, model or native error payloads.

## Incoming Conversation OpenCode

The [Incoming consumer](../../crates/graphit-app/src/source_server/knowledge_conversation/opencode.rs) uses the existing private USER store and service-owned process custody. Trusted opt-in and current USER read/write plus PROJECT Knowledge admission are required per turn. Create observes the native session ID; continue and cold reopen load only the exact committed private ID and admitted absolute cwd, with no New fallback. Replayed history before Load acknowledgment is validated and excluded from new-turn progress, candidate and usage.

When a model is explicitly requested, the loaded model must match it, followed by explicit configuration acknowledgment. With no requested model, Load preserves the native session selection without configuring it. Commit and final delivery require current bindings, native success and actual cleanup/join. The writer remains held until termination, including abandoned requests, cancellation, deadlines, dropped waiters and Source shutdown. List/delete manipulate Graphit metadata; native clear remains Kiro-only.

## Qualification

Focused commands select the relevant modules:

```text
cargo xtask cargo test -p graphit-ai --test integration cli_opencode:: --locked --offline
cargo xtask cargo test -p graphit-ai --test integration knowledge_conversation::opencode:: --locked --offline
cargo xtask cargo test -p graphit-app --test integration knowledge_opencode_ask:: --locked --offline
cargo xtask cargo test -p graphit-app --test integration knowledge_conversation::opencode:: --locked --offline
```

Installation, native configuration and plugin rendering belong to the shared Hub OpenCode 2 integration. The existing Xtask/workspace guard handles failures; unrelated providers are not replayed for this change.
