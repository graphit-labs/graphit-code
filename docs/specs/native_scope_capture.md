---
title: Captured Project and Installation roots
status: private-integration-pending
task: tsk-2706
references:
  - target: {type: task, id: tsk-2706}
    relation: implements
  - target: {type: task, id: tsk-1ed0}
    relation: relates_to
---

# Captured Project and Installation roots

A CapturedPublicationScope records the brand, typed owner, allowed paths and physical directory observations. Its factory receives an Arc<SnapshotContext> and ScopeLimits, opens the existing owner directory read-only, and retains that real handle. Scope capture creates no files or directories and changes no permissions.

The shared context supplies cancellation, the operation deadline and the bounded payload I/O budget. Cloning its Arc preserves the operation. A context rebuilt from the same deadline and signal has a different request identity.

ScopeLimits bounds allowed roots, each native path, total stored path bytes and observations. Native paths preserve raw Unix bytes or Windows UTF-16. Configured paths and decoded records cannot reconstruct retained handles.

The native_source module supplies the common walker, hash loop, revalidation and opened-file backing for both owner kinds. Public capture_project and capture_selected_project reject Installation owners. Core's private capture_native_scope accepts an existing captured scope, strict owner-relative selection, expected kind, policy and read limits. Only selected files enter payload reads; sibling bodies stay unopened.

Directory and file handles remain owned by scopes, readers and retained results until their last owner drops. Windows ancestor handles omit share-delete while retained. Relative opens and fresh binding observations reject replacement roots, links, unexpected types and filesystem crossings. Linux uses rustix openat2; macOS uses rustix openat and filesystem identity observations; Windows uses generated windows-sys handle APIs. Platform-specific behavior requires execution on that platform.

Cancellation returns Interrupted, deadline expiry TimedOut, missing objects NotFound, malformed input InvalidInput, configured path or payload limit excess FileTooLarge, and changed bindings InvalidData. Busy descriptor locks return WouldBlock; poisoned locks return Other. Failed reads return no partial result. These are cooperative checks around native operations.
