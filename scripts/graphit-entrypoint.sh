#!/bin/sh
set -eu

# Container listener ports are part of the image contract. Publish a different
# host port with Docker's HOST:CONTAINER mapping instead of moving the listeners.
GRAPHIT_UI_PORT=8080
GRAPHIT_MCP_PORT=8081
export GRAPHIT_UI_PORT GRAPHIT_MCP_PORT
GRAPHIT_ENTRYPOINT_HOOKS_DIR="${GRAPHIT_ENTRYPOINT_HOOKS_DIR:-/docker-entrypoint.d}"
export GRAPHIT_ENTRYPOINT_HOOKS_DIR

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

case "${GRAPHIT_ENTRYPOINT_HOOKS_DIR}" in
  /*) ;;
  *)
    echo "GRAPHIT_ENTRYPOINT_HOOKS_DIR must be an absolute path." >&2
    exit 1
    ;;
esac
if [ "${GRAPHIT_ENTRYPOINT_HOOKS_DIR}" = "/" ]; then
  echo "GRAPHIT_ENTRYPOINT_HOOKS_DIR must not be the filesystem root." >&2
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

run_scripts() {
  directory="${GRAPHIT_ENTRYPOINT_HOOKS_DIR}/$1"
  [ -d "${directory}" ] || return 0

  # POSIX pathname expansion returns paths in lexical order. The -f check
  # excludes directories, while -x lets users leave disabled files in place.
  for script in "${directory}"/*; do
    [ -f "${script}" ] && [ -x "${script}" ] || continue
    echo "Running ${script}"
    if "${script}"; then
      :
    else
      status=$?
      echo "Script failed (${status}): ${script}" >&2
      return "${status}"
    fi
  done
}

run_scripts pre-setup.d

if [ ! -f "${GRAPHIT_GLOBAL_DIR}/config.json" ]; then
  env GRAPHIT_MODULES_DAEMON=false graphit setup \
    --non-interactive \
    "--anonymize-events=${GRAPHIT_HUB_EVENTS_ANONYMIZE}" \
    "--agent=${GRAPHIT_AGENT}" \
    "--cli=${GRAPHIT_CLI}" \
    < /dev/null
  run_scripts setup.d
fi

run_scripts post-setup.d

if [ "$#" -eq 0 ]; then
  exec graphit daemon
fi

case "$1" in
  -*) exec graphit daemon "$@" ;;
esac

exec "$@"
