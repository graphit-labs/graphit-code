# syntax=docker/dockerfile:1.7

ARG BASE_IMAGE=debian:bookworm-slim
FROM ${BASE_IMAGE}


ARG GRAPHIT_VERSION=latest

ARG UI_PORT=8080
ARG MCP_PORT=8081

ARG EMBEDDING_PROVIDER=local
ARG EMBEDDING_MODEL=
ARG EMBEDDING_BASE_URL=
ARG RERANK_PROVIDER=local
ARG RERANK_MODEL=

ARG APP_USER=graphit
ARG APP_UID=10001


ENV GRAPHIT_GLOBAL_DIR=/opt/graphit \
    HOME=/home/${APP_USER}

ENV UI_PORT=${UI_PORT} \
    MCP_PORT=${MCP_PORT}

ENV GRAPHIT_MODULES_AGENT=false \
    GRAPHIT_MODULES_DREAM=false \
    GRAPHIT_MODULES_DAEMON_UI=true

ENV GRAPHIT_UI_HOST=0.0.0.0 \
    GRAPHIT_UI_ALLOWED_ORIGINS= \
    GRAPHIT_MCP_HOST=0.0.0.0 \
    GRAPHIT_MCP_PORT=${MCP_PORT}

ENV GRAPHIT_HUB_BUCKET= \
    GRAPHIT_HUB_REGION= \
    GRAPHIT_HUB_ENDPOINT= \
    GRAPHIT_HUB_PREFIX= \
    GRAPHIT_HUB_ACCESS_KEY_ID=

ENV GRAPHIT_HUB_SECRET_ACCESS_KEY= \
    GRAPHIT_AI_EMBEDDING_API_KEY= \
    GRAPHIT_AI_RERANK_API_KEY=

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

COPY install.sh /tmp/install.sh
RUN <<'EOF' sh -eu
case "${GRAPHIT_VERSION:-latest}" in
  latest|"") sh /tmp/install.sh --dir /usr/local/bin ;;
  *)         sh /tmp/install.sh --dir /usr/local/bin --version "${GRAPHIT_VERSION}" ;;
esac
rm -f /tmp/install.sh
mkdir -p "${GRAPHIT_GLOBAL_DIR}"
chown -R "${APP_USER}:${APP_USER}" "${GRAPHIT_GLOBAL_DIR}"
EOF

USER ${APP_USER}
RUN <<'EOF' sh -eu
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

graphit daemon stop || true
EOF

EXPOSE ${UI_PORT} ${MCP_PORT}

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${UI_PORT}/health" >/dev/null || exit 1

VOLUME ["/opt/graphit"]

COPY <<'SCRIPT' /usr/local/bin/graphit-entrypoint
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

WORKDIR /opt/graphit
ENTRYPOINT ["/usr/local/bin/graphit-entrypoint"]
