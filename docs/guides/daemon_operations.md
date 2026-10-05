---
title: Daemon Operations and Monitoring
type: guide
updated: 2026-09-23
tags: [daemon, watchers, service, operations, mcp, embeddings]
---

# Daemon Operations and Monitoring

The Graphit daemon is one machine-wide supervisor for every registered project. It keeps project
indexes current, owns recurring maintenance, shares the configured embedding backend, exposes an
authenticated MCP endpoint, and can host the Observatory. It is not a database server for the
graph: project stores remain independent and are opened when a module or request needs them.

## How it starts

### Automatic CLI and MCP start

Ordinary `graphit` commands register a per-user OS service on first use, then ask the OS manager to
start it before the command runs. Registration leaves login startup off. Existing exceptions in the
CLI root hook remain: `daemon`, `mcp`, `setup`, `uninstall`, `self-update`, `provider`, `login`, `logout`,
`account`, and internal commands do not autostart; `modules.daemon=false` also disables it. Setup
performs its own start near completion. Both stdio MCP entry points ensure the daemon at startup
and request the tray in a graphical session. The MCP proxy retries the daemon check while waiting
for its endpoint. MCP startup reports service errors on
stderr while it waits for the daemon endpoint. Codex installs this proxy with a 120-second startup
timeout because the OS service and runtime can take longer than its 30-second default after a cold
start. The ordinary CLI warns on manager errors
and still runs the requested foreground command; explicit service commands return the error.

Autostart serializes callers through `~/.graphit/daemon/.spawn.lock` and service registration through
`.service-install.lock`. If a foreground or older detached daemon still holds `daemon.pid`, it is
stopped before the service starts. The daemon itself holds that PID lock throughout its lifetime,
so simultaneous CLI invocations and OS starts cannot produce two active daemons.

### Manual foreground start

```bash
graphit daemon
GRAPHIT_MODULES_DAEMON_UI=true graphit daemon
graphit daemon --no-embedding
graphit daemon --no-dream
graphit daemon --log /absolute/path/daemon.log
```

The command remains in the foreground; `Ctrl+C`, `SIGINT`, and `SIGTERM` request graceful shutdown.
`GRAPHIT_MODULES_DAEMON_UI=true` starts the Observatory with the daemon. Otherwise it starts
only when requested from the tray or `graphit ui`; managed mode and stored module settings
do not activate it.
Autostarted instances run through the OS service manager using the stable launcher path when the
launcher supplied one; a directly executed binary registers its own path.

`--no-embedding` disables the global embedding socket and per-project AST/wiki embedding loops for
that process. `--no-dream` disables per-project Dream runners. These process flags override module
configuration. Manual foreground replacement preserves them; managed instances restart through the
OS manager with the service's default flags. `graphit daemon restart` also uses those defaults.

### Lifecycle commands

```bash
graphit daemon status
graphit daemon stop
graphit daemon restart
```

`status` reports PID, start time, uptime, PID path, machine-wide scope, service state, and the last
ten lines of the **default** log. If the daemon was started with `--log`, inspect that file directly.
`stop` tells the OS manager to stop the service, preventing automatic relaunch; it also stops a
remaining foreground or legacy daemon. A later eligible command or explicit `start` can start it
again. `restart` stops and starts through the OS manager and waits for the PID lock.

### Per-user OS service

The service is registered automatically on the first eligible command, without login startup.
Enable startup at **user login** explicitly:

```bash
graphit daemon service install --login
graphit daemon service uninstall --only-login
graphit daemon service login enable
graphit daemon service login status
graphit daemon service status
graphit daemon service stop
graphit daemon service start
graphit daemon service uninstall
```

| OS | Installed mechanism | Behavior |
|---|---|---|
| Linux | `systemd --user` unit | `Restart=on-failure`; `enable` only with `--login` |
| macOS | LaunchAgent | `KeepAlive` after nonzero exit; login plist only with `--login` |
| Windows | User Task Scheduler task | `RestartOnFailure` (up to 255 retries, one minute apart); separate login task only with `--login` |

On Linux without a reachable `systemd --user` manager, an eligible command still starts one
detached daemon directly, with UI inactive unless the environment enables it. The PID lock prevents a second daemon, and
`graphit daemon stop` or the tray's **Quit** stops it. This fallback has no OS supervisor:
it does not restart after a crash or start at login. `graphit daemon service install` reports
the unavailable manager instead of installing a service. If `systemd --user` responds but
service installation or startup fails, Graphit reports that error rather than using the fallback.

All three mechanisms run under the current user without normal administrator elevation. The
Linux MCP proxy can start `systemd --user` even when its agent host omits desktop session
variables: Graphit supplies this user's `/run/user/<uid>` bus address to `systemctl` when needed.
macOS addresses the `launchd` GUI domain by user ID, and Windows uses the current user's Task
Scheduler registration. The daemon serves its UI in the same process, on demand, using the configured `ui.host` and `ui.port`.
The daemon publishes the UI's actual address only after its listener binds. The service uses no one-minute cron or timer. Installation removes only
Graphit's identified legacy watchdog entry. `graphit daemon scheduler` remains an alias for
`daemon service`. `service uninstall` stops and removes the service and its login startup; `remove`
is an alias. `service uninstall --only-login` turns off daemon and tray startup at login without stopping the
running daemon or removing its service registration. `--managed` is an internal service flag and is
not needed when starting the daemon manually. A later eligible command registers an uninstalled
service again with login startup off.

### System tray

In a graphical session, eligible CLI commands and the stdio MCP proxy start a separate
`graphit tray` process if one is
not already running. The daemon service remains headless. The tray menu shows running, starting,
stopped, or error state and the active login provider and profile. When there is no active login,
it lists configured first-class Broker providers under **Sign in with Broker**. Selecting one opens
the existing browser login flow and activates a profile named after the provider unless a profile
for that provider already exists. The menu can **Open UI**, **Start daemon**, **Restart daemon**,
or **Quit**. Quit stops the service, when available, and the daemon before
closing the tray; if stopping fails, the tray stays open and shows an error. When a service is
installed, it remains registered: the next eligible command can start it again, and login startup
runs at the next login when enabled. Without `systemd --user`, the tray shows direct daemon state
and keeps **Start daemon** and **Restart daemon** available. Open UI requests the daemon's UI, waits for its listener, and opens the published URL in the default browser.
Repeated requests reuse that server. The published address includes the actual port when the configured port was
already occupied. If the daemon is stopped,
use **Start daemon** first. A browser launch failure produces a tray notification.

To check why an icon is missing, run `graphit tray` from a terminal in the graphical session.
On Linux it reports when the desktop lacks a StatusNotifier/AppIndicator watcher; enable that
desktop support to display the icon. The daemon and MCP continue working without the tray.

`graphit daemon service install --login` also enables the tray at user login. Tray login startup can
be managed separately with `graphit tray login enable`, `disable`, and `status`. The daemon and tray
use separate locks and processes; a missing graphical session never blocks the daemon. Linux tray
display requires a desktop that exposes a StatusNotifier/AppIndicator watcher over the session D-Bus.
When an agent host omits the desktop variables, the Linux MCP proxy retrieves only the session
addresses needed for the tray from `systemd --user`. A tray startup failure never stops MCP or the
daemon; the tray's stderr is written to `~/.graphit/daemon/tray.log`.
If that support is absent, `graphit tray` reports the missing watcher and the daemon continues
without an icon. macOS uses the menu bar; Windows uses the notification area. Native display behavior
on macOS and Windows still needs on-device verification.

## Boot sequence

1. Lower process priority and cap Go scheduling with the shared CPU budget.
2. Move the working directory to the global Graphit directory so deleting the checkout that
   launched it cannot invalidate the process CWD.
3. acquire the mode-`0600` singleton lock at `~/.graphit/daemon/daemon.pid`;
4. record the launcher stamp and global grammar signature;
5. open the daemon log;
6. start global modules;
7. publish the authenticated MCP listener after the PID has been claimed;
8. discover registered projects immediately and start eligible supervisors;
9. enter the discovery and replacement-check loops.

The daemon runs exactly once per machine/global directory. Projects do not start their own daemon.

## What the daemon monitors

| Signal or loop | Frequency | Result |
|---|---:|---|
| Registered project set in the global lock | Immediately, then every 30 s | Add, park, resume, or remove project supervisors |
| Launcher stamp | Every 30 s | Graceful self-replacement after a Core/launcher update |
| Global and supervised-project grammar signatures | Every 30 s | Replacement so newly installed/removed native parsers are loaded safely |
| Project filesystem events | OS notifications; 1 s quiet / 5 s max debounce | Incremental AST paths and/or full knowledge rebuild |
| AST and knowledge-wiki embedding backlog | Immediately, again right after an index run changed that store, and every 30 min as a safety net | Generate missing semantic vectors and rebuild affected indexes |
| Memory embedding backlog | Immediately, then every 2 min | Generate missing semantic vectors for new or changed memories |
| Project and user memory tables | Immediately, then every 15 min | Ensure indexes, compact, and prune retained versions when due |
| Dream idle state | Continuous per enabled project | Start bounded autonomous sessions after `dream.idle_timeout` |
| MCP discovery files | Every 5 s while serving | Restore the chosen port and bearer-key files if deleted or changed |

The discovery and version intervals are internal daemon defaults. The user-configurable timing keys
are `daemon.activity_window`, `dream.idle_timeout`, `dream.max_duration`, and the retention windows.

## Project discovery, activity, and parking

`graphit init` registers a project in the global lock. Discovery keeps entries whose project
lockfile still exists; the daemon then decides whether each is active.

- **Supervised:** project modules and the recursive filesystem watch are running.
- **Parked:** registration remains, but the supervisor, watches, embedding loops, and Dream runner
  are stopped.
- **Gone:** discovery no longer returns the project, so all runtime state is removed.

`daemon.activity_window` defaults to `30m`. A supervised project is parked after that much time
without a filesystem event. A parked or newly discovered project is resumed when a filesystem walk
finds a modification within the window. The walk excludes `.git` and `.graphit`; a failed probe is
treated as active so access errors do not incorrectly park work. Set the value to `0` to keep every
registered project supervised.

The activity window is read once at daemon start. Restart after changing it. A project
configuration's module set is also chosen when its supervisor is constructed; restart or let the
project park/resume after changing `modules.sync`, `modules.embedding`, `modules.dream`, or
`modules.memory`. Settings used inside a sync batch—such as AST grammar bindings, knowledge scope,
source storage, and AST/Knowledge gates—are reloaded from `graphit.lock.json` for every batch.

### One supervisor per checkout and project version

A worktree arrives with the committed lockfile, and its first `graphit init` or `graphit sync` —
including the `sync` an agent's stop hook starts — registers it as its own instance of the project
in the global lock. Each instance is supervised separately: the supervisor is keyed by project,
checkout directory and the
[project version](../architecture/storage_layout.md#one-store-per-project-version) checked out
there. Switching branches in a checkout changes that key, so the next discovery pass (every 30
seconds) stops the old supervisor and starts one for the new version; its watcher start runs a full
`graphit sync`, which creates the new version's stores from the default version and indexes only what
differs. No `post-checkout` hook is needed for this.

Every project job the daemon starts is pinned to the version checked out when it starts
(`GRAPHIT_PROJECT_VERSION`), so a run never switches stores halfway through a branch change, and a
version pinned in the daemon's own environment never leaks into a project.

Agent session hooks resolve the version of the checkout they fire in and send it with the
store-backed request; the daemon answers Task lifecycle and mandatory memory from that version, and
the session bootstrap names it.

## Process model

The daemon executes jobs that require `graphit-core` in its own process: published-base
hydration, AST and Knowledge indexing, AST/wiki embedding, full watcher resynchronization and
Dream's session bookkeeping. It calls their services with an explicit project directory,
configuration, version and cancellation context; it does not change the process directory or
set environment variables to imitate a CLI invocation.

Dedicated binaries retain their process boundary: `graphit-mcp`, `graphit-model`,
`graphit-store`, the tray and hook binaries, plus external SCIP containers and agent CLIs.
The UI also runs inside the daemon when activated, as described below.

```text
graphit daemon / graphit-core
├── internal project jobs: sync, hydrate, AST/Knowledge index, AST/wiki embed, Dream
├── graphit-model: local embedding and rerank models
├── graphit-store: local/direct Memory and Task maintenance and embedding
└── external Dream agent CLI and SCIP container when their work is required
```

- **Scope.** Each core phase uses a cancellation-aware project-version lease and explicit
  paths. Full sync carries its captured version into each phase without an outer lease.
  Dream scopes ledger creation briefly, releases the lease before agent inference, and passes
  `GRAPHIT_PROJECT_VERSION` to the agent. Its scoped proxy adds that captured version to omitted
  `project_version` arguments for the launch project, including after branch changes and reconnects;
  explicit tool versions and other project/user scopes are preserved. Same-project MCP tools can run during Dream.
  Dedicated project jobs receive that version in their own environment; user jobs retain user scope.
- **Logs.** Job output goes to `.graphit/runtime/daemon/daemon.log`; global modules use
  `~/.graphit/daemon/<module>.log`.
- **Resources.** Core jobs share the daemon's CPU and memory. Owned graph, search and wiki
  handles close when each phase finishes; grammar caches remain shared process resources.
  Native allocators may retain pages after close, so this does not promise an immediate RSS drop.
  Local models remain in `graphit-model`.
- **Cancellation.** Core jobs observe the module context, including waits for version and heavy-work
  leases. Dream cancels and joins its active session before returning. Dedicated child trees retain
  SIGTERM then SIGKILL after ten seconds on Linux/macOS and `taskkill /T /F` on Windows, with no
  console window for background children.
- **Failures.** Scheduled job errors and Go panics are logged and retried on a later tick. Native
  library crashes in an internal job can terminate the daemon; a Go recovery boundary cannot
  contain them. Dedicated binaries keep their process isolation.
- **Scheduling.** AST and Knowledge embedding run at module start, after a successful dependent
  index, and otherwise every thirty minutes. Triggers coalesce while a job is busy and reset the
  fallback timer. Empty filesystem batches do no indexing. Hydration is skipped without a
  possible published base; Broker storage still hydrates because it is discovered remotely.
- **Full watcher rescan.** The internal sync preserves hydration, indexing, authoritative Memory
  index refresh, skills/roles, adapter and Git-hook refresh, and heavy artifact reconciliation.
  It does not relaunch `sync` or `sync --heavy` in another core process.
- **Interactive `graphit sync`.** Its existing terminal/background CLI behavior remains available.
  The daemon's internal scheduling does not change that command's flags.

Each command above is an ordinary CLI command; see [CLI Reference](cli_reference.md#maintenance-commands-run-by-the-daemon)
to run any of them by hand.

### Lightweight runtime binaries

The launcher embeds dedicated binaries next to the core and runs them for invocations that do not
need the core's stores, parsers and native libraries:

| Invocation | Binary | Links | Typical memory |
|---|---|---|---|
| `graphit mcp --stdio` | `graphit-mcp` | Pure Go, proxies to the daemon | 12–35 MB |
| `graphit _session-hook ...` (Agent hooks) | `graphit-hook` | Pure Go | ~7 MB per event (core: ~150–165 MB) |
| `graphit _git-hook <event> ...` (Git hooks) | `graphit-githook` | Pure Go | ~3–8 MB per event (core: ~80 MB) |
| `graphit tray` (the resident tray) | `graphit-tray` | Tray UI only | ~11 MB |
| `graphit model serve <embedding\|rerank>` | `graphit-model` | ONNX Runtime only | ~11 MB idle, plus the loaded model |
| `graphit task maintain`, `graphit memory maintain\|embed\|index [--user]` started by the daemon | `graphit-store` | LanceDB and the embedding client | about 50 MB less than the core for the same run |

`graphit-hook` renders the hook payload itself from the project lockfile, the module mandates and
the installed Hub rules. Only the part that needs a store — the Task lifecycle effect (reconcile,
heartbeat or release) and reading mandatory memory — goes to the running daemon over the
authenticated local endpoint `POST /internal/session-store` (the same runtime key as `/mcp`); an
event that needs neither, for example with Task and Memory disabled, never leaves the hook
process. An Agent hook fired on every tool call therefore no longer starts a full core process
(~160 MB). Its output is identical to the core command's. When no daemon is running, it hands the
same event to the core, which handles it entirely; the hook never starts the daemon itself. The
Agent configuration keeps calling `graphit _session-hook`.

`graphit-store` serves only the store maintenance forms above, and only when a managed parent (the
daemon, or a CLI command that runs its children) started them; the same command typed in a
terminal still runs in the core, which also makes sure the daemon and tray are up. With a Broker
provider whose storage credentials are scoped per project, the daemon executes the existing
authorized store service internally, because its core already links the Hub registry. This avoids
launching `graphit-store` only for it to relaunch core. The standalone store binary retains its
core fallback for other managed callers; authorization still happens before mutation.

`graphit-tray` runs only the bare `graphit tray`; `graphit tray login ...` stays in the core.

## Per-project modules

| Module | Default | Scheduled job |
|---|---|---|
| Sync | On (`modules.sync`) | One recursive watcher in the daemon; `hub hydrate --json`, `ast index --changes-from -` and `knowledge index` per batch, `sync` when the watcher starts |
| AST embedding | On (`modules.embedding`) | `ast embed --if-idle` after an AST index changed the store, otherwise every 30 minutes; skipped while an AST index is publishing |
| Knowledge embedding | On (`modules.embedding`) | `wiki embed` after a Knowledge index, otherwise every 30 minutes |
| Memory embedding | On (`modules.embedding`, `modules.memory`) | `memory embed` every two minutes (user table: `memory embed --user`, global) |
| Project memory maintenance | On (`modules.memory`) | `memory maintain` every 15 minutes |
| Task maintenance | On (`modules.task`) | `task maintain` every 15 minutes |
| Dream | Off (`modules.dream`) | Idle detection in the daemon; each due session runs as `dream run --session <id>` |

`modules.ast` and `modules.knowledge` are evaluated within Sync and can independently suppress the
corresponding work. Disabling `modules.sync` removes the watcher entirely. Explicit CLI/MCP index
commands remain explicit operations where documented; module switches define orchestration, not a
general filesystem permission system.

### Disable filesystem watching

The watcher is enabled by default. Disable it for only the current project with:

```bash
cd /path/to/project
graphit config modules.sync false
graphit daemon restart
```

Disable it for every registered project with `graphit config --global modules.sync false`, or use
`GRAPHIT_MODULES_SYNC=false` in the daemon environment. Set `modules.sync` back to `true` and restart
to restore it. A restart reconstructs every project supervisor; alternatively, a project-level
change takes effect the next time that project is parked and resumed.

This switch removes the recursive OS watcher and all event-driven AST and Knowledge updates. It
does not stop the daemon, background modules controlled by other switches, explicit `graphit sync`,
or direct AST/Knowledge index commands. Use `modules.ast=false` or `modules.knowledge=false` when the
watcher should remain active for one pipeline but not the other.

### Filesystem routing

Sync creates one recursive OS watcher from the union of the AST and Knowledge interests. A path is
omitted from the underlying watch only when both ignore checkers reject it, then each pipeline
applies its own rules again during classification.

- AST receives files with a parser for the extension, subject to `.astignore`, grammar selectors,
  and the documentation-tree rule controlled by `ast.index_docs`.
- Knowledge receives configured extensions under `knowledge.docs_dir` plus explicitly included
  files such as the root README, subject to `.wikiignore`.
- A file can route to both pipelines.
- Hidden-directory events are not indexed; `.git` is never watched.
- Creates/modifications and removals are kept separate so AST can update only named paths.
- A watcher overflow sets `Rescan`; AST falls back to a full scan and Knowledge performs its normal
  full rebuild.
- New directories are registered and scanned immediately to close the create-before-watch race.
- Unreadable subtrees are skipped; Linux watch exhaustion produces an actionable inotify message.

See [Filesystem, State, and Watchers](filesystem_contract.md) and [Ignore Files](ignore_files.md) for
the complete file contract.

## Global modules and services

### Local model servers

The daemon supervises two model-server processes, `graphit model serve embedding` (unless disabled
by `--no-embedding`) and `graphit model serve rerank`, run by the `graphit-model` binary. Each
listens on its own socket, `~/.graphit/daemon/embed.sock` and `~/.graphit/daemon/rerank.sock`,
loads its model on the first request and serves the active provider's capability, so one model is
loaded per machine and its memory belongs to that process alone. A server that exits is restarted
with the daemon's normal backoff; a server whose daemon dies notices the closed pipe and exits.
If that path would exceed the system's Unix socket path limit (103 bytes, so it fits Linux,
macOS and Windows), both sockets move to a short per-user directory, `graphit-<hash>/` under
`$XDG_RUNTIME_DIR` when set, otherwise the temporary directory; the hash is derived from the global
directory and user, so the servers and every client compute the same path. That directory is created
with mode `0700`, and on Linux and macOS one owned by another user, or a symlink, is refused.

Every other process — the daemon's own MCP search, project children, interactive commands — uses the
socket when it answers. The daemon never loads a local model itself: while a local model's server
is unavailable, a daemon request that needs it fails with `local <task> model server is not
running`. Without a daemon, CLI commands build the configured client in their own process as before.

### User memory

When `modules.memory=true` and a user identity exists, the daemon runs `graphit memory maintain
--user` every 15 minutes and `graphit memory embed --user` every two minutes. These are independent
of project parking.

### MCP HTTP endpoint and health

The daemon always attempts to expose authenticated streamable HTTP MCP at `/mcp`:

- `mcp.host` defaults to `127.0.0.1`;
- `mcp.port` defaults to `0`, so the OS chooses a free port;
- the selected port is written to `~/.graphit/daemon/mcp.port`;
- each daemon start generates a fresh local runtime key;
- the runtime key is written to `~/.graphit/daemon/mcp.key` with mode `0600`;
- a local provider's static MCP key is also accepted;
- with an active Broker provider, each caller's access JWT must contain the configured MCP
  resource audience and is forwarded unchanged to Broker calls; Broker APIs validate their own
  audience in that same token;
- every `/mcp` request must send `Authorization: Bearer <key>`.

The same listener exposes unauthenticated `GET /health`, which returns HTTP 200 and
`{"status":"ok"}`. This is a liveness check for the MCP listener, not a deep check of stores,
providers, or downstream services.

In the Observatory, open **System → Daemon**. The page shows the current port and usable endpoint;
its **MCP bearer key** button masks the value on screen and copies the complete key for the client
configuration. Runtime keys rotate with a new daemon runtime. Stable per-account credentials and OIDC renewal are
managed by `graphit login`; see [Authentication](authentication.md).

The Observatory daemon page is status-only. Use the `graphit daemon` CLI commands listed under
**Lifecycle commands** for lifecycle administration.

The port file is discovery metadata, not a secret. The daemon rewrites both files if another
process deletes or changes them. On shutdown it removes them only when they still describe that
listener. Binding beyond loopback exposes every MCP tool to anyone holding the key; use a trusted
network, firewall, VPN, or authenticated proxy.

The bearer authenticates possession of one daemon credential; it does not identify distinct Hub
users or teams. A multi-user Hub deployment requires a trusted proxy or identity adapter to attach
the subject used by deny-by-default project ACLs. Client-supplied identity arguments and arbitrary
headers are rejected. The daemon may cache authorized Hub metadata below
`~/.<brand>/hub/cache/<hub>/<subject>/`, but remote content and mounts always revalidate access.

### Daemon-supervised Observatory

The daemon starts its unified UI only when requested by **Open UI** in the tray or `graphit ui`.
Set `GRAPHIT_MODULES_DAEMON_UI=true` to start it with the daemon, for example in a container.
An absent, false, or invalid value leaves the UI unallocated until a request; stored
`modules.daemon_ui` configuration and managed mode do not override this startup rule.

The server runs inside the daemon and shares its native runtime and CPU budget. Its default
scope is global, with no project selected and no project AST backend opened. Select a project
in the interface, or use `graphit ui --repo <directory>` for an explicit standalone server.
Concurrent requests reuse the same UI; the browser opens only after its listener binds.
The supervisor rebuilds a failed server with normal backoff. Cancelling a requester does not
stop the server. Daemon shutdown waits for UI resources and removes its published URL before
releasing the PID lock. Hub unavailability does not stop the mostly local UI.

The UI listener uses `ui.host` and its own port behavior. It is distinct from the MCP listener;
`ui.auth.enabled` enables Broker browser login and protects its data APIs.

The UI opens AST backends on demand. Every 30 seconds it releases the base backend and
context backends unused for two minutes, and trims the context cache to its 16 most recently
used entries. Cleanup waits for active AST requests; later queries reopen or recreate the backend.
Shutdown drains those requests before closing all backends. This releases native handles,
prepared statements and the canonical manifest, but native allocators may retain memory pages;
RSS is not guaranteed to drop immediately. MCP still uses native libraries in the daemon.
The daemon status API reads at most the final 64 KiB of its log and returns up to 50 complete
lines. An oversized line crossing that boundary is omitted. Broker-authenticated browsers
continue to receive no daemon logs or local MCP key.

For maintainers with a launcher built for the host and a Docker image built from the same source,
`go run ./cmd/qualitygate ui-smoke graphit graphit-ui-optimization:local` checks native UI readiness
and daemon activation on demand, then container UI on/off, authentication, fixed ports,
URL ownership, absence of a second UI process and container stop. The first argument may instead name the freshly
built launcher. This invocation works on Windows, macOS and Linux with Docker available; it creates
isolated temporary state and containers and removes them afterward. It does not restart the resident
user daemon. Unit lifecycle tests run through the portable pre-commit gate and the three-OS CI matrix.


## Resource control and failure recovery

The daemon lowers its process priority and uses `GRAPHIT_MAX_WORKERS`/the CPU budget.
Its UI and internal core jobs share that process and Go CPU budget. The CPU-heavy services and commands — `ast index`, `knowledge index`,
`ast embed`, `wiki embed` and `memory embed` — take one **machine-wide** slot before they start,
whether the daemon, an interactive command or a Git hook started them. A slot is an advisory file
lock under `~/.graphit/locks/`, released by the operating system when its holder exits. Only the phase doing heavy work holds a slot; sync orchestration never holds one while
calling a service or awaiting a dedicated child that acquires its own slot. Set
`GRAPHIT_HEAVY_SLOTS` above `1` only when the machine can trade higher peak CPU and memory for more
parallel throughput. Database, embedding, and ANTLR-specific environment limits remain available
as documented in [Configuration Reference](configuration.md#runtime-only-environment-controls).

That CPU gate is complemented by cross-process lifecycle locks for the AST and Knowledge stores.
CLI, MCP, watcher, and daemon indexers therefore cannot publish or reset a store while an
embedding cycle has handles open. AST and Knowledge embedding close them and release their locks
during model inference, then validate current content before publishing each batch. Full and incremental runs
use the same path; after publication, the next embedding cycle opens the current store automatically.

Every project and global module is isolated behind panic recovery. An unexpected return or panic
causes exponential restart waits of 2, 4, 8, 16, then at most 30 seconds. Ten consecutive failures
mark only that module failed; the daemon and other projects continue. A run stable for at least 60
seconds resets the failure counter.

On shutdown, project supervisors cancel concurrently and join their modules before closing
resources and logs. Parking or replacing a supervisor also waits for this cleanup before a
successor starts. The daemon waits for those joins and pending Hub events before exiting. An
unresponsive native call can delay graceful shutdown; forced OS/service-manager termination
remains the recovery path rather than abandoning a job that still owns process resources.

## Binary and grammar replacement

The launcher writes `~/.graphit/daemon/launcher.stamp`. The daemon compares the value every 30
seconds. It also fingerprints native grammar libraries in the global grammar directory and every
supervised project grammar directory. A changed stamp or signature triggers this sequence:

1. stop project/global work gracefully;
2. close the old MCP listener and remove its discovery files;
3. spawn a detached daemon from the resolved launcher/current executable, preserving
   `--no-embedding`, `--no-dream`, and `--log`;
4. let the new process acquire the singleton lock and publish its endpoint.

Query YAML is reloaded independently by the AST query loader and does not require native-library
replacement. New or removed parser binaries do.

## Runtime files and logs

| Path | Purpose | Mode/notes |
|---|---|---|
| `~/.graphit/daemon/daemon.pid` | PID, UTC start time, and singleton lock | `0600` |
| `~/.graphit/daemon/.spawn.lock` | Serializes concurrent autostart attempts | `0600`; acquisition waits at most 10 seconds |
| `~/.graphit/daemon/daemon.log` | Default global daemon log | Opened `0600`; spawned stderr appender may create `0644` |
| `~/.graphit/daemon/embed.sock` | Local embedding model server | Unix socket; removed on close; moves to a short per-user directory when the global path is too long |
| `~/.graphit/daemon/rerank.sock` | Local rerank model server | Unix socket; removed on close |
| `~/.graphit/daemon/model-embedding.log`, `model-rerank.log` | Model-server output | Appended |
| `~/.graphit/daemon/memory-*-user.log` | User-scope Memory child output | Appended |
| `~/.graphit/locks/heavy-slot-<n>.lock` | Machine-wide heavy-work slots | Advisory locks; released when the holder exits |
| `~/.graphit/daemon/mcp.port` | Selected MCP TCP port | Discovery metadata |
| `~/.graphit/daemon/mcp.key` | Generated runtime MCP bearer secret, rotated with the daemon | `0600` |
| `~/.graphit/daemon/launcher.stamp` | Installed launcher/Core identity | Replacement input |
| `.graphit/runtime/daemon/daemon.log` | Per-project supervisor/module log and the output of project jobs and dedicated children | Generated, `0644` |
| `.graphit/runtime/daemon/rebuild.log` | AST/wiki embedding and rebuild details | Generated |

Paths under `~/.graphit` follow `GRAPHIT_GLOBAL_DIR` when that override is set.

## Diagnosis

```bash
graphit daemon status
tail -n 100 ~/.graphit/daemon/daemon.log
tail -n 100 .graphit/runtime/daemon/daemon.log
tail -n 100 .graphit/runtime/daemon/rebuild.log
graphit daemon service status
```

- No daemon after an ordinary command: verify `modules.daemon`, launcher resolution, the user
  service manager, and global directory permissions. Run `graphit daemon service status`; start
  `graphit daemon` in the foreground to inspect daemon-specific startup errors.
- A project is absent: confirm it still has `graphit.lock.json` and is registered in the global
  lock, then check whether it is parked by `daemon.activity_window`.
- A pipeline does not react: check `modules.sync`, its AST/Knowledge gate, the relevant ignore file,
  extension/parser support, and project logs.
- Semantic results lag: check `modules.embedding`, provider credentials/model availability, and run
  an explicit embedding command for a synchronous checkpoint.
- MCP clients cannot connect: read `mcp.port` and the active runtime key or inspect the active
  account profile, use `/mcp`, and send the Bearer header. After a profile/provider change, do not
  look for the unrelated embedding socket over HTTP.
- A module reaches ten failures: fix the recorded error and restart the daemon to construct a fresh
  supervisor.

For provider behavior, see [AI Models, Providers, and Agent CLIs](ai_models.md). For developer-level
supervisor contracts, see [Daemon Module Specification](../specs/daemon_module.md).

## Validate internal jobs

Build a host launcher and Docker image from the same checkout, then run from the repository root:

```text
go run ./cmd/qualitygate core-smoke graphit graphit-core-jobs:local
```

Replace `graphit` with the path to that freshly built host launcher and the image tag with the
matching local image. The Go invocation works on Windows, macOS and Linux hosts with Go and
Docker available. It creates private project/global registries, starts an isolated daemon, checks
startup rescan and filesystem-triggered AST source/Knowledge updates through MCP, and verifies
joined shutdown. It repeats the scenario in a disposable Linux container. The container fixture
uses root solely to access private bind-mount permissions independently of host UID; the separate
`ui-smoke` runner exercises normal image-user startup. Neither runner restarts the resident daemon.
Cross-compilation alone does not establish native Windows/macOS runtime behavior.

Fast core-job, cancellation, Dream/proxy-version and Memory-scope regressions are selected by
`go run ./cmd/qualitygate pre-commit`. The full native suite and vulnerability analysis remain in
`go run ./cmd/qualitygate pre-push`; these commands are the saved Git gates.

CLI AST indexing (`init`, `sync`, `ast index`) also records SCIP failures,
parse errors and graph diagnostics in the project daemon log. Detailed
messages stay out of terminal output and the global CLI log.
This includes query profile validation/reloads, embedded parser diagnostics
and accumulated cache, search or graph write errors.
