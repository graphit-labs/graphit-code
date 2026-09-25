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
  └─ refreshable Broker access token or static broker credential
       ├─ /v1/hub/access/resolve ─ current SQL grants ─ authorized project selectors
       ├─ /v1/embeddings ─ ACL ─ broker-owned embedding upstream/key/model
       ├─ /v1/rerank     ─ ACL ─ broker-owned rerank upstream/key/model
       └─ /v1/s3/credentials (optional; called on first use of each scope/module and before expiry)
            └─ project/user/Hub scope + physical module + current ACL + private route ─ restricted STS session ─ direct S3 data traffic
```

The broker source, Dockerfile and server-side configuration reference live in the companion
[`graphit-broker` repository](https://github.com/graphit-labs/graphit-broker). This page covers the
Graphit client side.

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
password or upstream IdP tokens. Graphit verifies both signed tokens at login and on refresh,
including refresh responses that omit a new ID token.

Before opening the browser, Graphit requires the Broker-specific discovery issuer to match the
configured endpoint and the standard discovery issuer. It also requires Authorization Code and
refresh grants, PKCE S256, public-client token authentication (`none`), EdDSA ID tokens, a Broker
access-token audience, and same-origin authorization/token/JWKS endpoints. These checks prevent a compromised discovery
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
issuer, the endpoint's configured resource audience and JWKS, then checks that token with the Broker's userinfo endpoint before binding its
identity to request context. A remote MCP client therefore needs only its Broker access token:
Graphit uses that caller's token to request temporary STS credentials when it accesses a project or
Hub scope. Those S3 credentials remain in the daemon's process memory, isolated by caller and
scope; they are never sent to the MCP client or saved in the login profile.

A hosted agent obtains that token by itself, with nothing provisioned for it in advance and no
`graphit login` on the machine running the daemon. Point the agent at the MCP URL and the standard
discovery chain does the rest:

1. Its first unauthenticated MCP request answers `401` with `WWW-Authenticate: Bearer` carrying
   `resource_metadata`.
2. That URL serves the OAuth 2.0 protected resource metadata (RFC 9728) naming the Broker as the
   authorization server and this endpoint's canonical resource identifier.
3. The Broker's `/.well-known/openid-configuration` names `registration_endpoint` when
   `dynamic_registration` is enabled.
4. The agent registers itself there (RFC 7591). The Broker only ever issues a **public** client, so
   the response carries no `client_secret`.
5. It then runs Authorization Code with PKCE S256, passing `resource` (RFC 8707) with the canonical
   resource identifier in the authorization, code exchange and refresh requests. The person authenticates in the browser through whatever
   the Broker is configured to use — an upstream IdP or a Broker local user; the MCP path is the
   same either way, because the Broker is itself the OpenID Provider for this exchange.
6. The resulting access token carries both the Broker audience and that resource in `aud`, and it
   is accepted at the MCP endpoint.

The client registered in step 4 lives in the Broker, not in any upstream IdP, and the token is
signed by the Broker's own key. A client that skips `resource` is rejected. A Graphit login without
`--mcp-resource` receives a token for Broker API calls, which is not valid at the HTTP MCP endpoint.

The saved profile's issuer is the Broker and its subject is the stable Broker `sub`, never the
upstream IdP `sub`. Graphit verifies the signed ID and access JWTs at login and validates Broker access
JWTs locally through standard discovery/JWKS for HTTP MCP. Near expiry, it re-discovers the Broker,
uses the ordinary OIDC refresh grant, requires a new rotated refresh token, and atomically replaces
the saved `OIDCSession`. Reuse of an older refresh token is rejected by the Broker and revokes that
token family. The daemon also checks userinfo for each inbound MCP access token, so a revoked token
is rejected even before its JWT expiry.

Explicit revocation follows the same path. Revoking a refresh token at the Broker ends the whole
grant — that token, every access token minted from it, and any further renewal — so an MCP client
holding one of those access tokens is refused on its next request. Revoking an access token ends
only that token and leaves its refresh token working. Revoke the refresh token when someone's
access has to stop.

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

## HTTP MCP bearer propagation

A remote Streamable HTTP MCP client sends a Broker-issued access JWT as `Authorization: Bearer ...`. Graphit verifies its signature through the Broker JWKS, exact issuer, expiration, access-token use and the exact canonical MCP resource in `aud`. It then calls Broker UserInfo with that same bearer to confirm the token is still valid and its `sub` matches. The Broker API audience alone does not authorize access to MCP. The accepted resource is configured with `--mcp-resource` and must be advertised by the Broker.

Every broker call made by that MCP request uses the verified caller's bearer and identity. Concurrent callers remain isolated; the daemon's own login token cannot replace an inbound token. The Broker evaluates current SQL grants independently. The inbound client owns renewal of its token, while the daemon refreshes only its own login profile. Local stdio MCP uses the daemon's local runtime credential.

The Broker's access JWT format is documented by the Broker and includes `token_use: access`; it does not claim the optional RFC 9068 profile. The Broker may include both its API audience and the MCP resource audience so that one token can serve calls to both protected resources. No product-specific scope is required beyond the scopes advertised by the Broker.

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
`graphit-s3-credentials-v3`, `POST /v1/s3/credentials` requires a physical storage `module` in
addition to `scope`: `project` accepts `task`, `memory`, `knowledge`, `ast`, or `hub` and requires a
project ULID; `user` accepts only `memory`; `hub` accepts only `hub`. It returns a
temporary access key, secret, session token, expiration, bucket, region, endpoint, prefixes, and
authorization revision plus the exactly echoed scope, project ID, and module. Graphit rejects an
invalid combination before the request and rejects any response whose echo differs. The project ULID selects the resource to authorize;
the client cannot ask for a route, policy, prefix, operation, role, or duration.

The module follows the physical object directory, not the artifact type. Project `tasks/`,
`memory/`, `knowledge/`, and `ast/` select their matching modules. `project.json`, `registry/`,
`artifacts/`, and `events/` select `hub`; consequently `artifacts/knowledge/` and
`artifacts/ast/` also use `hub`. User `memory/` selects user/memory, while global `registry/` and
`global/rules/` select hub/hub. Unknown or ambiguous project paths fail closed.

When S3 is disabled, valid discovery omits `s3_credentials`. Login and OIDC token renewal continue,
the profile stores no S3 grant, and Tasks, Memory, Knowledge and AST use their normal filesystem
paths. A malformed advertised capability, failed credential issuance, or disappearance of the
capability while renewing an existing S3 grant fails closed; those conditions are not interpreted
as a storage-mode change.

Broker storage can contain multiple named routes. Current SQL grants select the route and logical
prefixes for the authenticated principal and requested scope. All matching S3 grants for one scope must
resolve to one route; ambiguity fails closed. The Broker converts effective read/write/publish/delete
rights into a bounded inline STS policy and calls `AssumeRole` with its private route identity.

Graphit exchanges credentials when each scope/module is first used and renews them before expiry,
refreshing its Broker OIDC access token first when necessary. The memory-only cache key includes
provider, provider revision, profile and authenticated session identity plus scope/project/module.
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

This is a coordinated breaking rollout. Graphit accepts only `graphit-s3-credentials-v3`; a Broker
advertising v2 or any other protocol fails closed. There is no fallback, dual stack, feature flag,
or alias. The `v2/...` S3 key namespace remains unchanged because it versions the storage layout,
not the credential protocol.

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
| Broker OIDC metadata rejected | Verify exact issuer/origin, Authorization Code + refresh grants, PKCE S256, auth method `none`, EdDSA, JWKS, the advertised loopback path, and the Broker access-token audience. |
| Broker login opens but callback fails | Check that the loopback listener path matches Broker discovery and that state/nonce were not changed by a proxy or browser extension. |
| Broker refresh fails | Log in again; the Broker requires refresh rotation and rejects reuse of an older family member. |
| 401 | Check access-token issuer/audience/expiry/scopes or static key. |
| MCP answers 401 with a generic Bearer challenge but no `resource_metadata` | Confirm the active Broker provider has `--mcp-resource` set to this endpoint's exact canonical URI, that the Broker lists it in `mcp_resources`, and that the request host and port match. |
| dynamic registration returns 404 | The Broker has `dynamic_registration` disabled, so `/.well-known/openid-configuration` omits `registration_endpoint`. Enable it, or register the client by hand and configure the agent with that `client_id`. |
| dynamic registration returns `invalid_client_metadata` | The request asked for something a registered client may not have. Registration requires `token_endpoint_auth_method: none`, grants within `authorization_code`/`refresh_token`, `response_types: ["code"]`, and scopes the Broker already supports. Each redirect URI must be absolute, with no fragment or embedded credentials, and either `https` on a non-loopback host or plain `http` on loopback — `https` on a loopback host is refused, as is `http` anywhere else. |
| authorization returns `invalid_target` | The `resource` indicator is not in the Broker's `mcp_resources`, is relative, carries a fragment, or more than one was sent. It must be the exact canonical identifier from the protected resource metadata document. |
| MCP rejects a token the Broker just issued | Require this deployment's exact canonical MCP resource in `aud`; a token with only the Broker API audience is insufficient. Also check JWT signature/claims and the Broker UserInfo response. |
| MCP rejects a token that worked a moment ago | The Broker's `access_ttl` is short, 10 minutes by default. Refresh instead of reusing, and check clock sync between the Broker and the daemon. |
| browser MCP client blocked by CORS | Declare the agent's origin in `mcp.allowed_origins` (`GRAPHIT_MCP_ALLOWED_ORIGINS`), empty by default. The metadata document itself is readable by any origin; the configured policy applies to the MCP endpoint. |
| 403 | Token is valid; inspect broker ACL principal/project/operation. |
| MCP works but broker returns 401 | Confirm the access JWT also carries the Broker API audience and has not been revoked. |
| broker Hub access is unavailable | Restore the broker; `projects.json` is intentionally not a fallback for this provider. |
| broker embedding revision or dimensions changed | Rebuild vectors for the current embedding revision and dimensions. |
| broker rerank revision changed | Rebuild the rerank client for the current service revision. |
| broker cannot issue storage credentials | Check the selected route key, STS role/trust policy, session-policy size, bucket policy, region, endpoint, base prefix, and clocks. |
| Broker discovery omits storage credentials | This is expected when Broker S3 is disabled; storage remains local. Enable and configure a route when remote Hub storage is required. |
| multiple matching storage prefixes | Consolidate matching broker ACL rules to one unambiguous prefix. |
| temporary S3 credential expired | Check refresh-token health, Broker/STS reachability and clock sync; log in again if the Broker OIDC session cannot refresh. |
| anonymous request gets 403 | Broker-issued S3 sessions require authentication; use a local provider with direct S3 for anonymous/local storage. |
| non-interactive missing value | Pass the named flag explicitly; no prompt fallback is allowed. |
