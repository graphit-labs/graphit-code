# syntax=docker/dockerfile:1.7

ARG BASE_IMAGE=debian:bookworm-slim
FROM ${BASE_IMAGE}

ENV HOME=/home/graphit \
    GRAPHIT_GLOBAL_DIR=/home/graphit/.graphit

ENV GRAPHIT_MODULES_AGENT=false \
    GRAPHIT_MODULES_DREAM=false \
    GRAPHIT_MODULES_DAEMON_UI=true

ENV GRAPHIT_UI_HOST=0.0.0.0 \
    GRAPHIT_UI_ALLOWED_ORIGINS= \
    GRAPHIT_MCP_HOST=0.0.0.0 \
    GRAPHIT_HUB_EVENTS_ANONYMIZE=false \
    GRAPHIT_AGENT= \
    GRAPHIT_CLI=

ENV GRAPHIT_CLIENT_SECRET=

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

COPY --chmod=0755 .build/graphit-linux-amd64 /usr/local/bin/graphit

RUN <<'EOF' sh -eu
groupadd --gid 10001 graphit
useradd --uid 10001 --gid graphit --create-home --shell /bin/bash graphit
mkdir -p "${GRAPHIT_GLOBAL_DIR}"
chown graphit:graphit "${GRAPHIT_GLOBAL_DIR}"
EOF

EXPOSE 8080 8081

HEALTHCHECK --interval=30s --timeout=5s --start-period=5m --retries=3 \
    CMD curl -fsS "http://127.0.0.1:8081/health" >/dev/null && \
        case "${GRAPHIT_MODULES_DAEMON_UI}" in \
          [Tt][Rr][Uu][Ee]) curl -fsS "http://127.0.0.1:8080/health" >/dev/null ;; \
        esac

VOLUME ["/home/graphit/.graphit"]

COPY <<'SCRIPT' /usr/local/bin/graphit-entrypoint
#!/bin/sh
set -eu

# Container listener ports are part of the image contract. Publish a different
# host port with Docker's HOST:CONTAINER mapping instead of moving the listeners.
GRAPHIT_UI_PORT=8080
GRAPHIT_MCP_PORT=8081
export GRAPHIT_UI_PORT GRAPHIT_MCP_PORT

case "${GRAPHIT_GLOBAL_DIR}" in
  /*) ;;
  *)
    echo "GRAPHIT_GLOBAL_DIR must be an absolute path." >&2
    exit 1
    ;;
esac
if [ "${GRAPHIT_GLOBAL_DIR}" = "/" ]; then
  echo "GRAPHIT_GLOBAL_DIR must not be the filesystem root." >&2
  exit 1
fi

if ! mkdir -p "${GRAPHIT_GLOBAL_DIR}"; then
  echo "Cannot create GRAPHIT_GLOBAL_DIR as user graphit (UID/GID 10001): ${GRAPHIT_GLOBAL_DIR}" >&2
  exit 1
fi
if [ ! -r "${GRAPHIT_GLOBAL_DIR}" ] || [ ! -w "${GRAPHIT_GLOBAL_DIR}" ] || [ ! -x "${GRAPHIT_GLOBAL_DIR}" ]; then
  echo "GRAPHIT_GLOBAL_DIR must be readable, writable, and traversable by user graphit (UID/GID 10001): ${GRAPHIT_GLOBAL_DIR}" >&2
  exit 1
fi

if [ ! -f "${GRAPHIT_GLOBAL_DIR}/config.json" ]; then
  env GRAPHIT_MODULES_DAEMON=false graphit setup \
    --non-interactive \
    "--anonymize-events=${GRAPHIT_HUB_EVENTS_ANONYMIZE}" \
    "--agent=${GRAPHIT_AGENT}" \
    "--cli=${GRAPHIT_CLI}" \
    < /dev/null
fi

if [ "$#" -eq 0 ]; then
  exec graphit daemon
fi

case "$1" in
  -*) exec graphit daemon "$@" ;;
esac

exec "$@"
SCRIPT

RUN chmod 0755 /usr/local/bin/graphit-entrypoint

WORKDIR /home/graphit/.graphit
USER graphit
ENTRYPOINT ["/usr/local/bin/graphit-entrypoint"]
