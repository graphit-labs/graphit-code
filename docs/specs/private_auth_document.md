---
title: Private installation authentication document capture
references:
  - target: {type: task, id: tsk-00c5}
    relation: documents
  - target: {type: task, id: tsk-2706}
    relation: documents
  - target: {type: task, id: tsk-1750}
    relation: documents
  - target: {type: task, id: tsk-8c09}
    relation: implements
  - target: {type: task, id: tsk-a22e}
    relation: implements
  - target: {type: task, id: tsk-effe}
    relation: implements
  - target: {type: task, id: tsk-e849}
    relation: derived_from
  - target: {type: task, id: tsk-3490}
    relation: implements
  - target: {type: task, id: tsk-e911}
    relation: derived_from
---

# Private installation authentication document

Core stores the configured installation directory as a `PathBuf` and accesses
`auth.json` with conventional filesystem operations. Configured directories may
be symlinks, junctions or reside on another volume. There is no captured parent,
ancestor chain, depth/path budget, physical receipt, directory revalidation or
filesystem-derived authority. Auth supplies the selected path and checks actual
authorization and profile semantics.

`AuthDocumentStore::new(root, AuthDocumentLimits::new(bytes)?, original)` checks
current credential-directory privacy without creating it or repairing access.
Only the actual document byte bound remains. `read(original)` returns current
private bytes in a zeroizing snapshot; absence returns no bytes. File privacy is
checked on the real file before reading. Newly created temporary files are
private and verified by the native FS creation API. A private credential may have
more than one filesystem link. Snapshot data is not a grant and is not
bound to historical directory or file identities.

## Short content CAS

`lock_latest(original)` holds the real publisher `FileLock` while reading current
bytes. Its `AuthDocumentMutation` exposes `bytes()` and consuming `publish(body)`.
Auth decodes the current document, verifies the original target's provider,
profile and token expectations, and encodes its mutation under this short lock.
This preserves independent profiles' changes without treating old whole-document
metadata as authority. No global publisher lock spans HTTP or async waiting.

`publish(expected, body, original)` also supports an explicit expected-content
snapshot. Changed current bytes return `Conflict`; replacing a file with equal
bytes does not create an identity conflict. Encoding/schema checks remain in
Auth. Core checks cancellation, deadline, current privacy and the actual content
bound. A private `tempfile` in the destination directory is written and synced,
then the current bytes are compared once more before publication. Data handles
close first, then `tempfile::TempPath::persist` atomically publishes the private
stage on the same filesystem. Successful reads contain a complete document;
captured snapshots remain immutable. New I/O concurrent with publication may
fail with a native access or sharing error. There is
no fixed stage slot, journal, backup, receipt or retained directory guarantee.

The result is `Result<AuthPublicationEffect, AuthDocumentError>` with `Applied`
or `Unchanged` on success. Native errors propagate through the existing error
mapping without retry, a second publication attempt, or substitution of stale or
empty data. Core adds no fallible post-commit cleanup protocol. Temporary-file
cleanup follows the library's ordinary lifecycle. A derived/reproducible operation is retried only
by explicit reexecution, without a recovery state machine or cleanup token.
Dropping an uncommitted mutation releases the lock without publishing.

## Profile refresh serialization

Auth derives a stable `.auth-refresh-{digest}.lock` basename from the validated
profile name. `try_profile_refresh_lock(basename, original)` returns a real
`FileLock` or `None` on contention. It makes one kernel attempt and does not block
waiting for that profile. Auth owns asynchronous waiting, original cancellation
and deadline handling. Different profile locks remain independent; no global
publisher lock spans the remote request. The lock grants no authority. Permanent
lock files remain private and are not removed on drop. Their existing bytes are
preserved; kernel locking does not interpret the file contents.

`read_locked(original)` supplies a short shared publisher lock and current bytes
for credential admission. It returns the conventional guard directly. Consumers
must finish inspection and release it before network work.

## Runtime secret

`RuntimeKeyOwner::start(root, original)` uses the conventional `daemon/` directory,
a real process-lifetime `mcp.owner.lock`, and a fresh private 64-byte hex secret.
The owner lock serializes live daemon owners, rather than proving directory
confinement. A new process startup replaces stale secret content atomically.
`check` returns a borrow of the actual owner secret tied to its request and
in-memory credential generation. No path, ancestor or physical-file receipt
recreates a caller or authorization.

`maintain` explicitly republishes a missing key with the same process secret and
advances its credential generation; a changed foreign body is preserved and
reported as a conflict. `remove` compares current secret bytes before unlinking.
Both return a conventional result with an applied/not-applied effect. Dropping
the owner releases the actual owner lock and never removes the key implicitly.
Restart is explicit reexecution; there are no recovery/bootstrap-ready carriers,
stage receipts, namespace effects or linear cleanup proofs.

## Verification scope

Functional tests cover current bytes and size bounds, stale-content conflicts,
equal-content replacement, short CAS locking, independent profile contention,
cancellation/deadline, credential privacy, configured links, process ownership,
secret recreation and explicit removal/restart. Structural tests for the removed
capture/revalidation and recovery protocols are retired. Qualification of this
new source is recorded by the coordinating task; historical capture-protocol
runs do not qualify these replacement APIs or native hosts.
