# S3 Credentials and UI Network Configuration

This guide covers the operator-facing network settings introduced by the
interactive setup: optional S3 credentials and the UI server bind/CORS policy.

## Configuration precedence and scope

All keys use the normal Graphit configuration resolver. From highest to lowest
priority, the effective value comes from inline configuration, a `GRAPHIT_*`
environment variable, project configuration in `graphit.lock.json`, global
configuration in `~/.graphit/config.json`, a compiled brand default, and finally
the built-in default.

Project scope is the default. Use `--global` only when a setting should apply to
every project on the machine:

```bash
graphit config ui.host 127.0.0.1
graphit config ui.allowed_origins http://localhost:8080
```

Both commands above write `graphit.lock.json`; add `--global` to write
`~/.graphit/config.json` instead.

## S3 keys

| Key | Environment variable | Purpose |
|---|---|---|
| `hub.bucket` | `GRAPHIT_HUB_BUCKET` | S3 or S3-compatible bucket used by Hub and shared memory |
| `hub.region` | `GRAPHIT_HUB_REGION` | AWS region |
| `hub.endpoint` | `GRAPHIT_HUB_ENDPOINT` | Custom endpoint, for example MinIO or LocalStack |
| `hub.prefix` | `GRAPHIT_HUB_PREFIX` | Optional key prefix inside the bucket |
| `hub.access_key_id` | `GRAPHIT_HUB_ACCESS_KEY_ID` | Optional explicit access key |
| `hub.secret_access_key` | `GRAPHIT_HUB_SECRET_ACCESS_KEY` | Optional explicit secret key |
| `hub.subject.user` | `GRAPHIT_HUB_SUBJECT_USER` | Trusted deployment user ID fallback; keep global, never project-scoped |
| `hub.subject.teams` | `GRAPHIT_HUB_SUBJECT_TEAMS` | Comma- or semicolon-separated trusted team memberships |

For an ephemeral publisher, keep these values in GitHub environment variables and secrets rather
than writing them to global configuration. The complete branch/tag workflow, including remote
embedding credentials and noninteractive setup, is documented in
[Publishing Graphit artifacts from GitHub Actions](github-actions-artifacts.md).

`graphit setup` asks for the Hub username first; an empty answer selects the teamless `anonymous`
fallback. The same answer can be supplied as `--username`, including `--username ""`. Setup asks
for the access key and secret only when a bucket was provided. The secret prompt does not echo
input. A complete pair is saved in the
global configuration and is used by all S3-backed consumers, including the Hub,
memory publication, LanceDB/object-store access, and LadybugDB.

### Leaving credentials blank

Leave both credential prompts blank to keep using the AWS SDK provider chain.
This supports the existing mechanisms, such as environment variables, shared
AWS configuration and credential files, profiles, container credentials, and
instance/workload roles. A partial pair is never retained: if either value is
blank, Graphit removes both explicit credential keys from global configuration.

For the provider-chain path, standard AWS variables remain available:

```bash
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
export AWS_SESSION_TOKEN=...   # only when the provider requires a session token
```

An explicit pair configured through Graphit is a static pair. It is not combined
with an unrelated `AWS_SESSION_TOKEN` from the process environment.

### Credential storage warning

Explicit credentials are stored as plain text in `~/.graphit/config.json`. The
file is written with mode `0600`, and `graphit config get` / `graphit config list`
redact `hub.secret_access_key`, but this is not encryption at rest. Prefer the
AWS provider chain, short-lived credentials, or workload roles whenever
possible. Never commit the global config file or copy it into a project.

For non-interactive configuration, mark secrets explicitly:

```bash
graphit config --global hub.access_key_id "$S3_ACCESS_KEY_ID"
printf '%s' "$S3_SECRET_ACCESS_KEY" | graphit config --global --secret hub.secret_access_key
```

To return to the provider chain, run `graphit setup` and leave either credential
blank, or unset both keys:

```bash
graphit config --global --unset hub.access_key_id
graphit config --global --unset hub.secret_access_key
```

Adding S3 settings to an existing project does not relocate its databases. Populated LanceDB stores
remain on the filesystem and continue incrementally; the first Hub publication seeds an empty remote
channel from that local state. Git-aware shallow hydration is used only for an empty local store. If
local and remote state already exist independently, select which side is authoritative instead of
expecting an automatic merge. See
[Publishing Graphit artifacts from GitHub Actions](github-actions-artifacts.md#update-and-cleanup-semantics).

## Project identity and authorization

S3 credentials authenticate the Graphit process to object storage. They do not identify the person
making a request and do not grant visibility to every project. Hub v2 roots project data below the
immutable project ULID and resolves the mutable globally unique name through a separate name
directory. See [Project Identity](../specs/project_identity.md).

A trusted authentication boundary supplies `user_id` and `team_ids`. Graphit then unions the
applicable global, authenticated, user, and team project grants. The `anonymous` subject instead
reads only global and `v2/anonymous/projects.json`. Missing grant files contribute no access and an unavailable or invalid
authorization backend fails closed. A shared MCP bearer key controls access
to the endpoint but does not distinguish users; CORS does neither. Multi-user deployments therefore
need an authenticated proxy/identity adapter and a Hub service or temporary S3 credentials scoped
to authorized `v2/projects/<ULID>/` prefixes. See
[Hub Access Control](../specs/hub_access_control.md).

The local `~/.<brand>/hub/cache/` tree holds only bounded, subject-isolated metadata. It never
replaces ACL validation. Clearing it is safe; a network failure cannot turn a cached positive grant
into offline authorization.

## UI bind address and browser origins

| Key | Environment variable | Built-in default |
|---|---|---|
| `ui.host` | `GRAPHIT_UI_HOST` | `127.0.0.1` |
| `ui.allowed_origins` | `GRAPHIT_UI_ALLOWED_ORIGINS` | localhost origins only |

`ui.host` controls the network interface on which `graphit ui` listens. The
default `127.0.0.1` accepts local connections only. Set `0.0.0.0` explicitly
when the dashboard must listen on every IPv4 interface:

```bash
graphit config ui.host 0.0.0.0
```

`ui.allowed_origins` controls browser CORS access to all UI API endpoints. It is
a comma-separated list of exact origins, including scheme and port when present:

```bash
graphit config ui.allowed_origins "https://graphit.example.com,https://admin.example.com"
```

Whitespace and duplicate entries are removed. A configured list replaces the
localhost default; it does not extend it. Use `*` only when every browser origin
is intentionally allowed:

```bash
graphit config ui.allowed_origins "*"
```

That wildcard is unsafe for most shared deployments.

The bundled UI calls the API through same-origin `/api` URLs, so it normally
needs no CORS entry of its own. Add origins only for a separately hosted frontend
or another browser client.

### Security boundary

The UI server does not provide authentication. CORS is a browser policy, not an
access-control mechanism for curl, scripts, or hosts on the network. Because the
default bind is `0.0.0.0`, protect reachable deployments with a firewall, VPN, or
an authenticated TLS reverse proxy. Do not expose the server directly to the
public Internet.

An authenticated reverse proxy must pass identity through a trusted integration understood by the
deployment; merely making up user or team headers is not sufficient. Until such an integration is
configured, operate the UI as a single-user service even if S3 contains user/team ACL documents.

Typical safe profiles:

```bash
# Local workstation
graphit config ui.host 127.0.0.1

# LAN/VPN, with a separately hosted frontend
graphit config ui.host 0.0.0.0
graphit config ui.allowed_origins "https://graphit.internal.example"
```

## Troubleshooting

Inspect the effective values without exposing the secret:

```bash
graphit config --get --global hub.bucket
graphit config --get --global hub.access_key_id
graphit config --get --global hub.secret_access_key
graphit config --get ui.host
graphit config --get ui.allowed_origins
```

- An S3 authentication error with blank Graphit credentials means the AWS
  provider chain could not find usable credentials; inspect the active profile,
  environment, or workload role.
- A browser CORS error means the page's exact `Origin` is not in
  `ui.allowed_origins`. Include the scheme and non-default port.
- A connection refusal usually means the selected interface is unreachable or a
  firewall blocks the automatically selected free port.
- A successful request from curl despite a browser CORS error is expected: CORS
  is enforced by browsers.

See also [Configuration Module](../specs/config_module.md) and
[Hub S3 Object Layout](../specs/hub-s3-object-layout.md).
