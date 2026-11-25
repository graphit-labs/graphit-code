---
title: Broker client identity verification
content-type: reference
audience: developers
status: partial
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-3582}
    relation: implements
  - target: {type: task, id: tsk-a7cf}
    relation: implements
  - target: {type: task, id: tsk-2aec}
    relation: derived_from
  - target: {type: task, id: tsk-e849}
    relation: relates_to
  - target: {type: task, id: tsk-72bc}
    relation: derived_from
---

# Broker client identity verification

`graphit-auth::broker` implements Broker discovery, public-key caching and
local token validation. `BrokerVerifier` does not persist profiles, perform
OAuth login or grant project/Role permissions. The separate
[native OAuth protocol](broker_native_oauth.md) uses the verifier for signed
ID/access tokens, code exchange and rotating refresh.

## Captured policy and public discovery

`BrokerVerifier::new` receives the explicit Broker endpoint, optional literal MCP
resource URI and nonzero provider revision, plus the [bounded HTTP policy](bounded_http_transport.md)
and positive `VerifierLimits`. It reads no CWD, profile or proxy environment.
Provider updates require rebuilding this captured verifier. It provides no
injected metadata, key verifier, clock, HTTP client or authentication callback.

Public discovery fetches `/.well-known/graphit-broker` and
`/.well-known/openid-configuration` below the captured endpoint. It requires
the exact issuer, an advertised MCP resource when configured, `openid`, Authorization Code and
refresh grants, PKCE S256, public-client `none` and EdDSA. Advertised
authorization, token, JWKS and optional revocation URLs must have the captured origin.
UserInfo is not required by access-token verification or discovery.
An omitted revocation endpoint is unsupported; present null/empty/malformed
or foreign-origin revocation metadata is denied.
Discovery needs no login bearer and never refreshes the daemon's active profile.
The resulting metadata retains the discovered client ID and API audience in
private fields with read-only getters; metadata is data. With no captured MCP
resource, absent or empty mcp_resources qualifies API-only metadata. A present
malformed list is denied. Incoming MCP verification then fails before discovery
or UserInfo. Native credential verification is described in
[Broker native login verification](broker_native_login_verification.md).

The MCP resource is an audience identifier. Parsing validates it without
rewriting its literal value. Token audiences must contain that exact value;
a Broker API audience may also be present. The incoming client's `client_id`
must be nonempty and is not required to equal the CLI client ID. That signed
client identity does not supply PROJECT permissions.

## Signature and local identity validity

JWT input has three bounded, canonical Base64URL segments. Header and payload
pass the bounded Serde parser before cryptographic processing. The only allowed
algorithm is EdDSA, with an explicit key ID and a public OKP/Ed25519 key from the
qualified JWKS URL. Header key URLs, embedded keys, certificate chains and
critical extensions are rejected; no token header selects a network endpoint.
Duplicate JSON keys and duplicate JWKS key IDs are rejected.

The mature `jsonwebtoken` verifier checks the signature, issuer, subject,
audience, expiration and any `nbf`, with zero leeway. Typed checks also require
`token_use: access`, nonempty client ID, token ID, numeric
issued-at/expiration, no future issued-at and expiration after issued-at.
Organization and groups remain identity attributes; they confer no permission.

Access JWTs authenticate locally until their signed expiration. Verification
uses trusted cached discovery/JWKS; a cache hit sends no request. No UserInfo,
introspection, blacklist or remote revocation poll is required per request.
Remote revocation, account blocking or claim changes do not invalidate an already
issued access JWT locally before its signed expiration. PROJECT authorization
remains a separate current decision.
A missing display profile does not invalidate signed issuer/sub identity:
`username()` returns an optional value, with no invented name from subject.
Profile enrichment, if a consumer needs it, is optional and grants no permission.
Expiry is anchored to UTC and monotonic time; clock rollback cannot extend it.
The Broker access TTL defaults to ten minutes; its current configuration permits
one minute through one hour for newly issued tokens. The SDK obeys the token's
signed exp. This is separate from the native RoleIssuer's thirty-minute TTL.

The returned caller has private fields and no Clone, Debug, serialization or
public reconstruction. It retains the original Core SnapshotContext and producing
verifier instance/provider revision, UTC and monotonic expiry. Its check method
accepts the same actual request, rejects another request
or verifier, and checks cancellation, deadline and expiry without renewing any
anchor or performing remote authentication. Each new request verifies its token
locally; explicit identity getters support the request ACL
context; `bearer_for_broker` borrows the original verified token for downstream
Broker APIs. It never substitutes the daemon login or a role bearer. This
identity snapshot alone does not authorize later effects or mint agent credentials.
Runtime consumers must enforce live profile/scope and server permission policy.

## Finite work, cache and failure

`BrokerRequestContext::new` borrows the original Core SnapshotContext and takes
cancellation and absolute deadline from it through cache access, parsing,
signature checks and all network steps. It never constructs a new request or
extends a deadline. HTTP byte caps remain limits on actual request/response data.
There is no extra whole-verification semaphore quota. Cache refreshes share
one asynchronous mutex; waiters keep their original cancellation and deadline.

Metadata and decoded public keys have separate positive monotonic cache TTLs.
The cache stores no bearer, user identity or authorization result. A failed reload
preserves the old entry but cannot admit it after cache expiry. Single-flight
refresh shares one fetch and rechecks the cache after obtaining the mutex. An unknown key or invalid signature permits
at most one forced JWKS fetch per admission. The shared cooldown is consumed
before attempting that fetch, including failures, cancellation and future drop.
Repeated failed requests therefore cannot bypass the cooldown by failing early.

Limits cover token bytes, JSON input bytes, nodes, depth, key/string bytes,
collection entries and public-key count. Serde visitors
check before Graphit node/key/string retention; numbers use fixed-width storage.
Vendor escaped-string scratch and crypto/HTTP internals have their own allocation
behavior bounded by input, not a process RSS guarantee. Raw response and decoded
JWT buffers use Zeroizing. Identity strings and vendor/header copies do not have
an erasure guarantee. Errors are closed static categories and never wrap response
bodies, vendor error text, issuer URLs or credentials.

Source: `crates/graphit-auth/src/broker.rs`, `broker/json.rs` and `broker/tests.rs`.
The crypto dependency is pinned to
[jsonwebtoken 11.1.0](https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/)
with default features disabled and the AWS-LC backend shared with Rustls.
Focused tests use real Ed25519 signatures and TCP metadata/JWKS/UserInfo servers.
Focused fixtures use signed tokens and controlled HTTP discovery/JWKS/token
endpoints. Local JWT verification through expiry remains separate from
project authorization and OAuth/profile publication. Test results and native
platform execution are recorded in Task, not inferred from the protocol contract.
