---
title: Daemon Operations and Monitoring
type: guide
updated: 2026-10-17
tags: [daemon, watchers, service, operations, mcp, embeddings]
references:
  - target:
      type: task
      id: tsk-0529
    relation: implements
  - target:
      type: task
      id: tsk-3747
    relation: supports
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-c8cd
    relation: supports
  - target:
      type: task
      id: tsk-24fc
    relation: supports
  - target:
      type: task
      id: tsk-9d9d
    relation: supports
  - target:
      type: task
      id: tsk-ba35
    relation: supports
  - target:
      type: memory
      id: 01M48ZVYFZQE3W17F7T2ZMAE84
    relation: derived_from
  - target:
      type: task
      id: tsk-9d92
    relation: supports
  - target:
      type: task
      id: tsk-cab0
    relation: supports
---

# Daemon operations

This guide covers the Rust daemon's per-installation service and process lifecycle.
The Source foreground owner is implemented. Per-user service adapters and the
typed application entry are implemented. Managed startup supports an empty
inventory from outside PROJECT and later maintains/retires a real checkout.
Service dispatch, detached startup and ordered replacement have qualified Linux
journeys in `tsk-0529`. Native Task Scheduler and launchd journeys remain pending.
Use the commands actually exposed by `graphit daemon --help`; an adapter test
does not establish that a command has been connected to the executable.

## Run the foreground service

```text
graphit daemon foreground
```

The process retains the installation's native singleton, current authentication
owner and bound Source listener for its lifetime. Discovery is published only
after the listener has bound. A discovery record gives the address and PID; it
does not grant permission or prove that a store or provider is healthy.

`Ctrl+C`, `SIGINT` and `SIGTERM` request cancellation. Shutdown stops admissions
and joins workers before removing the owned discovery record and runtime key.
A different valid discovery record is preserved. The singleton inode remains
in place when its lock is released. A cleanup error retains the causal failure
and any owner or outcome that still needs cleanup; it is not successful shutdown.

The service's working directory and launch paths must be absolute and independent
of a project checkout that may be deleted. Projects and worktrees keep their
separate identities, versions, authorization and mutable state. Removing or
parking one project cancels and joins its work without draining other projects.

Managed startup uses the same owner with the selected installation and global
inventory, without deriving PROJECT from the launching CWD:

```text
graphit --global-dir /absolute/installation daemon --managed
```

The private Main fixture verified empty startup, deletion of that launching
directory, then the real `projects maintain-checkout`/`retire-checkout` HTTP
producer and joined SIGTERM shutdown. An empty inventory does not invent a
default project or grant PROJECT authority.

The multi-project runner is [`scripts/verify-daemon-multiproject.py`](../../scripts/verify-daemon-multiproject.py). It uses a disposable installation and never controls the resident daemon.

Root CLI PROJECT reads maintain that checkout before reading; they can resume a
retired checkout. To observe retirement itself, this journey uses the same
authenticated Graph HTTP request before and after retirement without invoking
the launcher. An absent live Graph selection returns HTTP400 `invalid graph
request`; it is not a successful read of the retained graph.

Switching a project version must update its AST and Knowledge context while other projects continue independently. Before writing to an inherited Knowledge target, the foreground uses the existing mutable Data path.

Projects using the same local model profile share the model child; retiring one project must not stop a child still used by another project.

The CPU journey took 84.908 seconds of execution using existing executables.
Its model-server dependency build took 293.486 seconds including Cargo-lock
waiting and is recorded separately. The fixture qualifies shared foreground
composition and lifetime. Memory/Dream, profile-generation borrowers,
Browser/tray integration, and native Windows/macOS behavior remain separate
parts of the complete daemon journey.

## Open the configured Browser

~~~text
graphit ui
graphit ui --no-open
graphit --global-dir /absolute/installation ui --no-open
~~~

These commands run outside PROJECT. They reuse the daemon launcher and its
Runtime-authenticated control to obtain the active Browser entry URL. The
installation must have explicit global `task.browser` configuration at daemon
startup, including its existing Broker providers and PROJECT endpoint. The
origin is the actual bound loopback address or the explicitly configured,
validated `task.browser.public_origin`. The existing Browser policy controls
whether loopback HTTP is permitted. A running headless daemon returns
`browser_not_configured` without changing providers or authorizations.

`--no-open` prints only the URL. The default uses the system desktop opener;
if that opener fails, the command reports failure with a URL to open manually.
The daemon remains active. Opening a URL grants no Web session or PROJECT
access: the Browser retains its ordinary login and authorization checks.
Linux validation exercised the actual Main client with concurrent `--no-open`
calls against a live Foreground owner, Runtime authorization, missing Browser,
a deterministic unavailable opener, and joined shutdown. The system opener
accepted one observed handoff; no visible window was qualified. Windows/macOS
and tray remain separate qualifications. See the
[OpenUI contract](../specs/rust_ui_mcp_tray_contract.md).

## Linux desktop tray

~~~text
graphit tray
graphit --global-dir /absolute/installation tray
~~~

The tray is a separate desktop process for this installation. It uses Linux
StatusNotifierItem/Dbusmenu through ksni with Tokio; it does not add GTK or
require the headless daemon to have a desktop session. A missing session bus
or StatusNotifier host produces a tray error.

The native menu offers Status, Start daemon, Restart daemon, Open interface,
Log in by configured Broker provider/profile, and Quit and stop daemon. Status
refreshes on menu opening and after actions. Start the daemon to load profile
choices; create new profiles through the existing login CLI. The menu never
creates a profile or chooses a new name. Reauthentication preserves the selected
profile's stored S3-disabled choice; the read-only account metadata exposes it
without exposing credentials. Open interface needs the same explicit
Browser configuration as `graphit ui`; it grants no Web session or PROJECT access.

Quit cancels and joins an active login, then requests the real daemon stop.
If stopping fails, the tray stays usable and displays the cause. On success it
withdraws the SNI service and joins owned work. SIGTERM/Ctrl-C closes only the
tray consumer, leaving the daemon available. No periodic status polling runs.

See the [tray contract](../specs/rust_ui_mcp_tray_contract.md).

## Per-user service operations

The typed entry accepts the following operations. Main dispatch and native
daemon lifecycle tests qualify the Linux paths described below. The native
Task Scheduler and launchd journeys remain pending:

```text
graphit daemon install
graphit daemon install --login
graphit daemon start
graphit daemon status
graphit daemon stop
graphit daemon restart
graphit daemon login enable
graphit daemon login disable
graphit daemon uninstall
```

Installation leaves login startup disabled unless `--login` is supplied.
`login enable` and `login disable` change that setting explicitly.
Uninstall stops the service and removes its registration and login registration.
The entry borrows the composed native service and a finite Original for each
operation; cancelling one command does not retain that request as the context
of later commands.

| Platform | Native manager |
|---|---|
| Linux | systemd user service |
| macOS | LaunchAgent in the current user's GUI domain |
| Windows | User Task Scheduler with InteractiveToken and LeastPrivilege |

Registration does not require an administrator service. Login startup is a
separate explicit setting. Definitions use the actual executable and argument
array with the exact absolute installation directory and WorkingDirectory;
they do not depend on a project CWD. The service invokes
`--global-dir <exact-path> daemon --managed`; Windows login invokes
`--global-dir <exact-path> daemon start`. Both must
use the same executable parser as manual operations.

Executable and installation paths must be absolute, Unicode and free of NUL.
Launchd/Task Scheduler documents reject emitted values outside XML 1.0 before
publishing the registration or invoking the manager. The existing quick-xml
serializer preserves carriage returns in XML paths as numeric references;
ordinary XML escaping and Windows argument quoting remain unchanged. The
[service contract](../specs/rust_daemon_services.md#serialização-e-execução)
describes the exact validation and its evidence boundaries.

Status first reads the authenticated daemon's PID/state, including a foreground
daemon without a registered service. If Source is absent, an available manager's
exit status and output are preserved; without a Linux user manager, status reports
absence. Authentication and protocol errors remain errors. An unsuccessful manager
observation is not silently reported as “absent”. Manager errors retain their
process outcome, cleanup state and remaining resources. Liveness of the Source
listener alone does not establish health of stores, providers or indexing.

## Manager errors and direct startup

Only a genuinely unavailable Linux user manager permits direct detached startup.
A reachable manager that rejects registration or startup returns its error;
that failure does not select another launcher. Explicit service operations
report manager failures. An eligible ordinary command ensures daemon readiness
after configuration, project selection and role guards, then maintains its real
PROJECT before its Source request. A global/context request does not invent a
PROJECT. Foreground startup is always explicit.

Start reuses an authenticated running daemon. With an available manager and no
running daemon, it registers the service only if absent and starts it. A new
registration leaves login disabled; an existing registration and its login
setting are preserved. Manager failure remains causal. Managed/foreground,
status/stop/retire, authentication/bootstrap/configuration, local operations and
role transport do not cause implicit startup.

For systemd, an installation path with CR/LF, trailing ASCII whitespace or an
odd number of trailing backslashes cannot be preserved by WorkingDirectory's
literal parser and is rejected before registration. Internal spaces, quotes,
backslashes, percent signs and Unicode remain literal.

Direct detached startup must run the same Rust foreground owner and retain its
singleton. It has no OS supervisor or implicit login startup. Stop informs the
manager first, preventing automatic relaunch, then stops and joins any retained
foreground or detached owner. Restart waits for release before starting again.
Binary or grammar replacement follows the same ordered release and explicit
current flags; it does not load an older native library.

These launch and remote-stop paths belong to the executable composition.
The typed service entry does not invent a controller, probe a process by PID,
or classify manager availability from arbitrary stderr.

## Diagnose and verify

Start with the command's causal error and native manager output. A missing,
malformed, changed or inaccessible discovery record is distinct from a live,
authenticated Source service. Never remove or replace a singleton lock inode to
force startup. Resolve the owner or cleanup failure and retry an explicit
operation with a fresh Original when required.

Now applies the request deadline while waiting for worker admission as well as
while waiting for its result. An expired queued request returns `now_deadline`
without starting a worker; cancellation and service shutdown retain their own
errors.

For a stalled lexical Knowledge read, enable
`RUST_LOG=graphit_app::source_server::knowledge_search=debug,graphit_data::knowledge_embeddings::search=debug`
on the Source process. The existing queue/worker markers and the static
`knowledge_pin`, `knowledge_manifest` and `knowledge_membership` spans distinguish
dataset opening, manifest capture and validation through EOF. These spans carry
no query, path or credential fields and remain disabled without the filter.

Tests use private installations, service names, locks, paths and listeners.
They never restart the personal daemon. The focused entry command is:

```text
cargo xtask cargo test --locked --offline -p graphit-app --test daemon_service_entry
```

Its Linux fixture uses the real systemctl utility with a private nonexistent
session bus. It covers preserved manager failure, cancellation before install
and a fresh subsequent Original. It does not prove successful managed startup,
detached launch, double-start, stale PID, stop/restart or replacement.

The compiled foreground fixture passed with the actual Rust executable,
singleton, double-start refusal, discovery after a killed publisher, restart and
SIGTERM/exit130 with joined shutdown and owned cleanup. Its isolated Clippy also
passed. This proof precedes the managed/global-directory composition and does
not qualify detached startup or authenticated remote stop.
The two focused Linux entry tests passed without ignored cases. Native
Windows/macOS execution remains required; cross-compilation and generated
XML/plist do not prove those operating-system behaviors. `tsk-0529` tracks the
productive journeys and native-platform checks.

The managed empty-inventory fixture and its Clippy passed. The new compiled CLI
cases exercise direct start/status/stop with a private installation, unavailable
bus or a real user manager, and an isolated service name. The real-manager case
also checks preservation of existing registration and login. Their execution is
recorded separately. Both cases passed. The real-manager case compares native
MainPID/ActiveState with the authenticated Start PID, verifies Stop releases
discovery/singleton, and preserves registration bytes/mtime and login when
starting again. Uninstall removes the uniquely named registration.
The native systemd adapter fixture passed after the WorkingDirectory correction,
including literal argv, real CWD measured by the executable, restart, login and
uninstall. Its cleanup guard retains the private unit/installation if shutdown
is uncertain. These cases do not operate on the personal daemon.

Final affected SDK and entry-fixture Clippy checks passed with warnings denied;
fmt and diff checks also passed. Those early checks preceded the Linux replacement
qualification below; native Windows/macOS remain pending. During a native-fixture recovery, failed
stop/disable retains its unit/root. If reload fails after registration removal,
the root is retained and that removal remains a known effect.

A new PID and a successful restart command alone do not establish joined shutdown. Stop must drain the old owner, including incomplete HTTP requests.

The final `strict26` qualification also passed for `graphit-app --lib --tests`
with warnings denied (61.491s). These existing Linux results are reused without
another runtime campaign. The service fixture is Linux-only: a run on another OS
does not qualify its native manager. Windows still needs a private Task Scheduler
journey and macOS a private per-user launchd journey, each using the real Rust
CLI for install, start/status, restart/replacement, stop/drain, login and uninstall.
Observe authenticated PID, exact installation arguments and released singleton;
preserve causal failures and unfinished cleanup. These checks must use isolated
names/paths and must not change personal services. They remain pending while
their respective hosts are unavailable.

See [Rust per-user services](../specs/rust_daemon_services.md) for the adapter
contract, [Source discovery](../specs/rust_source_discovery.md) for publication
and owned cleanup, and [daemon lifecycle](../specs/rust_daemon_memory_lifecycle.md)
for project isolation and retirement. Observatory, tray, model transport and
module jobs retain their separate owners and integration checks.
