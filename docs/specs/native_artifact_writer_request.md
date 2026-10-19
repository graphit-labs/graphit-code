---
title: Artifact catalogue writer
status: implemented-library
task: tsk-1ed0
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-1ed0}
    relation: implements
  - target: {type: task, id: tsk-7dd5}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: relates_to
  - target: {type: task, id: tsk-8b90}
    relation: implements
  - target: {type: task, id: tsk-a110}
    relation: implements
---

# Artifact catalogue writer

`ArtifactLimits::new(document_bytes, key_bytes, copy_limits)` bounds real envelope/key inputs. The contained `CopyLimits` bounds the actual tree being published. Publication has no byte/node reservations, inventory cardinality quotas or synthetic future-envelope clones; actual bounded encoding precedes atomic catalogue replacement, and physical pending effects remain recoverable.

On Linux, Windows and macOS, ArtifactStore::writer_with_context receives Arc<SnapshotContext>, captures the existing owner-relative control chain, retains its handles and owns the kernel FileLock for the writer lifetime. The context carries the original cancellation and deadline. The portable writer route continues to serve its existing callers.

An absent Original deadline remains absent during lock acquisition; cancellation
still applies. Native Hub shared documents, managed roles and OpenCode plugin
publication/removal pass this captured context directly on all three platforms,
without synthesizing a lock timeout or rebinding the IO context afterward.

Loading the current catalogue reads the physical body through the captured
runtime directory, checks its binding and decodes the bounded document. A PROJECT
catalogue belongs to its PROJECT ID, brand and global storage/runtime roots;
worktrees of that ID share it. Storage directory receipts still match the live
capture. Checkout roots and allowed destinations remain observations of the
operation that recorded them, not catalogue identity or authority for a reader.
An absent catalogue stays absent until save. A loaded baseline remains tied to
its writer and is checked before publication.
The JSON is the document directly, without its own persisted inode. Owner data
appears once in `scope`; directory bindings retain only path, native identity
and permissions. Captured FD/parent identities and exact-byte checks continue
to guard writes and post-effect observation. An identical complete private
replacement before a new capture is valid, not an adoption of an unknown
artifact destination.

Entries, pending operations and abandoned publications from another worktree are
readable as data without rewriting their paths or receipts. Publish, check,
remove, access restoration and recovery require their destinations, including
the previous destination and staging paths, to belong to the current captured
scope before filesystem access. Native-document recovery also checks that live
destination scope in addition to its caller correlation and captured parent.
Publication identity is `(PublicationSlot, ArtifactKey)`. A PROJECT slot records
the physical root identity captured once by `CapturedPublicationScope`; nested
roots have distinct slots too. Permission changes do not change that identity.
Installation and USER scopes use `PublicationSlot::Scope`. The required slot is
stored in each publication candidate, including previous entries and archived
operations; decoding never supplies a missing slot.

Scope, store, writer, entry and publication-intent `publication_slot()` getters
return selection data. `ArtifactIntent::publication_slot()` returns `Some` for
Publication/Inspect and `None` for native-document operations. Readers
can inspect all catalogue entries; consumers select their current slot explicitly.
Publish, update, verify, remove, forget and publication recovery select that exact
slot before physical access. The same key can coexist in two checkouts; changing
or removing A preserves B. Destination and receipt conflicts remain global.
The slot grants no authority: the original request, live scope and physical guards
still apply, including when an outer root contains another checkout.

Generated plans publish directly through `ArtifactWriter::publish_generated`.
The installed package supplies the canonical templates; no permanent intermediate
copy, payload pins or payload GC exists. The candidate records Graphit's package
origin/version and output hash, plus the physical publication and recovery data.
The generated path uses Copy; a request for Link fails explicitly. External source
Link and Copy remain available through the same writer.

Current content is checked against the installed hash before update/remove.
A byte-identical atomic save does not imply user modification; altered content or
unexpected entries remain conflicts. The original request and current scope still
control access. Rendering or publishing in checkout A cannot affect B's entry.

A persisted directory binding permits permission hardening of the same physical
directory after a fresh capture: current Unix permission bits must be a subset
of the recorded bits; Windows readonly may change from false to true. Adding
permissions, changing identity, path, owner or other recorded node data remains
a mismatch. Live captures still revalidate their exact observations, so a chmod
during a live operation requires recapture. Reads never rewrite the catalogue
to accommodate the change, and payload/content receipts keep their exact rules.

Native-document saves serialize a borrowed DocumentProjection with the current scope and pending operation. Entries and physical recovery operations retain their existing values. The common bounded serde encoder returns Vec<u8> for atomic publication. Temporary catalogue files are cleaned on success and on write or sync failure.

A crashed `Publication/Building` can be aborted without losing the previous
published entry. When a staging namespace remains, `recover()` atomically moves
the complete operation and scope to `abandoned_publications` and clears pending.
If all staging paths are absent, it clears pending without adding a residue
record. A wrapper with a recorded birth
receipt must retain that identity and its private permissions; replacements,
links, changed permissions and unexpected entries in such a wrapper remain
conflicts. If interruption occurred between directory creation and recording its
birth receipt, that namespace stays unknown: recovery neither opens it to prove
ownership nor adopts or deletes its contents, even when empty. Only the logical
intent is abandoned. The existing `AbortedPublication` outcome allows the next
ordinary publication with a fresh staging path. Partial or unproved bytes remain
untouched. Generated staging exists only for the active/recoverable publication. Empty archives are omitted from the
catalogue; a missing archive means no abandoned publications. Native-document
saves preserve this optional record too. Save failure or cancellation before
recording leaves the pending intent and previous entry intact.

Intent, phase, resolution, acknowledgement and release records preserve restart recovery. Reconciliation of an already observed filesystem effect can finish after cancellation; acquisition and ordinary acknowledgement still check the operation. Recovery reads persisted state and does not repeat a completed publication. Dropping a writer closes its resources and keeps pending recovery records durable.

After a Copy destination effect, cooperative journal completion does not reuse
the cancelled pre-effect token for its verified receipt. A native Original,
when present, is still checked; its failure preserves the pending operation for
freshly authorized recovery rather than claiming rollback or completion.

Affected tests cover writer collisions, catalogue substitution, write/sync failures, baseline conflicts, phase/outcome serialization, cancellation, timeout, restart recovery and acknowledgement/release.

## Recreating a deleted owned profile

A recorded copied **file** can be republished at its exact existing key and destination after the file is deleted. Its parent must still exist. `check_publication` returns `NeedsPublication`; the authorized publication uses the current captured source and reports `Updated`. It does not adopt an existing file by matching its bytes.

The persisted recreation operation retains the previous logical entry until commit and publishes through the existing exclusive create/move path. Recovery proves the prepared payload or its actual published identity, rejects an unexpected destination or backup, and preserves a foreign file or symlink that arrives during publication. Cancellation and captured scope checks keep their existing boundaries. Missing directories, missing parents, previously owned links and moving the old key to a different slot do not use file recreation. The focused `artifact_store::tests::owned_missing_file` fixtures cover this contract; live Coordinator recovery requires the newly emitted CLI and its current Source.

The existing CLI `RUST_LOG` filter can enable `graphit_core::artifact_store=debug` in the launch environment to diagnose a retained native writer. Its span records the caller's source basename and line, a ProjectId only for PROJECT scopes, and fixed capture/wait/acquired/read/publish/recovery phase labels. The span's CLOSE follows the writer's FileLock drop. It retains no entered thread-local guard across awaits and logs no full filesystem path, USER identity, credential or payload. Diagnostics do not change acquisition, validation, deadlines or recovery.

## Fixed record directories

On Linux, Windows and macOS, `ArtifactWriter::create_record_directory(ProjectRecordDirectory::{Task, Knowledge, Memory}, limits)` exclusively creates the selected fixed `task`, `knowledge` or `memory` child under the captured PROJECT runtime. All three consumers use the original request acquisition and the held writer lock. The caller authorizes its module before acquiring the writer; this API grants no module authority. It accepts neither arbitrary paths nor installation/normal writers.

The returned directory remains bound to the original request and captured parent chain. Creation uses private permissions and rejects every existing entry instead of adopting it. Cancellation or parent/child substitution after a physical creation returns the real error and can leave an empty directory; callers must distinguish that residue from a successfully created corpus. Read capture and child probing remain observational and never create the runtime, lock or corpus. Directory birth alone does not create a Lance dataset or publish Task or Memory records.

Memory directory birth is integrated in Main under `tsk-8b90`. Linux qualification covers 19 affected native writer tests, including all three fixed corpora, existing sockets/files/symlinks/directories, original cancellation, parent replacement, and effect-free reads. Strict Core Clippy with `auth-test-support` and scoped formatting pass. This qualifies directory acquisition, not the Memory dataset or its consumer; Windows/macOS native execution was not run.

USER has a separate fixed `ArtifactWriter::create_user_memory_directory` under its captured principal namespace. It preserves the same native writer/Original/lock implementation and leaves the PROJECT factory PROJECT-only. See [native USER capture](native_user_scope.md) for layout, readonly absence and the independent Auth/Records requirements.

## Held-parent platform composition

The native writer retains the original request, control chain and exact permanent lock file on all three platform implementations. After waiting for the kernel lock it revalidates scope and directory bindings, reopens only the existing lock relative to the retained control parent, and compares its handle identity with the acquired file. A missing or replaced lock fails acquisition; validation never creates another lock. Catalogue reads use the native current-body capture and retain the same writer baseline.

Windows uses the existing generated NtCreateFile bindings with RootDirectory set to the captured parent, FILE_CREATE for exclusive birth and FILE_OPEN for existing entries. The existing owner-only protected DACL descriptor is supplied at birth. Same-handle privacy checks reject reparse objects, additional file links and changed owner/access. Lock contenders share read/write access; delete sharing remains excluded. Consequently namespace replacement tests observe native sharing denial while those handles are retained.

macOS uses public rustix openat/mkdirat, NOFOLLOW and the existing filesystem/device checks. Before birth it inspects the captured parent ACL for the child-specific FILE_INHERIT or DIRECTORY_INHERIT flags used by XNU. The parent itself may be public and may have non-inheritable ACEs. A relevant inheritable ACE denies birth before the entry is created. The resulting handle must still pass the shared exact owner/mode/link/no-ACE verifier. No ACL is removed or repaired after birth. This is an explicit limitation for fresh bootstrap under an inheritable ACL; it is not full support for that environment. Existing private read/revalidation operations remain observational. The separate path-based private birth producer with NO_INHERIT remains unchanged.

Cancellation after a real creation may leave an empty private entry. Neither an error nor a returned primitive handle grants corpus ownership or cleanup authority. The fixed record factory still rejects existing entries. No source path is reconstructed from a parent handle, no process/thread CWD is changed, and no private Apple SPI is linked.

Primary API evidence: [Microsoft NtCreateFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntcreatefile), [pinned XNU ACL inheritance](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_authorization.c#L736-L879), and [pinned XNU ACL flags/layout](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/sys/kauth.h).
