---
title: Native USER namespace selection
content-type: reference
audience: developers
status: implemented-library
updated: 2026-11-07
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

# Native USER namespace selection

`UserIdentity` stores the literal verified issuer and subject supplied by Auth.
Core validates data; it does not verify JWTs or grant access. Equal subjects from
different issuers and case variants remain different identities. The portable
namespace key is lowercase SHA-256 over `graphit-user-namespace-v1\0` followed
by each literal UTF-8 component's u64 big-endian byte length and bytes. Core
keeps both literal components in `StoreOwner::User { installation_id, identity }`;
the key, path and deserialized owner are never authentication evidence.

`PublicationScope::user(installation, installation_id, identity)` selects configured
paths without filesystem access. Its runtime is
`installation_root/users/<key>/runtime`, and its publication root is exactly
`installation_root/users/<key>`. The namespace may be absent or reached through
configured filesystem links or mount points. Construction creates no directory,
runtime, lock or dataset and retains no directory identity, mode, ancestor chain
or serialized physical binding. `user_root()` is a selection getter, not permission.
Relative record selections stay within the exact USER namespace; absolute paths
and parent traversal cannot select another USER or the installation root.

The Records USER service derives the fixed `runtime/memory/index.lance` path
from this selection and opens it through the Lance SDK. Configured links and
mount points use ordinary filesystem resolution. Reads observe an absent dataset
without creating the namespace, directories, locks or an empty corpus. Published
schema or history corruption remains an error; directory existence alone is not
proof that a dataset was published.

Legitimate first creation prepares the parent with normal filesystem operations
and uses the SDK Create CAS under the existing writer lock. Current Auth and the
operation's original cancellation/deadline are checked around exposed SDK work.
No directory handles, ancestor identities, mode bindings or serialized physical
scope records are retained. A caller cannot supply another USER's corpus URI or
select a PROJECT/Git branch through this service.

The logical PROJECT factories remain PROJECT-only. Core namespace selection
does not grant Memory, USER admission, cross-source reference access or
`include_user`. Auth requires ReadUser plus ReadMemory for reading, and MutateUser
plus the matching ordinary or Fenced Memory operation for mutation. JWT and Role
verification remain in Auth; a namespace hash, path or decoded owner is data.

Implementation is in Core `artifact_store/scopes/user.rs` and Records
`user_memory_store/native.rs`. The scope tests verify literal identity isolation,
namespace selection and lexical publication boundaries. Records tests exercise
real admissions, SDK pins, creation/history/CAS, USER isolation and transport
renewal. Platform qualification is reported with each executed suite; earlier
directory-capture receipts do not qualify the current conventional path code.
See [USER Memory](native_user_memory_store.md),
[verified user identity](verified_user_subject.md) and
[the writer contract](native_artifact_writer_request.md).
