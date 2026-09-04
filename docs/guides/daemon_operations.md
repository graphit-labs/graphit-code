---
title: Daemon Operations and Monitoring
type: guide
updated: 2026-09-03
tags: [daemon, watchers, scheduler, operations, mcp, embeddings]
---

# Daemon Operations and Monitoring

The Graphit daemon is one machine-wide supervisor for every registered project. It keeps project
indexes current, owns recurring maintenance, shares the configured embedding backend, exposes an
authenticated MCP endpoint, and can host the Observatory. It is not a database server for the
graph: project stores remain independent and are opened when a module or request needs them.

## How it starts

### Automatic CLI and MCP start

Ordinary `graphit` commands make a best-effort call to start the daemon before the requested command
runs. The attempt is skipped for `daemon`, `setup`, `uninstall`, `self-update`, and internal hook
commands, and whenever `modules.daemon=false`. Setup performs its own start near completion. Both
stdio MCP entry points ensure the daemon at startup, and the MCP server retries the same check for
tool calls.

Autostart is race-safe: processes serialize through `~/.graphit/daemon/.spawn.lock`, probe the lock
held on `daemon.pid`, resolve `GRAPHIT_LAUNCHER_PATH` when valid or the current executable otherwise,
and detach `graphit daemon`. Failure is intentionally non-fatal to the foreground command; explicit
operations that do not need background state can still work.

### Manual foreground start

```bash
graphit daemon
graphit daemon --no-embedding
graphit daemon --no-dream
graphit daemon --log /absolute/path/daemon.log
```

The command remains in the foreground; `Ctrl+C`, `SIGINT`, and `SIGTERM` request graceful shutdown.
Autostarted instances are the same command detached with stderr appended to the default daemon log.

`--no-embedding` disables the global embedding socket and per-project AST/wiki embedding loops for
that process. `--no-dream` disables per-project Dream runners. These process flags override module
configuration. A binary/grammar replacement preserves them; `graphit daemon restart` starts with
default flags and the default log path.

### Lifecycle commands

```bash
graphit daemon status
graphit daemon stop
graphit daemon restart
```

`status` reports PID, start time, uptime, PID path, machine-wide scope, scheduler state, and the last
ten lines of the **default** log. If the daemon was started with `--log`, inspect that file directly.
`stop` sends `SIGTERM`, waits up to ten seconds, then sends `SIGKILL` and removes PID/MCP discovery
files if graceful shutdown did not finish.

### Optional OS watchdog

The scheduler is not required for normal CLI use. Install it when Graphit must return after login,
reboot, or a crash even if no command is executed:

```bash
graphit daemon scheduler install
graphit daemon scheduler status
graphit daemon scheduler remove
```

| OS | Installed mechanism | Behavior |
|---|---|---|
| Linux | User crontab | Runs `<resolved-executable> daemon` every minute, output to `/dev/null` |
| macOS | `~/Library/LaunchAgents/<brand-label>.plist` | `RunAtLoad=true`, `StartInterval=60`, stdout/stderr to `/dev/null` |
| Windows | User Task Scheduler entry | Runs every minute through `schtasks` |

Each invocation exits immediately when the PID-file lock is already held. Installation replaces
only Graphit's marked entry/task and does not require administrator privileges.

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
| AST and wiki/memory embedding backlog | Immediately in their loops, then every 2 min | Generate missing semantic vectors and rebuild affected indexes |
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

## Per-project modules

| Module | Default | Work |
|---|---|---|
| Sync | On (`modules.sync`) | One recursive watcher; incremental AST updates and knowledge rebuilds |
| AST embedding | On (`modules.embedding`) | Embeds changed AST search documents every two minutes |
| Wiki embedding | On (`modules.embedding`) | Embeds knowledge plus project/user memory wiki targets every two minutes |
| Project memory maintenance | On (`modules.memory`) | Maintains the project's authoritative memory table every 15 minutes |
| Dream | Off (`modules.dream`) | Runs idle autonomous work and memory consolidation |

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

### Shared embedding socket

Unless disabled by process flag or `modules.embedding`, the daemon supervises one lazy embedding
server at `~/.graphit/daemon/embed.sock`. It serves whichever `ai.embedding.provider` is configured;
for local embeddings, one ONNX session can be reused across projects and short-lived processes.
Clients try the socket and fall back to direct construction automatically.

### User memory maintenance

When `modules.memory=true` and a user identity exists, one machine-wide module maintains the user
memory table every 15 minutes. It is independent of project parking.

### Authenticated MCP HTTP endpoint

The daemon always attempts to expose streamable HTTP MCP at `/mcp`:

- `mcp.host` defaults to `127.0.0.1`;
- `mcp.port` defaults to `0`, so the OS chooses a free port;
- the selected port is written to `~/.graphit/daemon/mcp.port`;
- a generated bearer secret is written to `~/.graphit/daemon/mcp.key` with mode `0600`;
- every request must send `Authorization: Bearer <key>`.

In the Observatory, open **System → Daemon**. The page shows the current port and usable endpoint;
its **MCP bearer key** button masks the value on screen and copies the complete key for the client
configuration. A daemon restart generates a new key, so clients must copy it again afterward.

The port file is discovery metadata, not a secret. The daemon rewrites both files if another
process deletes or changes them. On shutdown it removes them only when they still describe that
listener. Binding beyond loopback exposes every MCP tool to anyone holding the key; use a trusted
network, firewall, VPN, or authenticated proxy.

### Daemon-hosted Observatory

`modules.daemon_ui=true` starts the unified UI as a supervised global module. It selects the first
active registered project, or the global directory when none exists, and opens the AST store read-
only. Hub unavailability does not stop the mostly local UI. This mode is intended primarily for the
container/server deployment; workstation users normally run `graphit ui` on demand.

The UI listener uses `ui.host` and its own port behavior. It is distinct from the MCP listener and
has no built-in authentication.

## Resource control and failure recovery

The daemon lowers its process priority and uses `GRAPHIT_MAX_WORKERS`/the CPU budget. CPU-heavy AST,
Knowledge, and embedding cycles acquire one process-wide slot by default. Set
`GRAPHIT_HEAVY_SLOTS` above `1` only when the machine can trade higher peak CPU and memory for more
parallel throughput. Database, embedding, and ANTLR-specific environment limits remain available
as documented in [Configuration Reference](configuration.md#runtime-only-environment-controls).

Every project and global module is isolated behind panic recovery. An unexpected return or panic
causes exponential restart waits of 2, 4, 8, 16, then at most 30 seconds. Ten consecutive failures
mark only that module failed; the daemon and other projects continue. A run stable for at least 60
seconds resets the failure counter.

On shutdown, project supervisors stop concurrently. Each waits up to seven seconds for its modules;
the daemon waits up to ten seconds for all supervisors, flushes pending Hub events, then exits.

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
| `~/.graphit/daemon/.spawn.lock` | Serializes concurrent autostart attempts | `0600` |
| `~/.graphit/daemon/daemon.log` | Default global daemon log | Opened `0600`; spawned stderr appender may create `0644` |
| `~/.graphit/daemon/embed.sock` | Local embedding proxy | Unix socket; removed on close |
| `~/.graphit/daemon/mcp.port` | Selected MCP TCP port | Discovery metadata |
| `~/.graphit/daemon/mcp.key` | MCP bearer secret | `0600` |
| `~/.graphit/daemon/launcher.stamp` | Installed launcher/Core identity | Replacement input |
| `.graphit/runtime/daemon/daemon.log` | Per-project supervisor/module log | Generated, `0644` |
| `.graphit/runtime/daemon/rebuild.log` | AST/wiki embedding and rebuild details | Generated |

Paths under `~/.graphit` follow `GRAPHIT_GLOBAL_DIR` when that override is set.

## Diagnosis

```bash
graphit daemon status
tail -n 100 ~/.graphit/daemon/daemon.log
tail -n 100 .graphit/runtime/daemon/daemon.log
tail -n 100 .graphit/runtime/daemon/rebuild.log
graphit daemon scheduler status
```

- No daemon after an ordinary command: verify `modules.daemon`, executable resolution, and global
  directory permissions; then start `graphit daemon` in the foreground to see the error.
- A project is absent: confirm it still has `graphit.lock.json` and is registered in the global
  lock, then check whether it is parked by `daemon.activity_window`.
- A pipeline does not react: check `modules.sync`, its AST/Knowledge gate, the relevant ignore file,
  extension/parser support, and project logs.
- Semantic results lag: check `modules.embedding`, provider credentials/model availability, and run
  an explicit embedding command for a synchronous checkpoint.
- MCP clients cannot connect: read `mcp.port` and `mcp.key`, use `/mcp`, and send the Bearer header;
  do not look for the unrelated embedding socket over HTTP.
- A module reaches ten failures: fix the recorded error and restart the daemon to construct a fresh
  supervisor.

For provider behavior, see [AI Models, Providers, and Agent CLIs](ai_models.md). For developer-level
supervisor contracts, see [Daemon Module Specification](../specs/daemon_module.md).
