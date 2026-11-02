#!/bin/sh
set -eu

# An explicit command does not bootstrap or mutate the daemon's installation.
if [ "$#" -gt 0 ]; then
  case "$1" in
    -*) ;;
    *) exec "$@" ;;
  esac
fi

: "${GRAPHIT_GLOBAL_DIR:?GRAPHIT_GLOBAL_DIR is required}"
hooks_dir=${GRAPHIT_ENTRYPOINT_HOOKS_DIR:-/docker-entrypoint.d}
for directory in "$GRAPHIT_GLOBAL_DIR" "$hooks_dir"; do
  case "$directory" in
    /*) ;;
    *) echo "Container state and startup hook directories must be absolute paths." >&2; exit 1 ;;
  esac
  [ "$directory" != / ] || { echo "Container state and startup hook directories must not be /." >&2; exit 1; }
done

umask 077
if ! mkdir -p "$GRAPHIT_GLOBAL_DIR" ||
  [ ! -r "$GRAPHIT_GLOBAL_DIR" ] ||
  [ ! -w "$GRAPHIT_GLOBAL_DIR" ] ||
  [ ! -x "$GRAPHIT_GLOBAL_DIR" ]; then
  echo "GRAPHIT_GLOBAL_DIR must be readable, writable and traversable by UID/GID 10001." >&2
  exit 1
fi

# Hooks are separate executable processes; failures stop startup before the exec.
if [ -d "$hooks_dir" ]; then
  for script in "$hooks_dir"/*; do
    if [ ! -f "$script" ] || [ ! -x "$script" ]; then continue; fi
    echo "Running $script"
    "$script"
  done
fi

exec graphit --non-interactive daemon --managed "$@"
