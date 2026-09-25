# OpenID Connect through Graphit Broker

Graphit Code supports two provider types: `local` and `broker`. A `broker` provider uses Graphit Broker as its OpenID Provider (OP). The Broker may authenticate a person with its own local account or with an external identity provider. That upstream login is a separate connection managed entirely by the Broker; Graphit Code receives only Broker-issued tokens.

```text
Graphit Code or an MCP client -- Authorization Code + PKCE S256 --> Graphit Broker OP
                                                               |-- Broker local login
                                                               `-- Broker client --> external IdP
Graphit Code or MCP client <-- Broker code, ID token, access token, refresh token
```

## Configure Graphit Code login

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com

graphit login --provider company --profile alice
```

The Broker advertises a public `graphit-cli` client. Graphit Code discovers its issuer, endpoints, signing keys and loopback callback path. It sends an Authorization Code request with state, a PKCE S256 challenge and a nonce, then exchanges the code with the verifier. The public client has no client secret. The nonce is optional in the Authorization Code protocol, but Graphit Code sends one and checks it against the signed ID token.

Graphit Code requires the discovery issuer to match exactly. It verifies the Broker's EdDSA ID token through JWKS, including signature, exact issuer, audience equal to the client ID, numeric `iat`, `exp`, stable `sub` and the nonce when sent. It also verifies the EdDSA access JWT before accepting a login or refresh: signature, exact issuer, Broker audience, `client_id`, `token_use: access`, numeric times, `jti` and the same `sub` as the ID token. The token endpoint must return a Bearer token with positive lifetime. Refresh tokens are opaque and must rotate; a refresh response without an ID token still requires a valid access JWT for the existing subject.

The Broker access JWT is its own signed format. It does not claim the optional RFC 9068 JWT access-token profile. ID tokens follow OIDC Core; access tokens use the Broker's documented contract. Graphit Code does not accept a direct external IdP as a provider.

## Protect an HTTP MCP endpoint

Configure the canonical URI published for this Graphit MCP endpoint in the Broker's `authentication.local.tokens.mcp_resources`. Use that same URI for the Code provider:

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com \
  --mcp-resource https://graphit.example.com/mcp
```

The URI must match the Broker's advertised resource exactly, including scheme, host, port and path. Graphit Code advertises it in OAuth protected resource metadata. A remote MCP client discovers the Broker, registers as a public client if dynamic registration is enabled, and sends that URI as the RFC 8707 `resource` on the authorization request, code exchange and every refresh. The Broker refuses a missing, changed or unconfigured resource. Its access JWT then includes the resource URI in `aud` in addition to the Broker API audience.

For each HTTP MCP request, Graphit Code verifies the access JWT signature and claims, requires `aud` to contain this endpoint's exact resource URI, and checks the token with Broker UserInfo for revocation and matching `sub`. A token issued only for the Broker API, or for another MCP resource, receives `401`. The caller's verified bearer remains bound to that request when Graphit Code calls the Broker. Local `graphit mcp --stdio` uses the daemon's local runtime credential and does not relax remote HTTP validation.

An unauthenticated or invalid-token request receives `401` with a `WWW-Authenticate: Bearer` challenge containing the absolute `resource_metadata` URL. That document names this MCP resource and the Broker issuer; the client then reads the Broker's OIDC discovery. If a verified caller lacks an operation's required scope or permission, the operation returns `403`; an `insufficient_scope` challenge includes the required scopes and the same metadata URL when the operation can identify them.

## Storage and broker calls

A Broker provider receives temporary restricted S3 credentials from the Broker when a storage scope needs them. Graphit Code keeps these credentials in process memory, scoped to the caller and resource. The Broker authorizes embedding, rerank and S3 calls using its own audience and the caller's identity. Broker access tokens that also target an MCP resource can still be used at Broker API endpoints because the Broker audience remains present.

For local credentials or a static Broker key, use `--type local`. The Broker's external IdP client settings, claim selectors and secrets belong in the Broker's configuration, never in a Graphit Code provider.
