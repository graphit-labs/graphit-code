---
title: Pinned Lance native metadata
content-type: reference
audience: developers
status: library-qualified-consumer-pending
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-9199}
    relation: implemented_by
  - target: {type: task, id: tsk-cc2d}
    relation: implemented_by
  - target: {type: task, id: tsk-eb0d}
    relation: derived_from
  - target: {type: task, id: tsk-265f}
    relation: uses
  - target: {type: task, id: tsk-53c6}
    relation: relates_to
  - target: {type: task, id: tsk-ed33}
    relation: supports
  - target: {type: task, id: tsk-0688}
    relation: supports
---

# Pinned Lance native metadata

`ModuleSnapshot::native_metadata(&SnapshotContext)` returns a readonly
`NativeModuleMetadata` borrowing the actual retained SDK dataset. Its `uri`,
`manifest` and `manifest_location` getters expose data from that version, while
`indices` exposes the raw physical index section read by the Lance SDK from the
same manifest location. It does not reopen the latest URI or list dataset history.
The snapshot owner must remain alive while the view is borrowed.

The SDK manifest includes fragments, overlay data files, deletion and row-id
metadata, external base paths and transaction references. Raw index metadata
preserves entries that the query planner may hide or transform. The implementation
uses the public SDK `read_manifest_indexes` with the retained primary object store;
it does not parse or rewrite SDK manifests.

The same Original is checked before the SDK read and before delivery. Cancellation
and its optional deadline race the readonly wait. Ordinary `Some(Instant)` retains
its absolute deadline; a `SnapshotContext::service` with `None` waits without an
artificial timer and still cancels. The same Original is checked after the SDK
future resolves, so success cannot hide cancellation. Interrupted and TimedOut
retain their causes; SDK failures remain the source of an `io::Error::other`.
The returned data is not a live authorization lease. The consumer continues to
authenticate and revalidate its actual capture and authorization around protected
payload or remote operations and final delivery.

This view is not a complete object-copy plan. Index file lists can be absent, blob
sidecars may require SDK discovery, and external bases need independently admitted
access. Versions, URI, paths and metadata give no read or write permission. The
actual publisher in `tsk-ed33` still needs a qualified SDK transfer and the real
Broker STS/write producer tracked by `tsk-8cc0`; metadata qualification does not
publish a native corpus or qualify its compiled CLI.

`tsk-ed33` replaces the unimplemented caller `tsk-53c6`, preserving its publication
and STS requirements while separating the caller from its refinement parent.

The [snapshot opener](lance_native_snapshot.md) retains the dataset owner and exact
SDK version. The [native root contract](remote_native_root.md) records published
manifest integrity separately from admission. Source is
[lance_module_store.rs](../../crates/graphit-data/src/lance_module_store.rs), with
real SDK fixtures in
[lance_native_metadata.rs](../../crates/graphit-data/tests/lance_native_metadata.rs).

Focused Linux qualification passed three metadata fixtures. The fixtures create three SDK versions,
including a real BTree index, verify distinct pinned physical metadata, preserve
raw index data after writer release, and check Original cancellation/deadline and
the SDK cause of a deleted index-bearing manifest.

The earlier Linux qualification passed all three metadata tests. The current
focused command is `cargo xtask cargo test --locked --offline -p graphit-data --test integration -- lance_native_metadata::`.
The existing snapshot opener and module store suites
passed eight regressions. Scoped Clippy for the library, metadata target and
snapshot opener passed with `--no-deps -- -D warnings`; scoped rustfmt, whitespace,
frontmatter and four local documentation links also passed. This qualifies the
SDK metadata input, while the actual publisher, compiled CLI and Broker STS/write
integration remain necessary pending deliveries.
