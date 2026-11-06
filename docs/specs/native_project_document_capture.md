---
title: PROJECT document reads and owner-root records
status: implemented-main
task: tsk-a987
references:
  - target: {type: task, id: tsk-a987}
    relation: implements
  - target: {type: task, id: tsk-014c}
    relation: implements
  - target: {type: task, id: tsk-98e7}
    relation: relates_to
  - target: {type: task, id: tsk-2706}
    relation: implements
  - target: {type: task, id: tsk-403f}
    relation: implements
  - target: {type: task, id: tsk-1ed0}
    relation: relates_to
---

# PROJECT document reads and owner-root records

CapturedProjectDocumentDirectory::capture(scope, relative_directory, limits, context) takes an existing PROJECT scope, a relative directory and Arc<SnapshotContext>. An empty relative path selects the already held PROJECT owner root; a nonempty path must contain only normal components. It retains the directory chain through real handles opened relative to the captured owner root. Installation owners are rejected. Capture neither enumerates a tree nor creates missing parents.

revalidate(current) checks the actual caller against the original captured request before native work. A foreign request returns InvalidInput, including when the original is cancelled or expired. The original request retains its cancellation and absolute deadline. The method revalidates held root and directory-prefix identities and permissions; it does not read child contents, so legitimate SDK commits, new files and removed files remain valid. An Arc<CapturedProjectDocumentDirectory> can retain this existing owner without another guard or factory. SDK pathname operations must use the same original context and revalidate at their integration boundaries; these checks observe directory identity at that point and do not turn a pathname into a continuously confined handle.

probe_child_directory(child, current) observes one exact child under the captured directory. It validates a single normal name component and the original request, then revalidates the chain before and after a native no-follow directory open. The temporary child handle and directory lock are released before the final checks. Only NotFound from that exact child open under valid parents returns DirectoryPresence::Absent; PresentDirectory is a momentary observation, not a retained child handle. Missing or replaced parents, files, links, denied access, cancellation and deadline expiry remain errors. This allows first-use readers to observe an absent runtime, knowledge or index.lance directory without creating it.

read(name, context) accepts one document-name component and the same operation context. The chain is checked before opening, before reading and after the observations. Only a missing child under valid parents yields absence. Missing or replaced parents, links, hard links and nonfiles remain errors. A physically read-only regular file is accepted when the held handle can read it.

revalidate_observation(observation, current) checks the original request and requires the observation to retain this same captured chain and directory identity. It rereads the exact document name and compares the observed value. A foreign chain, changed identity or changed document yields a conflict; it does not adopt a fresh directory or treat decoded receipt data as authority.

PROJECT and published-document reads share one body/hash/growth-probe loop. It validates the initial length before allocation, reads at most that body plus a one-byte growth probe, and checks cancellation and deadline between operations. EOF, growth, mutation, cancellation or timeout returns no partial observation. PROJECT observations retain their captured chain; dropping owners closes the files through RAII. They cannot be used as a published-directory path or writer stage.

NativeDocumentFileLimits bounds document bytes, logical path bytes, ancestor count, aggregate selected-prefix/file path bytes and complete access data. ScopeLimits continues to bound root capture. PROJECT uses the existing rustix and generated Windows SDK backends. Windows owner/group/DACL reads use SDK-validated aligned storage; this operation does not require a new FFI bridge.

Windows directory enumeration reopens the retained directory with generated
`NtCreateFile`: an empty internal Unicode name relative to that handle,
`FILE_OPEN`, and directory, synchronous-I/O and no-reparse options give it an
independent cursor. It preserves the existing read/traverse access, read/write
sharing without delete sharing, reparse-point rejection and volume/file-index
comparison with the held parent. No pathname is reopened and public component
selection still rejects empty names, `.` and `..`. The same operation covers root
handles and descendants opened relative to a held directory. See the
[SDK contract](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile).

Source FILE observations and idle readers use a distinct read-only profile on
Windows that shares write and delete access, allowing edits, atomic replacement
and removal while a capture remains alive. Each actual range/stream read and
metadata observation first reopens the leaf through its retained authorized
parent using the strict read profile, checks the captured identity and
observation, and holds that short handle through the operation's final checks.
A writer already open causes an acquisition error before bytes are returned;
replacement or removal invalidates the old binding. The short handle prevents
writes and replacement during that operation and is dropped when it returns,
including each stream chunk. Initial capture payload/control hashing uses the
same strict file profile. This avoids relying on Windows last-write timestamps
while an external writer is still open. Complete supplied bodies are checked
against the captured digest by `validate_file_bytes`; individual ranges do not
claim a new whole-file digest check. Bytes already returned remain owned by
that earlier result. Directory/owner-root handles retain their no-delete fence;
document, credential and database profiles keep their existing sharing rules.
The Source profile grants no write/delete rights to its own handle and does not
permit an alias, reparse point or hardlink. See
[Windows file sharing](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
and [last-write timestamps](https://learn.microsoft.com/en-us/windows/win32/sysinfo/file-times).


scope.project_owner_binding(limits, context) returns the ProjectId, brand, complete physical owner-root identity, directory kind and mode/read-only observation. Its limits bound brand and encoded record bytes. The selected child's identity does not replace the owner-root identity. The GOROOT01 codec is data only: decoding it does not construct a scope or a retained handle. Independent Unix and Windows record fixtures check field ordering, truncation, malformed tags and configured length limits.

The affected tests cover existing and absent children, strict names and parents, independent contexts, owner kinds, length/path/access bounds, root and parent substitution, growth, read-only files, cancellation, deadlines and record decoding. Native Windows and macOS execution is separate from Linux verification.
