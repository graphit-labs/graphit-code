---
title: Rust USER knowledge conversation storage
references:
  - target: {type: task, id: tsk-625d}
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
  - target: {type: task, id: tsk-658c}
    relation: corrected_by
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-5830}
    relation: implements
  - target: {type: task, id: tsk-8fe2}
    relation: relates_to
  - target: {type: task, id: tsk-b74a}
    relation: implements
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Rust USER knowledge conversation storage

The Linux `graphit_ai::knowledge_conversation` module stores a small private
binding over the existing Core USER ArtifactStore. It receives an actual
`CapturedPublicationScope`, its caller's Original snapshot, and explicit IO
and functional bounds. Capture, list and read do not create an absent namespace.

This is a mechanical library. Its inputs do not establish authentication.
The [App conversation consumer](../architecture/rust-knowledge-question.md)
must separately admit USER Knowledge read/mutate operations for the verified
caller and obtain fresh PROJECT ReadKnowledge for every turn that reads sources.
USER Memory admissions and serialized metadata cannot authorize these operations.
The IncomingMcp composition remains the separate `tsk-8fe2` delivery.

## Stored data and public projection

The closed version-1 record contains a Graphit conversation selector,
provider, engine, requested model, captured program, PROJECT identity
and version, selected source identities, and the private native session ID.
Creation requires native identity observation. Every continuation must confirm
that stored identity through its engine's native continuity contract below. It contains no question, answer, copied vendor history,
credential, environment, proof, admission, cancellation or deadline.

The Graphit selector is independent of the native ID. It is bounded data chosen
by the consumer, not authority. A SHA-256 filename confines even selectors
containing path punctuation to one file directly under the captured USER root.
The ArtifactKey remains in the `graphit-ai/knowledge-conversation` namespace.

`PublicConversation` exposes the selector, provider, engine, requested model,
PROJECT identity/version and selected slugs. It exposes neither the native ID
nor physical paths. Native IDs have no public getter and error formatting omits
record values. Explicit owned-cause inspection is operational material and must
not be forwarded into the public protocol or ordinary logs.

Source matching compares the actual selected slug, path, physical digest when
available, compilation digest, recipe and recipe parameters. Provider, engine,
model, captured program and PROJECT version must also match. The record does not
contain a corpus-wide generation: changes in unrelated documents alone do not
invalidate it. Matching these observations never replaces fresh authorization.

## Owned turn and native continuity

`begin_create` obtains the existing USER kernel writer, confirms that both key
and destination are absent, and retains that writer before a child is launched.
It does not persist a provisional native identity. `begin_turn` re-reads the
existing record under the writer and rejects a changed binding before resume.

The writer lock is shared by all ArtifactStore writes of the same USER. This
initial implementation therefore serializes turns of different conversations
belonging to that USER as well. Cold reads do not acquire it.

`ConversationTurn::configure_codex_request` validates the captured program and
explicit model and preserves the request's environment, cwd, limits and existing
arguments. Continuation inserts the supported Codex exec resume selector before
the stdin prompt marker. It does not launch a process or refresh native login.

The sink supplies the actual `ThreadStarted` view to
`observe_thread_started`. Continuation requires the expected private native ID.
A different ID fails permanently for that turn, even if Codex opened and executed
a new session. The absence of a confirmed event also prevents commit. This
does not prove that the vendor performed no effects.

`configure_kiro_session(program, cwd, Original)` builds the typed ACP New or
Load request from the same binding and its private expected native ID. It
requires the captured program, a finite live Original, and an absolute cwd
within the field bound. The App supplies that cwd from the admitted PROJECT;
the builder does not establish PROJECT access. Load never falls back to New.
No MCP servers or additional directories are supplied by this binding API.

For current Kiro V3, both New and Load carry the binding's literal requested
model in the documented `_meta.kiro.modelId` field. No V2 or preview selector
is retained. These initial requests do
not prove that the native agent accepted a model: the App must use the SDK's
actual offers and confirmation where configuration selection applies.
See the [Kiro V3 ACP contract](https://kiro.dev/docs/cli/v3/acp-migration/).

`observe_kiro_session` consumes the actual ACP session identity and uses the
same bounded, sticky mismatch checks as Codex. The observation methods and
request builders reject the other engine. Neither method exposes the stored
native ID, launches a process, authenticates a caller, copies native history,
or proves child termination. App Kiro transport and custody integration is the
separate `tsk-ec15` delivery; this library's tests use the real USER ArtifactStore
and typed SDK requests, not a credentialed Kiro process.

`commit` publishes a generated private Copy payload only after a matching native
ID has been observed. The common `PayloadMode::private_file()` constructor supplies
Unix mode 0600 or writable Windows file attributes without I/O or ACL grants.
The existing Core producer retains private birth, ACL and publication ownership;
constructing the mode does not capture or change the USER directory's permissions. The caller retains the guard on failure. Commit
cannot establish terminal Process cleanup. The App must transfer the guard
to durable service custody before polling execution, retain it with any
`RemainingProcess`, and release it only on a real terminal observation.
The pre-receipt custody integration is part of `tsk-8fe2`, not proved by this
library.

The store's I/O helper uses `ScopeContext::from_snapshot` to retain the
Original's optional deadline and cancellation. A service Original without a
deadline can perform cold reads, owned writes and deletion while it remains
uncancelled; cancellation returns the actual `Interrupted` IO cause. A finite
Original keeps its absolute deadline. Every operation still requires the same
captured Original identity and the caller's actual scope. This I/O contract does
not supply a finite inference deadline or establish USER/PROJECT authorization.

The real store implementation and its SDK request builders are composed on Linux,
macOS and Windows. The scope, held writer, native ID checks and publication APIs
remain the same; source composition does not establish native Windows/macOS
execution evidence. Existing Linux qualifications keep their original scope.

## Private target for native Kiro clear

`begin_kiro_delete(id, KiroConversationTarget, Original)` rereads the existing
private record under the same USER writer. Its target compares actual selected
engine, literal model, captured program and PROJECT ID/version, with provider
`kiro`. It preserves stored source metadata and does not require current corpus
digests or read pages. These target fields are data; the App separately supplies
verified issuer/subject USER admissions and PROJECT access. Missing records,
Codex, changed targets or invalid private IDs fail before a child is launched.

`ConversationTurn::configure_kiro_delete(program, Original)` builds a typed SDK
Delete request from the expected private identity. There is no native-ID getter
or public native-ID selector. The same Original must remain finite and valid;
no request query/slugs, native history, global session listing or new authority is
required to build it.

After actual SDK success, clean native child join and the caller's post-Auth
checks, `delete_kiro_record` removes the registered local active copy through that same held writer. It does not unlock and call an unrelated
store delete. Unsupported capability, cancellation, SDK failure or ambiguous
remote effect retain the local binding. Local IO failure after native ACK keeps
the real response, observed state and causal error; neither the library nor its
[App caller](rust_knowledge_conversation_app.md#explicit-native-kiro-clear)
claims rollback, automatic retry or total history erasure. The seven Kiro store
tests passed (three new and four affected), with zero ignored, a 2.76 s harness
and 4.231335463 s wall. AI strict checks passed in 20.681566652 s wall.
The [App clear qualification](rust_knowledge_conversation_app.md#explicit-native-kiro-clear)
passed 51 distinct scoped cases overall, 13 new and zero ignored, including
SDK, App clear and existing Kiro/Codex continuity, with affected App strict checks
passing. Partial attempts, retries and loop scenarios are not additional tests.
The scope is Linux with real Broker/USER/PROJECT and typed Agent fixtures,
without commercial authentication, vendor listing/storage-wipe, Main, all CLI
or Windows/macOS qualification. An Incoming cancellation/deadline
error can return before the owned job is terminal, with no public receipt or
fabricated counter/ACK; the same writer remains held through actual child wait.

## Errors, deletion and maintenance

The same Original is checked before and after IO; equal cancellation signals
and deadlines cannot substitute a new snapshot. Missing, conflict, invalid
record, limit, binding change and native-session errors are finite. Actual Core
and request causes remain owned with sanitized default formatting. Unknown
record versions and malformed JSON are errors, without migration or fallback.

Delete removes only the registered Graphit-owned active copy and binding.
It does not remove vendor history. Generated session state is published directly to its effective private file.
There is no duplicate generated-content store, pin inventory or payload GC.
Deletion uses the existing registered publication and preserves vendor history;
it does not claim erasure outside the Graphit-owned active copy.

## Qualification

The conversation module in the [AI integration target](../development/ai-tests.md)
uses real Core USER captures, native kernel
locks, generated payload publication, private files and cold reopen. It does
not use vendor authentication or live inference. Run:

```text
cargo xtask cargo test --locked --offline -p graphit-ai --test integration knowledge_conversation::
cargo xtask cargo clippy --locked --offline --no-deps -p graphit-ai --lib --test integration -- -D warnings
```

In `tsk-5830`, the Linux fixture passed 10 tests with no failures or ignored
cases (harness 0.28 s). The selected Clippy command passed with warnings denied.
Test wall time was 461.478686604 s and Clippy wall time was 340.453071355 s;
both include the shared Cargo queue and build work, so neither is a storage
performance measurement. The Task receipt is
`.build/rust-knowledge-conversation/qualification.json`. App integration,
vendor behavior and Windows/macOS support remain separate from this fixture.

The `tsk-b74a` Kiro binding extension passed the current 15 unique Linux cases,
including four added Kiro cases, with zero failed or ignored cases. Cargo
reported 3.52 s for build work, 2.12 s for the harness, and `/usr/bin/time -p`
reported 5.70 s total. The first two attempts stopped at compilation because
of incorrect SDK imports and a `SessionId` trait assumption; they executed no
tests and are retained in
`.build/rust-knowledge-conversation-kiro-binding/qualification.json`.
The selected Clippy command also passed with warnings denied, with 64.82 s
total time including dependency check/build work. These timings
describe the affected fixture run, not inference performance or vendor
authentication. App Kiro execution and child custody remain `tsk-ec15` work.

## Claude continuation

`configure_claude_request` validates the same Original, Claude engine/provider,
captured program, absolute bounded cwd, stdin and fixed print/stream-json argv.
Create supplies no synthetic native ID. Continue appends only `--resume` and the
private ID previously observed by `observe_claude_session`. It preserves the
prepared stdin, environment and Process limits. A mismatching observed ID is
sticky; all Kiro entrypoints explicitly accept only current KiroV3.
The version-1 record and public projection stay the same: there is no stored-ID
getter or client native-session selector.

## Qwen continuation

The Qwen extension uses this same USER record and service-owned writer. The [Qwen contract](rust_qwen_completion.md#incoming-conversation-continuation) describes the exact headless `--resume` request, observed Init identity, sticky continuity failures and native tool-policy boundary.

## Copilot native continuity

Engine `copilot` keeps the version-1 USER binding, writer and public projection. There is no native-ID input or stored-ID getter. Consumer execution and cleanup are verified separately.

`configure_copilot_session` checks the same Original, captured program, literal
model, engine/provider and admitted absolute cwd. It returns Create or Resume
of the exact private ID loaded under the writer. The runner passes a complete
canonical UUID through `--resume` and requires exact equality with native
`result.sessionId`. Missing native sessions fail without fallback creation.
There is no SDK response or RPC ACK. `observe_copilot_session` checks the expected
Create/Resume mode and makes a differing identity a sticky continuity failure.

Observing `result` does not authorize commit: the CLI can fail its native save
after emitting result0. The App retains the real Process/Remaining receipt and
USER writer in its existing custody. Matching identity, a primary answer,
terminal process cleanup, current authority and successful persistence are all
required before delivery. Native `result` and selectors stay private; public
Conversation events use the App's safe projection.

See the [Copilot CLI contract](rust_copilot_completion.md#incoming-conversation-copilot)
and [Incoming integration](rust_knowledge_conversation_app.md#copilot-incoming-integration).

## Cursor continuation

The [Cursor extension](rust_cursor_completion.md#incoming-conversation-continuation) uses the same USER store, selected source binding and service-owned custody. Create observes native Init.session_id; continue and cold reopen supply only that committed private ID with explicit `--resume ID` before the literal prompt. The captured `--mode ask`, program, environment, model and admitted cwd stay unchanged. There is no latest/session creation fallback, public native selector or extra authority.

The existing native driver checks session identity, success, exit/EOFs and cleanup; typed custody retains the writer through actual Process termination. Current USER/PROJECT/Original and successful persistence are required for delivery. Native policy does not become an OS sandbox claim. Controlled peers and real Graphit consumers are the acceptance boundary, without a vendor binary or account requirement.

## Retired Gemini continuation

The Gemini CLI driver and its continuation are removed. Historical receipts do not qualify
Antigravity/AGY. Use the [AGY continuation](rust_antigravity_completion.md#incoming-conversation-continuation)
with its own protocol and private identity. Google Gemini API providers remain independent.

## Grok continuation (functional proof passed; quality pending)

The [Grok extension](rust_grok_completion.md#incoming-conversation-continuation) uses the same USER store and service-owned custody. Native `streaming-json` has no Init; the observed `end.sessionId` supplies the private UUID. Continuation and cold reopen use only explicit `--resume ID`, never title/latest/continue/fork/creation or fallback selection. Fresh USER/PROJECT Knowledge and unchanged selected source bindings remain mandatory. Native text before End is progress, not proof of continuity or a final answer.

Custody adopts TempPath before polling and retains it with the USER writer through actual Process/Remaining termination or Host join, including native-future abandonment. No parallel process owner is created. End/end_turn, matching UUID, exit/EOFs, cleanup, commit and current authority govern delivery. Native `--deny` and `*` stay literal model-tool rules, not an OS sandbox or personal settings change. No vendor binary, login or paid service is a test gate.

## OpenCode native continuity

The tsk-28ee extension uses engine/provider `opencode` in the version-1 USER
record and existing writer. `configure_opencode_session` checks the same
Original, captured program, literal model, engine/provider and admitted cwd,
then returns Create or Load of the exact private persisted ID. The synchronous
callback observes New's ID or confirms requested Load through its correlated SDK
ACK. LoadSessionResponse has no ID field; no echo, _meta identity or public
stored-ID getter is invented. Binding mismatch remains sticky.

The [OpenCode contract](rust_opencode_completion.md#incoming-conversation-opencode) separates audited pre-ACK replay from the new turn and rejects current-model drift before Set/prompt. The [Incoming integration](rust_knowledge_conversation_app.md#opencode-incoming-integration) retains the same ACP owner and writer through actual native/transport retirement. Metadata deletion leaves native history intact; clear is Kiro-only. Tests require no vendor installation or account.

## Kimi continuation (functional proof passed; quality pending)

The [Kimi extension](rust_kimi_completion.md#incoming-conversation-continuation) integrates the experimental Server API with the same USER store and native-process custody. Create observes the returned opaque session ID; continue/cold reopen GET only that persisted ID, validate ID and metadata.cwd, subscribe to the same main session and submit a fresh prompt there. The mature driver retains session/prompt/turn/UTF-16 correlation and native manual permission policy; no UUID, Init, autoapproval, trust or sandbox is invented. Fresh USER/PROJECT/Original and selected source bindings, native completed without error, successful exit/EOFs/join/cleanup and USER commit govern delivery. Arc<KimiServer> stays in existing custody through readiness, abandoned callers and actual terminal observation.

## Antigravity continuation (qualification pending)

The [Antigravity extension](rust_antigravity_completion.md#incoming-conversation-continuation) integrates the existing native stdin stream-json driver with the same USER store and Process/Remaining custody. Init supplies the private conversation_id; continuation and cold reopen append only --conversation ID. Init is checked before progress and result must keep that identity; only the current successful result.response after real exit/EOFs/cleanup and USER commit is delivered. Usage, num_turns and duration remain cumulative native terminal metadata, without per-turn reinterpretation or derived counters. Fresh USER/PROJECT/Original and selected physical/compiled sources remain mandatory. The captured headless policy gains no trust or autoapproval. SDK3, Incoming3 and the actual missing-program rejection case await execution; no vendor binary/account is a gate. The current completion providers map directly to a ConversationEngine, while unavailable service configuration still fails before native effects. The obsolete negative Gemini capability test is replaced, not treated as passed; historical Gemini proof is not a current AGY qualification.
