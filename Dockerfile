# syntax=docker/dockerfile:1.7
#
# Graphit Code container image: a daemon serving the MCP endpoint and the UI.
#
# PID 1 is the daemon, because the daemon is what brings the MCP server up. It also serves the
# unified UI, which is what `modules.daemon_ui` exists for. Two ports are published: the UI and
# the MCP endpoint.
#
# There is NO coding-agent CLI in this image, deliberately. Everything that needs one is turned
# off through `modules.agent`, and the UI does not offer those features rather than offering them
# and failing — see "What this image cannot do" below.
#
# The framework is NOT compiled here. install.sh downloads the published GitHub release, verifies
# its SHA-256, and installs the launcher — one file that carries graphit-core, graphit-mcp,
# liblbug, libonnxruntime, liblancedb_go, the LadybugDB httpfs extension and the AST query
# definitions as an embedded payload.
#
# This is a SERVER. It carries no source checkouts and needs none: any MCP-capable AI agent —
# anywhere, on any machine — connects to the MCP endpoint with the bearer key and reads the
# knowledge and code-graph artifacts installed into the global store, addressed by context name
# rather than by a path. The UI is the same content for humans.
#
#   docker build -t graphit-code .
#   docker run -d --name graphit \
#     -p 127.0.0.1:8080:8080 -p 127.0.0.1:8081:8081 \
#     -v graphit-global:/opt/graphit \
#     graphit-code
#
# Requires BuildKit (the default since Docker 23) for the heredocs below.
#
# ⚠️  THE UI HAS NO AUTHENTICATION, and this image serves the MCP bearer key over it so the UI can
#     offer a copy button. Anything that can reach the UI port can read every artifact in the
#     global store AND take over the MCP endpoint. Publish both ports to the host loopback, as
#     above, or put an authenticating proxy in front. Never expose them to a network you do not
#     control.

# BASE_IMAGE is a build argument, and the default changed on purpose.
#
# It was node:<major>-bookworm-slim while the image installed a coding-agent CLI: `gemini` is
# published on npm alone, and the MCP servers an agent extends itself with are npm packages
# launched with npx. NEITHER REASON SURVIVES an image with no agent in it — nothing here runs
# node — so the default is the smaller base and roughly 150 MB comes back. Override it if you
# intend to exec an npm-based tool inside the container:
#
#   docker build --build-arg BASE_IMAGE=node:22-bookworm-slim .
ARG BASE_IMAGE=debian:bookworm-slim
FROM ${BASE_IMAGE}

# ─────────────────────────────────────────────────────────────────────────────
# Build arguments
# ─────────────────────────────────────────────────────────────────────────────

# GRAPHIT_VERSION selects the published release. `latest` — the default — resolves the newest tag
# at build time; any other value is used verbatim as the release tag. The default is convenient
# and NOT reproducible, so pin it for anything you intend to rebuild identically. Either way the
# archive's SHA-256 is verified against the release's checksums file.
ARG GRAPHIT_VERSION=latest

# The ports. UI_PORT is only advertised, not chosen: the UI takes the first free port from 8080
# upward and there is no setting for it, so in a clean container it is deterministically 8080.
# MCP_PORT is genuinely configuration — `mcp.port` pins it, because a published port has to be
# known before the process starts.
ARG UI_PORT=8080
ARG MCP_PORT=8081

# EMBEDDING_PROVIDER / RERANK_PROVIDER are answered to `graphit setup` below. `local` downloads
# the ~132 MiB model into the image so a fresh container's first search does not wait for it. A
# remote provider skips the download; its API key belongs at run time, never in a layer, which is
# why there is no ARG for it.
ARG EMBEDDING_PROVIDER=local
ARG EMBEDDING_MODEL=
ARG EMBEDDING_BASE_URL=
ARG RERANK_PROVIDER=local
ARG RERANK_MODEL=

ARG APP_USER=graphit
ARG APP_UID=10001

# ─────────────────────────────────────────────────────────────────────────────
# Runtime environment
# ─────────────────────────────────────────────────────────────────────────────

# GRAPHIT_GLOBAL_DIR is read straight from the environment by internal/brand — it is not a config
# key and `graphit config` cannot set it. Everything machine-wide lives under it: the extracted
# launcher payload, the AST graphs, the compiled wikis, the memory stores, the Hub cache, the
# embedding model, the daemon's pid/port/key files, and global.lock.json.
ENV GRAPHIT_GLOBAL_DIR=/opt/graphit \
    HOME=/home/${APP_USER}

# The two ports are promoted from build arguments to environment variables because the HEALTHCHECK
# command is evaluated by a shell INSIDE THE RUNNING CONTAINER, where a build argument no longer
# exists: `${UI_PORT}` there would expand to the empty string and the check would poll
# `http://127.0.0.1:/health` on every interval, marking a healthy container unhealthy.
ENV UI_PORT=${UI_PORT} \
    MCP_PORT=${MCP_PORT}

# ── What this image cannot do, and how that is enforced ──────────────────────
#
# modules.agent=false turns off every feature that needs a coding-agent CLI. Each of them reaches
# ai.NewClientFromConfig, which only ever returns a CLI found on PATH — there is no HTTP fallback
# behind it, so without a binary these cannot degrade, only fail:
#
#   • natural-language Cypher in the AST explorer      (POST /api/generate-cypher)
#   • AI search in the knowledge AND memory explorers  (POST /api/wiki/ai-search)
#   • live search                                      (/api/live/*, not even registered)
#
# The UI reads the same flag and does not render those controls. What KEEPS WORKING is everything
# built on local ONNX embeddings or on the graph alone: hybrid search (GET /api/search), wiki BM25
# search, every Cypher/graph/complexity/dead-code route, the Hub registry, install and publish,
# the wiki and memory explorers, and THE WHOLE MCP TOOL SURFACE.
#
# That last one is the point of the image: the reasoning happens in the agent that connects, which
# brings its own model. This server supplies the graphs, the wikis and the memory it reasons over.
#
# modules.dream=false for the same reason — Dream is an overnight agent run. It is opt-in anyway,
# so this is explicitness, not a change of behaviour.
#
# modules.daemon_ui=true is what makes the daemon serve the UI. It is opt-in because on a
# workstation `graphit ui` is how you get one; here it must come up with PID 1.
ENV GRAPHIT_MODULES_AGENT=false \
    GRAPHIT_MODULES_DREAM=false \
    GRAPHIT_MODULES_DAEMON_UI=true

# Both servers bind every interface INSIDE the container, because a container that answers only
# its own loopback is unreachable. The compiled defaults are both 127.0.0.1 precisely so that this
# is a decision someone takes, and taking it is what the warning at the top of this file is about.
ENV GRAPHIT_UI_HOST=0.0.0.0 \
    GRAPHIT_UI_ALLOWED_ORIGINS= \
    GRAPHIT_MCP_HOST=0.0.0.0 \
    GRAPHIT_MCP_PORT=${MCP_PORT}

# Hub / S3. Every key resolves through the standard chain, in which the environment outranks both
# the project lockfile and the global config file. Declared empty for discoverability with
# `docker inspect`; the resolver skips an empty value, so declaring them changes nothing.
# An empty GRAPHIT_HUB_BUCKET is local-only mode: no registry, no publishing, everything else works.
ENV GRAPHIT_HUB_BUCKET= \
    GRAPHIT_HUB_REGION= \
    GRAPHIT_HUB_ENDPOINT= \
    GRAPHIT_HUB_PREFIX= \
    GRAPHIT_HUB_ACCESS_KEY_ID=

# 🔐 Every secret the configuration can hold is also readable from an environment variable — the
# only channel that leaves no copy in an image layer or a file on disk. The names are derived by
# config.ConfigEnvVar, the same function ResolveConfig uses, so there is no separate scheme:
#
#   GRAPHIT_HUB_SECRET_ACCESS_KEY    hub.secret_access_key
#   GRAPHIT_AI_EMBEDDING_API_KEY     ai.embedding.api_key
#   GRAPHIT_AI_RERANK_API_KEY        ai.rerank.api_key
#
# Declared EMPTY, and only empty: a value passed through ARG or ENV is recorded in the image and
# readable with `docker history`. Pass them at run time, ideally with --env-file or your
# orchestrator's secret store. GRAPHIT_HUB_ACCESS_KEY_ID sits above with the non-secrets because
# an access key ID is an identifier, not a credential.
#
# With no pair supplied the AWS default credential chain resolves S3 instead: AWS_ACCESS_KEY_ID /
# AWS_SECRET_ACCESS_KEY / AWS_SESSION_TOKEN, a mounted ~/.aws/credentials, or an instance role.
ENV GRAPHIT_HUB_SECRET_ACCESS_KEY= \
    GRAPHIT_AI_EMBEDDING_API_KEY= \
    GRAPHIT_AI_RERANK_API_KEY=

# ─────────────────────────────────────────────────────────────────────────────
# Base system
# ─────────────────────────────────────────────────────────────────────────────
#
# git is not optional: `graphit setup` looks for it in PATH and refuses to start without it.
# ca-certificates, curl and tar are what install.sh needs. ripgrep and less are what anyone who
# execs into the container to inspect a project will expect.
RUN <<'EOF' sh -eu
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates \
  curl \
  tar \
  unzip \
  git \
  less \
  ripgrep \
  tzdata
rm -rf /var/lib/apt/lists/*
EOF

RUN useradd --uid "${APP_UID}" --create-home --shell /bin/bash "${APP_USER}"

# ─────────────────────────────────────────────────────────────────────────────
# Graphit Code, from the GitHub release
# ─────────────────────────────────────────────────────────────────────────────
COPY install.sh /tmp/install.sh
RUN <<'EOF' sh -eu
# install.sh resolves the newest tag itself when given no --version, so `latest` is expressed by
# omitting the flag rather than by sending the string "latest" — no release is tagged that.
case "${GRAPHIT_VERSION:-latest}" in
  latest|"") sh /tmp/install.sh --dir /usr/local/bin ;;
  *)         sh /tmp/install.sh --dir /usr/local/bin --version "${GRAPHIT_VERSION}" ;;
esac
rm -f /tmp/install.sh
mkdir -p "${GRAPHIT_GLOBAL_DIR}"
chown -R "${APP_USER}:${APP_USER}" "${GRAPHIT_GLOBAL_DIR}"
EOF

# ─────────────────────────────────────────────────────────────────────────────
# graphit setup, with nothing left to ask
# ─────────────────────────────────────────────────────────────────────────────
#
# There is no non-interactive mode to switch on: `graphit setup` skips any question whose flag was
# supplied, so answering everything it reaches IS the absence of interaction. That means this
# invocation must be COMPLETE — a question left without its flag would be asked, read EOF from the
# build's empty stdin, and take the default in silence.
#
# What the run reaches, and why that is the whole list:
#   --hub-bucket ""       local-only, and an empty hub.bucket is what makes setup skip the region,
#                         endpoint and both credential questions entirely
#   --ide / --cli         always asked; empty keeps the framework's own default (opencode), which
#                         only names what an EXTERNAL agent would be — nothing here runs one
#   --embedding-provider  always asked; `local` ends that branch, anything else adds three more
#   --rerank-provider     the same shape
#
# The api-key flags are passed EXPLICITLY EMPTY rather than omitted: omitting asks, while an empty
# value is an answer meaning "store no key here", after which the provider reads its own
# environment variable at run time.
#
# No bucket is configured here on purpose — an image carrying someone's bucket name is not a
# reusable image. Configure the Hub with -e at run time.
#
# Nothing is softened by answering in advance: an unreachable hub bucket, or a failed
# embedding-model download, fails the BUILD rather than producing an image whose search silently
# answers on keywords alone.
USER ${APP_USER}
RUN <<'EOF' sh -eu
# The flags are probed for before they are used. GRAPHIT_VERSION defaults to `latest`, so which
# binary landed here is a fact about the day of the build, and a release predating these flags
# would otherwise fail with the words "unknown flag" against a version nobody chose.
#
# Answers are NOT piped into the prompts as a fallback, deliberately: piped answers are
# positional, so one prompt added upstream shifts every later answer onto the wrong key and setup
# reports success having stored the region as an endpoint. Refusing is the correct outcome.
if ! graphit setup --help 2>&1 | grep -q -- '--hub-bucket'; then
  echo "" >&2
  echo "This image needs a Graphit Code release whose 'setup' takes a flag per question." >&2
  echo "The installed binary does not: $(graphit --version 2>/dev/null || echo 'version unknown')" >&2
  echo "" >&2
  echo "Pin a release that has them:  docker build --build-arg GRAPHIT_VERSION=<tag> ." >&2
  exit 1
fi

graphit setup \
  --hub-bucket "" \
  --ide "" \
  --cli "" \
  --embedding-provider "${EMBEDDING_PROVIDER}" \
  --embedding-model "${EMBEDDING_MODEL}" \
  --embedding-base-url "${EMBEDDING_BASE_URL}" \
  --embedding-api-key "" \
  --rerank-provider "${RERANK_PROVIDER}" \
  --rerank-model "${RERANK_MODEL}" \
  --rerank-api-key "" \
  < /dev/null

# `setup` autostarts a daemon at its last step. Stopping it leaves the image without a pidfile
# pointing at a process that no longer exists — the one PID 1 claims must be its own.
graphit daemon stop || true
EOF

# ─────────────────────────────────────────────────────────────────────────────
# Ports
# ─────────────────────────────────────────────────────────────────────────────
#
# 8080 — the UI. Not configurable: NewUnifiedServer asks netutil for the first free port from
#        8080 upward. Remap on the host with -p when you need a different one.
# 8081 — the MCP endpoint, pinned by GRAPHIT_MCP_PORT above. Its bearer key is regenerated on
#        every daemon start and published to <global>/runtime/daemon/mcp.key; the UI's daemon page
#        shows it with a copy button.
EXPOSE ${UI_PORT} ${MCP_PORT}

# The UI answers /health as soon as its mux is serving. There is no health endpoint on the MCP
# port — it registers /mcp and nothing else — so this checks the UI, which the daemon only serves
# once it is past its own startup.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${UI_PORT}/health" >/dev/null || exit 1

# ─────────────────────────────────────────────────────────────────────────────
# Volumes
# ─────────────────────────────────────────────────────────────────────────────
#
# ONE volume, and it holds everything that matters: the installed knowledge and AST artifacts that
# agents query, the compiled wikis, the memory stores, the Hub cache, the embedding model, and the
# daemon's pid/port/key files.
#
# There is no projects volume. This server answers about ARTIFACTS, which are keyed by identifier
# and version rather than by a path on disk, so it needs no source checkout to serve them — see
# internal/mcpstdio/context.go, where an absent project_dir means exactly this global scope.
#
# Declared so a container without -v still has writable storage, as an anonymous volume. Name it
# for anything you intend to keep.
#
# Mounting a checkout anyway is supported and occasionally useful — it lets `graphit init` and
# `graphit sync` build a project's own indexes here, after which tools can address it by
# project_dir. Pick a path and keep it: global.lock.json records projects by absolute container
# path, so mounting the same checkout somewhere else registers it twice.
VOLUME ["/opt/graphit"]

# ─────────────────────────────────────────────────────────────────────────────
# Entrypoint: the daemon, as PID 1
# ─────────────────────────────────────────────────────────────────────────────
#
# The daemon is PID 1 because it is the process that owns the MCP server, and it serves the UI as
# one of its supervised global modules. Running the UI as PID 1 instead would leave the MCP
# endpoint — the thing external agents connect to — with no owner.
#
# `exec` matters: the daemon must BE process 1, not its child, so that Docker's SIGTERM reaches
# the signal handler that shuts the indexers and both servers down cleanly rather than killing a
# shell and orphaning them.
#
# Any arguments are passed through, so the same image serves the rest of the CLI and a shell:
#   docker run --rm -v graphit-global:/opt/graphit graphit-code graphit hub list
#   docker run --rm -it -v graphit-global:/opt/graphit graphit-code bash
COPY <<'SCRIPT' /usr/local/bin/graphit-entrypoint
#!/bin/sh
set -eu

if [ "$#" -eq 0 ]; then
  exec graphit daemon
fi

case "$1" in
  -*) exec graphit daemon "$@" ;;
esac

exec "$@"
SCRIPT

USER root
RUN chmod 0755 /usr/local/bin/graphit-entrypoint
USER ${APP_USER}

# The global store, because there is no project directory to sit in. The daemon chdirs here on its
# own anyway (see chdirToStableDir); this only decides where `docker exec … bash` lands.
WORKDIR /opt/graphit
ENTRYPOINT ["/usr/local/bin/graphit-entrypoint"]
