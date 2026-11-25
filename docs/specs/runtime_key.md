---
title: Private runtime key ownership
content-type: reference
audience: developers
status: partial
updated: 2026-11-07
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

`RuntimeKeyOwner::start(root: PathBuf, original)` returns a conventional result
containing the real process owner or a `RuntimeKeyError`. Configured roots may
be links or select another volume. No bootstrap observation, captured directory,
ancestor chain, path/depth limit, physical receipt or recovery carrier is used.

Startup checks current credential privacy, creates `daemon/` privately when
needed, and takes one real exclusive `mcp.owner.lock` for the process lifetime.
A live competing owner produces `Busy`. The actual owner lock is not removed or
replaced. It serializes process ownership, not a filesystem confinement claim.

Each successful startup encodes 32 bytes of fallible OS entropy as 64 lowercase
hex bytes. An old private key is replaced atomically with a newly generated
secret, rather than adopted as the new process's credential. Content is written
to a private `tempfile` beside `mcp.key`, synced and persisted by the library.
The secret and temporary read buffers are zeroized. File privacy is enforced
when opening existing files and by native private creation for new files,
through the real filesystem SDK, without a blanket ban on private hardlinks or
configured directory links. Permissions are never repaired implicitly.

## Actual secret and credential generation

`check(original)` compares current private key bytes with the process secret and
returns `RuntimeKeyBorrow`. It borrows that actual owner, its credential
generation and original request. `secret` and `recheck` require that request and
check its cancellation/deadline. A fresh legitimate operation can obtain a new
borrow. This generation protects runtime authentication and stale credentials;
it is not a directory identity or an ancestor receipt. Owner and borrow expose
no serialization, cloning or debugging of secret state.

The owner retains an ordinary `RwLock` around its in-memory generation and
present/removed state. It keeps no directory handle chain. `root()` exposes the
configured path as data, never authority. Auth validates the logical selected
installation and actual permissions independently.

## Maintenance and explicit restart

`maintain(original)` returns `Result<RuntimeKeyEffect, RuntimeKeyError>`.
Unchanged matching bytes return `NotApplied`. A missing key is atomically
republished with the same process secret and advances its credential generation;
old borrows then conflict. Changed foreign bytes return a conflict and are not
overwritten.

`remove(original)` compares current private bytes with the process secret before
unlinking, then advances generation and marks the owner removed. It returns
`Applied` on success. Cancellation or a content conflict leaves the key intact.
No fallible cleanup phase is attached after a successful commit. `Drop` only
releases the actual owner lock and zeroizes in-memory secret state.

An explicit subsequent startup after the owner drops generates a new secret.
There are no fixed stage slots, namespace-effect records, pending cleanup,
RuntimeKeyLimits, StartOutcome, BootstrapReady or stage-recovery APIs. A derived
runtime operation is recovered by explicit reexecution, not a generic protocol.

Functional tests cover native lock contention, actual private bytes, original
cancellation/request identity, stale credential generation, missing-key
maintenance, foreign-content preservation, explicit remove/restart and configured
links. The coordinating task records qualification of this new source; previous
capture/receipt test runs do not qualify it or a native host.
