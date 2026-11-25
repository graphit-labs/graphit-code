---
title: Daemon Broker verifier ownership
content-type: reference
audience: developers
status: sdk-qualified-linux
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-5020}
    relation: implements
  - target: {type: task, id: tsk-3582}
    relation: uses
  - target: {type: task, id: tsk-c52b}
    relation: uses
---

# Daemon Broker verifier ownership

`graphit_daemon::broker_verifiers::BrokerVerifierCache` retains the Auth SDK's
discovery and JWKS owner for current admitted Broker services. Keep one cache
alongside the daemon's configured issuer and share it with its Presence and
Inventory consumers. A cache lookup returns public verification configuration;
the caller must still authenticate the access JWT and obtain the current service
credential through `selection.native_credential(&verifier)`.

## Current selection and retained data

`new` receives explicit HTTP limits, verifier limits, loopback policy, proxy and
certificate roots. These settings are captured once; it reads no environment.
`get` takes an actual mutable `ServiceSelection` from the owning issuer. Its
mode, binding and profile checks precede reuse and are repeated before returning.
Opaque Broker credentials and unauthorized service selections do not become
Native credentials through this cache.

The key is provider, provider revision, endpoint and optional literal MCP
resource. Presence uses its captured Broker binding; Inventory obtains its
endpoint and resource from the current `ProviderKind::Broker` configuration,
matching the Auth SDK's Inventory validation. A provider keeps one current entry.
Replacing the binding replaces that entry. An old `Arc` remains alive only while
a caller holds it; it cannot authorize a new selection for a changed binding.
Different providers retain separate verifier owners.

Warm lookups borrow configuration and clone an `Arc`. Only a miss copies the
entry's strings. A short ordinary mutex protects the map; SDK construction occurs
outside it and publication checks whether another caller already installed the
same entry. No map lock is held during network requests. SDK errors preserve
their original typed causes without including credential contents.

## JWT validity and authorization

The verifier checks trusted signature, permitted algorithm, issuer, client,
audience/resource, token use and temporal validity according to
[Broker client verification](broker_client_verification.md) and
[Native login verification](broker_native_login_verification.md).
Discovery and JWKS refresh use the existing SDK's cache and key-rotation handling.
A warm verification makes no discovery, JWKS or UserInfo request. UserInfo is
optional profile enrichment and is not required as evidence of token validity.

An issued access JWT remains locally trusted until its signed expiration.
Revocation, account blocking and changed claims may take until that expiration
to be observed. The Broker's existing access-token default is ten minutes, with
configuration allowing one minute through one hour; this owner introduces no
access-token TTL. PROJECT authorization is a separate current decision, and a
valid JWT or cached verifier grants no project permission.

## Qualification and consumers

The focused `broker_verifiers` integration target uses actual DaemonAuth,
AuthStore and ServiceSelection instances and an Ed25519 TCP discovery/JWKS server.
It exercises shared verification, current configuration replacement, provider
separation, cancellation/deadline, network policy and opaque-mode rejection.
The four focused scenarios passed on Linux, including concurrent signed access
verification with one discovery/JWKS fetch and zero UserInfo or token requests.
The current daemon library and test targets also passed strict Clippy. Run the
functional target with
`cargo test --locked --offline -p graphit-daemon --test broker_verifiers`.
Execution details are recorded in task `tsk-5020` and
`.build/rust-daemon-broker-verifiers-5020/`. The public API's compiled example
checks the Native credential after retrieving its verifier.

Productive Presence subscription (`tsk-60d0`), Native Inventory (`tsk-e6e4`) and
Now/instances CLI/MCP composition (`tsk-d01a`) must retain this owner and use
`native_credential`. Their integrated delivery is separate from this library.
The cache does not create a second AuthStore, install credentials, refresh tokens,
resolve projects or manage subscriber shutdown.

Sources: `crates/graphit-daemon/src/broker_verifiers.rs`,
`crates/graphit-daemon/tests/broker_verifiers.rs`, and
`crates/graphit-auth/src/issuer/service_credentials.rs`.
