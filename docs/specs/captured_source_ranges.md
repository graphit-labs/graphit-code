---
title: Captured source ranges
task: tsk-2706
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: implements
---

# Captured source ranges

CapturedSourceReader records a content manifest at a configured path. It uses ordinary Path/PathBuf and File; configured links and other volumes are supported. It retains the real cancellation/deadline context for its I/O, without ancestor identities or directory handles.

SourceRangeRequest::new(file, start, length) borrows an opened file and validates checked endpoint arithmetic against the observed length. read_ranges requires files from the same reader batch. Another reader is a distinct batch resource; this is not an authorization boundary.

AggregateReadLimits::new(max_ranges, max_payload_bytes) bounds range count and returned bytes before reads. Duplicates and overlaps preserve order. SourceReadLimits::new(range_bytes, block_bytes, stream_bytes) bounds actual range, block and stream data. Positional reads allow concurrent ranges without a descriptor cursor mutex.

The held context checks cancellation and deadlines before and after actual reads and between blocks. Short EOF, overflow, cancellation, timeout or a limit error returns no partial batch. An opened file remains owned until its stream/file drops, including after its reader drops. Several files are not an atomic filesystem snapshot.

CapturedSourceFile::read_range uses the same engine. RetainedSourceRange owns only its returned bytes and exposes AsRef<[u8]>, len and is_empty; keeping it does not retain the snapshot, request or descriptors.

An empty plan checks the context and returns an empty vector. A zero-length range at or before EOF is valid; starting beyond EOF fails. Empty-buffer and EOF stream reads still check cancellation/deadline.

The manifest and retained content are data. Fresh authorized operations can reuse them without request-allocation equality checks. validate_file_bytes(relative, bytes, current) explicitly compares the bytes actually consumed with the manifest length and SHA-256. Consumers retain their real Auth admission and validate content where their operation requires integrity. There is no global directory/content revalidation API, ancestry receipt or private filesystem grant.

A complete `CapturedSourceFile::stream(0)` with manifest payload digests hashes
its actual blocks and checks length, SHA-256 and actual EOF before returning the
last successful block. Its opened file retains only the manifest node index; no
path or physical identity is retained for validation. A digest/length mismatch
returns `InvalidData` and stays failed. Premature EOF is `UnexpectedEof`; byte
limits, cancellation and deadlines still apply. An EOF probe catches growth
after open without rereading prior bytes.

A completed stream remains completed if the pathname or its contents later
change. A new stream reads and validates its own bytes. Identical content at a
replaced pathname is accepted. Partial streams (`start > 0`), positional ranges
and `capture_generated_metadata` do not promise a whole-file digest. Callers
using ranges must validate the consumed whole buffer explicitly when required.

`validate_file_bytes` hashes only the supplied immutable buffer and recorded
manifest under the current Original. A finished/cancelled capture request does
not invalidate that pure comparison. Opening files, positional reads and streams
retain the context of their actual I/O operation.

CapturedSourceReader has one representation check at construction: its configured
root must be Unicode for the SDK locator contract. An unrepresentable root
returns a clear InvalidInput without lossy conversion. This is not canonicalization
or confinement; native relative names and configured links keep their own
existing operation-specific contracts.

`open_file` checks the selected pathname's ordinary file type before the blocking
open, then checks the opened file type as well. An already-present FIFO or other
special file is rejected; this does not promise resistance to continuous hostile
replacement. The Unix regression runs the FIFO case in an isolated child with a
parent deadline, kill/join guard and parent-owned temporary directory.

An explicit `RetainedFileSet` requests every listed file. `SourceSnapshot::capture`
checks that each requested path has bytes in the completed manifest before
returning the snapshot. A missing path is `NotFound`; a selected directory is
`InvalidInput`. An empty file is present with zero retained bytes. These checks
use only the manifest and byte map produced by the read; they do not reopen or
revalidate paths. A failed request returns no partial snapshot.

Selection, ignore controls and generic retention are specified in
[PROJECT Source](project_source_capture.md). The direct integration coverage in
[`source_snapshot.rs`](../../crates/graphit-core/tests/source_snapshot.rs)
protects requested-file completeness, byte retention, empty files and configured
links; it does not assert directory identity or a filesystem snapshot.
