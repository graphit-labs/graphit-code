---
title: "Graphit Broker"
description: "End-to-end provider, login, OIDC, ACL, S3 exchange, embedding, rerank, deployment, and troubleshooting guide."
content-type: guide
audience: operators
related:
  - "docs/guides/authentication.md"
  - "docs/guides/oidc-integration.md"
  - "docs/guides/ai_models.md"
---

# Graphit Broker

The optional Graphit Broker lets users consume S3, embeddings and rerank without receiving
the organization's permanent AWS or AI-provider credentials. It is a separate server. A first-class
Broker profile keeps the Broker OIDC session. When S3 is enabled, temporary restricted sessions and
their topology are held only in process memory, independently per storage scope. Local/static providers may still use a static Broker
credential for AI and ACL calls.

```text
Graphit login profile
  └─ refreshable Broker/OIDC access token or static broker credential
       ├─ /v1/hub/access/resolve ─ current SQL grants ─ authorized project selectors
       ├─ /v1/embeddings ─ ACL ─ broker-owned embedding upstream/key/model
       ├─ /v1/rerank     ─ ACL ─ broker-owned rerank upstream/key/model
       └─ /v1/s3/credentials (optional; called on first use of each scope and before expiry)
            └─ project/user/Hub scope + current ACL + private route ─ restricted STS session ─ direct S3 data traffic
```

The broker source, Dockerfile and server-side configuration reference live in the companion
`graphit-broker` repository. This page covers the Graphit client side.

## Broker administration control plane

The companion broker exposes `/admin/` as an OIDC- or local-login-protected control plane. OIDC
uses the same issuer/client and claim mapping as Broker-managed Graphit login; administration is a
role decision, not a second identity provider. The server-side session has CSRF protection, and
local login supports forced password replacement plus required-by-default TOTP MFA. Its UI/API
manages SQL-backed local users, roles, assignments and resource grants. Deployment configuration,
including storage and AI upstreams, is a redacted read-only view and changes on restart.

All durable state is persisted in one configured SQL database. SQLite on a Docker volume is the
default single-node choice; PostgreSQL and MySQL use the same normalized schema and transaction
boundaries for multi-instance deployments. Bootstrap creates the first local `admin` only when the
database has no local users; later administration uses ordinary system-wide role assignments.
Configuration reads redact all secrets. Grant writes require the current ETag, and runtime
configuration changes are deployment-owned. See the broker repository's administration, API,
deployment and operations guides for the server-side procedure.

## Choose the service mode

Providers without broker configuration may select embedding and rerank modes independently. A
provider with broker configuration is an exclusive AI-routing boundary: both embedding and rerank
modes must be `broker`. Provider validation rejects omitted, `local`, `direct`, or `disabled` modes
for either service while a broker is configured.

| Mode | Where model/endpoint is selected | Where secret lives |
|---|---|---|
| `local` | Fixed Graphit local implementation | No secret |
| `direct` | Named Graphit provider (`protocol`, endpoint, model, dimensions) | Login profile API key |
| `broker` | Broker configuration, hidden from user | Broker server; profile has only access token/key |
| `disabled` | No backend | No secret |

Local embedding and rerank model choice is intentionally not configurable. The framework owns
those implementations. In broker mode, the broker owns the upstream protocol and model. In direct
mode, Graphit must know the remote contract because it calls that service itself.

The required broker rerank mode does not enable reranking. `search.rerank=false` remains the
independent activation switch and prevents rerank calls while preserving broker-only routing.

## Broker-managed login (recommended)

Use the `broker` provider type when the Broker must control whether users see local login, OIDC
login, or both:

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com

graphit login --provider company --profile alice
```

Graphit discovers the Broker's OpenID issuer and public client settings, then uses standard OIDC
discovery, an HTTP loopback callback, Authorization Code, PKCE S256, state, nonce and JWKS validation.
It opens the Broker-owned sign-in page.
The Broker performs local password/change/MFA verification or its upstream OIDC flow, then returns
a one-time code to Graphit. Graphit exchanges it for EdDSA-signed ID/access JWTs and an opaque
rotating refresh token; both signed tokens contain the stable Broker `sub`. It never handles the local
password or upstream IdP tokens. The same `OIDCSession` model and refresh path used by direct OIDC
providers is used here; there is no Broker-specific OAuth session or token response.

Before opening the browser, Graphit requires the Broker-specific discovery issuer to match the
configured endpoint and the standard discovery issuer. It also requires Authorization Code and
refresh grants, PKCE S256, public-client token authentication (`none`), EdDSA ID tokens, an explicit
Broker access-token audience, and same-origin authorization/token/JWKS endpoints. This prevents a compromised discovery
document from sending credentials or codes to another origin.

Only methods currently available at the Broker appear. Local login is governed by
`authentication.local_login.enabled` and requires an enabled human local user; OIDC appears when a
browser-capable `authentication.oidc` entry is configured. With upstream OIDC alone the Broker
redirects without rendering a choice page. The Broker's upstream redirect is
`/oauth/oidc/callback`, while Graphit's callback is a dynamically selected loopback port using the
advertised path.

Broker provider setup rejects static credentials, anonymous mode, direct upstream OIDC flags and
token exchange. The same Broker-issued access token is used for broker requests. A Graphit daemon
using this provider validates inbound Bearer JWTs locally through the Broker's discovered
issuer/audience/JWKS before binding their identity to request context.

The saved profile's issuer is the Broker and its subject is the stable Broker `sub`, never the
upstream IdP `sub`. Graphit verifies the signed ID token at login and validates signed Broker access
JWTs locally through standard discovery/JWKS for HTTP MCP. Near expiry, it re-discovers the Broker,
uses the ordinary OIDC refresh grant, requires a new rotated refresh token, and atomically replaces
the saved `OIDCSession`. Reuse of an older refresh token is rejected by the Broker and revokes that
token family. Broker-side revocation cannot invalidate an already issued JWT in this offline
validator; its acceptance window ends at `exp`.

## Local provider with a broker key

This is useful for a workstation or automation identity without OIDC:

```bash
graphit provider add company-services --type local \
  --broker-endpoint https://broker.example.com \
  --embedding-mode broker \
  --rerank-mode broker

graphit login --provider company-services --profile alice-local \
  --username alice --organization acme --team platform \
  --broker-key "$GRAPHIT_BROKER_KEY"
```

Provider configuration contains no account secret. Login stores and activates the profile. The
broker maps the static key to a server-side principal, so the CLI-supplied username/org/team is
local Graphit identity only; broker ACL decisions use the key's configured server identity.

Fully non-interactive:

```bash
graphit --non-interactive provider add company-services --type local \
  --broker-endpoint https://broker.example.com \
  --embedding-mode broker --rerank-mode broker

graphit --non-interactive login --provider company-services --profile ci \
  --username ci --organization acme --team platform \
  --broker-key "$GRAPHIT_BROKER_KEY"
```

`--non-interactive` forbids every fallback prompt. A missing provider, username, required secret,
token or confirmation produces a nonzero error.

## Public/anonymous provider

Anonymous access is explicit at both ends. The provider opts in, login creates and activates an
anonymous profile, and the broker must have a matching `access: anonymous` rule. No username,
organization, team or broker key may be combined with `--anonymous`.

```bash
graphit --non-interactive provider add public-hub --type local \
  --broker-endpoint https://broker.example.com \
  --broker-allow-anonymous \
  --embedding-mode broker --rerank-mode broker

graphit --non-interactive login \
  --provider public-hub --profile public --anonymous
```

Graphit omits `Authorization` for broker requests from this profile. A malformed or rejected token
is never retried anonymously. Broker `global` grants include anonymous and authenticated callers;
`anonymous` grants match only no-token calls. Keep public S3 grants read-only unless public writes
are a deliberate product requirement.

## Direct OIDC provider with all broker capabilities

Register a public/native OIDC client for Authorization Code + PKCE. In the default relay mode the
access token targets one shared MCP/broker audience and carries the claims/scopes the broker
validates.

This is a distinct advanced topology in which Graphit itself is the upstream IdP client. Prefer
the Broker-managed provider above when users should choose local or OIDC login on a Broker page.

```bash
graphit provider add company --type oidc \
  --issuer https://id.example.com/realms/acme \
  --client-id graphit-cli \
  --token-auth-method none \
  --redirect-uri http://127.0.0.1:8765/callback \
  --scopes openid,profile,offline_access,graphit.use \
  --username-claim preferred_username \
  --organization-claim organization.id \
  --teams-claim groups \
  --s3-bucket graphit-artifacts --s3-region us-east-1 --s3-prefix graphit \
  --s3-credential-source sts \
  --sts-role-arn arn:aws:iam::123456789012:role/graphit-user \
  --broker-endpoint https://broker.example.com \
  --broker-token-strategy relay \
  --broker-audience graphit-services \
  --mcp-audience graphit-services \
  --embedding-mode broker --rerank-mode broker

graphit login --provider company --profile alice-acme
```

Login opens the browser, verifies the ID token, exchanges web identity with STS, stores both
refreshable sessions in the restricted global auth file, and activates `alice-acme`. Before either
session expires Graphit renews it and atomically publishes the updated profile.

### HTTP MCP bearer propagation

When Graphit serves Streamable HTTP MCP with this provider active, the client sends its OIDC access
token as `Authorization: Bearer ...`. Graphit verifies signature, issuer, expiry, MCP audience and
the configured claims before creating request context. Every broker call made by that MCP request
uses that request's bearer and identity—not the daemon's active account token and not a static
credential. Concurrent users remain isolated. The broker validates the bearer independently and
evaluates current SQL grants; Graphit-supplied identity fields never authorize the request.

The caller owns renewal of an inbound MCP token. Refresh tokens in the active Graphit profile are
used only for Graphit-initiated work; the daemon never swaps an expired inbound token for another
user's profile token.

Direct relay is the default and simplest deployment. MCP and broker must accept the same audience
and resource indicator; the framework rejects divergent non-empty values:

```bash
graphit provider update company \
  --mcp-audience graphit-services \
  --broker-audience graphit-services \
  --broker-token-strategy relay
```

Use RFC 8693 only when the IdP supports token exchange and the broker requires a distinct
audience. Login requests an MCP-audience subject token; each MCP request exchanges that exact token
for a broker-audience token:

```bash
graphit provider update company \
  --mcp-audience graphit-mcp \
  --mcp-resource https://graphit.example/mcp \
  --broker-audience graphit-broker \
  --broker-resource https://broker.example.com/ \
  --broker-token-strategy token-exchange \
  --broker-token-exchange-endpoint https://id.example.com/oauth2/token
```

Omit `--broker-token-exchange-endpoint` to use discovery's `token_endpoint`. Graphit sends the RFC
8693 subject-token grant with the configured OIDC client authentication method, caches only the
resulting bearer until shortly before its expiry, and keys that cache by provider, source token and
target. Exchange failure is final: Graphit never retries by relaying a token minted for the wrong
audience. The broker must list `graphit-broker` as an accepted consumer audience.

Static `--mcp-key` and `--broker-key` are local-provider mechanisms. OIDC login rejects both so an
OIDC deployment cannot silently bypass its identity provider.

## Direct providers

On a provider without broker configuration, direct mode preserves the option to let the end user
supply an AI key at login:

```bash
graphit provider add direct-openai --type local \
  --embedding-mode direct \
  --embedding-protocol openai \
  --embedding-endpoint https://api.openai.com/v1 \
  --embedding-model text-embedding-3-small \
  --embedding-dimensions 1536 \
  --rerank-mode direct \
  --rerank-protocol openai \
  --rerank-endpoint https://api.openai.com/v1 \
  --rerank-model text-embedding-3-small \
  --rerank-dimensions 1536

graphit login --provider direct-openai --profile alice-direct \
  --username alice \
  --embedding-api-key "$OPENAI_API_KEY" \
  --rerank-api-key "$OPENAI_API_KEY"
```

Supported direct embedding protocols are OpenAI/OpenAI-compatible, Cohere, Voyage and Google
(Gemini API). Direct rerank uses native Cohere, Voyage and Jina endpoints, or simulates rerank for
OpenAI/OpenAI-compatible and Google by comparing query/document embeddings with cosine similarity.
Embedding-simulated rerank requires `--rerank-dimensions`; native rerank rejects it. Direct
endpoint/model/dimensions are topology in the provider; API keys are account secrets in the profile.

## Project-scoped STS storage

A first-class Broker provider contains only the Broker endpoint. When runtime discovery advertises
`graphit-s3-credentials-v2`, `POST /v1/s3/credentials` accepts `project` with a project ULID,
`user`, or `hub`, and returns a
temporary access key, secret, session token, expiration, bucket, region, endpoint, prefixes, and
authorization revision plus the echoed scope. The project ULID selects the resource to authorize;
the client cannot ask for a route, policy, prefix, operation, role, or duration.

When S3 is disabled, valid discovery omits `s3_credentials`. Login and OIDC token renewal continue,
the profile stores no S3 grant, and Tasks, Memory, Knowledge and AST use their normal filesystem
paths. A malformed advertised capability, failed credential issuance, or disappearance of the
capability while renewing an existing S3 grant fails closed; those conditions are not interpreted
as a storage-mode change.

Broker storage can contain multiple named routes. Current SQL grants select the route and logical
prefixes for the authenticated principal and requested scope. All matching S3 grants for one scope must
resolve to one route; ambiguity fails closed. The Broker converts effective read/write/publish/delete
rights into a bounded inline STS policy and calls `AssumeRole` with its private route identity.

Graphit exchanges credentials when each scope is first used and renews them before expiry,
refreshing its Broker OIDC access token first when necessary. The memory-only cache key includes
provider, provider revision, profile and authenticated session identity plus scope/project.
LanceDB, LadybugDB and framework uploads then use normal S3
requests directly. The Broker sees credential issuance and renewal, while object metadata, ranges
and bodies travel between the client and S3.

## Hub ACL authority

If discovery advertises `graphit-hub-access-v1`, the broker's normalized SQL grants are the sole
Hub ACL source. Graphit resolves authorized project selectors through
`POST /v1/hub/access/resolve` and does not read, write, synchronize, or fall back to S3
`projects.json` grant documents. If the capability is not advertised, the provider remains on the
standalone Graphit `projects.json` ACL backend. A malformed capability or broker outage fails
closed; it never changes the selected authority.

The administration UI/API creates grants for `global`, `anonymous`, `authenticated`, `user`,
`team`, `organization`, or exact OIDC `subject`, with exact project IDs or `*`, capabilities,
S3 operations, route and logical prefixes. Every committed CRUD transaction increments the ACL
revision used by discovery and responses, so a concurrent policy change cannot be silently mixed
with an older decision.

The framework never forwards its Broker bearer to S3. Temporary S3 values and topology exist only
in process memory, are isolated per scope, and are renewed before expiry. They are never serialized
to `auth.json`. ACL changes apply to the next issued session; already issued credentials remain
usable until expiry unless the storage provider revokes them. Broker providers require an
authenticated subject and do not issue anonymous S3 credentials.

With Broker S3 enabled, Tasks and Memory use remote LanceDB tables; Hub Knowledge and AST FTS mount
remote LanceDB; Hub Icebug uses LadybugDB against remote Parquet. Local Knowledge/AST FTS retain
their shallow-clone base in S3 with a filesystem overlay, while local Icebug stays entirely local.
Publication and export use the renewable session for their project scope. Without Broker S3, these
stores and local
artifacts stay on the filesystem and remote Hub publication is unavailable.

## Embedding compatibility

Broker discovery publishes revision and dimensions. Graphit includes the broker endpoint,
revision and dimensions in the Lance artifact fingerprint and validates revision/dimensions again
on every embedding response. Embedding and rerank requests do not include `model` or `route`;
each service uses its single broker-configured upstream. Embedding responses do not include `model`.
Change the broker revision whenever the effective vector space changes, then rebuild:

```bash
graphit ast embed
graphit wiki embed
```

Graphit refuses to silently reuse an incompatible index. A model swap behind an unchanged revision
is an operator error because no client can detect semantic incompatibility from equal dimensions
alone.

## Enable rerank

Selecting a provider backend does not add query cost by itself. Enable the second stage separately:

```bash
graphit config --global search.rerank true
```

AST and Knowledge keyword/hybrid searches widen retrieval and call the configured reranker with
readable candidate text, preserve the same candidate set, then trim to the requested limit. A
broker-configured provider always selects the broker reranker. Broker responses carry a revision
that Graphit validates. Set `search.rerank=false` to stop all rerank calls; broker model choice
remains server-side.

## Multiple organizations/accounts

```bash
graphit login --provider company --profile alice-acme
graphit login --provider partner --profile alice-partner
graphit account list
graphit account use alice-acme
graphit logout --profile alice-partner
```

Every login activates its profile. Switching changes identity, tokens, broker key, direct AI keys,
and cache subject as one snapshot. Provider updates advance the revision and require
dependent profiles to log in again.

## Troubleshooting

| Error | Resolution |
|---|---|
| no active account profile | Run `graphit login` or `graphit account use`. |
| broker credential missing | OIDC profile needs an access token; local profile needs `--broker-key`, unless provider/login intentionally use anonymous. |
| discovery version/protocol invalid | Upgrade the client or broker so their contract versions overlap. |
| Broker OIDC metadata rejected | Verify exact issuer/origin, Authorization Code + refresh grants, PKCE S256, auth method `none`, EdDSA, access-token audience/JWKS, and the advertised loopback path. |
| Broker login opens but callback fails | Check that the loopback listener path matches Broker discovery and that state/nonce were not changed by a proxy or browser extension. |
| Broker refresh fails | Log in again; the Broker requires refresh rotation and rejects reuse of an older family member. |
| 401 | Check access-token issuer/audience/expiry/scopes or static key. |
| 403 | Token is valid; inspect broker ACL principal/project/operation. |
| token exchange fails | Verify IdP RFC 8693 support, token endpoint, client authentication, subject-token audience, requested broker audience/resource and consent; there is intentionally no relay fallback. |
| MCP works but broker returns 401 | In relay mode, align accepted audiences; in exchange mode, verify that the broker accepts the exchanged audience. |
| broker Hub access is unavailable | Restore the broker; `projects.json` is intentionally not a fallback for this provider. |
| broker embedding revision or dimensions changed | Rebuild vectors for the current embedding revision and dimensions. |
| broker rerank revision changed | Rebuild the rerank client for the current service revision. |
| broker cannot issue storage credentials | Check the selected route key, STS role/trust policy, session-policy size, bucket policy, region, endpoint, base prefix, and clocks. |
| Broker discovery omits storage credentials | This is expected when Broker S3 is disabled; storage remains local. Enable and configure a route when remote Hub storage is required. |
| multiple matching storage prefixes | Consolidate matching broker ACL rules to one unambiguous prefix. |
| temporary S3 credential expired | Check refresh-token health, Broker/STS reachability and clock sync; log in again if the Broker OIDC session cannot refresh. |
| anonymous request gets 403 | Broker-issued S3 sessions require authentication; use a local provider with direct S3 for anonymous/local storage. |
| non-interactive missing value | Pass the named flag explicitly; no prompt fallback is allowed. |
