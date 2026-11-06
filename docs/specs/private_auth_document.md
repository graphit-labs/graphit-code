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

# Private installation authentication document capture

`graphit_core::auth_document` captures, reads and publishes the fixed `auth.json`
document inside an already existing, private installation directory.
Bytes returned by it do not authenticate a caller or restore permissions.
The profile schema is a separate `graphit-auth` producer.

## Capture and read

The caller supplies a real `CapturedPublicationScope`, positive
`AuthDocumentLimits`, and the original `SnapshotContext`. Capture accepts only
an installation owner whose allowed roots explicitly include its exact owner
root. A project scope or a child-only root selection is rejected before
authentication document IO. The installation root must already satisfy the
filesystem SDK's private directory checks. The store retains its actual
directory handle and revalidates the installation namespace, physical directory
identity and privacy
before and after reading, including after an unsuccessful read attempt.
The exact-handle reader checks the file's identity, privacy, bytes and EOF;
its inner callbacks check the original cancellation and deadline. Namespace
and ancestor checks fence the complete read and revalidation, rather than
repeating the same ancestor traversal at each metadata probe or content chunk.

The path is fixed; the API accepts no document filename. A missing document
returns a snapshot without bytes and creates no directory, file, lock, key,
profile or login. Capture and reads never repair permissions. Existing files
must be regular, private files and satisfy the SDK's physical path checks.
Native IO failures become closed, static errors without paths, document bodies
or retained vendor errors.

Capture and every store check use `revalidate_installation`: owner/runtime roots
and their captured ancestors remain physically checked, while unrelated admitted
project directories are not prerequisites for reading the installation auth
document. Project grants and operations keep their own complete physical checks.
Ordinary scope-lock contention waits and then rechecks the original cancellation
and deadline. Poison still returns the existing static `Io` error; explicit
`revalidate` recovery remains separate.

Managed Auth inspection takes a shared lock on the existing private
`.auth.json.lock`, allowing concurrent readers. It retains that lock through the
physical and logical checks, then releases it before network work. Publication
takes the exclusive lock and revalidates its expected observation before writing;
it cannot run while these readers hold their shared guards. Reads still create
neither document nor lock, and lock identity, privacy, cancellation and deadline
checks remain mandatory. Shared inspection grants no publication capability.

## Binding the selected installation

`AuthDocumentStore::recheck_installation(real_scope, context)` checks the store's
own live scope and SAME private parent FD before and after inspecting the supplied
real realm. The captured owners must match completely, including InstallationId,
with the same captured Brand, canonical installation root and full native root
identity. Both namespace checks cover the installation owner/runtime and their
pertinent captured ancestor receipts. Own and candidate scope locks are taken and released sequentially;
there is no retained guard or nested scope-lock acquisition. The candidate context
inherits this operation's original deadline and cancellation.

This bridge performs metadata checks without reading or writing document
content. It does not reopen or capture the root, clone the supplied
realm or binding record, read auth.json, or construct authority from paths/IDs/JSON.
The existing internal inspections make bounded metadata/path copies under each
realm's positive ScopeLimits. Native path/access limits remain those of the
captured AuthDocumentLimits; these separate domains are not a universal RSS cap.

InstallationId compares two selected real captures; it does not independently
verify persisted identity.json. A legitimate fresh metadata operation may check
the binding, but it neither rebinds nor validates an AuthDocumentSnapshot. Content revalidation still requires its original store and
request, with all body/physical/privacy checks. Caller authentication, cryptography,
permissions and RoleIssuer integration belong to their Auth/runtime consumers.

Fixtures preserve files when owner ID/Brand/root/Project differs, when a parent
is replaced (including restored mtime), symlinked or loses privacy during candidate
inspection, or on cancellation/deadline/own-or-candidate lock contention/poison.
Individual native-path, directory-entry and depth limits reject oversized
inputs without consuming body I/O. These are cooperative physical observations, not hostile
filesystem atomicity against arbitrary mutation and metadata restoration.

## Input bounds and original observations

All reads use the supplied cancellation and deadline when present. The configured
document byte limit bounds actual content, and the SDK verifies its physical
length and EOF. `AuthDocumentLimits::new(bytes, path_bytes, ancestors, access_bytes)`
bounds actual document content, individual native paths, depth and access metadata. `SnapshotContext` retains only
original request identity, deadline and cancellation; it has no shared byte
quota, reserved partitions or I/O counters.

`AuthDocumentSnapshot` retains the open file observation and zeroizing bytes.
Consumers borrow the bytes. It has no public reconstruction, `Clone`, `Debug`
or serialization interface. Revalidation accepts only a snapshot from the same
store and original request. Sharing that original context preserves its
identity; a newly constructed context cannot rebind the observation, even with
equal deadline and cancellation values.

Revalidation detects replaced files, changed bytes even with restored timestamps,
first publication after an absent observation, changed parents and changed
privacy. If a retained file fails its private-file check, a fresh pathname/handle
identity mismatch is a conflict, including replacement that unlinks the old inode.
This never accepts the old observation or weakens privacy checks on the current
file; a current file with unsafe permissions or hardlinks remains denied.
These checks provide cooperative change detection; they do not establish
atomic compare-and-swap against a hostile filesystem.

## Validation and integration boundaries

The current Main `tsk-1750` qualification passed 54 affected `auth_document`
cases with `auth-test-support`, including absence, actual content limits,
original request identity, changed contents/physical files, cancellation and
deadline, scope rejection, failed-read final checks and publication/recovery.
One ignored standalone crash child is invoked by its parent fixture. The
focused normal-feature documentation run passed six compile-fail examples;
strict Core all-target Clippy with `auth-test-support` and formatting passed.
These existing Source results are reused for this documentation correction;
no Cargo command was rerun for `tsk-00c5`. Evidence is in
`.build/rust-core-auth-document-unused-max-getter-20261011`. Executed runtime
evidence is Linux; no Windows or macOS runtime success is inferred.

Private installation bootstrap, schema-backed AuthStore, OAuth, server authority
and permission runtime are separate producer and consumer boundaries. Core does not check
application permissions or interpret authentication document contents.

## Publisher and managed stage recovery

`AuthDocumentStore::publish` consumes an actual snapshot from the same store and
original request and borrows the newly encoded body. It acquires the permanent,
empty, private `.auth.json.lock`, then revalidates the expected FD, physical
identity and actual bytes. Only that check can return `Unchanged`. A stale
snapshot, new request, changed parent or same bytes in a different inode cannot
authorize a write. The caller finishes any network operations before invoking
this filesystem boundary.

The single fixed `.auth.json.stage` is created exclusively by the private
filesystem SDK in the captured private installation root. No alternative path
is created when that slot is occupied. Maximum pending stages is structurally
one, including empty residue left by a failed private birth. Stage privacy,
physical identity and scope are checked before each content write and fsync.
Writes use bounded chunks and check the original cancellation and deadline.
The stage's bytes are compared
twice, including after the final physical checks, before publication.
After the old document's final byte comparison, the publisher captures and
rechecks the stage's original physical identity, private access and metadata
again before native replacement. This adds no third content pass. A different
private stage with identical bytes cannot reach the replacement primitive.

The new body must satisfy the configured document byte limit. The publisher
revalidates the actual expected observation, compares the stage and qualifies
the native replacement outcome under the original request. These content and
physical checks use their actual body bounds; they do not reserve a predicted
number of content passes or charge I/O counters. The new body remains borrowed
and is not copied into another retained secret buffer.

Windows stage and expected handles are closed before replacement because the
private SDK excludes delete sharing. The final expected comparison opens,
checks and closes its own handle. First publication uses native no-overwrite;
existing publication uses Windows replacement or Unix rename. Actual observed
old/new physical identity and bytes qualify a native error. Effects are closed:
`Applied`, `Unchanged`, `NotApplied`, `Unknown`. A successful native replacement
remains `Applied` if cancellation or another failure happens afterward; native
errors remain `Unknown` unless the bounded observation proves old or new.

Windows file privacy is checked from the native owner and protected DACL, rather
than requiring the creation descriptor's exact SDDL serialization.
[ReplaceFileW preserves and merges DACL information](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew).
Equivalent duplicate explicit full-access ACEs for the current owner and
auto-inheritance bookkeeping do not grant another principal access. The verifier
still requires a nonempty protected DACL, the current user as owner, and only
plain explicit owner full-access ACEs. Foreign principals, denied/conditional or
inherited ACEs, nonzero raw ACE flags and incomplete masks remain denied. File
type, hardlink count, readonly and reparse checks are retained. Directory privacy,
retained namespace handles and their sharing rules are unchanged; captured access
snapshots remain exact opaque observations for change detection.

Failed cleanup exposes a separate static cleanup failure and an opaque
`PendingCleanup` containing real process-local creation origin. It cannot be
restored, cloned or debugged and does not delete anything on drop.
`cleanup_pending` consumes that token under the same store and a new authorized
request, with current private scope/lock/physical revalidation. Replaced entries
are preserved. Cleanup has its own `Absent`, `Applied`, `NotApplied`, `Unknown`
effect, rather than hiding an unlink under the publication result.

`recover_stage` provides automatic recovery after process death. Its authority
is **discarding the current managed namespace**, not proving the historical
producer or adopting a previous creation receipt. The AuthStore consumer
must check current `ManageAuthProfiles` before either publication or recovery.
Recovery captures the current bounded, regular, private stage FD and physical
identity, rechecks parent, scope, permanent lock and privacy, closes incompatible
Windows handles, and unlinks only that exact slot. It never reads, parses,
promotes or authenticates its contents. Symlinks, hardlinks, nonprivate files,
oversized files and observed replacements are denied and preserved. There is no
directory scan, JSON ownership ledger, permission repair or unlimited retry.
Native cleanup errors also require an actual bounded observation before claiming
an effect. These are cooperative checks, not hostile filesystem CAS.
An absent-stage result also requires a final original-context, scope, parent and
permanent-lock check after the absence probe; cancellation or a retargeted
parent/lock cannot produce a successful `Absent` result.

`publication::AUTH_PRIVATE_NAMES` exposes `auth.json`, `.auth.json.stage` and
`.auth.json.lock` for the required agent filesystem views. Every such view must
exclude all three names in the canonical installation owner root, respecting
native pathname equivalences (including case on Windows); a private mode alone
does not authorize agent access. AuthStore's permission check and these runtime view
exclusions remain consumer responsibilities outside Core.

The historical `tsk-effe` publisher qualification exercised real SDK private
birth, real content IO, native
replacement, namespace discard, original cancellation and deadlines, bounded
quota admission, old/stage changes, parent/lock/privacy changes, cleanup origin
and ambiguous native effects. The bounded subprocess fixture terminates the actual
publisher without Rust destructors at private birth, a content write and fsync
(six child executions),
then recaptures and recovers the slot without secret parsing or residue growth.
The ignored standalone child is executed by its parent fixture. Native runtime
qualification here is Linux; Windows/macOS execution remains required elsewhere.

Its historical qualified focused unit run was
`CARGO_INCREMENTAL=0 cargo test --locked -p graphit-core --lib auth_document`:
36 passed (14 capture/read and 22 publisher/recovery) plus the one standalone
subprocess child described above. The focused doc run passes six compile-fail
tests, including three for `PendingCleanup`. `cargo fmt -p graphit-core --check`
passes. The coordinator's workspace fast gate additionally qualifies final
Clippy against the assembled workspace.

The historical `tsk-3490` installation-binding qualification ran
`CARGO_INCREMENTAL=0 cargo test --locked -p graphit-core --lib auth_document::`:
44 passed, comprising the 36 existing cases and eight binding cases, plus one
ignored standalone child invoked by its passing parent. That unchanged publisher
fixture executes six subprocesses: private birth, content write and fsync, each
twice. The affected AuthStore run passed 16 tests and executed one additional
subprocess through its parent. Thus these two runs exercised seven real child
processes. The focused Core and AuthStore doc runs passed six and three compile-fail
tests respectively. Core Clippy, owned formatting and diff checks passed.
The coordinator's exact `cargo xtask check` passed on the frozen code and the
previous document text; this count-label clarification was made afterward and
changes documentation only. The evidence artifact preserves both qualifications
and the gate's immutable before/after source hashes.

Five review regressions reproduced the two missing final checks before their
fixes: cancellation, original deadline, parent/lock retarget during absent-stage
recovery, and replacing the private stage with identical bytes after old-content
comparison. The latter records actual native replacement calls and now requires
zero calls while preserving both the old document and the foreign stage.

## Test fixtures for publication cancellation

The explicit, default-off Core feature `auth-test-support` exposes
`auth_document::test_support::PublicationCancellation::before_replace(&store,
&original_context)`. This is a closed cancellation fixture for integration tests
that need a genuine `PendingCleanup` from the real publisher. It accepts the
actual captured store and live request; no path, stage, callback, error injector,
proof constructor or authority is supplied or returned. Arming performs no I/O.
`ArmError` has only static `Busy` and `InvalidContext` variants.

One opaque lease can be held per thread. The guard cannot be sent, shared,
cloned, debugged or serialized. A checked local sequence identifies its own lease;
the slot holds only cloned store/request identities and the original cancellation
signal. Nested arming returns `Busy` without replacing the existing guard. The
trigger is one-shot, but retains that lease until guard drop. Drop disarms only
its lease and performs no filesystem cleanup or context renewal.

The feature hook runs after the publisher's existing private `before_replace`
phase and before its real access check. It matches actual store allocation and
original request identity, including a shared original context. Another store, a
fresh context with equal deadline/cancellation data, or another thread does not
consume the trigger. The TLS slot is restored before cancelling the original
request. Existing private test hooks and this feature fixture are independent;
the new fixtures arm only the feature guard.

Cancellation denies the actual replacement and cleanup checks. The real
`CreatedScratch` and `finish` path yields `NotApplied`, static cancellation
failures and the original opaque pending proof. For an existing destination the
old bytes remain; for cold start `auth.json` remains absent. Dropping the fixture
leaves the private stage in place. The same store can then consume that proof
with a fresh authorized request using ordinary `cleanup_pending`; foreign-store
denial preserves the proof. The original cancelled request stays cancelled.
Application permission checks and the NativeProfileManager pending-denial/retry
scenario are Auth consumer checks under `tsk-aad4`.

Normal Core/Auth library builds and their normal/build dependency tree exclude
this facility. Cargo resolver 3 can unify explicitly enabled dev features during
test, all-targets or all-features builds; those builds include the fixture and
are not proof of production absence. There are no new SDK/dependencies or default
features. Resource cost is a constant TLS slot per thread, small lease metadata
and the three shared identity/signal handles; no process RSS or native file cap
is claimed.

Separate `cargo check --locked -p graphit-core --no-default-features --lib` and `cargo check --locked -p graphit-auth --lib`, the normal/build feature tree and an external minimal default-feature import probe verify production absence. The feature tests exercise both present-old and initially-absent destinations, actual zero native replacements, original pending cleanup, nested/one-shot leases, early drop, intentional panic unwind, admission failure, foreign store and request, shared original context, other thread and invalid-context boundaries. The external default-off probe failed specifically with `E0432` for the configured-out module; its ordinary `AuthDocumentStore` control compiled successfully.
