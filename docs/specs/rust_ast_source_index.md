---
title: Rust local Source index acquisition
doc_type: technical
references:
  - target: {type: task, id: tsk-24b6}
    relation: implements
  - target: {type: task, id: tsk-bcf2}
    relation: uses
  - target: {type: task, id: tsk-49fe}
    relation: implements
  - target: {type: task, id: tsk-88f1}
    relation: implements
  - target: {type: task, id: tsk-2f37}
    relation: derived_from
  - target: {type: task, id: tsk-4c40}
    relation: derived_from
---

# Rust local Source index acquisition

`graphit_ast::source_index_locator` opens indexed raw Source for an already
resolved project. The caller authenticates and authorizes acquisition and delivery.
Logical scope data and SDK snapshots do not themselves grant access.

Implementation: [source_index_locator.rs](../../crates/graphit-ast/src/source_index_locator.rs).
The raw schema and decoder are specified in
[Rust AST and parser contracts](rust_ast_parsers.md#indexed-raw-source-snapshots).

## Inputs and locator

`open_local_source_index(installation, project, &original).await` accepts
`&PublicationScope`, Auth `ProjectSelection { scope, version }` and the original
`Arc<SnapshotContext>`. It checks Installation/PROJECT owner kinds, matching
brands and the VersionContext's project root. `LocalSourceIndex` retains those
logical selections, the Original and the SDK `BranchSnapshot`.

The dataset is `project.scope.runtime_root().join("ast/source.lance")`. Under
the ordinary installation layout this is
`<installation>/projects/<ProjectId>/ast/source.lance`. Configured roots remain
configuration; paths, URIs and branches are not user-selected acquisition inputs.
The SDK target is exactly `project.version.lance_branch()`: empty maps to Default,
otherwise the encoded branch is supplied. No Git, HEAD, refs or inventory lookup
occurs in the locator.

The locator uses normal filesystem metadata and mature Lance APIs. Configured
symlinks, junctions, hardlinks and mount points follow platform semantics. It
retains no directory descriptors, ancestor identities, path-depth budgets or
physical binding records. An observed non-directory is `UnexpectedEntry`.

## Acquisition and delivery

| Observation | Result |
|---|---|
| Exact configured dataset is absent | `SourceIndexPresence::Absent` |
| Selected SDK generation is acquired | `Available(Box<LocalSourceIndex>)` |
| Invalid logical scope or foreign Original Arc | `InvalidScope` or `RequestMismatch` |
| Filesystem/request failure | `Operation(io::Error)` |
| Invalid Source schema or SDK storage failure | `Storage` |
| Invalid rows/keys, duplicate rows, decoder or scan failure | `Source` |

SDK failures after observed presence are errors. Reads create no directory,
dataset, branch, journal or cache. The SDK may supply an inherited Default
snapshot for an absent named target without materializing that target.

`version()` and `branch_observation()` expose the selected generation and actual
SDK physical/inherited branch observations. `snapshot(&original)` borrows that
same pin. `read(path, &original).await` uses `read_branch_source`, checking the
Original and logical selection before and after the operation. It never reopens
latest. Empty text remains available; no row and NULL retain distinct
`FileAbsent` and `TextNotStored` reasons.

Before delivery use `index.check_request(&original)` and recheck actual caller
authorization. A different request Arc is rejected. An original without a finite
deadline keeps its real cancellation; no deadline is invented. Explicit absence
may be handled by an authorized downstream physical Source reader. An SDK,
schema or Auth error must not become a physical fallback.

## Enumerating Source paths

`LocalSourceIndex::paths(&original).await` enumerates literal keys from that same
SDK generation. Lower-level `indexed_source_paths(&ModuleSnapshot, &original)`
and `branch_source_paths(&BranchSnapshot, &original)` accept genuine Data pins;
the caller owns acquisition and authorization.

The full schema must equal `source_schema`: ordered non-null Utf8 `path` and
nullable Utf8 `source`, with module `ast-source` and format `1`. Scanning projects
only `path`, with no lookup predicate/row cap, and consumes EOF before returning.
It copies no source bodies. A late SDK, decoding or Original failure returns no
partial path list. Keys preserve exact case, Unicode, punctuation and whitespace,
and sort by literal Rust string order. Empty/NUL/null/duplicate keys fail,
including duplicates across batches. No SDK output casting or normalization occurs.
Caller allocation failures keep `IndexedSourceError::Allocation`.

NULL source still contributes its path; later reading reports TextNotStored.
An empty corpus has an empty path list, distinct from absent corpus.
`index_source=false` does not discard file keys or publish empty semantic rows.
An assembler must acquire actual Original source or report unavailability.

## Verification and limits

[Locator fixtures](../../crates/graphit-ast/tests/source_index_locator.rs) cover
absence, raw/empty/NULL text, independent projects, Default/named/inherited SDK
pins, old generations, corrupt/wrong-kind storage, configured directory links,
logical scope mismatch and Original Arc/cancellation/deadline. Indexed-source
fixtures cover cross-batch duplicate keys and EOF errors. Current qualification
is tracked by the session coordinator; older directory-identity tests are obsolete.
SDK pinning is not an external garbage-collection lease. Cancellation checks are
cooperative; transport and native Windows/macOS execution need separate evidence.
