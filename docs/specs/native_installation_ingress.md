---
title: Rust Native package installation ingress
type: reference
status: implemented
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-8d58}
    relation: implements
  - target: {type: task, id: tsk-a456}
    relation: derived_from
  - target: {type: task, id: tsk-810f}
    relation: derived_from
---

# Rust Native package installation ingress

This reference describes the Rust App daemon's local corpus installation endpoint.
It is intended for daemon and CLI consumers. The endpoint composes the authenticated
SDK installer on the existing Source listener and Runtime owner. The HTTP boundary is qualified by the scoped Linux SDK/loopback tests below;
the compiled CLI consumer is a separate delivery.

## Request and authority

`POST /admin/packages/install` accepts a JSON object containing `input`, `producer`,
`id`, `version`, and optional `kind`. `input` is an absolute normal ZIP pathname on
the daemon's filesystem; `producer` is a ProjectId ULID, `id` the complete artifact
ID, and `version` a concrete version. `kind` is `ast` or `knowledge` when supplied;
null, unknown fields and malformed JSON are rejected. Omitting kind uses the actual
native envelope. No host selection or project identity is inferred from the ZIP.

The request must carry exactly one `Authorization: Bearer …` header and exactly one
`x-graphit-auth-origin: runtime` header. Malformed or missing protected headers return
401. Structural request errors return 400, excessive input 413, and a stopped owner
503. A deadline expiring while waiting for the pre-admission job mutex returns 408
without starting a job. Business validation and Auth/SDK refusals are terminal
operation outcomes.

The SDK derives the Runtime caller from the existing `Arc<DaemonAuth>` and verifies
the active Native profile's signed Broker JWT. The Foreground owner opts into exactly
Configure, ManageCache and PublishedFilesystem for Native installation. Ordinary
Source permissions, administrative permissions and Local origin remain independent.
Neither the loopback address, discovery document nor protected header is authority.

The installation policy is captured from Distribution, Global, Environment and
Command configuration; PROJECT is excluded. `LocalGraphPolicy::CONFIG_KEY`
(`data.local_graph`) is explicitly allowed in captured Environment input. Core parses
its captured JSON string while preserving configuration origin. Hub content limits,
Data local graph limits and the finite request deadline use the actual producers.
The caller cannot supply or replace this policy.

## Terminal response and delivery uncertainty

After request admission, the response uses HTTP 200, `application/json` and
`Cache-Control: no-store`. A completed transport contains exactly one bounded
`InstallOutcome` JSON object. HTTP 200 alone does not indicate installation success:
inspect the terminal failure and payload/registry effects.

The outcome preserves `payload` as NotPublished or Published, and `registry` as
NotAttempted, Registered, NotRegistered or Unknown. Only Published plus Registered
without failure is operation success. The SDK retains typed causes locally; the
transport sends only fixed public diagnostics and the outcome. Credentials and Auth
documents are never serialized. Published payloads are not removed or automatically
replayed after registration or delivery failure.

Encoding, output-bound and response-channel failures close the stream with an error
instead of fabricating a terminal receipt. A client with missing, truncated or invalid
terminal JSON must treat the attempt as **MayHaveApplied**, inspect known installation
and registry state, and resolve that state before another attempt. A queued operation
whose Original construction is refused also has no terminal receipt: the real typed
construction error is retained locally. Channel enqueue does not prove client receipt.

## Retained work and shutdown

The handler captures cancellation and a finite deadline before waiting for the owned
job mutex or SDK blocking queue. The retained job creates one server Original from
those captured values. It never starts a fresh timeout after waiting. If cancellation
or expiry already applies, the real `RequestContext::new` rejects construction; no
SDK operation or synthetic NotPublished outcome is created.

Response closure and owner stop cancel the same Core cancellation. The deadline
remains the captured deadline, preserving its distinct causal meaning. The job awaits
the actual blocking SDK worker even after these signals. SDK filesystem and authority
barriers cooperate with cancellation; opaque work is not forcibly preempted and TCP
closure detection is not promised to be instantaneous.

`NativeInstallHttp` owns its JoinSet. Cancelling a shutdown waiter leaves that set and
its owner in shared state so a later waiter can continue joining. `SourceServer`
joins Admin, Native installation, roles and Source work before Foreground removes the
Runtime key, discovery document and singleton ownership. Native HTTP uses a global
registry handle with the same installation identity and physical document/lock;
Source's existing read handle and its policy remain independent. No second issuer,
daemon, registry, journal or grant is introduced.

## Implementation and verification

The endpoint and retained jobs are in `crates/graphit-app/src/native_install_http.rs`;
its policy is composed by `foreground_source.rs`, and listener/state/shutdown by
`source_server.rs`. `native_install.rs` owns actual Auth, native ZIP staging, Data,
AST/Knowledge admission and registry mutation. The physical codec and SDK effects are
specified in [Native AST and Knowledge packages](../development/native-packages.md).

Focused checks are:

```text
cargo xtask cargo test --locked --offline -p graphit-app --test integration -- native_install_http::
cargo xtask cargo test --locked --offline -p graphit-app --lib native_install_http::tests
cargo xtask cargo test --locked --offline -p graphit-app --test integration -- native_install::
```

The HTTP fixtures use a real loopback listener, real Runtime issuer, temporary Auth
store and signed JWT verification. Lifecycle tests occupy the SDK blocking queue
with a real Hub kernel-file lock; they exercise queued expiry, response closure and
an aborted shutdown waiter retaining the same owner. An active installer test waits
for physical AST publication while the actual registry writer is locked, then
observes owner cancellation and preserves the Published effect. The affected Source
tests separately prove a retained job gated before FILE reading and a sibling real
FILE read; this does not prove interruption inside a blocked filesystem syscall.
These checks do not qualify
production JWTs, installed hosts, nonempty Parquet corpus semantics or Windows/macOS.
Kiro uses the current V3 contract (`acp --agent-engine=v3`); global corpus
installation does not select a host. Standard tests emulate the host boundary
with Cargo-built fixtures and require no vendor executable, account or plan.
