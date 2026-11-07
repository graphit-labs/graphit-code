---
references:
  - target: {type: task, id: tsk-67e7}
    relation: implements
  - target: {type: task, id: tsk-265d}
    relation: implements
  - target: {type: task, id: tsk-c623}
    relation: supports
  - target: {type: task, id: tsk-9613}
    relation: requires_contract
  - target: {type: task, id: tsk-41d1}
    relation: requires_contract
  - target: {type: task, id: tsk-7fc2}
    relation: requires_contract
  - target: {type: task, id: tsk-29fb}
    relation: requires_contract
  - target: {type: task, id: tsk-db94}
    relation: relates_to
  - target: {type: task, id: tsk-74e7}
    relation: requires_contract
  - target: {type: task, id: tsk-b2ca}
    relation: requires_contract
  - target: {type: task, id: tsk-8dd2}
    relation: requires_contract
  - target: {type: task, id: tsk-f5d5}
    relation: requires_contract
  - target: {type: task, id: tsk-dcee}
    relation: specified_by
  - target: {type: task, id: tsk-9ccd}
    relation: specified_by
  - target: {type: task, id: tsk-8b90}
    relation: uses
  - target: {type: task, id: tsk-613a}
    relation: derived_from
  - target: {type: task, id: tsk-a59d}
    relation: relates_to
  - target: {type: task, id: tsk-b129}
    relation: uses
  - target: {type: task, id: tsk-bd00}
    relation: uses
  - target: {type: task, id: tsk-8a17}
    relation: uses
  - target: {type: task, id: tsk-7240}
    relation: supports
  - target: {type: task, id: tsk-3ddd}
    relation: implemented_by
  - target: {type: task, id: tsk-be9c}
    relation: implemented_by
  - target: {type: task, id: tsk-1228}
    relation: implemented_by
  - target: {type: task, id: tsk-63c5}
    relation: relates_to
  - target: {type: task, id: tsk-612f}
    relation: relates_to
  - target: {type: task, id: tsk-4c15}
    relation: implemented_by
  - target: {type: task, id: tsk-a684}
    relation: implemented_by
  - target: {type: task, id: tsk-d7c3}
    relation: implements
---

# Native PROJECT Memory creation and current metadata

Reads use the admission's checked request once at each authority boundary.
Scans preserve admission and captured-directory checks around SDK awaits, EOF
and delivery; per-row decoding and in-memory history, catalogue and lexical
processing check Original cancellation and deadline without repeating physical
authority checks for each item. Directory probes validate their own captured
chain before and after opening the child. Fenced writes retain their explicit
obligation check, writer and publication/CAS rules.
History validation compares borrowed creation/revision snapshots and returns the
already owned current head after matching the complete chain, without copying
the Memory body for every revision.

Read capture opens the current dataset and consumes that new store with
`LanceBranchStore::into_target`. It selects the opened Main observation without
resolving Main a second time; retained stores still use `capture_target` to
refresh their head. Both Main and the selected branch must match the complete
Memory schema. Local capture/admission checks still surround opening and branch
selection separately. Remote capture retains current admitted transport parameters,
checks and the existing distinction between an absent dataset and failed selection.
Each new operation opens the current head; an already captured pin keeps its version.

Memory metadata/history and embedding-source scans consume one batch at a time.
Their Lance scanners use native `fragment_readahead(1)` and `batch_readahead(1)`
so parallel project jobs do not each pre-open and decode a large queue of files.
The batch row hint, filters, schema and row validation remain unchanged, and
every consulted stream is consumed to EOF. This backpressure does not limit the
number of files, rows, memories or revisions in a corpus. Captured snapshots and
guards remain alive when needed for generation consistency, embeddings and
publication; other modules' search/index scanners are unchanged.

Audience: Records, Data, Auth, Core and Now maintainers. Status: Source exported
in `graphit_records::memory_store` and qualified on Main/Linux under `tsk-3ddd`.
Native integration passed 10 cases and the
Records library passed 45 cases, including seven Memory boundary cases.
Five doctests and strict Records Clippy passed. The contract was closed in `tsk-9ccd`,
and Core Memory birth `tsk-8b90` is complete. Now integration remains pending.
Those results qualify the creation/current-metadata slice only. The catalogue
extension `tsk-be9c` exports `NativeMemoryStore::list`, `MemoryListFilter`
and `MemoryListRead` and is qualified on Linux. The affected native suite passed
16 tests, the library passed 46 (including eight Memory boundary cases), five
doctests passed, and strict Records Clippy and owned formatting passed.
The corrected canonical-duplicate fixture also passed its targeted rerun.
The receipts are in `.build/rust-records-memory/catalogue/validation.json`.
Windows/macOS, USER, Dream publication, search and full revision history remain
unqualified by this slice.

Lifecycle Source `tsk-1228` is implemented and qualified on Linux. The full
native suite passed 24 tests; the latest revision rerun passed eight tests with
16 filtered out. The library passed 50 tests, including 12 Memory boundary
cases; six doctests passed (four ordinary and two compile failures), and strict
Records all-targets Clippy exited zero. Formatting, diff, links and whitespace
checks passed. The receipt is
the lifecycle validation (`../../.build/rust-records-memory/lifecycle/validation.json`; historical local artifact unavailable),
which records command results and source/documentation hashes. Creation
and catalogue receipts above retain their historical counts and scope.

Writers initialize a missing derived lexical index under their actual ordinary/Fenced policy after validation, CAS/idempotency and encoded batch checks; Ready is reused for later mutations. Explicit ordinary Ensure/Refresh maintenance and readonly resolution have a separate [native Memory search contract](native_memory_search.md#publicação-explícita-do-índice-tsk-63c5), the historical `tsk-63c5` publisher/resolution is Linux-qualified with 45 distinct affected tests and strict exit0; lifecycle remains in the Task. Its index does not qualify lexical selection/search or extend the CRUD/catalogue receipts above. The [lexical query](native_memory_search.md#query-lexical-tsk-612f), `tsk-612f`, is Linux-qualified with 39 affected integration tests plus 18 affected library tests (57 distinct Memory cases), six Records doctests (four ordinary/two compile_fail), and scoped strict exit0. The eight passing focal cases are included in the 39, and the six doctests are not six executable Memory docs. SDK `batch_rows` remains a hint, not a guarantee of one-row FTS batches; the search contract preserves the failed-attempt history and exact coverage limits.

This first native corpus supports ordinary PROJECT creation, a complete current
record read, reobservation of an uncertain creation, and current metadata reads.
The [Memory contract](rust_memory_records.md) distinguishes qualified PROJECT
update/deletion/history from future
search, consolidation, USER and Dream obligations. The
[transaction proposal](rust_record_transactions.md) retains full conditional
publication and recovery obligations. USER storage and fenced Dream publication
require their own actual authority and producers. The [USER owner/layout data contract](native_user_memory_store.md) in `tsk-9613` is Linux-qualified for
Source data/layout: USER6 (four data/serde +two SDK) plus seven PROJECT causal
guards = 13 distinct functions, with scoped strict/fmt/diff0. It supplies no USER
CRUD, authority, healthy-empty result, semantic product or new doctest proof.
The [private domain-sharing progress](native_user_memory_store.md#private-domain-sharing-c623-progress)
of `tsk-c623` retains its 12-case helper proof, separate from `tsk-9613`/13.
The [ordinary USER APIs](native_user_memory_store.md#native-user-apis-source-implemented-validation-pending)
now have Linux Source with actual admission/capture, USER25/owner/default SDK
generation, immutable audits/markers and causal UserConfirmed/UserIndexConfirmed.
**Ordinary USER APIs are Linux-qualified**: the [c623 evidence](native_user_memory_store.md#c623-linux-qualification-and-evidence-boundaries) records 115 distinct functional passes
(USER SDK15/library5 +PROJECT SDK68/library27), final scoped strict0 and owned
fmt/diff0. Six Records doctests passed separately (four ordinary/two compile_fail),
without addition to the 115 functional cases. PROJECT wrappers
and historical 9613 receipts retain their scope; USER Fenced/automatic, references,
semantic and daemon/App remain separate deliveries.

## Authority and ownership

Records owns `crates/graphit-records/src/memory_store.rs`, its
`memory_store/{model,rows,native,read,write,catalogue,revision,history}.rs` submodules, `tests/memory_store`
and this page. The coordinator confirmed those boundaries. Core owns the
addition of `ProjectRecordDirectory::Memory` to the existing fixed birth API.
Data adapters, Auth, global exports, manifests, locks and build scheduling keep
their existing owners. Task's native codec and lifecycle belong to the Task
producer; Memory does not call its private helpers.

Every operation takes the original `Arc<SnapshotContext>` and a borrowed actual
PROJECT admission. Read requires `ProjectReadModule::Memory`; ordinary creation
and creation reobservation require `ProjectWriteOperation::Memory`. Other
module admissions, another original Arc and `FencedMemory` cannot authorize
ordinary creation. The consumer checks the original request, admission and
captured path before and after every awaited SDK boundary and before returning.
There is no caller-supplied project root, version selector, actor or grant.

Project identity comes from `StoreOwner::Project` on the admitted capture.
Requested logical version is `admission.project()?.version.version()`; only
`lance_branch()` supplies the SDK branch selector. A named snapshot can inherit
default rows, and materialization preserves those rows. Physical branch names
or the absence of an inheritance marker cannot establish an attempt's original
logical version. Actor comes from the write admission. `Actor::identifier()`
returns the ULID formed from the first 16 bytes of its originating digest; Records
persists its canonical uppercase 26-character encoding. This is a 128-bit
identifier truncation, not a bijection of the full SHA-256 value or a creation
timestamp. Authorship, idempotency actors and caller bindings use that identity.
The codec rejects the retired hexadecimal form and noncanonical aliases. An Actor
is not a USER subject; credentials, MACs, fences and tokens retain their existing
full-width security contracts.

These checks preserve actual admission and conditional SDK publication. They
do not establish atomic linearization between Auth revocation and the SDK
commit. There is no new guard callback, authority reconstructed from record
fields, or claim that full P-D has been qualified.

## Public operations and values

`NativeMemoryStore` has these associated asynchronous operations:

| Operation | Additional inputs | Result inside `Result<_, MemoryStoreError>` |
|---|---|---|
| `create(original, admission, input, limits)` | `CreateMemory`, `MemoryStoreWriteLimits` | The immutable initial `Memory` for that attempt |
| `get(original, admission, id, limits)` | `MemoryId`, `MemoryStoreReadLimits` | `Option<MemoryDetail>` from one complete pin |
| `reobserve_create(original, admission, input, limits)` | The same `CreateMemory`, `MemoryStoreWriteLimits` | `MemoryCreateObservation::Found(Memory)` or `Unresolved` |
| `observe_heads(original, admission, limits)` | `MemoryHeadReadLimits` | `MemoryHeadRead::NoCorpus` or `Published { generation, heads }` |
| `update(original, admission, id, input, limits)` | `MemoryId`, `UpdateMemory`, `MemoryStoreWriteLimits` | Event's exact `after: Memory` |
| `delete(original, admission, id, input, limits)` | `MemoryId`, `DeleteMemory`, `MemoryStoreWriteLimits` | Final tombstoned event's `after: Memory`, not a live current read |
| `history(original, admission, id, limits)` | `MemoryId`, `MemoryStoreReadLimits` | `Option<MemoryHistory>`, including deleted final state |
| `reobserve_mutation(original, admission, id, operation, limits)` | `MemoryId`, `MemoryMutation`, `MemoryStoreWriteLimits` | `Option<MemoryRevision>`; `None` is unresolved |
| `list(original, admission, filter, limits)` | `MemoryListFilter`, `MemoryStoreReadLimits` | `MemoryListRead::NoCorpus` or complete `Published { generation, memories }` |

`MemoryId` is a nonzero 128-bit ULID value. Display and persisted text use its
canonical uppercase 26-character form; parsing rejects zero, noncanonical
text and out-of-range values. Creation generates the ID in the store. An ID
constructor is logical data and admits nothing.

`CreateMemory` contains only the authored idempotency key, title, body,
qualified `MemoryAttributes`, ordered tags and optional authored references.
It accepts neither identity/revision/time/actor nor scope/version/generation.
Title, body, key and each tag must be nonblank; validation preserves their
literal UTF-8 bytes, order and duplicates. Classification and independent
mandatory/important flags use the existing attributes type.

The Rust input uses owned typed reference values: the existing closed
`ReferenceTargetKind`, project identity or a nonempty USER logical identity,
target ID, optional Knowledge context, relation and optional field. Conversion
to borrowed candidates calls `project_authored_references` with the admitted
PROJECT source namespace. Omission is `None`; an explicit empty list is
`Some([])`. These remain distinct attempt inputs. This is a Rust library
contract; a future host parser must preserve omission versus null/invalid
shape using the established reference projection contract.

Canonical USER references remain authored logical namespace data. They do not
read, dereference, authorize or create a USER target. Cross-project targets and
installed Knowledge contexts likewise receive no existence/access assertion.
Reference order and duplicates survive persistence. No target query or derived
reference graph is published by this slice.

`Memory` contains ID, positive revision, title/body, attributes, ordered tags,
qualified owned references, desired reference generation, initial and last
actor and creation/update times. `MemoryDetail` pairs current Memory and its
immutable creation event with the captured generation. Initial revision and
reference generation are both 1. Both authors are the actual write actor;
both times are the same store-derived checked UTC epoch-millisecond `u64`.
The store rejects a pre-epoch clock or conversion overflow. Creation has no
caller timestamp and promises millisecond precision only.

`MemoryActivityHead` contains exactly `id`, `title`, `updated_by` and
`updated_at`. `MemoryGeneration` is an SDK observation constructed internally
from the pin: numeric SDK version plus branch/inheritance observations and
the admitted logical project/version. It cannot open a corpus or grant access.
It is not the desired reference generation. Now must build logical links and
exclude physical observations from its remote DTO.

## Schema24 and document projection qualification

`tsk-4c15` has implemented the canonical schema24 and pure identity/document
projection/embedding codec in [embeddings.rs](../../crates/graphit-records/src/memory_store/embeddings.rs).
**Schema24/projection/codec are qualified on Main/Linux under `tsk-4c15`:**
44 integration `memory_store` cases PASS/zero failures (64.68 s) and 28 library
`memory_` cases PASS/zero failures, 32 filtered (4.88 s): **72 distinct Memory
cases**. Six Records doctests PASS (four ordinary/two compile_fail) are separate.
Scoped Clippy `--locked -p graphit-records --lib --test memory_store --no-deps
-- -D warnings` exited zero (17.67 s), and fmt of 13 owned files exited zero.
These timings are not benchmarks. The [schema qualification scope](native_memory_embeddings.md#qualificação-de-schemaprojeção-no-mainlinux)
retains initial compile/link ENOSPC recovery history and the communicated,
unsuppressed warning in a foreign Task library-test fixture. It does not claim
whole-library-test strict coverage or semantic consumer qualification.
The preceding creation/catalogue/lifecycle/index/lexical receipts retain their
historical counts and scope; they are not added to the current 72 or used to infer this schema proof.
The [embedding contract](native_memory_embeddings.md#schema-único-e-família-de-embeddings)
specifies all six nullable fields, exact neutral headers and closed payload.
`MemoryEmbeddingIdentity` and `MemoryEmbeddingDocument` have private fields,
a validated identity constructor and readonly getters. The pure
`prepare_memory_embedding_document` and `encode_memory_embedding_row` exports
produce literal JSON/SHA256 identity and a RecordBatch, without SDK publication,
Auth admission or model calls. Capture/attach/semantic query/recovery `tsk-a684`
are implemented and Main/Linux-qualified as a library: 12 real-SDK tests +
30 library Memory + 44 affected Memory integration = 86 distinct cases,
six Records doctests separately and scoped strict/fmt0. The
[semantic qualification results](native_memory_embeddings.md#qualificação-da-biblioteca-semântica-mainlinux)
keep that proof separate from the historical schema72 above. Eligibility uses
the five actual current-head MemoryListFilter predicates and explicit deleted
history; updated_at is head metadata, not a temporal filter. Compositor
`tsk-5c9b` still needs its correction package before Source and consumer
qualification; ModelService/GGUF remains outside this proof.

Dispatch validates the expected ProjectId before skipping an embedding row,
then validates canonical headers/payload/shape while borrowing vector slices.
All input rows, including ignored embeddings, count toward rows/text limits;
recipe/role bytes are included. Create checks physical rows for ID collision:
an embedding-only physical row cannot masquerade as an available logical ID.
The four-column head projection remains payload-independent.

## Native truth and exact layout

Local PROJECT operations select their corpus once, after authoritative S3
selection, through Core `local_corpus::resolve`. The single dataset is
`memory/index.lance` relative to the admitted PROJECT runtime for every project
version. VersionContext selects a native Lance branch inside that dataset;
branches retain their own ancestry, revisions and embeddings. Git snapshots use
native immutable tags in the same dataset. No version binding map or alternate
dataset path participates in selection.

The same selected URI supplies physical captures and recovery-key validation for
the entire operation. First publication creates the module and dataset under the
Core PROJECT writer. An existing module directory without its dataset is an
incomplete corpus and is rejected; unexpected entries and symlinks do not grant
permission to replace it. A recovery key from another dataset or branch is
rejected before publication. S3 is selected first and remains authoritative. USER Memory retains its own scope.
Sharing a PROJECT corpus does not admit another checkout by identity alone.
The authenticated runtime must first trust its exact captured project root
through `RoleIssuer::trust_project_root` under configuration authority. Each
operation still requires its own verified module admission and live Original.

One dataset and one strict branch delta publish current head plus immutable
initial creation event. There is no cross-dataset transaction or independent
audit/reference side store.

The current Source schema has 24 fields: the original 18 nonnullable fields
below, followed by the six nullable embedding fields specified in the linked
contract. It is Main/Linux-qualified under `tsk-4c15` in the separate scope
above. Original fields retain
this order:

| Field | Arrow type |
|---|---|
| `row_key`, `kind`, `record_id`, `project_id`, `operation_version`, `idempotency_key`, `idempotency_actor`, `title` | UTF8 |
| `revision`, `created_at`, `updated_at` | UInt64 |
| `created_by`, `updated_by` | UTF8 |
| `superseded`, `tombstone` | Boolean |
| `reference_generation` | UInt64 |
| `references`, `payload` | UTF8 |

Current Source metadata is exactly `graphit.memory.layout: project-v2-embedding-list`;
fields have no extra metadata. The former schema18/project-v1 is rejected as
incompatible, without converter, adoption or automatic rebuild. Logical rows
keep all six embedding fields NULL. Initial creation uses `head` and `creation`. Its keys are
`head/<canonical-id>` or `creation/<canonical-id>`; both rows contain the same
ID, initial revision, attribution, attempt binding and reference marker.
Born heads have `superseded=false` and `tombstone=false`; creation audit rows
have `superseded=true` and `tombstone=false`. The creation event is immutable;
its payload retains the initial record and authored attempt, including
reference omission versus explicit replacement. A full decoder checks typed
payload against duplicated headers and reference metadata. Unknown kinds,
malformed IDs/actors, zero revisions or inconsistent pairs are errors.

`references` is the deterministic serialization of the qualified desired
reference list, including `[]`. Current head metadata records that list and its
positive reference_generation; this authored metadata does not grant target
access or deliver the separate cross-source projection/query below.

### PROJECT Memory reference projection and query: Linux-qualified

`tsk-29fb` now provides the exported [Memory references Source](../../crates/graphit-records/src/memory_store/references.rs)
and DTOs through `memory_store`. Actual Linux functional runs passed **93
distinct functions: Memory SDK68 (60 existing +8 new) and Memory library25**.
The SDK run exited zero in 572.87s; the current library rerun exited zero in 25.16s
and includes the actual Confirmed/revoke reference-query assertion. That new
assertion is not an extra function. The earlier library25 run in 53.88s is
historical, not added to the total. Native PROJECT Memory reference Source is
**Linux-qualified**: strict attempt02 (`../../.build/rust-records-references/project-memory/evidence/strict-query-attempt-02.log`; historical local artifact unavailable)
exited zero in 32.46s; the eight reference cases rerun (`../../.build/rust-records-references/project-memory/evidence/memory-references-query-attempt-02.log`; historical local artifact unavailable)
passed in 42.55s and remain a subset of SDK68, not extra functions.
Records doctests (`../../.build/rust-records-references/project-memory/evidence/doc-query-attempt-01.log`; historical local artifact unavailable)
passed six cases separately (four ordinary/two compile_fail), compilation 51.94s
and docs 2.46s. Owned rustfmt --edition2024 --config skip_children=true --check
and diff --check exited zero. Strict attempt01 failed only on test-helper
type_complexity; the test-only alias correction and failed log are retained.
Formatting/compilation alone supplies no runtime proof.
The corrected packet (`../../.build/rust-records-references/crosssource-contract/project-memory-corrected.json`; historical local artifact unavailable)
and [cross-source coverage](record_relations.md#planned-cross-source-producers-and-coverage)
retain the contract; previous CRUD/Fenced/automatic qualification does not
qualify this new projection/query or FullNoLoss, App, USER or live Dream.

The private marker key is references/<MemoryId>/<domainrevision> in the same
24-column dataset, with no duplicated Memory body. Marker, head and audit form
the common three-row publication in the same strict SDK delta. The marker
carries source_revision, existing reference_generation, operation_version,
authored order/duplicates and tombstone. Generation starts at1 and changes
only on effective authored-reference change or delete/clearing. A body or
importance publication adds the new domainrevision marker with unchanged
generation. Empty/delete markers are explicit; no-op and replay publish none.
Deleted sources contribute no current outgoing/incoming edges and old markers
cannot resurrect links.

The actual asynchronous method is
NativeMemoryStore::references(original:&Arc<SnapshotContext>,
admission:&ProjectReadAdmission<'_>,query:MemoryReferencesQuery,
limits:MemoryStoreReadLimits)->Result<MemoryReferencesRead,MemoryStoreError>.
It requires actual ReadMemory and the same Original before I/O and at rechecks.
MemoryReferencesQuery has outgoing:Option<MemoryId>,
incoming:Option<MemoryReferenceTarget>, page_size:NonZeroUsize and separate
outgoing_cursor/incoming_cursor:Option<MemoryReferencesCursor>. At least one
focus is required. MemoryReferenceTarget owns target_kind:ReferenceTargetKind,
scope:MemoryReferenceScope, id:String and context:Option<String>. Incoming
accepts all four literal logical target kinds without target lookup/grant.

Memory references take `MemoryStoreReadLimits` directly. Original/admission,
typed query, logical target and cursor checks precede capture and NoCorpus.
Heads and markers are consumed through EOF without aggregate row/edge/text
quotas or an accounting ledger. Both reference JSON copies are decoded and
validated against the canonical header and current head. Retained data is
minimal head/key metadata and selected pages, without collecting full
histories or bodies for the output. Cursor binding preserves admitted root,
generation, foci, direction and offset; page size and scan batch hint may change.
Native file limits and live authority are checked on every call.


Historically, `tsk-d7c3` removed only query-output accounting and serialization previews from
this reader and its shared USER budget. Functional storage/delivery Serde,
raw scan limits, both reference-JSON predecode counts, Original callbacks and
final delivery checks remain. Source02 compile01 passed, and the eight affected
PROJECT reference cases passed; the nine USER reference cases also passed on
unchanged shared Memory/USER Source. Task-only Source03 removes a remaining
empty continuation-accounting loop and stale comment after the own strict01
failure. Current Task revalidation, compile02, scoped strict02 and fmt02 passed,
completing Linux qualification of this removal across 40 distinct affected
functions (8 PROJECT +9 USER +9 Task refs +5 backlinks +9 private Task library);
repeated runs add no functions. APIs, input/scan guards, EOF/cursors and Auth
remain unchanged; other domains/platforms are outside this qualification. Historical
`tsk-29fb` qualification and its sealed receipt keep their original scope.

Full EOF validates all markers, including old/orphan canonical forms and
key uniqueness. Current markers must match exact current head revision,
generation, references, tombstone and origin operation_version. That origin
may be main when observed through a named query branch; it is not replaced by
the query version. Missing current markers yield
MissingCurrentProjection{source,revision} and Published incomplete warnings,
never adoption of older markers or repair. Auth, request, caps and corrupt
storage remain causal errors. Source deletion does not erase its history.

MemoryReferencesRead is NoCorpus or Published{generation,outgoing,incoming,
coverage,warnings}; pages are Option<Box<MemoryReferencePage>>.
MemoryReferencePage contains results,total,complete,next_cursor. Each edge
contains source,source_revision,reference_generation,source_snapshot,reference,
provenance:Authored,ordinal,source_observation,target_resolution. Resolution is
Memory{id,title} only for live own-PROJECT/context-None/canonical Memory targets
at the same pin, otherwise Unresolved. Coverage explicitly distinguishes
memory_project_source_complete from inbound_other_sources_not_qualified.

Closed Serde cursors bind root NativePath, MemoryGeneration (logical version,
physical branch/inherited target/pin), both foci, direction, offset and public
query/read bounds, including read batch_rows. page_size may vary1..100;
stale/foreign bindings or offset greater than total reject, while offset equal
to total is an empty page. Cursor with NoCorpus is invalid. Opaque Core
NativeDocumentFileLimits are rechecked on each call and grant no authority;
every continuation needs a fresh live admission.

Lexical candidate filtering admits only head/creation/revision families;
reference markers are not valid hits, semantic candidates or attachments.
Embedding scans explicitly distinguish marker kind from validated embedding
metadata rather than treating decode(None) as a generic skip. The24-column
schema has no public_search column. SDK physical payload indexing may still
contain marker bytes: eligible-result exclusion does not promise their absence
from the physical index. No cross-corpus atomic snapshot or target authority
is supplied.
Knowledge `tsk-41d1` is a registered investigation for J1 authored/wiki-link
persistence and current markers, J2 query and J3 read-only imported namespaces
with their own grants; it delivers no reference Source API or qualification.

## PROJECT revision, deletion and history (`tsk-1228`)

The canonical lifecycle packet (`../../.build/rust-records-memory/extensions-contract/packets.json`; historical local artifact unavailable)
and inspected `memory_store/revision.rs`, `history.rs`, `model.rs`, `rows.rs`,
`write.rs` and `read.rs` agree on the implemented PROJECT protocol below.
Linux qualification executed the native, revision rerun, library and doctest
suites, strict Clippy and owned formatting successfully. Lost-ack outcomes,
atomic Auth/SDK and cross-corpus transactions remain outside these proofs.

All four lifecycle methods in the operations table take the same original
`&Arc<SnapshotContext>` and borrowed actual admission. Update/delete and
mutation reobservation require ordinary `ProjectWriteOperation::Memory`;
history requires `ProjectReadModule::Memory`. `FencedMemory` does not authorize
these ordinary operations. Actor, PROJECT scope and requested logical version
come from the actual admission, never input metadata.

`UpdateMemory` owns positive `expected_revision: u64`, `idempotency_key: String`,
`reason: String`, `title/body: Option<String>`,
`classification: Option<MemoryClass>`, `mandatory/important: Option<bool>`,
`tags: Option<Vec<String>>` and `references: Option<Vec<MemoryReference>>`.
`DeleteMemory` owns only the expected revision, key and reason.
Supplied key/reason/title/body/tag must be nonblank, preserving literal bytes;
omitted fields preserve current values and explicit empty tag/reference lists
clear them. Changing classification preserves omitted flags; flags are set
independently. An empty or effectively equal update returns `NoChange` without
an event, stored attempt or materialization.

`MemoryMutation` is `Update(UpdateMemory)` or `Delete(DeleteMemory)`.
`MemoryRevision` contains admitted `project_id: ProjectId`,
`operation_version: String`, `actor: String`, the typed `operation`, and exact
`before/after: Memory` snapshots. `MemoryHistory` contains immutable
`creation: CreationMemory`, final `current: Memory`, `deleted: bool`,
`revisions: Vec<MemoryRevision>` ordered by revision ascending, and the actual
captured `generation: MemoryGeneration`. These values carry provenance, not
authority.

The lifecycle retains the original 18 logical columns in schema24 with NULL
embedding extras. Logical kinds remain `head`, `creation`, `revision`; the
separate validated `embedding` family is not a mutation/history event. Revision keys are
`revision/<canonical-memory-id>/<after_revision>`, canonical decimal >=2.
Creation row/header/payload/authored input never change. The changed head keeps
the initial creation bindings `project_id`, `operation_version`,
`idempotency_key`, `idempotency_actor`; its current payload/title/revision/date,
last actor and reference marker agree with the event's `after`. Head remains
`superseded=false`; its tombstone becomes true on delete. Creation remains
`superseded=true`, `tombstone=false`, revision/reference generation 1.
Revision rows are `superseded=true`, tombstoned iff Delete; their attempt
binding uses the current admitted operation version, key and actor, with
actor equal to `after.updated_by`. Their payload preserves the entire typed
attempt and both snapshots. No historical key is overwritten.

An effective update preserves ID/creator/creation time, increments revision
with checked addition and records the actual actor and checked UTC epoch-ms
update time, rejecting a backward clock. Omitted or canonically equal
references preserve `reference_generation`; an effective reference change
increments it by one. Delete retains final title/body/attributes/tags and
identity/creation attribution, increments revision and always increments the
reference marker, clears current desired references and keeps every final
reference and content byte in `before`. Revision/marker overflow is causal.
Ordinary explicit WriteMemory deletion with reason and expected revision may
delete mandatory or important records. Protected bare deletion is an automatic
consolidation rule, distinct from [Linux-qualified direct fenced Dream deletion](rust_dream_records.md#direct-memory-tools-versus-automatic-consolidation). The latter conditionally archives/tombstones without a blanket important/mandatory restriction; ordinary lifecycle qualification does not qualify the fenced handlers. F1 `tsk-b2ca` now implements create/update/delete/reobservation and typed FencedUpdateMemory/FencedMemoryMutation under private WritePolicy, sharing captured writer/CAS/history/reobservation/Confirmed; its Linux functional evidence is 48 integration plus 22 library tests (70 distinct, including five new functions; focal five overlaps), with Source fmt/diff0. Native PROJECT direct fenced create/update/delete and reobservation are Linux-qualified: scoped strict3 passed exit0, six Records doctests passed separately (four ordinary/two compile_fail), and final owned formatting/diff0. Initial lock mismatch and subsequent fixture-only strict failure remain historical; Root reconciled the lock and targeted isolation passed after removing only the fixture lint line, with production Source unchanged. See the [F1 qualification evidence](rust_dream_records.md#f1-create-update-delete-and-scoped-reobservation) for commands/logs and receipt location. Typed Rust inputs do not prove host JSON unknown-field rejection. F2 `tsk-8dd2` (importance after F1) is native/Linux-qualified: 52 affected integration plus 22 library functions = 74 distinct, with four new importance functions included and focal four overlapping; six Records doctests separately, scoped strict/edition-2024 fmt/diff0. Its [separate evidence and receipt location](rust_dream_records.md#f2-importance-transitions-after-f1) preserve the F1 receipt and shared private Confirmed proof, without new importance-specific fault injection or host/App/automatic consolidation qualification. Its typed ChangeMemoryImportance and distinct Promote/Demote codec share the Fenced writer/CAS/reobserve: a new no-op checks expected revision and authority before delivery without materialization/publication/event; an exact historical retry returns the original after state before new CAS. Only important changes, preserving mandatory and all other content/metadata/references except actual transition actor/clock; automatic consolidation is the independent Source-free refinement `tsk-f5d5`. Incoming links remain unresolved logical provenance;
this operation neither scans other corpora nor builds/rewrites a reverse graph.

Original/Auth checks precede input validation and acquisition. Under the held
original writer and same pin, complete attempt lookup precedes live-head guards.
The global create/update/delete attempt namespace is PROJECT, requested logical
version, actual Actor and literal key. Changed kind/target/expected revision,
patch or reason conflicts; invalid input is not a valid retry. An identical
successful mutation returns its exact immutable original `after`, even after
later updates/deletion, without another event. For a new attempt, a missing
corpus/head or tombstoned head is `NotFound` before expected-revision comparison;
only a live head admits CAS, effective-change and clock/overflow checks.
`RevisionConflict { expected, actual }` and `NoChange` preserve distinct outcomes.

The original writer lock is held through lookup/decision and the one strict
`prepare_merge(row_key)` delta containing changed head plus immutable event.
Missing/rejected/no-change attempts do not birth or materialize. Named branch
materialization occurs only for a mutation to publish, retaining captured
inherited rows and their original event attribution. The actual strict SDK
predecessor guard rejects stale decisions without automatic retry. Publication
failure cannot leave a successfully removed head without its final event;
confirmed publication followed by a failed check retains `Confirmed`, without
rollback or atomic Auth/SDK linearization promises.

Chain reads consume matching rows on one pin to full EOF, never an SDK limit of
two. They require unique head/creation, unique attempts and revision keys,
contiguous events 2..N, each `before` equal to the preceding snapshot, valid
before/after mutation semantics, logical project/header attribution and final
head equality. Missing events, forks, duplicates, marker/actor/time mismatches
and any event after deletion are errors. `get` returns None for a
deleted head only after validating that complete chain; `history` returns Some
with final content and `deleted=true`. Catalogue and four-column heads exclude
tombstones and do not certify history.

Creation retry/reobservation with its original key returns immutable initial
creation Memory after later mutation/deletion, validates the complete chain
and never revives that ID. A new creation key may create a different ID.
`reobserve_mutation` uses fresh actual ordinary authority for the same bound
Actor/version/key and exact typed operation; Some is the original event, None
is unresolved. It reads without birth, materialization, repair or replay.
SDK causes and recovery keys remain causal; no timeout/unknown observation
proves rollback, and no metadata or saved key reconstructs authority.

The executed 24-case native suite includes revision/omission/flag/reference
semantics, complete chain and original-attempt outcomes, inherited named
revision attribution, protected explicit deletion and no resurrection, plus
missing/duplicate/forked/event-header/tombstone corruption rejection. The latest
revision fixture rerun passed eight cases with 16 filtered out; affected
creation and catalogue cases remain included in the full native suite. The
50-case library run, including 12 Memory boundary cases, passed actual stale
SDK rejection with its causal recovery key, Confirmed cancellation/revocation
and fresh same-actor reobservation, NoChange and clock/overflow cases. Six
doctests and strict all-targets Clippy passed. These results qualify the
ordinary PROJECT library on Linux; Windows/macOS are UNRUN. Lost publication
acknowledgements and Unknown commit outcomes were not injected, and no atomic
Auth/SDK guarantee follows. USER, fenced Dream integration, search, consolidation
and complete host/product cutover remain separate producer work.

## Birth, concurrency and idempotency

Acquire `ArtifactStore::writer_with_context` with the original Arc, retaining
the writer through capture, branch materialization, complete attempt lookup
and publication. Only lock acquisition enters `spawn_blocking`; it does not
create a corpus inside the detached worker. Recheck after acquisition.

Prove each physical prefix with the existing captured directory probe and
revalidation. Valid absence permits exclusive fixed Memory birth through
Core `tsk-8b90` and SDK creation of an empty default dataset. An existing
Memory directory without its dataset is `IncompleteCorpus`, not permission
to adopt or repair it. Directory/SDK failure may leave bounded residue;
preserve its causal error. Named creation materializes exactly the captured
default generation before applying its own delta; bootstrap conflict,
incomplete or unknown observations retain their Data errors. Readers and
reobservation never birth, materialize, repair or retry.

PROJECT Memory birth uses this same implementation on Linux, macOS and Windows.
Core supplies the platform-specific held-parent directory creation and original-request
writer; Records does not substitute an Unsupported branch or a path-based writer.
The original admission, directory revalidation, PROJECT lock and SDK publication
checks remain required. Availability in Source does not extend the historical Linux
qualification to another host.

An attempt is bound to admitted ProjectId, requested logical version, actual
Actor and literal key. Search both current and immutable creation members for
all four fields under the held writer and captured latest pin. This avoids
hiding a surviving head when its audit is missing or corrupt. Exactly one
complete matching pair
with identical authored input returns that initial Memory without a new ID,
audit or commit. Different input is `IdempotencyConflict`; duplicate matching
events are corruption. Another Actor or requested version has a distinct
attempt, even when an inherited default event has the same key and input.

Generate ID/time only after that lookup. Reject a generated-ID collision
rather than overwriting a record. Validate and charge both encoded rows,
then `prepare_merge` with `row_key` and commit the same immutable delta.
The strict Data predecessor/branch/version guard supplies no automatic SDK
retry; an intervening writer fails instead of replaying a stale decision.
The Core lock and the actual SDK guard preserve concurrency across ordinary
native callers. Existing foreign/raw SDK writes are not claimed to obey the
Core lock.

## Errors and uncertain publication

Retain actual Auth, request, capture, acquisition, schema/Arrow, codec and SDK
causes. Wrong admission, different request, invalid input, budget exhaustion,
incomplete corpus, inconsistent head/audit and idempotency conflict are
distinct domain conditions. None becomes an empty successful corpus.

SDK creation retains `StoreError::Creation(CommitFailure)`; delta failure
retains `DeltaCommitFailure` and its real storage recovery key. A confirmed
commit followed by failed Auth/request/path recheck returns
`Confirmed { version, cause }`, not success or rollback. These local errors
are not remote Now payloads; physical paths, raw SDK messages and recovery
keys are excluded from remote allowlists.

`reobserve_create` uses a fresh live ordinary Memory write admission, acquires
the existing writer, then only reads the admitted corpus and its requested
branch. It never creates or materializes a missing branch. A matching complete
creation event and head proves `Found` with the original actor and initial
state. A mismatched payload remains a conflict; a missing result is
`Unresolved`, including valid NoCorpus. No absence, timeout or Data
`DeltaResolution::Unresolved` is proof of rollback. Unknown/corrupt observations
remain errors. There is no implicit repeat-write, orphan adoption, recovery
ledger or authority reconstructed from a saved recovery key.

Fresh authority must still identify the same Actor, requested version and key
to reobserve that attempt. This operation cannot locate another Actor's
attempt by assuming that a replacement actor owns its history.

## Current read-policy qualification

`tsk-265d` removes aggregate corpus row/text/head quotas and the Memory reference
ledger/cursor quotas for PROJECT and USER. Query expansion, authored input,
individual encoded rows, native files, complete EOF, schema/keys, live authority,
physical collision, generation and provenance remain functional guards.
The six maintained surfaces describe this policy; earlier receipts retain their
original Source, counts and quota-era scope.

The current Linux qualification has **66 distinct Records functions**: ten
library cases, PROJECT store30 and semantic6, USER store16 and semantic4.
The first run was 65 PASS/one fixture FAIL; the corrected exact negative case
passed and adds no distinct function. It now preserves the complete corpus and
asserts the real head/projection-header errors from both JSON copies, rather
than the removed Budget error. Compile06 built the five actual targets; compile07
rebuilt the corrected PROJECT store target. Scoped Records strict02 passed after
the correction. These are native reader/guard tests, without Model/AI replay.

The current App cfg(test) compilation is Root's `tsk-393d` artifact; its unrelated
Raw runtime failures remain separate. Two causal managed-role Bootstrap cases
passed against that actual ELF using the existing controlled native peer.
Daemon library/now_publisher compilation passed for causal type/literal checks;
no daemon runtime or Model qualification follows from that compilation.
App project_hook_context/foreground_now targets also compiled on current Source;
the exact altered Hook admission/bounds case passed. Together with the two
Bootstrap cases, these are three separate App cases, not additional Records
functions. They preserve real denial/output behavior while query-only policy
cannot cap the admitted corpus. The evidence, failures, Source hashes and scope
are consolidated in `.build/rust-records-memory-read-policy/validation.json`.
Scoped formatting/whitespace passed. Local documentary links introduced here
resolve; unavailable historical artifacts remain identified as preexisting,
without fabricated receipts or replay claims.

## Complete metadata reads and limits

`observe_heads` opens the exact full schema, pins one `BranchSnapshot`, and
filters `kind='head' AND superseded=false AND tombstone=false`. It explicitly
projects only `record_id`, `title`, `updated_by`, `updated_at`. Projection
validation checks those fields, nullability, schema metadata, canonical IDs
and actor encoding. It does not decode body, attributes, tags, references or
audit payload to produce heads. It does not claim physical column byte/RSS
measurements or continuous path confinement.

Consume all batches to EOF on that pin. Validate global ID uniqueness and
sort the complete owned set by canonical ID. `MemoryHeadReadLimits` contains
only `batch_rows` and native file limits; there is no head-count or projected
text-byte quota. `batch_rows` is an SDK hint, not a result cap.

`MemoryStoreReadLimits` contains `max_query_bytes`, `batch_rows` and native
file limits. The query bound applies only to one raw/expanded lexical query,
not to the corpus, reference pages or write batches. Write policy preserves
Core `ArtifactLimits` (including its `CopyLimits`), `max_input_bytes` and
`max_row_bytes`; each encoded row must fit before publication. `ArtifactStore`
takes the captured scope and `ArtifactLimits`; it has no separate payload-limit
object or retained payload copy. Encoders do not charge the cumulative batch
against the read policy.
The common three-row publication was qualified under `tsk-29fb`, with 93 distinct functional
passes, six Records doctests separately and scoped strict/fmt0; earlier creation/lifecycle
receipts retain their original scope. A failed row-budget check after
birth can still leave the valid empty default dataset or named branch; this is
not a claim of zero storage effects. No process-global budget, quota, TTL or
first-request cache is introduced.

NoCorpus is only valid captured absence of the Memory directory; an existing
Memory directory without its dataset is `IncompleteCorpus` for reads and
reobservation too. An empty existing dataset returns
Published with its actual generation and zero heads. Lifecycle `get`
uses one pin for the complete creation/current/revision chain: no matching rows
means None; missing members or invalid attribution/marker agreement are
incomplete/corrupt history, including a deleted record's chain. All methods
recheck before delivery and return no partial set after late failure.

The initial creation pair means the logical current head and immutable creation
relationship, not a physical row count or an SDK `limit(2)`/`limit(3)`. Lifecycle
reads count and validate all consulted rows, including reference markers, through
EOF and validate every revision event. A canonical marker is a distinct valid
family; additional rows must satisfy their own codec/chain constraints rather
than become a healthy prefix or hide duplicate/corrupt history.

Now consumes the complete admitted heads, sorts `updated_at` descending with
canonical ID as the stable tie, computes total, then limits to 20 and computes
has_more. Recorded last actor supplies authorship. It never substitutes Git,
the requester or selected CLI agent. Runtime's refresh/cache/stale behavior
and per-section error DTOs remain separate consumer work.
The [Now PROJECT source contract](rust_now_project_heads.md) maps all four
sections and the remaining Runtime/Broker prerequisites.

## Current-content catalogue (`tsk-be9c`)

This extension is Linux-qualified under `tsk-be9c` and the canonical
catalogue packet (`../../.build/rust-records-memory/extensions-contract/packets.json`; historical local artifact unavailable).
It adds `memory_store/catalogue.rs` and the model values below without changing
the four-column `observe_heads` contract. The recorded receipt covers 16 native
cases, 46 library cases, five doctests, strict Records Clippy and owned formatting;
those suites were not rerun for this lifecycle documentation update.

```text
NativeMemoryStore::list(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    filter: MemoryListFilter,
    limits: MemoryStoreReadLimits,
) -> Result<MemoryListRead, MemoryStoreError> // async
```

`MemoryListFilter` owns `classification: Option<MemoryClass>`,
`tag: Option<String>`, `mandatory: Option<bool>`, `important: Option<bool>`
and `exclude_mandatory: bool`. Its default selects every active current head.
All supplied predicates combine with AND; classification is typed, and a tag
matches an exact, case-sensitive literal member of stored tags. Both flags
remain independent. Contradictory predicates produce a valid empty set.
A supplied blank tag is invalid after original/Auth checks and before corpus
I/O; valid tag bytes are preserved, with no trimming or substring search.

The catalogue requires actual `ProjectReadModule::Memory`, the original Arc,
captured PROJECT scope and one branch pin. Its current Source scans the full Main/Linux-qualified schema24
and head payload with `kind='head' AND superseded=false AND tombstone=false`.
It does not scan creation/history payloads to reconstruct the catalogue.
The current-head decoder accepts self-consistent head revision >=1 and checks
headers, logical project/identity, attribution, dates and desired references;
initial creation invariants remain separate. A current catalogue does not
certify the complete immutable history chain.

Consume every active input head to EOF, including rows later excluded by
filters. Validate global canonical Memory ID uniqueness and every current head.
Schema/codec/duplicate/Auth/cancellation/SDK failures discard the entire result;
there are no aggregate row/text corpus quotas. Only after complete validation
does filtering and ordering select the returned content.

Order mandatory records first (including records also important), then
important nonmandatory records, then normal records. Within each category,
sort `updated_at` descending, then canonical Memory ID ascending. There is no
score, top-k or silent prefix. `mandatory: Some(true)` returns every matching
mandatory record with its full body, or an explicit error; contextual
`exclude_mandatory: true` uses the same actual ReadMemory authority.

`MemoryListRead::NoCorpus` represents only valid captured absence, without a
generation. `Published { generation, memories }` retains the actual scoped pin
generation, including a valid empty dataset or an empty matching subset.
Denial, corrupt schema/payload, incomplete existing corpus, path failure and
cancel/deadline remain causal errors, never healthy empty results. Recheck the
original admission and guard at awaited SDK boundaries and before delivery.
Reads never birth, materialize, repair, publish reference state or change SDK
generation; named inherited reads retain actual generation and authorship.

The catalogue fixture contract covers AND/literal filters, independent flags,
category/date/ID ties, complete mandatory content, full-input charging,
multi-batch EOF/global duplicates/late failures, NoCorpus versus published
empty/incomplete, original admission isolation and observational named reads.
Run only affected Memory/library/doctest, strict Clippy and formatting suites
in the agreed window. USER, Dream, search, historical-chain certification and
Now integration are separate work; the catalogue receipt qualifies Linux only
and supplies no lifecycle or other-OS proof.

## Qualification of the creation Source leaf (`tsk-3ddd`)

Use actual temporary Auth/Core/Lance fixtures for project A/B and
default/named/inherited versions. Prove original versus foreign request,
module denial before corpus I/O, revoke/cancel/deadline and observed captured
path failures. Prove same-actor same-key concurrent creation yields one ID,
one head and one creation event; different payload conflicts, different Actor
and requested version remain distinct, and reopen preserves initial authorship.

Auth's Actor is bound to issuer epoch, run and role; the run is also bound to
its requested logical version. The real default/named fixture consequently
uses different Actors. Reverification under a fresh request keeps the same
Actor only within the same version. The four-field attempt filter and stored
logical version remain explicit; these tests do not fabricate a grant or claim
an unavailable same-Actor cross-version fixture.

Inspect head/audit/reference marker in the same actual generation; preserve
explicit empty versus omitted attempt metadata and canonical cross-scope
logical references without target I/O. Exercise actual Data publication and
post-confirmation failures; use fresh authority for reobservation and retain
Unresolved after missing/unknown results without writing.

Metadata fixtures cover multiple batches, late duplicates, superseded/audit
and tombstone exclusion, empty versus absent versus bad schema, historical
actor different from reader, millisecond bounds, exact/one-under byte/count
limits and one old pin during concurrent publication. A large body must not
enter metadata projection/decoding; no unmeasured performance claim follows.

Portable affected integration invocation:
`cargo test --locked --offline -p graphit-records --test memory_store`.
Run the affected library/doctests, owned formatting/diff and strict Records
Clippy in the coordinator's agreed build window. Verify this page and the
broader Memory proposal against actual implementation in both directions.
Linux evidence cannot qualify native birth on Windows/macOS, USER, Dream,
Now, Broker or host adapters.

Recorded Main/Linux commands and results:

| Command | Result |
|---|---|
| `cargo test --locked --offline -p graphit-records --test memory_store` | 10 Passed, 0 failed or ignored |
| `cargo test --locked --offline -p graphit-records --lib` | 45 Passed, including seven Memory boundary cases |
| `cargo test --locked --offline -p graphit-records --doc` | Five Passed, including two expected compile failures |
| `cargo clippy --locked --offline -p graphit-records --all-targets -- -D warnings` | Passed after the Task owner corrected its fixture; that shared Records evidence is reused with coordinator confirmation |
| Owned `rustfmt --edition 2024 --check` and diff whitespace checks | Passed |

The named-version case also passed again after its grant/admission lifetime was
expressed as a lexical block. There is no runtime behavior change from that
fixture correction or the enum-local Clippy justification preserving the
owned `MemoryCreateObservation::Found(Memory)` API. The Task evidence names
the commands, logs, source hashes and bounded coverage; no broad workspace
gate or performance qualification is inferred.

The boundary cases use the same private `publish` and `collect_heads` helpers
as public creation and metadata reads. They stage and commit real SDK deltas,
drive actual cancellation/revocation at the post-commit or later-batch check,
and replace a captured temporary directory. The strict stale delta retains
its actual failure cause and recovery key; confirmed publication followed by
a failed check retains `Confirmed`. Fresh authority can reobserve the saved
pair without advancing the SDK generation, including real reissue in the same
run after credential revocation. These are bounded native helper proofs.
Lost publication acknowledgements and unknown SDK outcomes were not injected
or observed; they remain causal errors to reobserve, without inferred rollback,
automatic retries or a claim that full P-D is qualified.

## Qualified native automatic application

The completed Source-free contract `tsk-f5d5` defines application leaf `tsk-74e7`. Its [automatic.rs Source](../../crates/graphit-records/src/memory_store/automatic.rs) now implements async NativeMemoryStore::automatic_fold, automatic_delete, reobserve_automatic_fold and reobserve_automatic_delete under actual same-Original Dream ReadMemory/FencedMemory admissions. See the [automatic API/data/recovery contract](rust_memory_records.md#qualified-native-automatic-application) for exact DTOs and limits. The [automatic fixtures](../../crates/graphit-records/tests/memory_store/automatic.rs) and affected native suites qualify the APIs/Source on Linux: 60 Memory SDK integration plus 25 Memory library functions = 85 distinct; focal eight/private three/final private three overlap and are not added. Six Records doctests passed separately (four ordinary/runtime plus two compile_fail), with corrected scoped strict exit0. Initial expanded six-pass/two-fixture-failure and initial strict seven-style-error logs remain retained without weakened assertions. This is the automatic component’s own qualification, preserving the separate F1/F2 receipts and scopes.

Paired read/fenced-write admission checks run before capture acquisition and after revalidation of its held chain. Each removal chain is discarded after its batch is built; no operation-wide quota/ledger is introduced. One strict same-corpus PreparedDelta merges the observed survivor revision and removal archive/tombstones; every effect has a distinct explicit key. An equal survivor gets an anchor only for effective removals. Public authored/result DTOs coexist with opaque module-owned MemoryFoldSurvivor/MemoryFoldDelete and private closed stored tags; observations supply provenance, not authority. Shared derive/lookup_attempt and write::publish_version preserve actual SDK version, Confirmed and DeltaCommitFailure causes without Auth+SDK atomicity, replay or a group ledger.

An existing local corpus uses one Core `capture_runtime` for the complete
selection (`memory/index.lance`, or `memory` when acquiring the parent). Core
retains and revalidates the entire directory chain with the Original; Memory
does not reconstruct its prefixes. Only an initial `NotFound` invokes Core's
probe for that same selection, with paired authority checks around the probe.
Only confirmed `Absent` becomes `None`. Probe errors propagate, and a directory
appearing during the probe preserves the original capture error. Wrong kinds,
links, replaced roots and late cancellation/revocation remain errors.

Complete metadata EOF guards protected flags and the last record under the held writer/pin. Exact read-only whole-group reobservation uses the historical anchor/observations after removals without reconstructing live members; get/history validate only the queried local chain, not cross-member completeness. Runtime `tsk-db94` still owns the real Dream caller gap. No host JSON parser, automatic consumer journey, App/executor, USER/non-Linux or performance qualification follows from this native API/Source qualification; IAB, HTTP and whole Root integration are not qualified either.

### Automatic corruption fixtures

The eight fold-corruption and four deletion-metadata variants each build one
small, valid Auth/Core/Lance seed. Between variants a fresh SDK pin performs a
strict full overwrite of that complete row image, including removed members.
This is explicit fixture preparation. The no-repair assertion compares the
damaged version before any subsequent preparation, and SDK versions continue
to advance independently of the restored domain revisions.

The same live bearer/run/actor is reverified with a new Original and new paired
admissions for every variant. No seed grant or admission is reused. An
authorized positive read proves the restored group or heads healthy before
damage; Original and admission rechecks after the negative assertions prevent
expiry from satisfying a corruption case. All damage variants, complete EOF
checks and unchanged-version assertions remain, with the existing deadlines
and credential lifetimes.

Focused before/after measurements use the same executable cases and control
case without Cargo preparation in their timings. Traced filesystem call counts
are recorded separately from untraced elapsed/CPU time. Shared-host cache/load
and Linux-only execution are explicit limits; those samples do not qualify
Windows/macOS performance or the complete Memory suite.
