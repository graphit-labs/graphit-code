---
title: Rust Kiro ACP completion
description: One SDK ACP prompt under the original execution context, with bounded consumer retention and real protocol receipts.
type: specification
references:
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-01eb}
    relation: derived_from
  - target: {type: task, id: tsk-6365}
    relation: supports
  - target: {type: task, id: tsk-658c}
    relation: corrected_by
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-2c72}
    relation: implements
  - target: {type: task, id: tsk-b2d4}
    relation: implements
  - target: {type: task, id: tsk-a5cf}
    relation: implements
  - target: {type: task, id: tsk-a2bb}
    relation: derived_from
  - target: {type: task, id: tsk-5640}
    relation: relates_to
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Rust Kiro ACP completion

`graphit-ai::cli_kiro::run_kiro` runs exactly one ACP prompt through a caller-supplied
`ConnectTo<Client>` transport. It reuses `agent-client-protocol =3.3.0` with `process`
and its stable schema rather than creating a JSON-RPC parser or a second process
supervisor. Production can supply `AcpAgent`; tests use the SDK's `Channel::duplex`
and typed `Agent` handlers. See [the model guide](../guides/ai_models.md).

## Caller and native protocol

The caller supplies the original `Arc<SnapshotContext>`, transport, explicit
`KiroEngine::V3`, `KiroSessionStart::New(NewSessionRequest)` or
`Load(LoadSessionRequest)`, `KiroPrompt`, positive
`KiroLimits` and borrowed `KiroSink`. Protocol version 1 does not identify the
engine. `KiroPrompt::new(text)` has no selection; its optional `KiroConfigSelection`
contains literal SDK `SessionConfigId`, `SessionConfigValueId` and optional `Meta`.
Transport launch, authentication, global model selection, native tool permissions,
MCP configuration and admission remain caller responsibilities. The consumer
forwards session cwd/MCP/meta and additional directories literally and never changes process-global cwd,
HOME or credentials. `AcpAgentConfig` in this pin has no per-child cwd setter.

The current application engine is `ConversationEngine::KiroV3` and its binding is
`NativeBinding::KiroV3` (`kiro-v3`). Native ACP launch is
`kiro-cli acp --agent-engine=v3`. Agent, model, effort and trust options are not
ACP launch arguments. Initial `modelId` travels in `_meta.kiro` on both New and Load;
`modeId` selects the Role, and later configuration uses the options advertised by
the native session. V2 and preview aliases are not accepted by the current contract.

This finite ACP operation calls the Snapshot's `required_deadline()` in its
existing preflight before connecting the transport or publishing Initialize.
A service Original without a deadline fails with the actual IO `InvalidInput`,
no driver result, session or cancellation enqueue, and zero transport connections.
Driver waits and sink delivery use that same finite Instant. None is not treated
as expiration, and the adapter does not add a timeout, TTL, permission or rebase.

The handler is registered before initialize/new-session/load/configure/prompt. For New, the
returned ID routes updates; notifications before its response or for another
session cannot reach this turn's sink or candidate. Load requires the actual
`InitializeResponse.agent_capabilities.load_session`; false or absent fails
`UnsupportedLoad` with no Load/New/Prompt/Cancel. Nonempty additional directories
in Load require the advertised session additional-directory capability, otherwise
`UnsupportedAdditionalDirectories`. The caller owns absolute cwd, session binding
to the same agent/engine/cwd, persistence and admission. No history store is read.
Session IDs, metadata and tool-call IDs are data, not authority.

Following [ACP session setup](https://agentclientprotocol.com/protocol/v1/session-setup),
Load installs the bounded supplied ID before sending its typed SDK request, so
matching updates delivered before `LoadSessionResponse` reach the serial sink.
`KiroDeliveryPhase::Loading`, `Configuring`, `Prompting` and `Deleting` label the consumer's operation stage;
they do not classify native replay or replace native `_meta`. Prompting starts
immediately before publishing the new prompt, after successful Load. Loading
text never enters the new candidate; history alone cannot satisfy `answer()`.
Configuring text is also excluded from the candidate.
There is no New fallback, retry, replay or resend after Load failure. This explicit
SDK operation does not implement application Conversation, session/resume without
history, session/list, session/close or clearing persisted history.

The [Kiro ACP contract](https://kiro.dev/docs/cli/acp/) and
[V3 migration contract](https://kiro.dev/docs/cli/v3/acp-migration/) define
the current launch and session/config options; the consumer
does not infer a headless `stream-json` envelope. Only the actual SDK
`PromptResponse` or error terminates this prompt, never an inferred turn-end
notification. Authenticated model availability and application model selection
require their own native evidence.

## Explicit native session deletion

`graphit-ai::cli_kiro::delete_kiro_session` accepts the same original
`Arc<SnapshotContext>`, caller transport, explicit engine, typed
`DeleteSessionRequest`, positive `KiroLimits`, borrowed sink and borrowed
`before_delete` check. It reuses the existing Client driver, QuietSdk boundary,
serial delivery and first-cause handling, with ACP 3.3.0/schema 1.11.0's stable
Delete API. It does not send New, Load, Select, Prompt or dummy text.

Actual Initialize must advertise `session_capabilities.delete`; unsupported
agents receive no Delete and there is no fallback. The caller's borrowed check
revalidates its real USER/PROJECT admissions after Initialize and immediately
before the Delete RPC under the same finite Original. Session updates, when
observed, reach the existing sink in `Deleting` phase and never become an answer.
The actual `DeleteSessionResponse` remains in `delete_response()` with Initialize,
capability, driver, notification and cancellation observations, including failure
of a subsequent Original check. SDK and IO errors retain their real private
causes; ordinary formatting does not expose session IDs or native payloads.
The App can project an actual SDK error's numeric code as `sdk_error_code`
without forwarding its raw message or data.

The [ACP session-delete contract](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/rfds/session-delete.mdx)
removes the selected session from `session/list`. Storage erasure and later Load
behavior are defined by the agent. This operation does not promise history wipe,
retry, rollback or native child cleanup. The caller owns private ID lookup,
authorization, child join and any local record removal; the
[App clear contract](rust_knowledge_conversation_app.md#explicit-native-kiro-clear)
describes that composition. An Incoming caller may return a finite cancellation
or deadline error before its owned job reaches terminal cleanup. No public
receipt is observed on that path; it does not fabricate a counter or ACK, and
the caller retains custody/writer through actual child wait.
The affected SDK suite passed 37 tests (five new and 32 affected), with zero
ignored, a 0.41 s harness and 4.814219791 s wall. AI strict checks passed in
20.681566652 s wall. The [App clear qualification](rust_knowledge_conversation_app.md#explicit-native-kiro-clear)
also passed: 51 distinct scoped cases overall, 13 new and zero ignored, including
store, App clear and existing Kiro/Codex continuity cases; affected App strict
checks passed. Repeated attempts and loop scenarios do not increase that count.
These Linux SDK/Broker/USER/PROJECT fixtures do not authenticate a commercial
CLI or qualify vendor session storage/listing behavior, Main, all CLIs or other OSes.

## Explicit session configuration

A requested selection uses only the typed `config_options` snapshot in the actual
New or Load response. Following [ACP configuration](https://agentclientprotocol.com/protocol/v1/session-config-options),
the config ID must appear exactly once, its kind must be Select, and the literal
value ID must occur in its flat or grouped options. Category is only a UX hint;
missing, unknown or Model category never controls correctness. The consumer does
not infer a method or model from engine/version, labels, IDs or metadata.

No offer fails `UnsupportedConfiguration`; missing ID, duplicate ID, unsupported
kind and unoffered value fail `ConfigurationNotFound`, `AmbiguousConfiguration`,
`UnsupportedConfigurationKind` and `ConfigurationValueNotOffered`, respectively.
These failures send no Set or Prompt. A successfully created/loaded session can
still use the existing common cancel path. With no selection, no Set is sent.
Boolean selection is unsupported and no Boolean client capability is advertised.
There is no legacy V2 `session/set_model`, default selection, retry or fallback.

After validation, Configuring starts immediately before constructing one typed
`SetSessionConfigOptionRequest` with the actual session ID and literal config/value/meta.
Its response is moved into `config_response()` before the post-response original
check. Prompt requires the returned full configuration list to contain exactly one
matching Select, with the requested current value also present in its returned
options. Otherwise `UnconfirmedConfiguration` preserves the native response and
sends no Prompt. This confirmation policy is a consumer check, not a fabricated
SDK error. Native SDK rejection remains the real private SDK cause.

The snapshot is not a latest-state cache, global model catalog or atomic model
binding. Concurrent `ConfigOptionUpdate` notifications still reach the sink intact;
the agent arbitrates Set and may change configuration during Prompt. Full returned
state, dependent changes and metadata remain inspectable. No global model factory,
application Conversation, persistence or model-availability policy is added here.

## Streaming, bounds and errors

Notifications are processed serially by the SDK dispatch loop. `KiroSink::push`
takes the stage enum and borrows the native `SessionNotification` and the same
original context. It must
poll cooperatively and leave no detached work when dropped. A pending delivery
holds subsequent dispatch, including configuration and prompt responses, until it resolves.
The sink must not wait for a request response from inside that serial callback.

Only Prompting `AgentMessageChunk` with `ContentBlock::Text` contributes to the candidate
text. Empty text is observed `Some("")`; absent text remains `None`. User, thought,
tool, media, plan and configuration events reach the sink without becoming text.
Unknown methods and structurally valid unknown update discriminators are ignored.
Known variants and required envelope fields use real serde errors. Classification
consumes the SDK's existing `Value`, without cloning or re-encoding it. SDK field
normalization is not a duplicate-field audit of raw wire JSON.

`prompt_text_bytes`, `response_text_bytes`, `notifications` and `id_bytes` must be
positive. Caps are checked before our additional retention/copies. `id_bytes`
bounds session IDs, requested config/value IDs and known message/tool-call ID strings; it is not a cap on
every string in opaque metadata. Observed notifications include ignored and
pre-session notifications admitted up to the cap; accepted counts increment
only after a sink delivery succeeds under the valid original. The candidate
may record text from an unaccepted event on failure and is explicitly diagnostic,
never a finalized answer. No transcript is retained by the consumer. Loading and Configuring
notifications consume the same notification/identifier limits; their text is
not retained and does not consume the candidate response-text cap. An oversized
supplied Load ID or requested config/value ID fails before transport polling or
any additional ID copy. Opaque metadata and native offer/response strings have no
new cap; an empty requested ID has no special consumer policy.

These are consumer limits. The pinned SDK has unbounded queues and allocates
incoming lines/decoded values before this callback. They do not establish a
hard SDK frame limit, preallocation limit or RSS bound. No token or RPC count is
invented; final token usage is unobserved in the stable feature set used here.
Typed native New, Load and Set responses are moved into the receipt without cloning.
Their config options/modes/meta have already been allocated by the SDK and are outside
the candidate-retention caps. New is retained only after the returned session ID cap
passes; retaining an offer does not itself choose a model.

The original is checked before admission/sends/decode/retention and polled during
requests and sink delivery for cancellation and its absolute deadline. On a
failed active turn the consumer enqueues the actual `CancelNotification` before
closing the same SDK driver. After constructing the Load response future, the consumer polls it once
synchronously without yielding. An immediately ready SDK error (including
preparation/publication failure) is preserved without session cancellation. A
pending request or successful Load becomes eligible for session cancellation;
this is observed state, not proof of frame transmission or publication/peer ACK.
Capability and pre-send admission failures do not cancel a preexisting session.
Once New/Load is active, configuration failure follows the same eligible cancel
path; constructing a Set request is not proof of publication or acknowledgement.
A received Load or Set response remains inspectable even if the original fails
after it is observed. A created ID admitted by the cap is preserved even
when the post-response original check fails; an oversized created ID is cancelled
without retaining it or sending a prompt. Consumer failure is stored separately: the main
closure returns successfully so the SDK performs its outgoing drain. This does
not convert that consumer failure into a successful answer. The stored
`connect_with` result still reports actual transport/handler/close failures.
Cancellation drops a pending sink future once and does not accept or replay it.

`KiroFailure` preserves the first actual context IO, sink IO, serde, allocation
or SDK cause for explicit inspection. Consumer `Debug`/`Display` expose only
static reasons, counts and presence/success flags, not prompt/session/model/meta/
stderr/credentials. Do not log the explicit cause or native receipt payloads.

The pinned SDK itself emits raw protocol tracing before callbacks and while
forwarding errors. A safe consumer formatter alone cannot protect those records.
The operation uses existing `tracing =0.1.44/std` with a local `NoSubscriber`
dispatcher for every poll and Drop of the owned SDK future, including synchronous
SDK construction, sends and cancellation inside that future. It leaves the
caller's global/default logger unchanged; logging outside the operation remains
available. No connection handle escapes this scope. The pinned SDK's standard
actors, child stderr and scoped tasks are polled by this connection future.
This boundary does not claim control over dependency threads or external tasks
created by a caller, nor over explicit logging of returned native data.

## Receipts and qualification boundary

`KiroReceipt` retains the actual created or explicitly supplied Load session ID,
typed native New, Load and full Set responses through `new_response()`,
`load_response()` and `config_response()`, and native prompt response when observed,
SDK driver result and optional cancel enqueue result. Queue acceptance is not
agent acknowledgement. It does not fabricate a `Cancelled` prompt response,
Process outcome, EOF or successful process-tree cleanup. When the caller supplies
SDK `AcpAgent`, that transport owns its process/group guards and close behavior.
The App supplies its concrete
[`KiroProcess`](../../crates/graphit-app/src/knowledge_kiro_process.rs) transport so
its existing ask owner retains the actual child, admission and conversation custody.
There is no second `ProcessHost` for the same child.

The App transport composes native process groups on Linux/macOS and the pinned
`process-wrap` Job Object on Windows, with explicit cwd/environment and bounded
bidirectional ACP pipes. Windows uses native EXE resolution, accepting the optional
`.exe` suffix as documented by [`std::process::Command`](https://doc.rust-lang.org/std/process/struct.Command.html#method.new),
and rejects explicit script/batch extensions without adding a shell.
Original cancellation, service stop and the original
finite deadline request termination; observing that request does not release
custody. The retained task drains stderr and observes native termination before
releasing admission. A Windows leader exit alone is not a completed Job: the
whole-Job wait remains owned through descendant completion. An initial wait or
signal error stays observable even when physical cleanup subsequently succeeds.
Failed native setup retains its recovered child; unknown birth or tree completion
stays unconfirmed rather than becoming a successful cleanup receipt.

The descendant fixtures are Linux-specific; the portable byte/frame bounds fixture also belongs to this App transport. Source composition is distinct from native execution: these changes do not qualify Windows/macOS cleanup or a vendor account.

`answer()` requires observed text, actual `EndTurn`, successful driver completion
and a valid original. Context/transport/sink/parse/limit failures never expose an
answer. `Refusal`, `MaxTokens`, `MaxTurnRequests` and `Cancelled` remain explicit
native stop reasons with diagnostic candidate text and no finalized answer.
An `EndTurn` without text fails `MissingText` while preserving that response.

Run the affected suite with `cargo test --locked --offline -j 1 -p graphit-ai --test cli_kiro`. SDK fixtures cover New/Load routing, optional Select configuration, native confirmation and owned cleanup without requiring an installed vendor program or account.

The actual Role HTTP caller has separate [held-writer lifecycle evidence](managed_role_context.md#validation-and-sources)
for EOF, stop and deadline. That proof preserves the distinction between an
undelivered I/O failure and a complete failed outcome; it does not expose a
private SDK cause or qualify installed Kiro events.

Implementation: [consumer](../../crates/graphit-ai/src/cli_kiro.rs),
[SDK notification classifier](../../crates/graphit-ai/src/cli_kiro_wire.rs),
[affected SDK fixtures](../../crates/graphit-ai/tests/cli_kiro.rs).
