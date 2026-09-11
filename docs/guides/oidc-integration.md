# Integrating an OpenID Connect provider

This guide covers direct OIDC, where Graphit Code is the native client, and an optional Graphit
Broker. Interactive login uses Authorization Code
with PKCE and an HTTP loopback callback, verifies the ID token, maps trusted claims, persists a
refreshable profile, and activates it atomically.

When the Broker should own login policy and offer both local and OIDC methods, use a first-class
Broker provider instead:

```bash
graphit provider add company --type broker --broker-endpoint https://broker.example.com
graphit login --provider company --profile alice
```

In that mode, the Broker is itself the OpenID Provider. Graphit discovers its issuer, public client,
scopes and loopback path, then runs the same standard OIDC client flow used for direct providers.
The Broker may authenticate locally or act as an upstream IdP client; none of the upstream
issuer/client/claim flags belong in Graphit Code configuration.

The two browser hops are intentionally separate protocol relationships:

```text
Graphit Code (native OIDC client)
  └─ authorization request + state + nonce + PKCE → Broker issuer
       └─ Broker-owned login page
            ├─ local password → forced change → TOTP enrollment/challenge
            └─ or Broker (confidential OIDC client) → organization IdP → Broker callback
       └─ Broker authorization code → Graphit loopback callback
  └─ code + verifier → Broker token endpoint
       └─ Broker EdDSA ID/access JWTs + opaque rotating refresh token
```

The outer issuer and `sub` seen by Graphit are always the Broker's. The Broker derives that stable
subject from its canonical local or upstream identity. An upstream authorization code, ID token,
access token, refresh token, client ID, or client secret never crosses the Broker/Graphit boundary.

Remote storage follows provider type. A direct OIDC provider with S3 configured uses
`AssumeRoleWithWebIdentity`; a first-class Broker provider receives a restricted STS session plus
topology per requested project, user-memory, or Hub-metadata scope when it discovers
`graphit-s3-credentials-v2`. In both enabled cases the credentials are renewable and
LanceDB/LadybugDB send object traffic directly to S3. Either provider uses local
storage when S3 is not configured or advertised.

## Supported identity paths

| Graphit provider | Human authentication happens at | Issuer and subject saved by Graphit | Token validation |
|---|---|---|---|
| `broker`, local method | Broker SQL password/change/TOTP flow | Broker issuer + stable Broker `sub` | Broker ID/access JWTs through issuer, advertised audience and JWKS |
| `broker`, upstream method | Organization IdP through the Broker's confidential OIDC client | Broker issuer + stable Broker `sub` | Same Broker JWT/JWKS contract; upstream tokens remain inside the Broker |
| direct `oidc` | Configured organization IdP | Configured IdP `iss` + `sub` | ID/access JWT signature, issuer, audience, time, nonce, and configured claims |

The first two rows are one OIDC client implementation from Graphit Code's perspective. Local
password versus upstream OIDC is an internal Broker authentication-method choice, not a different
Graphit protocol. The direct row uses the same `OIDCClient` flow but keeps issuer, client,
redirect, claim mappings, audiences, and optional RFC 8693 exchange in the Graphit provider.

## Prerequisites

| Component | Required contract |
|---|---|
| OIDC issuer | Stable HTTPS issuer with `/.well-known/openid-configuration` |
| Native client | Authorization Code enabled; PKCE `S256`; no client secret preferred |
| Redirect | HTTP loopback callback; fixed port if the IdP cannot register a dynamic one |
| ID token | Signed RS256/384/512, ES256/384/512, or EdDSA/Ed25519 JWT with valid `iss`, `sub`, `aud`, `exp` |
| Claims | Required string username; optional string organization and string/array teams |
| Access token | MCP audience/resource; either also accepted by the broker or exchangeable to its audience with RFC 8693 |
| Renewal | Refresh token for sessions that must survive access-token expiry |
| Broker | HTTPS discovery and capability endpoints; ACL rules for the trusted principal |

Graphit Code does not use Device Authorization, Resource Owner Password, or Client Credentials as
the interactive login for a configured provider. A Broker may expose its separate local
access-token-only device flow, but `--type broker` deliberately uses standard browser OIDC.
Automation may acquire direct-provider tokens externally and pass them to strict non-interactive
login; Broker automation uses a Broker service credential with a local/static provider.

## End-to-end flow

The following is the direct-OIDC topology. For the Broker-managed topology, use the nested flow
above; after the Broker issues its own tokens, downstream Broker calls use those tokens directly
and do not use RFC 8693 exchange.

```text
graphit provider add
  └─ issuer, client, scopes, claim mappings, S3 topology and STS role
       ↓
graphit login
  ├─ state + nonce + PKCE verifier
  ├─ browser → IdP → loopback authorization code
  ├─ code exchange and ID-token signature/issuer/audience/nonce validation
  ├─ map username, organization, teams from verified claims
  └─ persist + activate refreshable profile
  └─ exchange ID/access token with STS when S3 is configured
       ↓
each storage session
  ├─ refresh the OIDC token when needed
  ├─ call AssumeRoleWithWebIdentity before S3 credential expiry
  └─ use the temporary key, secret and session token directly with S3
```

For a Broker provider, login records only whether Broker S3 is available. Each storage scope later
calls `/v1/s3/credentials` on first use and renewal; Broker ACL changes are reflected at the next renewal. An already issued STS session remains valid until its expiry or
storage-side revocation.

## 1. Register the native client

Create a public desktop/native client in the IdP:

1. Enable Authorization Code.
2. Require or allow PKCE with `S256`.
3. Disable implicit flow unless another application independently needs it.
4. Register `http://127.0.0.1` dynamic loopback ports when supported, or an exact fixed callback
   such as `http://127.0.0.1:8765/callback`.
5. Permit refresh tokens/offline access if unattended renewal is required.
6. Release only the claims and scopes Graphit and the broker need.

An installed CLI cannot protect an embedded client secret. If policy nevertheless requires a
confidential client, Graphit supports `client_secret_post` and `client_secret_basic`; the secret is
stored in the restricted global auth file and redacted from output.

Verify discovery before configuring Graphit:

```bash
OIDC_ISSUER=https://id.example.com/realms/acme
curl --fail --silent --show-error \
  "$OIDC_ISSUER/.well-known/openid-configuration"
```

The returned `issuer` must exactly match the configured issuer (apart from normalized trailing
slash) and publish authorization, token, and JWKS endpoints.

## 2. Design claims and audiences

Canonical security identity is always verified `iss` + `sub`. Claim flags only map attributes:

```json
{
  "iss": "https://id.example.com/realms/acme",
  "sub": "00u-stable-subject",
  "aud": "graphit-cli",
  "preferred_username": "alice",
  "organization": {"id": "acme"},
  "groups": ["platform", "security"]
}
```

This maps with:

```text
--username-claim preferred_username
--organization-claim organization.id
--teams-claim groups
```

An exact top-level claim name wins over dotted-path interpretation, which allows namespaced claims
such as `https://claims.example.com/teams`. Required mappings must be present in the ID token, not
only in `userinfo` or the access token. Keep group lists bounded; some IdPs emit overage markers.

The ID token audience identifies the CLI client. The access token audience/resource identifies
the API. Choose one of two explicit bearer strategies:

- `relay` (default): the same token authenticates MCP and broker, so both non-empty audiences and
  resource indicators must match and both services validate the token independently;
- `token-exchange`: the subject token is minted for MCP, then Graphit uses OAuth 2.0 Token Exchange
  (RFC 8693) to obtain a short-lived broker-audience token for each request identity.

Use exchange only when the IdP implements RFC 8693 and a distinct resource audience is required.
Failure never falls back to relay.

## 3. Create the provider

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
  --s3-bucket graphit-artifacts \
  --s3-region us-east-1 \
  --s3-prefix graphit \
  --s3-credential-source sts \
  --sts-role-arn arn:aws:iam::123456789012:role/graphit-user \
  --broker-endpoint https://broker.example.com \
  --broker-token-strategy relay \
  --broker-audience graphit-services \
  --broker-resource https://broker.example.com/ \
  --mcp-audience graphit-services \
  --embedding-mode broker \
  --rerank-mode broker
```

Provider-specific authorization parameters may be repeated:

```bash
graphit provider update corporate \
  --auth-param prompt=select_account \
  --auth-param login_hint=alice@example.invalid
```

Graphit refuses overrides for `state`, `nonce`, `redirect_uri`, `scope`, `client_id`,
`response_type`, `code_challenge`, `code_challenge_method`, `audience`, and `resource`.

The S3 topology and STS role above belong to this direct OIDC provider. Omit them for local-only
operation. A first-class `broker` provider accepts none of these flags because the Broker either
supplies topology and temporary credentials through discovery or intentionally leaves storage local.

## 4. Log in and inspect redacted state

```bash
graphit login --profile alice-corporate --provider corporate
graphit account show
graphit account list
graphit provider show corporate
```

Login prints and opens the authorization URL, waits for the local callback, verifies the result,
then stores and activates the profile. The loopback response renders a self-contained, build-branded
confirmation page (or a generic failure state) without echoing the authorization code, state, or
tokens; the terminal remains authoritative for final token validation. A second login creates an
isolated profile:

```bash
graphit login --profile bob-corporate --provider corporate
graphit account use alice-corporate
```

Updating a provider increments its revision and invalidates dependent sessions:

```bash
graphit provider update corporate --scopes openid,profile,offline_access,graphit.use
graphit login --profile alice-corporate --provider corporate
```

State is stored in `~/.graphit/auth.json` (or `$GRAPHIT_GLOBAL_DIR/auth.json`) with directory mode
`0700`, file mode `0600`, atomic replacement, and cross-process locking. Outputs redact client
secrets, access/refresh/ID tokens, MCP/broker keys, direct AI keys, and S3 secrets. Direct OIDC
temporary S3 credentials are stored in this restricted file so they can be refreshed and resumed.
Broker-issued S3 credentials and returned topology remain only in process memory and are reacquired
after restart.

## 5. Authenticate HTTP MCP and broker calls

For Streamable HTTP MCP, the caller sends:

```http
Authorization: Bearer eyJ...
```

Graphit verifies that JWT against OIDC discovery and JWKS, checks the configured MCP audience and
expiry, maps only claims from the verified token, and attaches both identity and raw bearer to the
request context. Hub ACL resolution, Broker credential renewal, embeddings and rerank use that request bearer.
The active profile is configuration context; its token is never substituted for another HTTP
caller. This keeps concurrent users isolated.

In relay mode, configure one shared API audience:

```bash
graphit provider update corporate \
  --mcp-audience graphit-services \
  --broker-audience graphit-services \
  --broker-token-strategy relay
```

For a separate broker audience:

```bash
graphit provider update corporate \
  --mcp-audience graphit-mcp \
  --mcp-resource https://graphit.example/mcp \
  --broker-audience graphit-broker \
  --broker-resource https://broker.example.com/ \
  --broker-token-strategy token-exchange \
  --broker-token-exchange-endpoint https://id.example.com/oauth2/token
```

The exchange request uses grant type
`urn:ietf:params:oauth:grant-type:token-exchange`, the incoming access token as `subject_token`,
access-token subject/requested token types, the configured target audience/resource, and the
provider's `none`, `client_secret_post`, or `client_secret_basic` client authentication. If the
endpoint flag is omitted, Graphit uses OIDC discovery's `token_endpoint`. Successful results are
cached only until shortly before expiry and isolated by provider, source token and target. Invalid
or failed exchange returns an error; the MCP-audience token is never leaked to the broker as a
fallback.

An HTTP MCP client must renew its own token and send the replacement. Graphit's refresh-token
rotation applies to its saved login profile for CLI/background work, not to an inbound request.

## 6. Configure the broker to validate OIDC

On the broker, configure the same exact issuer, its accepted shared or exchanged broker audience,
algorithms, and claim paths used to build its trusted principal. Example broker YAML:

```yaml
authentication:
  oidc:
    - issuer: https://id.example.com/realms/acme
      audiences: [graphit-services]
      username_claim: preferred_username
      organization_claim: organization.id
      teams_claim: groups
```

Resource ACLs do not live in YAML. After signing in to `/admin/`, create normalized grants through
the Resource grants screen or `POST /admin/api/v1/grants`. A typical team grant selects
`access=team`, `principal=platform`, capabilities `hub,s3,embeddings,rerank`, exact project IDs,
read/write constraints, a private S3 route, and allowed logical prefixes. The SQL grant database
is then authoritative for every request.

The broker must validate access tokens independently; Graphit verifying the ID token does not
authorize an API request. Validate signature, exact issuer, intended audience/resource, expiry,
and any required scopes. Do not trust username, organization, or teams sent separately by a local
client when a bearer token or server-side broker key defines the principal.

The broker uses one browser-capable entry from that same `authentication.oidc` list for both its
administration session and Broker-managed Graphit login. Register a confidential web application
with the exact callback `https://BROKER/oauth/oidc/callback`, a client secret, and Authorization
Code (the broker also sends PKCE S256). Administration still requires an assigned system role;
authentication never bypasses RBAC or consumer resource grants. The first local administrator is
created on an empty database with the broker's `--bootstrap-admin` command.

For a first-class `broker` provider, Graphit validates both discovery layers before opening the
browser. The Graphit-specific document must advertise `type: openid_connect`, the exact Broker
issuer, a public client ID, scopes containing `openid` and `graphit.use`, and an absolute loopback
callback path, plus a non-empty `access_token_audience` contained in its advertised audiences.
Standard discovery must advertise Authorization Code and refresh grants, PKCE `S256`, token
authentication method `none`, and EdDSA ID-token signing. The authorization, token, and JWKS URLs
must remain on the configured Broker origin. A
cross-origin endpoint, mismatched issuer, missing capability, invalid callback path, or downgraded
algorithm is rejected before login.

The daemon validates Broker access JWTs offline. It checks signature, issuer, the discovered
audience, expiry, Broker client ID, `graphit.use`, subject, username and optional identity claims.
It does not call userinfo for each MCP request, so an already issued token remains valid there until
`exp` even if it is revoked at the Broker in the meantime.

## 7. Configure Broker STS issuance

Only a first-class Broker provider obtains storage topology from the Broker. Each named route owns
one permanent access key/secret and an assumable role, normally injected through the deployment's
secret manager:

```yaml
services:
  s3:
    enabled: true
    default_route: primary
    routes:
      primary:
        region: us-east-1
        bucket: graphit-artifacts
        base_prefix: graphit
        access_key_id: ${PRIMARY_S3_ACCESS_KEY_ID:?required}
        secret_access_key: ${PRIMARY_S3_SECRET_ACCESS_KEY:?required}
        sts_role_arn: arn:aws:iam::123456789012:role/graphit-broker
        sts_endpoint: ""
        sts_session_name: graphit-broker
        sts_duration: 1h
```

Multiple routes may point to different buckets, regions, accounts, endpoints and prefixes. A SQL
resource grant selects `s3_route`; all effective S3 grants for one requested scope must select the same
route or issuance fails closed. `POST /v1/s3/credentials` accepts a framework-selected `project`,
`user`, or `hub` scope and a project ULID only for project scope. It returns
only the restricted temporary key, secret, token, expiry, topology, permitted root prefixes, and
authorization revision. Broker-issued S3 credentials require an authenticated principal.

## 8. Configure Hub ACL authority

Broker discovery always advertises `graphit-hub-access-v1`. For a provider that selects it, Graphit
calls `/v1/hub/access/resolve` with the same relayed or exchanged request bearer and treats the
broker SQL database as the only Hub ACL source. It never consults or updates S3 `projects.json`
grants in this mode. A bad response, revision mismatch, `401`, `403`, or outage fails closed.

Providers without the capability keep Graphit's standalone `projects.json` ACL backend. There is
no migration, synchronization, dual read/write, or fallback between the two authorities.

## 9. Non-interactive automation

`--non-interactive` forbids every prompt and browser/device flow. Acquire tokens in a trusted CI
step, then invoke:

```bash
graphit --non-interactive login \
  --profile ci --provider corporate \
  --access-token "$OIDC_ACCESS_TOKEN" \
  --refresh-token "$OIDC_REFRESH_TOKEN" \
  --id-token "$OIDC_ID_TOKEN" \
  --token-expires-at 2026-09-07T18:00:00Z
```

The ID token is still checked against discovery/JWKS and the provider client ID. Missing flags,
invalid claims, or a provider without a non-interactive flow return a nonzero error; Graphit never
falls back to a prompt.

## Provider examples

### Keycloak

- Use the realm issuer, e.g. `https://id.example.com/realms/acme`.
- Configure a public client with Standard Flow and PKCE `S256`.
- Add protocol mappers for organization and groups into the ID token.
- In relay mode, use the same API audience in Graphit and the broker; in exchange mode, configure
  Keycloak token exchange and accept only the exchanged broker audience.

### Microsoft Entra ID

- Use the tenant-specific v2 issuer, not `common`, for stable authorization boundaries.
- Register a mobile/desktop redirect URI and enable public-client flow as required.
- Map app roles or bounded group claims; handle group overage explicitly outside token claims.
- Configure the broker for the exact tenant issuer and API application-ID URI audience; confirm
  the tenant supports the requested token-exchange scenario before selecting it.

### Auth0

- Use `https://TENANT/` as the issuer and register the exact loopback callback.
- Enable Authorization Code with PKCE for a Native application.
- Emit custom authorization attributes as namespaced ID-token claims.
- Set the shared audience through the API identifier, or configure Auth0's supported token-exchange
  flow before selecting a distinct broker audience.

## Troubleshooting

| Symptom | Likely cause | Correction |
|---|---|---|
| Discovery issuer mismatch | Proxy/alias/tenant path differs | Use the exact issuer published by discovery and token `iss` |
| `redirect_uri_mismatch` | Callback not registered exactly | Register the fixed loopback URI and set `--redirect-uri` |
| Invalid client | Wrong public/confidential mode | Match `none`, `client_secret_post`, or `client_secret_basic` |
| Signature/JWKS failure | Wrong issuer, key, algorithm, or stale metadata | Verify discovery/JWKS and supported RS/ES algorithm |
| Audience failure | Token minted for another client/API | Align shared audiences for relay, or verify MCP subject and broker target audiences for exchange |
| Token exchange failure | IdP/tenant/client does not permit RFC 8693, or target is wrong | Verify endpoint, client auth, grant permission, audience/resource and consent; no relay fallback occurs |
| Username/teams missing | Claim not in ID token or wrong path | Add an ID-token mapper and update claim flags |
| Refresh fails | No offline access, consent, or refresh token revoked | Enable refresh grant and log in again |
| Broker returns 401 | Token invalid for broker | Compare issuer, audience/resource, expiry, signature, and scopes |
| Broker returns 403 | Current SQL grant does not match trusted principal/project/operation | Inspect broker Resource grants and mapped claims |
| Broker discovery has no S3 | STS capability disabled; local storage is expected | No action for local operation; configure an S3 route and STS role when remote Hub storage is required. Broker-routed AI services remain independent. |
| Credential issuance fails | Broker cannot assume the selected route role or policy is too large | Check route key, role trust, session-policy size, bucket policy, endpoint, clocks, and logs |
| S3 returns 403 | Session expired or its policy lacks the requested prefix/action | Trigger renewal, inspect current grants and check clock skew |
| Provider changed | Provider revision advanced | Log in again for that profile |

Do not paste raw tokens, broker keys, temporary S3 credentials, or broker-side cloud credentials into issue
reports. Record redacted provider output, request ID, HTTP status, issuer, and claim names.

## Security checklist

- Use a tenant/realm-specific HTTPS issuer and exact audience/resource validation.
- Prefer a public native client with Authorization Code + PKCE.
- Prefer direct relay for one shared API audience; use RFC 8693 only for deliberate audience
  separation and never configure a fallback.
- Keep state, nonce, redirect URI, and PKCE under Graphit's control.
- Map stable IDs rather than mutable display names.
- Keep access-token and STS credential lifetimes short enough for the required revocation window.
- Protect the broker administration API separately from end-user authentication.
- Keep permanent cloud credentials only in Broker deployment configuration/secrets. Direct OIDC
  providers contain topology and role metadata, never a permanent key.
- Broker-issued S3 sessions require authentication; never downgrade an invalid bearer to anonymous.
- Do not log temporary access keys, secrets, or session tokens.

## References

- [OpenID Connect Discovery 1.0](https://openid.net/specs/openid-connect-discovery-1_0.html)
- [OAuth 2.0 for Native Apps, RFC 8252](https://www.rfc-editor.org/rfc/rfc8252.html)
- [OAuth 2.0 Token Exchange, RFC 8693](https://www.rfc-editor.org/rfc/rfc8693.html)
- [Keycloak OIDC layers](https://www.keycloak.org/securing-apps/oidc-layers)
- [Microsoft identity platform OIDC](https://learn.microsoft.com/en-us/entra/identity-platform/v2-protocols-oidc)
- [Auth0 Authorization Code with PKCE](https://auth0.com/docs/get-started/authentication-and-authorization-flow/authorization-code-flow-with-pkce/call-your-api-using-the-authorization-code-flow-with-pkce)
- [AWS Signature Version 4](https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv.html)
