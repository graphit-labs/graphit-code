---
title: Official Lance integration
description: Official SDK versions, native local paths, branches, independent exports and integrity limits.
content-type: reference
audience: developers
updated: 2026-10-30
references:
  - target: {type: task, id: 01M66HYM6CGCND6FNA4807HCZE}
    relation: implemented_by
---

# Official Lance integration

Graphit uses the official `lance` 14.0.2 and corresponding SDK crates from
crates.io, without a Lance patch or generated Graphit source package. The exact
versions and registry checksums belong to `Cargo.lock`. Direct ObjectStore
consumers use 0.14.2. Arrow 58 remains the Lance-facing family; the separate
Arrow/Parquet 60 adapters are intentional. Upstream's namespace HTTP client
0.13.0 and Arrow scalar/statistics helpers 58.0.0 have their own versioning;
they are not a second Lance 13 SDK.

Release packaging and the updater require Lance 14.0.2 from crates.io, with no
Graphit fork provenance file. The published crate omits its repository license;
the unmodified notice and its upstream revision are retained under
[`crates/graphit-data/licenses/`](../../crates/graphit-data/licenses/README.md)
and included in the runtime package without downloading sources during packaging.

Ladybug and llama.cpp have independent builds and provenance. Removing the
Lance fork does not replace their engine, SDK or `cxx` bridges. See
[native packaging](native-runtime-packaging.md).

## Local paths and remote storage

Local producers pass a Unicode native filesystem path directly to the SDK.
Project-relative selections resolve under the existing admitted canonical
parent; the SDK is not asked to normalize dot segments in persisted branch
bases. Memory, Task, USER Memory, Knowledge, imported Knowledge, received
packages and clone destinations use that representation. Non-Unicode paths
fail explicitly. SDK locators remain data, never filesystem or PROJECT authority.

Windows SDK arguments use the existing native-path adapter while public corpus
locators and recovery keys retain their original spelling. Knowledge manifest
capture translates the SDK prefix back under that retained corpus, then uses
the existing captured directory traversal and protected leaf read. It does not
canonicalize the manifest file or follow a symlink to another leaf. Readonly
Source and semantic pins compare their retained locators; reopening selects
the captured physical branch and version through the same SDK adapter.

S3 selectors, endpoints, credentials, branches and provider authorization retain
their existing contracts. No local copy, synchronization queue or fallback is
introduced for remote authoritative tables. See
[S3 and UI networking](../guides/s3-and-ui-network.md).

## Branches and initial Knowledge history

Branch creation uses official `Dataset::create_branch`. Graphit does not attach
custom properties to its Clone transaction. The first Knowledge create already
stores the synchronization summary on Main version 1. For a branch's initial
Clone of that version, the history reader follows only the SDK reference to
Main of the same admitted table and reads that existing transaction's summary.
It does not open a caller-selected URI or copy document bodies.

The named event retains its actual Clone UUID and timestamp. Subsequent
synchronizations keep their own transaction properties. Confirmed Main/named
effects, recovery keys, CAS, authorization rechecks and cancellation boundaries
remain observable, including errors after a successful publication. There is
no extra summary commit, duplicated history catalog or branch implementation.

## Independent publication

`ModuleSnapshot::spawn_native_clone_local` delegates to official
`Dataset::deep_clone` at the captured physical version. The SDK copies data,
deletions, row IDs and index files and writes the output metadata. A shallow
clone is not an independent publication. The completed package has no bases
pointing at its producer and remains readable by a fresh consumer after the
producer directory becomes unavailable.

The existing worker owns its temporary directory until SDK completion. Callers
join it before cleanup/publication and preserve Original, authorization,
cancel/deadline checks and causal errors. Cancellation does not promise hard
interruption of a started synchronous SDK filesystem copy. Blob inputs remain
outside this local export contract.

## Existing datasets

Opening an existing table by its native path does not change its persisted
BasePaths. Referenced `file://` bases still fail upstream `deep_clone` with an
`EmptySegment` error. Unreferenced old bases do not contribute files to a clone.

The installation inspection opens existing tables and their current native
branches with the official SDK, without rewriting records or metadata. If no
current head references a file-URI base, no conversion is necessary. This leaves
all existing versions, domain revisions, history and embedding files untouched.

Inspection includes the distinct snapshots referenced by native tags and other
explicit pins still in use, as well as branch heads. Open and sample those
captured versions through the SDK; a successful head read alone does not qualify
historical snapshots. Tags retain their original branch/version references.
Inspection does not require scanning every unreferenced historical manifest.

Historical manifests and unused base entries remain intact. Raw SDK deep
cloning of an old snapshot that still references a file URI retains the upstream
limitation. Product exports capture current authorized heads; published Native
packages are already independent. A future installation with referenced
file-URI heads requires a separately verified one-time conversion through public
SDK APIs before cutover. No on-open converter, dual layout, compatibility
reader or URI repair is installed in the product.

## Integrity limits

Graphit preserves its own schema, input, namespace, digest, authorization, CAS
and Original checks. The SDK supplies decoding and storage errors for valid
envelopes with invalid protobuf content. It does **not** provide the removed
fork's universal checked bounds for arbitrary manifest, index and transaction
sections. A truncated-manifest fixture reproduced an arithmetic-underflow panic
in the official SDK's manifest reader. Do not describe malformed offsets or
lengths as guaranteed causal errors, or reuse the fork's old PASS as upstream
qualification.

There is no copied binary parser, pre-validation codec or generic panic-catching
layer. The former patch-specific bounds tests are retired; valid manifests,
inline indices, public decode errors and Graphit admission/error behavior keep
their functional tests. Corruption must never be reported as a healthy empty
corpus, but arbitrary corrupt SDK bytes can still panic instead of returning a
normal domain error. That residual upstream limitation remains explicit.
Lance 14.0.2 also panics in its JSON full-text tokenizer when fed a syntactically
invalid payload written outside Graphit's record writer. The record search tests
exercise Graphit's rejection of valid JSON with an invalid record shape.

## AST embedding checkpoints

Conditional enrichment selects exact `(uid, data_key)` pairs under the captured
publication namespace. Graphit uses the SDK's native UID `IN` selection grouped
by data key, combining groups as a balanced OR tree. This preserves the exact
pairs; two independent `IN` lists would also admit their unwanted cross product.
The SDK's recursive expression planner must not receive a linear-depth chain
for a large file. No batch-size quota or larger daemon stack substitutes for
that construction. Fingerprint comparison and the strict prepared commit still
prevent stale vectors from replacing newer content.

The `native_nn_json` regression exercises a real SDK checkpoint with thousands
of identities, including the same UID under distinct data keys, then queries
the published vectors. It complements the incremental/reuse and causal-error
consumer tests rather than changing their contracts.

## Affected verification

The maintained SDK tests are `lance_integrity`, `lance_native_branch_clone`,
`lance_native_clone`, `lance_native_snapshot` and `lance_module_store`. Knowledge
tests cover initial empty/named publication, history, recovery, conflicts,
revocation, cancellation and deadlines. Consumer verification includes native
package export/import and the Memory/Task S3 runner:

```sh
cargo xtask cargo test --offline --locked -p graphit-data --test integration -- lance_integrity:: lance_native_branch_clone::
python scripts/test-record-storage-s3.py --test-binary PATH_TO_APP_TEST_EXECUTABLE
```

The S3 runner and `cargo xtask` test commands give unoptimized Rust test threads an 8 MiB stack, unless
`RUST_MIN_STACK` is already set. The compound fixtures nest inference callbacks,
concurrent record mutations and SDK/S3 polling; a debugger measured over 2 MiB
on the failing default libtest stack, including over 1 MiB of fixture frames.
This is a test-process setting, not a daemon memory limit or runtime change.
All storage, authorization, concurrency, cancellation and deadline assertions
remain active.
For a direct compound `cargo test` invocation, supply `RUST_MIN_STACK=8388608`
in that test command's environment. Do not set it as a daemon configuration.

The S3 runner uses isolated Moto S3/STS and local Broker contracts, without an
external account. It does not qualify real AWS IAM or native Windows/macOS.
Compilation, executed tests, installed behavior and remote CI/publication must
be recorded separately; none implies another.
