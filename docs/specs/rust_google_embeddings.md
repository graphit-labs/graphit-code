---
title: Rust Google text embeddings
description: Explicit Google embedding families, single and batch requests with bounded transport and positional vectors.
type: specification
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-170b}
    relation: implements
  - target: {type: task, id: tsk-0f32}
    relation: implements
  - target: {type: task, id: tsk-ed52}
    relation: uses
  - target: {type: task, id: tsk-3352}
    relation: implements
  - target: {type: task, id: tsk-ad37}
    relation: derived_from
  - target: {type: task, id: tsk-56cb}
    relation: uses
  - target: {type: task, id: tsk-55ac}
    relation: derived_from
  - target: {type: task, id: tsk-72bc}
    relation: derived_from
  - target: {type: task, id: tsk-f845}
    relation: derived_from
  - target: {type: task, id: tsk-c775}
    relation: validated_by
---

# Rust Google text embeddings

For consumers integrating direct text retrieval embeddings. Source
`crates/graphit-ai/src/google_embedding.rs` is exported by the crate. Its real
TCP suite is at `crates/graphit-ai/tests/google_embeddings.rs`. Its historical Linux TCP
suite, strict Clippy and separate precompiled fixture passed on
2026-10-09. The matching Google Git guard passed in `tsk-c775` on the same date. This library does not select profiles, refresh credentials, discover
Broker routes, publish vectors or cut over storage/search.

## Captured configuration and wire

`GoogleEmbeddings::new(client, GoogleEndpoints { single, batch }, model_resource,
dimensions, request_dimensions: Option<usize>, family, limits)` borrows the existing
`graphit-http::client::BoundedClient`, two captured full operation endpoints and
the canonical `models/<name>` resource. The caller supplies positive expected width,
`GoogleFamily::{Gemini1, Gemini2}` and existing `EmbeddingLimits`. Family and
width are explicit; a model name supplies no implicit capabilities or defaults.
The constructor rejects zero width, invalid resource and endpoint/operation
mismatch, and enforces captured dimension and request-byte caps. The model name
is a nonempty ASCII segment of letters, digits, dot, underscore or hyphen.

Both endpoints require an HTTP/HTTPS URI with authority, no query, and a path
ending in the same model resource and corresponding `:embedContent` or
`:batchEmbedContents` operation. Nothing repairs or discovers an endpoint.
The real transport still enforces captured origin, URL/header/body/concurrency
policy. Endpoint mismatch fails before HTTP.

`embed(texts, GoogleInput::{Document, Query}, &SecretString, InferenceContext)`
borrows a credential for each call and retains no key. A nonempty call requires
usable `x-goog-api-key`; no Bearer or query key is added. Its header is sensitive
and constructed with a bounded zeroizing temporary. Adapter/result types have
no public Debug/Serde payload representation. Private request structs use Serde.

`embed_guarded(texts, GoogleInput, &HeaderMap, InferenceContext, &mut gate)` uses
that same native encoding/decoding pipeline. Plain `embed` delegates with an
infallible gate. The existing bounded HTTP client checks admission immediately
before send and after awaits; the SDK also checks after decoding and before
returning. Denied admission exposes no partial vectors. Embedding failures may
retain validated complete-response vectors and observed usage through the
[common guarded error contract](rust_inference.md). `NetworkEffect` accumulates
across batches. Guarded calls accept an empty or configured sensitive header map.
Convenience `embed` builds its sensitive `x-goog-api-key` only after input/context
validation; empty input needs no key.

The [Direct lease consumer](rust_direct_embedding_leases.md) maps exactly
`GoogleGemini1`/`GoogleGemini2` to these families. It borrows the captured
single endpoint, required `batch_endpoint`, `models/<name>` resource and expected
width from Auth. Changing captured options invalidates the real lease; neither
family nor batch route is guessed or derived from the model name.

A chunk with one text uses single POST: `content.parts[0].text` and
`embedContentConfig`. Larger chunks use batch POST: each `requests[]` item has
`model`, its own individual `content.parts[0].text` and `embedContentConfig`.
Every item uses the endpoint's captured model resource. A final chunk of one
entry uses single without renewing its context. Inputs are never aggregated
into one Content.

The current [Google REST reference](https://ai.google.dev/api/embeddings),
inspected on 2026-10-09, defines `embedContentConfig` with
`outputDimensionality`. `None` omits `embedContentConfig.outputDimensionality`;
`Some(width)` must equal positive expected width and sends it for single and every
batch item, for both families. Direct continues passing `Some(dimensions)`. Deprecated top-level `taskType` and
`outputDimensionality` are omitted. It adds no truncation/title/multimodal/async
batch feature.

For Gemini1, the configuration includes `taskType=RETRIEVAL_DOCUMENT` or
`RETRIEVAL_QUERY`, and text is unchanged. For Gemini2, `taskType` is omitted,
including inside configuration. Query text receives exactly
`task: search result | query: ` and Document text exactly `title: none | text: `,
including the final space before the original input. These asymmetric retrieval
instructions follow the [Google embedding guide](https://ai.google.dev/gemini-api/docs/embeddings).
No separate title or alternate task mode is inferred.

## Positional decoding and finite budgets

Single requires `embedding.values`; batch requires `embeddings[].values` in
input order. Exactly one finite `f32` vector of the captured width is required
per input for full success. There are no indices to invent or remap. Required/null/duplicate
fields, invalid structure/count/width, numeric overflow, malformed/trailing JSON
fail. A positional cardinality mismatch invalidates every candidate from that
response. With exact cardinality, an invalid individual vector can leave valid
siblings available through the guarded error. Complete JSON and EOF are required;
earlier complete batches remain retained. Recognized usage follows the common
embedding contract; other valid extras are ignored without retention.

The existing [embedding common API](rust_inference.md) supplies seven positive,
representable caps: texts, total input UTF-8 bytes, dimensions, total vector
values, batch, request JSON bytes and response JSON bytes. Its effective batch
policy already caps the captured value at 2048. The Google adapter adds no
provider hardcap: 100 is an operational caller choice, not a verified API maximum.
Provider token budgets are separate from UTF-8 bytes and can produce a real
HTTP rejection.

`GoogleFamily::max_input_prefix_bytes()` returns zero for Gemini1 and the maximum
Query/Document prefix length for Gemini2. The SDK budget includes the actual prefix;
a consumer such as Broker can separately enforce its literal HTTP input-byte cap.
Input-byte accounting includes every Gemini2 prefix, using checked sums and
count × width before allocation or network. Prepared strings and request objects
are reserved fallibly for the current chunk. Every chunk is preflight-encoded
through a bounded geometric writer before the first request; temporary zeroizing
buffers are discarded. A later oversized escaped body prevents all sockets.
Execution retains only the current encoded chunk and internal complete vectors.

Serde seeds bound row count and width during decoding, before excess growth.
The response must meet both the real HTTP client's body cap and the adapter cap;
the latter is checked after the existing HTTP body is buffered. These logical
caps do not establish process RSS or allocator overhead bounds. Synchronous
Serde is cooperative: an ignored field may finish parsing before the next
context check.

## Context, outcomes and recovery

Every request borrows the same Core cancellation signal and absolute Instant
from the original context. Checks cover input/encoding, chunks, decode rows and
components and final return. The real HTTP client observes that context while
waiting for headers/body. There is no replacement cancellation token, per-chunk
timeout extension, retry, redirect, decompression, extra pool or Go fallback.

Empty input checks context/policy and returns an empty existing `EmbeddingBatch`
without HTTP. Pre-cancel or expiry remains an error. Success exposes the captured
model resource, width and vectors through existing explicit accessors; these
metadata alone do not establish storage compatibility identity. Ordinary `embed`
remains all-or-error. Guarded failures can retain earlier complete batches and
unambiguous valid siblings as described above, and stop subsequent requests.

Existing `EmbeddingError` retains real `ContextError`, `NetworkError` and HTTP
status. Configuration, credential, cap, codec, shape, non-finite and allocation
failures remain typed. Diagnostics contain no key, endpoint/query, header,
body, model or input. Consumers can distinguish policy/authorization correction
from cancellation/deadline/provider failure. The adapter never turns errors into
empty/default vectors or hides them through retries.

## Linux library qualification

Nine functional tests passed, with one measurement ignored in the suite, using
real Tokio TCP sockets and the real bounded client with synthetic in-memory
credentials. Cases cover both families and modes in single/batch, exact config
and prefixes, credential changes, captured batching 100/101/102 and 101 in one
batch, smaller policies, count/UTF-8-prefix/request/response/tensor boundaries,
zero-effect preflight including escaped late chunk, malformed/null/duplicate
required fields, count/width/float overflow, real 401/429/503/refused connection,
origin/cap failures, redacted diagnostics and shared cancellation/deadlines.

The inter-batch oracle uses first/second body delays 60/60ms and the original
100ms deadline. Expected `Network(Deadline)` distinguishes HTTP renewal to about
160ms: the body at about120ms would instead pass HTTP then yield
`Context(Deadline)`. A valid first response followed by a failed second also
observes that a third request is absent. These tests establish local wire/execution behavior, not authenticated inference.

The exact focused command was `cargo test --locked -p graphit-ai --test
google_embeddings -- --test-threads=2`, with jobs2/incremental0: nine functional
tests passed, one measurement ignored. `cargo clippy --locked -p graphit-ai
--all-targets -- -D warnings` passed, as did owned fmt/diff checks. Suite command
wall was 3.96 s including compilation, CPU user/system 2.84/0.65 s, peak RSS
567864 KiB. Strict Clippy wall was 1.11 s, CPU 1.13/0.35 s, peak RSS 215136 KiB.
These process measurements cover compiler/harness, not embedding operations.

The separate precompiled fixture passed 128 sequential calls × eight texts × two
values: total 90704579 ns (90.704579 ms), sorted-sample p95 807509 ns
(0.807509 ms). Process wall was 0.15 s, CPU user/system 0.10/0.02 s, peak RSS
13560 KiB, including the debug harness, TCP server and cleanup. Raw logs/time and
hashes are in `.build/rust-ai/google/qualification-2026-10-09/`:
`tests-attempt-01`, `clippy-attempt-01`, `fixture-attempt-01` and
`execution-receipt.json`. Source, test, lib and protected provider hashes matched
after execution. These observations establish no provider latency, performance
gain, authenticated inference or other-platform qualification. The matching Root Git guard was qualified by `tsk-c775`; final independent
source/docs/results review remains a separate check. The previous direct guard
does not qualify this suite.

Source-to-document coverage: constructor/endpoints/family/header/request structs
implement configuration and wire; response/row/vector seeds implement positional
bounds; common budgets/original context/real HTTP implement limits and errors.
Document-to-source comparison uses these same entrypoints and executed cases.
The active product specification still owns profile/key selection separately.

## Matching Git guard qualification

The coordinator completed `tsk-c775` with the exact
`graphit-ai/google_embeddings` profile. Its temporary `broken_fast_contract`
ran and failed at runtime with the matching panic and sentinel, failed the
phase and Git pre-commit command with status 1. The same positive gate passed
after restoration. The grouped proof confirmed physical compiler isolation,
361 unchanged production source files and removal of its owned scratch directory.

Receipts are in
`.build/rust-six-suite-fast-guards-2026-10-09/qualification-1791575907926208406/`:
`proof-summary.json`, `grouped-six-receipt.json` and `grouped-six.log` (SHA256
`4378f7a7404b64451fe76498422f7cb27d15be400b7fc1bec8432c864351429c`).
The matching Google profile qualifies this suite; the previous direct guard,
native reranking, Process, future plain CLI consumers and full migration have
separate evidence. Windows/macOS and authenticated inference remain unverified.
The focused suite and precompiled measurements above are retained; no provider
measurement or Cargo command was rerun for this documentation update.


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
