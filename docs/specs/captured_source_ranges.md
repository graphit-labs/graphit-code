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

CapturedSourceReader reads bounded ranges through files opened from its captured source. PROJECT and private Installation reads use retained native root and directory handles and the original Arc<SnapshotContext>. General-source reads retain their existing captured-root observations.

SourceRangeRequest::new(file, start, length) borrows a CapturedSourceFile and validates checked endpoint arithmetic against its captured length. read_ranges(plan, AggregateReadLimits) requires every file to belong to that exact reader. Another capture of the same path is a different reader.

AggregateReadLimits::new(max_ranges, max_payload_bytes) validates positive bounds for range count and total returned payload bytes. The plan is validated before payload reads. Duplicate and overlapping ranges stay distinct and preserve input order. SourceReadLimits::new(range_bytes, block_bytes, stream_bytes) bounds each range, buffer block and stream by the data actually read. Files and concurrent operations use normal ownership and the descriptor mutex.

The original SnapshotContext carries its absolute deadline and cancellation signal. It does not reserve or count I/O. Descriptor and namespace observations are checked around reads and once more for every requested file before the batch is returned. Selected PROJECT reads validate that FILE's root/ancestor chain and applicable present or absent Git/custom controls, including scopes before the selected subtree. Other FILES and sibling controls are checked by explicit global source validation. Cancellation, timeout, local mutation, short EOF, overflow or a configured limit failure returns no partial batch. These checks do not make several files an atomic snapshot.

CapturedSourceFile::read_range uses the same engine for one request. RetainedSourceRange exposes AsRef<[u8]>, len and is_empty and owns only its returned Vec<u8>. Keeping these bytes does not retain the source snapshot, request context or open files. A stream retains its open file and shared context until the stream drops. A consumer can retain that owner directly, including through Bytes::from_owner when its library supports that API.

An empty plan checks the context and returns an empty vector without payload reads. A zero-length range at or before captured EOF is valid and returns an empty owner after physical validation; a start beyond EOF is invalid. Empty-buffer and stream EOF reads also retain the local physical and Original checks. A partial or abandoned stream is not global validation. Consumers validate their complete selected input set before publication: a changed unrelated selected FILE can leave one local read valid while invalidating the final global check. When the required contents and digests have already been verified, binding revalidation checks the captured observations without hashing the same bytes again. Whole-content revalidation remains available when fresh content verification is needed.
