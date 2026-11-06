---
title: Rust Voyage native reranking
type: spec
status: linux-library-qualified
updated: 2026-10-18
tags: [rust, ai, voyage, rerank]
references:
  - target: {type: task, id: tsk-ea46}
    relation: relates_to
  - target: {type: task, id: tsk-9044}
    relation: implements
  - target: {type: task, id: tsk-c976}
    relation: uses
  - target: {type: task, id: tsk-7b3b}
    relation: implements
  - target: {type: task, id: tsk-ad37}
    relation: derived_from
---

# Rust Voyage native reranking

`VoyageReranker` scores a query against documents on the caller's existing bounded HTTP client. The caller selects the full endpoint and model and supplies credentials for each call. The adapter creates no HTTP pool and retains no credentials.

## API and wire protocol

`VoyageReranker::new(&BoundedClient, endpoint, model, NativeRerankLimits)` borrows those configured values. Empty endpoint or model returns a policy error; values larger than the configured request bound return a limit error.

`score(query, documents, &SecretString, InferenceContext)` returns one finite `f64` score per input document in original order. Each POST sends `model`, `query`, `documents`, `top_k` equal to the chunk length, and `return_documents: false`. It omits `truncation`, preserving the provider's default of true. The expected response envelope is `data` with required `index` and `relevance_score` fields.

The [Voyage reranker API](https://docs.voyageai.com/reference/reranker-api), checked on 2026-10-10, permits at most 1,000 documents per request. Chunks therefore use `min(max_batch, 1000)`; larger input is split without dropping documents. Configured total candidate limits still apply. Token limits vary by selected model and remain provider enforcement: UTF-8 byte limits are not token estimates, and provider rejection remains an error.

Every chunk requests all its scores. Response indices are local to that chunk and are remapped into input order before accumulation. Negative scores and signed zeros are valid; finite values retain `f64` precision. The existing `rank_order` consumer sorts descending scores, then ascending caller index and original position, with signed zeros tied.

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

## Limits, context and failure behavior

The adapter validates total candidate count and UTF-8 input bytes and serializes every chunk within the configured JSON cap before the first HTTP request. Valid empty input returns an empty vector without credential validation or network work. Cancelled or expired empty calls still fail.

The adapter reuses the private [native rerank wire pipeline](rust_native_rerank_wire.md). Bodies use zeroizing temporary buffers and authorization headers are marked sensitive. Credentials are borrowed per call, so a reused adapter can receive a different key on its next call.

Every chunk uses the original cancellation signal and absolute deadline. The adapter creates no retry or deadline renewal. The bounded HTTP client's own request/response limits and origin policy remain in force; the adapter additionally checks its JSON response cap.

Malformed JSON, missing/null or duplicate required fields, wrong envelope, invalid or duplicate indices, nonfinite scores, incomplete or excess cardinality and trailing JSON fail. Bounded unknown metadata is ignored through Serde. There is no zero filling or successful partial result. If a later chunk fails, accumulated scores are discarded and subsequent chunks are not requested.

Context, network, HTTP status, policy, limits, encoding, shape and allocation failures remain typed. Context and network errors preserve their sources; safe diagnostic text excludes credentials, endpoint, model, input and provider response bodies.

## Batching and comparability

The adapter preserves document identity and all returned scores, but does not promise numerical equivalence between a single provider request and multiple chunks. It also makes no score calibration promise between different models or providers. Applications own their model selection and ranking policy.

## Affected verification

`crates/graphit-ai/tests/voyage_reranking.rs` uses actual loopback TCP requests on the existing `BoundedClient` and shared fixture. Its cases cover exact POST/Bearer/fields, per-call credentials, 1,000/1,001 documents with a larger configured batch and smaller configured chunks, input-order remapping, empty/preflight zero HTTP, strict response rejection, UTF-8/JSON boundaries, real 401/429/503 and transport errors, cancellation and the original cross-batch deadline, late failure stopping further requests, signed zeros and the existing ranking consumer.

Run the affected modules with the canonical workspace configuration:

```text
cargo xtask cargo test --locked -p graphit-ai --test integration voyage_reranking::
cargo xtask cargo test --locked -p graphit-ai --test integration cohere_reranking::
cargo xtask cargo test --locked -p graphit-ai --test integration native_rerank::
cargo fmt --package graphit-ai -- --check
cargo xtask cargo clippy --locked -p graphit-ai --lib --test integration -- -D warnings
```

Linux Main qualification on 2026-10-10 used the warmed SDK target with jobs=2, incremental=false and locked/offline dependencies: Voyage 10/10 PASS, Cohere 8/8 functional PASS with its existing optional measurement ignored, native rerank 2/2 PASS, and Jina 10/10 PASS after the shared Data extension. Build duration was 4.64 s; respective harness durations were 1.29, 1.35, 0.05 and 1.30 s, with 8.685 s observed wall time for the combined invocation. Package fmt and affected library/test Clippy with warnings denied passed; Clippy took 2.37 s (2.452 s observed wall time). These are build/test timings, not a provider benchmark.

The scoped Source-to-documentation review found no material divergence and no broken local links. Logs and a compact result are retained under `.build/rust-ai/voyage-rerank/`. Local TCP verification does not establish an authenticated provider call, application cutover, production latency or execution on another operating system.
