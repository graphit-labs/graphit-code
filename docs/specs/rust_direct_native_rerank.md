---
title: Rust Direct native reranking with Auth admission
type: spec
status: linux-auth-consumer-qualified
updated: 2026-10-18
tags: [rust, ai, auth, rerank, limits]
references:
  - target: {type: task, id: tsk-ea46}
    relation: relates_to
  - target: {type: task, id: tsk-658c}
    relation: corrected_by
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-4ec8}
    relation: implements
  - target: {type: task, id: tsk-9d27}
    relation: uses
  - target: {type: task, id: tsk-3350}
    relation: uses
---

# Rust Direct native reranking with Auth admission

The public graphit_ai::direct_native_rerank::score consumer borrows a real ServiceCredentialLease, query, document slice, existing BoundedClient and NativeRerankLimits. It returns complete input-order Vec<f64> scores or DirectNativeRerankError. Authentication and service capture remain with RoleIssuer: authenticate_local, capture_service(Rerank), then selection.credential(). No independent credential, cancellation or deadline is accepted by this consumer.

## Literal binding and execution

The consumer accepts Direct/Rerank/dimensions0 bindings for Cohere/CohereV2, Voyage/VoyageV1 and Jina/JinaV1. Endpoint and model pass literally to the corresponding existing SDK. Other valid bindings return UnsupportedBinding before a native send; producer failures retain their ServiceUseError. No model catalog guess, automatic refresh, retry or default provider is introduced.

The lease original supplies the cancellation reference and absolute deadline. The credential is an operation-scoped borrowed &str from Auth, without another SecretString or a credential cache. Temporary authorization bytes and request bodies retain the existing zeroizing buffers and sensitive headers; this does not claim erasure of copies inside dependencies. Empty documents validate admission and context but do not create a header or send a request.

Each SDK has score_guarded<G,E> accepting a borrowed &HeaderMap and a finite FnMut admission callback. Direct constructs a bounded sensitive Bearer header from the validated borrowed lease credential; it does not accept unauthenticated or caller-supplied headers. Header construction precedes the adapter JSON preflight, while all input and serialized batches still preflight before the first send. Its active score entrypoint uses the same pipeline with an infallible callback and its existing SecretString API. The shared private wire invokes execute_guarded on the caller's existing HTTP pool. Complete input and every serialized batch are preflighted before the first send. Voyage retains its protocol batch cap of 1000; all adapters retain configured bounds and complete result validation.

Context checks surround every admission callback, including a callback returning an error. Admission is revalidated before send, after response/body awaits, between batches, after decoding and before final output. Cancellation/deadline take precedence at those boundaries; otherwise an already observed native or admission failure retains its typed cause. Synchronous work has boundary checks, without a hard CPU preemption promise.

The captured Snapshot must carry a finite deadline. Before constructing
InferenceContext or invoking the SDK, `required_deadline()` rejects a service
Original without a deadline as `Context(io::Error)` with `InvalidInput` and
`NotSent`. `Error::source` retains that actual IO cause. This check preserves the
preceding lease validation and never manufactures a timeout or authority.

## Failure, effects and cleanup

DirectNativeRerankError distinguishes UnsupportedBinding, Context(io::Error), Service { source: ServiceUseError, effect } and Rerank { source: NativeRerankError, effect }. SDK GuardedRerankError<E> distinguishes typed native and admission causes. Debug and Display omit credentials, generic cause text and payloads; Error::source exposes a typed cause explicitly.

NetworkEffect starts at NotSent and becomes MayHaveApplied after a possible send or successful batch. SDK operations aggregate this state across batches and never reset it when a later admission denies before send. A late failure discards all accumulated scores, prevents following requests and performs no retry. The effect is conservative and does not promise remote rollback. Dropping a stalled future releases the existing client permit through RAII; no returned error or effect is promised after the future itself is dropped.

## Verification scope

Current commands use the [AI integration target](../development/ai-tests.md)
with the `direct_native_rerank::` filter.

The fixture uses public installation bootstrap, runtime ownership, AuthDocumentStore/AuthStore publication, RoleIssuer, caller, selection and lease producers. CapturedPublicationScope::installation receives the original RequestContext snapshot directly. Real profile-key publication invalidates a previously captured lease. TCP barriers extend the existing shared rerank fixture.

The consumer suite covers all six protocol enums, literal wire/model/endpoint and a Direct credential distinct from the MCP credential; complete f64 remapping; physical revocation before entry and during a response; a successful first batch followed by revocation during the second response with no third request; original cancellation/deadline before and after an observed send; complete preflight, empty input, late HTTP status, opaque typed causes and permit reuse after drop. The separate focused wire test proves a successful TCP batch followed by generic admission denial before the next send retains MayHaveApplied. That wire test is not proof of Auth revocation before the second send.

Related references: [shared wire](rust_native_rerank_wire.md), [native validation](rust_native_reranking.md).
