---
title: Broker native login credential verification
content-type: reference
audience: developers
status: partial
updated: 2026-10-09
references:
  - target: {type: task, id: tsk-3582}
    relation: implements
  - target: {type: task, id: tsk-051d}
    relation: derived_from
  - target: {type: task, id: tsk-136a}
    relation: derived_from
  - target: {type: task, id: tsk-e849}
    relation: relates_to
---

# Broker native login credential verification

`graphit-auth::broker` verifies native login and refresh JWTs with the same
Broker discovery, public-key cache, bounded HTTP client and EdDSA engine used
for incoming MCP identities. The private `TokenPurpose` selects distinct
claim rules; there is no public arbitrary-purpose verifier or injected identity
callback. `VerifiedNativeLogin` cannot become an `AuthenticatedBrokerCaller`,
`LocalParent`, permission or grant.

This leaf verifies received credentials. It does not perform Authorization Code
exchange, generate PKCE/state/nonce, open a browser, receive a loopback callback,
validate UI cookies, rotate a remote refresh token, publish a profile, poll
revocation, or authorize an effect. The separate
[native OAuth protocol](broker_native_oauth.md) now supplies PKCE/state/nonce,
code exchange, rotating refresh and advertised revocation, reusing these
credential checks. Listener/browser, profile publication and application
guards remain required consumers under `tsk-e849`. Expected nonce and subject
parameters are comparison data, supplied by the actual captured OAuth session.

## Captured policy and public discovery

`BrokerPolicy::mcp_resource` is `Option<&str>`. Some validates the captured URI
without rewriting its literal value and requires membership in advertised
`authentication.mcp_resources`. None qualifies an API-only deployment: that
field may be absent or an empty array. A present malformed/null/nonarray or
duplicate list is still rejected. Incoming MCP verification with None fails
before discovery or UserInfo; API-only credentials cannot become MCP callers.

Broker metadata retains the discovered client ID and access-token audience.
The API audience must belong to the advertised audience list. Neither value is
hardcoded, taken from an untrusted JWT override, or persisted as local OIDC
configuration. Exact issuer, same-origin endpoints, openid, Authorization Code,
refresh, PKCE S256, public-client none and EdDSA metadata requirements
remain shared. Discovery metadata is data, not authentication or permission.

## Native initial login

`verify_login(id_token, access_token, expected_nonce, context)` requires both
tokens and a nonempty bounded expected nonce before network work or owned
copies. Each token passes the existing bounded canonical Base64URL/JSON
preflight and the same qualified JWKS key selection and EdDSA signature engine.
Duplicate keys/audiences, invalid numeric dates, malicious key-selection
headers and unbounded extensions are rejected.

The ID token must have the exact issuer, nonempty subject, numeric integer
issued-at/expiration, optional numeric nbf, no future issued-at/nbf, and expiry
after issued-at and now. Every individual token lifetime must fit checked UTC
and monotonic arithmetic before the pair's minimum is chosen. An overflowing
ID expiration cannot hide behind a shorter valid access token.

For a string ID audience, it must exactly equal the discovered client ID.
An audience array must be unique, include that client, and contain only the
client, discovered API audience and configured MCP resource when present.
Multiple audiences require azp equal to the client; a present azp must always
equal it. A present azp or nonce must have a nonempty string shape. Initial
nonce comparison uses existing `subtle::ConstantTimeEq` for contents; input
lengths are bounded and are not promised secret by the comparison. No
nonstandard ID token_use field is required.

The access token must have the exact issuer, token_use access, client_id equal
to the discovered native client, nonempty jti, matching subject, the API
audience and any configured MCP resource. It has the same strict numeric date
and checked lifetime requirements. Its audience may contain additional
members as in the existing incoming-access policy. Incoming MCP keeps its
own rule: a nonempty client_id may identify another registered client.

Profile attributes come only from the verified ID token: nonempty
preferred_username, optional organization and bounded groups. They are
identity data, never roles, teams-to-permissions mappings or an authorization
result. Native verification makes zero UserInfo requests.

## Refresh verification

`verify_refresh(access_token, optional_id_token, expected_subject, context)`
requires bounded nonempty expected subject data and verifies the new access
JWT against it. When an ID token is supplied, issuer/client/audiences/azp,
subject, dates and profile attributes are validated as for login, but no new
nonce echo is required. Both subjects must match the expected one.

When the ID token is absent, `profile_claims()` is None. The consumer must
preserve previously validated profile attributes explicitly; it must not
invent a username from access claims or subject. This result does not restore
authority from private auth.json or validate refresh-token rotation. Those
protocol response and publication checks belong to the next flow consumer.

## Original context and retained validity

`BrokerRequestContext::new` borrows the original Core `SnapshotContext`.
Cancellation and deadline come from that context throughout parsing, cache
access, cryptography and HTTP. It creates no fresh request identity or
extended deadline. Network byte/concurrency caps remain the HTTP producer's
functional limits; there is no additional verification-admission quota.

Returned native and MCP proofs retain that original context, their producing
verifier instance/provider revision, and effective UTC plus monotonic
expiration. Native lifetime is the minimum of ID and access when both exist.
`check(&verifier, context)` validates exact instance/revision, same actual
request identity, cancellation, deadline and both expirations. Cloning the
original Arc retains its identity; a fresh context with identical deadline and
cancellation is rejected. Provider changes require rebuilding the captured
verifier and checking the live provider generation in the application consumer.

Checking does not refresh discovery, keys, UserInfo, expiration anchors,
cache TTL or deadline. UTC rollback before monotonic expiry may remain valid;
rollback cannot extend the captured monotonic expiration. The negative fixture
advances monotonic time past expiry while UTC moves backwards and is denied.
These are local validity checks. Incoming MCP and Web access JWTs are also
verified locally until expiration, without mandatory UserInfo or revocation polling.

Proof fields are private, with no Clone, Debug, Serde or public restoration.
Getters expose verified data only. Raw token strings remain borrowed input,
decoded buffers use existing Zeroizing storage, and no secret token cache or
extra key/HTTP/crypto engine is added. Vendor/header/identity string copies
retain the existing bounded-input allocation caveat and no erasure/RSS promise.

## Finite pair work and evidence

Initial and optional-ID refresh use the shared discovery/JWKS cache.
Both JWT inputs are preflighted before metadata fetch. A native pair gets at
most one forced JWKS reload total, charged to the existing shared cooldown
before an attempt, including failure, cancellation and dropped futures.
The pair reuses its original time anchor across retry. Signature work is at
most two tokens on the first pass and two on that one retry. No second token
creates a second forced fetch or bypasses cooldown.

Implementation: `broker.rs`, private `broker/purpose.rs`, existing
`broker/json.rs`, baseline `broker/tests.rs` and new
`broker/tests/native.rs`. Fixtures use actual Ed25519 signatures and TCP
discovery/JWKS/UserInfo servers, including variable client/API audiences,
API-only metadata, optional refresh ID, claim/header/date/nonce failures,
original request/instance checks, minimum lifetime and key-pair rotation.
Outgoing request counts are asserted; no token-endpoint or profile effect is
part of this leaf. Historical `tsk-2aec` evidence remains unchanged.

Focused commands:

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p graphit-auth --lib broker::
CARGO_INCREMENTAL=0 cargo test --locked -p graphit-auth --doc
CARGO_INCREMENTAL=0 cargo clippy --locked -p graphit-auth --all-targets -- -D warnings
```

The coordinator runs the exact configured fast gate after shared files are
stable. Executed results/hashes belong in new delta evidence; Linux library
fixtures do not prove full OAuth lifecycle, runtime authorization, three OS
qualification.

Shared engine references: [jsonwebtoken Validation](https://docs.rs/jsonwebtoken/latest/jsonwebtoken/struct.Validation.html)
and [subtle ConstantTimeEq](https://docs.rs/subtle/latest/subtle/trait.ConstantTimeEq.html).
The current qualified jsonwebtoken release is 11.1.0; no dependency change is
needed for this purpose extension.

Current Main `tsk-3582` qualification on Linux passed 23 Broker cases, 21 native/Web OAuth cases (including five Web cases), eight verified-subject cases, three affected incoming-service cases and 50 Broker-filtered documentation tests. Strict Auth all-target Clippy and scoped formatting passed. The App-owned current browser suite passed ten cases with zero UserInfo authentication calls, optional profile and signed expiry. Historical mandatory-UserInfo receipts do not qualify this local-JWT contract; PROJECT authorization remains separate.
