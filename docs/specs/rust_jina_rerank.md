---
title: Rust Jina reranking adapter
type: spec
status: linux-library-qualified
updated: 2026-10-18
tags: [rust, ai, rerank, jina, limits]
references:
  - target: {type: task, id: tsk-ea46}
    relation: relates_to
  - target: {type: task, id: tsk-0e1b}
    relation: implements
  - target: {type: task, id: tsk-c976}
    relation: uses
  - target: {type: task, id: tsk-7b3b}
    relation: derived_from
  - target: {type: task, id: tsk-ad37}
    relation: derived_from
---

# Rust Jina reranking adapter

This reference describes the text-only Jina adapter in graphit-ai for maintainers and library consumers. Its public module is integrated and the affected Main Linux suite passed ten real TCP tests, with fmt and strict affected Clippy passing. Loopback tests do not prove authenticated Jina service execution or application profile cutover.

## Call contract

The caller creates JinaReranker with its existing borrowed BoundedClient, full endpoint, model and NativeRerankLimits. No HTTP pool, credential store or model discovery is created. The client, endpoint and model must outlive the adapter; the SecretString key is borrowed separately for each score call.

JinaReranker::new(client, endpoint, model, limits) returns a policy error for empty endpoint/model and a limit error for configuration exceeding the request cap. score(query, documents, key, context) returns one finite f64 score per input document in the original order, or NativeRerankError. An empty list with valid context returns [] without serialization, credential validation or HTTP. Cancellation or deadline expiry still rejects an empty call.

The consumer may pass the complete output to the existing rank_order with its own candidate indices. rank_order returns input positions; descending scores are followed by caller index and input position for ties. Negative scores and signed zeros are valid. No score normalization, f32 conversion or calibration is applied.

## Wire and batching

The primary contract is [Jina OpenAPI](https://api.jina.ai/openapi.json), version 2026.10.09.2011, verified on 2026-10-10. Each POST to the captured /v1/rerank endpoint contains model, query, text documents, top_n equal to the chunk size, and return_documents=false explicitly. This suppresses the default document echo. The adapter omits return_embeddings and additional options.

NativeRerankLimits.max_batch is the caller's positive batching policy. The current document schema has minItems=1 and no demonstrated maxItems; the adapter adds no hidden 200/1000 document cap. Jina v3/v3.5 listwise scores depend on the candidates presented together. Splitting into batches can change those scores, so concatenated batches do not promise equivalence to a single full-list request, or calibration across batches, models or providers. UTF-8 input bounds do not establish token coverage or service truncation behavior.

The response must contain results[].index and relevance_score. Every local index must occur exactly once, within the chunk, with a finite f64 score. The common decoder remaps the response to input order before chunk concatenation. Missing/null/duplicate known fields, invalid indices/counts, non-finite or overflowing numbers, malformed/trailing JSON and the wrong envelope fail. Unknown fields are skipped with Serde under the response cap; there is no alternate-envelope fallback.

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

## Work limits, context and errors

The adapter validates max_candidates and the UTF-8 bytes of the query once plus every document, using the original InferenceContext. It preflights every chunk's real serialized JSON before opening the first socket. If escaping or another later chunk exceeds max_request_json, zero HTTP requests are made, even when the key is invalid.

Execution then encodes and submits one chunk at a time through the existing private common pipeline. Buffers use Zeroizing and fallible allocation; the adapter does not claim control over copies inside dependencies. The same cancellation reference and absolute Instant deadline reach every request through NetworkRequestContext; batching never renews a deadline. The BoundedClient retains its request/response caps, and the adapter separately enforces its JSON caps.

NativeRerankError retains typed context, network, HTTP status, policy, limit, encoding, shape and allocation failures. A successful first chunk followed by a failure discards accumulated scores and stops subsequent requests, with no retry or partial output. Error displays omit credentials, URLs, query and document text. Synchronous encoding/decoding checks context at actual boundaries; these input bounds are not a process RSS guarantee or hard CPU preemption.

## Source and affected verification

- crates/graphit-ai/src/jina_rerank.rs: adapter and typed request.
- crates/graphit-ai/src/rerank_wire.rs: shared encoding, transport, configured-header seam and strict results decoder.
- crates/graphit-ai/tests/jina_reranking.rs: real loopback TCP requests, response and failure fixtures.
- crates/graphit-ai/tests/support/rerank_http.rs: existing shared TCP server.

The affected invocation is:

~~~text
cargo test --locked -p graphit-ai --test jina_reranking
~~~

The suite checks exact request/defaults/per-call Bearer, batching without hidden caps, complete remapping and f64 ranking, empty/preflight zero socket effects, malformed/incomplete/duplicate/wrong-envelope responses, exact/+1 UTF-8/request/response caps including HTTP client caps, 401/429/503/refused/origin errors, pre-cancel/expiry, stalled cancellation and absolute deadlines, late failure without a third request, and diagnostic redaction. The Main Linux run passed all ten cases with zero failures or ignored tests. Cargo reported 12.39 s for the warm build and the harness reported 1.93 s; observed total wall time was 14.476 s. Strict affected Clippy completed with exit 0 in 6.63 s reported by Cargo (7.012 s wall), and scoped rustfmt --check passed. These are validation costs, not authenticated provider latency or a performance benchmark. Logs are in .build/rust-ai/jina-rerank/main-tests.log and main-clippy.log; native/Cohere source and their shared fixture were not changed.

The coordinator additionally reported ten Jina Main tests passing after the common Data envelope was added for Voyage, within its affected regression run. Jina source and its Results selection were unchanged. That additional run is coordinator evidence, separate from this leaf's measured ten-case qualification.

Related references: [shared wire pipeline](rust_native_rerank_wire.md), [native score validation](rust_native_reranking.md).
