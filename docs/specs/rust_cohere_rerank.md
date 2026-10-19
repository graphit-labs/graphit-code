---
title: Rust Cohere native reranking
type: spec
status: linux-library-qualified
updated: 2026-10-18
tags: [rust, ai, cohere, rerank]
references:
  - target: {type: task, id: tsk-ea46}
    relation: relates_to
  - target: {type: task, id: tsk-c976}
    relation: implements
  - target: {type: task, id: tsk-7b3b}
    relation: implements
  - target: {type: task, id: tsk-ad37}
    relation: derived_from
  - target: {type: task, id: tsk-72bc}
    relation: uses
  - target: {type: task, id: tsk-56cb}
    relation: uses
  - target: {type: task, id: tsk-9f12}
    relation: relates_to
  - target: {type: task, id: tsk-b559}
    relation: relates_to
---

# Rust Cohere native reranking

`CohereReranker` scores a query against documents using the caller's existing bounded HTTP client. The Rust library implementation is qualified by local TCP fixtures on Linux. Application selection, credentials and authenticated cutover are owned by their integration callers.

## API and request

`CohereReranker::new(&BoundedClient, endpoint, model, NativeRerankLimits)` borrows the configured client, full endpoint and model. Empty endpoint/model or an incompatible configured bound returns a typed error. The constructor does not create another HTTP pool or choose a model implicitly.

`score(query, documents, &SecretString, InferenceContext)` returns one finite `f64` score per input document in original order. Each POST serializes `model`, `query`, `documents` and `top_n` equal to that batch's document count. The expected response is `results` containing `index` and `relevance_score`; a different or incomplete envelope is an error.

Batches use the configured `max_batch`. The adapter does not impose an additional 200/1000 document cap or truncate the input set. Scores preserve precision, negative values and signed zero. Feed the complete vector into the existing `rank_order` API with the caller's indices.

`score_guarded(query, documents, &HeaderMap, context, gate)` borrows explicit
configured authentication headers, including custom headers or an empty map.
The protocol controls Content-Type; an override is rejected, header values are
bounded and marked sensitive by the shared pipeline. The callback retains its
typed admission failure and accumulated NetworkEffect. The active SecretString
`score` API retains Bearer validation and builds that header after preflight;
Auth Direct composes its own map from the validated lease before SDK entry.
The guarded seam is reused by the Broker rerank consumer without an Auth Code
lease or a credential cache. See the [shared wire](rust_native_rerank_wire.md)
for the separate ea46 affected integration evidence and its scope.

## Validation, execution and errors

The adapter validates candidate count and total UTF-8 bytes, counting the query once. It preflights every batch's escaped JSON before the first HTTP request. A valid empty candidate list returns an empty vector without requesting credentials or HTTP.

Execution reuses the [private wire pipeline](rust_native_rerank_wire.md), with zeroizing request ownership and sensitive Bearer headers. Each batch retains the original cancellation signal and absolute deadline. HTTP-client and adapter response caps both apply.

The response must contain every unique in-range batch index exactly once. Missing/null/duplicate fields, invalid indices, nonfinite scores, malformed/trailing JSON and wrong result cardinality fail. Scores are remapped within the batch before concatenation, preserving global document order. There is no default score, partial success or retry. A late error drops the accumulated vector and prevents another request.

`NativeRerankError` preserves context, network and HTTP status causes plus policy, limit, encoding, shape and allocation classifications. Displays do not expose the query, documents, key or response payload. Synchronous Serde work has ordinary context checks, without a hard preemption or RSS promise.

## Verification and scope

`tests/cohere_reranking.rs` uses the actual shared TCP fixture `tests/support/rerank_http.rs`. The recorded affected run passed 8 Cohere and 2 native tests. Cases cover exact schema/Bearer, explicit batches, full-set `f64` remapping/ranking, strict response errors, byte bounds and late preflight, 401/429/503/refused connections, stalled cancellation, one absolute deadline across batches, and late failure stopping further requests. The additional measurement is ignored in that run.

Evidence and invocation are documented in [the wire pipeline](rust_native_rerank_wire.md#affected-verification). Prior operation measurements and historical guard evidence remain scoped to their original Task results; they do not prove current provider latency, throughput, RSS limits, other operating systems or authenticated cutover. Decisions 566/570 retired the former per-leaf matching-hash guard requirement; it is not an adapter prerequisite.
