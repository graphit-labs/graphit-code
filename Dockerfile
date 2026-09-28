# syntax=docker/dockerfile:1.7

ARG BASE_IMAGE=debian:bookworm-slim
FROM ${BASE_IMAGE}

ENV HOME=/home/graphit \
    GRAPHIT_GLOBAL_DIR=/home/graphit/.graphit \
    GRAPHIT_ENTRYPOINT_HOOKS_DIR=/docker-entrypoint.d

ENV GRAPHIT_MODULES_AGENT=false \
    GRAPHIT_MODULES_DREAM=false \
    GRAPHIT_MODULES_DAEMON_UI=true \
    GRAPHIT_UI_AUTH_ENABLED=true

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
mkdir -p "${GRAPHIT_ENTRYPOINT_HOOKS_DIR}"
chown graphit:graphit "${GRAPHIT_GLOBAL_DIR}"
EOF

EXPOSE 8080 8081

HEALTHCHECK --interval=30s --timeout=5s --start-period=5m --retries=3 \
    CMD curl -fsS "http://127.0.0.1:8081/health" >/dev/null && \
        case "${GRAPHIT_MODULES_DAEMON_UI}" in \
          [Tt][Rr][Uu][Ee]) curl -fsS "http://127.0.0.1:8080/health" >/dev/null ;; \
        esac

VOLUME ["/home/graphit/.graphit"]

COPY --chmod=0755 scripts/graphit-entrypoint.sh /usr/local/bin/graphit-entrypoint

WORKDIR /home/graphit/.graphit
USER graphit
ENTRYPOINT ["/usr/local/bin/graphit-entrypoint"]
