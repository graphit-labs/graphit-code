# Authentication providers and account profiles

Graphit Code has two provider types: `local` and `broker`. A provider stores reusable service configuration; a profile stores one login result. `graphit setup` creates the non-secret default `local` provider and does not log anyone in. `graphit login` creates and activates a profile.

Development configurations that still contain a direct `oidc` provider or embedded upstream OIDC settings are rejected. Remove those provider entries and create a `broker` provider for interactive login; there is no automatic conversion.

## Local provider

A local provider uses a username and optional organization and teams supplied at login. It can use local or direct AI services, local files, explicitly configured S3 credentials, and an optional static Broker key for Broker capabilities.

```bash
graphit provider add workstation --type local \
  --embedding-mode local --rerank-mode local

graphit login --provider workstation --profile alice \
  --username alice --organization acme --team platform
```

A local provider may use a Broker key for automation:

```bash
graphit provider add services --type local \
  --broker-endpoint https://broker.example.com \
  --embedding-mode broker --rerank-mode broker

graphit login --provider services --profile ci \
  --username ci --broker-key "$GRAPHIT_BROKER_KEY"
```

The static key does not turn this into OIDC login. The Broker evaluates its own grants for that key. Anonymous Broker calls require explicit `--broker-allow-anonymous` and an anonymous grant at the Broker.

## Broker provider

A Broker provider makes the Graphit Broker the OpenID Provider for login. Graphit Code is a public native client: Authorization Code, PKCE S256, state, nonce, a loopback callback, and no client secret. The Broker may authenticate with a local account or an external IdP; that upstream connection and its secrets remain at the Broker.

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com

graphit login --provider company --profile alice
```

Graphit Code checks exact issuer and Broker origin, discovery capabilities, the EdDSA ID token signature and OIDC claims, and the signed Broker access JWT. The ID token must target the advertised public client; the access JWT must target the Broker audience and carry `token_use: access`, the client ID, token ID, timestamps and the same stable subject. Nonce is checked when sent. The token response must use `Bearer`. Refresh tokens rotate, and Graphit verifies the new access JWT even if no new ID token is returned.

The active profile stores the Broker issuer and stable Broker subject, plus short-lived access and opaque refresh tokens. It never receives an upstream IdP token or password. Broker providers default embedding and rerank to Broker mode. When the Broker advertises S3, Graphit obtains restricted, temporary credentials for each requested storage scope and holds them only in process memory; otherwise storage stays local.

## Web UI login

The Observatory header always shows the current account or **Anônimo**. Set `ui.auth.enabled=true` (or `GRAPHIT_UI_AUTH_ENABLED=true`) to offer login for configured `broker` providers. The CLI profile remains separate: each browser logs in to the Broker with its own public client, Authorization Code, PKCE S256, state and nonce. The Broker must enable `authentication.local.tokens.dynamic_registration`. The web client requests the Broker API resource `<issuer>/v1`; the Broker returns an access token with both that resource and its own API audience. Local providers do not appear as web login choices.

Graphit verifies the Broker ID and access JWTs, then stores the browser's access and rotating refresh tokens in an encrypted `HttpOnly`, `SameSite=Strict` cookie. The temporary callback cookie is `HttpOnly` and `SameSite=Lax`. Both cookies are `Secure` by default. Web tokens are not written to `auth.json` or exposed to JavaScript. When web auth is enabled, the UI's data APIs accept only the browser cookie and use its Broker identity for downstream calls; missing or invalid cookies return `401` while the session endpoint reports **Anônimo**. Logout revokes the refresh grant at the Broker and clears the browser cookie. A daemon restart changes the in-memory encryption key and requires a new browser login.

For external HTTPS access, set `ui.auth.public_url` to the exact public origin, such as `https://code.example.com`. If developing on loopback HTTP, set `ui.auth.cookie_secure=false`; the callback URL then follows the loopback request host and port. Mutating UI requests require a same-origin request header, and the server checks the browser `Origin` when provided. See [Configuration](configuration.md) and [Container deployment](container.md).

## HTTP MCP access

For a Broker-backed HTTP MCP endpoint, configure its canonical public URI in both the Broker's `authentication.local.tokens.mcp_resources` and the Code provider:

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com \
  --mcp-resource https://graphit.example.com/mcp
```

Graphit advertises that URI in OAuth protected resource metadata. A remote public MCP client sends it as `resource` on authorization, code exchange and refresh. The Broker includes the URI in the access JWT `aud`; Graphit requires that exact audience, verifies the signature and claims, then checks the token with Broker UserInfo for revocation and matching `sub`. A Broker API token without the MCP resource audience receives `401` at MCP. Each request retains the caller's bearer for Broker calls, even when several callers share one daemon. `graphit mcp --stdio` connects with the daemon's local runtime credential.

See [OpenID Connect through Graphit Broker](oidc-integration.md) and [Graphit Broker](auth-broker.md) for the protocol and deployment details.

## Profiles and lifecycle

`graphit account list` shows stored profiles; `graphit account use <name>` activates one; `graphit logout --profile <name>` removes one. Updating a provider invalidates dependent login sessions. Local identity and Broker-issued identity are separate: switching profiles changes the active identity, tokens, service configuration and cache subject together.
