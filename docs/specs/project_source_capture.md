---
title: Captured PROJECT source
status: integration-in-progress
task: tsk-2706
references:
  - target: {type: task, id: 3Q1RADNF4Z9RB9HXQ4J9Q3JSAK}
    relation: derived_from
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-852b}
    relation: implements
  - target: {type: task, id: tsk-e25e}
    relation: supports
  - target: {type: task, id: tsk-2706}
    relation: implements
  - target: {type: task, id: tsk-5651}
    relation: supports
  - target: {type: task, id: tsk-0257}
    relation: implements
---

# Captured PROJECT source

This contract is for Rust consumers selecting and reading PROJECT files.
A capture is an immutable content manifest, with optional retained bytes. It
uses configured `Path`/`PathBuf` locations and ordinary files. Configured symbolic
links, hardlinks and locations on other volumes are supported. Several files
are not an atomic filesystem snapshot.

## Selection and authority

`PublicationScope::project(&ProjectPaths, ProjectId)` supplies the logical
PROJECT and its configured paths. `CapturedSourceReader::capture_project`
receives that scope, a relative selection, expected `NodeKind`, `SnapshotPolicy`,
`SourceReadLimits` and the operation's `Arc<SnapshotContext>`. It rejects an
Installation scope. The generic `capture` factory accepts a configured source
path directly.

The scope, path, manifest and digest are data. The consumer retains its real Auth
admission, module/version selection and Original through access, SDK effects
and delivery. A snapshot does not grant PROJECT access or bind later operations
to the allocation identity of the context that produced its data.

Native paths exposed through Source use Unicode strings, without lossy
conversion. The reader rejects a non-Unicode configured root. Relative
selections use Core's native path validation and normal components; rooted,
prefixed and parent-traversal components are invalid. An empty relative path
addresses the captured root. The document selector additionally accepts `.` for
the owner root. A directory cannot be opened as a file. Relative artifact and
ZIP entry boundaries remain separate from following a configured source link.

## Generic snapshots and retained bytes

`SnapshotPolicy::new(links, copy, selected, retention)` supplies explicit
payload and retention policies. `CopyLimits::new(bytes, nodes)` bounds file
content and entry counts for a payload read, without summing native path sizes.
Relative entry names retain their own validation at external payload boundaries.
`RetainedBytesLimits::new(bytes, files)` bounds retained file contents and the
selected file count. Unselected bodies are hashed without being retained.

`CopyPolicy::FollowLinks` is the configured-source policy. The separate
`RejectEntryLinks` policy is available where the caller actually requires
artifact-entry restrictions. Source does not require rejecting links for every
configured directory.

`SourceSnapshot::capture` reads files and records their lengths and SHA-256.
Every path requested by `RetainedFileSet` must have retained FILE bytes before
the snapshot is returned. An absent requested path is `NotFound`; a selected
directory is `InvalidInput`. An empty selected file is present with zero bytes.
This completeness check uses the completed manifest and retained map; it does
not reopen the paths. A failed capture exposes no partial snapshot.

Clones share the immutable manifest and retained byte storage. Removing or
replacing a physical pathname does not change previously retained bytes.
A fresh capture observes the new content and produces its corresponding
fingerprint. A fingerprint does not prove authority or pin a directory.

## Selected documents

`capture_selected_project(scope, criteria, selection_limits, read_limits, original)`
combines an optional subtree, individual extra files and README discovery.
The union deduplicates logical relative paths. `None` selects no subtree;
`.` selects the owner root. Missing initial subtrees and extras are optional.
A present subtree file or extra directory is an error.

A selector with a leading dot matches the file extension using ASCII case
folding; one without that dot matches the complete basename. Unicode spelling
is preserved. README discovery chooses the first eligible root basename in
native path order, then applies ignores. If that candidate is ignored, it does
not select a later README instead.

The caller supplies protected defaults and custom/Git control names. Ignore
precedence and descendant scopes use the ignore SDK. Pruned directories are
not descended into. Controls are read through ordinary paths with individual
byte bounds and regular-file checks. A missing control is optional; permission,
wrong-kind and other read errors remain errors. Equal custom and Git control
names reuse one control read. Raw duplicate criteria still consume input limits.

`ProjectSelectionLimits` has four positive fields:

| Field | Bounded input |
| --- | --- |
| `max_input_items` | Raw selectors, watcher changes and textual criteria |
| `max_input_bytes` | Their actual native-name/text byte lengths |
| `max_selected_file_bytes` | Each selected document, including bytes read |
| `max_control_file_bytes` | Each ignore control |

The selector has no recursion-depth policy or aggregate corpus file/directory
quota. WalkDir follows configured links and detects cycles. Optional or dangling
paths reported as `NotFound` during discovery are skipped; other walk failures
preserve their cause. Selected document bodies are read only after discovery.
Ignored bodies stay unopened. An empty selected union is valid.

`capture_selected_project` retains selected bodies. Its `_metadata` and
`_changes_metadata` variants still read and hash the selected files, but do not
retain their bodies. Selection coverage records logical subtree/extra/README
or change-path coverage for reconciliation. It is not a physical witness.
Changes to files or controls after a completed capture do not revoke its
retained data. A caller needing a fresh selection explicitly discovers again.

## Reads, streams and content integrity

`open_file` opens the selected ordinary file and owns it until its last file or
stream drops, even when the reader has been dropped. Its point-in-time file-kind
check rejects an already-present FIFO or other special file before a blocking
open. It does not promise continuous protection against pathname replacement.
Positional reads do not serialize through a shared file cursor.

[Ranges and streams](captured_source_ranges.md) define byte limits, checked
range arithmetic, batching and EOF behavior. A complete `stream(0)` from a
manifest with payload digests hashes the bytes it actually reads and verifies
length, SHA-256 and actual EOF before its last successful block. Changed content
fails even when its length is unchanged; identical content at a replaced
pathname is accepted. A completed stream stays completed after later changes.
A partial or abandoned stream is not a whole-file integrity proof.

Ranges and generated-metadata readers do not promise a whole-file digest.
`validate_file_bytes(relative, bytes, current_original)` explicitly compares a
complete supplied buffer with the recorded length and SHA-256. It uses the
current operation's cancellation/deadline; it can reuse a manifest after the
capture context has ended. Files and streams retain the context of their real
I/O operation. Generated metadata records declared files and lengths without
inventing payload hashes.

Malformed selections and wrong kinds fail explicitly; real I/O causes remain
available. Cancellation is `Interrupted`, an expired deadline is `TimedOut`,
content mismatch is `InvalidData`, premature stream EOF is `UnexpectedEof` and
configured content/range excess is `FileTooLarge`. Unsupported digest validation
on metadata-only generated output is `Unsupported`. These outcomes do not
promise rollback of other effects owned by the caller.

## Publication and incremental discovery

PROJECT runtime and corpus paths are configured independently of checkout
content. The consumer applies its protected ignore policy to checkout state.
Creating a derived corpus outside that selected content does not invalidate the
manifest or require a publication-before-capture ordering protocol. Auth and the
Original remain required at the actual publication operation; catalog/SDK CAS,
content verification and confirmed partial effects belong to their producers.

`selected_project_paths` and `selected_project_changed_paths` use the same
selector and ignore walker without reading document payloads. The latter
restricts discovery to the actual watcher changes. Enumeration supplies paths,
not permission or content integrity. AST and Knowledge then capture each FILE
under their own admission and validate the content their operation consumes.
A failed or cancelled tail does not establish the complete desired EOF required
for reconciliation or pruning.

## Compiler output entry

The App SCIP consumer reads the compiler's fixed `index.scip` output as an
external artifact entry. It rejects a symbolic-link entry with `InvalidInput`
before opening it, while configured source inputs continue to follow links.
This is a point-in-time leaf-entry check, without ancestor identities or retained
directory handles. The bounded SCIP parser validates the bytes actually consumed;
a regular-file observation alone does not certify protobuf content.

## Implementation and qualification

The maintained sources are
[`snapshot.rs`](../../crates/graphit-core/src/artifact_store/snapshot.rs),
[`source_reader.rs`](../../crates/graphit-core/src/artifact_store/source_reader.rs)
and [`selection.rs`](../../crates/graphit-core/src/artifact_store/native_source/selection.rs).
[`source_snapshot.rs`](../../crates/graphit-core/tests/source_snapshot.rs)
protects retention, content fingerprints, missing/wrong-kind selections, empty
files, configured links and cancellation/deadline. The logical selection test in
[`project_source_publication_order.rs`](../../crates/graphit-core/tests/project_source_publication_order.rs)
checks exclusion of checkout state and the global derived corpus.

The current Linux source qualification includes six snapshot integration cases
and that one selection case. Existing SourceReader tests cover ranges,
byte-integrity streams, request cancellation and the isolated FIFO subprocess.
These are source-test results, not proof of the installed final release or
native Windows/macOS execution. This documentation update requires no new
compilation; installed composition remains with the main migration coordinator.
