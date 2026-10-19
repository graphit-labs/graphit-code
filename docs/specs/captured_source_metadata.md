---
title: Captured source metadata
task: tsk-2706
references:
  - target: {type: task, id: tsk-2706}
    relation: implements
---

# Captured source metadata

CapturedSourceFile::last_modified returns the SystemTime observed through the retained file descriptor. It checks the shared SnapshotContext, locks the descriptor with try_lock, revalidates the captured binding, compares the complete file observation and checks the binding and context again before returning.

Identity, kind, length, mode, modification time and supported native change time participate in that observation. A replacement or metadata change is an error. PROJECT files keep their owner-root and selected-directory handles; general-source files retain their existing captured-root observations.

The operation reads no document body and performs no hashing. It remains available independently of the payload-read budget while the context and physical binding are valid. len and is_empty return captured values; last_modified performs a fresh observation and can fail.

SystemTime is returned directly, including a pre-epoch value when supported by the filesystem. Consumers perform their own fallible time conversion. Cancellation returns Interrupted, expiry TimedOut, a busy descriptor WouldBlock and a changed binding InvalidData. OS errors preserve their category. The returned timestamp does not replace whole-content revalidation.
