# Authentication providers and account profiles

Graphit separates installation, reusable authentication topology, and login sessions.
`graphit setup` handles runtime/Agent installation and ensures the non-secret default provider
`local`. It never receives identity, OIDC, MCP, broker, custom AI, storage, or credential values.

A named provider contains reusable identity and service configuration: `local`, direct `oidc`, or
`broker`, claim mappings, MCP settings, S3/STS topology where applicable, an optional Graphit
Broker endpoint, AI modes, and independent ONNX
execution blocks for local embedding/rerank services. Without a broker,
embedding and rerank may use local, direct, or disabled modes independently. With a broker, both
must use broker mode. A profile contains one login result: canonical identity, mapped
organization/teams, one standard OIDC session or static keys, and direct AI API keys. Login persists the
profile and activates it atomically.

Setup creates a real provider named `local` with explicit local embedding and rerank services, ONNX
`auto`/`0`, and no broker. It does not create a profile. Without an active profile, AI resolution
uses this persisted provider; if it is absent, the resolver recreates it before use. An active
profile always wins and prevents this automatic recreation. Rerank activation remains off by
default. `graphit init`, AST, Knowledge, Memory, and Task continue to work with anonymous local-only
identity; remote Hub operations do not.

S3 configuration depends on provider type. A local provider may configure bucket, region, endpoint,
prefix and either login credentials, an AWS profile, or the explicitly enabled AWS credential
chain. A direct OIDC provider configures the same topology plus an STS role and exchanges web
identity for renewable temporary credentials. A first-class Broker provider stores no S3 topology.
When discovery advertises `graphit-s3-credentials-v2`, it receives complete topology and a
restricted STS session from the Broker separately for each project, user-memory, or Hub-metadata
scope as that scope is first used. When the Broker has S3 disabled and omits that capability,
login still succeeds and the authenticated profile uses filesystem storage.

## Local identity

Use a local provider without a broker for a fully local installation:

```bash
graphit provider add workstation --type local \
  --mcp-endpoint http://127.0.0.1:8090/mcp \
  --allow-daemon-mcp-key \
  --embedding-mode local --embedding-device auto --embedding-device-id 0 \
  --rerank-mode local --rerank-device auto --rerank-device-id 0

graphit login --profile alice --provider workstation \
  --username alice --organization acme --team platform
```

Interactive provider add/update prompts for `device` and `device_id` only on services whose mode is
`local`, preselecting that provider's current value or `auto`/`0`. Embedding and rerank are stored
independently. Non-interactive commands may omit a local value to preserve the current selection or
accept the new-service default. Direct, broker, and disabled modes store no ONNX block and reject
device flags. The service mode controls this behavior for both local and OIDC authentication types.

## Broker-managed identity

Use a first-class `broker` provider when the Broker must own the login experience and decide which
authentication methods are available:

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com

graphit login --profile alice-company --provider company
```

Graphit Code reads `/.well-known/graphit-broker`, resolves the advertised issuer/client/scopes, then
uses ordinary OpenID Connect discovery, Authorization Code, PKCE S256, state, nonce, JWKS and
userinfo. The Broker page—not the CLI—offers local login and/or organization OIDC according to its
active configuration; upstream-only deployments redirect automatically. After either method
succeeds, Graphit Code verifies the Broker-signed ID token, stores the short-lived opaque access and
rotating refresh tokens in its normal OIDC session, and activates the profile. The profile issuer
and subject are the Broker issuer and stable Broker `sub`; they do not expose the upstream identity
provider's subject. Graphit never receives a local password, upstream OIDC client secret, or
upstream IdP token.

Broker providers are browser-interactive and accept only `--broker-endpoint`; upstream OIDC,
static key, anonymous, audience, resource, and token-exchange flags are rejected. Their embedding
and rerank modes default to `broker`. The Broker-issued access token authenticates Broker calls and,
when configured, inbound HTTP MCP requests are verified through the Broker's advertised
standard `userinfo_endpoint`.

Broker bootstrap discovery is accepted only when its issuer matches the configured Broker and its
authorization, token, JWKS, and userinfo endpoints stay on that origin. Standard metadata must
advertise Authorization Code and refresh grants, PKCE S256, public-client authentication method
`none`, and EdDSA ID tokens. Missing or downgraded metadata fails before the browser is opened.

The older `local` plus static Broker credential topology remains useful for service automation.
Add a broker to a local provider when the organization manages Hub ACL or AI services. This makes
AI routing exclusive: both embedding and rerank backend modes must be `broker` (rerank
execution remains off unless `search.rerank=true`):

```bash
graphit provider add company --type local \
  --broker-endpoint https://broker.example.com \
  --embedding-mode broker --rerank-mode broker

graphit login --profile alice-company --provider company \
  --username alice --broker-key "$GRAPHIT_BROKER_KEY"
```

This local/static provider does not receive Broker-issued S3 credentials. Configure S3 directly on
the local provider when it needs remote storage. First-class `broker` providers receive their S3
topology and credentials dynamically when Broker discovery exposes storage; otherwise they remain
local while retaining Broker identity and AI services.

## Anonymous broker profile

Anonymous use requires explicit opt-in on both sides. The provider permits no-token calls, login
creates an active anonymous profile, and the broker ACL must grant `anonymous` or `global` access:

```bash
graphit --non-interactive provider add public --type local \
  --broker-endpoint https://broker.example.com \
  --broker-allow-anonymous \
  --embedding-mode broker --rerank-mode broker

graphit --non-interactive login \
  --profile public --provider public --anonymous
```

Graphit omits `Authorization`. An invalid bearer token is never retried as anonymous. Public writes
should be granted only when they are an intentional product feature.

## Direct OIDC identity

Graphit implements native-app Authorization Code + PKCE, loopback callback, discovery, JWKS
signature verification, nonce/state validation, claim mapping, and refresh-token rotation.
Canonical identity is verified `iss` + `sub`; configured claims select display/ACL attributes but
cannot replace it.

This direct mode is distinct from `--type broker`: Graphit Code is the IdP client here, so the
provider contains issuer/client/claim configuration. Prefer `--type broker` when login policy and
the choice between local and OIDC authentication belong to the Broker.

```bash
graphit provider add corporate --type oidc \
  --issuer https://id.example.com/realms/acme \
  --client-id graphit-cli \
  --token-auth-method none \
  --redirect-uri http://127.0.0.1:8765/callback \
  --scopes openid,profile,offline_access,graphit.use \
  --username-claim preferred_username \
  --organization-claim organization.id \
  --teams-claim groups \
  --s3-bucket graphit --s3-region us-east-1 --s3-prefix tenant/acme \
  --s3-credential-source sts \
  --sts-role-arn arn:aws:iam::123456789012:role/graphit-user \
  --broker-endpoint https://broker.example.com \
  --broker-token-strategy relay \
  --broker-audience graphit-services \
  --mcp-audience graphit-services \
  --embedding-mode broker --rerank-mode broker

graphit login --profile alice-acme --provider corporate
```

For CLI-initiated work, the refreshed access token authenticates each broker call. For HTTP MCP,
Graphit verifies the caller's MCP-audience access token and propagates that request bearer to Hub
ACL resolution, Broker calls, embeddings and rerank. It never substitutes the active profile's
token across users. Direct OIDC S3 uses a renewable web-identity STS session; the optional Broker
validates its own calls independently. See the
[detailed OIDC integration guide](oidc-integration.md) for IdP registration, claims, audiences,
route policies, and examples.

The default `relay` strategy requires MCP and broker to accept one shared audience. Select
`--broker-token-strategy token-exchange` for an explicit RFC 8693 exchange when the IdP supports it
and the broker has a distinct audience; exchange failure fails closed without relay fallback.

## Non-interactive policy

Global `--non-interactive` forbids prompts, confirmations, browser/device flows, editors, pagers,
and interactive chat. Missing input is a deterministic error naming the required flag. Local
login supplies `--username` and applicable `--mcp-key`, `--broker-key`,
`--embedding-api-key`, or `--rerank-api-key`. OIDC automation supplies externally acquired,
still-verifiable tokens:

First-class Broker login is intentionally unavailable in non-interactive mode because it requires
the Broker-owned browser page. Use a Broker service credential with a local/static provider for
automation.

```bash
graphit --non-interactive login --profile ci --provider corporate \
  --access-token "$OIDC_ACCESS_TOKEN" \
  --refresh-token "$OIDC_REFRESH_TOKEN" \
  --id-token "$OIDC_ID_TOKEN" \
  --token-expires-at 2026-09-07T12:00:00Z
```

Destructive commands require `--yes`, such as
`graphit --non-interactive provider remove NAME --cascade --yes`.

## Multiple accounts and lifecycle

```bash
graphit provider list
graphit provider show corporate
graphit provider update corporate --client-id graphit-cli-v2
graphit account list
graphit account show alice-acme
graphit account use personal
graphit logout
graphit logout --profile personal
```

Updating a provider increments its revision and invalidates dependent sessions. Removing a
referenced provider is refused unless `--cascade --yes` is explicit. Logout removes only the
selected profile; it never activates another profile implicitly.

## Storage and redaction

State lives in `~/.graphit/auth.json` (or `$GRAPHIT_GLOBAL_DIR/auth.json`), under an owner-only
directory (`0700`) and file (`0600`) with atomic replacement and cross-process locking. Command
output redacts OIDC client secrets, tokens, MCP/broker keys, direct AI keys, and temporary or direct
S3 secrets. Direct OIDC temporary S3 sessions may be persisted and are renewed before expiry.
Broker-issued S3 credentials and returned topology are held only in process memory; restart causes
fresh scoped exchanges.

There is no migration or compatibility path for incompatible provider/profile schema versions.
Recreate providers and log in again after a development schema change.

## Hub ACL authority

The provider selects exactly one authorization backend. If its broker advertises
`graphit-hub-access-v1`, current normalized broker SQL grants are authoritative and Graphit never
reads, writes, or falls back to S3 `projects.json` ACL documents. Without that broker capability,
Graphit keeps its standalone `projects.json` resolver for global, anonymous, authenticated, user,
and team grants. An outage or invalid response from the selected backend denies the operation; it
does not switch authority.
