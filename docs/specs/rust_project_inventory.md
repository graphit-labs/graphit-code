---
title: Rust maintained project inventory
description: Local project and version facts consumed independently of online presence.
status: implemented-main-qualified
references:
  - target: {type: task, id: tsk-f48f}
    relation: supports
  - target: {type: task, id: tsk-a437}
    relation: supports
  - target: {type: task, id: tsk-b91e}
    relation: relates_to
  - target: {type: task, id: tsk-0137}
    relation: supports
  - target: {type: session, id: ses-c98a}
    relation: relates_to
---

# Maintained project inventory

`graphit_core::project_inventory` records locally maintained projects and versions.
The installation file is `project-inventory.json`; its stable coordination file is
`.project-inventory.lock`. This is a new Rust format. No Go lockfile, old layout,
import, converter or compatibility alias is read.

The document also records its captured brand; reading it through another brand
bound to the same installation path fails.

A project uses its current canonical, nonzero `ProjectId` and validated
`ProjectIdentity`, including discovery metadata. Its shared `project.json` keeps
metadata at the root alongside `settings` and `githooks`; Config/metadata edits
preserve the ID. A rename does not rekey it or rebuild inventory/stores.
Ephemeral projects cannot be registered. Checkouts use captured canonical paths
and the `NativePath` JSON string representation. Non-Unicode native paths are
rejected; selectors retain exact Unicode spelling and grant no authority.

Each checkout records both its physical Git state and its selected version.
`GitCheckout` is produced by the existing system-git inspection; the registry
does not enumerate refs or invoke Git implicitly. A pinned/explicit selection may
differ from the branch. Detached and non-Git observations remain distinguishable.
The default and selection source are captured with `VersionContext`.

Versions are maintained facts, supplied by checkout registration and the actual
AST, Knowledge or Records producers. Registering a checkout retains its selected
version. Switching selects the new version and preserves the previous versions.
A producer calls `retain_version` after real creation/maintenance and
`remove_version` after it has confirmed removal of data and use. A version still
selected by a registered checkout cannot be removed. Removing a checkout preserves
versions; the empty project entry disappears only after both its checkouts and
versions have been removed. These operations never delete project data.

Parking, an offline process and closing the daemon do not remove maintained
inventory. A snapshot supplies no online status, heartbeat, lease, remote ACL or
authorization. Installation IDs and secrets are absent from this document.
Registration and realtime Now are separate consumers and still require their
actual local and Broker contracts.

## Boundaries and operations

`GlobalProjectInventory::at(&InstallationPaths, max_document_bytes)` binds explicit
brand/root data and a positive document byte limit. Constructing or loading the
store creates no root, lock or identity. Exact absence returns an empty snapshot.
Bad JSON, schema, unknown/duplicate fields, duplicate project IDs/checkouts/versions,
noncanonical identities/metadata/versions, links, wrong file types, permission
failures and oversized files fail instead of becoming absence. The bounded reader
keeps its existing `InvalidData` error for a file exceeding the read limit.

`CapturedProjectInventory::capture` consumes `ProjectPaths`, `GitCheckout`,
`VersionContext`, existing scope/native limits and the original request Arc.
It loads an existing identity without ensuring one, checks its persistence
eligibility and the selection's brand/root, then reuses the existing PROJECT
capture of the held root. It does not manufacture a guard or expose a descriptor.
Mutation revalidates the original request, directory chain and captured identity;
a changed identity/metadata requires a fresh capture.

A consumer that already has an admitted PROJECT scope uses
`CapturedProjectInventory::capture_in_scope(project, checkout, selection, scope,
native_limits, request)`. It passes that original live scope and the same request
Arc supplied by its admission. The scope captures a namespace and physical
handles; it does not itself retain a request. The captured inventory retains the
provided request through B1 and its writer checks that same request. Consumers
pass the live scope rather than a reconstructed binding record or a scope derived from a fresh
identity load. Core checks PROJECT ownership, the canonical root and brand,
captures the held root through that scope, and compares the identity loaded
inside this flow with the scope's `ProjectId`. It shares the same checkout
finalization and full identity revalidation as bootstrap `capture`.

An unexpected loaded ID or an observed identity change fails with `InvalidData`.
A different root/brand/selection or installation owner fails with `InvalidInput`;
a writer using a foreign request after this capture also fails with `InvalidInput`; missing identity remains `NotFound`, and ephemeral identity is
ineligible. Original cancellation/deadline and filesystem/decoder failures retain
their causes. This comparison covers the observed capture and writer flow; it
does not establish a history of all identity changes or lock another dataset.


`writer` consumes an already prepared `PrivateInstallation` for the same root,
brand and request. It does not create a missing parent while acquiring the stable
kernel `FileLock`. The writer is short-lived and releases its lock through RAII.
The request's existing deadline bounds waiting and its original cancellation is
preserved. There is no deadline renewal or long-lived request in a snapshot.

`register_checkout`, `retain_version`, `remove_checkout` and `remove_version`
reread the current file under that lock. Two processes serialize mutations rather
than replacing each other's snapshots. Missing removals and equivalent updates
return false. A checkout path already registered for another project cannot be
rekeyed by registration.

Read snapshots expose borrowed `projects()`, `get(ProjectId)`,
`identity()`, `versions()` and `checkouts()`; checkout accessors expose path,
selected/default versions, source and physical state. Deserializing pure historical
data restores no request, handle or mutation capability. Reading does not reopen
every recorded checkout or remove historical entries because directories are
currently unavailable. Consumers capture and validate their own resources before
using them.

The same-directory `atomic_replace_with` helper publishes complete old/new
documents with the existing tempfile SDK, file sync and cancellation checks.
Unix files use mode 0600 and existing private-root qualification is reused.
The byte limit bounds serialized output and decoded input. Mutations use a single
loaded snapshot; JSON is streamed directly into the temporary, without cloning
the whole registry or allocating an additional JSON buffer. There is no I/O/RAM
reservation, contribution ledger, operation counter, resource meter, global cache
or invented revision/stream protocol.

Cooperative checks detect observed identity and namespace changes. They are not
a hostile-filesystem guarantee. Synchronous filesystem calls have no hard deadline.
Atomic visibility does not promise directory fsync, power-loss durability, an
atomic transaction with project datasets or abrupt-process temporary cleanup.

## Consumer shape

```rust
use graphit_core::{project_inventory::ProjectInventorySnapshot,
                   project_identity::ProjectId};
fn maintained(snapshot: &ProjectInventorySnapshot, id: ProjectId) -> Vec<&str> {
    snapshot.get(id).map(|p| p.versions().collect()).unwrap_or_default()
}
```

The Core producer is independent of the Broker. Runtime `tsk-0137` may consume
its qualified snapshots and separately resolve local instance identity. A registry
test is not evidence of native daemon/agent/Broker integration.

Runtime's Auth-bound consumer obtains `ProjectCapture.scope.live`, its
`VersionContext` and original request after the admission's recheck, then supplies
them to `capture_in_scope`. Core imports no Auth types and exposes no identity
getter or new grant. A selected logical version may differ from the observed
branch; this API introduces no physical Source fallback matching. If a confirmed
Knowledge publication is followed by an inventory failure, the Runtime consumer
must preserve the confirmed publication or `PartialDefault` result and report the
actual inventory cause. There is no transaction across those stores.


## Qualification

The original-scope extension `tsk-f48f` is integrated and qualified in Main on
Linux: 22 functional inventory tests passed (14 existing and eight new scoped
regressions), with one child helper exercised by its parent. Both public doctests,
strict Core all-targets Clippy and formatting passed. The new cases cover
encapsulated A/B identity, scope root/brand/owner and selection, original request
retention in the writer, cancellation/expiry, missing/ephemeral/corrupt identity
and failure preservation. The initial 21-pass/one-failure log is retained: it
exposed an incorrect expectation that scope alone binds a request. The fixture and
contract now use Auth's supplied original and the capture's retained request.
Logs and the three-path delta are in
`.build/rust-core-inventory-scope-binding-20261011`. Runtime Auth/Data integration
and Windows/macOS native execution are separate, unexecuted consumers here.
The results below describe the earlier N5 producer.


The Main Linux gate passed 14 functional tests, including real Git/worktrees,
independent process writers, two projects, original request/cancellation/deadline,
metadata/identity/matching-brand conflicts, Unicode paths and explicit rejection
of non-Unicode inputs, with failure preservation.
One ignored child helper is executed by its parent process fixture and is not
counted again. The public doctest and strict Core all-targets Clippy passed, with
formatting verified. The five owned paths are integrated in Main; the same focal
gate had already passed in the private stage. Logs and the exact source packet
are retained in `.build/rust-core-project-inventory-n5-20261011`.

The initial nine-pass/one-failure run exposed an incorrect expectation of the
existing bounded reader error, corrected without changing the reader. Its failed
log is retained. Windows/macOS native registry execution and remote Broker/daemon
consumer integration remain unexecuted.
