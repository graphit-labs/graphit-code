---
title: Rust Direct embedding service leases
description: Compose six literal Direct embedding protocols with the real Auth lease and existing SDKs.
type: specification
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target: {type: task, id: tsk-658c}
    relation: corrected_by
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-0f32}
    relation: implements
  - target: {type: task, id: tsk-ed52}
    relation: uses
  - target: {type: task, id: tsk-174e}
    relation: implements
  - target: {type: task, id: tsk-d1ac}
    relation: derived_from
  - target: {type: task, id: tsk-9d27}
    relation: uses
  - target: {type: task, id: tsk-3350}
    relation: uses
  - target: {type: task, id: tsk-099a}
    relation: uses
  - target: {type: task, id: tsk-521f}
    relation: uses
  - target: {type: task, id: tsk-4ec8}
    relation: derived_from
  - target: {type: knowledge, id: Per-call_service_credentials}
    relation: supports
---

# Rust Direct embedding service leases

For Rust callers that already captured and authenticated a service through Auth.
`graphit_ai::direct_embedding::embed` connects a real `ServiceCredentialLease`
to the existing [Cohere](rust_cohere_embeddings.md),
[Voyage](rust_voyage_embeddings.md), [OpenAI/Compatible](rust_inference.md) or
[Google](rust_google_embeddings.md) SDK and caller's bounded HTTP pool. It does
not select a profile, refresh a credential or infer provider configuration.

## Calling contract

```rust
pub async fn embed(
    lease: &ServiceCredentialLease<'_>,
    texts: &[&str],
    input: DirectEmbeddingInput,
    client: &BoundedClient,
    limits: EmbeddingLimits,
) -> Result<Vec<Vec<f32>>, DirectEmbeddingError>;
```

The binding must be Direct with purpose `Embedding`, positive dimensions and
exactly `DirectProtocol::{Cohere, Voyage, OpenAi, OpenAiCompatible,
GoogleGemini1, GoogleGemini2}`. Auth's current
embedding validation excludes `CohereV2` and `VoyageV1`; those are native-rerank
protocols, not embedding aliases. Unsupported modes, purposes, protocols or zero
width fail with `UnsupportedBinding` before sending. Endpoint, model and width
are the literal captured values. No environment lookup or default supplies them.

OpenAI/Compatible preserve the literal model, endpoint and expected width.
Auth `request_dimensions: Option<u32>` is converted with checked arithmetic to
the SDK's optional output-width reduction. The producer permits it only for the
two supported OpenAI embedding-3 models, matching expected width and smaller
than native width; Compatible rejects it before HTTP. The native widths are
1536 for `text-embedding-3-small` and 3072 for `text-embedding-3-large`. The lower
OpenAI SDK validates only the structural optional-dimension contract; this Direct
consumer owns that whitelist. No retired protocol alias remains.
Google families are explicit. Both captured endpoints and the required
`batch_endpoint` correspond to the literal `models/<name>` resource; the
consumer never derives a route. Query/Document map to Google retrieval modes:
Gemini1 retains `embedContentConfig` task type, Gemini2 retains the exact native
text prefixes and omits task type. Direct passes `Some(dimensions)` to the Google
SDK. A consumer's Google compatibility namespace distinguishes the captured single
endpoint, batch endpoint and explicit family. OpenAI-shaped text remains unchanged.

`DirectEmbeddingInput::Query` maps to Cohere `search_query` or Voyage `query`;
`Document` maps to `search_document` or `document`. The lease lends its service
key as `&str`; MCP and rerank keys cannot substitute for that credential. The
consumer retains neither the key nor an authority or selected model after the
future. Direct constructs the existing sensitive provider headers and lends them
to `embed_guarded`. It reuses the lease's original cancellation signal and absolute
deadline.
The captured Snapshot must have a finite deadline. The adapter calls
`required_deadline()` before constructing InferenceContext or invoking the SDK.
A service Original with no deadline returns `DirectEmbeddingError::Context`
with the real `io::ErrorKind::InvalidInput`, `NotSent` and zero HTTP requests;
absence of a deadline is not expiration. Existing lease checks still precede
this boundary, and no timeout, credential, permission or Original is replaced.

The successful batch moves its vectors with `into_vectors`, preserving width and
original input order without copying the vector payload.

The SDKs validate all input and preflight every encoded chunk before the first
send. They preserve their native batch caps, exact widths, finite `f32` values,
and existing Serde codecs. Every network batch uses `execute_guarded` with
`lease.check()`; context is checked before and after admission. The HTTP client
also checks admission at send, await and output boundaries. SDK decoding is
followed by admission before vectors are retained, and the consumer checks again
before returning. A physical profile/provider change or current permission loss
follows Auth's concrete lease invalidation contract. No account-revocation RPC,
UserInfo, token refresh, ledger or artificial quota is added to DirectKey calls.

Empty input is validated and passes the real lease gate, returning an empty
vector list with zero HTTP and without obtaining a credential. An expired, cancelled or invalidated lease fails even
for empty input. Later failure drops all internal vectors and prevents following
batches; no partial output, zero-filled vector, retry or fallback is returned.
Lower guarded partials leave Direct's public result all-or-error: Direct preserves
the actual cause and accumulated network effect while discarding partial vectors
and usage on failure.

## Typed failures and recovery

`DirectEmbeddingError` distinguishes `UnsupportedBinding`, `Context(io::Error)`,
`Service { source: ServiceUseError, effect }` and
`Embedding { source: EmbeddingError, effect }`. Its `source()` preserves the real
Auth, context IO or SDK cause. `effect()` reports `NotSent` until any HTTP request may have
been sent; once that occurs, `MayHaveApplied` survives failures in later batches.
HTTP statuses, network errors, original context, input limits, malformed shape
and non-finite output remain typed. Unsupported binding and constructor/input
failures before transmission have `NotSent`.

Debug and Display omit binding, text, credential and headers. The cause is
available explicitly for typed handling. A caller must correct authentication or
configuration failures through their owning flows; this consumer never silently
refreshes, replaces a lease or changes service mode. An ambiguous remote effect
must inform any retry decision.

## Qualification

Source ownership is `crates/graphit-ai/src/direct_embedding.rs`, the guarded
pipeline in `embedding.rs`, `cohere_embedding.rs`, `voyage_embedding.rs`,
`openai.rs` and `google_embedding.rs`, and
the exported module in `lib.rs`. `tests/direct_embedding_leases.rs` reuses the
real public Auth fixture from `tests/support/direct_rerank_auth.rs` and Tokio TCP
fixture from `tests/support/rerank_http.rs`.

Current commands use the [AI integration target](../development/ai-tests.md)
with the `direct_embedding_leases::` filter. Related SDK and rerank modules use
their own filters in that same executable.

The historical `tsk-174e` verification command was:

```text
cargo test -p graphit-ai --locked --offline -j1 --test cohere_embeddings --test voyage_embeddings --test direct_embedding_leases --test direct_native_rerank
```

The `tsk-174e` affected TCP run on Linux passed 37 functional tests with zero failures:
Cohere 9, Voyage 9, Direct leases 8 and native rerank 11. Two historical provider
measurement fixtures were ignored; no measurement campaign was run. Command wall
was 21.403 s including compilation and shared-target scheduling; the individual
harness runtimes were 0.87 s, 1.04 s, 3.30 s and 2.52 s respectively. These values
are execution receipts, not provider latency or a performance comparison.
Strict Clippy passed for the library and these four integration targets with
`--locked --offline -j1 -- -D warnings`; command wall was 211.032 s including
shared-target scheduling and dependency checks. Formatting of the seven affected
Rust files and scoped diff checks passed. Eight local documentation links
resolved. The tracker review confirmed typed error precedence after a decode
failure and found no other material defect in the bounded delta.
Results and logs are recorded in `.build/rust-direct-embedding-leases-174e/`.
The fixtures cover both
retrieval roles, literal binding/key and vector order, empty/preflight zero sends,
incompatible real bindings, physical profile mutation between batches, original
cancel/deadline and typed status/codec/shape/finite/transport failures. The rerank
suite is affected only by its shared Auth fixture extension.

The `tsk-0f32` Main extension passed 41 affected functional tests on Linux:
10 Direct lease, 10 OpenAI/Compatible SDK, 10 Google SDK and 11 native rerank
regressions. Two historical measurement fixtures stayed ignored. The suite
command was `cargo test -p graphit-ai --test direct_embeddings --test google_embeddings --test direct_embedding_leases --test direct_native_rerank --locked --offline -j1`.
Wall time was 516.35 s including the shared Cargo queue and dependency builds;
individual harness times were 10.12 s, 0.80 s, 1.36 s and 2.83 s respectively.
Selected strict Clippy also passed for the library and these four test targets
with `--no-deps -- -D warnings`; wall time was 168.09 s including shared
queue/dependency checks. These are qualification costs, not vendor inference latency. Current logs and
commands are in `.build/rust-direct-openai-google-0f32/`.

The OpenAI/Compatible and Google composition above consumes the literal Auth
producer from `tsk-ed52`; qualification of the extension belongs to `tsk-0f32`.
Broker/Local, completion,
search/storage integration and platform or authenticated vendor inference require
their own evidence. This library integration does not qualify those deliveries.

The subsequent tsk-170b providers04 run passed all five affected suites. The Google
optional-dimension focal passed one test in 0.55 s across both families and
single/batch routes; the bounded wire suite passed three tests. These focused
results do not qualify the entire Broker composition or additional platforms.
