---
title: Rust Web OAuth protocol
content-type: reference
audience: developers
status: implemented
updated: 2026-10-16
references:
  - target: {type: task, id: tsk-5651, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-4546, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses
---

# Rust Web OAuth protocol

`graphit_auth::broker::oauth::web::WebOAuth` registers a public client for the configured browser UI callback and implements Authorization Code with PKCE S256, code exchange, rotating refresh and advertised refresh-token revocation. It shares the private OAuth client, token codec, bounded HTTP transport, discovery/JWKS cache, clock and signature verifier with `NativeOAuth`. Both use the seven functional fields of `OAuthLimits`; pending-flow quotas are absent.

The configured callback is an exact HTTPS URI without credentials, query or fragment. Explicit development policy may permit literal loopback HTTP. Native login retains its separate literal-loopback HTTP, explicit nonzero port and advertised callback-path contract; Web callback support does not loosen it.

`register(original)` posts public UI-client metadata to the same-issuer registration endpoint advertised by OIDC discovery. It requests a public client with `token_endpoint_auth_method: none`, Authorization Code and refresh grants, code response, issuer scopes and one exact callback. Missing registration is Unsupported. Invalid metadata, rejected registration and mismatched returned redirect/client metadata produce causal errors. Only this response constructs the private `RegisteredWebClient` binding; a supplied client ID or audience cannot restore it.

Registration derives `application_type` from the callback already admitted by `WebOAuth::new`: explicitly permitted literal-loopback HTTP requests `native`; HTTPS requests `web`. The response must match that type. An omitted response type has the OIDC standard Web default, so it cannot satisfy a Native request; null, unknown and mismatched values are rejected. This classification does not create a Native personal profile or change the `AuthenticatedWebCaller` authority boundary. The local daemon’s explicitly enabled loopback login keeps the same PKCE S256, state, nonce, ID-token, refresh, exact redirect and resource checks. [RFC 8252](https://www.rfc-editor.org/rfc/rfc8252.html#section-7.3) describes loopback redirection and public Native clients.

Web authorization, code exchange and refresh request the canonical issuer `/v1` resource for Broker API calls. ID tokens must match the registered client ID and nonce; multiple audiences require the matching `azp` and only the registered client, configured deployment API audience and requested resource. Access tokens must match issuer, deployment API audience, `/v1` resource, registered `client_id`, token use, subject and valid signed times/identifier. The preconfigured Native-login client ID and MCP endpoint audiences cannot substitute for the registered UI client values.

`begin` returns a single-use `PendingWebLogin` borrowing the flow, registered client and original request. `exchange` consumes it after exact callback, state and original checks. Cancellation and absolute deadline remain those of that original. A later HTTP callback consumer must retain this flow inside an owned async job; it must not forge static borrows or substitute a new original.

`exchange` and `refresh` return owned `WebCredentials` data for a server session. Access/refresh tokens are zeroized on drop; the ID token is discarded after verification. Refresh credentials must rotate and preserve subject; a response without an ID token has no new profile attributes. These values neither activate a personal native profile nor carry PROJECT permission.

Each browser data request calls `verify_access` with its current original, registered binding and current access token. It validates the signed Web token locally until expiration with trusted cached discovery/JWKS, producing a borrowed `AuthenticatedWebCaller` without UserInfo. This type is distinct from an MCP caller. Its local `check` rechecks verifier/provider revision, original and expiry; it performs no remote revocation check.

PROJECT permission requires the separate maintained Broker authorization service and Records admission producer. Cookies, browser login/session endpoints, route composition and React/Gantt remain separate consumers. Profile attributes and verified identity never imply Task permission. Never place these credentials in browser JS/localStorage, catalogue/history or logs.

```rust
use graphit_auth::broker::{BrokerVerifier, oauth::{OAuthLimits, web::WebOAuth}};
fn web<'a>(
    verifier: &'a BrokerVerifier,
    limits: OAuthLimits,
) -> Result<WebOAuth<'a>, graphit_auth::broker::oauth::NativeError> {
    WebOAuth::new(verifier, limits, "https://workspace.example/auth/callback", false)
}
```

The [OIDC registration standard](https://openid.net/specs/openid-connect-registration-1_0.html) defines client registration and exact redirects; [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707.html) defines resource indicators. The maintained Broker adds its signed access-token client/resource contract. The implementation reuses the OAuth2 and JWT SDKs; it creates no second transport, permission policy or session store.

Web ID and access tokens may omit display profile attributes. Web login then returns no profile claims, and `AuthenticatedWebCaller::username()` returns None; literal issuer/sub remain required and no name is fabricated. Native profile publication retains its separate verified-profile requirements.

Earlier Main `tsk-3582` qualification on Linux passed 23 Broker cases, 21 native/Web OAuth cases (including five Web cases), eight verified-subject cases, three affected incoming-service cases and 50 Broker-filtered documentation tests. Strict Auth all-target Clippy and scoped formatting passed. The App-owned current browser suite passed ten cases with zero UserInfo authentication calls, optional profile and signed expiry. Historical mandatory-UserInfo receipts do not qualify this local-JWT contract; PROJECT authorization remains separate.
