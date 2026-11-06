---
title: Captured PROJECT source
status: private-integration-pending
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

CapturedPublicationScope::project receives ProjectPaths, ProjectId, ScopeLimits and Arc<SnapshotContext>. CapturedSourceReader::capture_project receives that scope, a strict relative selection, expected kind, SnapshotPolicy, SourceReadLimits and the current shared context. Metadata, files, streams and retained ranges use the same captured backing. `CapturedSourceReader::matches_binding(&SnapshotBindingRecord)` compares the stored full snapshot binding against that retained backing without I/O, allocation or admission; source/canonical path, kind, policy and node observations must match. It does not replace authority, Original or physical rechecks before access. Public PROJECT factories reject Installation scopes; the common private engine also supports selected Installation subtrees.

A selection rejects empty paths, dot/parent components, repeated or trailing separators, NUL and absolute paths. Windows additionally rejects slash and colon aliases. Non-Unicode native names, including Unix invalid UTF-8 and Windows unpaired UTF-16, fail explicitly. Paths use JSON strings with exact Unicode spelling; no OS/units alternative is decoded. The captured root uses an empty suffix internally; a selected root file can be read by that suffix, while a directory cannot be opened as a file.

Relative data admitted by `GeneratedNode`, `PayloadOverlay` and `RetainedFileSet`
also retains the per-entry 64 KiB native path bound: Unix bytes or twice the
Windows UTF-16 unit count. The Unicode string's UTF-8 bound remains independent.
Generated nodes and overlays check the original path before normalizing admitted
interior dots or redundant separators; aggregate metadata bounds charge the
canonical stored path. Retained selections and decoded receipts require canonical
spelling and do not normalize malformed input. A larger aggregate retention bound
does not enlarge the per-entry native bound.

CopyPolicy::RejectFileLinks is required. Symlinks, magic links, reparse points, hard-linked files, unexpected types and observable filesystem crossings fail before selected payload reads. Capture first inspects the selected subtree, checks tree and retained-byte limits, then hashes selected files with one reusable buffer. Generic tree captures keep their directory handles through readers, files and streams; fresh relative opens check namespace continuity. Selected PROJECT captures use the active-chain lifetime described below. Returned ranges own their bytes and release the capture independently.

Linux uses rustix openat2 with BENEATH, NO_SYMLINKS, NO_MAGICLINKS and NO_XDEV. An unsupported strict open returns Unsupported. macOS uses rustix openat/NOFOLLOW and observable filesystem identities. Windows uses generated windows-sys relative-handle APIs and retains ancestors without share-delete during their owned IO. These implementations do not claim identical mount semantics across platforms.

SnapshotContext::new(deadline, cancellation) returns the original context directly. Arc clones share its request identity, cancellation and absolute deadline. Contexts made from equal inputs remain distinct. It carries no I/O reservations or counters. Handles use try_lock: contention returns WouldBlock and poison returns Other. Malformed input is InvalidInput, observed mutation InvalidData, missing selection NotFound, configured limit excess FileTooLarge and unsupported native operations Unsupported. An error exposes no partial reader or range batch.

## Ancestor bindings for a published copy

`ArtifactWriter::read_published_copy` selects a confirmed Copy receipt under the
catalog writer and captures its physical metadata without retaining or reading
payload bodies. The verified receipt supplies the digests after the complete
layout and file observations match. This capture keeps metadata node, path and
depth limits, but does not charge the corpus's physical lengths against bytes it
does not read. Each SourceNode and opened file still reports its exact physical
length. Ordinary capture still reads and hashes payload, even with no retained
paths, and keeps its payload byte limit. Actual copy operations and later
[ranges and streams](captured_source_ranges.md) keep their own byte bounds;
manifest JSON and native Graph decoder limits remain independent.
Binding revalidation also checks metadata without charging body bytes. Explicit
content revalidation recaptures and hashes payload under its actual byte limits.

A captured subtree retains the identity, directory type and permission mode of
its ancestors using the same DirectoryBinding as native record chains. Activity
outside that subtree can change an ancestor directory timestamp or size without
changing the selected publication. Such activity must not invalidate the reader.
Fresh relative opens still reject a replaced ancestor or changed binding.

The selected root, its complete inventory and captured files retain their full
observations. Changes inside the selected root, including same-length file
replacement or content mutation, remain errors; the Original and scope checks
remain active. This distinction does not relax a selection that captures the
PROJECT root itself.

The confirmed-copy regressions in [published_copy.rs](../../crates/graphit-core/src/artifact_store/native_source/tests/published_copy.rs) exercise sibling activity under the shared brand directory and independently reject a replaced ancestor and a changed captured file. The sibling case failed with InvalidData before the fix.

## Selected documents

capture_selected_project(scope, criteria, selection_limits, read_limits, context) combines an optional subtree, individual extra paths and README discovery. None selects no subtree; the literal subtree dot selects the owner root. The union deduplicates observed relative paths. Missing initial subtrees/extras are optional; a subtree file or extra directory fails capture.

Extensions have one structural leading dot. Matching compares Path::extension against the suffix using ASCII case folding and exact Unicode spelling. A non-Unicode selected basename is rejected even if its extension is valid Unicode. README discovery chooses the smallest observed UTF-8 name with an ASCII readme stem and an allowed extension, then applies ignore rules. If that first candidate is ignored, discovery returns no README.

The caller supplies protected defaults and custom/Git control names. The ignore SDK preserves precedence and descendant scopes. Pruning happens before child enumeration and control reads. Controls open relative to retained root handles. Equal custom and Git control names are read once in the Git class; duplicate raw inputs still count toward input limits.

ProjectSelectionLimits bounds raw criteria count/bytes, each observed path, each selected file, each control file and recursion depth. Depth above 256 is invalid. There is no corpus file/directory count quota, aggregate byte/path quota or scan-work accounting. Individual path checks also apply to unsupported entries and necessary missing-path observations. A complete inventory determines the selected union before document bodies are read. Ignored and out-of-subtree bodies stay unopened; an empty selection produces an empty snapshot. Selection still retains selected document bodies for the current Knowledge consumer; this cleanup does not provide lazy payload retention.

Selected snapshots retain their selection criteria and physical witness rather than a generic copy/retention budget. Their binding records still observe RejectFileLinks. Payload publication rechecks delegate to the actual snapshot so selected ignore and path criteria remain in force. Generic snapshots keep their explicit active SnapshotPolicy.

Selected inventories and controls retain physical observations, names/kinds and digests, without keeping one open handle per corpus directory or control file. The original birth-root backing remains owned. Discovery, hashing and revalidation reopen relative to that root, check every ancestor and leaf against captured observations, and retain only the current ancestor chain until its IO finishes. Windows keeps that active chain without share-delete; between operations a replacement is detected by receipts rather than prevented by a permanent corpus pin. Recapture can run while the first selection remains alive without duplicating corpus-wide descriptor ownership. This does not make a multi-file snapshot atomic or replace Original, scope, ignore, link, mutation or digest checks.

Directory observations use a per-capture native-path index shared with the witness. The index preserves inventory traversal positions and shares the paths already held by those receipts. Selected files are unique and retain native path ordering; ordered component-prefix lookup identifies their ancestor nodes. Discovery, relative reopen and witness validation resolve observations through that index instead of repeatedly scanning the complete inventory. The index holds metadata only; the same Original, physical observations and active IO guards govern every read.

Opening or reading a selected FILE checks its Original and scope, each ancestor's physical identity, directory type and captured permission mode, and the complete current leaf observation around IO. Creating or removing an unrelated sibling file or directory can change ancestor size, timestamps or link counts without changing that binding; those namespace effects alone do not invalidate the local FILE. Capture and explicit global inventory/content/publication checks keep their full directory observations. Its applicable Git/custom controls include the root and every ancestor before the selected subtree. Present controls retain their captured observations; missing controls must still be absent, and only NotFound establishes absence. Equal configured control names identify one physical control. Each directory receipt identifies its zero to two present controls without scanning controls from other scopes. A changed sibling FILE or sibling control does not by itself invalidate a local FILE read. Replaced or symlinked ancestors, changed ancestor permissions, changes to applicable controls or to the FILE itself remain errors. Empty-buffer and EOF reads retain these local checks. A partial or abandoned stream establishes no global source validity.

The retained inventory includes directory observations, control digests and selected file observations. Explicit `revalidate_bindings` checks the complete selection before and after its node checks, detecting additions, removals and optional-presence changes. It does not repeat that global inventory scan per node or per FILE chunk. Consumers retain whole-content validation at operation EOF/preparation and before publication effects. A locally readable FILE cannot authorize publication of a snapshot whose sibling changed. These operations do not promise an atomic image of several files.

The FS DirectoryEntries API preserves native names and directory-entry kinds. `next_name(max_component_bytes, before)` and `next_entry(max_component_bytes, before)` enforce the concrete component bound before copying the native name. The operation check runs for each entry, including Windows entries already buffered without a new native query. Core checks the full observed path before retaining an inventory entry or joining its path; configured path excess remains FileTooLarge. Native regularity and physical file identity are checked on the selected handle. No allocation-admission callback, raw descriptor or authority-reconstruction API is exposed.

## First publication ordering

Complete the PROJECT brand/runtime bootstrap before the first selected Source
capture, under the same Original. `ArtifactStore::prepare_project_runtime_with_context`
establishes private control directories without a writer lock, lock file,
manifest or record. It is idempotent and preserves native scope bindings.
The publication acquires its own actual writer. Creating a new brand entry in the owner
root after capture is a Source change and remains rejected by the real guard.
When the actual brand already exists and the protected ignore policy prunes it,
control and Knowledge record-directory creation inside it preserve the captured
selection. These effects do not publish an index.

The Knowledge factory supplies the real PROJECT/version binding and matching
configuration brand. `check_request` only validates the Original; it does not
grant PROJECT access. The causal cases in
[project_source_publication_order.rs](../../crates/graphit-core/tests/project_source_publication_order.rs)
exercise the Core APIs without recapture or relaxed inventory checks. They
cover a late brand birth, an existing pruned brand, preparation while a real
writer is held, reusable physical scopes with the current operation context,
and cancelled, expired or physically replaced projects before creation. A scope
does not grant caller authorization; Knowledge retains its actual read/write
admissions and the same Original around preparation and Source capture. The updated
API's qualification is tracked in `tsk-9d92`; SDK publication and Foreground
composition have their own consumer tests.

## Progressive path discovery

`selected_project_paths` and `selected_project_changed_paths` reuse the same
Core selector, ignore controls and directory walker without opening or hashing
document payloads. The latter restricts enumeration to the actual watcher
paths. The returned relative paths are selectors, not filesystem authority.
The AST and Knowledge producers capture each FILE through the existing admitted
Source reader before preparing and committing its results. A full pass can
therefore publish the first FILE without hashing the remaining corpus. Content,
identity, Unicode-path, confinement, control-file and Original validation remain
at the corresponding capture/publication boundaries. Completed enumeration alone
does not authorize pruning after a failed or yielded FILE tail.
