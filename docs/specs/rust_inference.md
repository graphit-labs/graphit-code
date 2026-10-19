---
title: Rust direct inference library
description: Explicit OpenAI-shaped embedding requests with bounded transport, vectors and execution context.
type: specification
updated: 2026-10-17
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

The suite/Clippy/measurement receipts below are historical qualification of the
previous fixture revision. A later static review found that the second-batch
deadline oracle also accepted renewal of only the HTTP deadline. The prepared
correction keeps first/second bodies at 60/60ms with original deadline100ms: total
arrival around120ms falls before a renewed HTTP deadline around160ms. Expected
`Network(Deadline)` then distinguishes renewal, which would deliver the body and
yield `Context(Deadline)` in the adapter. The adapter/API remains unchanged.
The corrected case passed in the new coordinated Linux CPU window on
2026-10-09: nine functional tests passed, including the 60/60ms oracle, and one
measurement was ignored. Strict Clippy passed. The Root guard proof for this revised direct suite passed in `tsk-b96c`
revision20; historical receipts below remain separate from the revised test.

The affected integration suite is
`cargo test --locked -p graphit-ai --test direct_embeddings`. It uses real loopback
TCP and the real bounded client: exact request/credential/model/extension rules,
shuffled indices, batch splitting, zero effects, malformed shapes and f32 overflow,
real status/transport failures, stalled-body cancellation, original deadlines and
failure after a successful batch. It does not use a fake transport or external
account. Run with the coordinator's CPU window and existing pins; strict Clippy
covers all crate targets. The existing ranking suite must remain valid after the
new exports/dependencies.

`measure_direct_loopback_fixture` is an ignored precompiled Linux fixture with
128 calls of eight texts and two dimensions. Its wall/p95 and process CPU/RSS
measure the local debug transport/JSON/harness, not authenticated inference,
provider latency, product throughput or improvement over Go. On 2026-10-09 the coordinated Linux x86_64 run passed nine functional tests;
the one measurement fixture was ignored in that suite. Strict Clippy over all
crate targets also passed. The first compile failed with E0502 in the writer's
reservation call; computing the additional capacity before borrowing the buffer
fixed it, and the actual suite rerun passed. The original failure is preserved in
`.build/rust-ai/direct-tests-attempt-01.log`; successful suite and Clippy logs/time
receipts are `direct-tests-attempt-02` and `direct-clippy-attempt-01` in that folder.
The suite command took 2.58 s including compilation, with 562728 KiB peak RSS;
these are compiler/harness observations, not embedding-operation measurements.

The precompiled fixture passed separately: 128 calls totaled 56872731 ns
(56.872731 ms), with p95 470534 ns (0.470534 ms). Process wall time was 0.11 s,
CPU user/system 0.07/0.01 s and peak RSS 11540 KiB, including the debug test
harness, local server and its cleanup. The fixture log/time receipts are
`.build/rust-ai/direct-fixture-attempt-01.{log,time}`. These numbers do not measure
an external provider or establish product performance. The historical run preceded the coordinator's focused hook qualification; the
current revised-suite proof is recorded below. Windows/macOS runtime, external inference
and consumer integration are unverified.

The corrected suite used `cargo test --locked -p graphit-ai --test
direct_embeddings -- --test-threads=2`, jobs2/incremental0. Suite command wall
was 3.19 s including compilation, CPU user/system 2.29/0.63 s, peak RSS
565728 KiB; this measures compiler/harness, not embedding operations. Strict
Clippy for all AI targets took 1.17 s, CPU 1.26/0.40 s, peak RSS 206844 KiB.

Its separate precompiled fixture passed 128 calls × eight texts × two values:
total 85150726 ns (85.150726 ms), sorted-sample p95 1058899 ns (1.058899 ms).
Process wall was 0.15 s, CPU user/system 0.10/0.02 s, peak RSS 11740 KiB,
including the debug harness, local TCP server and cleanup. New raw logs/time and
hashes are under `.build/rust-ai/qualification-2026-10-09/`:
`direct-tests-attempt-01`, `direct-fixture-attempt-01`, `clippy-attempt-01` and
`execution-receipt.json`. These local results do not establish authenticated
provider latency, product throughput or a performance gain. Source/API and own
manifest hashes remained unchanged after execution. The dedicated direct-suite
Root guard qualification passed below; consumer integration remains a separate
delivery.


## Dedicated direct-suite Git guard qualification

`tsk-b96c` completed revision20 with six active checks PASS and final independent
Tracker NOFAULT. Its one actual grouped command was:

```text
cargo xtask verify-fast-guards graphit-ai/direct_embeddings graphit-runtime/roles graphit-knowledge/staleness
```

Check `chk-413ea8ae7f4c` records compiled isolation1, baseline/restored positive
checks, and an actual FAILED/panic marker plus nonzero Git pre-commit outcome
for each of the three exact targets. Original source bytes were restored,
freeze had zero mismatches and scratch was absent. This proves the matching
`direct_embeddings` guard; it does not qualify Cohere, Voyage, Google, native
rerank, the full native gate or product integration.

The grouped command took 177.025779242 s. Raw log
`.build/rust-kernel-fast-guard-2026-10-09/attempt-2-profile-input-closure/selected-kernel-guards.log`
has SHA-256 `fc1d95d8aadc6665f18b18adcc743f66bd76cd3fa0e62d5fed46128db95cfdab`.
This is a combined isolated compilation/Git/harness measurement, not an embedding
operation cost. The preceding root fast check took 8.80746917 s and observed 143
PASS executions plus two ignored tests across 17 harness rows, rather than 143
unique cases. Its raw hash is
`d48b14b60f2f8d9d7835adc06f9b46ac2c1c4e29d40e11d0ecd071b4c59e49af`.
The first failed isolation attempt is preserved by b96c and is not a successful
Git proof. Only Linux was exercised; Windows/macOS and authenticated inference
remain unverified. No shared manifest, lockfile or guard source was changed by
the direct-provider owner for this completion.


## Historical guarded consumer qualification

The shared guarded SDK and real captured Auth composition are exercised in
`tsk-0f32`. Its affected Linux run passed all 41 functional tests (including
10 tests in this SDK suite), with two historical measurement fixtures ignored. Selected strict Clippy
also passed for the library and all four affected test targets.
The [Direct lease qualification](rust_direct_embedding_leases.md#qualification)
records the command, scope and actual queue/build/harness costs. No live vendor
authentication, product factory, Broker/Local integration or Windows/macOS
execution is inferred from these local fixtures.

The subsequent tsk-170b providers04 run passed all five affected suites. The Google
optional-dimension focal passed one test in 0.55 s across both families and
single/batch routes; the bounded wire suite passed three tests. These focused
results do not qualify the entire Broker composition or additional platforms.
