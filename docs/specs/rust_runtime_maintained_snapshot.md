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

The reader borrows the service's actual installation capture and checks the
original cancellation and optional absolute deadline. Its `ScopeContext` uses
Core `ScopeContext::from_snapshot` with that same deadline and cancellation
signal. A service Original preserves `None`; no deadline is synthesized. It calls Core's `revalidate_installation`
and the bounded recorded-binding comparison before resolution, after resolution,
and before delivery. The physical check covers the installation owner/runtime
roots and their ancestors, keeping the original receipts and poison checks.
An unrelated captured checkout can disappear without invalidating this namespace;
its observations remain retained for complete project/provider validation.
A false recorded comparison is `InvalidData`;
it is a namespace mismatch, not an authorization decision.

`InstallationPaths::resolve` performs read-only canonical filesystem work from
the live brand and owner root, without a home directory or personal settings.
The resolved start, root, brand, and runtime path must match the captured
namespace exactly. This prevents trimming or alias resolution from silently
redirecting the read. The final physical check is required even for an empty
inventory: Core's missing-root behavior cannot turn a disappeared or replaced
live installation into a successful empty response.

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
Core or the context/binding checks: cancellation, timeout, malformed or invalid
schema/data, duplicate entries, wrong brand/type, links, file type, private
permissions, byte overflow/oversize, missing/replaced physical namespace, and
scope contention/poison failures. There is no empty error fallback.
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
Core bootstrap/capture/register/retain writers and actual filesystem failures,
all confined to temporary directories. They verify A/B installation isolation,
independent projects, default/named/old versions, read-after-write, absence of
historical checkout reopening, missing-document read-only behavior, original
cancel/expiry, startup-context independence, moved/replaced roots, document
bounds, malformed/schema/type/duplicate inputs, links and permissions.

Current Main qualification with Core's four-argument `RuntimeKeyLimits` passes:
eight focused tests, one compiled API example, owned formatting/whitespace,
relative document links, and strict daemon all-targets Clippy. Receipts belong to
`.build/rust-runtime-maintained-inventory-20261011/`: `main-tests-core4.log`,
`main-doc-core4.log`, `main-fmt-core4.log`, `main-clippy-core4.log`, and final
hash/readback packets. Earlier five-argument baseline receipts and the temporal
ABI conflict remain historical evidence.

The executed Cargo targets are `cargo test --locked --offline -j1
-p graphit-daemon --lib maintained_inventory::tests`, `cargo test --locked
--offline -j1 -p graphit-daemon --doc maintained_inventory`, and `cargo clippy
--locked --offline -j1 -p graphit-daemon --all-targets -- -D warnings`, using the
coordinator's existing warm target with development/test debug and incremental
compilation disabled. No Cargo manifest or lockfile change was needed.

The installation-only consumer correction in `tsk-4ca0` adds a regression with a
captured checkout that disappears while a sibling remains. It checks complete
historical facts, explicit Core checkout removal with versions preserved, and the
continued failure of full scope validation for that checkout. The current focused
snapshot run passed all nine tests. The seven discovery tests, seven pure Broker
tests, two public native Broker journeys and four daemon doctests also passed;
strict daemon library/test Clippy, scoped formatting and diff checks passed.
Receipts are in `.build/rust-runtime-installation-consumers-20261011/`. An earlier
discovery compilation failure caused by a concurrently declared AI module without
its source is preserved separately; the real producer source was present before
the successful rerun.

These are Linux library checks. They do not establish installed CLI/App/daemon,
model, agent-host, Kiro V2/V3, Windows/macOS, Now/remote transport, retirement, or
whole Runtime migration behavior. Current host contracts target Kiro V3; previous V2/preview requirements are superseded and do not impose vendor account/install gates.
