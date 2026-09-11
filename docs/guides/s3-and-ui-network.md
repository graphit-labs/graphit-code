# S3 STS storage and UI network configuration

## Storage credentials follow the provider

Graphit mounts native LanceDB and Icebug data directly on S3. Authentication and storage are
configured together on the active provider:

- `local` without an S3 bucket uses filesystem paths. With a bucket, it uses login keys, an AWS
  shared-config profile, or the AWS credential chain when explicitly enabled.
- direct `oidc` with S3 exchanges the verified ID token (or the access token when configured) by
  `AssumeRoleWithWebIdentity`. The provider owns the stable bucket/region/endpoint/prefix and STS
  role configuration.
- `broker` uses filesystem paths when valid discovery omits S3 because Broker storage is disabled.
  When discovery advertises `graphit-s3-credentials-v2`, Graphit calls `POST /v1/s3/credentials`
  with its current Broker bearer and the framework-selected `project`, `user`, or `hub` scope. The Broker derives a session policy from current grants and
  returns a temporary access key, secret, session token, expiry, bucket, region, endpoint,
  prefixes, and authorization revision.

An in-process manager obtains and refreshes each Broker scope before expiry. Object requests and their bodies travel
directly between Graphit, LanceDB or LadybugDB and S3; the Broker is contacted only to issue or renew
the session.

The same provider cannot bypass its configured broker for AI services: provider validation requires
both embedding and rerank modes to be `broker`. The broker must advertise the capabilities when they
are used; `search.rerank=false` prevents rerank calls without changing the required backend mode.

The Broker may define several named storage routes, but every effective S3 grant for one requested
scope must resolve to one route. Different projects may receive different topology. Conflicting
matching routes fail closed. The client sends only the scope and, for project scope, the immutable
project ULID; it cannot select a route, bucket, prefix, operation set, role, or duration. The Broker
keeps its permanent S3 identity private and uses it only to call STS.

Temporary credentials are bearer secrets. For a Broker provider, credentials and returned topology
exist only in process memory, keyed by authenticated identity, provider revision and storage scope;
`auth.json` never receives them and a restarted process requests fresh grants. Concurrent requests
for one key share one exchange, while projects and user/Hub scopes never share a credential. An ACL change is reflected
at the next renewal. A credential already issued remains usable until its STS expiry or an
object-store-side revocation, so deployments should choose a duration that matches their revocation
requirements.

Tasks and Memory use authoritative LanceDB tables directly on S3 whenever S3 is configured.
Knowledge and AST FTS from the Hub mount their LanceDB prefixes directly. Local Knowledge and AST
FTS use a shallow clone whose inherited base remains in S3 while new fragments are local until
commit/publication. Local Icebug Parquets are built and queried locally; Hub Icebug Parquets are
queried directly from S3 by LadybugDB. File artifacts that agents require as ordinary files may
still be downloaded to their managed local installation.

## Configuration examples

Direct local credentials:

```bash
graphit provider add workstation --type local \
  --s3-bucket graphit --s3-region us-east-1 \
  --s3-endpoint http://127.0.0.1:9000 --s3-prefix tenant/alice \
  --s3-credential-source login
graphit login --profile alice --provider workstation \
  --s3-access-key "$S3_ACCESS_KEY" --s3-secret-key "$S3_SECRET_KEY"
```

Direct OIDC web-identity STS:

```bash
graphit provider add corporate --type oidc \
  --issuer https://id.example.com --client-id graphit-cli \
  --username-claim preferred_username \
  --s3-bucket graphit --s3-region us-east-1 --s3-prefix tenant/acme \
  --s3-credential-source sts \
  --sts-role-arn arn:aws:iam::123456789012:role/graphit-user
graphit login --profile alice --provider corporate
```

Broker-issued STS:

```bash
graphit provider add company --type broker \
  --broker-endpoint https://broker.example.com
graphit login --profile alice --provider company
```

A first-class Broker provider requires an authenticated user. It uses Broker-issued S3 sessions
only when discovery exposes storage; otherwise its storage remains local. It does not issue
anonymous S3 sessions. A local provider can still use an anonymous Broker identity for AI/Hub ACL
calls, but it must configure its own S3 access if it needs remote storage.

## UI and MCP listeners

`ui.host` and `ui.allowed_origins` remain runtime settings. The UI has no built-in authentication,
and CORS is not authorization. Bind to loopback unless a VPN, firewall, or authenticated reverse
proxy establishes the boundary.

`mcp.host` and `mcp.port` configure the daemon listener. Its generated runtime key rotates at each
start. A local provider may use a static MCP key. With a direct OIDC or Broker-managed provider,
the caller sends an access token that Graphit verifies against the issuer or Broker userinfo and
propagates to the broker. Direct OIDC may use relay or RFC 8693 exchange. Remote MCP
endpoint/audience/resource belongs to the named provider.

## Diagnostics

```bash
graphit provider show company
graphit account show alice
graphit account list
graphit mcp
```

Inspect `~/.graphit/auth.json` permissions, but never publish its contents. It contains identity
and service credentials. Broker-issued S3 credentials and topology are never stored there; local
and direct OIDC profiles may contain their own configured or temporary S3 values.
