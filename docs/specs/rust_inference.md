---
title: Rust direct inference library
description: Explicit OpenAI-shaped embedding requests with bounded transport, vectors and execution context.
type: specification
updated: 2026-11-05
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target: {type: task, id: tsk-0f32}
    relation: implements
  - target: {type: task, id: tsk-ed52}
    relation: uses
  - target: {type: task, id: tsk-56cb}
    relation: implements
  - target: {type: task, id: tsk-55ac}
    relation: derived_from
  - target: {type: task, id: tsk-72bc}
    relation: derived_from
  - target: {type: task, id: tsk-b96c}
    relation: validated_by
---

# Rust direct inference library

`graphit-ai::openai::OpenAiEmbeddings` supplies direct OpenAI and compatible
embedding requests through a caller-owned `graphit-http::client::BoundedClient`.
The [Direct service-lease consumer](rust_direct_embedding_leases.md) composes
this SDK with the captured Auth binding; the active product flows remain
separately integrated. This library does not select an active profile or refresh credentials,
discover Broker routes, publish vectors, run local models or cut over search.

## Captured inputs and wire contract

`OpenAiEmbeddings::new(client, endpoint, model, dimensions,
request_dimensions: Option<usize>, limits)` captures the existing client, full
endpoint, model, positive expected dimensions and `EmbeddingLimits`. `Some(width)`
must be positive and equal to expected width; `None` omits the wire dimensions
field. Native-model reduction policy belongs to the Direct lease consumer. Endpoint/model are borrowed; the constructor reads no ambient
environment, configuration or profile and builds no HTTP pool. The caller's
client enforces the captured origin and its own URL/header/body/concurrency caps.

`embed(&[&str], &SecretString, InferenceContext)` borrows the credential for this
call. The adapter never stores it. Nonempty calls require a nonempty usable bearer
header. Header construction uses a zeroizing temporary and a sensitive
`HeaderValue`; its bytes including the seven-byte prefix must fit
`request_json_bytes`. The endpoint/model also fit that cap. The public adapter and
result payloads have no diagnostic/serialization representation. The private
request type uses Serde serialization only to produce the wire body. The batch
exposes model, expected dimensions and vectors through explicit accessors; these
fields alone do not establish a storage compatibility identity.

`embed_guarded(texts, &HeaderMap, InferenceContext, &mut gate)` borrows configured
headers and an admission callback for the same finite future. Plain `embed` delegates to
that pipeline with an infallible gate. The SDK reuses `execute_guarded` for final
native send and response admission, then rechecks after decode and before return.
A denial discards every internal vector. `GuardedEmbeddingError<E>` retains the
actual typed SDK or admission cause and accumulated `NetworkEffect`; after any
batch may have been sent, later failures retain `MayHaveApplied`. No request,
key or provider response is printed by the guarded diagnostic. Guarded calls
accept empty or caller-configured sensitive headers and the SDK supplies its
protocol Content-Type. Convenience `embed` retains its default Bearer helper
after input/context validation; empty input returns without requiring a key.

Requests include `model`, `input` and `encoding_format: "float"`, with `dimensions`
only when explicitly requested. The lower SDK checks that optional width
structurally; it has no model-name whitelist. Expected width always comes from
the caller. The real Direct lease consumer owns the native reduction whitelist
for `text-embedding-3-small` (1536) and `text-embedding-3-large` (3072): positive,
smaller than native width and equal to expected width. Direct Compatible rejects
that extension before HTTP. The option never truncates input text. Provider token
budgets remain provider-side, rather than being guessed from UTF-8 bytes.

## Limits, execution and responses

`EmbeddingLimits::new` requires seven positive representable caps, in order:
maximum texts, total input UTF-8 bytes, dimensions, total vector values, batch
texts, request JSON bytes and response JSON bytes. Effective batch size is
`min(max_batch, 2048)`. Input counts, byte sums and count × width use checked
arithmetic before HTTP. These are logical retained-data limits, not an allocator
or operating-system memory ceiling. The caller must choose suitable transport
caps too: HTTP buffers its bounded response before the adapter checks its own
response-byte cap.

Every batch's JSON is preflight-encoded before the first request, so even an
oversized later encoded batch causes zero socket effects. Preflight discards each
zeroizing buffer; execution encodes one current batch at a time without cloning
all texts or keeping all JSON. Serde performs JSON encoding through a bounded
writer and parses responses through visitors/seeds that constrain retained row
counts and vector widths during decode. Recognized usage is retained; other extra fields are ignored without
retaining their values. Explicit fallible reservations report allocation errors.

The same borrowed Core cancellation signal and original absolute `std::time::Instant`
deadline govern preflight, every sequential HTTP batch and decode. There is no
new cancellation signal or per-batch timeout. Checkpoints occur per encoded write,
input, row and vector component, between batches and before return. Synchronous
bounded Serde work is cooperative, not scheduler preemption; parsing an ignored
field can finish before the next checkpoint. The existing transport observes the
original signal/deadline while waiting for headers or body and provides no
retry, redirect, proxy environment discovery or decompression.

Full success requires every expected vector in `data`, with exact-width finite
f32 components and an explicit unambiguous index. Rows are remapped to input
order. Only complete JSON through EOF can yield guarded partial results. Invalid
or duplicate indices invalidate every affected candidate, including duplicate
index fields within one object. Invalid/truncated JSON contributes no entries
from that response. Fully completed earlier batches remain retained; following
batches are not sent after failure. There is no retry or single-item salvage.
An empty input list returns an empty batch without HTTP after context/policy checks.

`GuardedEmbeddingError::Embedding` contains `source`, accumulated `effect`,
`completed: Vec<(usize, Vec<f32>)>` and `usage: Vec<Box<RawValue>>`. `Admission`
exposes neither partial vectors nor usage. Ordinary `embed` remains all-or-error.
Successful `EmbeddingBatch` also exposes `usage()` and `take_usage()`. Usage is
retained per completed provider response in observed order without invented
zero values or totals. Debug omits vectors, usage and credential payloads.

`EmbeddingError` preserves `Context(ContextError)` and `Network(NetworkError)`
with typed sources, or the actual non-success HTTP `StatusCode`. Other categories
are `InvalidPolicy`, `InvalidRequest`, `Credential`, `Limit`, `Codec`, `Shape`,
`NonFinite` and `Allocation`. Errors contain no endpoint/query, credential,
provider body, input text or model echo. Serde/vendor diagnostics are not retained
or logged. Non-success bodies are not decoded as embeddings.

## Verification scope

The ordinary inference tests run offline after the locked dependencies have
been fetched. They use private temporary directories, ephemeral loopback ports,
Cargo-built fixture programs and the pinned ACP SDK. They do not authenticate
a vendor account, discover an installed provider, download weights or require
an accelerator.

| Boundary | Ordinary tests | Explicit additional qualification |
| --- | --- | --- |
| Direct OpenAI/compatible, Cohere, Google and Voyage embeddings | Actual bounded HTTP clients against owned loopback peers; document/query encoding, limits, batching, cancellation, deadlines and causal errors | Authenticated vendor inference and provider-specific model quality |
| Cohere, Jina, Voyage and Graphit reranking | Actual HTTP clients, native scores and indices, service/lease admission and zero-send rejection | Authenticated provider inference |
| Model artifacts, bindings and native workers | Private synthetic artifacts, metadata/recipe validation, original request guards and real thread/channel/join lifecycle; no fabricated numerical inference | Pinned GGUF CPU inference, CUDA, long input and retained upstream numerical references |
| Codex, Claude, Cursor, Qwen, Grok, Antigravity, Kimi and Copilot CLI boundaries | Cargo-built or same-test-executable peers, actual subprocess cleanup and bounded protocol decoding | Installed and authenticated vendor executables |
| Kiro V3 and OpenCode 2 ACP | Genuine pinned SDK dispatchers with private channels; App Ask/Conversation adds the workspace's actual native ACP fixture processes | Authenticated vendor sessions and host integration |
| KnowledgeAsk and Conversation | Real Auth/Core/Knowledge stores, incoming credentials, progress, USER writers, source revalidation and joined service-owned children | External Broker deployment and vendor account behavior |

Run the package with
`cargo xtask cargo test --locked --offline -p graphit-ai --all-targets`.
The App protocol modules `knowledge_{agent}_ask` and
`knowledge_conversation` share the `integration` target. Use the same Xtask Cargo
wrapper with the corresponding `module::` filter for these journeys.
Their controlled wire peers use the Cargo-emitted `app-agent-fixture` binary
and raw-frame JSON scenarios, including intentionally malformed frames.
They do not execute Python source or shell launchers. Kiro, OpenCode and
Copilot reuse their existing workspace fixture executables. These App
journeys remain Linux-specific where they inspect real Unix process groups,
signals, permissions or `/proc`; portable AI protocol checks do not qualify
those App journeys on Windows or macOS.
Live's unit tests consume the absolute
`GRAPHIT_TEST_KIRO_KNOWLEDGE_FIXTURE` path emitted by that wrapper from Cargo's
compiler-artifact result. They neither infer a target directory nor consume a
binary left by another test. Default Kimi suites start their ignored peer entry
point themselves; that one entry point is not an omitted scenario.

CLI control uses readiness and ACKs under the same Original. Sequence peers
confirm a completed pipe write with an explicit probe/reply while retaining the
ACK barrier; only the ACK releases the next write or process exit. Streaming
checks require that reply after observing the callback and before its write's
ACK, including on Windows, and reject
EOF, unexpected control bytes, cancellation and expiry. They do not infer a live
blocked peer from a platform-specific empty-socket read result. Stalled HTTP bodies
remain gated until the caller returns its real error. Conversation concurrency
uses private per-turn gates and observed authorization, rather than a fixed
delay before assuming a second writer is waiting. Deliberate expiry tests still
wait for the original finite deadline or the credential's real expiration;
their waits are part of the behavior under test.

Real GGUF tests remain `--ignored --exact` qualifications. Embeddings require
`GRAPHIT_EMBEDDINGGEMMA2_MODEL`; BGE requires
`GRAPHIT_BGE_RERANKER_V2_M3_MODEL`, and reference comparisons additionally
require their declared captured reference. CPU and CUDA qualifications are
distinct. A qualification that initializes `NativeSdkRoot` must respect its
single initialization per process and keep it alive until its workers join;
the current guard rejects reinitialization even after drop. This is a backend
lifetime boundary, not a requirement to serialize independent invocations or
ordinary parallel tests. An ignored throughput measurement only
measures its local harness, not a provider or a production SLA.

Strict lint uses the same wrapper and locked offline dependencies:

```text
cargo xtask cargo clippy --locked --offline --no-deps -p graphit-ai --all-targets -- -D warnings
cargo xtask cargo clippy --locked --offline -p graphit-app --all-targets --no-deps -- -D warnings
```

The App checks cover captured authentication Originals, provisioning,
ACP/Kiro process ownership and BrowserLive. Native Live cases use the module
filter `knowledge_live::tests::native::` and cover queued cancellation.
BrowserLive exercises the actual main stdio/HTTP consumer and requires a
completed outcome, joined work and owned cleanup. Its four AST calls request
`ai_optimized:false`: the Live HTTP adapter consumes that presentation flag
before parsing and retains JSON `structured_content`, including Source's
`selection.source`. This endpoint's result is distinct from Root AST Source's
plain-text presentation.

Run the OpenCode plugin's native Node fixture with
`node --test scripts/test-opencode-plugin.mjs`. Its TCP readiness barrier
observes the actual `ChildProcess` returned by `execFile`. A connection reset is
accepted only after its owner has successfully sent a kill signal to the identified
peer; cleanup then awaits the child's real `close` event. Earlier resets and other socket errors fail
the fixture. The peer flushes partial output before readiness and receives an
explicit ACK; cancellation, deadlines and output checks retain their real plugin
oracles. Windows process and TCP termination require the native Windows CI run.

Auxiliary ACP and Kiro process tests reuse the same Cargo fixture's raw-process
scenario. It can retain actual stdin, pipes and a child of its own executable
without decoding a product prompt. PID/readiness, release files and observed
TERM markers establish custody; Unix group and Windows Job/EOF assertions remain
the retirement evidence. The Kiro SDK fixture also has an explicit
`answer_exit_gate` case for a complete response followed by a retained,
TERM-resistant child; the journal records actual TERM and driver-close events.
KnowledgeQuestion's dedicated fixture case reads its actual stdin in the child,
checks the literal selected Paris body and derives its answer from that admitted
prompt. Constructing a request for authorization does not parse an empty parent
stdin or fabricate the later native response. Cancel/deadline cases retain the
child after `turn.started` until the actual owner retires it.

The absolute batch-deadline oracle keeps first/second bodies at 600/600ms
with an original deadline of 1s. Total arrival around 1.2s falls before a renewed
HTTP deadline around 1.6s. Expected `Network(Deadline)` distinguishes renewal,
which would deliver the body and yield `Context(Deadline)` in the adapter.
The scheduling margin adds no elapsed-time SLA.

Run the direct integration suite with
`cargo xtask cargo test --locked --offline -p graphit-ai --test integration direct_embeddings::`.
It uses real loopback TCP and the real bounded client: exact request,
credential, model and extension rules, shuffled indices, batch splitting,
zero effects, malformed shapes and f32 overflow, real status/transport failures,
stalled-body cancellation, original deadlines and failure after a successful
batch. It does not use a fake transport or external account. Keep the existing
pins; strict Clippy covers all crate targets.

`measure_direct_loopback_fixture` is an ignored local Linux measurement fixture.
Its wall/p95 and process CPU/RSS include the debug harness, local TCP server and
cleanup. These measurements do not establish authenticated provider latency,
product throughput, a performance gain or a production SLA.

The [Direct service-lease consumer](rust_direct_embedding_leases.md) covers the
shared guarded SDK and captured Auth composition. SDK and lease fixtures prove
their own boundaries; authenticated vendor inference, installed host behavior
and each additional platform require their explicit qualifications.
