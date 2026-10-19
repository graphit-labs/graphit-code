---
title: Rust private native rerank wire pipeline
type: spec
status: strict-library-qualified-app-integration-pending
updated: 2026-10-21
tags: [rust, ai, rerank, limits]
references:
  - target: {type: task, id: tsk-ea46}
    relation: relates_to
  - target: {type: task, id: tsk-4ec8}
    relation: implements
  - target: {type: task, id: tsk-c976}
    relation: implements
  - target: {type: task, id: tsk-7b3b}
    relation: derived_from
  - target: {type: task, id: tsk-55ac}
    relation: uses
  - target: {type: task, id: tsk-72bc}
    relation: uses
  - target: {type: task, id: tsk-9f12}
    relation: relates_to
  - target: {type: task, id: tsk-b559}
    relation: relates_to
---

# Rust private native rerank wire pipeline

This reference describes the private pipeline shared by the Rust Cohere, Voyage, Jina and Graphit adapters. It reuses the caller's bounded HTTP client and inference context. The original Linux qualification below is historical; the new [Auth Direct consumer](rust_direct_native_rerank.md) records its separate integration scope.

## Used internal operations

`crates/graphit-ai/src/rerank_wire.rs` is private to graphit-ai. All four adapters call the same operations:

- `encode_request<T: Serialize>(request, limits, context)` serializes with Serde into a bounded `Zeroizing<Vec<u8>>`.
- `request_scores(client, endpoint, WireBatch, Credentials, limits, context, CallAdmission)` submits the request and returns complete `Vec<f64>` scores in input order.

`WireBatch` owns the encoded body and expected document count. Its typed response envelope is Results for Cohere/Jina, Data for Voyage, or Graphit with a borrowed captured model identity. The identity is observation data, not authority. The batch contains no credentials, execution context or cache.

The adapter validates the entire input and preflights every serialized batch before making the first HTTP request. Preflight buffers are dropped; execution encodes one batch at a time. An empty candidate list under a valid context returns an empty vector without HTTP or credential use.

## Transport, decoding and failures

The pipeline uses execute_guarded on the existing graphit_http::client::BoundedClient. CallAdmission borrows the finite callback and the operation effect state. Context checks run before and after callback invocation, including errors; admission is checked at send/await/body/output boundaries. A possible send changes the shared operation state to MayHaveApplied and later batches never reset it. Every request uses the same cancellation reference and absolute deadline through `NetworkRequestContext::new`; batching never renews the deadline. Configured headers are borrowed by the public guarded adapters, permit custom authentication or no credential, and cannot override the protocol Content-Type. The pipeline bounds and copies their values as sensitive before transport. Active SecretString convenience consumers use internal Bearer preparation after preflight; Auth Direct prepares its sensitive Bearer map from the validated lease. Empty candidates do not use the credential or send HTTP. Temporary authorization bytes and request bodies use zeroizing buffers, without claiming control over copies inside dependencies.

The HTTP client retains its own response bound. The pipeline additionally checks its request/response JSON caps and expected count. Serde visitors require the selected envelope, reject missing/null/duplicate required fields, malformed or trailing JSON, and validate indices and finite `f64` scores through `validate_results`. Unknown fields are skipped with Serde under the response bound.

HTTP status, network, context, policy, limit, encoding, shape and allocation failures remain typed NativeRerankError variants. Guarded calls retain admission E separately in GuardedRerankError<E> and aggregate NetworkEffect across batches. A later batch failure discards accumulated scores, stops subsequent requests and performs no retry. Error displays contain neither credentials nor input payloads. Synchronous serialization/decoding checks context at its actual boundaries; it does not promise hard CPU preemption or a process RSS bound.

## Graphit adapter and configured authentication

`GraphitReranker::new(client, &BrokerRerankService, limits)` borrows the Auth
discovery descriptor, including its resolved endpoint, service revision, model,
model revision, recipe and document limit. It reuses the same pipeline with the
Graphit envelope. `score_guarded(query, documents, &HeaderMap, context, gate)`
sends `query`, `documents` and `top_n` equal to each chunk length, without a
`model` field. Chunk size is the smaller of the captured local batch limit and
the descriptor's `max_documents`; the total candidate cap remains the caller's
existing policy. A zero advertised document limit is rejected before sending.

Every response requires complete indexed `results` and a `graphit` object whose
`revision`, `model`, `model_revision` and `recipe` strings exactly match the
captured descriptor. `revision` is the service revision, distinct from
`model_revision`. The bounded Serde visitor checks each batch and rejects a
missing, duplicated, mistyped or different identity as a malformed response.
Unknown extension fields remain skippable under the existing JSON bound.
No partial scores escape if a later batch changes identity or fails.

Scores are remapped to input positions before the caller orders its candidates.
The App [search consumer](rust_search_rerank_consumer.md) applies explicit opt-in
and reorders only an already pinned page; the SDK does not select pages or change
their cursors or `top_k`. No additional HTTP pool, retry or fallback is introduced.

Cohere, Voyage and Jina expose the same HeaderMap guarded seam. Their active
`score(..., &SecretString, ...)` convenience calls retain Bearer validation;
Direct obtains and validates its credential through its genuine Auth lease and
then composes headers. No key-string overload of score_guarded remains.

The historical six-target Linux integration run for ea46 passed 47 functional
cases (native2, Cohere8, Voyage10, Jina10, Direct12, Graphit5); one optional
Cohere measurement stayed ignored. Wall time was 108.429 s including Cargo's
1m38s build, with harness times recorded separately. The new Graphit target
uses the existing real TCP fixture for custom/no-key calls across the three
provider adapters, model-free Graphit wire, complete batch remapping, malformed
responses, late status/effect retention, admission denial after observed send
and zero-network preflight/content-type rejection. This is SDK integration
evidence; Broker HTTP/SQL authorization/cache and ONNX composition have their
own owner and qualification. That historical ONNX composition boundary is
superseded for production Local rerank by the 2026-10-21 GGUF/llama/cxx decision;
the remote protocol and the six-test qualification below are unchanged.

On 2026-10-21, `cargo test --locked --offline -p graphit-ai --test
graphit_reranking` passed all six affected functions on Linux, with Cargo's default
concurrency: 57.22 s compilation and 0.66 s harness. Five existing functions retain
their malformed-row, late-status, guard, preflight and complete-remapping purposes
with valid captured identity metadata. The new function checks eleven missing,
different or duplicate identity cases in a second batch, preserving the possible
network effect and preventing a third send or partial return. The complete-batch
case also enforces descriptor max_documents=2 with local max_batch=100. These are
six functions in this target, not six additional functions over the historical
overlapping qualification. One unused shared-fixture entry produced a warning;
its owner is reconciling that declaration separately.

New App consumer fixtures cover real Auth leases and the same identity contract,
but have not yet been executed. Local GGUF App composition, Broker Native and the
disabled real-consumer matrix remain separate pending proofs. App execution stays
coordinated with its changing provider composition.

The retained-success/next-admission-denied wire unit passed separately (1 case,
15.034 s wall including compilation, 0.05 s harness). Strict Clippy on the
library and the same six integration targets passed in 77.998 s wall after the
coordinator reconciled a shared Records lockfile edge. The earlier 0.417 s
locked-resolution failure happened before compilation and is preserved as a
shared prerequisite failure, not an SDK test failure. No SDK/provider tests
were repeated for that lockfile composition. Scoped rustfmt and the 17 relative
file links in the six maintained rerank contracts also passed.

## Affected verification

The existing affected invocation is:

```text
cargo test --locked -p graphit-ai --test native_rerank --test cohere_reranking -- --test-threads=2
```

This historical recorded run passed 2 native and 8 Cohere cases, with one operation measurement ignored. Its evidence remains in `.build/rust-ai/rerank-wire/qualification-1791587193466216965/` and Task `tsk-c976`. It covers exact wire requests, batching, full-set remapping, finite precision/signed-zero ordering, boundary caps, preflight without HTTP, real HTTP failures, stalled cancellation/deadline and late failure without a partial result. Earlier fmt/Clippy evidence has its recorded scope; this documentation reconciliation is not a new runtime run.

Decisions 566/570 in `ses-c98a` retired reservations, custody/floor promises, hash freezing and per-leaf artificial guards. Historical guard records remain provenance, not prerequisites or current execution contracts. Functional bounds, authorization, cancellation and errors remain required.

The current guarded pipeline and Auth consumer qualification are maintained in [Direct native reranking](rust_direct_native_rerank.md).

Related references: [Cohere adapter](rust_cohere_rerank.md), [native score validation](rust_native_reranking.md).
