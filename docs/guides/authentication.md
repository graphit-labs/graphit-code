---
references:
  - target: {type: task, id: tsk-aeda}
    relation: relates_to
---

# Authentication providers and account profiles

A provider stores reusable, non-secret topology. An account profile stores a login/session and its
secrets in the private installation. `provider` operations do not log in; `login` creates/activates
a profile, `account use` selects an existing one. Identity and PROJECT permission remain separate.
See [OIDC](oidc-integration.md) for token contracts and [S3](s3-and-ui-network.md) for storage.

## Local provider

A Local profile uses its explicit local identity and permissions. Its two inference services can
select Local, Direct, Broker or disabled independently. It can use local storage or its explicit
standalone S3 configuration. An optional static Broker key is automation identity, not OIDC.

```sh
graphit provider add workstation --type local \
  --embedding-mode local --embedding-device cpu --embedding-device-id 0 \
  --rerank-mode local --rerank-device cpu --rerank-device-id 0
graphit login --provider workstation --profile workstation --username workstation
```

The selected local model still needs an installed `daemon.models` slot. Configuring a rerank
service does not enable rerank. Direct keys belong to the profile at login, not project Config.
A Local provider with Broker service mode needs its configured Broker endpoint and admitted
`--broker-key`, or explicit anonymous policy/grants where supported. No local username can
impersonate a verified Broker subject or acquire another user's Memory.

## Broker provider and native login

Graphit Broker is the OpenID Provider; an upstream enterprise IdP is configured entirely there.
Code receives Broker tokens, never the upstream password/token. A Broker provider accepts only
Broker or explicit disabled embedding/rerank and cannot bypass it with Local/Direct inference.

```sh
graphit provider add company --type broker --broker-endpoint https://broker.example.com
graphit login --provider company --profile workstation
graphit account list
graphit account show workstation
```

Native login is a public-client Authorization Code flow with PKCE S256, state, nonce and loopback
callback. Validate discovery issuer/origin, trusted JWKS, permitted algorithm, ID-token issuer/
audience/authorized party/nonce/subject/times and access-token issuer/audience/client/token-use/
subject/times. The token response must be Bearer with valid lifetime. Refresh rotates its opaque
credential and verifies the returned access token even when no new ID token is supplied.

Access JWTs are verified locally against trusted cached discovery/JWKS, including key rotation.
UserInfo is optional enrichment for missing profile fields, not mandatory proof of validity or
revocation. Account blocking, token revocation or changed claims can take until access expiration
to become visible to a JWT consumer. Current PROJECT authorization is still independently checked;
a valid JWT is not permission for every project. Token duration is the configured Broker access
lifetime, not another silently invented Code TTL. The current Broker default is 600 seconds
(`broker-server/src/config.rs::access_ttl`); an operator can configure the actual issuance lifetime.

`--non-interactive` suppresses local prompts/browser opening; the printed authorization URL still
requires completion before the finite callback deadline. Actual agent/model use needs its own
provider account; documentation/contract tests use local controlled peers without paid plans.

## Profile lifecycle and running services

```sh
graphit account use workstation
graphit refresh
graphit logout --profile workstation
```

Provider update preserves omitted topology fields but changes its revision; dependent credentials
must match the new provider generation. Log in again where required. There is no merging of keys
from another profile or automatic conversion of retired Auth formats.

When the installation's daemon owns its singleton, CLI Auth operations use its genuine Runtime
administrative route. Otherwise supported native/local operations acquire the installation-only
owner and clean up their standalone resources. Failed authentication or a busy owner is not
permission to start another daemon or replace its key. Reads redact credentials.

Records consumers reuse native refresh for an expiring active profile under actual Runtime
administrative admission, preserving provider/issuer/subject/CAS and Original. Incoming HTTP and
Browser callers retain their own credentials and refresh responsibilities. Other consumers can
report NeedsRefresh explicitly; `graphit refresh` renews the active native profile. A dropped
rotation may have changed the remote token: preserve the producer's uncertain result instead of
replaying it blindly. A fresh valid login remains usable. No per-request UserInfo/introspection,
credential polling or silent storage fallback is introduced.

## Web UI login

The current Rust Browser uses Global `task.browser`, not `ui.auth.enabled`, `ui.auth.public_url`
or `ui.auth.cookie_secure`. Its strict object selects trusted Broker provider names,
`project_endpoint`, optional `public_origin`, explicit local HTTP exception and session timings.
Absent Browser configuration is headless service state, not an anonymously authorized data API.
See [all fields/defaults](../reference/configuration-keys.md#browser-object).

```sh
# Illustrative production topology; configure this real provider first.
graphit config --global set task.browser '{"providers":["company"],"project_endpoint":"https://broker.example.com/v1/project/access/authorize","public_origin":"https://graphit.example.com"}'
graphit daemon restart
graphit ui
```

The Browser logs in independently of the active CLI profile. Its native public Web client retains
PKCE/state/nonce, exact configured callback origin and signed token checks. Only configured Broker
providers appear. Mutation handlers check the exact browser Origin and current operation authority;
Host/forwarding headers cannot replace trusted origin configuration.

**Cookies contain opaque random login/session handles, not access/refresh tokens.** The daemon
holds OAuth flows and credential-bearing sessions in process memory. Cookies are HttpOnly,
path `/`, SameSite Lax for callback/login and Strict for the session; HTTPS uses Secure and
`__Host-` names. Explicit loopback HTTP development uses its separate unprefixed cookie names.
Credentials and rotation are serialized under the actual session owner, not exposed to JavaScript
or written to `auth.json` by Web login.

Logout removes/cancels the server session and clears its cookie; a removed handle cannot continue
accessing that Browser session. This immediate handle invalidation is separate from the JWT
expiration window for a copied bearer used at another admitted API. Restart/drain loses in-memory
sessions/flows, requiring a fresh login. Sharing a cookie key cannot replicate this state across
instances. `ui.auth.cookie_encryption_key` is a redacted installation-secret key accepted by generic
Config, but it is **not consumed by the current opaque-handle Browser implementation**; it does
not make sessions durable, encrypt token cookies or enable replica failover.

If an old `/oauth/login?id=...` page says the form/challenge is invalid, start a fresh login from
`graphit ui` and complete that same flow. Do not reuse an expired challenge from a replaced daemon.
The UI exposes a bounded error/retry result rather than secrets or raw IdP responses.

## HTTP MCP access

Configure the exact public MCP resource at the Broker and Code provider:

```sh
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com \
  --mcp-resource https://graphit.example.com/mcp
```

The remote public client requests that exact resource during authorization, token exchange and
refresh. The access JWT must include this audience and the Broker audience for subsequent Broker
calls. Code verifies complete trusted token claims locally; a Broker-only token or one for another
resource is rejected. The caller's verified bearer remains bound to its request and is not replaced
with the daemon's active profile. Local stdio uses the installation's admitted runtime transport.
OAuth protected-resource metadata and Bearer challenges describe the resource; CORS and a public
URL do not authorize PROJECT access.

## Storage and USER isolation

Valid S3 discovery obtains independent restricted STSv3 sessions for scope/module. Broker-issued
credentials remain in memory and are reused until renewal, not written to Auth. Failure to obtain
credentials or access storage is explicit. Only valid disabled-storage discovery selects local;
no error-driven local mirror exists. USER Memory includes verified issuer/subject and installation,
not the project's identity or a display username. [S3](s3-and-ui-network.md) specifies exact roots.

## Implementation map

`graphit-auth` owns provider/profile validation, JWT/OIDC and service credentials. App's
`auth_cli`, `auth_active`, `admin_auth` and record-storage adapters compose those APIs. Browser
`browser_auth.rs` / `browser_auth/session.rs` owns opaque cookies and server sessions;
`foreground_tasks.rs` owns Browser topology. Historical token-cookie/Go descriptions do not
describe this implementation. Platform/library tests retain their own execution scope.
