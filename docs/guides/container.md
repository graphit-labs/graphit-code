# Run Graphit as a server in a container

The repository `Dockerfile` runs the global daemon as PID 1, serves the Observatory, and publishes
the streamable HTTP MCP listener. Runtime configuration and authentication are deliberately
separate.

Every release publishes a Linux amd64 image to GHCR. A Git release `v0.1.2` publishes Docker tags
`0.1.2`, `0.1`, `0`, and `latest`; Docker tags do not include the `v` prefix. Replace the example
version when selecting another release:

```bash
VERSION=0.1.1
docker pull "ghcr.io/graphit-labs/graphit-code:${VERSION}"
docker volume create graphit-global
docker run -d --name graphit \
  -p 127.0.0.1:8080:8080 -p 127.0.0.1:8081:8081 \
  -v graphit-global:/home/graphit/.graphit \
  "ghcr.io/graphit-labs/graphit-code:${VERSION}"
```

For a local image, run `make build-linux VERSION=dev` before
`docker build -t graphit-code:dev .`. The Dockerfile copies `.build/graphit-linux-amd64`, the same
binary shape produced by release CI, instead of downloading a published release.

On the first container start, the entrypoint runs `graphit setup` in non-interactive mode
in the mounted global directory before it starts the daemon as PID 1. Setup writes `config.json`, so later starts
reuse the volume without repeating setup or overwriting persisted choices. It also writes the
non-secret default provider `local` with ONNX `cpu`/`0`; it does not write an account profile,
identity, MCP key, S3 location, or credential. To override execution, update `local` in the
persistent volume, or create and log in to a separate provider/profile.

The image uses fixed internal ports `8080` for the UI and `8081` for MCP, so callers do not choose
Graphit listener ports for the container. To expose either service on a different host port, change
only the host side of the Docker mapping; for example, `-p 127.0.0.1:9090:8080` publishes the UI at
host port 9090 while it continues to listen on 8080 inside the container. Other container runtime
defaults are declared as environment variables rather than build arguments and use ordinary
Graphit environment precedence. The entrypoint reserves `GRAPHIT_UI_PORT` and
`GRAPHIT_MCP_PORT` for the fixed internal listeners and replaces same-named values supplied by the
runtime. The image declares the fixed non-root user `graphit` (UID/GID
`10001`). The entrypoint, setup, daemon, and explicit commands all start as that user; there is no
root phase or privilege drop. `GRAPHIT_GLOBAL_DIR` defaults to
`/home/graphit/.graphit`, and the image uses the same path as its working directory. That directory
is created and assigned to `graphit` while the image is built, so the default writable named-volume
path works without runtime ownership repair.

The image sets `GRAPHIT_UI_AUTH_ENABLED=true`, so UI data APIs require a browser Broker login.
Set `GRAPHIT_UI_AUTH_PUBLIC_URL` to the external HTTPS origin and enable the Broker's dynamic client
registration before exposing the UI. The login cookie is `Secure` by default. For loopback HTTP
development only, set `GRAPHIT_UI_AUTH_COOKIE_SECURE=false`; set `GRAPHIT_UI_AUTH_ENABLED=false`
to disable web login in a container.

A bind mount or custom `GRAPHIT_GLOBAL_DIR` must already be readable, writable, and traversable by
UID/GID `10001`. Provision it on the host, use an init container, or configure the orchestrator's
volume ownership mechanism such as Kubernetes `fsGroup`. The entrypoint checks access and fails
with a direct diagnostic instead of attempting `chown`. Read-only mounts, root-owned paths without
matching permissions, root-squashed filesystems, and inaccessible parent directories cannot be
repaired by the non-root container.

Except for the two image-reserved listener variables, the variables declared by the Dockerfile are
defaults, not an allowlist. Any other supported
Graphit configuration key can be supplied at runtime through its canonical environment name even
when that name does not appear in the image, for example `modules.sync` as
`GRAPHIT_MODULES_SYNC=false`:

```bash
docker run -e GRAPHIT_MODULES_SYNC=false \
  ghcr.io/graphit-labs/graphit-code:0.1.1
```

This works with `docker run -e`, Compose, Kubernetes, and equivalent environment injection; Docker
does not require a corresponding `ENV` instruction. The name must still map to a configuration key
that Graphit consumes—an arbitrary `GRAPHIT_*` name does not create a new setting.
`GRAPHIT_GLOBAL_DIR` remains the bootstrap exception because it locates the configuration itself.
Provider and AI credentials use the authentication store or their documented native variables
instead of invented `GRAPHIT_*` names.

The first-start setup answers use the same configuration keys and environment-variable resolution
as the running application:

| Variable | Default | Setup option |
|---|---|---|
| `GRAPHIT_HUB_EVENTS_ANONYMIZE` | `false` | Value of `--anonymize-events`. |
| `GRAPHIT_AGENT` | empty | Value of `--agent`; empty keeps Graphit's documented default. |
| `GRAPHIT_CLI` | empty | Value of `--cli`; empty keeps Graphit's documented default. |

The entrypoint passes these values to setup only when `config.json` does not exist. Values are
passed as individual arguments, so spaces are preserved. Setup is always non-interactive in the
image. After setup, the canonical environment variables remain ordinary runtime overrides for
their corresponding configuration keys; alternatively, remove an override and change the
persistent value with Graphit's configuration commands. A restart does not rerun setup while
`config.json` exists.

## Extend the image with an Agent CLI

The published base image does not install a coding-agent CLI and sets
`GRAPHIT_MODULES_AGENT=false`. This is a packaging default, not a limitation of containerized
Graphit. A derived image can install and authenticate any supported CLI in `PATH`, then override
the ordinary configuration environment variables:

```dockerfile
FROM ghcr.io/graphit-labs/graphit-code:0.1.1

# Install the selected Agent CLI and its runtime here, with the executable in PATH.
ENV GRAPHIT_MODULES_AGENT=true \
    GRAPHIT_AGENT=codex \
    GRAPHIT_CLI=codex
```

The same values can be supplied to `docker run -e` instead. `modules.agent`, `agent`, and `cli` may
also be stored through Graphit's configuration system when no non-empty environment variable
overrides them. Enabling the module does not install or authenticate the external CLI; the derived
image or deployment remains responsible for both.

## Configure authentication in the persistent volume

For a service identity backed by the organizational broker:

```bash
docker exec graphit graphit --non-interactive provider add server --type local \
  --broker-endpoint https://broker.example.com \
  --embedding-mode broker --rerank-mode broker

docker exec graphit graphit --non-interactive login \
  --profile service --provider server --username graphit-service \
  --mcp-key "$MCP_KEY" \
  --broker-key "$BROKER_KEY"
```

Avoid passing secrets in a shared shell history. In orchestration, inject them from the platform's
secret store into the one-shot login process. `auth.json` is stored in the mounted global volume,
mode `0600`, under a mode-`0700` directory.

For enterprise identity, configure a Broker provider as documented in [Authentication](authentication.md).
Its interactive login uses the Broker OpenID Provider; storage topology and temporary S3
credentials come from the separately deployed Broker.

## MCP authentication

The local daemon creates a new runtime key at each start; it is visible in **System → Daemon** and
`/home/graphit/.graphit/daemon/mcp.key`. A local provider may also supply a static MCP key. With an active
Broker provider, every remote client sends its own Broker-issued access token as
`Authorization: Bearer ...`. Graphit verifies the signature, issuer, token purpose and exact MCP
resource audience; it forwards that same bearer to Broker APIs. The token may also contain the
Broker API audience, which those APIs validate independently.

A remote client that has no token yet discovers where to get one. An unauthenticated call to the
MCP endpoint returns `401` with a `WWW-Authenticate: Bearer` challenge pointing at this server's
OAuth 2.0 protected resource metadata (RFC 9728), which names the Broker as the authorization
server. The client then registers with the Broker and runs Authorization Code with PKCE on its
own, with no `graphit login` on this container. The daemon advertises this only when the Broker
lists the configured `--mcp-resource` URI among its accepted MCP resources and the request host
and port match; a freshly started container, whose provider is still `local`, announces nothing.

A browser-based MCP client additionally needs its origin declared in `mcp.allowed_origins`
(`GRAPHIT_MCP_ALLOWED_ORIGINS`), which is empty by default and then emits no CORS headers at all.
An agent whose runtime connects server-side does not need it. The Observatory UI requires a
Broker browser session by default in this image. CORS does not authenticate non-browser clients.
Keep both ports on intended private networking or behind TLS.

## Multiple accounts

All profiles remain in the mounted global volume. `login` activates its profile atomically;
`account use NAME` changes the active account and therefore the identity, MCP Bearer, broker access,
Hub ACL view, and user caches together.

```bash
docker exec graphit graphit account list
docker exec graphit graphit account use service
docker exec graphit graphit logout --profile old-service
```

## Health and lifecycle

The image exposes UI port 8080 and MCP port 8081 by default. Its health check always calls the MCP
`/health` endpoint. When `GRAPHIT_MODULES_DAEMON_UI=true`, it also requires the UI `/health`
endpoint; with the UI disabled, only MCP health is required. Stop/restart through the container
runtime; provider and profile state survives in the volume. A daemon restart rotates only the
generated runtime key, not profile credentials.

See [Authentication](authentication.md), [Configuration](configuration.md), and
[Filesystem contract](filesystem_contract.md).
