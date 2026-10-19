---
title: "Native USER Memory data contracts and layout"
description: "Literal USER owner, shared Memory result DTOs and exact Arrow25 layout; storage qualification is scoped separately."
content-type: reference
audience: developers, ai-agents
references:
  - target: {type: task, id: tsk-67e7}
    relation: implements
  - target: {type: task, id: tsk-265d}
    relation: implements
  - target: {type: task, id: tsk-5995}
    relation: uses
  - target: {type: task, id: tsk-9613}
    relation: implements
  - target: {type: task, id: tsk-c623}
    relation: supports
  - target: {type: task, id: tsk-74cc}
    relation: derived_from
  - target: {type: task, id: tsk-319e}
    relation: uses
  - target: {type: task, id: tsk-633b}
    relation: uses
  - target: {type: task, id: tsk-5a58}
    relation: uses
  - target: {type: task, id: tsk-c62d}
    relation: supports
  - target: {type: task, id: tsk-fb6e}
    relation: supports
  - target: {type: task, id: tsk-85c4}
    relation: supports
  - target: {type: task, id: tsk-8e7b}
    relation: supports
  - target: {type: task, id: tsk-6286}
    relation: supports
  - target: {type: task, id: tsk-d7c3}
    relation: implements
---

# Native USER Memory data contracts and layout

For Records/Core/Auth/Data maintainers integrating USER Memory. `tsk-9613`
provides actual data APIs in [user_memory_store.rs](../../crates/graphit-records/src/user_memory_store.rs),
[model.rs](../../crates/graphit-records/src/user_memory_store/model.rs) and
[rows.rs](../../crates/graphit-records/src/user_memory_store/rows.rs). Source is
implemented and **Linux-qualified for data/layout only**: six USER functions
(four data/serde +two actual SDK), seven directly affected PROJECT guards and
scoped strict passed. These 13 distinct cases do not qualify USER store operations.

NativeUserMemoryStore now has actual ordinary store/search methods
in `tsk-c623`, backed by [native.rs](../../crates/graphit-records/src/user_memory_store/native.rs),
[store.rs](../../crates/graphit-records/src/user_memory_store/store.rs),
[search.rs](../../crates/graphit-records/src/user_memory_store/search.rs) and the
USER row codec. **The ordinary USER APIs are Linux-qualified by the c623 functional and scoped
strict evidence below**. Historical `tsk-9613` data/layout 13 retains its own scope.

The USER store, fenced and automatic operations, references, lexical/hybrid search
and embedding capture, publication and query are selected on Linux, macOS and
Windows. They retain the same typed USER admission, original request and Core
USER writer/directory capture. Source availability, target compilation and
native-host execution are separate evidence; historical Linux receipts keep
their recorded scope. The scoped Windows cross-target check covers the Records
library and the existing USER store and embedding integration targets; it does
not execute those tests on Windows. Native Windows and macOS runs remain
unqualified, and the Darwin cross-check remains blocked by the available C/C++
toolchain.
Fenced/automatic (`tsk-c62d`), full reference query (`tsk-fb6e`), semantic
(`tsk-85c4`), daemon (`tsk-8e7b`) and App (`tsk-6286`) remain separate deliveries.
Core `tsk-319e`, Auth `tsk-633b` and Web READ `tsk-5995` supply their own prerequisites.
See [PROJECT Memory](native_memory_store.md) for its separate delivered scope
and [reference coverage](record_relations.md#planned-cross-source-producers-and-coverage)
for cross-source obligations.

## Literal owner and logical scope

UserMemoryOwner has installation_id:InstallationId and identity:UserIdentity.
Its Serde shape rejects unknown fields. The installation codec writes
InstallationId::as_str(); on decode it constructs the actual Core InstallationId
and rejects stored text differing from that canonical result, instead of silently
normalizing it. Existing Core UserIdentity preserves literal issuer and subject
through its closed canonical Serde representation. Installation, issuer and
subject jointly distinguish owners; attribution actor strings are not subjects.

UserMemoryScopeId is a private String representation, derived solely by
from_owner(&UserMemoryOwner)->Result<Self,MemoryStoreError>, with as_str().
It serializes as a string and has **no Deserialize or principal parser**. The
string is the compact JSON array [installation.as_str(),issuer,subject], using
normal JSON escaping and preserving literal identity bytes. It is not a
concatenated display name, USER grant or way to reconstruct an admission.

Callers can derive a scope identifier or schema from these data values, but a
storage operation additionally needs its original request, actual typed USER
admission and real capture. Data equality/serde validity does not supply those
proofs. The historical `tsk-9613` data unit alone supplies no store operation or healthy-absence observation; current c623 methods require real admission/capture.

## Generation and shared domain DTOs

UserMemoryGeneration is a closed Serde object containing owner:UserMemoryOwner,
sdk_version:u64, physical_branch:Option<String> and inherited_target:Option<String>.
It describes the actual USER SDK observation; there is no ProjectId or Git/logical
PROJECT version in it. The current fixture uses the SDK default target.

The [model](../../crates/graphit-records/src/user_memory_store/model.rs) reuses
Memory, CreateMemory, MemoryMutation, MemoryActivityHead and MemoryLexicalHit
from the existing Memory domain rather than copying or downgrading their
canonical representation. Root-owned shared serialization remains its contract.

| Data type | Actual fields/variants |
|---|---|
| UserCreationMemory | owner,actor,input:CreateMemory,memory:Memory |
| UserMemoryRevision | owner,actor,operation:MemoryMutation,before,after |
| UserMemoryDetail | memory,creation:UserCreationMemory,generation |
| UserMemoryHistory | creation,current,deleted,revisions:Vec<UserMemoryRevision>,generation |
| UserMemoryHeadRead | NoCorpus or Published{generation,heads:Vec<MemoryActivityHead>} |
| UserMemoryListRead | NoCorpus or Published{generation,memories:Vec<Memory>} |
| UserMemoryLexicalSearchRead | NoCorpus, PublishedEmpty{generation}, or Published{generation,total_hits,hits:Vec<MemoryLexicalHit>} |
| UserMemoryLexicalIndexOutcome | NoCorpus, Ready(UserMemoryGeneration), or Published(UserMemoryLexicalIndexPublication) |
| UserMemoryLexicalIndexPublication | generation,recovery_key:DeltaRecoveryKey |

The historical `tsk-9613` shapes now support the implemented c623 methods below,
whose ordinary USER functional qualification is now recorded below. UserMemoryPublication
additionally carries generation:UserMemoryGeneration and recovery_key:DeltaRecoveryKey
for an actual ordinary committed delta. Actor/audit/recovery data is not authority.

## Private domain sharing: c623 progress

The first independently reportable `tsk-c623` unit adds five crate-private
scope-aware functions while retaining the existing typed PROJECT wrappers:
[write.rs](../../crates/graphit-records/src/memory_store/write.rs) contains
validate_references_in_scope and validate_input_in_scope;
[revision.rs](../../crates/graphit-records/src/memory_store/revision.rs) contains
validate_in_scope and derive_in_scope;
[rows.rs](../../crates/graphit-records/src/memory_store/rows.rs) contains
valid_memory_in_scope. They reuse the original domain bodies/rules with an
actual logical scope; USER does not require a fabricated ProjectId. No public
generic authority or admission API is introduced.

[tests/user_memory_store/domain.rs](../../crates/graphit-records/tests/user_memory_store/domain.rs)
contains three new pure USER functions, included under cfg-test. All three
passed: literal reference order/duplicates and create/snapshot validation;
omission/clear, domain revision versus reference_generation, importance/delete
and backward-clock rejection; and invalid context/input-byte limits/original
cancellation before derivation. USER pure runtime was 0.00s and compilation
32.49s. Nine directly affected PROJECT cases also passed (eight SDK +one domain
unit, 105.75s), giving **12 distinct functions for this helper unit**. Final
strict02 passed exit0 (3.84s) with the current helpers/cfg-test include; owned
formatting passed and the six-file Source manifest stayed unchanged. Prior
product-helper strict 44.59s remains history. Timings are evidence, not benchmarks.

These 12 are the first helper unit, not added to historical `tsk-9613`/13. It supplied
no USER Auth/capture/CRUD/CAS/SDK proof or new Confirmed type. Subsequent native/
store/search/row Source is Linux-qualified below. The separate data/layout
receipt retains its historical scope; Task lifecycle is tracked in Graphit.

## Exact USER25 Arrow layout

The actual user_memory_schema()->Arc<arrow_schema::Schema> delegates to the
private shared [schema_for(MemoryLayout::User)](../../crates/graphit-records/src/memory_store/rows.rs).
The existing schema() still selects MemoryLayout::Project and its 24-field
project-v2-embedding-list metadata. Only the physical field builder is shared.

USER replaces PROJECT project_id/operation_version immediately after record_id
with three required UTF8 owner strings. Its metadata is exactly
`graphit.memory.layout: user-v1-embedding-list`; ordinary fields 0..18 are non-null,
and the six embedding fields 19..24 are nullable. Indices are zero-based:

| Index | Fields | Arrow type/nullability |
|---|---|---|
| 0..2 | row_key,kind,record_id | Utf8 non-null |
| 3..5 | installation_id,user_issuer,user_subject | Utf8 non-null |
| 6..8 | idempotency_key,idempotency_actor,title | Utf8 non-null |
| 9..11 | revision,created_at,updated_at | UInt64 non-null |
| 12..13 | created_by,updated_by | Utf8 non-null |
| 14..15 | superseded,tombstone | Boolean non-null |
| 16 | reference_generation | UInt64 non-null |
| 17..18 | references,payload | Utf8 non-null |
| 19..20 | embedding_vector_id,embedding_fingerprint | FixedSizeBinary(32) nullable |
| 21..22 | embedding_recipe,embedding_role | Utf8 nullable |
| 23 | embedding_dimensions | UInt32 nullable |
| 24 | embedding_vector | List<Float32>, nullable column and nullable child named item |

This schema describes data, not permission to open/write an SDK dataset or
create its directory. A PROJECT24 or malformed USER25 layout is incompatible;
this unit provides no conversion/fallback. Nullable embedding columns do not
qualify USER embedding capture, attachment, cosine query or model inference.

<a id="native-user-apis-source-implemented-validation-pending"></a>

## Native USER APIs: Linux-qualified ordinary storage

All functions below are actual asynchronous associated methods of
NativeUserMemoryStore on Linux, macOS and Windows. First arguments are the same original
&Arc<SnapshotContext> and borrowed UserWriteAdmission (mutation/index) or
UserReadAdmission (read/query). Results are Result<_,MemoryStoreError>.
The implementation uses shared Memory domain types and actual USER owner,
without public generic authority or a fabricated ProjectId.

| Method | Other inputs | Result |
|---|---|---|
| create | CreateMemory,MemoryStoreWriteLimits | Memory |
| reobserve_create | CreateMemory,MemoryStoreWriteLimits | MemoryCreateObservation |
| update | MemoryId,UpdateMemory,MemoryStoreWriteLimits | Memory |
| delete | MemoryId,DeleteMemory,MemoryStoreWriteLimits | Memory |
| promote/demote | MemoryId,ChangeMemoryImportance,MemoryStoreWriteLimits | Memory |
| reobserve_mutation | MemoryId,MemoryMutation,MemoryStoreWriteLimits | Option<UserMemoryRevision> |
| get | MemoryId,MemoryStoreReadLimits | Option<UserMemoryDetail> |
| history | MemoryId,MemoryStoreReadLimits | Option<UserMemoryHistory> |
| observe_heads | MemoryHeadReadLimits | UserMemoryHeadRead |
| list | MemoryListFilter,MemoryStoreReadLimits | UserMemoryListRead |
| search_lexical | MemoryLexicalQuery,MemoryStoreReadLimits | UserMemoryLexicalSearchRead |
| maintain_lexical_index | MemoryLexicalIndexOperation,MemoryStoreWriteLimits | UserMemoryLexicalIndexOutcome |
| resolve_lexical_index | &DeltaRecoveryKey,MemoryStoreWriteLimits | DeltaResolution |

Ordinary write requires UserMemoryWriteOperation::Memory and Unconditional;
FencedMemory is rejected. Actual Auth admission requires ReadUser +ReadMemory
or MutateUser +matching Memory operation. Native/Incoming and Web READ retain
their existing factories; no Web write is supplied here. Store checks the exact
Arc against admission.request()?.snapshot(), rechecks identity/StoreOwner::User
and Core capture before/after public SDK awaits/batches and delivery. JWT follows
signed expiry and actual local native/provider/config observation; no per-query
UserInfo/introspection or instant remote revocation is promised.

## USER capture, absence and publication

The admitted installation USER namespace supplies
installation/users/<Core identity namespace_key>/runtime/memory/index.lance.
Core user_root is the USER namespace; owner_root is the installation. Caller
DTOs cannot choose another URI, PROJECT/Git version or branch. SDK opens exact
USER25 and capture_target(None); generation carries the actual default pin's
sdk_version/physical_branch/inherited_target, never a fabricated PROJECT value.

Readonly Core probe_memory establishes valid absence. Existing memory with a
missing/bad index is IncompleteCorpus or its causal SDK/schema error. Only
legitimate create uses the original ArtifactWriter/create_user_memory_directory,
held through pin/publication/output. Reads, reobserve, missing-ID mutation and
index operations do not birth/adopt/repair. Exclusive birth may conflict, and
later SDK/budget/recheck failure can leave physical residue/empty dataset;
no blanket NoEffect guarantee follows.

Creation/real mutation publishes head +immutable audit +reference marker in one
strict PreparedDelta. Marker key references/<MemoryId>/<domainrevision> carries
owner/revision/reference_generation/ordered authored edges, including empty and
tombstone. Each header/payload uses the literal installation/issuer/subject,
without ProjectId/operation_version. USER25 offsets and canonical row families
are checked; logical ordinary rows require nullable embedding 19..24 null.
Markers are decoded separately, never treated as embedding candidates.

Attempts bind owner +actual attributed actor +literal key/typed input, without
Git-version namespace. Complete same-pin lookup precedes new expected-revision
CAS. Exact historical retry returns its original result after later mutations/
delete without reapplying or resurrecting; changed target/operation/input conflicts.
Omitted refs preserve, [] clears. Reference generation changes only on effective
reference change/delete, while body/importance also gets a new domainrevision
marker. Ordinary delete tombstones/clears and keeps full history, including
important/mandatory records under actual ordinary write authority. Importance
changes important only; new satisfied promote/demote checks expected revision
and final authority, returning without event/SDK publication. A historical retry
remains distinct. Strict captured SDK CAS rejects stale decisions without replay.

## USER reads, limits and recovery

get/history validate the complete matching chain through EOF at one pin: unique
head/creation/attempts, contiguous revisions, before/after equality, owner/header
attribution, final head and terminal deletion. Deleted get is None only after
validation; history retains current/deleted content. Missing/corrupt members are
errors. List uses AND classification/tag/mandatory/important/exclude_mandatory,
literal nonblank tag checks after Auth and before corpus access, and category
mandatory >important >normal, updated_at DESC, ID ASC ordering. Metadata projects
seven columns (ID, three owner strings, title, actor, time), without body/history.

Absent list/heads/search/index is NoCorpus, absent get/history is None. Present
empty list is Published{generation,memories:[]}; metadata is
Published{generation,heads:[]}. Search has PublishedEmpty{generation} for an
empty logical row set, including physical empty; native row count and physical
schema checks establish that distinction without decoding every chain. Denial/storage/corruption/cancel/late candidate error is never a healthy
prefix/empty result. MemoryStoreReadLimits bounds only the raw/expanded lexical query through
max_query_bytes; native file checks and batch_rows remain. Head reads contain
only native limits and batch_rows. Complete PROJECT/USER scans have no aggregate
row/text quota; write input and individual encoded-row guards remain functional.
Scans consume EOF and batch_rows is an SDK hint, not a result cap.

UserConfirmed{publication:Box<UserMemoryPublication>,cause} preserves actual
ordinary committed USER generation/recovery_key after failed final recheck.
UserIndexConfirmed preserves UserMemoryLexicalIndexPublication/key for the index
case. MemoryStoreError::source forwards each causal error. Unknown/failed commit
remains real Commit(DeltaCommitFailure), not rollback or assumed success.
Fresh actual admission/Original is required for reobserve/resolve.
resolve_lexical_index validates exact USER URI/default physical binding before
I/O, returns actual DeltaResolution and never births/replays. There is no
reobserve_lexical_index method. Point-in-time Core checks do not continuously
confine pathname SDK I/O or make Auth/SDK atomic.

## USER lexical index and query

[search.rs](../../crates/graphit-records/src/user_memory_store/search.rs) uses the
shared [Memory lexical pipeline](native_memory_search.md) with USER owner/
generation and actual UserReadAdmission. Query/tag validation precedes capture;
actual payload JsonFtsIndexState and complete USER chains for FTS candidate IDs
are checked at one pin. Search does not load unrelated histories or audit the
whole corpus; full validation remains part of index construction. Candidate
histories are validated before filters. A single native scan filters the candidate
IDs with IN and orders by record_id; each history is discarded after ranking,
retaining only the requested top-k bodies and the total hit count. There is no
scan per ID or reread of the selected results.
Missing/Incompatible raises LexicalIndexUnavailable, not healthy flat
NoHits. Read never creates, repairs, refreshes or publishes an index.

The actual create/mutate writer initializes a missing derived index after validation, CAS/idempotency and encoded batch checks, under the same Subject/actor/installation/scope and ordinary or legitimate Fenced policy. It retains the writer and uses the actual committed index pin for the CRUD merge. Ready is reused; incompatible policy fails. Later merges/deletes/folds do not rebuild the index. Read/search/GET remain readonly.

Explicit maintain_lexical_index requires ordinary USER write. Ensure returns NoCorpus, Ready(actual UserMemoryGeneration) without a transaction, or Published(actual publication). Missing indexes use Data Create after complete chain validation; Refresh requires Ready, validates complete chains and uses Data Replace. The default USER physical branch and owner remain exact. Candidate eligibility admits head/creation/revision only; marker bytes cannot become hits even when physically indexed.

UserIndexConfirmed retains an index publication and any later CRUD failure. When CRUD also confirmed, UserConfirmed stays at the top and its cause retains UserIndexConfirmed; generation/recovery keys of both effects remain distinct. Neither index confirmation nor its SDK version proves a CRUD operation committed. The shared [maintenance contract](native_memory_search.md#inicialização-e-manutenção-do-índice) defines the causal outcomes and current authority checks.

MemoryLexicalQuery uses text, MemoryListFilter, include_deleted_history and top_k.
Current matches win over archives; otherwise highest numeric matching archive
wins. Explicit deleted histories remain archived/deleted without resurrecting get.
Current-head filters and shared category/date/ID order precede unique-ID top-k;
score is metadata. Expansion uses the 12 allowlisted paths, simple/lowercase JSON
policy, no stem/stopword/accent folding or max-token cap, SDK OR/no result limit.
This ordinary USER pipeline is **functionally qualified on Linux** within the
evidence below; it does not qualify the separate consumers.
Fenced/automatic/semantic/reference query and daemon/App/browser remain separate.

<a id="current-c623-proof-and-pending-user-validation"></a>

## c623 Linux qualification and evidence boundaries

There are **115 distinct functional passes**: USER SDK15 (27.19s), USER library5
(2.97s: three pure functions and two real postcommit delta/index boundary tests),
PROJECT SDK68 (529.19s) and PROJECT library27 (48.73s). The library run of 30
includes the three USER pure functions already counted in USER5. The final three
PROJECT input-budget guards passed in 10.26s as a subset of SDK68. Neither focal
reruns nor the historical `tsk-9613`/13 and first helper 12 are added to 115.

USER boundary tests commit real PreparedDelta/PreparedIndex, then cancel the
original request and recover with fresh actual admission: ordinary reobservation
and index resolution preserve the actual generation/key/cause without a second
commit or rebuild. This is private production composition, not intercepted SDK
lost acknowledgement or atomic Auth/SDK proof.

Source compile09 no-run passed (48.88s). Final scoped strict passed exit0 (7.23s)
with `cargo clippy --locked -p graphit-records --lib --test user_memory_store
--test memory_store --no-deps -- -D warnings`; owned formatting of 23 files and
diff checks passed. Six Records doctests passed separately: four ordinary and
two compile_fail (1.78s execution, 2.64s run wrapper). These are Records docs,
not six executable Memory tests and not additions to the functional 115. The
ordinary qualification receipt (`../../.build/rust-records-references/user-memory-ordinary/validation.json`; historical local artifact unavailable)
consolidates scoped results, commands, Source hashes and failure history.

Preserved failure history includes USER SDK attempt01 fixture InvalidPolicy
(0/10); attempt02 (7/10, product no-op and native/search fixture corrections);
attempt03 stale binary, which supplies no qualification; attempt04 (12/14,
list/head/raw UTF8 oracle corrections); and final attempt05 (15 passes).
Compilation history retains foreign Data E0451 in05, fixture types in06 and
fixture E0716 in08, corrected before compile09. Assertions and scope are not
weakened by these corrections.

This qualifies native ordinary USER storage, authority-bound CRUD, history,
metadata/list and lexical index/query on Linux. It does not qualify Fenced/
automatic `tsk-c62d`, reference query `tsk-fb6e`, semantic `tsk-85c4`, daemon
`tsk-8e7b`, App `tsk-6286`, global FullNoLoss, live Runtime, Windows/macOS or
performance. Task lifecycle is independent from these recorded execution results.

<a id="user-reference-query-implemented-sdk-qualification-pending"></a>

<a id="user-reference-query-linux-functional-proof-final-strict-pending"></a>

## USER reference query: Linux-qualified native API

`tsk-fb6e` implements async NativeUserMemoryStore::references with
original:&Arc<SnapshotContext>, admission:&UserReadAdmission<'_>,
query:UserMemoryReferencesQuery and limits:MemoryStoreReadLimits, returning
Result<UserMemoryReferencesRead,MemoryStoreError>. The actual
[USER facade](../../crates/graphit-records/src/user_memory_store/references.rs) and
[DTOs](../../crates/graphit-records/src/user_memory_store/model.rs) compile in
check02 --lib (exit0, 3.80s). The functional SDK evidence below covers the
current USER query on Linux; current strictfinal03 passed after space recovery.
This proof is separate from the sealed c623 ordinary 115+Records doc6 receipt.

UserMemoryReferencesQuery is closed Serialize/Deserialize data:
outgoing:Option<MemoryId>, incoming:Option<MemoryReferenceTarget>,
page_size:NonZeroUsize (1..100), outgoing_cursor and incoming_cursor as
Option<UserMemoryReferencesCursor>. It reuses the existing public
MemoryReferenceTarget; there is no USER target alias. At least one focus is
required; a cursor needs its matching direction/focus. The literal target's four
kinds/scope/id/context specify identity, never a target read grant. Actual USER
ReadUser+ReadMemory admission, owner and same Arc Original precede preflight,
capture/I/O and each await/batch/output; PROJECT ReadMemory cannot substitute.

UserMemoryReferencesRead is snake_case NoCorpus or Published with generation,
outgoing/incoming:Option<Box<UserMemoryReferencePage>>,
coverage:Box<UserMemoryReferencesCoverage> and
warnings:Vec<MemoryReferencesWarning>. A page contains results, total, complete
and next_cursor. Each UserMemoryReferenceEdge contains source:MemoryId,
source_scope:UserMemoryScopeId, source_revision, reference_generation,
source_snapshot:UserMemoryGeneration, reference:MemoryReference,
provenance:MemoryReferenceProvenance, ordinal, source_observation and
target_resolution:MemoryReferenceResolution. The source_scope remains canonical
Serialize-only data; no principal parser/admission constructor was added.

Source observation is minimal ID/title from validated same-pin head metadata.
Target resolution is Memory{id,title} only for a live canonical MemoryId in this
exact USER scope with context None. Other owners, installations, literal issuers,
PROJECT/Task/Session/Knowledge/import targets and opaque or missing IDs remain
Unresolved, with no extra lookup, invented title/href or authority. Coverage has
exactly owner, memory_user_source_complete and
inbound_other_sources_not_qualified; no aggregate rich-resolution flag. Boxing
changes Rust layout only; serialized JSON and the concrete generation are unchanged.

The closed cursor has private binding/direction/offset. Binding uses the actual
admitted runtime/memory/index.lance NativePath, owner through UserMemoryGeneration,
default SDK pin/physical_branch/inherited_target, both foci, direction and offset.
Page size may change within 1..100. Batch/query policy changes do not invalidate
continuation; root, owner, generation and focus changes do. Opaque native file
limits are rechecked per call, never serialized as authority. Offset>total fails;
offset==total is valid empty, and NoCorpus plus any cursor fails.

Original/admission and typed query/target/cursor checks precede capture and
NoCorpus. The reader consumes consulted heads and markers through EOF without
aggregate rows/edges/text accounting. Both reference JSON copies must decode
and match canonical schema/header/current generation. The
[shared private query_pin](../../crates/graphit-records/src/memory_store/references.rs)
selects actual PROJECT or USER owner decoder through the two typed
facades. Candidate filters include canonical row_key prefixes as well as kind
(head/% and references/%), so altered kind is rejected by full decoding rather
than silently omitted. It reads validated heads and every marker through EOF,
including old, orphan and duplicate/tampered rows. The complete
[USER marker decoder](../../crates/graphit-records/src/user_memory_store/rows.rs)
checks schema/family/owner/header, null embedding columns and canonical payload;
no PROJECT placeholder or operation_version is fabricated for USER.

Current marker must match head domain revision, reference_generation, tombstone
and literal references. Reference generation changes only with authored refs or
delete, separately from domain revision. Missing current marker yields warning
and incomplete own-source coverage; old edges are not adopted and no repair
occurs. Deleted/missing outgoing is NotFound; incoming with zero edges is a
valid published page. Retention is bounded minimal head/key metadata and selected
pages/counters at one pin, not full histories, bodies or all edges. Late errors
invalidate the query; no healthy prefix or source-global completion is inferred.


The historical `tsk-d7c3` delta removed the shared query-output counter and
serialization-preview pass while retaining explicit Original checks in edge,
cursor and final delivery paths. Public DTOs/cursors, Serde delivery, raw scan
caps and causal errors remain unchanged. Source02 compile01 and nine affected
USER reference cases passed; eight PROJECT reference cases passed on the same
shared Memory/USER Source, unchanged in Source03. Current Task revalidation,
compile02, scoped strict02 and fmt02 passed after the preserved own strict01
empty-accounting-loop failure and its Source03 correction. This removal is
Linux-qualified for 40 distinct affected functions, retaining the PROJECT8 and
USER9 results without replay counts; it establishes no broader domain/platform
qualification.
The sealed `tsk-fb6e` 51-case qualification below retains its historical scope;
these affected reruns are not added to that producer count.

The query has **51 distinct functional passes**: USER SDK24 (15 causal +nine
new journeys), USER library5 (three pure +two real postcommit boundaries, now
including fresh reference query), PROJECT SDK10 (eight reference and two
lexical/embedding guards), and library write-boundary12 including Confirmed
references. USER library5 ran in 3.22s; PROJECT refs8 in 42.52s, exclusions2 in 27.39s
and library12 in 22.89s. Six Records doctests passed separately (2.11s), not added
to 51. Real boundary composition does not intercept SDK lost acknowledgement.

SDK Source01 passed 23/24 and final query02 passed 8/9; the sole remaining Native/
Web journey required fixture ProviderNone/actual native production and the
no-hit FTS oracle Published{total_hits:0,hits:[]}, not PublishedEmpty. Current
Source03 passed that corrected journey (8.96s), with no Product Source changes
from01 through03. The successful coverage above preserves both earlier failed
runs; no overlapping retry is added to the distinct counts.

Source/Product strict02 passed (4.54s), target01 (13.29s) and final01 (17.09s)
before the last test-only oracle correction. Required strictfinal02 failed due
to ENOSPC while producing Core rmeta; that failure remains preserved. After
coordinator space recovery, current Source03 strictfinal03 passed exit0 (11.04s).
The single corrected Native/Web SDK journey passed (8.96s), qualifying the
51 distinct functions without replay or addition of overlapping runs. Earlier
strictsource01 large-enum failure (232-byte variant) was fixed by boxed coverage
without lint suppression. Owned fmt11/diff13 passed. The existing
query qualification receipt (`../../.build/rust-records-references/user-memory-query/validation.json`; historical local artifact unavailable)
consolidates commands, results, Source hashes, failure history and limits.
This qualifies the native USER references API on Linux. App/global FullNoLoss,
Fenced/automatic, semantic, non-Linux and other-source coverage remain outside
this native functional proof. Rich-target coverage is a separate authorized
dimension.

<a id="user-fenced-and-automatic-implemented-sdk-qualification-pending"></a>

## USER fenced and automatic: qualified native Linux scope

`tsk-c62d` supplies actual Linux Source in
[fenced.rs](../../crates/graphit-records/src/user_memory_store/fenced.rs) and
[automatic.rs](../../crates/graphit-records/src/user_memory_store/automatic.rs).
The qualified focused scope has **40 distinct functional passes**: USER26 and
PROJECT14. USER26 consists of 11 new SDK functions (fenced3 on source03 and
current automatic8 on source04), three real postcommit library functions, ten
causal SDK guards (ordinary7/history2/marker1) and two existing USER Confirmed/
index library functions. PROJECT14 consists of eight automatic SDK and two
fenced SDK functions, plus three automatic causal library functions and one
fenced Confirmed library function. These are scoped distinct functions, not a
full Memory suite or consumer qualification; overlapping runs are not added.

Current automatic8 passed (46.09s), including the new USER metadata-EOF fixture,
and the three new actual USER postcommit functions passed (13.54s). Source05
changed only the output-boundary fixture to retain the actual removed-group
receipt: its single exact pass (4.61s) is already included in those three, not
an extra function or replay of all three. Fenced3 passed (36.89s). The current
output guard preserves actual UserMemoryPublication and maps a failed final
recheck after receipt construction to UserConfirmed with generation/key/cause;
real SDK publication plus cancellation/fresh historical recovery is tested.
This is private production composition, not SDK lost-ack interception.

Source04 compiled exit0 (22.82s), and current scoped strict passed exit0 (26.38s)
with --lib --test user_memory_store --no-deps -- -D warnings. Six Records
doctests passed separately (2.10s), not executable Memory tests and not additions
to 40. Owned fmt13/diff15 files passed exit0. Earlier check lib0 (3.91s), strict
lib0 (24.02s), source03 preliminary ten-SDK/two-postcommit proof and strict
targets03 (15.09s) remain historical evidence, not additive counts. The known
post-receipt output-guard gap was corrected and the current automatic/postcommit
scope rerun; its causal contract remains actual USER owner/generation/key/error.
Historical ordinary/query qualification and immutable receipts retain their
scopes. No live Dream USER capability or Task completion is inferred.

Every direct fenced method takes original:&Arc<SnapshotContext>,
admission:&UserWriteAdmission<'_> and MemoryStoreWriteLimits. Private
[UserWritePolicy::Fenced](../../crates/graphit-records/src/user_memory_store/native.rs)
requires the real UserMemoryWriteOperation::FencedMemory operation and
Obligation::MemoryFenceRequired, original Arc identity, literal owner and actual
admission rechecks. Ordinary Memory admission cannot substitute; ordinary APIs
retain their existing Memory-only policy. No caller field supplies a grant.

| Actual asynchronous method | Additional input | Result value |
|---|---|---|
| fenced_create | CreateMemory | Memory |
| reobserve_fenced_create | CreateMemory | MemoryCreateObservation |
| fenced_update | MemoryId,FencedUpdateMemory | Memory |
| fenced_delete | MemoryId,DeleteMemory | Memory |
| fenced_promote / fenced_demote | MemoryId,ChangeMemoryImportance | Memory |
| reobserve_fenced_mutation | MemoryId,FencedMemoryMutation | Option<UserMemoryRevision> |

All results use Result<_,MemoryStoreError>. Typed update changes only title/body/
references, preserving class/flags/tags/creation; refs None preserves and an
explicit list, including [], replaces. Direct delete conditionally archives
without an automatic important/mandatory restriction. Importance changes only
important; expected CAS precedes a new no-op and exact historical replay returns
the original event. USER writer, owner25 codec, current marker, strict SDK delta,
lookup/CAS and causal UserConfirmed are shared with ordinary production paths.
This typed Rust contract is not a host JSON parser/handler qualification.

Automatic methods additionally require read:&UserReadAdmission<'_> paired with
write:&UserWriteAdmission<'_> and the same original Arc. Actual checks require
matching literal owner and runtime_root; capture and each SDK boundary recheck
the pair and guard. Names/results are automatic_fold(FoldMemories)->MemoryFoldReceipt,
reobserve_automatic_fold(FoldMemories)->Option<MemoryFoldReceipt>,
automatic_delete(MemoryId,DeleteMemory)->AutomaticDeleteOutcome and
reobserve_automatic_delete(MemoryId,DeleteMemory)->Option<AutomaticMemoryEffect>,
all with MemoryStoreWriteLimits and Result<_,MemoryStoreError>.

Fold validates actual live observations/CAS at one pin, derives strongest class
and independent OR flags, preserves survivor refs/tags/creation, uses explicit
nonblank body and preserves title when omitted. Each effect has a distinct key.
One strict PreparedDelta publishes survivor and all member archive/tombstones
with their head/audit/current-reference markers atomically within the SDK; deleted
current references are empty. The survivor anchor is first in the publication
batch. There is no sequential group progress, ledger or Auth+SDK transaction.
Bare automatic delete refuses important/mandatory and the last live record;
these restrictions do not become universal direct-delete predicates.

AutomaticMemoryEffect and MemoryFoldReceipt are shared pure, owner-free result
data. An effect has memory_id,before_revision,after_revision,idempotency_key and
sdk_version:Option<u64>; Some is only the actual manifest committed by this call,
None is historical exact retry/reobservation. USER Confirmed retains actual
owner/generation/key and typed cause; failed/unknown SDK commit is not rollback,
replay permission or guaranteed no-effect. Independent caller actions can still
have separate outcomes; no multi-action atomicity is introduced.

Whole-fold reobservation uses historical anchor/member events through readonly
capture, without writer, birth, materialization or reconstructing live members.
It rejects missing/partial/cross-event disagreement; ordinary get/history certify
only their queried local chain. Fresh admission/capture is required for a fresh
Original after cancellation. No SDK lost-ack interception, atomic Auth fencing,
App/executor, ModelService, semantic, non-Linux or performance proof is claimed.
The [Dream boundary](rust_dream_records.md#native-user-fenced-and-automatic-source) still
denies ReadUser for Role::Dream. A non-Dream USER admission/API is not a new
Runtime Dream USER capability.

<a id="user-semantic-capture-and-publication-implemented-sdk-validation-pending"></a>

## USER semantic capture and publication: Linux-qualified native SDK

`tsk-85c4` has actual Source in
[embeddings.rs](../../crates/graphit-records/src/user_memory_store/embeddings.rs)
and its [capture](../../crates/graphit-records/src/user_memory_store/embeddings/capture.rs),
[publish](../../crates/graphit-records/src/user_memory_store/embeddings/publish.rs)
and [query](../../crates/graphit-records/src/user_memory_store/embeddings/query.rs)
modules. Native Linux qualification passed **60 distinct functional cases**:
USER45 (nine semantic SDK +35 causal store +one actual Confirmed library function)
and PROJECT15 (12 semantic SDK +three codec/publication library functions).
The four-case library filter includes the already-counted USER Confirmed case;
focused/rerun cases are subsets, not additional functions. Six Records doctests
passed separately, not executable Memory cases or additions to 60.

Fixture compile07 and strict-final07 passed exit0; strict uses the owned Records
library and affected test targets with --no-deps -- -D warnings. Current Product
Source04 is unchanged through05/06/07, which changed only fixtures. Current USER
ranking and the three focused PROJECT chunk/cap/inherited cases passed. The
stale user-rank-one-05 binary before current compilation is invalid qualification
evidence; the correctly compiled ranking rerun supplies the passing observation.
All prior failed logs remain preserved; no failed/stale run is silently promoted
to PASS and no duplicate run is added. Detailed wall/runtime timing belongs to
execution provenance JSON/logs, not an inferred benchmark.

Historical layout, ordinary, reference-query and fenced/automatic receipts keep
their own scopes. This qualifies the native USER semantic SDK/library contract,
not the separate daemon consumer `tsk-8e7b`, model RPC or Task lifecycle.

All native methods are asynchronous and use the same original:&Arc<SnapshotContext>
and actual concrete USER admission; results are Result<_,MemoryStoreError>.
The API input/output contracts are:

| Method | Other inputs | Result value |
|---|---|---|
| capture_embeddings with UserReadAdmission | MemoryEmbeddingIdentity,MemoryStoreReadLimits | UserMemoryEmbeddingCorpusRead |
| attach_embeddings with ordinary UserWriteAdmission | &UserMemoryEmbeddingCorpus,Vec<MemoryEmbeddingAttachment>,MemoryStoreWriteLimits | UserMemoryEmbeddingAttachOutcome |
| resolve_embeddings with fresh ordinary UserWriteAdmission | &DeltaRecoveryKey,MemoryStoreWriteLimits | DeltaResolution |
| search_semantic with UserReadAdmission | MemoryEmbeddingIdentity,&[f32],MemorySemanticSearchOptions,MemoryStoreReadLimits | UserMemorySemanticSearchRead |

UserMemoryEmbeddingCorpusRead is NoCorpus or Published(UserMemoryEmbeddingCorpus).
The opaque corpus retains the real USER read guard, literal owner/runtime root,
original Arc and default BranchSnapshot pin. generation()->&UserMemoryGeneration
and identity()->&MemoryEmbeddingIdentity borrow data, not authority. Its async
next_documents(&mut self,original:&Arc<SnapshotContext>,admission:&UserReadAdmission)
returns Result<Option<Vec<MemoryEmbeddingDocument>>,MemoryStoreError>; None is
EOF, including a valid published corpus with no eligible documents. Each call
checks original pointer, current admission, literal owner/root and retained guard.
A fresh request requires new admission/capture, not renewed old opaque authority.

Identity/document preparation reuse the pure MemoryEmbeddingIdentity,
MemoryEmbeddingDocument and prepare_memory_embedding_document contract: recipe
memory-document-json-v1, actual vector_id32/dimension, literal JSON tuple title/
body/tags and exact text fingerprint. Metadata flags/reference/time changes do
not invent a different content fingerprint; revision remains an independent
binding. This produces input data only, not ModelService permission or inference.

The pure encode_user_memory_embedding_row(owner:&UserMemoryOwner,
document:&MemoryEmbeddingDocument,vector:&[f32],MemoryStoreWriteLimits) returns
Result<RecordBatch,MemoryStoreError>. It writes exact USER25: owner strings at
3/4/5; config/fingerprint at19/20; recipe/Document role21/22; dimensions23 and
List<Float32>24. The embedding key is embedding/<id>/<revision>, with closed
ID/revision payload, neutral logical headers and empty references. Vector length
must equal actual identity dimensions and components must be finite. Checked
4*dimensions+56 input bytes precede Arrow List retention; actual owner/text,
recipe/role and encoded row limits apply. This encoder performs no publication,
Auth or model call and does not fabricate PROJECT fields.

The logical decoder identifies the embedding family before applying the nullable
embedding-field gate to logical rows. Reference markers are a separate explicit
family, never documents, candidates, attachments or semantic hits; decode(None)
is not a generic embedding signal. Existing CRUD/history/list/lexical/reference/
automatic readers were exercised after real USER embeddings in the 35 causal
USER store cases above. Their historical receipts are not substituted or counted
again; current causal proof has its own scope. Schema conversion or fallback is absent.

Attach requires ordinary operation Memory, same captured Original/owner/root
and valid captured-read plus current-writer guards. Only the emitted ID/revision/
fingerprint allowlist is accepted; reject duplicates, nonfinite/wrong-dimension
vectors and un-emitted data. Validate immutable sources/fingerprints again at the
current writer pin before prepare_merge/strict CAS. No birth or arbitrary content
injection occurs. Embedding metadata changes neither domain/reference revision
nor produces fake markers. Existing{generation} returns actual current USER
generation without a recovery key/publication; Published(UserMemoryEmbeddingPublication)
contains generation and recovery_key from the actual committed SDK delta.

UserEmbeddingConfirmed carries Box<UserMemoryEmbeddingPublication> and typed
causal error with forwarding Error::source, preserving owner/generation/key after
failed postcommit recheck. Unknown SDK commit remains causal, not rollback or
replay permission. Resolve uses fresh ordinary write admission and exact USER
URI/default physical branch; it returns actual DeltaResolution under a fresh
captured current pin without birth/materialization/repair/replay. It does not
reconstruct an initial publication or turn key/identity data into a grant.

Semantic read has four exact states: NoCorpus; PublishedEmpty{generation};
Unavailable{generation,reason:MemoryEmbeddingUnavailable}; and
Published{generation,total_hits,hits:Vec<MemorySemanticHit>}. One query pin validates
complete chains and validates embedding identities/fingerprints. Compatible
revisions are searchable while other revisions remain pending. Unavailable means
no eligible compatible vector exists, rather than incomplete total coverage.
Malformed rows and authorization/cancellation failures remain errors, never a
healthy prefix. Covered revisions are skipped; new revisions with identical
Document fingerprint reuse validated vectors through the ordinary attach/CAS.
The five current MemoryListFilter predicates and include_deleted_history select
heads before scoring; updated_at is metadata, not an implicit date filter.
Mature Data cosine over persisted List<Float32> validates actual finite vectors.
For each Memory ID select minimum distance across eligible immutable revisions,
tie current then highest numeric revision; collapse before unique top_k, ordering
by distance then ID after EOF. Hits return current head, matched_revision, location
and distance, with no invented deleted bool or resurrection of tombstoned get.

Private PROJECT/USER row codecs, borrowed HistoryView, discovery and scoring
helpers are actually shared by both typed paths. PROJECT public APIs remain
unchanged; there is no copied whole-module pipeline, public generic grant,
model authority, quota/ledger or RSS guarantee. The model consumer retains the
same captured pin through Query inference and result selection. Ordered SDK
scanning validates one Memory chain and its embeddings at a time; only ranks
and lexical candidate metadata survive between groups. Selected top_k bodies
are read from that same pin, without a chain/vector scan for every ID. The
[USER semantic query note](native_memory_search.md#user-semantic-source-compilado-validação-sdk-pendente)
keeps lexical ordering and semantic selection distinct. Daemon `tsk-8e7b` must
supply real ModelService/lease/ExpectedModel integration; no RPC, App/global,
Runtime Dream USER, non-Linux or performance qualification is claimed here.

## Written fixtures and qualification boundary

[tests/user_memory_layout.rs](../../crates/graphit-records/tests/user_memory_layout.rs)
contains six new functions: literal compact scope/escaping and owner separation;
closed owner/generation serde with noncanonical installation rejection; shared
canonical audit/history serialization preserving PROJECT DTO shape; exact
USER25 schema comparison; real SDK empty typed batch/default pin,
close and reopen; and SDK rejection of PROJECT/malformed layouts without a
converter. Four are data-contract fixtures and two use actual Lance storage.
The actual USER target passed all six (0.02s), including the two SDK cases;
compilation took 13.40s. Direct PROJECT guards separately passed seven: four SDK
cases (create concurrency, incompatible old schema, embedding roundtrip and
reference publication) and three library guards (two canonical serialization
and one projected-schema case). These are **13 distinct functions**, not added
to older PROJECT qualification totals. PROJECT compilation took 14.20s;
three preexisting cfg-test warnings were preserved. Timings are execution
evidence, not benchmarks.

Focused producer commands are cargo test --locked -p graphit-records --test
user_memory_layout, scoped lib/PROJECT schema guards, and cargo clippy --locked
-p graphit-records --lib --test user_memory_layout --no-deps -- -D warnings,
plus owned formatting/diff and documentation QA. Actual scoped strict
--lib --test user_memory_layout --no-deps -- -D warnings exited zero (4.90s),
with owned Source formatting/diff0. The Source compile01 manifest stayed
unchanged through the runs. No new doctest/rustdoc execution is claimed.
Qualification receipt location:
`.build/rust-records-references/user-memory-layout/validation.json`.
This data/layout qualification supplies no USER CRUD/Auth/caller/semantic
runtime, global FullNoLoss, Windows/macOS or performance qualification.

The current PROJECT/USER corpus and reference policy is qualified separately under `tsk-265d`; see [current read-policy qualification](native_memory_store.md#current-read-policy-qualification). Existing USER receipts below retain their original scope and quotas, and are not added to that count.
