---
title: Rust Voyage text embeddings
description: Native-width Voyage embeddings with indexed responses, bounded transport and original execution context.
type: specification
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target: {type: task, id: tsk-174e}
    relation: implements
  - target: {type: task, id: tsk-3350}
    relation: uses
  - target: {type: task, id: tsk-521f}
    relation: implements
  - target: {type: task, id: tsk-ad37}
    relation: derived_from
  - target: {type: task, id: tsk-56cb}
    relation: uses
  - target: {type: task, id: tsk-55ac}
    relation: derived_from
  - target: {type: task, id: tsk-72bc}
    relation: derived_from
  - target: {type: task, id: tsk-c775}
    relation: validated_by
---

# Rust Voyage text embeddings

For library consumers integrating direct text retrieval embeddings. The exported
source is `crates/graphit-ai/src/voyage_embedding.rs`; its TCP suite is
`crates/graphit-ai/tests/voyage_embeddings.rs`. The
[Direct lease consumer](rust_direct_embedding_leases.md) composes this SDK with
Auth's captured service binding, credential and original request. Profile capture
and service authentication remain with Auth. Storage/search, other providers and
other service modes require their own integration.

## Inputs, requests and native width

The `VoyageEmbeddings::new` API borrows the caller's existing
`graphit-http::client::BoundedClient`, full endpoint and model. It requires an
explicit positive expected native width and the existing `EmbeddingLimits`.
Endpoint/model must be nonempty and fit the request byte cap; the width must
fit the dimension cap. No model-name table, profile/environment discovery or
new pool supplies missing values. The transport enforces its captured origin,
URL/header/body/concurrency policy.

`embed(texts, VoyageInput, &SecretString, InferenceContext)` takes an explicit
mode per call: `Document` emits `document`, and `Query` emits `query`. The Bearer
key is borrowed and never retained by the adapter. For nonempty input it must be
usable; header bytes including the seven-byte prefix fit the request byte cap.
Construction uses a zeroizing temporary and a sensitive `HeaderValue`. Public
adapter and batch payloads have no Debug or Serde representation.

`embed_guarded(texts, VoyageInput, &HeaderMap, InferenceContext, &mut gate)` accepts
borrowed admission and configured sensitive headers, including an empty map. Both entrypoints share one
encoder/decoder/preflight/batching pipeline. The ordinary `embed` supplies an
infallible admission callback; it retains its typed `EmbeddingError` result.
Guarded calls check context before and after admission, during preflight, at each
batch, after decoding and before returning. Each batch passes that callback to
the existing HTTP client's `execute_guarded`, including transport send and body
await boundaries. Empty guarded input checks admission and returns without HTTP. Convenience
`embed` retains its default Bearer helper after input/context validation; empty
input returns without requiring a key.

POST targets the supplied complete `/v1/embeddings` endpoint. The private Serde
body contains exactly `model`, `input`, `input_type` and `output_dtype: "float"`.
Both `encoding_format` and `output_dimension` are omitted. Generation stays at
the provider's native width, which is validated against the consumer's supplied
expectation. No reduction/extension or guessed default is requested.

Each sequential chunk contains `min(captured max_batch, 1000)` texts, matching
the [official Voyage embeddings API](https://docs.voyageai.com/reference/embeddings-api)
inspected on 2026-10-09. A lower captured batch size is enforced. UTF-8
byte limits do not establish model-specific token budgets. Provider rejection
remains a real HTTP status.

## Indexed response and execution

Full success requires `data` to contain exactly one item per chunk input. Every
item requires integer `index` and `embedding`. Indices are local to that chunk,
unique, complete and in range. Rows may arrive out of order; the adapter remaps
by index, then concatenates chunks in original input order. It never associates
by arrival position, fills a missing index with zero or accepts duplicates.
Each vector has exactly the captured width and finite `f32` values. Numeric
overflow, malformed/trailing JSON, missing/null/duplicate fields and invalid
count/index/width fail. Observed usage is retained; other valid extra fields are
ignored without retaining their payload.

The [common embedding budgets and batch API](rust_inference.md) apply: positive
representable caps for text count, total UTF-8 input bytes, dimensions, total
vector values, batch texts and request/response JSON bytes. Checked arithmetic
validates byte sums and count × width before HTTP. A fallible bounded writer
limits geometric allocation during encoding. Serde seeds constrain retained
row count and width while parsing; explicit reservations report allocation
failure. Logical retained-data caps do not promise an allocator or OS RSS
ceiling. The transport buffers its bounded body before the adapter applies its
own response cap, so consumers must also choose suitable HTTP limits.

Every chunk is preflight-encoded before any network effect. Preflight discards
each zeroizing body; execution encodes one current chunk at a time. If a later
prepared body exceeds the request cap, no socket is opened. Every request and
decode use the same Core cancellation signal and original absolute
`std::time::Instant` deadline. There is no replacement signal or renewed
per-batch timeout. Checks occur per input, encoded write, row, component, between
chunks and before return. Synchronous bounded Serde work is cooperative; an
ignored field can finish parsing before the next check.

The reused transport preserves its original context while waiting for headers
or body. The adapter adds no retry, redirect, decompression, parallel transport
or fallback. Success returns the existing `EmbeddingBatch` with explicit
model/dimension/vector accessors; that metadata alone is not storage
compatibility identity. Empty input checks context/policy and returns an empty
batch without HTTP. Expiry/cancellation remains an error. Ordinary `embed` is
all-or-error. Guarded failures can retain complete-response vectors with
unambiguous explicit indices. Duplicate indices, including duplicate index fields
in one object, invalidate every affected candidate. Invalid/truncated JSON
contributes no current-response entries. Earlier completed batches remain retained;
arrival order never supplies a missing index. Following requests stop.

## Errors and recovery

The existing `EmbeddingError` preserves real `NetworkError`, `ContextError` and
non-success HTTP `StatusCode`. Configuration, credential, cap, codec, shape,
non-finite and allocation errors remain typed. Diagnostics contain no key,
header, body, endpoint/query, input or model. The consumer distinguishes policy
correction, authorization failure, deadline/cancellation and provider failure;
this adapter does not retry or turn errors into empty/default vectors.

`GuardedEmbeddingError<E>` separates `Embedding { source, effect, completed,
usage }` from `Admission { source, effect }`. Embedding errors expose validated
complete-response vectors and observed usage; admission exposes neither. Both
retain typed causes. Debug omits payloads. `NotSent` applies before any possible
send; `MayHaveApplied` remains after any batch may have reached the service.
Only complete JSON through EOF supplies partial results; earlier completed batches
remain available. No retry or single-item salvage is performed. The common
[embedding contract](rust_inference.md) defines accessors and observed usage.

## Qualification scope

The provider TCP suite exercises native request/response contracts, exact width,
finite values, batching, malformed output, context and transport limits. The
Direct lease suite adds real AuthStore/RoleIssuer composition and physical
profile invalidation. Current commands and results are recorded in
[Direct lease qualification](rust_direct_embedding_leases.md#qualification).
These local fixtures do not establish authenticated vendor inference, provider
latency, RSS gains or Windows/macOS qualification.

Historical provider and Git-guard results remain in Tasks
521f and c775 and their qualification receipts. Their compiler-isolation,
hash or measurement campaigns are not gates for the current integration.
Source-to-document coverage follows constructor/input/header/encoder, decoder,
context, guarded HTTP and batch result; the documented Direct composition uses
those same entrypoints and its real lease fixtures.

The subsequent tsk-170b providers04 run passed all five affected suites. The Google
optional-dimension focal passed one test in 0.55 s across both families and
single/batch routes; the bounded wire suite passed three tests. These focused
results do not qualify the entire Broker composition or additional platforms.
