---
title: Native USER namespace capture
content-type: reference
audience: developers
status: implemented-library
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-319e}
    relation: implements
  - target: {type: task, id: tsk-8c29}
    relation: derived_from
  - target: {type: task, id: tsk-633b}
    relation: supports
  - target: {type: task, id: tsk-7fc2}
    relation: supports
  - target: {type: task, id: tsk-fa19}
    relation: implements
  - target: {type: task, id: tsk-a110}
    relation: implements
---

# Native USER namespace capture

`UserIdentity` stores the literal verified issuer and subject supplied by Auth.
Core validates data; it does not verify JWTs or grant access. Equal subjects from
different issuers and case variants remain different identities. The portable
namespace key is lowercase SHA-256 over `graphit-user-namespace-v1\0` followed
by each literal UTF-8 component's u64 big-endian byte length and bytes. Core
keeps both literal components in `StoreOwner::User { installation_id, identity }`;
the key, path, deserialized owner and binding are never authentication evidence.

`CapturedPublicationScope::user(installation, installation_id, identity, limits,
original)` captures the existing installation birth anchor. Its runtime is
`installation_root/users/<key>/runtime`, and its publication root is exactly
`installation_root/users/<key>`. A missing namespace is valid. Capture observes
and revalidates the actual native directory identities without creating any
directory, runtime, lock or dataset. Restored `ScopeBindingRecord` values check
the exact derived USER layout and remain data. `user_root()` is a selection
getter, not permission.

`CapturedUserDocumentDirectory::capture(scope, relative_directory, limits,
original)` selects the USER root itself when `relative_directory` is empty,
or a validated descendant for a nonempty selector. Empty selection preserves
the root without adding a trailing separator. A missing root returns NotFound
without creating the namespace; a captured root supports the same readonly
child operations and Original checks. Nonempty trailing separators, dot/parent
components, absolute paths and NUL remain invalid. It cannot
select another USER, the installation root or a PROJECT capture. Its held chain
shares the native directory implementation and retains the same Original.
`revalidate`, `probe_child_directory`, `read` and `revalidate_observation` check
that operation and physical chain. Child observations do not provide a writer.
`probe_memory` observes only this USER's `runtime/memory`; absence is returned
after the existing held ancestor and scope checks. Permission errors, links,
wrong types, replaced captured directories, cancellation and deadline remain
errors. READ does not create an empty corpus to disguise absence.

On Linux, Windows and macOS, `ArtifactStore::writer_with_context` reuses its existing held control chain and kernel lock at the USER runtime. After the caller's actual Auth write admission, `ArtifactWriter::create_user_memory_directory(limits)` creates only the fixed private `memory` child and returns its USER directory capture. It rejects existing entries rather than adopting or repairing them. The writer keeps the original operation and closes its lock on drop. A failure after a real directory birth can leave an empty directory; it does not imply rollback or a published dataset. The platform implementation follows the native writer contract and its held-parent privacy requirements. Windows/macOS native execution remains UNRUN. Fresh Darwin birth under a parent with a child-relevant inheritable ACE is denied before creation; non-inheritable parent access does not require a private parent.

The PROJECT directory capture and fixed PROJECT record factory retain their
PROJECT restriction. The existing Auth PROJECT scope validator rejects USER
owners explicitly. Core provides no Memory schema, SDK history/search, USER
admission, cross-source reference authority or `include_user` policy. Those
remain required Auth/Records deliveries. Read requires ReadUser and ReadMemory;
mutation requires MutateUser and the actual ordinary/Fenced Memory permission,
checked separately from this physical scope.

Source is in Core `artifact_store/scopes/user.rs`, the owned scope dispatch,
`native_document_file/user.rs` and the existing native held-chain/writer helpers.
Focused fixtures in `artifact_store/tests/user_scope.rs` exercise literal
isolation and binding restore, no-read birth, private first write/reopen,
foreign USER selection, physical replacement/links/files, Original mismatch,
cancel/deadline and native contention. Focused Linux qualification passed six USER fixtures, 19 native writer guards
and 31 held-directory guards. Strict Core lib/tests Clippy, scoped formatting
and three local documentation links passed. Receipts are recorded by tsk-319e. See [the native writer contract](native_artifact_writer_request.md)
and [verified user identity](verified_user_subject.md) for the independent boundaries.

The focused fa19 correction passed seven existing/extended Core USER fixtures.
Its root-selection case uses the actual typed directory and ArtifactStore writer
held under the same Original: cold root capture reports NotFound without birth;
after writer birth, empty selection reads an absent child and a private 0600
file and revalidates both observations. Foreign Original and malformed selectors
remain rejected, and the FileIO canonical validator is unchanged. This Core proof
does not qualify AI conversation persistence or its consumer integration.
