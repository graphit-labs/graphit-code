---
title: Broker native OAuth protocol
content-type: reference
audience: developers
status: implemented
updated: 2026-10-11
---

# Broker native OAuth protocol

`graphit_auth::broker::oauth::NativeOAuth` implements Authorization Code with PKCE S256, code exchange, rotating refresh and advertised refresh-token revocation. It borrows the captured `BrokerVerifier` and shares the private OAuth client, HTTP transport, discovery/JWKS cache, clock and signed token verifier with [Web OAuth](rust_web_oauth.md). Credentials are verified data, never a caller, permission or role.

Both protocols use `OAuthLimits`: authorization URL bytes, callback bytes/query entries, form-body bytes, token bytes, response nodes and scalar bytes. These validate actual untrusted input and output. Native pending-flow semaphore quotas and manifest mirror tests were removed; ordinary cancellation, expiry and ownership remain.

All operations receive `BrokerRequestContext` borrowing the actual original `SnapshotContext`. A pending attempt retains its flow, request and provider revision. An independent request cannot consume it; the original Arc can. Discovery, token POST and verification share the original absolute deadline and cancellation.

`begin(redirect, original)` accepts only HTTP literal IPv4/IPv6 loopback, explicit nonzero port, no credentials/query/fragment and the exact advertised callback path in raw URI and parsed URL. Remote/HTTPS/localhost/shortened IPv4 aliases are outside this native contract. Explicit `:80` is retained in the OAuth SDK redirect parameter and token POST. Web callbacks use their separate registered-client protocol.

The OAuth2 SDK constructs the authorization URL with state, nonce and PKCE challenge. The URL contains flow secrets and must not be logged. `PendingLogin` is consumed once by `exchange`; dropping it erases owned secrets. Callback origin/path, state, duplicate/unknown fields and code/error framing are validated before token exchange.

Initial Code login requires ID and access tokens; refresh may omit the ID token. The signed ID token matches native client ID, nonce and subject. Access JWT matches issuer, configured deployment audience, native client, access use, identifier and times, and requested MCP resource when one was captured. Refresh must rotate the opaque refresh token and preserve subject. An omitted ID token supplies no new profile attributes.

`FinalNativeTokens` retains its flow and original, clamps the response lifetime to signed-token lifetime using UTC and monotonic clocks, and rechecks before exposing data. It cannot be restored by JSON, converted into permission or used as an incoming MCP identity. Native login itself makes no UserInfo call; incoming MCP and Web access verification use their separate token purposes and local signed-token validation until expiry.

`revoke` requires an advertised same-issuer endpoint and submits the refresh token with the captured native client ID. Its receipt describes this request's HTTP success, not other token copies. Failures preserve `RemoteEffect::NotSent` or `MayHaveApplied`; token exchange/refresh/revocation are not retried automatically.

The native profile manager consumer adds actual issuer authority, physical AuthStore observation and publication CAS around this protocol. Login/refresh recheck permission, original, provider and current store at their real boundaries. A remote response never overrides a competing local publication. The interactive consumer owns its real loopback listener and callback; this library API alone does not launch the complete executable.

Web client registration, browser cookies, browser PROJECT authorization and server composition are separate consumers. Neither selected profile attributes nor an OAuth identity grant Task/Memory/Knowledge access. No old cookie/configuration/profile conversion is performed.

Affected functional validation covers native protocol, profile login/refresh and original deadline/cancellation/CAS effects. The OAuth SDK [redirect wrapper](https://github.com/ramosbugs/oauth2-rs/blob/5.0.0/src/types.rs) preserves the original redirect text alongside its parsed URL.

Current Main `tsk-3582` qualification on Linux passed 23 Broker cases, 21 native/Web OAuth cases (including five Web cases), eight verified-subject cases, three affected incoming-service cases and 50 Broker-filtered documentation tests. Strict Auth all-target Clippy and scoped formatting passed. The App-owned current browser suite passed ten cases with zero UserInfo authentication calls, optional profile and signed expiry. Historical mandatory-UserInfo receipts do not qualify this local-JWT contract; PROJECT authorization remains separate.

## Device access-only consumer

The [Native Broker Device contract](native_broker_device_login.md) adds a separate initial access-only grant and managed/CLI consumer under tsk-5fb0. It uses empty scopes, a typed optional-metadata Device profile and the existing signed NativeAccess verifier. Its implementation and qualification status are recorded in that contract; Code login and refresh retain the requirements above.
