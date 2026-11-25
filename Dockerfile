# syntax=docker/dockerfile:1.7

# Keep the maintained Linux base pinned; validate runtime compatibility
# together with the packaged Rust executables and native assets.
ARG BASE_IMAGE=debian:unstable-slim@sha256:8c99fc3b4617c61eaa16d4436748f779e9a6354b004117814453a23a0dc35279
FROM ${BASE_IMAGE}

ENV HOME=/home/graphit \
    GRAPHIT_GLOBAL_DIR=/home/graphit/.graphit \
    GRAPHIT_ENTRYPOINT_HOOKS_DIR=/docker-entrypoint.d

ENV GRAPHIT_AGENT=codex \
    GRAPHIT_MCP__HOST=0.0.0.0 \
    GRAPHIT_MCP__PORT=8081 \
    GRAPHIT_MODULES__AGENT=false \
    GRAPHIT_MODULES__DREAM=false \
    GRAPHIT_MODULES__DAEMON_UI=true

RUN <<'EOF' sh -eu
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates \
  libstdc++6 \
  libgcc-s1 \
  libssl3t64 \
  libzstd1 \
  zlib1g \
  curl \
  tar \
  unzip \
  git \
  less \
  ripgrep \
  tzdata
rm -rf /var/lib/apt/lists/*
EOF

# Copy the complete release, including its optional sibling CUDA module.
COPY .build/release/graphit-linux-amd64/ /usr/local/bin/

RUN <<'EOF' sh -eu
groupadd --gid 10001 graphit
useradd --uid 10001 --gid graphit --create-home --shell /bin/bash graphit
install -d -m 0700 -o graphit -g graphit "${GRAPHIT_GLOBAL_DIR}"
mkdir -p "${GRAPHIT_ENTRYPOINT_HOOKS_DIR}"
EOF

EXPOSE 8081
STOPSIGNAL SIGTERM

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD graphit --non-interactive daemon status >/dev/null

VOLUME ["/home/graphit/.graphit"]

COPY --chmod=0755 scripts/graphit-entrypoint.sh /usr/local/bin/graphit-entrypoint

WORKDIR /home/graphit/.graphit
USER graphit
ENTRYPOINT ["/usr/local/bin/graphit-entrypoint"]
