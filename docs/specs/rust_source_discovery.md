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

Call `publish_discovery(auth, listener, max_bytes, original)` after initializing
the Source server and binding its Core listener. Publication derives the PID
from the process. `local_address(listener)` retains a loopback listener address
or maps an explicit IPv4/IPv6 wildcard listener to the matching loopback family
at the same port. Other nonloopback listeners fail. The retained runtime owner
checks its real key and Original, and supplies the configured root. The writer
creates `daemon/` as needed and uses `local_files::atomic_replace`. On Unix new
records use `0600`. The record contains no credential. The service's permanent
singleton lock is neither replaced nor removed.

`read_discovery(installation, max_bytes, original)` reads the configured record
with normal filesystem resolution. A missing directory/document returns `None`
without creation. A positive byte limit applies to the actual JSON. Links and
other volumes use ordinary OS semantics; malformed content, wrong kinds, access
errors, cancellation and expiry remain errors. `DiscoveryError` retains causal
I/O, runtime and JSON errors while its own Display/Debug omit document content.

There is no captured directory, ancestor/physical identity, observation receipt,
revalidation phase or recovery state. The discovery record grants no authority.

## Shutdown

Stop admissions and join the service's actual workers first. Then call
`remove_discovery(auth, expected, max_bytes, original)` with the record returned by
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

The suite covers readonly absence, actual PID/loopback publication, JSON limits
and malformed records, cancellation/expiry, replacement preservation, explicit
cleanup, retained runtime keys and singleton ownership. Native platform execution
must be reported separately; Linux checks alone do not qualify Windows/macOS.
