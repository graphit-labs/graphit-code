---
title: Pinned local Lance SDK clone
content-type: reference
audience: developers
status: library-qualified-consumer-pending
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-c453}
    relation: implemented_by
  - target: {type: task, id: tsk-a380}
    relation: derived_from
  - target: {type: task, id: tsk-cc2d}
    relation: uses
  - target: {type: task, id: tsk-ed33}
    relation: supports
  - target: {type: task, id: tsk-b913}
    relation: relates_to
  - target: {type: task, id: tsk-8cc0}
    relation: relates_to
---

# Pinned local Lance SDK clone

`ModuleSnapshot::spawn_native_clone_local(stage: TempDir, original: Arc<SnapshotContext>)`
returns `io::Result<JoinHandle<io::Result<NativeModuleClone>>>`. It stages the retained
physical SDK version through Lance 14.0.2 `Dataset::deep_clone`; it does not select the
latest version or infer a storage version from an artifact or Git version.

The supported input is a retained SDK dataset with direct local primary storage,
no Blob fields. Bases actually referenced by Lance data/overlay files, deletion
files or raw index metadata must be explicit absolute local paths or file URIs for
the Main root of that same captured SDK table. Foreign, relative, remote, data-only
and other-branch references are unsupported. Unreferenced entries retained in the
SDK manifest after an overwrite are not opened or resolved: they contribute no
files to the SDK deep clone. This is a layout check, not
source admission. The [branch bridge](lance_native_branch_clone.md) uses this same
worker for actual Knowledge branch captures. The supplied directory must be a fresh,
empty `TempDir` in a parent the caller has already admitted. Paths, a temporary
directory and SDK metadata do not grant access. The caller retains its source
capture, selected local SDK session and Original through the operation.

The SDK owns fragment, deletion, row-id, index and manifest handling. This method
does not enumerate a checkout, scan history or construct a complete object-copy
list. It supplies the owned directory as a native local path and uses the SDK's
normal local destination configuration. The SDK generates the clone's new
manifest and transaction; the output's actual physical version and location come
from that result and can differ from the input's version.

A standard Tokio `spawn_blocking` worker owns both the retained dataset and the
staging directory. Inside it, the captured runtime runs the SDK future. The caller
must retain the returned `JoinHandle`, join it during shutdown and before cleanup
or publication, and inspect both its `JoinError` and inner I/O result. Dropping a
waiting future cannot remove the worker's directory while the SDK is still using
it. Dropping the standard handle detaches it, so caller shutdown must keep it.

Original is checked before spawning, before SDK copying and after the SDK
completes. A started SDK local copy uses synchronous filesystem I/O and cannot be
hard-aborted. Cancellation and deadlines are cooperative at these boundaries;
they are not promises of an exact stop time. An error refuses the output, and
worker-owned `TempDir` cleanup happens after the SDK future completes. SDK failures
remain the source of `io::Error::other`; a nonempty/non-directory stage or unusable
file URL is `InvalidInput`, while filesystem failures preserve their I/O kinds.
Unsupported storage/schema is `Unsupported`, and Original retains `Interrupted`
or `TimedOut`.

`NativeModuleClone` owns the private SDK snapshot and directory. `directory()`
borrows its path; `native_metadata(&SnapshotContext)` delegates to the existing
[pinned metadata view](lance_native_metadata.md), borrowing this owner. No raw
snapshot is returned that could escape the directory owner. Drop releases the SDK
owner before temporary-directory cleanup. As with normal `TempDir`, destructor
cleanup is best effort.

This local staging input does not provide immutable S3 publication, conditional
catalog updates or authorization. The actual publisher `tsk-ed33` still requires
fresh HubWrite/Broker STS and genuine source captures. The actual named-branch
input is implemented by `tsk-b913`, using the
[official SDK and native paths](../development/lance-upstream.md).
The earlier universal Blob/tagged-FRI envelope was superseded after current
Graphit writers and schemas were inspected; no active producer was found for
those generic features. Blob remains outside Graphit's export contract; the
official SDK's tagged-FRI support is not independently qualified here. A local clone does not qualify remote publication,
Broker STS or the compiled publisher CLI.

Implementation: [lance_module_store.rs](../../crates/graphit-data/src/lance_module_store.rs).
Real SDK fixtures: [lance_native_clone.rs](../../crates/graphit-data/tests/lance_native_clone.rs).
The [snapshot opener](lance_native_snapshot.md) and [native root](remote_native_root.md)
keep physical data separate from admission and published integrity.

Primary library contracts: [Lance Dataset](https://docs.rs/lance/14.0.2/lance/dataset/struct.Dataset.html),
[Tokio spawn_blocking](https://docs.rs/tokio/1.48.0/tokio/task/fn.spawn_blocking.html)
and [TempDir](https://docs.rs/tempfile/3.27.0/tempfile/struct.TempDir.html).

Historical Lance 13 qualification passed five real SDK fixtures and eleven affected
regressions (module store six, pinned metadata three, snapshot opener two). They
exercise stable row IDs, selected versions, deletion and BTree metadata, strict
output schema/rows, source preservation, owned-directory cleanup, actual shallow
base and Blob rejection, missing referenced-file SDK causes and Original/runtime
boundaries. The queued worker fixture retains its directory until the caller
unblocks and joins it; it does not claim hard cancellation during synchronous I/O.

`cargo test --locked --offline -p graphit-data --test lance_native_clone` passed.
Scoped Clippy for the Data library and clone/metadata targets passed with
`--no-deps -- -D warnings`; scoped rustfmt, whitespace, frontmatter and five local
links passed. These receipts qualify this local SDK input, while the mandatory
full closure, actual publisher/compiled CLI and Broker STS/write remain pending.
