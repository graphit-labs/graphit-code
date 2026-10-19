---
title: Rust Source service discovery
doc_type: technical
references:
  - target: {type: task, id: tsk-4ca0}
    relation: implements
  - target: {type: task, id: tsk-9faa}
    relation: implements
  - target: {type: task, id: tsk-7fb5}
    relation: derived_from
  - target: {type: task, id: tsk-2436}
    relation: supports
---

# Source service discovery

`graphit_daemon::source_discovery` publishes and reads the current installation's
Source listener address. It borrows the actual [DaemonAuth service owner](rust_daemon_auth_startup.md)
and Core listener; it does not start an HTTP server or implement CLI discovery
dispatch. The current Rust record is `daemon/mcp.json` beneath the selected
installation root:

```json
{"pid":1234,"address":"127.0.0.1:32100"}
```

Only `pid` and `address` are accepted. Duplicate or unknown fields, malformed
JSON, PID zero, port zero and nonloopback addresses fail. The record contains no
credential. Its presence is address data, not proof of a live process or an
authentication grant. A caller must still connect and authenticate with the
current issuer. This component neither probes nor kills a process from its PID.

## Publish and observe

Call `publish_discovery(auth, listener, limits, original)` after initializing the
actual Source server and binding its Core `BoundListener`. Publication derives
the PID from the current process and the address from that retained listener.
The caller retains the listener and the installation's native singleton guard.
The operation checks the live runtime key, installation scope and directory
through existing Core APIs before writing with `local_files::atomic_replace`.
The installation check uses `revalidate_installation`: owner/runtime roots,
their ancestors and recorded binding must remain valid under the Original.
An unrelated captured checkout may disappear without blocking publication or
owned-record cleanup. Its receipts remain available for full project/provider
validation; discovery does not weaken those operations.
On Unix the published file mode is `0600`; elsewhere the existing private parent
provides the applicable privacy boundary. No credential directory is created by
this module. Root-level `daemon.pid` remains the permanent kernel lock inode and
is never replaced, reacquired or removed by discovery.

`read_discovery(installation, limits, original)` observes the selected
installation's record through `CapturedDocumentDirectory`. A missing directory
or document returns `None` without creating any file, installation, key or
service owner. The reader deserializes and validates the document, then
revalidates the actual observation with the same original snapshot before
returning it. Access failures, wrong file kinds, symlinks, changed observations,
invalid documents, cancellation and deadlines remain errors. None of these
errors selects another installation or turns into missing data.

`NativeDocumentFileLimits` are the existing Core read and path limits. The record
has two fixed scalar fields; there is no corpus inventory or retained connection
in this module. `DiscoveryError` retains filesystem, runtime and JSON causes via
`Error::source`. Its own Display and Debug describe only static categories and
do not echo malformed document content.

## Shutdown

Stop admissions and join the service's actual workers first. Then call
`remove_discovery(auth, expected, limits, original)` with the record returned by
publication, while the same owner and singleton remain alive. A matching record
is removed and returns `true`; absence or a different valid record returns
`false`, preserving that record. Invalid documents or SDK errors remain errors.
This call does not remove the runtime key or release the singleton.

After successful discovery cleanup, use the owner's explicit `remove_runtime`
operation and inspect its actual outcome before dropping the owner. Cancelled
cleanup requires a fresh operation and retains the service resources for retry.
There is no filesystem cleanup in discovery Drop. Ordinary publication uses the
existing atomic replacement helper under the service guard; neither publication
nor cleanup promises continuous confinement against hostile concurrent path
mutation or power-loss durability of parent directories.

## Validation scope

`crates/graphit-daemon/tests/source_discovery.rs` uses real temporary Core
installations, current `DaemonAuth`, native locks and bound listeners. The focused
commands are:

```text
cargo test --locked --offline -p graphit-daemon --test source_discovery -j1
cargo clippy --locked --offline -p graphit-daemon --lib --test source_discovery -j1 -- -D warnings
```

The six native integration tests and the affected strict Clippy pass on Linux.
Fixtures cover readonly absence, publication fields and file privacy, malformed
and inaccessible records, cancellation and deadlines, replacements, explicit
cleanup, preserved live keys and singleton ownership, and restart. Execution
qualification is recorded with task `tsk-9faa`; the native Unix fixtures target
Linux. This component does not qualify Windows/macOS execution, endpoint RPC,
compiled CLI dispatch, full daemon shutdown or measured memory savings.

The focused `tsk-4ca0` regression additionally removes a captured checkout,
checks that full scope validation still fails, then removes and republishes the
real owner's discovery record. It preserves a different listener's record,
checks absence, the retained runtime key and singleton, and performs explicit
key cleanup. All seven current discovery tests passed, including this regression.
The nine snapshot tests, seven pure Broker tests, two public native Broker
journeys, four daemon doctests and strict daemon library/test Clippy passed too.
Scoped formatting, diff and authored relative links passed. Current receipts and
the earlier incomplete-AI-module compilation failure are preserved in
`.build/rust-runtime-installation-consumers-20261011/`.
