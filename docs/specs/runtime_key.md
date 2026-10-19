---
title: Private runtime key ownership
content-type: reference
audience: developers
status: partial
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-c2c5}
    relation: implements
  - target: {type: task, id: tsk-a5fa}
    relation: implements
  - target: {type: task, id: tsk-bbd6}
    relation: implements
  - target: {type: task, id: tsk-0070}
    relation: derived_from
  - target: {type: task, id: tsk-effe}
    relation: derived_from
  - target: {type: task, id: tsk-a22e}
    relation: derived_from
  - target: {type: task, id: tsk-e849}
    relation: supports
  - target: {type: task, id: tsk-9bd4}
    relation: supports
  - target: {type: task, id: tsk-e911}
    relation: derived_from
---

# Private runtime key ownership

`graphit_core::runtime_key` owns the local runtime's private key and process
lease. Startup consumes a real [private installation observation](private_installation_bootstrap.md)
from its original `SnapshotContext`, then qualifies the fixed `daemon` directory,
`mcp.owner.lock` and `mcp.key`. A path, configuration record or JSON document
cannot construct the owner. The existing private filesystem SDK and document
kernel provide physical handles, privacy checks and native publication.

This module does not launch a daemon, bind HTTP/MCP, publish a port, schedule a
timer, authenticate an HTTP caller or grant RuntimeAdmin. Runtime startup and
server authority remain required consumers. Runtime keys have no intrinsic TTL;
finite administrative sessions belong to a separate authority boundary.

## Owner and checked data

`RuntimeKeyLimits::new(path_bytes, ancestors, access_bytes)` validates three
input limits. `RuntimeKeyOwner::start` checks each native path and its
ancestor count before creating the fixed
`daemon` directory. Key size is fixed at 64; access observations require at
least the 12-byte POSIX allowance. Stage size is at most 64, including an empty
or partially written stage. These bounds constrain native input and content;
there is no estimated owner-memory reservation.

Each successful startup uses 32 bytes from fallible OS entropy and encodes
64 lowercase ASCII hexadecimal bytes, with no newline. It creates a new
process-owned secret; an existing private, exact-size hexadecimal key can be
replaced under the exclusive lease, but its bytes are never adopted as the
new owner's secret. Public, symlinked, hardlinked, oversized or malformed
entries are denied and retained. There is no permission repair.

The owner retains a private namespace and owned kernel lease rather than a
borrowed startup deadline. Later operations take a fresh legitimate operation
context. The startup bridge still checks the original bootstrap identity before
consuming it; `PrivateInstallation::revalidate` itself still rejects a fresh
request. Drop releases the lease and zeroizes the secret without unlinking
keys, stages, locks or directories.

`check(context)` compares the private key against the immutable secret and
physical receipt before returning opaque `CapturedRuntimeKeyBorrow`. That
borrow retains its owner, generation and original check request. `recheck` and
deliberate secret exposure require that same original request,
with its deadline and cancellation. Equivalent fresh request values cannot
restore it. Neither owner nor borrow implements Clone, Debug or Serde, and
the borrow holds no lock guard. An Arc-shared owner uses a standard `RwLock`:
readonly checks and physical key comparisons share state access, while key
maintenance, removal and pending-stage cleanup retain exclusive access. Readers
cannot observe a partial publication or a generation change during their physical
inspection. Ordinary contention does not deny authentication. Cancellation and
deadline are checked before and after acquisition and throughout physical
inspection; lock waiting itself is not preemptible. A state poisoned by a writer
remains an `Io` error. `Busy` remains available for
actual filesystem lock contention.

Each readonly key comparison checks the complete captured namespace, directory
privacy and owner lease before and after the operation. The exact-handle file
kernel keeps the key's identity, privacy, 64-byte comparison and EOF checks;
its inner callbacks check the original cancellation and deadline. They do not
repeat the entire ancestor walk at every metadata probe or content read while
the owner state read lock is held. Publication and maintenance retain their existing
exclusive state and namespace checks. This removes repeated filesystem work;
it does not establish a measured end-to-end latency improvement.

`borrow.recheck_installation(scope, original_context)` performs one fresh key
comparison of the actual 64-byte key, then checks the caller's real
`CapturedPublicationScope` under its bounded lock. It requires an Installation
owner, the same captured Brand and canonical installation root, and the full
native identity of the retained private root FD. The installation owner/runtime
namespace and its captured ancestors are revalidated; unrelated admitted project
roots do not prevent installation authentication when a checkout is removed.
Matching paths, InstallationId or a restored binding alone cannot establish this
relationship. Project operations still validate their own full physical captures. Its scope context inherits the original deadline and cancellation.
Lock order is runtime state then scope directories; neither guard escapes.

The key comparison retains its real FD and bounded observation throughout that
inspection. A final metadata-only check reconfirms the same FD/path/full identity,
privacy and receipt, plus the namespace, lease and context. It neither reopens
the key nor reads its body again. Replacement by a private file with identical
bytes, or a permission change during inspection, denies the binding and preserves
the files. These cooperative checks do not guarantee atomicity against arbitrary
external mutation followed by restoration of metadata.

The public `CapturedPublicationScope::revalidate_installation` validates this
installation namespace without granting project access. Both it and full
`revalidate_bounded` wait for ordinary scope-lock contention, recheck the incoming
context after acquisition, and reject poison. Existing `revalidate` retains its
explicit poison recovery behavior. Scope inspection performs metadata work without
content I/O.
Its caller-owned scope and transient inspection use their own positive
`ScopeLimits::new(roots, path_bytes, directory_entries)` bounds root and
directory-entry cardinality, individual native paths and path depth. Repeated
inspection does not consume a work counter or accumulate pathname charges.
Binding creates no new capture and clones neither the supplied realm nor its
binding record. The internal inspection owns the bounded paths and vectors it
uses. Real handles and observations live until their operation or owner ends;
there is no shared-allocation charge or forecast of allocator overhead or RSS.

`borrow.generation()` returns its observed epoch without I/O. It neither proves
current validity nor reconstructs a borrow or authority. A consumer must recheck
the original request and installation before issuing a runtime-owned session;
missing-key recreation invalidates earlier epochs even with the same secret.

## Maintenance and shutdown

`maintain` validates the directory, ancestor handles, lease path/FD, key
identity, privacy and actual bytes. An unchanged key returns `NotApplied`
without a write. A key missing externally can be published again with the
same immutable secret, a new physical receipt and incremented generation.
All earlier borrows become stale. Foreign replacement, even identical bytes
and restored mtime in another inode, is a conflict; maintenance cannot overwrite
it. Age and mtime do not provide ownership or expiration.

`remove` unlinks only the observed own key under the live lease. Known removal
increments generation and enters terminal `Removed`; checks and maintenance
cannot resurrect it. Unknown removal enters `Indeterminate`, which also cannot
emit a borrow or retry automatically. Removed/indeterminate outcomes retain
their effects even if later cancellation, syncing or observation fails. The
permanent owner lock remains. This API does not remove `mcp.port`.

## Explicit stage discard and continuation

A private stage in `.mcp.key.next` occupies the one managed slot. Startup
conflicts rather than selecting another path or parsing/adopting its content.
For a bounded, regular, private observed stage, `RuntimeKeyRecovery` owns the
real namespace, exclusive lease and observation. `discard(self, context)` checks
them under a new legitimate operation, removes only the same physical slot and
returns opaque consumptive `RuntimeKeyBootstrapReady` only after successful
final qualification. `ready.start(context)` uses fresh OS entropy and actual
publication under the retained lease; it never fabricates a new installation
observation or extends an old deadline.

An empty/partial stage may be discarded without content reads; arbitrary bytes
within 0–64 do not become credentials. Oversized, linked, nonprivate or replaced
stages are preserved. There is no recursive scan, cleanup in Drop, public path
constructor, JSON ownership ledger or stage promotion.

A failed publication with unfinished cleanup also returns recovery retaining
the namespace, lease and original SDK `CreatedScratch` identity. Fresh discard
must qualify the current private bounded slot against that creation origin;
foreign replacements stay untouched. A currently absent slot permits ready
only after namespace, lease, absence and sync qualification. A successful native
unlink followed by observer failure reports `Applied` and no ready value.

Maintenance retains at most one pending creation origin inside the owner.
Further maintenance cannot create another stage until explicit
`discard_pending(context)` succeeds. That operation reports namespace cleanup
independently, reads no content, changes neither key generation nor receipt,
and cannot reactivate `Removed` or `Indeterminate`.

## Native effects and content

`RuntimeKeyStartOutcome` and mutation/recovery outcomes expose closed static
failure separately from `RuntimeKeyEffects`. The key effect is `NotApplied`,
`Applied` or `Unknown`. Namespace effects independently describe directory,
permanent lock, stage birth and discard. A failed key publication does not
erase earlier namespace effects. Successful native replacement or unlink stays
`Applied` if final observation fails and cannot provide a qualified owner or
ready value. Native errors require observed old/new physical state to distinguish
`NotApplied` from `Applied`; inconclusive observation reports `Unknown`.
No outcome converts physical facts into caller permissions.

Content operations read or write only the bounded key/stage data they need.
Startup and missing-key maintenance publish one fixed 64-byte hexadecimal key
through the existing private-document kernel. Key checks stream the comparison
and retain its physical observation; metadata, ACL inspection, syncing and
unlinking preserve their native errors and effects. The original request
checks cancellation and deadline around these operations. `SnapshotContext`
has no byte accounting, partitions or reserved-capacity counters.

The comparison kernel uses a 64-byte stack scratch for bodies up to 64 bytes,
including the runtime key, and retains the 64 KiB scratch for larger private
metadata. Both buffers use `Zeroizing` on success and error paths. The shared
comparison loop preserves full-byte comparison, short-read errors, the extra
EOF check, and the Original checks around each read. This changes scratch work
only; key identity, privacy, retained-FD inspection and authorization rechecks
remain in force.


Derived native paths use fallible allocation for the actual path. Preflight
checks each path and its ancestor count before namespace
birth. It does not reject a valid path because an allocator provided extra
capacity, and it does not reserve predicted owner, entropy, FD or shared Brand
memory. Cooperative revalidation does not promise preemption or atomic CAS
against arbitrary external filesystem mutation.

## Qualification and remaining consumers

Focused real filesystem fixtures exercise startup/check/maintenance/removal,
original-context binding, generations, replacement/privacy/link rejection,
explicit partial-stage recovery and actual path/access/key input limits,
native faults and bounded subprocess termination at actual publication phases.
An ignored child is invoked by its bounded parent; a skipped child alone is
not qualification. Executed native runtime evidence is Linux; Windows and macOS
remain required final qualification.

The daemon owns maintenance scheduling and listener/port/shutdown state.
Application authority compares
captured key data under its actual permission and finite-session contract.
Agent filesystem views must exclude all three `RUNTIME_PRIVATE_NAMES` in the
canonical runtime directory. Physical privacy alone never authorizes exposing
the secret. These application responsibilities remain separate from Core's
physical key validation.

## Current Main reservation cleanup

`tsk-a5fa` removed the maximum-retained-memory argument, the owner-memory
forecast and its exclusively used helpers. Main passed 34 runtime-key cases
(two child helpers are ignored and invoked by parent fixtures), 96 native
document-file cases, 11 installation-bootstrap cases and 17 runtime-key
compile-fail documentation cases on Linux. Strict Core all-target Clippy with
`auth-test-support` and formatting passed. The affected Auth, Daemon, Records,
Data, AI and Knowledge test targets, plus the selected AST Source handler,
compiled with the four-argument constructor. These are compile checks for the
mechanical callers; their unchanged functional SDK suites were not rerun.
Logs are in `.build/rust-core-runtime-key-reservation-cleanup-20261011`.
Native Windows/macOS runtime execution and unrelated deliveries remain separate.
