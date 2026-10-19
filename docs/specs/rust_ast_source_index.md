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

This reference describes the Rust library for acquiring indexed raw Source from
an already resolved local project. All six SDK fixtures passed in Main on Linux,
including the boxed owner representation. Strict affected Clippy also passed.
HTTP, MCP/CLI, imported contexts and the
authenticated Source handler require separate integration evidence.

Implementation: [source_index_locator.rs](../../crates/graphit-ast/src/source_index_locator.rs).
The raw format and decoder are defined in
[Rust AST and parser contracts](rust_ast_parsers.md#indexed-raw-source-snapshots).

## Inputs and ownership

`open_local_source_index(installation, project, &original, limits).await` accepts
the existing Auth `ScopeCapture` for the installation and `ProjectCapture` for
the selected project, the original `Arc<SnapshotContext>`, and Core
`NativeDocumentFileLimits`. Captures borrow the real live scope and persisted
binding record. The project capture also borrows its resolved `VersionContext`.
Limits come from the caller's policy; this library supplies no implicit limit.

The caller authenticates and authorizes acquisition and delivery. Scope
observations grant no authority. The library checks installation/project owner
kinds, matching brands, the version's project root, each record's bounded match
against its live scope, and live scope revalidation. Each operation uses the
original cancellation token and optional deadline through
`ScopeContext::from_snapshot`. A service original without a deadline retains
those same scope checks and cancellation; no finite default is invented.

The locator is exactly:

```text
<captured installation owner root>/ast/<captured ProjectId ULID>/source.lance
```

The typed ProjectId comes from the captured project owner. Paths, storage URIs,
branch names and installation overrides are not user parameters. The selected
branch comes only from `project.version.lance_branch()`: empty encoding maps to
SDK `None` (Default), and nonempty encoding maps to `Some(encoded.as_str())`.
The locator performs no Git, HEAD, refs or inventory lookup.

## Acquisition and errors

Core's generic `CapturedDocumentDirectory` captures the installation root. Each
of the three fixed child components is observed with no-follow metadata and
captured under a revalidated parent. Canonical paths must equal the derived
paths. Symlinks and non-directories fail as `UnexpectedEntry`. The final capture
owns handles for all native ancestors.

| Observation | Result |
|---|---|
| Exact child absent before its presence is observed, with valid parent/scopes/original | `SourceIndexPresence::Absent` |
| Corpus and selected generation acquired | `SourceIndexPresence::Available(Box<LocalSourceIndex>)` |
| Scope kind, brand, binding or version root inconsistent | `InvalidScope` or actual bounded IO error |
| Canonical path differs or SDK URI is not strict UTF-8 | `InvalidLocator` |
| IO, permission, cancellation, deadline or disappearance after presence | `Operation(io::Error)` |
| Dataset opening or branch capture fails | `Storage(Box<BranchError>)` |
| Raw reader rejects schema, rows, key or SDK scan | `Source(IndexedSourceError)` |

`Error::source()` preserves underlying IO, storage and reader causes. A failure
after presence is observed cannot become absence. Reads create no directories,
datasets or branches and perform no repair. Directory and scope revalidation
bracket `LanceBranchStore::open` and `capture_target`. The SDK may observe an
inherited Default generation for an absent named target; the reader retains it
without materializing the target.

## Reading and delivery

The `Available` variant boxes its owner to keep the enum compact.
`LocalSourceIndex<'a>` retains the actual `BranchSnapshot`, final native
directory capture, borrowed installation/project captures and original request
Arc. `version()` returns the selected SDK generation. `branch_observation()`
returns the SDK's physical branch and inherited target observations.

`index.read(path, &original).await` revalidates before and after the shared
`read_branch_source` decoder. It never reopens latest or captures a new target.
The owned `IndexedSourceSnapshot` retains the selected generation and raw text.
Its `source(&original)` supplies `IndexedSource` to `inspect_source`. Empty text
remains available; zero rows and matching NULL retain their distinct
`FileAbsent` and `TextNotStored` reasons.

Keep the index alive through selection. Before delivery call
`index.revalidate(&original)` and perform actual caller authorization again.
Foreign request Arcs fail as `RequestMismatch`. The locator performs no physical
fallback. A downstream handler may use explicit absence reasons to capture the
requested file at its runtime-resolved project root and invoke Core selection.

## Verification and limits

[Real fixtures](../../crates/graphit-ast/tests/source_index_locator.rs) cover each
absent component without writes, two persisted project identities, raw and
empty/NULL text, Default/inherited/materialized named branches, retained old
generations, corrupt/wrong-kind entries, symlinks, scope mismatches, original
identity, pre-cancellation/deadline and a replaced corpus directory. Setup uses
actual Core identity/scope/version producers and explicit SDK publication.
Publication is not performed by the locator.

Affected commands are:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test source_index_locator --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test source_index_locator --jobs 1 -- -D warnings
rustfmt --check --edition 2024 crates/graphit-ast/src/source_index_locator.rs crates/graphit-ast/tests/source_index_locator.rs
```

The initial Main attempt stopped in HTTP before AST compilation; after the HTTP
owner's correction and coordinated lockfile composition, the same affected Main
suite passed all six fixtures. The repeat suite after the boxed owner adjustment
also passed all six. Strict Clippy passed after the Auth owner corrected its
producer integration error. Evidence is retained in
`.build/rust-ast-source-locator/test.log` and `clippy.log`; earlier failed attempts
are preserved separately. Pathname SDK IO checks observation boundaries; it does not guarantee
continuous confinement during SDK operations. Dataset retention is not an
external GC pin. Request checks are cooperative, without hard preemption of SDK
work. These fixtures establish no injected mid-await race, permission-denial,
authenticated transport or Windows/macOS execution. Raw-format publication is a
separate producer.


## Enumerating captured Source paths

`LocalSourceIndex::paths(&original).await` enumerates the literal paths in the
same retained SDK generation. Keep the index alive for subsequent `read` calls.
This method revalidates the actual installation/project scopes, directory and
request Arc before and after enumeration. It neither opens latest nor changes
an inherited target into a materialized branch. Acquisition still performs no
external inventory or Git lookup; enumeration is a separate operation on the
already acquired snapshot.

Lower-level `indexed_source_paths(&ModuleSnapshot, &original)` and
`branch_source_paths(&BranchSnapshot, &original)` accept genuine Data snapshots.
The caller remains responsible for acquisition, scope and authorization. Their
owned strings are keys, without a restored snapshot or grant. Dataset retention
also supplies no external garbage-collection pin or proof of current HEAD.

The full snapshot schema must match `source_schema`: ordered non-null Utf8 path
and nullable Utf8 source, with module `ast-source` and format `1` metadata.
The SDK scanner projects only `path`, with no predicate or two-row lookup limit.
The library does not copy or decode source bodies. Enumeration consumes every
batch through EOF before returning. A late SDK, decoding or Original failure
returns an error, never the paths accumulated so far.

Returned paths retain their exact case, Unicode, punctuation and whitespace and
are sorted by Rust's literal string order with an in-place sort. Empty or NUL
keys, null path values and duplicate keys across batches are errors. The path
projection's name, type and nullability are checked without casting or rewriting
SDK output. Caller-owned reservations preserve `TryReserveError` through
`IndexedSourceError::Allocation`; this does not promise that every SDK allocation
is fallible. IO and SDK causes remain available via `Error::source`.

A row with source NULL still contributes its path. `read` on that same index
then reports `TextNotStored`; available empty text remains available. An actual
zero-row index returns an empty path list. `SourceIndexPresence::Absent` remains
a distinct acquisition outcome. In particular, `index_source=false` is not a
reason to discard keys or publish an empty semantic corpus. A later assembler
must obtain an actual original through its supported capture journey or report
unavailability before publication.

Original checks bracket SDK awaits, batch/row decoding and ordering. They are
cooperative checks, not a hard-preemption or new work quota. Caller authorization
must be rechecked before delivery and future effects. The captured inventory
itself does not revalidate a new raw HEAD, publish NN vectors or bind a model.

The added inventory fixtures cover literal keys, NULL/empty bodies, a corpus
crossing SDK batches, corrupt/duplicate keys, Module/Default/named/inherited and
retained old generations, actual captured scopes and Original/control failures.
A real temporary Dataset data-file removal checks SDK errors. The private
collector test supplies a projected batch followed by an actual SDK error to
prove EOF consumption; it does not claim a native late-read race.

Affected commands are `cargo test --locked -p graphit-ast --test indexed_source --test source_index_locator`,
`cargo test --locked -p graphit-ast --lib inventory_stream_tests` and
`cargo clippy --locked -p graphit-ast --lib --test indexed_source --test source_index_locator -- -D warnings`.
Linux qualification passed all 15 indexed-source tests and all eight locator
tests, including a duplicate key observed in distinct SDK output batches. The
private EOF collector test also passed; strict Clippy, formatting, diff and local
links passed. Windows/macOS are UNRUN. Evidence is retained under
`.build/ast-source-inventory/`. Earlier ENOSPC and fixture-compilation failures,
and complete baseline-plus-delta recovery, remain separate historical evidence.
This qualification does not deliver NN publication or an authenticated caller.

The optional-deadline extension passed all nine affected Linux locator tests,
including a real service original that reads the retained Source generation and
then rejects cancellation. Strict Clippy passed for the affected library and
Source/NN targets. Windows/macOS are UNRUN for this extension; attempts are
retained in `.build/ast-option-deadline/`.
