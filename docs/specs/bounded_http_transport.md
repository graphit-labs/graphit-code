---
title: Bounded HTTP transport
type: reference
status: partial
updated: 2026-10-20
references:
  - target: {type: task, id: tsk-bcf2}
    relation: implemented_by
  - target: {type: task, id: tsk-3350}
    relation: derived_from
  - target: {type: task, id: tsk-9d27}
    relation: supports
  - target: {type: task, id: tsk-72bc}
    relation: derived_from
  - target: {type: task, id: tsk-e849}
    relation: relates_to
  - target: {type: task, id: tsk-bca6}
    relation: derived_from
---

# Bounded HTTP transport

`graphit-http::client` is the outgoing transport for Broker discovery, JWKS, UserInfo and OAuth clients. It retains explicit request limits, cancellation and the caller deadline.

The streaming API extends this transport with a borrowed response sink.

## Captured policy

The [native WebSocket transport](rust_http_websocket.md) reuses the same private
policy constructor with HTTP/1 only for its SDK upgrade. Its socket lifecycle and
frame/message bounds are separate from these finite HTTP response APIs; it adds
no authorization, profile mutation or fallback to the existing client.

`BoundedClient::new` receives a `ClientPolicy` and explicit positive `Limits`.
There are no library defaults. Policy supplies one HTTP origin, an explicit
development exception for literal loopback HTTP, an optional captured proxy,
and bounded DER trust certificates. HTTPS verifies the certificate and hostname
using Rustls and platform trust, plus the supplied roots. There is no public
client/builder injection or option to disable TLS verification.

Requests must match the captured scheme, host and effective port. The URL must
be absolute and have no userinfo or fragment. Paths and queries remain request
inputs. The client disables ambient proxy lookup, redirects, retries, referer,
cookies and automatic decompression. A supplied proxy is captured at construction;
later environment changes cannot select one. `Accept-Encoding` is always
`identity`; a response advertising another encoding is rejected.

The single-origin connection pool is reused. In-flight requests and idle sockets
per host have separate finite limits. Admission returns `Busy` when all permits
are held; the transport does not retain an unbounded queue of waiting callers.

## Original request and bounded data

`NetworkRequestContext::new(Instant, &Cancellation)` borrows an ordinary request's
absolute deadline. `service(&Cancellation)` supplies no deadline, and
`with_deadline(Option<Instant>, &Cancellation)` propagates the actual Original.
All sending and body reading share the same cancellation and optional deadline.
Neither chunks nor network steps renew it. A missing deadline keeps cancellation
active; it does not change authentication, signed token expiry, PROJECT permissions,
connection pool admission or response bounds. Core's optional `async-cancel` feature adds
Tokio notification to the same permanent signal used by synchronous lock
waiters, without polling or one thread per request. Core's default feature set
does not depend on Tokio.

Raw and normalized URL bytes, request body, logical header count/bytes and captured certificate count/bytes
are checked before Graphit retains their copies. Managed request headers are
included in the logical header budget. Response header limits are checked before
copying them; HTTP/1 parser count and HTTP/2 header-list limits also apply.
Response bodies are read in chunks with checked cumulative lengths before each
buffered append or streaming sink callback. An excessive `Content-Length` fails
before the streaming head callback or body reading. Buffered execution returns
no partial response on a limit, cancellation, deadline, encoding or transport
error. Streaming can already have delivered bytes to a caller-owned stage when
it fails; the caller must discard that stage rather than publish it.

These are logical bounds on Graphit's own data. They do not promise a process RSS
limit or that every internal DNS, TLS, HTTP parser or allocator buffer obeys the
same byte limit. In particular, HTTP/1 header-byte admission occurs after the
vendor parser, before Graphit's copy. Dropping a request releases its admission
permit and drops its transport future; platform resolver work may have a
separate lifetime managed by the HTTP library.

## Per-call admission and remote effects

`BoundedClient::execute_guarded` borrows a synchronous
`FnMut() -> Result<(), E>` gate for the finite request future.
The caller supplies the real admission check. The transport stores no identity,
authorization lease or gate, and has no Auth dependency. The public
`execute` API and guarded requests share one transport implementation
and the same connection pool, limits and immediate `Busy` behavior. Buffered
execution uses an internal sink over the same `execute_stream_guarded` pipeline;
it does not create a separate HTTP client or body-reading implementation.

With the original context still valid, the gate runs at entry, immediately
before the native send, after every response/body await including EOF, and
before output. Buffered failure drops the response and retained partial body
instead of returning it. Streaming failure preserves any external sink effects;
HTTP cannot roll them back. The original cancellation and deadline keep their existing
priority; they are never renewed. The context is checked both before and after
every synchronous gate invocation, even when the gate returns an error. A gate
that cancels the original signal or consumes its deadline cannot allow sending
or response release by returning `Ok`.

`NetworkEffect::NotSent` means this request did not reach the native
send boundary. `MayHaveApplied` is marked immediately before polling the
native send, so errors after that boundary conservatively retain the possibility
of a remote effect. It does not assert that the server applied the request, and
it does not undo or retry it. The effect covers one HTTP request; an AI consumer
must also account for earlier batches in its original operation.

`GuardedNetworkError<E>` distinguishes a typed caller admission error
from the existing `NetworkError` category and exposes `effect()`.
Post-await checks still run when transport fails, but a later admission failure
does not replace an already observed transport error. The final gate likewise
does not replace an earlier admission or transport cause. Cancellation/deadline
retain the context priority described above.

Guarded error `Debug` and `Display` do not format the caller's
error or request data. The typed cause remains explicitly accessible through
the enum, and through `Error::source()` when `E: Error + 'static`.
The caller controls any further formatting of its own error. Dropping the future
releases its permit and transport future; dropping it does not produce a
returned effect/error value.

The callback transport does not grant Auth leases, profile selection, refresh or AI output authority.

## Borrowed streaming response

The implemented interface is:

```text
trait NetworkResponseSink {
    type Error;
    fn head(&mut self, status: StatusCode, headers: &HeaderMap)
        -> Result<(), Self::Error>;
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

BoundedClient::execute_stream_guarded<G, E, S>(
    &self, request: NetworkRequest<'_>, context: NetworkRequestContext<'_>,
    gate: &mut G, sink: &mut S,
) -> Result<NetworkStreamResponse, NetworkStreamError<E, S::Error>>
where G: FnMut() -> Result<(), E>, S: NetworkResponseSink
```

`execute_stream_guarded` is asynchronous; head/chunk are synchronous borrowed
callbacks belonging only to that finite future. The transport retains neither
the gate nor the sink. Head receives the bounded status/metadata before any body
callback, after header, content-encoding and Content-Length admission. Its
borrowed HeaderMap has sensitive values. A caller can reject a status before
body delivery; HTTP does not itself interpret a non-success status as Auth or
provider policy. Chunks are borrowed vendor buffers, not protocol frames or
durable references. The transport retains no aggregate body Vec in this mode.
Vendor chunk/parser buffers and caller storage are not a process RSS bound.

The original context and admission gate are checked around response processing
and head/chunk callbacks, after every network await including EOF, and at final
output. Context is checked before and after synchronous gates and sink callbacks.
A callback that cancels the original or consumes its deadline cannot hide that
failure by returning `Ok`; no later chunk or successful response is released.
Callbacks may perform caller-owned synchronous work, but the transport cannot
preempt that work or a filesystem syscall. Sending and all reads keep the same
original Cancellation and absolute Instant, with the existing immediate Busy,
pool, TLS, captured origin/proxy, no-redirect and no-retry policies.

NetworkStreamResponse exposes only `status()` and `accepted_bytes()`. It is
returned after successful EOF and final original/admission checks, and is not a
body buffer, publication receipt or model-completion proof. NetworkStreamError
has Network { source: NetworkError, effect, accepted_bytes }, Admission {
source: E, effect, accepted_bytes } and Sink { source: S, effect,
accepted_bytes } variants. Its `effect()` retains the same conservative
NotSent/MayHaveApplied boundary as buffered requests. An already captured
network, admission or sink cause is not replaced by a later gate denial; the
original cancellation/deadline priority still applies. Typed causes remain
accessible through the enum and Error::source when both caller error types
implement Error + 'static. Debug/Display show the static category, effect and
count, never the admission/sink payload, URL, headers or body.

`accepted_bytes` sums only complete chunk callbacks that returned `Ok`, using
checked addition and the aggregate response_body_bytes limit before delivery.
It remains counted if a later original/gate check fails. A failed callback may
have written part of its chunk without adding it to this count; head can also
alter the stage. The count is not committed storage or a rollback guarantee.
Failure and future drop stop this operation and release transport/admission
ownership, but cannot undo remote effects or bytes already written by a sink.
Dropping the future does not return a synthetic effect/count receipt.

There is no HTTP commit/flush callback or filesystem publication. The caller
owns private staging, cleanup on failure/drop, and publication only after HTTP
success plus its actual integrity and authorization checks. HF/CDN redirect
allowlists, model selection, artifact hash/range policy and provisioning belong
to consumers; this transport supplies none of that authority or policy.

The executed fixtures cover head-before-body, bounds, partial sink effects,
cause priority, original cancel/deadline, revocation, future drop/permit reuse,
and a counting sink consuming 2 MiB twice over one reused connection. The
earlier locked offline command failed before building, exit101 in
0.141530627 s, because the shared Main Lock needed a Root-owned update.
That failure remains in the evidence; it was not an executed streaming-test
failure. The prior 23-test `tsk-3350` buffered qualification and its receipts
also remain separate historical records.

This qualifies the affected HTTP library on Linux, including the shared
buffered/stream pipeline. Windows/macOS, the pinned 557950176-byte model
download, App CLI/default selection, Auth model-lease integration and product
cutover were not exercised or qualified by these tests.

## Secrets, response and errors

Request, response and client types have no `Debug` or serialization representation.
Header values are marked sensitive on transport and response copies. Buffered
response body ownership uses `Zeroizing<Vec<u8>>`; consumers explicitly access
bytes or take that buffer. Streaming instead borrows vendor chunks and delegates
retention to the sink; it promises no erasure of those vendor or caller-owned
bytes. Sensitive `HeaderMap` values are not erased on drop. This does
not promise erasure of reqwest/TLS/socket internal
copies. Callers must also protect their original credential inputs.

Connection identifies a native timeout, a recognized connection I/O kind in the
SDK error chain, or Hyper's typed closed/incomplete-message condition. The latter
covers EOF before response headers complete even when no std::io::Error is nested.
Hyper HTTP parsing errors have terminal precedence. Invalid HTTP/chunk frames,
TLS certificate/hostname verification and unknown causes remain Transport; no raw
vendor cause escapes. Classification does not change MayHaveApplied or retry the
request. The continuous Now owner can use its existing fresh-admission recovery
without interpreting an uncertain write as rolled back.

The typed disconnect correction passed 35 HTTP client tests and 16 WebSocket
tests on Linux. The two suite-level ignored entries are child probes executed
by their passing parent isolation tests. Native loopback cases close before or
during headers and truncate fixed-length/chunked bodies; they preserve
MayHaveApplied and observe one request, without replay. Malformed headers/chunk
frames and actual certificate/hostname rejection remain terminal. The EOF-before-
response regression failed as Transport before the correction and passed as
Connection afterward. Compilation took 2.92 seconds; client execution took 2.27
seconds and WebSocket execution 0.47 seconds. This is transport SDK evidence;
it does not prove the cause of an earlier production failure or installed Source
stability, which require the rebuilt consumer and its own runtime observation.

Errors are static categories: `InvalidPolicy`, `InvalidRequest`, `Origin`,
`Busy`, `Cancelled`, `Deadline`, `Connection`, `Transport`, `Limit` and `Encoding`. They do not
wrap vendor errors or contain URLs, bodies, identities or credentials. HTTP
status codes, including OAuth errors and redirects, are returned to the caller
within a bounded response; the transport does not follow a redirect or interpret
an HTTP status as authorization.

Source: `crates/graphit-http/src/client.rs` and
`crates/graphit-core/src/file_lock.rs`. Primary library APIs:
[reqwest 0.13.5](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html)
and [Tokio Notify](https://docs.rs/tokio/1.53.2/tokio/sync/struct.Notify.html).
The earlier `tsk-72bc` qualification ran `cargo test --locked -p graphit-http --all-targets`
against real TCP, TLS and proxy fixtures, with affected Core cancellation and
protected completion tests. The then-current pre-commit runner (since replaced by the
[native configured dispatcher](../guides/git-hooks.md)) passed SDK safety, formatting and workspace Clippy. Core's normal dependency
tree without features contains no Tokio; the lock audit found no advisories.
The evidence records exact commands, log hashes and the dependency delta.

No endpoint, Broker OP, native agent, Windows/macOS runtime, performance gain or
end-to-end product validation pass follows from this library qualification. Full compiled
isolation and final ecosystem gates remain integration work; their runners and
policies were unchanged by this delivery.
