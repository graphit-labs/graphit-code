# Running Graphit Code as a server in a container

The repository ships a `Dockerfile` at its root that produces a **server**: the daemon running as
PID 1, publishing an **MCP endpoint** and a **UI** on two ports.

**Any MCP-capable AI agent connects to it.** Claude Code, Codex, Gemini, Cursor, OpenCode, Copilot,
Kiro, an in-house client — anything that speaks MCP over HTTP with a bearer token. The agent runs
wherever the developer is and brings its own model; the server supplies the code graphs, the compiled
documentation wikis and the memory it reasons over. One container serves a whole team, and nobody has
to index anything locally.

The image carries **no source checkouts and no coding-agent CLI**, and needs neither. It answers about
**artifacts** — knowledge and AST contexts installed into its global store — which are addressed by
name and version rather than by a path on disk.

The framework is not compiled here; the image installs a published release. See
[why](#why-the-image-does-not-build-from-source).

## Quick start

```bash
docker build -t graphit-code .

docker run -d --name graphit \
  -p 127.0.0.1:8080:8080 \
  -p 127.0.0.1:8081:8081 \
  -v graphit-global:/opt/graphit \
  graphit-code
```

- **UI** → <http://127.0.0.1:8080>
- **MCP** → `http://127.0.0.1:8081/mcp`, bearer-authenticated

`docker logs graphit` shows the daemon's output. `docker inspect --format '{{.State.Health.Status}}' graphit` reports the healthcheck, which polls the UI's `/health`.

> **Neither port is authenticated.** The UI has no accounts, and it serves the MCP bearer key so the
> interface can offer a copy button — so anything that can reach the UI port can also take over the
> MCP endpoint. Publishing both to `127.0.0.1`, as above, is the only configuration that is safe by
> default. See [Exposing it to other people](#exposing-it-to-other-people).

## Connecting an AI agent

### 1. Get the endpoint and the key

The bearer key is regenerated on **every daemon start** and written to
`/opt/graphit/runtime/daemon/mcp.key` with mode `0600`.

The UI's **daemon page** (`/system/daemon`) shows the MCP port, the endpoint, and the key behind a
copy button — masked on screen, full value to the clipboard. Or from a shell:

```bash
docker exec graphit cat /opt/graphit/runtime/daemon/mcp.key
```

### 2. Point your client at it

Most MCP clients take an HTTP server with a header. The shape, whatever the client:

```json
{
  "mcpServers": {
    "graphit": {
      "url": "http://your-server:8081/mcp",
      "headers": { "Authorization": "Bearer <key>" }
    }
  }
}
```

Clients that only speak stdio can wrap it with any stdio-to-HTTP MCP bridge. A local Graphit install
does this for itself — `graphit init` writes a stdio proxy that reaches the daemon over loopback —
which is the same transport this exposes over the network.

### 3. Ask for artifacts, not paths

This is the one thing an agent gets wrong against a server. There is no checkout here, so tools take
a **`context`** instead of a `project_dir`:

```
graphit_ast_query      project_dir omitted, context: "acme-api@2.1.0"
graphit_knowledge_search                context: "acme-docs"
graphit_ast_source                      context: "acme-api@2.1.0"
```

Omitting both is refused with a message that says so, rather than silently answering from an empty
store. Tools that genuinely need a project — indexing, linting, exporting, anything that writes —
keep requiring `project_dir` and are not what a server is for.

What works with no project at all: the AST query/schema/source/search tools, knowledge search, the
wiki browse/source/xrefs/log tools, the whole Hub surface, and the user scope of memory (keyed by the
machine, so it is a real scope rather than a fallback).

## Publishing artifacts to it

The server is only as useful as what has been installed into it. That needs a Hub bucket:

```bash
docker run -d --name graphit \
  -p 127.0.0.1:8080:8080 -p 127.0.0.1:8081:8081 \
  -v graphit-global:/opt/graphit \
  -e GRAPHIT_HUB_BUCKET=my-artifact-bucket \
  -e GRAPHIT_HUB_REGION=us-east-1 \
  --env-file ./graphit.secrets.env \
  graphit-code
```

Then, from a developer machine with a checkout, publish the project's graph and documentation; and on
the server, install them:

```bash
# on the server
docker exec graphit graphit hub search acme
docker exec graphit graphit hub install acme-api --type ast
docker exec graphit graphit hub install acme-docs --type knowledge
docker exec graphit graphit hub list
```

Installed contexts live on the `/opt/graphit` volume, which is what makes the server's content
survive a restart.

### Or index a checkout on the server

Mounting a checkout is supported when you would rather the server build the indexes itself:

```bash
docker run -d --name graphit \
  -p 127.0.0.1:8080:8080 -p 127.0.0.1:8081:8081 \
  -v graphit-global:/opt/graphit \
  -v "$HOME/projects/acme-api:/srv/acme-api" \
  graphit-code

docker exec -it graphit sh -c 'cd /srv/acme-api && graphit init && graphit sync'
```

Agents then address it with `project_dir: /srv/acme-api`. **Pick a path and keep it:**
`global.lock.json` records projects by absolute container path, so mounting the same checkout
somewhere else on a later run registers it twice and orphans the first entry.

## Why the daemon is PID 1

The daemon owns the MCP server: it generates the bearer key, binds the listener, and publishes the
port and key files. It also runs the indexers, and here it serves the UI as one of its supervised
global modules.

Running the UI as PID 1 instead would leave the MCP endpoint — the thing agents connect to — with no
owner, which is backwards for a server whose main job is being connected to.

Two consequences worth knowing:

- **`exec` is load-bearing.** The entrypoint `exec`s the daemon so it *is* process 1. Docker's
  `SIGTERM` then reaches the daemon's signal handler, which shuts the indexers and both servers down
  cleanly, instead of killing a shell and orphaning them.
- **The daemon serving the UI is a config key, not a container hack.** `modules.daemon_ui` is opt-in
  and `true` in this image. On a workstation `graphit ui` remains how you get a UI, which is why it is
  off by default: a background process silently holding port 8080 is not what anyone asks the daemon
  for.

## The two ports

| Port | Serves | Configurable |
|---|---|---|
| `8080` | The UI, and `/health` | **No.** The UI takes the first free port from 8080 upward and there is no setting for it. Remap on the host with `-p` |
| `8081` | The MCP endpoint at `/mcp`, bearer-authenticated | **Yes**, `mcp.port`. It has to be, because a published port must be known before the process starts |

Outside a container the MCP port defaults to `0` — kernel-assigned — and the host to `127.0.0.1`,
exactly what the daemon did before these keys existed. The image sets `GRAPHIT_MCP_HOST=0.0.0.0` and
`GRAPHIT_MCP_PORT=8081`, because a port nobody can reach is not a published port.

There is no health endpoint on the MCP port; it registers `/mcp` and nothing else. The healthcheck
therefore polls the UI, which the daemon only serves once past its own startup.

## What this image cannot do

Everything needing a coding-agent CLI **on the server** is off, through `modules.agent=false`. Those
features reach `ai.NewClientFromConfig`, which only ever returns a CLI found on `PATH` — there is no
HTTP fallback, so without a binary they cannot degrade, only fail.

**Disabled, and not offered in the UI:**

| Feature | Route |
|---|---|
| Natural-language Cypher in the AST explorer | `POST /api/generate-cypher` |
| AI search in the knowledge explorer | `POST /api/wiki/ai-search` |
| AI search in the memory explorer | the same route — one component, one endpoint |
| Live search | `/api/live/*`, not even registered |

The UI reads the same flag and does not render those controls: a button that appears and then fails
teaches nothing, and one that vanishes when a capability check returns is worse.

**This is not a limitation on what agents can do through it.** Those four features are the server
doing its own reasoning. An agent connecting over MCP brings its own model and uses the full tool
surface — which is the whole design. The server holds the knowledge; the agent thinks.

**Still working:** hybrid search over local ONNX embeddings (`GET /api/search`), wiki keyword search,
every Cypher/graph/complexity/dead-code route, the knowledge and memory explorers, the Hub, the
ecosystem view, and **the entire MCP tool surface**.

`modules.dream=false` for the same reason: Dream is an overnight agent run.

### If you want the server to reason too

Do not add a CLI here and flip the flag — install the framework on a machine with an authenticated
agent CLI and run `graphit ui` there. If you want to try anyway, it is
`-e GRAPHIT_MODULES_AGENT=true` plus a CLI on `PATH` inside the container. Untested.

## The volume

| Path | Holds | What losing it costs |
|---|---|---|
| `/opt/graphit` | Everything: installed knowledge and AST artifacts, compiled wikis, memory stores, the Hub cache, the embedding model, the daemon's pid/port/key files, and `global.lock.json` | The server comes back empty. Every artifact has to be installed again |

There is no projects volume, because artifacts are keyed by identifier and version rather than by a
path. Declared in the image, so a container without `-v` still has writable storage — as an anonymous
volume. Name it for anything you intend to keep.

## Pinning the version

`GRAPHIT_VERSION` defaults to `latest`, resolving the newest release when the image is built.
Convenient, and not reproducible: two builds a week apart can install different versions from the same
Dockerfile.

```bash
docker build -t graphit-code --build-arg GRAPHIT_VERSION=v0.1.26 .
```

Either way the archive's SHA-256 is checked against the release's `checksums.sha256`. Pinning chooses
which artifact, not whether it is verified.

## Configuration

Every configuration key can be set with an environment variable — `GRAPHIT_` plus the key upper-cased
with dots turned into underscores — and that layer **outranks both the project lockfile and the global
config file**. So a container is configured with `-e`, and nothing has to be written inside it. The
derivation is one function, `config.ConfigEnvVar`, the same one the resolver uses, so there is no
separate list of supported variables.

### What the image sets, and why

| Variable | Key | Value | Reason |
|---|---|---|---|
| `GRAPHIT_MODULES_AGENT` | `modules.agent` | `false` | No coding-agent CLI in the image |
| `GRAPHIT_MODULES_DREAM` | `modules.dream` | `false` | Dream is an agent run |
| `GRAPHIT_MODULES_DAEMON_UI` | `modules.daemon_ui` | `true` | The daemon serves the UI |
| `GRAPHIT_UI_HOST` | `ui.host` | `0.0.0.0` | A container answering only its own loopback is unreachable |
| `GRAPHIT_MCP_HOST` | `mcp.host` | `0.0.0.0` | Same, for the MCP endpoint |
| `GRAPHIT_MCP_PORT` | `mcp.port` | `8081` | A published port must be known in advance |
| `GRAPHIT_GLOBAL_DIR` | — | `/opt/graphit` | Read from the environment only; not a config key |

### Hub / S3

| Variable | Key | Notes |
|---|---|---|
| `GRAPHIT_HUB_BUCKET` | `hub.bucket` | Empty is local-only mode — which for a server means nothing to install and nothing to serve |
| `GRAPHIT_HUB_REGION` | `hub.region` | |
| `GRAPHIT_HUB_ENDPOINT` | `hub.endpoint` | For MinIO and other S3-compatible servers |
| `GRAPHIT_HUB_PREFIX` | `hub.prefix` | |
| `GRAPHIT_HUB_ACCESS_KEY_ID` | `hub.access_key_id` | An identifier, not a secret — the framework prints it |

Supply no credentials and the AWS default credential chain resolves them instead:
`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN`, a mounted `~/.aws/credentials`,
or an instance role.

### Secrets

Every secret the configuration can hold is also readable from an environment variable — the only
channel that leaves no copy in an image layer or a file on disk.

| Variable | Key |
|---|---|
| `GRAPHIT_HUB_SECRET_ACCESS_KEY` | `hub.secret_access_key` |
| `GRAPHIT_AI_EMBEDDING_API_KEY` | `ai.embedding.api_key` |
| `GRAPHIT_AI_RERANK_API_KEY` | `ai.rerank.api_key` |

Pass them with `--env-file` or your orchestrator's secret store. **Never with `--build-arg`**: a build
argument is recorded in the image and readable with `docker history`.

`graphit config get` and `graphit config --list` redact all three.

### Embedding provider, and image size

The build runs `graphit setup`, and with the default `local` provider that downloads the ~132 MiB
model into the image. The cost is size; the benefit is that the first search in a fresh container does
not wait for it. Semantic and hybrid search over installed artifacts is a large part of what agents
ask this server for, so `local` is the sensible default here.

To use a remote provider and skip the download:

```bash
docker build -t graphit-code \
  --build-arg EMBEDDING_PROVIDER=openai-compatible \
  --build-arg EMBEDDING_MODEL=nomic-embed-text \
  --build-arg EMBEDDING_BASE_URL=http://ollama:11434/v1 .

docker run … -e GRAPHIT_AI_EMBEDDING_API_KEY=… graphit-code
```

The API key is deliberately not a build argument.

### The base image

`BASE_IMAGE` defaults to `debian:bookworm-slim`. It was the official Node image while this image
installed a coding-agent CLI, because `gemini` is npm-only and the MCP servers an agent extends itself
with are npm packages launched with `npx`. Neither reason survives an image with no agent in it, and
dropping Node returns roughly 150 MB. Override it if you intend to exec an npm-based tool inside:

```bash
docker build --build-arg BASE_IMAGE=node:22-bookworm-slim .
```

## Exposing it to other people

A server that only answers its own loopback is not a server, so this is the section to read before
anyone else's agent connects.

**Neither port is authenticated, and the UI hands out the MCP bearer key.** Anything that can reach
port 8080 can read every artifact in the global store, act on it, *and* copy the credential for the
MCP endpoint. Both servers bind `0.0.0.0` *inside* the container because a container answering only its
own loopback would be unusable — that is not a judgment that the ports are safe to publish.

For anything beyond the local machine:

1. **Put an authenticating reverse proxy in front of the UI.** Keep `-p 127.0.0.1:8080:8080` and let
   the proxy hold the only route in.
2. **Treat the MCP port as the credentialled one.** It checks a bearer key, which the UI port does
   not, so it is the port that can reasonably be reached by agents on a private network. Terminate TLS
   in front of it — the key travels in a header, and plain HTTP puts it on the wire in clear.
3. **Tell the browser's origin to the UI**, or its requests are rejected by the CORS policy:

   ```bash
   -e GRAPHIT_UI_ALLOWED_ORIGINS=https://graphit.internal.example
   ```

   Left empty, the secure default stands: same-origin requests and localhost loopback origins.
   Configured origins **replace** that allowlist rather than adding to it.
4. **Rotate by restarting.** The bearer key is regenerated on every daemon start, so `docker restart`
   invalidates every distributed key. That is also why a key copied before a restart stops working.

## docker compose

```yaml
services:
  graphit:
    build:
      context: .
      args:
        GRAPHIT_VERSION: latest
    ports:
      - "127.0.0.1:8080:8080"   # UI
      - "127.0.0.1:8081:8081"   # MCP — agents connect here
    volumes:
      - graphit-global:/opt/graphit
    environment:
      GRAPHIT_HUB_BUCKET: ${GRAPHIT_HUB_BUCKET:-}
      GRAPHIT_HUB_REGION: ${GRAPHIT_HUB_REGION:-}
    env_file:
      - path: ./graphit.secrets.env
        required: false
    stop_grace_period: 30s
    restart: unless-stopped

volumes:
  graphit-global:
```

`stop_grace_period` is raised because the daemon shuts two servers and the indexers down on `SIGTERM`;
the 10-second default can cut a graph write short.

Keep secrets in `graphit.secrets.env`, out of version control, rather than in the `environment` block
where `docker compose config` prints them.

## Why the image does not build from source

Building the framework needs a Node toolchain for the UI, a Go toolchain with cgo, ONNX Runtime,
`liblbug`, and a Rust toolchain that compiles the LanceDB native from source — which cannot be
cross-compiled, which is why the release pipeline runs one job per platform. That is tens of minutes
per image to reproduce an artifact the release already contains.

The published Linux artifact is the launcher, carrying `graphit-core`, `graphit-mcp`, `liblbug`,
`libonnxruntime`, `liblancedb_go`, the LadybugDB httpfs extension and the AST query definitions as an
embedded payload. One downloaded file is the whole runtime.

To run the working tree instead of a release, build it on the host with `make build-local`.

## Troubleshooting

**The build fails saying it needs a release whose `setup` takes a flag per question.** The image runs
`graphit setup` by answering every question with a flag, and the release `GRAPHIT_VERSION=latest`
resolved is older than those flags. Pin a newer tag with `--build-arg GRAPHIT_VERSION=<tag>`.

**An agent's query comes back empty, or says it needs an artifact reference.** There is no checkout on
the server, so `project_dir` resolves to nothing. Name the artifact in `context` — see
[Ask for artifacts, not paths](#3-ask-for-artifacts-not-paths).

**`graphit hub list` shows nothing to install.** The Hub is in local-only mode. Set
`GRAPHIT_HUB_BUCKET` and its credentials.

**An MCP client gets 401.** The bearer key is regenerated on every daemon start, so a key copied
before a restart is stale. Copy it again from the UI's daemon page or from
`/opt/graphit/runtime/daemon/mcp.key`.

**An MCP client cannot connect at all.** Check that 8081 is published and that
`GRAPHIT_MCP_HOST=0.0.0.0` is still set — with the compiled default of `127.0.0.1` the listener is
inside the container only. `docker exec graphit cat /opt/graphit/runtime/daemon/mcp.port` shows the
port actually in use.

**The AI search or live search controls are missing from the UI.** By design — see
[What this image cannot do](#what-this-image-cannot-do). Your agent is unaffected.

**The container is marked unhealthy while the logs look fine.** The healthcheck polls the UI, so it
fails if `modules.daemon_ui` was overridden to `false` — the daemon is then healthy and simply serving
no UI.

**The browser console reports a CORS failure.** The UI is reached through an origin not in the
allowlist. Set `GRAPHIT_UI_ALLOWED_ORIGINS` to the exact origin, scheme and port included.

**The build fails downloading the embedding model.** That step is fatal on purpose: an installation
without the model would let search answer on keywords alone and never say the semantic half did not
run. Behind a proxy, pass `HTTP_PROXY` / `HTTPS_PROXY` as build arguments, or select a remote
embedding provider.

## See also

- [MCP tools reference](mcp_tools_reference.md) — the tool contracts an agent gets, and which take a `context`
- [S3 and UI network security](s3-and-ui-network.md) — the bind address and origin policy in full
- [CLI reference](cli_reference.md) — `graphit setup`, `graphit daemon`, `graphit hub`
- [Configuration](../specs/config_module.md) — `modules.agent`, `modules.daemon_ui`, `mcp.host`, `mcp.port`
- [Getting started](getting_started.md) — a local install, for the machine the agent runs on
