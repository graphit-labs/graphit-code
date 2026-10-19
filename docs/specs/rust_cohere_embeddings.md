---
title: Rust Cohere text embeddings
description: Explicit positional Cohere embeddings with bounded transport and the original execution context.
type: specification
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target: {type: task, id: tsk-174e}
    relation: implements
  - target: {type: task, id: tsk-3350}
    relation: uses
  - target: {type: task, id: tsk-099a}
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

# Rust Cohere text embeddings

For library consumers integrating direct text retrieval embeddings. The exported
source is `crates/graphit-ai/src/cohere_embedding.rs`; its TCP suite is
`crates/graphit-ai/tests/cohere_embeddings.rs`. The
[Direct lease consumer](rust_direct_embedding_leases.md) composes this SDK with
Auth's captured service binding, credential and original request. Profile capture
and service authentication remain with Auth. Storage/search, other providers and
other service modes require their own integration.

## Captured inputs and wire

The `CohereEmbeddings::new` API borrows an existing
`graphit-http::client::BoundedClient`, a full endpoint and a model. It also
requires the positive expected width and the existing `EmbeddingLimits`.
No model-name width table, environment discovery, profile lookup or new pool is
introduced. Empty endpoint/model and zero width fail with `InvalidRequest`;
width beyond the dimension cap and endpoint/model beyond the request byte cap
fail with `Limit`. Endpoint validation and origin authorization remain with the
real transport.

`embed(texts, CohereInput, &SecretString, InferenceContext)` borrows its
credential per call. `Document` selects `search_document`; `Query` selects
`search_query`. The adapter retains no key. A nonempty call requires a usable
Bearer header. The seven-byte prefix plus key must fit the request byte cap;
construction uses a zeroizing temporary and a sensitive `HeaderValue`.
Public adapter/result types have no Debug or Serde payload representation.

`embed_guarded(texts, CohereInput, &HeaderMap, InferenceContext, &mut gate)` accepts
borrowed admission and configured sensitive headers, including an empty map. Both entrypoints share one
encoder/decoder/preflight/batching pipeline. The ordinary `embed` supplies an
infallible admission callback; it retains its typed `EmbeddingError` result.
Guarded calls check context before and after admission, during preflight, at each
batch, after decoding and before returning. Each batch passes that callback to
the existing HTTP client's `execute_guarded`, including transport send and body
await boundaries. Empty guarded input checks admission and returns without HTTP. Convenience
`embed` retains its default Bearer helper after input/context validation; empty
input returns without requiring a key.

The private Serde request emits exactly `model`, `texts`, `input_type` and
`embedding_types: ["float"]`. It omits `output_dimension`. This preserves the
native-generation contract of the current direct adapter rather than assuming
an extension or reducing output. Text batches contain at most
`min(EmbeddingLimits.max_batch, 96)` entries, matching the
[official Cohere embed API](https://docs.cohere.com/v2/reference/embed), inspected
on 2026-10-09. Rich inputs, images and additional task types are outside this
text retrieval contract. UTF-8 bytes do not establish provider token budgets;
provider rejection remains a real HTTP status.

## Positional results, bounds and context

`embeddings.float` is required and positional. There must be exactly one vector
per batch text, in the same order, and exactly the captured number of finite
`f32` components per vector. No index field is required or reconstructed.
Duplicate required fields, missing fields, invalid structure/cardinality/width,
malformed JSON, trailing content and numeric overflow fail. Observed usage is
retained; other additional fields are ignored without retention.

The existing [embedding limits and batch API](rust_inference.md) apply: positive
representable caps for text count, total input UTF-8 bytes, dimension count,
total vector values, batch size, request JSON bytes and response JSON bytes.
The Cohere provider further caps batch size at 96. Count × width and byte sums
use checked arithmetic. Reservations are explicit and fallible. These caps
bound logical retained data, not operating-system RSS or allocator overhead.
The existing transport buffers its bounded body before the adapter checks the
adapter-specific response cap; consumers must also choose suitable HTTP caps.

Every chunk is preflight-encoded before the first HTTP request. Each temporary
zeroizing body is discarded, and execution encodes only its current chunk.
An oversized later JSON body therefore causes zero socket effects without
retaining all encoded copies. A fallible bounded writer checks geometric growth
against the request cap. Serde seeds bound vector count and width during decode,
before excessive retained allocation.

The same borrowed Core cancellation signal and absolute `std::time::Instant`
deadline govern preflight, encoding, all sequential requests and decoding.
Checks occur per input/encoded write/row/component, between batches and before
return. No new signal or per-batch timeout extends the request. Synchronous
bounded Serde work is cooperative; parsing an ignored field can finish before
the next check. The reused HTTP client observes the original context while
waiting for headers/body, with its captured origin, concurrency and byte policy.
No adapter retry, redirect, decompression or fallback is introduced.

Success returns the existing `EmbeddingBatch` with model, width and vectors
available through explicit accessors. Its metadata does not establish storage
compatibility identity. Empty input returns an empty batch without HTTP after
context/policy validation; cancellation or expiry remains an error. Ordinary
`embed` is all-or-error. A guarded failure can retain finite exact-width siblings
from a complete positional response only when cardinality is exact; a count
mismatch clears candidates from that response. Following requests stop.

## Errors and recovery

The existing `EmbeddingError` preserves `NetworkError`, `ContextError` and real
non-success HTTP `StatusCode`. Configuration, credential, cap, codec, shape,
non-finite and allocation failures remain typed. Errors contain no body, URL,
query, key, header, model or input payload. The consumer must distinguish policy
or authorization correction from deadline/cancellation or provider failure;
the adapter never converts an error to empty/default vectors or retries it.

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
099a and c775 and their qualification receipts. Their compiler-isolation,
hash or measurement campaigns are not gates for the current integration.
Source-to-document coverage follows constructor/input/header/encoder, decoder,
context, guarded HTTP and batch result; the documented Direct composition uses
those same entrypoints and its real lease fixtures.

The subsequent tsk-170b providers04 run passed all five affected suites. The Google
optional-dimension focal passed one test in 0.55 s across both families and
single/batch routes; the bounded wire suite passed three tests. These focused
results do not qualify the entire Broker composition or additional platforms.
