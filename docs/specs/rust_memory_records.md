---
references:
  - target: {type: task, id: tsk-633b}
    relation: uses_api
  - target: {type: task, id: tsk-c623}
    relation: uses_api
  - target: {type: task, id: tsk-16e4}
    relation: relates_to
  - target: {type: task, id: tsk-8140}
    relation: implements
  - target: {type: task, id: tsk-222c}
    relation: implements
  - target: {type: task, id: tsk-265d}
    relation: implements
  - target: {type: task, id: tsk-74e7}
    relation: requires_contract
  - target: {type: task, id: tsk-db94}
    relation: relates_to
  - target: {type: task, id: tsk-b2ca}
    relation: requires_contract
  - target: {type: task, id: tsk-8dd2}
    relation: requires_contract
  - target: {type: task, id: tsk-f5d5}
    relation: requires_contract
  - target: {type: task, id: tsk-dcee}
    relation: specified_by
  - target: {type: task, id: tsk-613a}
    relation: implements
  - target: {type: task, id: tsk-d5a9}
    relation: derived_from
  - target: {type: task, id: tsk-1f4a}
    relation: derived_from
  - target: {type: task, id: tsk-a59d}
    relation: relates_to
  - target: {type: task, id: tsk-cd05}
    relation: supports
  - target: {type: task, id: tsk-85d6}
    relation: supports
  - target: {type: task, id: tsk-09f5}
    relation: relates_to
  - target: {type: task, id: tsk-c775}
    relation: supports
  - target: {type: task, id: tsk-31f5}
    relation: relates_to
  - target: {type: task, id: tsk-b129}
    relation: supports
  - target: {type: task, id: tsk-ec62}
    relation: relates_to
  - target: {type: task, id: tsk-d76c}
    relation: relates_to
  - target: {type: task, id: tsk-08ae}
    relation: supports
  - target: {type: task, id: tsk-9ccd}
    relation: relates_to
  - target: {type: task, id: tsk-3ddd}
    relation: relates_to
  - target: {type: task, id: tsk-1228}
    relation: supports
  - target: {type: task, id: tsk-be9c}
    relation: supports
  - target: {type: task, id: tsk-3a8e}
    relation: relates_to
  - target: {type: task, id: tsk-dc2d}
    relation: requires_contract
  - target: {type: task, id: tsk-4f78}
    relation: relates_to
  - target: {type: task, id: tsk-a666}
    relation: requires_contract
  - target: {type: task, id: tsk-1597}
    relation: relates_to
  - target: {type: task, id: tsk-da58}
    relation: requires_contract
  - target: {type: task, id: tsk-631e}
    relation: requires_contract
  - target: {type: task, id: tsk-63c5}
    relation: requires_contract
  - target: {type: task, id: tsk-612f}
    relation: requires_contract
  - target: {type: task, id: tsk-b07b}
    relation: uses
  - target: {type: task, id: tsk-4c15}
    relation: requires_contract
  - target: {type: task, id: tsk-dec0}
    relation: requires_contract
  - target: {type: task, id: tsk-a684}
    relation: requires_contract
  - target: {type: task, id: tsk-7154}
    relation: implements
  - target: {type: task, id: tsk-c558}
    relation: integrates
---

# Rust Memory records

Audience: Records consumers and Auth/Data/AI/Runtime maintainers. Status: bounded PROJECT creation Source `tsk-3ddd` is COMPLETED r18 and qualified on Main/Linux; current catalogue Source `tsk-be9c` is qualified on Linux (16 native tests, 46 library tests, five doctests, strict Records Clippy and owned formatting). The broader Memory contract remains under refinement. Planning `tsk-613a` is completed. Ordinary PROJECT update/delete/history `tsk-1228` is implemented and Linux-qualified: full native24, latest revision8 (16 filtered), library50 including 12 Memory boundary cases, doctests6 (four ordinary/two compile failures), strict all-targets Clippy exit0 and owned formatting/diff/links/whitespace PASS. Search investigation `tsk-3a8e` produced Source-free contracts/readiness with five planning checks Passed and registered implementation leaves; lifecycle is consulted in the Task. Its lexical Memory engine `tsk-612f` is Linux-qualified: 39 affected integration plus 18 affected library tests (57 distinct Memory cases), six Records doctests (four ordinary/two compile_fail) and scoped strict exit0. The eight passing focal cases are included in the 39, and the doctests are not six executable Memory docs. Source-free Data refiner `tsk-dc2d` is COMPLETED r10 with five planning checks Passed. SDK qualification leaf `tsk-4f78` is COMPLETED r15: eight SDK tests, two reinforced error tests and strict Clippy/formatting passed on Linux. Data index Source `tsk-a666` is COMPLETED r22 and Linux-qualified: eight Source tests (0.37 s), eight Branch tests (0.16 s), 20 doctests and affected strict Clippy exit0, with owned formatting/diff/links/whitespace PASS. Its delivered index publisher does not implement Memory selection or semantic search. Root-approved current/archive/deleted selection has executed lexical Linux qualification in the bounded scope above. Planning completion does not qualify search. Catalogue qualification does not close revision/CAS, search, consolidation, USER, Dream or complete product cutover. Auth `tsk-1f4a` and guard `tsk-c775` provide qualified PROJECT borrowed-admission evidence. Catalogue affected suites were executed for this Source; unrelated producer qualifications were reused.

Existing product behavior remains in the [Memory specification](memory_module.md). Shared authority, publication, references and recovery obligations live in the [Rust record transaction proposal](rust_record_transactions.md); this page specializes Memory semantics rather than duplicating those producer packets. Go schemas, Markdown path keys and cursor formats are not Rust compatibility requirements.

The bounded [native PROJECT creation and current metadata contract](native_memory_store.md)
(`tsk-9ccd`, Source `tsk-3ddd`) specifies the first corpus, initial audit,
desired reference marker, scoped idempotency and complete metadata projection.
Its Rust Source is exported and uses qualified Data branch adapters and the
completed fixed Core Memory birth `tsk-8b90`. This bounded Source is qualified
on Main/Linux;
see that page for affected-suite evidence and limitations. This does not
close update/delete/search/consolidation, USER or Dream obligations below.

The [Memory attribute aggregation library](rust_memory_attributes.md) combines scalar classification and independent flags through the main two-argument API under `tsk-7154`. Eight kernel and eight affected native automatic-operation tests passed on Linux; scoped strict, formatting and documentation checks passed. `tsk-b129` is historical and `tsk-c558` qualified an isolated stage, not this main integration. The fold uses O(N) time and O(1) additional memory without successful-fold allocation or I/O; Core request-error construction may allocate. Automatic application `tsk-74e7` and Memory references `tsk-29fb` retain separate scopes and receipts. Attributes provide no admission, live-head/coverage proof, survivor/provenance/reference preservation or publication.

The independent [pure Task specification library](rust_task_specification.md), `tsk-ec62`, is COMPLETED r62 with all nine checks Passed by fresh authoritative Task readback. It qualifies pure reset, verification and supersession behavior; the current library/doctest suites also include it. It does not deliver persistent Task Auth/CAS/recovery or the complete Records product. Memory PROJECT storage, history and writer/CAS have their own actual qualified implementation above; USER/Dream/search and full-product integration remain separate.

## Bounded extension plan after PROJECT creation

The canonical extension packets (`../../.build/rust-records-memory/extensions-contract/packets.json`; historical local artifact unavailable) received `NOFAULT_PLANNING_ONLY` review. All three leaves depend on completed `tsk-3ddd`. Catalogue has its separate Linux receipt; ordinary PROJECT lifecycle is Linux-qualified with its separate native/revision/library/doctest/strict receipt, and lexical search has its independent Linux-qualified Source receipt. Planning readiness supplies no product proof.

| Leaf | Approved scope and APIs | Pending proof |
|---|---|---|
| `tsk-1228` | Linux-qualified ordinary PROJECT Source: `memory_store/revision.rs` and `history.rs`, `NativeMemoryStore::update`, `delete`, `history`, `reobserve_mutation`; exact patch/event/history values. See [PROJECT lifecycle](native_memory_store.md#project-revision-deletion-and-history-tsk-1228). | Full native24, latest revision8 with 16 filtered, library50/Memory boundaries12, doctests6 and strict Clippy/formatting PASS. Actual stale SDK/recovery key and Confirmed cancel/revoke with fresh same-actor reobservation passed; lost ack/Unknown injection, other OSes and USER/Dream/search/full products remain outside these proofs. |
| `tsk-be9c` | Linux-qualified current catalogue: `memory_store/catalogue.rs`, `NativeMemoryStore::list`, `MemoryListFilter` and `MemoryListRead`; full current content separately from four-column heads. See [catalogue contract](native_memory_store.md#current-content-catalogue-tsk-be9c). | Executed affected suites and targeted duplicate/path/schema proofs: one pin through EOF, AND/literal filters, priority/date/ID ordering, all-input limits, no partial results, actual role reads/denials and self-consistent active heads revision >=1. Whole-history certification belongs to the revision leaf. |
| `tsk-3a8e` | Source-free contracts/readiness result, five planning checks Passed and implementation leaves registered; consult the Task for lifecycle. Source-free `tsk-dc2d` COMPLETED r10/planning5 Passed; SDK `tsk-4f78` COMPLETED r15/seven Passed; Data index publisher `tsk-a666` COMPLETED r22, with Source8/Branch8/doc20/affected strict0/fmt/diff/links PASS. Root-approved living-chain current/archive selection and explicit deleted-history option; lexical work is independent of AI. | The lexical engine `tsk-612f` is Linux-qualified: focal8 included in integration39, library18, six Records doctests and scoped strict0; 57 distinct Memory cases. Third focal 6 PASS/2 fixture failures remains historical, before corrections and fourth focal8 PASS. Registration readbacks were OPEN r1/Ready for Data inspector `tsk-631e` and OPEN r1 for Memory WRITE index `tsk-63c5` and lexical reader/search `tsk-612f`; these are historical creation snapshots. Their required dependencies remain explicit below. The inspector has its qualified Data contract; Linux-qualified explicit index APIs (45 distinct tests/strict0) are documented separately below, while the reader has its distinct Linux qualification described below. SDK/Data results do not qualify Memory search; semantic Memory has the Source-free `tsk-da58` [planned contract](native_memory_embeddings.md); its schema/projection, Data List cosine and capture/attach/query producers are not delivered by lexical qualification. |

The qualified [NativeMemoryStore](../../crates/graphit-records/src/memory_store.rs) creation slice publishes revision 1 and retains immutable creation/reobservation; the qualified catalogue reads full current content, while `observe_heads` remains four-column metadata. Implemented PROJECT lifecycle Source validates a full EOF creation/current/revision chain: deleted `get` returns None only after validation, `history` retains final content, and creation retries return the initial event after update/delete without resurrection. Mutation and adapted-read native/library/doctest/strict evidence is executed and Linux-qualified. Current catalogue does not certify history. See [native Memory storage](native_memory_store.md) for separate receipts and the implemented protocol.

For ordinary explicit deletion, actual `WriteMemory` admission plus `expected_revision`, idempotency key and reason may delete important or mandatory records. The protected bare-delete rule below applies to automatic Dream/consolidation proposals. Incoming references retain unresolved logical provenance; ordinary deletion does not scan other corpora, build a reverse graph or rewrite inbound links. The new event preserves final history and attribution; current reads exclude a tombstoned head, while an identical successful retry reobserves its immutable original outcome and never resurrects it.

Root approved module/test wiring and Memory documentation ownership. Serialize edits to shared `memory_store.rs`, its model/read/rows/native/write helpers and the test root between revision and catalogue workers; neither sibling needs an artificial dependency on the other. Each worker preserves existing creation tests and qualifies only its affected Memory suites, library/doctests, strict Clippy and formatting in the agreed build window. Root retains shared manifests/lockfiles and session coordination. USER, Dream fencing/runtime integration, search and consolidation still require their own contracts and executed evidence; a planning packet supplies none of those product proofs.

## PROJECT heads/history and future consumer obligations

The following labels describe contract roles, not authority. Actual PROJECT types and operations are documented in the [native lifecycle](native_memory_store.md#project-revision-deletion-and-history-tsk-1228); broader USER/Dream action outcomes remain future consumer contracts:

| Proposed label | Required meaning |
|---|---|
| `MemoryHead` | Current identity, authorized record scope, positive revision, title/body, classification, independent mandatory/important flags, tags, typed references and update date |
| `MemoryId`, `RecordScope` | Logical identity and namespace metadata; neither authorizes access or reconstructs a grant |
| `ArchivedRevision` | Immutable scoped identity chain/revision with actor, reason and tombstone attribution; history survives deletion |
| Conditional mutation | Explicit expected observed head revision; Dream additionally needs its proven Memory fence precondition |
| Field patch | Omitted fields preserve metadata; explicitly supplied blank content is distinct from omission, proposed through an optional field value |
| Action outcome | Applied, Refused, Failed or Uncertain for each action, with coverage/sources, publication proof or recoverable receipt identity and legible partial results; names remain proposal labels |

Flags are independent: changing classification cannot clear mandatory or important, and omitted metadata must survive updates. Delete cannot resurrect a head or discard its revision chain. Current, archived and trace reads are observational. Heads and history retain logical scope/version identity, not checkout-root paths.

The PROJECT lifecycle publishes head, immutable revision/audit and desired reference generation through the actual held writer and one strict Data delta; broader Dream/consolidation publication retains its additional future obligations. Failure to archive the final record blocks destructive deletion. Expected-revision conflicts must not fork history; conditional ownership/fence checks use the same current scoped head and authoritative generation as publication.

## Qualified PROJECT admission and bounded Memory view

P-F/P-A is the implemented and qualified PROJECT-only opaque Auth boundary in `tsk-1f4a`. The bounded captured Records/Data creation view is now delivered by `tsk-3ddd`, including initial publication, `get`, `reobserve_create` and complete metadata heads; catalogue and ordinary PROJECT revision/history are Linux-qualified with distinct receipts; lexical search has its separate Linux qualification above. The factory derives the private AuthStore from the selected issuer, compares selected/grant issuer pointers and original request pointers plus real `same_request`, and rechecks source generation, physical PROJECT scope/version and live grant lineage. The historical PROJECT-only qualification did not provide a USER subject or admission. Current Native Runtime and Incoming USER admission is qualified separately by `tsk-633b` COMPLETED r14, and the native USER SDK by `tsk-c623` COMPLETED r26; see [USER record admission](user_record_admission.md) and [native USER Memory](native_user_memory_store.md). These actual producers require a verified literal identity, physical USER capture, same Original and real permissions. Actor, declared ProfileData and ReadUser alone remain insufficient; extra ReadUser does not deny legitimate PROJECT ReadMemory. Authenticated Local mode has no Native USER proof. Runtime hook consumer qualification remains its own `tsk-16e4` obligation. Calling `authorize` separately on a body-derived scope/subject/generation DTO or NativePrincipal does not create a read view. Samples, neighborhoods and Lance probes do not provide a complete Memory source. Denial before admission requires zero Records/Data corpus I/O, exposure and logical mutation; bounded physical Auth/Core validation I/O retains the original context and no refund. Measure these domains separately. Auth qualification alone does not qualify the remaining Memory extensions.

The implemented PROJECT Memory library binds the [qualified Auth library](project_record_admission.md) through `ProjectRecordAuthority::admit_read` with `ProjectReadModule::Memory`. Ordinary writes use `admit_write` with `ProjectWriteOperation::Memory`; Auth offers `FencedMemory` with `MemoryFenceRequired`, while F1 native fenced Dream handlers are [Linux-qualified direct fenced handlers](rust_dream_records.md#native-fenced-leaves-implemented-f1-and-f2) in `tsk-b2ca`; F2 native importance transitions are implemented and Linux-qualified in their separate affected scope. Ordinary Memory methods continue to reject FencedMemory. The selector constructor is pure and admits nothing. Read admission exposes `recheck/project/request`; write admission also exposes `actor`. Retain the private borrowed handle, rechecking before every query/effect; Role/Actor/project/request results are observations, not substitute handles. Ordinary PROJECT revision CAS and captured full-chain reads are implemented and Linux-qualified; future cursor/USER/Dream consumers retain their own contracts. Auth-versus-Data commit linearization is not claimed by the library.

PROJECT library qualification passed 18 focused fast tests, 90 issuer tests including the real 30-second deadline, 87 doctests and strict Clippy on Linux. Production `crates/graphit-auth/src/issuer/records.rs` remains at SHA-256 `3c94683414cab082525fdc9fefbcf89c597e230b9316857fee588464c8db914c`. The qualified `tsk-c775` failure/nonzero Git barrier and restored-positive evidence has raw group-6 SHA-256 `4378f7a7404b64451fe76498422f7cb27d15be400b7fc1bec8432c864351429c`. These producer results were not rerun for this page and do not qualify other Auth changes, USER, Memory/P-V/P-D/CAS or other OSes.

| Role ceiling | Memory | Task | Private-user Memory |
|---|---|---|---|
| Scout | Read permitted by ceiling | Read permitted by ceiling | `ReadUser` denied |
| Scribe | Read; `WriteMemory` | Read; `WriteOwnTask` requires `OwnActor` | `ReadUser` denied |
| Tracker | Read denied | Read permitted by ceiling | `ReadUser` denied |
| Dream | Read; `WriteFencedMemory` requires `MemoryFenceRequired` | Read denied | `ReadUser` denied |
| Coordinator | Actual granted permission required | Actual granted permission required | Native USER read requires the qualified current identity/capture/Original admission and actual `Read(Module::Memory)` **and** `ReadUser`; a Coordinator label supplies no authority |

These ceilings were checked against `crates/graphit-auth/src/policy.rs::Permission::allowed`; a ceiling is not an actual grant. `ReadMandatory` does not exist. Mandatory selection requires live `Read(Module::Memory)` in the authorized namespace. Mandatory/important flags cannot override denial; parent Coordinator projections, unfiltered views and private-user bodies cannot elevate the other four roles.

Scout/Scribe/Dream with actual PROJECT ReadMemory admission can read nonmandatory records as well as bootstrap mandatory records; do not impose universal MandatoryOnly. The proposed selector label `queryMandatory` has no accepted signature. Contextual search with `exclude_mandatory: true` and Dream consolidation reads use the same actual issuer-verified Role/grant/operation, selected issuer/private AuthStore, original request and bounds. Role enums, missing ReadMemory, revocation or foreign issuer/request cannot admit a read; Tracker remains denied and these PROJECT Role paths do not admit USER. Standalone Auth proof does not depend on Data; real P-V integration must apply authorized selection/exclusion before paging and head collapse.

Root USER/per-call direction D1–D5 is closed by `tsk-31f5` COMPLETED r21/specification 5 with seven planning checks Passed: extended real issuer/policy, canonical verified issuer+stable subject identity separate from permission, dedicated physical Core USER binding, explicitly authorized refresh/new phase under the same original context and captured local policy. That planning receipt is not producer execution or a grant. The later `tsk-633b` and `tsk-c623` qualifications above provide actual Native/Incoming USER admission and SDK storage; each Runtime/Daemon consumer retains its own integration proof. Role::Dream still gains no `ReadUser` or user-mutation permission.

Administrative refresh `tsk-d76c` has Root-reported Linux PASS for 46 Manager tests, 17 OAuth tests, 93 doctests and strict Clippy; authoritative readback remains OPEN r40 with 13 checks Passed and two Pending, including final guard/readback obligations. Refresh publication proof does not authenticate a USER subject or authorize Memory corpus reads/CAS; no pending check is inferred Passed from another library.

The bounded PROJECT view now uses the qualified Auth library and an actual Data snapshot through `tsk-3ddd`. Runtime `tsk-cd05` remains a consumer; producers must not depend on that consumer. Planning refinements retain `tsk-d5a9` for independent gaps. Catalogue is Linux-qualified; ordinary PROJECT revision/history is Linux-qualified with its full native/library/doctest/strict receipt. Remaining Data/AI/Runtime contracts and integration proofs still govern search, consolidation, USER and Dream; library or design acceptance alone cannot qualify those consumers.

## Canonical selection and complete bounded recall

Root-approved search decisions are recorded by `tsk-3a8e`; SDK and Data index Source are Linux-qualified with their separate receipts, while Memory lexical integration has its own executed Linux qualification below. Memory owns logical head collapse and canonical ordering; Data owns actual candidate/query/index APIs. Normal search includes current and archived matches only within living chains. If the current live head matches, it wins and older matching revisions are omitted. Otherwise choose the matching archived revision with the greatest numeric revision, preserving its archived/superseded provenance and link to the logical current identity. Lexical search matches only title, body, tags and classification; reason, attribution, references and other payload metadata are not search fields. Lexical implementation does not depend on AI or semantic embeddings.

AND filters for class, literal tag, flags and exclusions use current-head metadata, even when the matching content is archived. Priority and ordering date also come from the current head: mandatory before important before normal, then newest head update date and deterministic ID tie-break, before `top_k`. Match score remains metadata and cannot outrank current-head flags/date. The Rust representation must distinguish current-head metadata from matched-revision provenance without copying Go fields or schemas. This preserves the [existing recall contract](memory_module.md).

An explicit search option adds histories of tombstoned memories under the same actual ReadMemory admission. Those hits are marked both deleted and archived; they never become live current records, resurrect a head or gain authority from provenance. Normal search excludes them. Authorized deleted history already exists in the qualified PROJECT lifecycle, the explicit `MemoryLexicalQuery::include_deleted_history` option is implemented, and its lexical runtime fixtures passed in the Linux scope below. The current-match and greatest-matching-revision rules above are approved design, not missing selection decisions.

Consume candidates to full EOF from the same captured pin, apply authorized filters and collapse to unique logical memories, then apply canonical category/date/ID order and only then top-k. Late failure or incomplete input cannot yield a healthy partial result. Score is metadata and never chooses a revision or outranks current-head priority. If the qualified producer needs paging, it must close completeness and retained original admission/request plus scope/version/generation binding; no new cursor framework or silent generation advancement is implied by these decisions.

Full mandatory recall means every authorized matching record without silent truncation. Positive page/retention/work/IO/body-byte limits, original cancellation and deadline must admit the entire result, or return an explicit failure or a defined complete paging protocol. A bounded subset cannot be described as full mandatory recall. Final paging/error/lifetime signatures remain pending producer acceptance.

After Auth admission, distinguish `NoCorpus`, `EmptyPublished`, no query match, `IncompleteProjection` and denial/cancel/deadline/integrity/source-change outcomes. Zero rows without a complete capture proof do not establish an empty published source. Errors must not leak inaccessible existence. Reads do not repair projections, index, create tables or mutate catalogs.

## SDK qualification and pending Memory integration

Source-free Data investigation `tsk-dc2d` is COMPLETED r10 with five planning
checks Passed, under `tsk-86ec` in `ses-c98a`, depending on qualified `tsk-8a17`
(COM19). Its D1–D7 findings are recorded in the
SDK contract report (`../../.build/rust-records-memory/search-contract/index-publication-report.json`; historical local artifact unavailable),
SHA256 `52b4ca8ae9796712d27272fbe5084b4bb7a63375a527a3b3dbaefd3f6b727c77`.
Root serially composed the Lance index13 development dependency with metadata
exit zero. SDK qualification leaf `tsk-4f78` is COMPLETED r15 with seven checks
Passed: eight SDK tests, two reinforced error tests and strict Clippy/formatting
passed on Linux. Its historical [qualification note](../development/lance13_fts_index_qualification.md)
and receipt (`../../.build/rust-records-memory/search-contract/fts-qualification/validation.json`; historical local artifact unavailable)
retain failures and scope limits. Root accepted its exact paths and serial warm-target -j1 window.
Data index Source `tsk-a666` is COMPLETED r22 and Linux-qualified; [its contract](../development/native_json_fts_index.md)
records Source8 at 0.37 s, Branch8 at 0.16 s, doctests20 and affected strict
Clippy exit0, plus owned fmt/diff/links/whitespace PASS. Its receipt is
`.build/rust-records-memory/search-contract/index-source/validation.json`;
ENOSPC/E0308 failures and SDK historical receipts remain preserved. JSON path
and append behavior have qualified SDK evidence; Memory merge/delete lexical
coverage has its own executed `tsk-612f` fixtures below. These are SDK/Data results, not Memory search qualification;
`tsk-3a8e` records Source-free contracts/readiness and five planning checks Passed, not a delivered engine; consult its Task for lifecycle.

Lance13's JSON tokenizer is a real alternative: word tokens and a restricted
logical title/body/tags/classification path allowlist can preserve the existing
original 18-column Memory corpus rather than adding a parallel truth. Current
Source `tsk-4c15` extends that same table to schema24/project-v2-embedding-list;
it is Main/Linux-qualified under `tsk-4c15`, separately from the historical
index receipt: 44 integration + 28 library = 72 distinct Memory cases, six
Records doctests separately and scoped strict/fmt0. SDK runtime
fixtures are qualified; actual Memory lexical integration is Linux-qualified in its distinct affected scope below. Analyzer,
safe-query encoding and token length behavior were exercised by the SDK
producer without an arbitrary hardcoded limit; Memory caller integration still
requires its own proof. Invalid JSON can panic in the SDK tokenizer, so
input validation must precede index creation. This is a required qualification
boundary, not a fallback or permission to tolerate malformed current rows.

The SDK13 public `execute_uncommitted` returning `IndexMetadata` exists.
Convenience `create_index`/`optimize_indices` methods commit internally and do
not prove the required strict publication protocol.
`Operation::CreateIndex` with `TransactionBuilder`/`CommitBuilder` was exercised
by the qualified SDK leaf and Data Source tests; the adapter also passed
Branch8, doctests20 and affected strict Clippy. `max_token_length: Option` with None disables that filter; a configured
limit accepts token byte length strictly less than the limit. Exact-bound
and long UTF8 token fixtures passed in the SDK qualification. These are bounded
SDK/Data proofs, not a qualified Memory index publication contract.

The actual scanner already exists; captured index publication has its
Linux-qualified Data Source contract and Source8/Branch8/doc20/affected strict
evidence. A usable captured index must still be inspected with its actual
tokenizer policy: SDK13 can fall back to flat simple-text search when no index
segment is usable, and that fallback does not implement JSON triplet semantics.
Name-only index presence, filtered SDK metadata or zero hits cannot establish
the required policy. Root approved Data ownership of `fts_state.rs` and its
tests for registered inspector `tsk-631e`, requiring delivered `tsk-a666`.
Its creation readback OPEN r1/Ready is historical registration evidence.
Its qualified [Data inspection API](../development/native_json_fts_index.md#inspeção-json-fts-no-pin-capturado-tsk-631e) binds actual metadata to physical tokenizer parameters;
those parameters are not assumed to reside in metadata `details` JSON. Memory request/guard integration
and lexical behavior have their own executed proofs in `tsk-612f` below.

The independently registered Memory WRITE index leaf `tsk-63c5` requires
the `tsk-3a8e` contract; lexical reader/search `tsk-612f` requires `tsk-3a8e`,
`tsk-631e` and `tsk-63c5`. Their OPEN r1 readbacks are historical registration
snapshots, not assertions of current blocking state. Root accepted ownership
for all three; actual lifecycle/dependency readiness belongs to their Tasks.
The index leaf must assemble shared history from a single complete EOF rather
than scanning once per record. Actual ReadMemory/WriteMemory admission and
original-context checks remain per operation. Root composed the Records SDK
edge through `tsk-b07b`: official Lance 14.0.2, with default features disabled.
Registration and that dependency composition do not qualify the Memory APIs.
The separate [explicit index publication/resolution contract](native_memory_search.md#publicação-explícita-do-índice-tsk-63c5) documents Linux-qualified `tsk-63c5` explicit index APIs (45 distinct affected tests and strict0); the [lexical query](native_memory_search.md#query-lexical-tsk-612f), `tsk-612f`, is Linux-qualified with focal8 included in integration39, library18, six Records doctests and scoped strict exit0: 57 distinct Memory cases plus six Records docs. The first pre-test ENOSPC, second fixture compile failure and third focal 6 PASS/2 fixture failures remain in the history before corrected fourth focal8 PASS. SDK `batch_rows` is a hint, not a guarantee of FTS batch fragmentation: 120 candidates arrived in one batch with hint1; the separate typed corpus scan exercised 120 rows in multiple batches.
An unusable/missing index cannot silently become flat JSON-triplet search or
a healthy NoHits result. Read-only Memory search
must not create or repair an index. Executed lexical fixtures cover living versus
explicit deleted chains, current-match precedence, greatest numeric archived
match, the allowed search fields, safe query/tokenizer behavior, full same-pin
EOF/collapse/order/top-k, limits and causal errors. Private production
Candidates composition covers two-batch late errors/cancellation, without SDK
interception or atomic Auth/SDK proof. The query contract separates these
executed proofs from the creation/catalogue/lifecycle and old index receipts.

The Source-free `tsk-da58` [planned Memory embedding contract](native_memory_embeddings.md) selects the same-table canonical24 schema/projection (`tsk-4c15`, schema/projection/codec Main/Linux-qualified: 44 integration + 28 library = 72 distinct Memory cases, six Records doctests separately, scoped strict/fmt0), Data List cosine (`tsk-dec0`, eight distinct tests and strict/fmt0 in its own scope) and capture/attach/query/resolve (`tsk-a684`, Main/Linux library-qualified: SDK12 + library30 + affected integration44 = 86 distinct Memory cases, six Records doctests separately, scoped strict/fmt0). These are executed capture/attach/query/recovery proofs; daemon/ModelService/GGUF integration remains outside their scope. The [pure projection/codec APIs](native_memory_embeddings.md#texto-documental-e-identidade) expose validated private-field identity/document getters and RecordBatch encoding without publication, Auth or model inference. Expected ProjectId is checked before validated embedding skip; all rows and recipe/role text bytes count toward budgets, physical embedding-only ID collisions are refused, and the four-column head projection stays payload-independent. Semantic selection uses the five actual current-head MemoryListFilter predicates (classification, literal tag, mandatory, important, exclude_mandatory) plus deleted before scoring; updated_at remains head metadata, not an implicit temporal filter. It chooses the best cosine per Memory (tie current, then greatest numeric revision), and orders unique results by distance/ID after complete EOF, before top-k. This changes only the semantic order; lexical current-wins/archive selection and category/date/ID order above are unchanged.

The [qualified semantic library APIs](native_memory_embeddings.md#captura-inferência-attach-e-resolução-implementados) distinguish Existing with generation/no key, Published with generation/recovery key and known postcheck EmbeddingConfirmed. Resolve uses fresh original/ordinary WRITE, exact physical key binding and no birth/materialize/replay. Physical source/embedding EOF, complete coverage and ID collapse precede global distance/ID top-k; final body lookup uses the same pin and complete EOF/schema/keys/Auth checks, without aggregate corpus quotas. The producer results (`../../.build/rust-records-memory/semantic-source/results.json`; historical local artifact unavailable) record 12 final real-SDK cases, 30 library Memory cases (32 filtered) and 44 affected integration cases, 86 distinct, with six Records doctests separately. Final scoped strict includes lib, memory_store and memory_embeddings with --no-deps/-D warnings, exit0; owned fmt/diff/whitespace also passed. ModelService/GGUF and daemon integration are not qualified. Earlier schema/kernel receipts keep their separate scopes and are not added to these counts.

Knowledge NN Data `tsk-1597` is COMPLETED r40 with its separate [Data/Linux contract](../architecture/rust-knowledge-semantic-data.md); that delivery does not define Memory corpus identity, filters, archive collapse or its semantic writer/query API. The compositor `tsk-5c9b` must reconcile its correction package with actual Memory producer APIs before Source; Task FTS remains the independent `tsk-3e62` investigation.

## Consolidation and preservation

AI can propose merge, resolve, promote, demote, update or delete actions; proposals and historical observations do not carry authority. Each action needs observed heads/preconditions, member/source identities and evidence coverage. Validate live membership/existence, nonempty preserved content, survivor membership and reference provenance before admitting effects.

A merged survivor inherits mandatory and important through independent OR operations. Classification precedence is correction > convention > decision > tension > skill > fact; classification never substitutes for flag preservation. Preserve every unique rule, evidence item and provenance/reference condition rather than accepting a model summary as coverage proof. Resolving a contradiction cannot promote speculation to confirmed fact.

The qualified native automatic fold publishes an intact survivor and every final archive/tombstone together in one conditional SDK manifest; the survivor batch is placed first in the prepared group and removal cannot become visible first. If the single commit fails, no committed group is established; actual uncertainty remains causal. Bare automatic deletion protects important or mandatory records and the last record. This consolidation policy does not prohibit direct Dream conditional delete of such records or direct demote: direct update changes only title/body/references, while promote/demote change only important and preserve mandatory. The distinct [fenced handlers and importance contract](rust_dream_records.md#direct-memory-tools-versus-automatic-consolidation) permits insert flags/tags/references and reference replacement, without a universal critical predicate. F1 `tsk-b2ca` now implements its typed Rust fenced handlers through shared writer/CAS/history/reobservation/Confirmed, with 70 distinct Linux functional passes (48 integration plus 22 library; focal five overlaps), Source fmt/diff0 and no host JSON parser proof. Final scoped strict3 and owned formatting/diff passed; Records doctests passed six cases separately (four ordinary/two compile_fail), qualifying native PROJECT direct fenced create/update/delete and reobservation on Linux. Initial lock mismatch and fixture-only strict failure remain historical; targeted isolation passed after the sole fixture-line cleanup without production Source change. The [F1 evidence](rust_dream_records.md#f1-create-update-delete-and-scoped-reobservation) retains logs and the receipt location. F2 `tsk-8dd2` (depends on F1) now implements typed ChangeMemoryImportance, distinct Promote/Demote persistence and extended Fenced reobservation, with native/Linux qualification: 52 affected integration plus 22 library functions = 74 distinct (four new functions included, focal four overlapping), six Records doctests separately and scoped strict/edition-2024 fmt/diff0. [F2 evidence](rust_dream_records.md#f2-importance-transitions-after-f1) retains its separate receipt location and shared Confirmed proof, without new importance-specific injection, host/App or automatic consolidation qualification. New no-op checks CAS before state and rechecks authority before delivery, without materialization/event/revision/receipt; exact historical retry returns its original after before new CAS. A transition changes only important, preserving mandatory/content/classification/tags/references/reference generation/creation while updating real actor/clock; automatic consolidation is the independent Source-free refinement `tsk-f5d5`. Registration supplies no functional qualification.

### Qualified native automatic application

The completed Source-free refinement `tsk-f5d5` closes this contract. Records leaf `tsk-74e7` qualifies the four native PROJECT APIs on Linux: Memory SDK integration (`../../.build/rust-records-memory/automatic-application/memory-sdk.log`; historical local artifact unavailable) passed 60 functions, and Memory library (`../../.build/rust-records-memory/automatic-application/memory-lib.log`; historical local artifact unavailable) passed 25, totaling 85 distinct functional tests. Focal eight, private three and final private three overlap those counts and are not added. Records doctests (`../../.build/rust-records-memory/automatic-application/records-docs.log`; historical local artifact unavailable) passed six separately (four ordinary/runtime plus two compile_fail). Corrected scoped strict (`../../.build/rust-records-memory/automatic-application/strict-corrected.log`; historical local artifact unavailable) passed exit0 with cargo clippy --locked -p graphit-records --lib --test memory_store --no-deps -- -D warnings. Final private production boundaries (`../../.build/rust-records-memory/automatic-application/lib-boundaries-final.log`; historical local artifact unavailable) passed actual predecessor conflict, Confirmed and capture-pair recheck after equivalent lint fixes. Initial expanded six-pass/two-fixture-failure and initial strict seven-style-error logs remain retained; corrections did not weaken assertions. Receipt location `.build/rust-records-memory/automatic-application/validation.json` is the coordinator’s final logs/results/hashes/limits artifact location, pending creation after documentary hashes; no receipt hash or existence is inferred. Native API/Source qualification does not qualify live Dream, Runtime db94, IAB, HTTP, USER/non-Linux, performance or the whole Root integration. Qualified direct F1/F2 above do not qualify automatic application. Root approved `memory_store/automatic.rs` and narrow shared helper/test/doc ownership; shared interfaces/manifests/lockfiles and caller composition stay with their owners.

All four implemented async `NativeMemoryStore` APIs take `original: &Arc<SnapshotContext>`, actual borrowed `read: &ProjectReadAdmission<'_>`, `write: &ProjectWriteAdmission<'_>` and existing `MemoryStoreWriteLimits`. Both admissions must bind the same actual PROJECT, requested version and Original; require ReadMemory and FencedMemory with MemoryFenceRequired before I/O/lookup and at existing rechecks, including checks inside writer/read capture acquisition and its physical steps. No authored flags, DTO, observation or extra WriteMemory permission grants authority.

| Implemented method | Additional input | Result |
|---|---|---|
| `automatic_delete` | `id: MemoryId`, `input: DeleteMemory` | `Result<AutomaticDeleteOutcome, MemoryStoreError>` |
| `reobserve_automatic_delete` | Same id/input | `Result<Option<AutomaticMemoryEffect>, MemoryStoreError>` |
| `automatic_fold` | `input: FoldMemories` | `Result<MemoryFoldReceipt, MemoryStoreError>` |
| `reobserve_automatic_fold` | Same fold input | `Result<Option<MemoryFoldReceipt>, MemoryStoreError>` |

`FoldMemories` contains `kind: MemoryFoldKind::{Merge, Conflict}`, `survivor: MemoryFoldMember`, `members: Vec<MemoryFoldMember>`, `title: Option<String>`, `body: String` and `reason: String`. Each `MemoryFoldMember` contains `id: MemoryId`, positive `expected_revision: u64` and explicit `idempotency_key: String`. Members are only removal targets, at least one; survivor is separate, IDs are unique and all keys are pairwise distinct in the actual actor/project/version lookup namespace. Removal order is part of exact request identity. Keys are caller data, never a derived hash/prefix/truncation. Existing input/content/key/reason and per-row write limits apply before work; read policy retains query/native guards and batch hint without aggregate corpus limits; no unlimited defaults or new accounting framework. Body is explicit and nonblank, title None preserves. No authored attributes, tags, references, actor, clock or authority fields enter the fold DTO.

The implemented result `AutomaticMemoryEffect` contains `memory_id: MemoryId`, `before_revision: u64`, `after_revision: u64`, `idempotency_key: String` and `sdk_version: Option<u64>`. Fresh publication returns Some only from the actual SDK publish result; historical validated domain events return None because they do not persist SDK version. `AutomaticDeleteOutcome` is Deleted(effect) or Refused(`AutomaticDeleteRefusal::{Protected, LastRecord}`); missing/stale/invalid/Auth/SDK conditions retain actual MemoryStoreError. Refusal reserves no key and creates no event/version. `MemoryFoldReceipt` contains `survivor: AutomaticMemoryEffect` and `removed: Vec<AutomaticMemoryEffect>` in input order. One fresh group commit shares its actual SDK version; historical reobservation preserves original domain revisions without inventing an SDK version or receipt.

#### One fold, one conditional SDK publication

Observe all selected live full chains under one held Core Writer/pin and check every expected revision before preparing the group. Derive strongest classification and independent important/mandatory OR from actual observed members using `merge_attributes(sources: &[MemoryAttributes], context: &SnapshotContext)`. The main API and callsite cleanup under `tsk-7154` preserve original-request checks at entry, before each source and before success; historical automatic-application receipts remain separate. Preserve survivor creation fields, tags and references exactly; no union, target lookup or inbound-reference rewrite. Inference/content selection stays outside Writer; no universal semantic fact verifier is claimed.

Closed AutomaticDelete, FoldSurvivor and FoldDelete history identities are distinct from direct mutations, with private deny_unknown_fields codecs. Public MemoryFoldSurvivor and MemoryFoldDelete expose opaque observations: their fields are private to the Memory module and are stamped by the admitted application, not caller-authored grant data. FoldSurvivor records exact FoldMemories and actual scalar member observations (ID/revision/class/flags); full-chain derivation validates complete coverage/order and the derived after state. FoldDelete binds its own member revision/key, shared reason and survivor ID/key/published domain revision. Only typed automatic APIs stamp these observations; ordinary/direct APIs must not accept authored automatic facts. Closed persistence codecs do not prove a future host JSON parser.

Read each removal member’s complete chain, record its scalar observation, construct its final archive/tombstone batch and then discard that chain before reading the next. Derive the survivor anchor from the actual observations and insert its batch first in the prepared vector. All unique-row-key, same-schema batches exist before one strict existing PreparedDelta is published once on the same corpus/branch. Survivor flags/class/body and removal events become visible in the same SDK manifest; removal cannot precede intact survivor. An otherwise equal survivor receives a FoldSurvivor revision/event only when there are effective member removals, providing the exact request anchor; this is distinct from direct importance no-op. Do not call public mutators under the held Writer or introduce another lock, sequential group progress, pin-refresh/compensation protocol or group ledger.

Exact survivor-event lookup precedes new live-member requirements. Exact historical reobservation after deletions or later current changes returns the full original group without resurrection. Different ordered members/input/reason/key is an IdempotencyConflict; preexisting orphan/conflicting target-key events cannot be adopted. Reobservation validates the survivor event, every expected FoldDelete event, source/request agreement and full chains. A subset or tampered observations/source attributes/before/after invalidates the whole result, not a successful logical partial group. Existing get/history validate only the queried Memory chain: they reject local chain corruption but cannot certify another member’s FoldDelete without a cross-chain read. Missing members or cross-event group mismatch are rejected by reobserve_automatic_fold. Historical retry reads the stored anchor/input/observations before requiring new live members; it must not reconstruct historical observations from current live state. The applied cross-chain fixture contract keeps this distinction; implementation and fixture presence do not supply qualification. Reobservation is read-only: no birth, materialization, repair or replay; current legitimate admissions and fresh Original do not renew old authority.

Atomicity is limited to one native SDK delta. SDK failure retains actual DeltaCommitFailure key/cause; known publication followed by failed check retains Confirmed { version, cause }. Materialization or physical intent may exist before failure, and unknown outcome proves neither absence nor rollback. There is no new MemoryFoldFailure/progress/pending protocol, Auth+SDK atomicity or multi-action transaction. Across independent caller actions, preserve earlier successful receipts when a later action fails and use current-admitted reobservation before replanning; never blind-replay uncertainty.

#### Protected automatic bare deletion

Look up an exact prior automatic event before new-head CAS/protection. For a new attempt, validate live full chain and expected revision; important OR mandatory yields Protected. Count all active authorized heads through complete metadata EOF under the same Writer/pin, rejecting corrupt/foreign/duplicate input rather than a healthy prefix. One active head yields LastRecord. No corpus-wide body copy, persisted counter or model/class/action filter supplies this decision. Refusal creates no event/version/reservation; a later state may admit the unused key. Concurrent guarded deletes serialize to preserve at least one active head. A fold may remove protected members only while their flags/class are preserved by its actual survivor in the same manifest. Direct explicit delete/demote keep their separate qualified behavior.

#### Caller and pending evidence

Runtime `tsk-db94` is the existing caller/scheduler correction target. Its owner must close actual PROJECT/version/Original, Dream issuance, admitted reads/writes, typed AI completion/tool mediation and caller export before Source. Retain real idle/activity/watermark/current-run/rotation/cancel/drain contracts; Prompt, Kiro execution and embeddings precedent alone are not a Dream caller. No Records dependency on a Runtime umbrella, ledger/quota/heavy-slot framework or duplicate generic planner is introduced.

`tsk-74e7` requires the refinement and completed lifecycle/Auth/direct-fenced/attribute producers. Executed native coverage includes class precedence/OR flags and preserved tags/refs/creation, distinct keys and unchanged-survivor anchor, stale member rejection, strict predecessor conflict, exact historical reobservation after removals, corrupt partial/orphan/tampered events, protected/last-record/concurrent deletion, actual paired admissions/foreign Original/cancellation and default/named isolation. The existing production Confirmed boundary and final capture-pair recheck have actual private boundary proof; no SDK interception, lost-ack/Unknown injection or Auth+SDK atomicity is inferred. Native affected integration/library, Records doctests and scoped strict passed in the separate scopes above. Runtime/App/host/USER/non-Linux/performance delivery remains outside this qualification.

Implementation evidence reused from the refinement: `memory_store/rows.rs` 322–364 supplies unique head/revision batches; existing Data prepare_merge 417–463 accepts same-schema batches with duplicate-source Fail and strict read-version/zero retries; `memory_store/revision.rs` 162–228 supplies global actor/project/version/key lookup plus exact operation/Memory ID validation. The new [automatic application Source](../../crates/graphit-records/src/memory_store/automatic.rs) implements the four methods, one same-corpus PreparedDelta, complete metadata EOF protection and exact read-only reobservation; [automatic fixtures](../../crates/graphit-records/tests/memory_store/automatic.rs) have executed native Linux qualification. Model owns public authored/result DTOs and opaque MemoryFoldSurvivor/MemoryFoldDelete plus the private stored tags. Revision adds derivation/lookup_attempt; write::publish_version returns the actual SDK version while the ordinary publish wrapper preserves existing behavior. This qualifies the native API/Source component on Linux, not a live automatic Dream journey or caller. Qualified F1/F2 receipts remain unchanged.

Shared reference patches preserve omitted authored references and distinguish explicit empty replacement by a confirmed generation marker; structural links remain independent. Data/Knowledge own shared reference DTOs. No-op maintenance and pruning must preserve the confirmed head marker, empty-reference marker and unconfirmed future generations; Memory does not implement Knowledge corpus behavior.

## Recovery, inference and boundaries

Use the transaction proposal's original-authority admission, logical commit and operation receipts. Zero precommit publication does not promise zero physical intent writes. Applied/rejected/Uncertain boundaries, restart reobservation and orphan/winner proof require the actual Data/Auth writer; a read cannot perform cleanup. An uncertain commit is not rollback, and failure to finish a Runtime observation cannot erase an already committed Memory mutation. A ledger is not a prerequisite for the independently qualified direct F1/F2 handlers or planned automatic application.

Cold restart cannot reconstruct an old actor/run/issuer/request identity. Receipt/head reobservation and repair require new live authority, matching namespace/subject, current generation CAS and bounded original recovery context. Historical actors remain audit data, distinct from the current recovery actor. Idempotent retry of the same live attempt is distinct from recovery; neither grants new authority nor refunds work.

Inference runs outside read handles and critical sections. After a proposal, revalidate actual grant/admission and current expected head generation without renewing the original request. Qualified Memory vector publication uses the [canonical JSON document recipe and vector_id32/dimension identity](native_memory_embeddings.md#texto-documental-e-identidade), finite exact-shape validation and current source-fingerprint/CAS checks; the Records capture/attach/query/resolve library has executed Main/Linux tests and scoped strict exit0 under `tsk-a684`, while the compositor remains pending. Complete SDK scans preserve EOF/schema/keys/owner/Auth/cancel checks without aggregate row/text corpus limits; query/input/encoded-row/native guards remain concrete; inference is outside Writer, attach uses current Writer CAS and fresh source fingerprint, and recovery preserves known/unknown causes/keys. Fresh original/admission requires a new capture, never renewal of the old opaque. This Memory library qualification does not qualify ModelService/GGUF or the daemon consumer; delivered Knowledge/Data scopes retain their own evidence. No Go metadata coercion, padding or source-hash shortcut establishes compatibility.

Shared project records contain only non-sensitive project-inherent facts needed for continuity. Private-user records preserve legitimately durable preferences/corrections under their own authority. Secrets, prompts and model output do not belong in an operational ledger.

## Future fixtures and delivery gaps

M1–M12 preserve behavioral purposes. The separate PROJECT creation/catalogue receipts and 24-case lifecycle native execution cover bounded subsets; the lifecycle final library/docs/strict suites also passed. Future USER/Dream/search cases are not inferred Passed. Each implementation leaf needs accepted producer APIs/returned dependency IDs, positive limits, real substrate/authority tests, focused commands and domain documentation.

| Fixture | Required observation |
|---|---|
| M1 | All flag combinations, score conflicts and date/ID ties preserve priority and head collapse before limiting. |
| M2 | Stale update/delete fails without losing revision history or omitted metadata. |
| M3 | Two actors updating one head yield one winner without a history fork. |
| M4 | Archive faults block deletion; concurrent references cannot revive a removed target. |
| M5 | Project/private-user/version scopes remain isolated under actual authority. |
| M6 | Merge preserves independent flags, type precedence, unique evidence/references; stale heads and survivor failure cause honest outcomes. |
| M7 | Partial AI coverage and per-action failures cannot silently omit guidance or claim full success. |
| M8 | Query/tombstone/cursor/limit/cancel behavior preserves fixed generation, full-recall completeness and honest errors. |
| M9 | Foreign issuer/equal-realm/request/subject/source-generation/scope/version denies Records/Data corpus I/O, exposure and logical mutation before admission, using actual grant/capture/view; bounded original-context Auth/Core physical validation I/O remains permitted and separately measured. |
| M10 | Tracker Memory and Dream Task denial, four `ReadUser` denials and forbidden parent projections; PROJECT-only factory denies USER to Coordinator too. Native positive access now uses its separately qualified trusted-subject producer plus actual dual permissions and matching subject; wrong subject denies. |
| M11 | Living-chain current+archive matches collapse before limiting; current match wins, otherwise greatest numeric matching revision stays archived. Explicit deleted-history selection marks deleted+archived without resurrection. Current-head flags/date govern lexical priority, never archived metadata or score. Bounded PROJECT lexical integration/runtime passed under `tsk-612f`. The separate semantic library `tsk-a684` uses current-head filters, best cosine per Memory and distance/ID order after EOF, with its own executed Main/Linux qualification (86 distinct Memory cases, six Records docs separately, scoped strict0). USER/Dream and daemon/ModelService integration are not inferred. |
| M12 | Actual PROJECT ReadMemory grants permit complete bootstrap mandatory selection and bounded contextual exclude-mandatory/nonmandatory reads, including Dream consolidation; valid PROJECT Coordinator ReadMemory also permits with extra ReadUser, without USER selection. Distinguish filters before paging/collapse. Tracker, missing permission, revoked/foreign issuer/request and USER deny, with actual grant/view proof rather than Role enums. |

No consumer becomes ready merely from a readonly probe or qualified Auth admission. PROJECT read/write admission is delivered by `tsk-1f4a`; PROJECT captured reads, current catalogue, typed references and revision/CAS writer APIs are implemented, with their Linux receipts distinguished above. PROJECT lexical search has its own executed qualification above; semantic search, embedding, USER and Dream consumers still require their actual producer contracts and proofs. Any further search-specific sorting/validation work needs closed inputs/limits/cancellation, readiness and accepted gates; catalogue and lexical category/date/ID ordering are already implemented. Closed USER/per-call planning alone admits nothing. Current Native USER access uses the separately qualified Auth/Core/Records path above; consumer readiness still requires its own real composition and qualification.

Source consistency covers the Memory specification's current-match/archive-only selection, current-head category/date ordering, collapse before `top_k`, independent survivor flags and protected bare deletion, plus the actual Auth role matrix and PROJECT admission getters/issuer/request binding. PROJECT Auth is qualified; ordinary PROJECT publication, complete revision history and scoped reobservation are implemented and Linux-qualified with actual native/library/doctest/strict evidence. Root-approved archive/deleted lexical selection has its own executed Linux Memory qualification under `tsk-612f`; qualified SDK/Data producer evidence alone does not replace that proof; USER and Dream publication/recovery remain separate consumer obligations. Go persistence internals are not copied as contracts. `tsk-09f5` acceptance and `tsk-08ae` guard proof are completed library evidence; full-product consumer cutover remains held for its actual qualified Runtime/AI/USER/Dream producers as applicable. This documentation delta records existing producer execution and introduces no build or new product acceptance.

The current PROJECT/USER read policy is described and qualified in [the native read-policy section](native_memory_store.md#current-read-policy-qualification), under `tsk-265d`. Its 66 distinct reader/guard cases do not add to historical producer counts, certify a broad Dream matrix, or deliver the separate hybrid consumer.

## Hybrid query producer

The [PROJECT hybrid producer](native_memory_search.md#busca-híbrida-project),
`tsk-222c`, adds RRF across complete lexical and semantic rankings. Lexical
category/date/ID order and current/archive selection remain its own channel
contract; semantic distance/current-tie/highest-revision stays separate.
Hybrid orders fused score/ID after complete union and moves one selected current
head per ID, retaining each channel's matched revision/location and raw metric.
A single final pin/generation supplies all of that result. Eligible unavailable
coverage fails the hybrid result explicitly, without a healthy lexical prefix.

Prepared query text has a concrete encoded-input guard; no corpus quota or
history shortcut is reintroduced. The borrowed Model caller retains its
existing authority and lifecycle. Native/RRF and tiny RPC evidence belong to
this unit and do not add to old producer counts or certify a broad Dream,
USER hybrid, performance or SourceServer MCP delivery.

## PROJECT Memory Source consumer

`tsk-8140` integra o [contrato de busca Source MCP](native_memory_search.md#source-mcp-project-memory-search)
com as APIs nativas PROJECT lexical, semantic e hybrid. A admissão real
ReadMemory controla os três modos; o lease Embedding controla apenas os modos
com modelo e não substitui ReadMemory. O Source usa o M existente e não altera
stores, schema, Dataset, Writer ou o endpoint Role FTS.

A resposta conserva o head atual e a revisão/location de cada match, inclusive
archive e histórico deletado explícito, filtros AND atuais, total antes de top_k
e a geração do produtor. Hybrid conserva os canais separados sob um único pin
final, em vez de projetar metadados lexicais sobre matches semânticos. Zero hits
em corpus publicado continua Published; vazio físico e ausência têm estados
próprios. Coverage/identidade incompatível são Unavailable integral, com geração
e causa; M ausente/desabilitado não fabrica geração. A indexação real recupera
coverage depois de update; leituras não reconstroem índices.

As provas causais HTTP/RMCP incluem archive versus head comum, cinco filtros,
seleção project/version, update→MissingCoverage→indexação, delete e histórico
explícito, ausência/vazio sem RPC, Auth/lease separados, wire fechado e
cancel/deadline/join. Isso qualifica esse consumer PROJECT; as contagens dos
produtores e os contratos USER/Task/Dream mantêm seus próprios escopos.

A [qualificação Source MCP](native_memory_search.md#qualificação-source-mcp) registra
os três casos distintos e suas tentativas; resultados do modelo foram
requalificados depois da correção do lifetime de lease no delivery.
