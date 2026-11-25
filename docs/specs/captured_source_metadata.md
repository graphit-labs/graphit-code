---
title: Captured source metadata
task: tsk-2706
references:
  - target: {type: task, id: tsk-2706}
    relation: implements
---

# Captured source metadata

`CapturedSourceFile` owns an ordinary open file through a shared handle. Its
`last_modified()` method checks cancellation and the deadline, calls
`File::metadata()?.modified()`, checks the operation context again and returns
the `SystemTime` supplied by the filesystem. It reads no document body, hashes
no content and acquires no descriptor mutex.

`len()` and `is_empty()` describe the length observed when that file was opened.
`last_modified()` requests current metadata from the same open file. Changes to
permissions, timestamps, directory names or other metadata do not themselves
invalidate the operation. An open handle follows the filesystem's normal
semantics if a pathname is subsequently replaced; it is not reopened merely to
compare the new pathname with the old object.

Configured roots and parent directories may be legitimate links or reside on
another volume. Source access does not retain a directory ancestry or compare
ancestor identities. Relative member names remain validated, and opening a
member still requires an ordinary file. Neither metadata nor a path grants
project authorization; that decision belongs to the authenticated consumer.

The timestamp is returned without normalization, including pre-epoch values
when supported. Consumers perform their own fallible time conversion.
Cancellation returns `Interrupted`, expiry returns `TimedOut`, and filesystem
errors propagate through the normal I/O result. There are no synthetic
`WouldBlock` or changed-binding errors from this metadata method.

A modification timestamp is an observation, not proof of content integrity.
Full payload reads with an expected digest continue to verify their actual
bytes; ranges preserve their bounds and read errors. Generated-metadata reads
do not claim to have verified a payload digest. These checks protect the
requested content rather than requiring equality of all filesystem metadata.
