---
title: Rust Incoming MCP Knowledge conversations
references:
  - target: {type: task, id: tsk-625d}
    relation: implements
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-28ee}
    relation: implements
  - target: {type: task, id: tsk-4c72}
    relation: implements
  - target: {type: task, id: tsk-e6ff}
    relation: implements
  - target: {type: task, id: tsk-be6e}
    relation: uses
  - target: {type: task, id: tsk-9953}
    relation: implements
  - target: {type: task, id: tsk-6365}
    relation: supports
  - target: {type: task, id: tsk-ec15}
    relation: implements
  - target: {type: task, id: tsk-b74a}
    relation: uses
  - target: {type: task, id: tsk-8fe2}
    relation: implements
  - target: {type: task, id: tsk-5830}
    relation: uses
  - target: {type: task, id: tsk-0e19}
    relation: uses
  - target: {type: task, id: tsk-4559}
    relation: uses
  - target: {type: task, id: tsk-f02f}
    relation: uses
---

# Incoming MCP Knowledge conversations

The Codex composition of this Linux App integration was qualified in `tsk-8fe2` by ten Incoming fixtures,
five distinct configuration/custody tests and fifteen affected Ask regressions,
with strict checks of the affected App targets. It reuses the startup
Codex Process owner and the [private AI store](rust_knowledge_conversation.md).
The source-generation check now consumes the public authenticated Knowledge
[document republication](rust_knowledge_document_republication.md) API. Its
current App fixture verifies actual subset updates, generation advancement and
selected binding rejection before a new child.

`task.browser.knowledge_conversations` is an optional trusted boolean, default
false. Only Distribution, Global, Environment and Command configuration layers
supply this object. `null` and other value types are invalid. Enabling it adds
USER Knowledge read/mutate policy to the same configured Broker provider entry,
while retaining the existing Memory read policy. It also attaches the same
startup BrokerVerifier and explicit PROJECT access backend Arcs to Source.
Every selected Broker provider must have its explicit MCP resource configured
when conversations are enabled. It creates no new verifier, credentials, profile
loader or transport cache.

`graphit_knowledge_conversation` accepts operation `create`, `continue`, `list`,
`delete` or `clear`, with the name of a configured Broker provider. Create/continue
require `conversation_id`, `query`, `selected_slugs`, and `model`, and may select
`project_dir`/`project_version`. List requires only provider; delete also requires
the Graphit conversation ID. Clear requires that ID and the literal model,
with optional PROJECT selectors, and accepts no query or slugs. Unknown fields, missing required fields and invalid
field types are rejected. The native session ID is never an accepted selector.

Each call verifies its actual HTTP bearer as IncomingMcp, derives USER from the
verified issuer/subject and real installation, and obtains USER Knowledge read
admission. Mutation additionally requires its own Knowledge write admission.
Runtime and Local credentials remain eligible for the separate stateless Ask
path; they cannot establish the USER identity for this tool. Memory permission
does not grant Knowledge access. Each source-reading turn requests a fresh
PROJECT ReadKnowledge decision for the same caller, explicit backend and Original.
List/delete do not read PROJECT sources or remove provider history.

Preparation retains the exact Original and PROJECT read admission used to read
selected Knowledge documents. The consumer borrows their actual digests, recipe
and parameters before execution; it does not reread Data or reconstruct metadata
from the encoded prompt. The stateless Ask uses this same preparation and prompt.
The USER writer may wait; the consumer rechecks USER and PROJECT after acquiring
it and before polling execution. A changed selected binding fails before launching a child. Unrelated global
corpus generation alone does not invalidate an unchanged selection.

The service registers the owned USER writer synchronously before polling the
Codex transport. The entry retains no ProcessHost Arc. The sink confirms the
actual ThreadStarted ID privately and omits that event from public progress.
Other progress uses the existing native projection and streaming decoder.
A resume mismatch is sticky and cannot become successful continuation.

The actual receipt transfers RemainingProcess to that same entry before
persistence, serialization or another await. Complete/NotSpawned cleanup with
no Remaining is terminal observation. Provably unsubmitted paths release the guard: preparation denial before the
transport closure, or the current Codex producer returning Context/Policy/Admission/State
with no Process before its sole Process run call. This uses actual control flow
and keeps the original failure. Persistence reports `not_started`; it does not
claim a pending process or fabricate a NotSpawned receipt.
Pending retains writer and Remaining;
it cannot claim successful persistence or renew the Original. An internally
dropped future without a receipt retains the writer until actual Host join.
MCP request cancellation is separate: the request token drops the HTTP waiter's
receiver and cancels the Original, while the existing blocking job keeps its
worker and borrowed proofs. Dropping a client's local call future alone does
not send that cancellation notification. Shutdown joins those jobs and
the existing Host; pending or failed join is not terminal proof.

A concrete persistence result is reported separately from the actual completion,
usage, process and failure receipt. Failed continuity/persistence or denied
delivery suppresses Answer and the private USER conversation projection while
preserving these observations and any actual persistence status. The tool does
not promise vendor atomicity, zero effects, total history erasure, live vendor
authentication, Windows/macOS support or measured storage performance.

## Explicit native Kiro clear

`operation=clear` is distinct from `delete`, which removes only Graphit metadata.
Clear is available only with the current startup Kiro V3 binding.
The verified Incoming issuer/subject selects the USER namespace and real
Knowledge read/write admissions. The private store rereads the record under its
held writer and resolves the expected native ID internally, matching engine,
literal model, captured program and PROJECT ID/version. Current corpus digests
are not part of this deletion target. No corpus read, query, slugs or native-ID
request/getter is involved; missing records, changed binding, Codex or unavailable
Kiro fail before spawn.

The existing App child adapter and two slots execute the captured V2 argv or
V3 `--v3 acp` with their existing IO, grace and cleanup limits. The
[SDK deletion consumer](rust_kiro_completion.md#explicit-native-session-deletion)
uses ACP 3.3.0/schema 1.11.0's typed Delete API, actual Initialize capability and
a borrowed USER/PROJECT recheck immediately before the effect. It reuses the
Client driver, QuietSdk and serial sink; observed updates are in `Deleting`
phase and never answer text. There is no New/Load/Select/Prompt or dummy request.

The USER writer remains held through actual child wait, including cancellation,
abandonment and service shutdown; driver completion or SDK EOF is not join.
Incoming cancellation/deadline can return a finite error before the owned job
is terminal. No public receipt is observed on that path, so there is no
fabricated counter or native ACK; custody and writer remain held until actual wait.
Only actual Delete success, successful driver, clean join and the same post-Auth
checks permit `finish_delete` to remove the local record under that same writer.
`cleared` additionally requires confirmed local deletion and authorized delivery.
Unsupported capability, cancellation, SDK failure or ambiguous remote outcome
retain the local binding, without automatic retry. Local IO failure after native
ACK preserves response/observations and its cause, with no rollback claim.
Delivery denial removes private projection while retaining evidence; there is no
answer for this operation. When an actual SDK error is observed, `sdk_error_code`
preserves its numeric code (`i32::from` the SDK error code), without exposing the
raw SDK message or data.

The [ACP deletion semantics](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/rfds/session-delete.mdx)
remove the selected session from `session/list`; storage and later Load behavior
remain agent-defined. Clear is not a vendor history wipe. The qualification
passed 51 distinct cases, including 13 new cases, with zero ignored:

| Scope | PASS / new | Harness (s) | Wall (s) |
| --- | --- | --- | --- |
| SDK | 37 / 5 | 0.41 | 4.814219791 |
| Kiro store | 7 / 3 | 2.76 | 4.231335463 |
| AppClear04 | 5 / 5 | 7.77 | 42.722747192 |
| Existing Kiro continuity | 1 / 0 | 13.78 | 14.251365067 |
| Existing Codex continuity | 1 / 0 | 3.39 | 45.642411516 |

AI strict checks with `--no-deps -D warnings` passed in 20.681566652 s wall;
App lib/conversation-test/SDK-bin strict checks passed in 29.921909115 s wall.
These are command/harness costs, without a performance gain claim. The
post-response fixture gates the response
without a pending notification, distinguishing an actual ACK followed by Auth
denial from a legitimate post-push delivery failure. These are controlled
fixtures, without vendor authentication or other-platform qualification.

Prior attempts remain historical: SDK01 had a temporary-borrow compile error,
App01 a helper-Arc compile error, App02 stopped before its harness on the external
AST closing delimiter at `catalog.rs:471`, and App03 failed two fixture
expectations. Their logs and receipt history remain intact; partial passes,
retries and loop scenarios do not add distinct cases. The qualification record
location is `.build/rust-kiro-native-clear/qualification.json`, written after this
documentation snapshot. This proof covers Linux with actual Broker/USER/PROJECT
and typed native Agent fixtures. It does not qualify commercial authentication,
vendor session listing/storage wipe, Main, all CLI protocols or Windows/macOS.
Decoder/runtime pins, AskService and capture remain unchanged.

## Kiro App extension and Linux caller qualification

The current Incoming Kiro binding is KiroV3 (`kiro-v3`), alongside Codex and
the Claude composition below. New/Load use the same private-store API and a
literal requested model capped at 4096 bytes. The launcher selects
`kiro-cli acp --agent-engine=v3`; the initial model is supplied through native
session metadata. Neither a model factory nor a global model configuration key
is introduced. Current SDK and caller proof is separate from the
[installed native gate](rust_project_hook_context.md#current-installed-kiro-v3-qualification).

The following `tsk-ec15`/`tsk-b74a` receipts are historical V2/V3-preview
qualification. Those executions used V2 `acp --agent-engine=v2 --model MODELO`
and preview `kiro-cli --v3 acp`. The historical help probe returned ACP usage;
it did not qualify authentication or model execution. These argv observations
are not current selectors or compatibility surfaces.
The official page still presents `--agent-engine=v3`, which is not this caller's
argv. The [ACP V3 reference](https://kiro.dev/docs/cli/v3/acp-migration/) supports
only the metadata/Select contract here: `_meta.kiro.modelId` for New/Load.
Its explicit SDK Select must match a native literal offer and receive full native
confirmation before Prompt, with no model or session fallback. The caller does
not supply MCP servers or additional roots; this does not establish native
configuration isolation or inspect and clamp profiles.

Incoming retains USER Knowledge read/write and requests fresh PROJECT
ReadKnowledge for each source turn, without PROJECT write. The separate
stateless Ask retains Runtime/Local authorization. Both reuse one corpus read,
the shared renderer and the existing INSTRUCTION. Empty corpus/selection does
not spawn a child. Loading history is not fresh answer text, and a requested Load
ID is not observed continuity until the actual Load response arrives. Public
progress carries phase and native updates without the envelope `session_id`;
tool IDs and data remain in the update and do not prove Records effects.

The App adapter owns a Tokio Command child in its own group with captured cwd
and environment and connects the SDK through ByteStreams, without a second Host
or RPC. Async `AskService::kiro_process` waits for one of two slots under the
Original cancellation/deadline and service stop, then rechecks the same borrowed
USER/PROJECT admission. The SDK-static `ConnectTo` transport receives the already
admitted owned permit without a semaphore await and rechecks Original/service
stop before spawn. The permit stays held through actual Child wait; driver
completion does not release the slot or USER writer. Cleanup sends TERM to the
owned process group, waits its grace, then sends KILL even if the leader already
exited. It retains the real stderr drain through EOF or an observed read/size
failure. A clean join requires the same child wait, successful group control and
stderr EOF; this does not claim to reap descendants belonging to another parent
or control descendants that leave the owned group. Input/stdout/frame/stderr caps are respectively
1 MiB/8 MiB/1 MiB/64 KiB, with 2 s grace and 10 s cleanup. Prompt/query/response
caps are 1 MiB/64 KiB/8 MiB, IDs/models 4096 bytes and notifications 65536. These
are operation limits, not SDK queue or RSS bounds.

The USER writer is transferred before polling the RPC and retained through the
actual kernel child wait, which is distinct from SDK driver OK or EOF. Pending
or failed join retains child and cause. A lexical `ConversationDecisionGuard`
starts unsubmitted after the turn is held, before waiting for a slot. Drop without
a transport calls `release_unsubmitted`. After the owned transport returns,
`submitted()` is called before execution; subsequent Drop abandons only the
persistence decision, retaining the writer through actual Child wait, including
the interval after receipt but before join/finish. `decided()` disarms the guard
only after finish. The SDK may normally discard the transport future in
`connect_with`; this is not abandonment by the App caller. The adapter has no
COMPLETED flag: its Drop only requests stop and releases a provably unspawned
turn. Persistence decisions belong to the App guard and finish, not the SDK
future. Driver completion does not release the writer. HTTP waiter
cancellation leaves owned jobs running cleanup: Cancelled text or no receipt
does not prove zero effects or join. Post-execution authorization denial strips
Answer and the private record projection while retaining actual observations.
Default logs exclude native session IDs and secrets.

App check02 and fixture bin02 passed. Before the V3 argv change, native04 passed
four cases, native06 passed one and httpcancel02 passed one: six unique Linux conversations using a real
Broker, USER/Lance and SDK Agent child. They cover V2/V3 New/Continue/cold reopen,
selected/unrelated binding changes, partial persistence, Load failure without New
fallback and recovery, and frame/stderr/deadline limits without a partial answer.
The focused total before the argv change was 29 distinct cases: six conversations, six ec15
libkiro cases, eight common-preparation cases, eight Ask regressions and one
Codex Incoming case. Libkiro02 passed seven tests, including one foreign hook
excluded from this count (2.15 s harness/2.69 s wall); commonprep01 passed eight
(8.06 s/11.82 s); Askregression01 passed ten, including two repeated cases
excluded from the total (10.88 s/11.34 s); CodexIncoming01 passed one
(1.96 s/23.62 s). Loops, retries and incidental hooks are not additional unique
tests. Clippy01 passed the lib, Question/Conversation integration and bin scopes,
without dependencies and with `-D warnings`, in 23.78 s. Source Tracker found no
divergence in the three guard corrections. These are focused Linux proofs and
command costs, not an inference benchmark.
Native01's one PASS/two fixture failures and bin01's schema/borrowed-ID compile
failures remain historical. Native02 passed three cases with a 16.42 s harness
and 264.67 s wall, including approximately 4 min 08 s of build. Custody01 stopped before execution on three external
compilation errors in `role_tool_server/git.rs`; build06 passed in 16.16 s.
Libkiro01 passed six new ec15 cases plus one incidental hook (2.24 s harness,
29.19 s wall). Custody02 passed three cases including queued Drop with a USER
writer (2.13 s harness, 31.05 s wall), before the latest adapter-Drop revision.
Native03 exposed two real premature writer-release failures, motivating the
correction above. These receipts are preserved without adding repeated revisions
to the current total.

Httpcancel02 passed the actual HTTP cancellation case: the USER writer remains
held after SDK DriverClosed and public MCP cancellation, releasing only after
Source shutdown observes actual Child wait. The controlled native child ignores
TERM and delays exit; it is not a commercial authentication fixture. Harness
time was 2.64 s, command wall 100.19 s, including 1 min 37 s of build and shared
C++ rebuild. An earlier attempt failed during a shared AST build before its
tests ran; its evidence remains historical.

The historical receipt `.build/rust-knowledge-kiro-app/qualification.json`
covered this App code, tests and focused documentation. It was removed and is
not available in this checkout; the qualification remains recorded in `tsk-ec15`. Before the argv change,
Clippy02 of `knowledge_conversation` and the
SDK bin, without dependencies and with `-D warnings`, passed: dev 1 min 50 s and
wall 110.69 s, including shared C++ rebuild, without a performance gain claim.
All 29 unique cases, both affected strict Clippy scopes, fmt10, diff and local
links passed; the Source Tracker readback of guard corrections was NOFAULT.

With the current V3 argv, v3flag-conversation01 passed four conversations
(17.21 s harness/42.72 s wall). The SDK Agent fixture accepts only
`["--v3", "acp"]` for V3 and rejects the previous `--agent-engine=v3` form.
V3flag-ask01 passed both stateless V3 Ask cases, zero failures or ignored and 206
filtered (2.34 s harness, 1 min 21 s build, 84.83 s wall). These six requalified V3
cases remain within the 29 distinct cases; they do not increase the total to 35.
Clippy03 of lib, `knowledge_conversation` and SDK bin, without dependencies and
with `-D warnings`, passed (dev 1 min 03 s, wall 63.91 s, including shared C++
rebuild). Service/fixture formatting and diff after the flag change passed; the
Tracker readback of the exact argv was NOFAULT. V2, caps, metadata/Select, Auth,
custody, IO, HTTP cancellation and the Codex guard are unchanged and were not
rerun. The ec15 caller is qualified in this Linux scope, without qualifying the whole App, commercial Kiro authentication,
Main, all CLI protocols, Windows or macOS.

The affected commands are:

```text
cargo xtask cargo test --locked --offline -p graphit-app --test knowledge_conversation
cargo xtask cargo clippy --locked --offline --no-deps -p graphit-app --lib --test knowledge_conversation -- -D warnings
```

The historical Codex Incoming fixture set covers Foreground opt-in/reopen/caller isolation,
partial USER permissions and genuine Runtime denial through the public Source
entry, fresh PROJECT denials, physical capture changes and JWT expiry after an
effect, original deadlines during writer wait/execution, actual source/recipe/
program/model binding changes, corrupted records, post-effect destination
conflicts, private incremental progress and HTTP cancellation. C5 publishes
Paris and Unrelated together from captured compiled files, updates only
Unrelated and allows continuation, then updates Paris and observes BindingChanged
before a third child: exactly two launches and three fresh PROJECT decisions.

Local origin rejection, foreign installation/proof association and PROJECT
version association also rely on inspection of the closed App composition and
the delivered Auth/private-store tests. They are not additional Incoming
end-to-end fixtures, transplanted installation tests or multi-version Data
qualification. A local dropped call future is not TCP disconnect proof.

Results, queue/build time and unresolved cases belong to the Task receipt.
The preexisting [stateless Ask contract](../architecture/rust-knowledge-question.md)
continues to apply to its separate ingress.

## Claude Incoming composition

With a trusted Claude startup binding, create/continue use the same actual
Incoming USER read/write and fresh PROJECT ReadKnowledge admissions and borrow
the selected document metadata before consuming the prepared prompt. The
native print request uses `--output-format stream-json --verbose
--include-partial-messages --model <literal>`; continuation adds `--resume`
with the private observed ID. It never selects the latest session.

The existing Claude decoder supplies Init to conversation custody before
progress projection. Actually observed Init IDs may appear in authorized
operational progress/receipts; the stored native ID has no getter or public
conversation field. Success requires the producer's primary final result,
exit/EOF proof, live admissions and terminal cleanup before persistence.
RemainingProcess transfers synchronously before projection or another await.
Only Context/Policy/Admission without Process prove an unsubmitted Claude
failure; the Codex-specific State case does not apply. Failure, mismatching
identity or failed persistence suppresses final answer/native result and USER
conversation projection while preserving the actual failure receipt. Already
emitted authorized progress cannot be retracted; its accepted prefix remains
operational evidence.

List/delete remain metadata operations; clear remains Kiro-only.

## Qwen continuation

The Qwen extension uses this same USER record and service-owned writer. The [Qwen contract](rust_qwen_completion.md#incoming-conversation-continuation) describes the exact headless `--resume` request, observed Init identity, sticky continuity failures and native tool-policy boundary.

## Copilot Incoming integration

Copilot uses the native prompt-mode CLI rather than the former SDK/TCP transport.

The [consumer](../../crates/graphit-app/src/source_server/knowledge_conversation/copilot.rs)
prepares fresh documentary sources and acquires and registers the USER writer
before polling the real ProcessHost producer. The captured program, environment,
PROJECT cwd and literal model feed `--output-format=json --no-ask-user
--available-tools= --model=<literal>`, with the documentary prompt on stdin.
Continue and cold reopen append only `--resume=<exact-private-UUID>` from the
committed binding. Missing or mismatched native sessions fail without creating
a replacement. Fresh selected-source and USER/PROJECT bindings still apply.
The [AI contract](rust_copilot_completion.md#incoming-conversation-copilot)
defines native correlation and limits.

The CLI may reveal the session ID only in its terminal `result` after consuming
the prompt and emitting progress. Create observes that native ID; Resume requires
it to match the private selector. There is no session-start ACK before the prompt.
A late authority change can follow actual native effects; it blocks persistence
and delivery without claiming those effects never happened.

The actual Process receipt transfers RemainingProcess to the existing custody
entry before persistence, projection or another await. Abandoned native futures
retain the USER writer until actual Host join. Cancellation, deadline and Source
shutdown use that same Process owner. Success requires correlated main answer
text, a successful native `result`, actual process exit 0, both pipe EOFs,
Complete cleanup without Remaining, live admissions and successful USER commit.
The CLI can emit `result.exitCode=0` before a later session-save failure; that
nonzero process exit cannot commit or deliver the answer.

Conversation progress and `receipt.native_result` are structured public
projections: native `sessionId` and tool `arguments` are removed recursively.
The complete native terminal remains private in the AI receipt; the public
projection is not called raw JSON. Stateless Ask retains its separate raw-event
and raw-terminal contract. Denied delivery or failed persistence removes answer,
public terminal and USER projection while preserving accepted progress and
categorical process observations. An expired Incoming Original can return the
canonical deadline error without a structured engine payload; custody still
retains the writer through native cleanup, without a USER commit.

List/delete retain USER metadata semantics; delete leaves vendor history intact. Clear remains Kiro-only. App tests run actual local CLI children through Source, HTTP/STDIO and the compiled Graphit caller, without a vendor executable, account or plan. The Incoming suite includes exact resume/cold reopen, native save failure after result, late USER/PROJECT denial, deadline, destination conflict and public projection privacy. Other-platform and authenticated vendor execution are not established by these controlled Linux fixtures.

## Cursor continuation

The [Cursor extension](rust_cursor_completion.md#incoming-conversation-continuation) uses the same USER store, selected source binding and service-owned custody. Create observes native Init.session_id; continue and cold reopen supply only that committed private ID with explicit `--resume ID` before the literal prompt. The captured `--mode ask`, program, environment, model and admitted cwd stay unchanged. There is no latest/session creation fallback, public native selector or extra authority.

The existing native driver checks session identity, success, exit/EOFs and cleanup; typed custody retains the writer through actual Process termination. Current USER/PROJECT/Original and successful persistence are required for delivery. Native policy does not become an OS sandbox claim. Controlled peers and real Graphit consumers are the acceptance boundary, without a vendor binary or account requirement.

## Grok continuation (qualified)

The [Grok extension](rust_grok_completion.md#incoming-conversation-continuation) uses the same USER store and service-owned custody. Native `streaming-json` has no Init; the observed `end.sessionId` supplies the private UUID. Continuation and cold reopen use only explicit `--resume ID`, never title/latest/continue/fork/creation or fallback selection. Fresh USER/PROJECT Knowledge and unchanged selected source bindings remain mandatory. Native text before End is progress, not proof of continuity or a final answer.

Custody adopts TempPath before polling and retains it with the USER writer through actual Process/Remaining termination or Host join, including native-future abandonment. No parallel process owner is created. End/end_turn, matching UUID, exit/EOFs, cleanup, commit and current authority govern delivery. Native `--deny` and `*` stay literal model-tool rules, not an OS sandbox or personal settings change. No vendor binary, login or paid service is a test gate.

## OpenCode Incoming integration

The active contract is [OpenCode 2 via ACP protocol 1](rust_opencode_completion.md). OpenCode 1 support is removed. Child-session progress is preserved.

The OpenCode 2 [consumer](../../crates/graphit-app/src/source_server/knowledge_conversation/opencode.rs)
selects captured CliCompletionProvider OpenCode without adding a NativeBinding
host. It reuses central documentary preparation, private USER store and existing
ACP transport. Trusted opt-in, verified USER read/write, fresh PROJECT Knowledge
admission and the same finite Original apply per turn. Typed rechecks guard
Initialize, New/Load, model selection, Prompt, callbacks, commit and delivery.
A session ACK never grants authorization.

Create observes the native ID; continue/cold reopen load the exact private ID
without New fallback. Pre-ACK replay is validated and counted separately, never
forwarded as new progress, usage or answer. Current-turn native updates retain
the existing scoped progress payload, including protocol IDs when present;
private stored selectors are absent from metadata/final receipts and are never
accepted as tool inputs. Load's current model must equal the
requested literal before normal Set ACK confirmation and prompt. See the
[protocol/error contract](rust_opencode_completion.md#incoming-conversation-opencode).

AskService associates the actual ACP owner with custody synchronously before
returning its transport or a native-spawn failure. A recovered RAW child or
unknown NativeAttempted birth is retained with its original reason; a raw leader
wait cannot release custody as though the Job tree had joined. See
[platform process ownership](rust_opencode_completion.md#native-process-ownership-and-platforms). Submission precedes SDK polling. Abandon, cancel,
deadline, dropped waiter and Source shutdown stop/reap that same owner; shutdown's
OpenCode branch also observes retained Conversation entries. The writer releases
only after actual process/tree and transport/pipe retirement, not because an RPC
waiter or SDK foreground finished. Existing map_future finish metadata is retained.

EndTurn/new text, completed driver, clean process observation, successful USER commit and live authority govern final delivery. Denial or failed persistence suppresses answer, native terminal and public USER projection while preserving accepted current progress and causal errors. Incoming deadline never permits partial commit. List/delete remain metadata; clear remains Kiro-only.

## Kimi continuation (qualified)

The [Kimi extension](rust_kimi_completion.md#incoming-conversation-continuation) integrates the experimental Server API with the same USER store and native-process custody. Create observes the returned opaque session ID; continue/cold reopen GET only that persisted ID, validate ID and metadata.cwd, subscribe to the same main session and submit a fresh prompt there. The mature driver retains session/prompt/turn/UTF-16 correlation and native manual permission policy; no UUID, Init, autoapproval, trust or sandbox is invented. Fresh USER/PROJECT/Original and selected source bindings, native completed without error, successful exit/EOFs/join/cleanup and USER commit govern delivery. Arc<KimiServer> stays in existing custody through readiness, abandoned callers and actual terminal observation.

## Antigravity continuation (qualified)

The [Antigravity extension](rust_antigravity_completion.md#incoming-conversation-continuation) integrates the existing native stdin stream-json driver with the same USER store and Process/Remaining custody. Init supplies the private conversation_id; continuation and cold reopen append only --conversation ID. Init is checked before progress and result must keep that identity; only the current successful result.response after real exit/EOFs/cleanup and USER commit is delivered. Usage, num_turns and duration remain cumulative native terminal metadata, without per-turn reinterpretation or derived counters. Fresh USER/PROJECT/Original and selected physical/compiled sources remain mandatory. The captured headless policy gains no trust or autoapproval. No vendor binary/account is a gate. All active providers map directly to a ConversationEngine, while unavailable service configuration still fails before native effects.
