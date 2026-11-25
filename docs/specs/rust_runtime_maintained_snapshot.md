---
title: Rust Runtime maintained inventory snapshot
doc_type: technical
status: source-main-qualified
references:
  - target: {type: task, id: tsk-cab0}
    relation: supports
  - target: {type: task, id: tsk-9d34}
    relation: corrects
  - target: {type: task, id: tsk-bcf2}
    relation: uses_api
  - target: {type: task, id: tsk-4ca0}
    relation: implements
  - target: {type: task, id: tsk-a51f}
    relation: implements
  - target: {type: task, id: tsk-2436}
    relation: uses
  - target: {type: task, id: tsk-a437}
    relation: uses
  - target: {type: task, id: tsk-a5fa}
    relation: relates_to
---

# Maintained inventory for a live installation

The daemon library exposes `maintained_inventory::read_maintained_inventory` on Linux,
macOS and Windows through the same Core and Auth APIs.
It consumes the running `DaemonAuth`, the incoming operation's original
`SnapshotContext`, and a positive document byte bound. It returns owned
`MaintainedInventorySnapshot { installation_id, projects }` data. Each project
contains only `project_id` and `versions`, in the actual Core inventory order.

```rust
use graphit_daemon::maintained_inventory::read_maintained_inventory;
let snapshot = read_maintained_inventory(service, original, max_document_bytes)?;
```

The installation ID comes from the live scope captured by
[daemon authentication startup](rust_daemon_auth_startup.md). The reader does
not reload the installation identity document and does not attest that the
current disk metadata still carries that ID. Projects and maintained versions
come from [Core's local inventory](rust_project_inventory.md). Versions are
historical facts; they do not select a checkout or establish activity, online
presence, ACL authorization, contribution provenance, or a runtime lease.

The result contains no paths, project metadata, credentials, captures, file
handles, contexts, current checkout selection, activity counts, or Broker wire
fields. The reader never opens historical project paths, inspects Git, acquires
an inventory writer, creates state, or caches the inventory. A new read sees
the current Core document. Removing a historical checkout from disk does not
erase its maintained facts.

## Namespace and operation lifetime

The reader uses the live service's logical installation ID, brand and configured
owner root. It checks the same Original before loading Core inventory and before
delivery, preserving its cancellation and optional absolute deadline. No deadline
is synthesized.

`InstallationPaths::resolve` resolves the configured path for each operation,
without a home directory or personal settings. Resolution and inventory I/O
errors propagate. A missing installation root can fail resolution with NotFound;
an existing replacement directory with no inventory yields empty projects.
Configured links use ordinary OS resolution. Historical checkout paths are not
opened by this reader.

The full Core snapshot and temporary paths are dropped before delivery. The
owned result retains no daemon resources. A fresh operation is independent of
a cancelled or expired startup request. Service teardown continues to require
the caller's real stop/join and `remove_runtime` lifecycle; dropping this data
does not remove a runtime key or establish quiescence.

Synchronous filesystem calls have cooperative checks. They do not provide hard
preemption or an atomic view against hostile concurrent filesystem changes.
The reader does not extend a deadline or create a replacement grant/request.

## Empty results and errors

Exact absence of the inventory document or a valid document with `projects: []`
returns empty projects under a valid live installation. This read does not create
an inventory, lock, identity, or configuration file. All errors remain actual `io::Error` results from
Core or path resolution: cancellation, timeout, malformed or invalid schema/data,
duplicate entries, wrong brand/type, file type, access failures and byte
overflow/oversize. There is no empty error fallback.
An inventory file exceeding the read bound retains Core's `InvalidData` cause.

The byte bound is validated by the actual Core constructor and loader. No new
project/version quota, ledger, reservation, wire DTO, or authority producer is
introduced by this reader.

## Qualification boundary

The producer is available in source on all three supported systems. The existing
filesystem fixtures remain Linux qualifications; enabling the producer does not
claim native Windows or macOS execution.

Source: `crates/graphit-daemon/src/maintained_inventory.rs` and its `tests.rs`.
The fixtures use real `DaemonAuth::start`, persisted installation/project IDs,
Core observation/register/retain writers and actual filesystem failures,
all confined to temporary directories. They verify A/B installation isolation,
independent projects, default/named/old versions, read-after-write, absence of
historical checkout reopening, missing-document read-only behavior, original
cancel/expiry, startup-context independence, moved/replaced roots, document
bounds, malformed/schema/type/duplicate inputs, links and permissions.

Earlier capture-protocol qualifications are historical and do not qualify this
ordinary-path reader. The directory replacement fixture restores the original
installation before asserting observations and tearing down its runtime owner;
an expected read failure must not cause a second cleanup panic.

These are Linux library checks. They do not establish installed CLI/App/daemon,
model, agent-host, Kiro V2/V3, Windows/macOS, Now/remote transport, retirement, or
whole Runtime migration behavior. Current host contracts target Kiro V3; previous V2/preview requirements are superseded and do not impose vendor account/install gates.
