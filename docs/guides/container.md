# Run Graphit in a Linux container

The maintained image runs the Rust daemon as PID 1 with its current installation
and project inventory. MCP and the configured browser share the Source listener
on port 8081. Each instance owns its global directory; repositories are mounted
and registered separately.

## Build and start

Build the complete Linux release package and extract it for Docker:

```sh
make release
mkdir -p .build/release
tar -xzf .build/dist/graphit-linux-amd64.tar.gz -C .build/release
docker build -t graphit-code:dev .
docker volume create graphit-global
docker run -d --name graphit \
  -p 127.0.0.1:8081:8081 \
  -v graphit-global:/home/graphit/.graphit \
  graphit-code:dev
```

The Dockerfile consumes the complete package: both executables and graphit-runtime. It installs the Linux
base runtime libraries needed by those executables. The native
runtime includes upstream licenses, notices and provenance; see
[native runtime packaging](../development/native-runtime-packaging.md).

To use a CUDA-enabled release, include its complete package when building the
image. The Cargo-built engine and CUDA user-space runtime are included in the complete package; the
host supplies the NVIDIA driver and NVIDIA Container Toolkit. Start that image
with `docker run --gpus all ...` and select the desired device through Graphit
configuration. A CUDA-enabled build does not change the CPU default or
enable rerank. CPU-only images need no CUDA installation. A successful package
or Docker layout check does not prove actual GPU container execution.

The daemon uses `graphit daemon --managed`, which runs the retained global
inventory in the foreground. Its installation directory is never registered as
a project. A fresh instance can start with an empty inventory. Mount repositories
at stable paths and use `graphit projects --help` to maintain their registered
checkouts and versions.

The native SDK initializes missing installation state during startup and reads
the current persisted configuration. Environment and command overrides follow
the existing configuration precedence.

## Configuration and browser access

The image declares these current defaults:

| Variable | Default | Purpose |
|---|---|---|
| GRAPHIT_GLOBAL_DIR | /home/graphit/.graphit | Private installation and persistent state. |
| GRAPHIT_AGENT | codex | Native agent binding. |
| GRAPHIT_MCP__HOST | 0.0.0.0 | Explicit deployment listener. |
| GRAPHIT_MCP__PORT | 8081 | Shared Source port. |
| GRAPHIT_MODULES__AGENT | false | Agent automation is opt-in; no assistant CLI is installed in the image. |
| GRAPHIT_MODULES__DREAM | false | Dream is opt-in. |
| GRAPHIT_MODULES__DAEMON_UI | true | Allow the browser when its trusted Broker configuration is present. |
| GRAPHIT_ENTRYPOINT_HOOKS_DIR | /docker-entrypoint.d | Executable startup scripts. |

Dotted configuration segments use two underscores. For example, mcp.port maps
to GRAPHIT_MCP__PORT. GRAPHIT_GLOBAL_DIR is the bootstrap path exception.
The image does not overwrite listener values supplied at runtime. Its wildcard
listener is explicit; ordinary daemon configuration still defaults to loopback.
Private discovery always advertises same-family localhost and the actual port,
so installation-local CLI requests and runtime credentials stay local.

The browser requires trusted Broker providers and a PROJECT authorization
endpoint in task.browser. Configure the provider/profile through the maintained
[authentication](authentication.md) flows before starting the daemon. For example,
create a Broker provider in the stopped installation volume (the credential source
must agree with the provider type):

```sh
docker run --rm -v graphit-global:/home/graphit/.graphit \
  graphit-code:dev graphit --non-interactive provider add company \
  --type broker --broker-endpoint https://broker.example \
  --mcp-resource https://graphit.example/mcp \
  --s3-credential-source broker \
  --embedding-mode disabled --rerank-mode disabled
```

This example leaves model services disabled until configured; use the maintained
authentication flows to log in and configure the services needed by the deployment.
A deployment behind HTTPS uses an explicit public origin, for example:

```sh
docker run -d --name graphit \
  -p 127.0.0.1:8081:8081 \
  -v graphit-global:/home/graphit/.graphit \
  -e GRAPHIT_TASK__BROWSER='{"providers":["company"],"project_endpoint":"https://broker.example/v1/project/access/authorize","public_origin":"https://graphit.example"}' \
  graphit-code:dev
```

Replace the example provider and endpoints with the maintained Broker setup.
The origin is validated by the existing browser authentication implementation,
including HTTPS, callback and cookie rules. It is not inferred from Host or
forwarding headers. The default remains the local listener origin; HTTP browser
development requires the explicit allow_loopback_http option. Configure the
external HTTPS service to forward to the container's Source port.

A valid access token does not grant PROJECT access by itself. The existing
project authorization applies to MCP, browser and data operations. Keep each
instance's authentication state in its own private installation volume.

For a different host port, change the Docker mapping, for example
`-p 127.0.0.1:9090:8081`, and use the actual external browser origin.
Local models use the packaged llama.cpp/cxx GGUF runtime. Install/select the
appropriate model and configure its independent CPU/accelerator execution on
the Local provider; native files alone do not select a model or activate rerank.

## Storage, health and shutdown

The image runs as graphit, UID/GID 10001. The default installation is created
with mode 0700; runtime credentials and discovery are private files. A bind mount
or custom installation directory must already be owned and privately accessible
to that user. Provision it with the deployment's volume ownership mechanism.
The entrypoint creates a missing directory privately and checks access; it does
not elevate privileges or repair an existing directory's ownership or mode.

Docker health uses the existing authenticated local CLI:

```sh
docker exec graphit graphit --non-interactive daemon status
docker inspect --format '{{.State.Health.Status}}' graphit
docker stop --time 15 graphit
```

SIGTERM reaches the Rust process directly. It cancels work, joins retained
owners, then removes its own discovery and runtime key. A replaced foreign
record is not removed. Persistent configuration, identities, inventory and data
remain in the volume for a later start. Health fails when the Source service is
absent; it does not start another daemon.

## Customize startup

Place regular executable files directly in /docker-entrypoint.d, or select
another absolute directory with GRAPHIT_ENTRYPOINT_HOOKS_DIR. Startup scripts
run in lexical filename order as UID/GID 10001, after the private directory is
available and before the daemon starts. A failing script stops startup with its
exit status; non-executable files and subdirectories are skipped.

Scripts run as separate processes. Changes to their shell environment do not
change the daemon's environment; supply runtime environment values through the
container configuration. Persistent setup performed by a script should be
idempotent because startup scripts run on each daemon start.

Mount scripts outside the persistent installation volume:

```sh
docker run -d --name graphit \
  -p 127.0.0.1:8081:8081 \
  -v graphit-global:/home/graphit/.graphit \
  -v "$PWD/hooks:/docker-entrypoint.d:ro" \
  graphit-code:dev
```

An explicit command bypasses daemon startup and startup scripts, for example
`docker run --rm graphit-code:dev graphit --help`. Arguments beginning with
a dash are forwarded to the managed daemon. A derived image can install an
assistant CLI in PATH and enable the relevant module explicitly.
