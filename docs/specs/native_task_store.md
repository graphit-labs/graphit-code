---
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-ea469}
    relation: implements
  - target: {type: task, id: tsk-59c7}
    relation: implements
  - target: {type: task, id: tsk-13af}
    relation: implements
  - target: {type: task, id: tsk-23f2}
    relation: relates_to
  - target: {type: task, id: tsk-b777}
    relation: requires_contract
  - target: {type: task, id: tsk-8923}
    relation: supports
  - target: {type: task, id: tsk-f382}
    relation: implements
  - target: {type: task, id: tsk-9a83}
    relation: requires_contract
  - target: {type: task, id: tsk-cd41}
    relation: uses
  - target: {type: task, id: tsk-6cf9}
    relation: requires_contract
  - target: {type: task, id: tsk-1962}
    relation: implements
  - target: {type: task, id: tsk-a59d}
    relation: specified_by
  - target: {type: task, id: tsk-3e62}
    relation: specified_by
  - target: {type: task, id: tsk-dd92}
    relation: requires_contract
  - target: {type: task, id: tsk-add4}
    relation: requires_contract
  - target: {type: task, id: tsk-d7c3}
    relation: implements
  - target: {type: task, id: tsk-e35c}
    relation: implements
---

# Native Task ownership and read views

The Records NativeTaskStore creates durable sessions and tasks in the fixed project runtime Task corpus. It uses the original Auth project admission, Core native captures and Lance SDK branch adapter. Reads never create directories or repair an incomplete corpus.

An absent optional `Task.commit` is omitted by Serde from both the wire value
and the canonical stored payload. Adding this delivery association does not
require rewriting records that have no commit; nonempty associations remain
validated and retained in lifecycle history.

The service exposes create_session, create_task, set_task_commit, read_task, read_session, catalogue, query, query_schema, session_heads, read_view and observe_heads. Task and Session claims, heartbeat, progress/checkpoint and release use the same corpus and codec. Specification revision, check verification/supersession, links, flags, Task completion/cancellation and Session completion use that same writer and history. Task comments in `tsk-1962` are Linux-qualified with affected functional tests, Records doctests and scoped strict passed; lifecycle completion is consulted in the Task. Comment MCP/App/UI methods, hard removal and Session cancellation remain separate product work. Task/Session authored references in `tsk-6cf9` are native Linux-qualified with 70 distinct functional tests and six Records doctests separately, scoped strict Clippy and formatting passed. This does not qualify an App references route. A new session or task is Open with an unclaimed Ownership observation; owner or status alone never proves runner activity.

## Authority and versions

Creation requires ProjectWriteOperation::Task, which is currently admitted for Coordinator with WriteTask permission. A read requires ProjectReadModule::Task. The caller passes the SAME original Arc<SnapshotContext> used for the admission. The service checks cancellation, deadline, admission and captured directory at SDK boundaries. Auth mints the creation actor; request data never chooses it.

Creation/event/comment actors and ownership owners use the canonical uppercase
26-character form of `Actor::identifier()`. The identifier is the first 128 bits
of the Actor digest, not a reversible encoding of all 256 bits or a creation
timestamp. The same ULID identity controls ownership and idempotency, including
reclaiming a live claim by its owner. The actual Auth admission, Original,
permissions, private 32-byte fence/token, lease and revision checks remain
required. No extra private actor binding, registry or old-format reader is added.

Reading the admitted request already revalidates the admission. Task reads and
writes use that check once at each existing boundary, including EOF, delivery
and publication, without immediately repeating the same physical checks.

Local Task access captures the complete selected runtime directory chain once
through Core. It does not probe and recapture every prefix of a present corpus.
If capture reports `NotFound`, the existing Core runtime probe revalidates the
retained parents before classifying the selection as absent. Other errors remain
errors; a directory appearing before the probe does not turn the failed capture
into a successful read. Admission, Original, retained-directory and publication
checks remain at their operation boundaries. Missing state is not created by a
read, and an existing partial Task directory remains an incomplete corpus.

Catalogue and full-text scans revalidate the admission and captured directory
at SDK await boundaries, EOF and final delivery. Their decoding, graph
selection and lexical candidate processing over the captured
pin check the Original's cancellation and deadline per row without reopening
authority or directory state per item.
Directory probes already validate their captured chain before and after opening
the child; Records keeps its admission checks around those probes without
duplicating the same chain validation.

A Task read can also use the existing admission produced by [Broker PROJECT authorization](auth_broker_project_records.md): a signed, locally verified caller and a fresh PROJECT Task-read decision remain bound to the original request and real project captures. This does not admit writes. The Records `broker_task_project_acl_preserves_original_and_denies_before_storage_io` case targets actual Broker JWT/JWKS and PROJECT ACL admission, Original binding, wrong-module rejection and denial before malformed local storage can be read. Its locally written corpus is only a sentinel for those negative paths; this fixture has no record-storage resolver and does not qualify successful corpus reads or storage selection. Positive selected reads belong to App `record_storage_tests` consumers composed with the production resolver: an advertised disabled Broker storage capability permits local storage, while active S3 remains authoritative and cannot fall back to a local table. The previous hybrid case failed with `Auth(Denied)` during RecordStorage service selection before corpus IO; its failure remains evidence and is not a passing storage test. The reconciled ACL case and corresponding App coverage require their own qualification before completion. JWT authentication remains valid until signed expiry; each new admission separately checks the current PROJECT ACL.

Local PROJECT operations select their corpus once, after authoritative S3
selection, through Core `local_corpus::resolve`. The single dataset is
`task/index.lance` relative to the admitted PROJECT runtime for every project
version. VersionContext selects a native Lance branch inside that dataset;
branches retain their own ancestry, revisions and embeddings. Git snapshots use
native immutable tags in the same dataset. No version binding map or alternate
dataset path participates in selection.

The same selected URI supplies physical captures and recovery-key validation for
the entire operation. First publication creates the module and dataset under the
Core PROJECT writer. An existing module directory without its dataset is an
incomplete corpus and is rejected; unexpected entries and symlinks do not grant
permission to replace it. A recovery key from another dataset or branch is
rejected before publication. S3 is selected first and remains authoritative.

Inputs cannot supply another URI or branch. The creation, lifecycle, specification and full-text producers are composed on Linux, macOS and Windows through the existing Core native writer APIs. The writer retains the original request and PROJECT lock through capture and publication. Read captures use the native Core implementation for the current platform. Source availability is separate from qualification: historical Linux receipts below do not qualify execution on Windows or macOS. These captures detect replacement at checks and do not promise continuous confinement against arbitrary hostile pathname changes.

Creating the first session at the default location creates the Task parent if absent and an empty default dataset under the original Core writer lock. A named target is materialized from the captured default. First creation in that branch leaves the default empty. An existing Task directory without index.lance is rejected as incomplete. The productive writer initializes the derived Task FTS index once under that same writer/pin before its first record merge. Ready indices are not rebuilt by later writes; queries never initialize them. Derived index and record publications are separate, with their actual effects preserved independently. SDK errors retain their underlying cause; strict commits retain recovery keys. If a commit succeeded but subsequent cancellation or revalidation failed, Confirmed includes that SDK version.

## Finite public Task queries

`NativeTaskStore::query` composes the existing authenticated `native::read_pin`
with Lance Scanner filter, projection, ordering and limit. It does not call
`read_view` or retain all Task heads/history to answer a scalar query. The SDK
may inspect more source rows to sort; Records retains at most the requested
page and one lookahead row. This query does not calculate readiness, blocked
state, Session totals, or expose payload, ownership/fence or audit envelopes.

`TaskQuery.selection` selects one public view: `tasks` or `sessions`.
Both accept optional typed canonical nonzero ULID `ids` and typed `status`;
Tasks also accepts `session_id`. Conditions combine before the SDK limit.
Omitted/null IDs add no ID restriction; an empty ID array matches no records.
This is a structured query, with no caller SQL/predicate parser. The required
`columns` list selects public scalar columns in output order; an empty,
repeated, unknown or inapplicable column is rejected. Both views expose
`id,status,title,revision,created_at,updated_at`. Tasks additionally exposes
`session_id,parent_id,task_type,priority,flagged`; only `parent_id` is nullable.
`NativeTaskStore::query_schema` describes these exact views and typed columns
under the same Original and Task read admission, without creating a corpus.

Required positive `page_size` bounds this page, while required positive
`top_k` caps the total rows delivered across its continuation chain. No
implicit global quota is introduced. Rows are ordered by ascending native ID.
The optional opaque canonical base64url `cursor` is returned by the preceding
page and binds project identity/root/brand, selected version, complete physical
SDK generation, selection, projection, page size and total cap. It cannot
select authority, a physical URI or a branch. Changing any bound input or
publication rejects it. A caller changing the query starts without a cursor.
Reaching the total cap returns no cursor even if more records exist.

`TaskQueryRead` serializes `state: no_corpus` for validated absence, or
`state: published` with `generation,columns,rows,next_cursor`. Rows contain
only the selected scalar values, parallel to `columns`. A published view
with zero matches has empty rows and no cursor; it remains distinct from
absence. The finite SDK stream is drained to EOF, with Original/admission
and captured-directory checks around awaits and before delivery; errors and
cancellation never return a partial page. This does not assert traversal of
the entire corpus or continuity with a different store/history.

The focused producer tests are `cargo test --locked --offline -p graphit-records --test task_store query:: -- --test-threads=1`. Host MCP/CLI
catalogue and authenticated consumer qualification belong to the App delivery.

## Data and reads

IDs are canonical nonzero ULIDs. TaskSpecification is the authoritative check and calendar model. A Task contains a session link, optional parent, actual dependency IDs, status, flag, owner, idempotency key and UTC epoch-millisecond creation/update timestamps. These timestamps order a catalogue; they are never schedule dates. Calendar fields are strict ISO civil days with no timezone conversion.

CreateSession includes an idempotency key, title, description and strategy. CreateTask includes a key, session, parent/dependencies, title/description/type/priority, checks and optional schedule. All fresh checks are Pending and both Acceptance and Test kinds are required. IDs and the creation actor are generated server-side. Reusing a key for the same input returns the original record; different input is an IdempotencyConflict. Parents share the session; dependencies may refer to tasks in another session of the same project/version, and all related tasks exist; terminal parents are rejected. The combined graph must remain acyclic.

Current data and immutable creation/lifecycle history publish together in one
strict SDK delta. Public exact reads use the selected `read_task` and
`read_session` APIs. The former public full `get_task`/`get_session` wrappers and
Session-only full detail DTO have been removed after their only production
consumer moved to current-state context. Complete audit/export uses explicit
same-pin history pages and Task comment pages through EOF. The private
`read::task_detail_at` and its `TaskDetail` model remain necessary for actual
comment-write idempotency, event attribution and historical claim fencing; they
are not a public read compatibility path.

Reference focus resolution in `foreground_record_references` uses
`read_task(TaskGetInput::current(id))` and
`read_session(SessionGetInput::current(id))` under its actual Task admission.
It consumes only the current Task/Session title, preserving absent-result and
typed-error propagation without requesting lifecycle pages. This title lookup
is not a full audit. Project-hook context also requests selected current Session/Task records and
exposes their start cursors; ID selection is not an audit request. These
consumer changes require the current joint build and integration qualification;
source review alone does not establish a runtime or performance result.

The public Task Get instead uses `NativeTaskStore::read_task` with `TaskGetInput`
and returns `Result<Option<TaskGetRead>, TaskStoreError>`. It reads current state
and only explicitly requested history/comment pages. The protocol boxes present
results without changing JSON null semantics. Historical response-boxing and
Comment qualification apply to their original boundary, not this selected-read
change. Public catalogue and read_view project only head/calendar/dependency/check-state
columns; descriptions and full record payloads are absent. Readiness uses the real
functional GateGraph over all captured task heads, not a persisted marker.

read_view returns NoCorpus for validated absence, or Published with TaskGeneration version/physical_branch/inherited_target, TaskSummary heads and SessionHead counts. It returns only after EOF and final authorization/capture checks. An error or cancellation returns no partial view. All denominators count tasks of that same session and SDK pin. Session heads come from the same Task dataset and codec, for consumers such as Now/Data; no second session store exists. TaskSummary and SessionHead include actual Ownership and typed ProgressObservation metadata.

Catalogue supports a title query, status/session filters and optional boolean `flagged`/`blocked` filters, caller page size and a typed cursor bound to the selected project/version/filter/generation. These conditions combine globally before page slicing. `flagged` selects the persisted flag; `blocked` is true only for an Open Task with `ready == false`. False selects the complement and null or omission adds no condition. The cursor includes both boolean fields and rejects a changed filter. A changed generation rejects the cursor rather than mixing pages. Exact reads and an available empty catalogue remain distinct from a missing corpus in read_view.

Catalogue uses its own same-pin streaming observation instead of materializing
`read_view`. Every task/session head is transiently validated, including heads
outside the filter/page. Readiness still requires the complete GateGraph, so
memory remains proportional to graph IDs, edges and check states. Only evidence
presence after trimming is retained, with no evidence text, title, ownership,
progress or calendar in those retained facts. Session membership is checked
before selection. Filters and readiness precede bounded page-plus-lookahead
selection. The full selected summaries are then read from the same pin, checked
for completeness, and returned in creation-time/ID descending order after EOF
and final authorization/capture checks. There is no full-corpus/prefix summary
retention or second snapshot capture.

Catalogue continuation uses the last creation-time/ID key, bound to project
identity/root/brand, selected version, full SDK generation and filter. Page size
may change between pages, as before. A changed query or generation is invalid;
continuation cannot choose a branch or authority. Complete `read_view` and
Session-head/count consumers retain their separate contracts.

### Current Task and explicit detail pages

`TaskGetInput` contains the Task `id` and optional `history` and `comments`
sections. Each requested section requires a positive `page_size` and accepts an
opaque `cursor`; omission or null leaves that section unloaded. An absent Task
returns null. A present result contains `task`, `creation`, `last_event`,
`generation`, `history_start_cursor`, `comments_start_cursor`, `history` and
`comments`. `last_event` is null at revision one. An unloaded section is null;
a requested empty page is a completed empty read.

History pages return `{before, events, next_cursor}` with events in ascending
revision order. `before` is the validated predecessor Task for that page, so a
consumer can replace the page and compute its first diff without retaining all
earlier events. Comment pages return `{comments, next_cursor}` in ascending
sequence order. Current state includes initial cursors that bind the first
explicit section request to the same captured generation. A Get without any
cursor observes the latest authorized snapshot.

Continuation binds Task, section, project/root/brand/version, corpus identity,
physical generation and optional SDK `BranchProvenance`. Records now uses
`at_version_with_provenance(version) -> (pin, Option<BranchProvenance>)` for
current, named and still-unmaterialized inherited reads. A named-branch cursor
requires provenance and rejects a different namespace after retire/recreate.
For an inherited cursor whose child has since materialized,
`at_inherited_version(version) -> (pin, BranchProvenance)` requires the direct
default parent, a requested version no later than the fork, and SDK double-ref
identity validation. An inherited binding without proof can gain that proof
at materialization. Both requested section cursors are validated against the
received binding before promotion; returned start and next cursors carry the
new proof. Current Auth, PROJECT/version, corpus and physical-generation
identities remain checked. There is no retained snapshot cache, new secret,
cursor lease or caller-selected URI/branch authority. The selected Records API
and proof paths passed the qualification below; App consumers remain separate.

Named historical capture reuses the native Lance Session owned by the current
`BranchSnapshot`, between the same two actual branch-reference reads. Session
metadata/index caches have that owner's lifetime; they do not select a project,
branch, version or transport, and no process-wide snapshot registry is introduced.
Task's observed-schema opener still resolves each new current read independently.

Current reads validate head, creation, last event and its predecessor transition,
including reference updates and comment/event association when the last event
adds a comment. They do not audit every historical record. Pages validate their
selected records, continuity at the predecessor seam, reference transitions and
comment/event ownership bindings. Old corruption outside the selected scope
can leave a current read usable; reading the affected page or performing full
audit must reject it. Explicit full-history export traverses every history and
comment page at that generation.
Neither an unloaded section nor a single page proves a complete export.

### Current Session and explicit audit pages

`NativeTaskStore::read_session(original, admission, SessionGetInput, limits)`
returns `Result<Option<SelectedSessionRead>, TaskStoreError>`. The closed input
contains the Session `id` and optional `history: {page_size, cursor?}` with a
positive page size. Omission or null leaves history unloaded. A present result
contains `session`, immutable `creation`, `last_event`, `generation`,
`history_start_cursor` and `history`. A missing Session returns null. At revision
one the head must equal creation and `last_event` is null. There is no top-level
`events` collection in this selected response.

The current read selects only the head, creation, last lifecycle event and its
predecessor. It validates their identity, reference transition, revision and
observation timestamp, including the existing Session revision rules for actor,
live claim, renewed lease and preserved checkpoint. It does not collect or sort
all `session_event` payloads. Each explicit Session history page shares the
Session specification transition validator with the current read.

An explicit history page contains `{before, events, next_cursor}` in ascending
revision order. The record/kind/revision predicate and page-plus-one lookahead
are applied by the actual Lance scan. `before` is the predecessor Session; every
selected transition and the lookahead are validated. The final page must reach
the captured head revision before returning a null continuation. An unloaded
history differs from a requested empty revision-one page. A page is not a full
audit or export; an explicit complete export must traverse through EOF at that
pin.

Session cursors have their own closed, canonical opaque identity and cannot be
used as Task cursors. They bind Session, history section, project/root/brand,
logical version, corpus, actual SDK generation and branch provenance. They use
the same admitted receiver and SDK inherited-version validation as Task Get:
parent writes do not change the selected pin, child materialization can promote
its provenance, and a retired/recreated branch cannot reuse an old namespace.
A cursor grants no authority. Current admission, cancellation and captured
physical-directory checks remain active around SDK reads. There is no aggregate
history quota, snapshot cache, caller-selected URI or branch, or post-read
filtering of a complete audit.

Selected Session source and protocol are implemented. Their new tests and
consumer integration require the current joint qualification; the older Task
Get results below do not qualify this Session change.

#### Selected Get Linux qualification

On 2026-10-24,
`cargo test --release --locked --offline -p graphit-records --test task_store get_pages::`
passed three tests with zero failures and 59 filtered tests. Harness runtime was
2.42 seconds, build time 13 minutes 30 seconds and total command time 13:32.73.
The three [page integration cases](../../crates/graphit-records/tests/task_store/get_pages.rs)
cover current reads and replaceable pages pinned across writes; inherited
parent writes, child materialization, post-fork forgery, namespace replacement
after retire/recreate with the same public generation, and closed nested proof;
and old corruption that leaves current usable while the affected page and full
audit reject it.

`cargo test --release --locked --offline -p graphit-records --test task_protocol`
then passed all 10 tests with zero failures or filtered tests: 1.98 seconds in
the harness, 4 minutes 20 seconds building and 4:22.73 total. Coverage includes
positive page sizes and closed wire inputs, real comments, actual Auth and
cancellation. These are 13 distinct affected tests; historical SDK and other
suite counts are not added to them. Neither command failed.

On 2026-10-25, eight exact App backend selections passed with zero failures,
covering bounded encoding, confirmed writes, actual MCP/native HTTP, local
managed-role control and cancellation/delivery. The [App qualification](rust_task_mcp.md#task-get-app-backend-qualification)
keeps those eight results separate from these 13 Records tests, which were not
rerun. The principal executable CLI case subsequently passed completely in
[PASS02 with a private corrected caller](rust_task_mcp.md#task-get-executable-cli-qualification),
including `stale_main` without relaxing its assertion. FAIL01 remains historical
evidence; the prior parallel two-Root CLI pass was not repeated. Browser cookie/socket
and React qualification remain pending. This functional proof does not establish installed App
delivery. Historical tests below remain historical evidence.
No new page default, aggregate quota, endpoint, parallel protocol or cache
framework is introduced. Cargo/build totals measure the commands, not product
RAM, RSS, latency or speedup; no O(1) memory claim is made.

## Task/Session references: native Linux qualification

`tsk-6cf9` implements [native references](../../crates/graphit-records/src/task_store/references.rs) with Linux qualification: Task SDK47 + TaskStore library15 + TaskProtocol8 = **70 distinct functional tests**, plus **six Records doctests separately**, locked no-run compilation and scoped strict Clippy passed. Focal/retry runs and historical Comment/FTS receipts are not added. The [shared contract](record_relations.md#rust-tasksession-persistence-and-query-native-linux-qualification) records current marker, namespace, EOF, cursor and causal recovery proof, together with the remaining App/FullNoLoss and Broker runtime gates. The R7 App mapper unit passed one case separately under the Root/core_writer `tsk-cd41` receipt; this and the Broker no-run compilation do not add native functional tests or qualify an HTTP route.

Task and Session require references Vec and reference_generation in every snapshot, without old-snapshot defaults/fallback. Generation equals the published domain revision. The sixteen patch-capable paths (two creation plus fourteen actual mutators) take the borrowed AuthoredReferencesField before limits; omission preserves and [] clears authored edges, while null/nonsequence/invalid tail rejects the whole input. TaskStoreWriteLimits.references contains explicit max_entries/max_text_bytes NonZero bounds mapped from existing caller ContentLimits, checked before kernel/clones/SDK work. Claim/heartbeat preserve refs and publish a matching marker only when they actually publish; no-op does not invent one.

Normalized Preserve|Replace request identity lives in creation/comment/lifecycle storage envelopes, not the pure TaskComment model. Exact create/comment retry compares that patch before Existing; later replay never reapplies an old patch. Reference-only revise follows the real owner/fence/CAS/lease/reason guards and preserves specification/check state when those fields are unchanged. One strict commit publishes changed head, creation/event/comment and a complete typed ReferenceProjection together. Empty authored state is a real marker; structural session/parent/dependency precede authored order, duplicates remain. Current projection mismatch/tamper is causal, missing current marker is incomplete and old/orphan markers never restore cleared links.

NativeTaskStore::references uses the current actual ReadTask/Original/guard and one SDK pin for both pages. It scans filtered heads and markers through EOF with per-read bounds, retains only a minimal validated current-head map and selected pages/counters, and performs same-pin point reads rather than get recapture or global history/corpus retention. Closed cursors bind NativePath/ProjectId/version/pin/focus/direction and all three bounds. NoCorpus and missing focus differ from complete empty edges or incomplete projection. Task/Session PROJECT source coverage is explicit; rich same-pin Task/Session resolution is separate from unqualified Memory/Knowledge/USER/import/global incoming coverage. No birth/materialization/repair, new grant or invented title/href follows.

The implemented `tsk-d7c3` delta removes the reference-query output counter,
serialization previews and manual output-text debits in `page_edge`; that helper
now constructs the edge value without an output-budget result. Both references
and backlinks retain raw rows/edges/text scan limits, one pin/full EOF,
Auth/Original/delivery checks, cursor bindings and functional Serde. They do not
promise a serialized query/response byte-accounting pass. Source02 compile01
and Task reference SDK9, backlinks SDK5 and private reference-library9 passed
(23 distinct affected functions). Own strict01 then failed on an unused
direction in an empty continuation-accounting loop; Source03 removes only
that loop and its stale comment, without an allow or stub. Current Task SDK/lib
compile02 and current Source03 reruns passed: Task refs9, backlinks5 and private
Task refs library9. Scoped strict02 and fmt02 passed. Together with unchanged
PROJECT8/USER9, these are 40 distinct affected functions qualifying this removal
in Linux; repeats add zero. API/input guards, EOF/cursors and Auth are unchanged,
with no broader domain/platform qualification.
Historical `tsk-6cf9` and `tsk-9a83` receipts/counts remain unchanged, and reruns
are not additive.

### Literal-target backlinks: Linux-qualified

`tsk-9a83` now exports NativeTaskStore::reference_backlinks and its closed
TaskReferenceTarget/TaskReferenceBacklinksQuery/Cursor/Page/Read DTOs from the
[references Source](../../crates/graphit-records/src/task_store/references.rs).
The [shared API contract](record_relations.md#literal-target-task-backlinks-linux-qualified)
describes actual ReadTask source-only authority, literal targets of all four
kinds, boxed incoming results, EOF/current-marker completeness and bound cursors.
Actual scoped Linux functional coverage is 23 distinct cases (SDK14 +library9),
with six Records doctests separately passed. SDK 13 passed in the full run and
the sole Auth-fixture failure passed after a test-only correction under actual
ReadMemory denial and same-parent ReadTask-only bearer revocation; product
Source stayed unchanged. Current strict02 passed exit0, with owned edition2024
skip_children formatting/diff0 and final Source-manifest verification after
those fixture edits. Native backlinks are Linux-qualified in this 23+6 scope;
prior strict01 PASS is retained as history. Probe overlap
adds zero; historical `tsk-6cf9` 70+6 receipts are not added to this scope.
The full 52-case TaskStore suite, other Task/Auth domains and cross-source/App/
non-Linux delivery remain outside this proof.

The existing references API, writes and schema remain unchanged. A historical
marker publication-version label may differ from the authorized query's logical
version on a legitimate inherited/forked pin. Exact ProjectId/current-head
state and actual pin/branch/cursor selection remain authoritative; this narrow
reader correction belongs to `tsk-9a83` qualification. No target grant/body lookup,
App route, global FullNoLoss, USER/Knowledge or non-Linux proof follows.

## Full-text projection and discovery

[Native Task FTS](native_task_search.md) follows the Source-free `tsk-3e62` contract. Projection/schema/encoder and index publication/recovery `tsk-dd92` are implemented and Linux-qualified with 35 distinct functional tests, six Records doctests separately and scoped strict/fmt0; reader/cursor `tsk-add4` is implemented and Linux-qualified with 45 distinct functional tests, six Records doctests separately and scoped strict/fmt0. Both delivery leaves are siblings under `tsk-8c6a`. The public projection contains only current spec/check text/evidence (including retained Superseded checks) and append-only comment bodies through a new validated public_search JSON column. Session/history/coordination/fence data is excluded; old-schema corpora will be rejected without conversion. The current catalogue remains title-substring discovery, and the selected `read_task` remains authoritative. Task search now also accepts explicit semantic/hybrid modes through the configured embedding service. Incremental public specifications and comments keep their derived vectors in this same Task table; unchanged public text reuses its model/recipe/fingerprint association. Semantic results include compatible completed vectors immediately and do not wait for corpus-wide coverage. Metadata-only revisions retain their vector, while stale inference is rejected against the current public fingerprint. The catalogue and selected reads remain model-independent. See [Task search](native_task_search.md) for ranking and consumers. Index publication, same-pin EOF/ranking/cursor, recovery and required SDK mutation proof are specified once in the linked contract. Publisher and reader have their historical native Linux qualification. Current `tsk-9748` removes aggregate FTS corpus quotas and uses a direct `NonZeroUsize` bound for one query and its SDK expansion; schema, public projection, EOF, ranking and cursor contracts stay active. The RoleTool consumer uses this direct bound; Linux Source/CLI/HTTP now expose the same Search reader with the host policy bound through task_protocol::execute_read. Their real first-corpus/pagination/version qualification belongs to tsk-59c7, separately from the historical reader receipt.

## Claims, renewal and handoff

`tsk-ea469` removes the aggregate ActorBusy scan and its public error variant. The current correction uses the same public APIs, authenticated actors, native schema and per-target fences.

Task ownership operations accept OwnTask or Task admissions; Session coordination requires CoordinateTask. The rechecked `ProjectWriteAdmission::task_actor()` owns the claim. Runtime participants are namespaced by the server-owned InstallationId and source origin; Local participants additionally use the canonical provider/profile authenticated by the current credential. These Task identities exclude issuer epoch, credential generation and secrets. Newly authenticated Runtime/Local participants retain their Task identity across restart or credential rotation; old credentials and revoked admissions remain invalid. Managed Role participants retain their execution-specific Actor, and Broker Task writes remain denied. Existing persisted owners are not rewritten or adopted. The caller cannot choose an owner or physical branch. Claims require an open/ready task and a nonterminal session. The same authenticated actor may hold live claims on distinct Tasks and Sessions. There is no aggregate claim quota per actor, user or installation; each target retains its own private token, revision, lease and epoch. The executor workflow still assigns one Task at a time to an agent. A competing live owner fails; normal claiming after expiry rotates the private token and increments the claim epoch. Reclaiming a live claim by its same actor returns that claim without silently renewing it.

Every renewal, progress/checkpoint and release requires the current actor, private token, expected revision and unexpired lease, rechecked at the SDK publication boundaries. The caller supplies a positive Duration with millisecond precision; checked arithmetic rejects overflow. Heartbeat and progress renew without shortening the current lease. Release clears ownership and lease timestamps, preserves the epoch, saves meaningful handoff progress and returns the record to Open. Current record plus immutable event commit atomically; a stale decision is never retried automatically.

ClaimToken is an opaque participant credential without Debug or ordinary Serde. Claim responses provide an explicit byte transport accessor; stored current rows keep the cryptographic nonce separately from public Task/Session values. Public exact reads, catalogue, head observations and history never serialize that token. Ownership contains owner, claim_epoch, claimed_at, heartbeat_at and lease_expires_at. Progress contains its observation kind, sequence, timestamp, summary and next step for full authorized reads. Head projections include only the typed kind/sequence/timestamp, with no summary, next step or raw content.

Reads remain observational: they do not expire claims, repair rows or change generations. A persisted expired lease remains observable as expired. Consumers compare the actual expiration with their observation time; an absent lease is inactive. These UTC epoch-millisecond timestamps remain separate from civil schedule dates.

## Specification and terminal operations

`revise_session` accepts `SessionRevisionUpdate` with the live coordinator claim/revision, positive lease, canonical reason and `FieldUpdate<&str>` title/description/strategy. Omission keeps the field; supplied text replaces it without normalization, and null, empty or padded text is invalid. A revision must change specification or normalized authored references. Omitted references preserve, `[]` clears, and a reference-only change is valid. Invalid/no-op input is checked before fork materialization and neither renews nor publishes. The same writer and Original perform materialization/publication; common fencing and causal SDK outcomes remain unchanged.

Session revision preserves ID, initial idempotency key/Creation, progress, related Tasks and ownership identity. It renews without shortening the lease and publishes one Revised event with its reason and complete snapshot. Retrying the original create input returns the revised head; using a revised payload under the original creation key conflicts. Selected `read_session` validates immutable identity, specification changes only in Revised, unchanged checkpoint/status/ownership identity for that event and valid renewal/actor for its last transition and explicitly requested contiguous history transitions. Complete audit traverses those pages through EOF. Terminal sessions cannot be revised or reopened. This operation does not add Session search/FTS.

revise_task uses the TaskSpecification planner with Keep/Set fields. An omitted calendar update keeps it; Set(None) clears it. Calendar-only and priority-only changes preserve verification evidence. Effective title, description or type changes reset active checks. verify_task_check records pass/fail with required evidence. supersede_task_check preserves existing evidence and appends an optional replacement with a server-generated ID; active Acceptance and Test kinds remain required. The planner's revision is the one persisted Task revision, including ownership history.

These operations require the actual live actor, token and revision. They renew the lease without shortening it, reject a no-op, and publish one current row plus its immutable event. Events retain revision/supersession reasons and affected check/replacement IDs; full snapshots retain both specification and evidence. Caller inputs never choose actor, timestamp, check ID or branch.

update_task_links replaces the supplied parent/dependency fields under the writer and validates current same-project/version targets and same-session parents and the combined acyclic graph. A missing, duplicate, self or cyclic target is rejected. set_task_flag requires a reason and an effective flag change. Neither operation silently alters checks or calendar dates.

complete_task requires an InProgress live claim, no flag and real GateGraph completion conditions over the current SDK pin, including active checks, dependencies and child tasks. complete_session requires its existing InProgress coordinator claim and all of its tasks cancelled or validly completed. An empty session can close, but an empty graph never substitutes for the existence of the Session record.

cancel_task requires a reason. An Open task may be cancelled by a broad Task admission; OwnTask cannot cancel an unowned Open record. An InProgress task requires its actual live owner/token/revision even under broad Task authority. Terminal records reject further mutation. Completion and cancellation clear the private token and lease observations, preserving the claim epoch and progress. They do not recursively cancel children or reschedule dependencies.

The `tsk-e35c` [TaskServer integration fixture](../../crates/graphit-app/src/task_server/tests.rs) passed **one current Linux HTTP/MCP/Auth/SDK case** after successful compile03, with 21.45s test runtime (21.52s runner). Its five-Task tree exists before owned-child cancellation: the child remains Cancelled/owner-null/epoch1 with Pending checks, and both required grandchildren verify actual checks and complete before the parent can complete. Two blocked parent probes preserve get/audit and return MCP `invalid_native_input`; fresh Runtime Auth/Original/project capture through public `execute_write` confirms native `Invalid("completion gates are pending")`, distinct from structural `GateError`/`gate_blocked`. Parent release precedes each grandchild claim under ActorBusy rules. The cancelled child's explicit dependent stays Open/ready=false in the actual HTTP catalogue; claim rejects with native `Invalid("task is not ready")` and unchanged get/audit. Creation/events, actor, contiguous revision/reference generation, final head snapshots, private-token exclusion and owned joins passed. Compile01 Data Box errors, compile02 locked-file guard and Root Broker missing-module failure remain preserved; resolved producer Source enabled the successful current compile and exact case. App cfg(test) Clippy (`--lib --tests --no-deps -- -D warnings`) passed exit0; formatting, scoped diff and the two documented contracts are included in the qualification recorded in Task `tsk-e35c` (historical local receipt `.build/rust-records-task-cancelled-child-mcp/validation.json`, unavailable in this checkout). This test-only qualification changes no Records API or production gate and leaves the external active-MCP scheduler `tsk-bc28` outside its scope; [the MCP component contract](rust_task_mcp.md) describes the transport boundary. Prior native producer and D7 receipts/counts remain unchanged.

## Activity observations

observe_heads scans all current Task and Session headers through EOF at one SDK generation without loading record payloads, checks, dependencies, calendar or history. Published contains complete TaskActivityHead and SessionHead sections, each as a Result with a boxed causal error. TaskActivityHead includes its canonical session_id. There is no top20 limit or active-only filter at this boundary: consumers compute requested totals first and choose their presentation order by updated_at and stable ID. Cancelled tasks are excluded from the Session progress denominator; only Completed contributes to its numerator.

Authority, native capture, schema/stream errors, duplicate global identity and invalid Task-to-Session references fail the whole observation. A demonstrably row-local Session presentation error yields a Session section error after EOF and final original-request checks, preserving valid Task heads. A Task section error makes dependent Session counts explicitly unavailable; it never fabricates zero counts or NoCorpus. A valid Session identity is retained for linkage validation even when its local presentation fields fail. read_view, catalogue and session_heads retain their complete-view error contract.

## Task comments: Linux-qualified

`NativeTaskStore::add_task_comment(original, admission, id, input: TaskCommentUpdate<'_>, limits)` is async and returns `Result<TaskCommentOutcome, TaskStoreError>`. [The transactional contract](rust_record_transactions.md#taskcomment-contract-tsk-1962) maintains R1–R5; [TaskComment](../../crates/graphit-records/src/task_comment.rs) and the [lifecycle implementation](../../crates/graphit-records/src/task_store/lifecycle.rs) are the current Source evidence. This extension is **Linux-qualified** in its own affected scope, distinct from the prior native receipts above.

Input holds the existing `ClaimUpdate`, positive lease, canonical nonempty per-Task idempotency key, kind and literal body. Kinds are note/decision/problem/lesson/knowledge. Blank body is rejected with `body.trim().is_empty()`; nonblank Markdown bytes and indentation are preserved. The server supplies canonical native ULID, actor, epoch-millisecond at and contiguous sequence. A new append requires the ordinary OwnTask or Task admission, InProgress status, actual live actor/private fence and expected current revision. It renews without shortening the lease, increments Task revision/sequence once and commits head + CommentAdded event + standalone comment row in one strict SDK delta, without duplicating body into the event.

Acquisition retains the actual Writer and same captured pin **unmaterialized** for full history/idempotency validation. Exact retry requires the same live actor/fence and historical CommentAdded claim epoch, plus equal kind/body/actor. It accepts the currently observed revision or original `comment.revision - 1`; other revisions conflict. A valid new token after release/reclaim by the same actor fails the historical epoch check. Existing returns the observed Task and original comment without branch materialization, publication/version advance or lease extension. Only a validated new insertion materializes an inherited target; later failure can leave branch metadata, without a NoEffect metadata promise.

`TaskCommentOutcome` distinguishes Created and Existing, each with Task and comment.
The private comment-write audit's `TaskDetail.comments` contains stored comments ordered by
sequence. That audit validates full same-pin EOF and a bijection with CommentAdded
events, unique IDs/keys/sequences, continuous sequence, increasing revisions,
nondecreasing at (equal milliseconds allowed), exact event time and historical
owner/status/epoch/live-lease bindings. Incomplete, orphan or late malformed data
fails the whole audited detail. Public selected Get uses the explicit comment
pages described above; an unloaded section is not a fabricated empty list and
does not establish a universal audit. Release/reclaim and terminal records retain
readable comments. Catalogue/heads do not count comment rows; the codec uses the
existing schema with closed neutral comment headers.

Original Auth/context/guard and live lease are rechecked at exposed boundaries. Known commit followed by failed recheck conserves `Confirmed { version, cause }`; uncertain SDK errors retain their cause and never imply rollback. There is no continuous Auth/SDK atomicity or pathname fence, automatic replay, new authority or public comment nonce. Affected Linux functional/docs verification and scoped strict passed, with existing store receipts kept separate. No MCP/App Comment consumer is implemented here. Recovery retry authenticates a new caller in a fresh RequestContext/admission, retaining the actual current claim epoch/fence. Runtime/Local Task participants can continue across an issuer restart with a new valid credential for the same installation and canonical participant. Managed Role participants remain execution-specific. The lease must still be live and the token and revision current. The cancelled request's caller or admission is not reused.

Affected Linux executions passed TaskStore21 + TaskProtocol7 + library `task_`32 = **60 distinct tests**; focal7/1 are included, not additional. Records doctests passed separately: **four ordinary + two compile_fail**, not six additional native store cases. Actual inherited retry without branch birth, historical Session attribution corruption, Role credential revocation, concurrent keys and completed/cancelled history passed. The real SDK publication→actual cancellation→production Confirmed boundary→fresh same-issuer caller Existing composition preserves version/cause; it is not an intercepted SDK lost-ack or Auth/SDK atomicity test. The [transactional contract](rust_record_transactions.md#comment-fixtures-documentation-and-qualification-gate) retains failed fixture/strict history and qualification limits. Scoped strict `--lib --test task_store --test task_protocol --no-deps -- -D warnings` exited zero; protocol7 and Records docs6 also passed after response boxing, without increasing distinct counts. Task completion is not inferred.

## Verification

Native temporary-project tests use real Core/Auth admission and the Lance SDK, persist and reopen actual records, and exercise branch isolation and errors. Their existing receipts qualify the prior native service operations, including specification and terminal transitions; Comment Source is Linux-qualified with affected functional/docs/scoped strict verification, and live hosts, the remaining lifecycle operations and the React Gantt require their own productive consumers.

A host can consume the complete lightweight view using its original admission:

```no_run
use std::sync::Arc;
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_records::task_store::{
    NativeTaskStore, TaskReadView, TaskStoreError, TaskStoreReadLimits,
};
async fn observe(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    limits: TaskStoreReadLimits,
) -> Result<TaskReadView, TaskStoreError> {
    NativeTaskStore::read_view(original, admission, limits).await
}
```

## Default-stack claim regression and enclosing-caller diagnosis

`tsk-f382` is a bounded diagnostic/regression unit, not a production Records
correction. The lifecycle/write/read implementation and original SDK fixture
match the unchanged Source02 snapshot; the public five-argument claim operation,
Original/admission, CAS, fence/lease/ActorBusy and strict head/audit/reference
publication contracts were unchanged in that historical qualification. `tsk-ea469`
subsequently removes only the aggregate ActorBusy gate; the current default-stack
fixture accepts independent target claims and retains foreign-token/actor and stale-CAS denials. `tsk-6cf9`, `tsk-b777` and
`tsk-dd92` keep their separate qualified contracts/receipts. Runtime consumer
`tsk-8923` owns its enclosing fixture and Now integration.

The new [task_store_claim_stack tests](../../crates/graphit-records/tests/task_store_claim_stack.rs)
use actual NativeCaller/Auth/SDK and the default harness stack. There are **two
functional cases**, not a new broad TaskStore qualification:

- Claim/heartbeat and same-actor observation preserve immutable audit,
  reference_generation and authored references; stale CAS, ActorBusy and a
  foreign actor's fence are rejected, with actual Session coordination.
- Native authored-reference omission with batch_rows1 reproduces the seed path
  and returns a valid InProgress claim with the actual reference generation.

Both had earlier exact passes (4.69s/1.50s). Current sdk03 passed all three test
functions: these two functional cases plus one diagnostic observation. The
latter measures `size_of` of the public claim future type without constructing
or polling that future: **17,744 bytes is future-state size**, not peak stack,
functional test coverage or a universal stack bound. Returned/audit-fence-complete/
native-empty-returned markers were observed in the current SDK regression.

### ELF frames and the Runtime experiment

Native06/07/08 aborted with SIGABRT before Returned. External boxing was tried,
failed and removed; it is not a delivered Records remedy. Native08's inspected
ELF prologue showed enclosing poll 1,072,680 bytes, seed 193,896 and
mutate 151,632 bytes plus 24 register pushes. The SDK mutate frame was identical;
SDK caller prologues were 109,672/399,568 bytes (the latter plus 8 pushes). These
are selected generated-code/frame observations, not cumulative or peak stack
measurements, allocation quotas or a portable performance guarantee.

The Root/Runtime owner split its own enclosing fixture into six async helpers,
keeping the same SDK seams, claims, Auth, Original and default stack. In
Native09-corrected, future-built/Returned/Session-claim/Memory-create markers
advanced without SIGABRT. **The full Now test still failed** with
KnowledgeRequiredField summary index0; Runtime owns that fixture correction and
next Native run. These observations delimit this enclosing-caller symptom;
they establish neither a Records defect nor Now/Main/global success.

The post-Native09 prologue artifact (`.build/rust-records-task-claim-stack/evidence/post-native09-frame-prologues.json`, artifact histórico indisponível)
records the current post09 ELF comparison: enclosing 292,608 bytes plus 8 pushes,
channels 483,784, seed 211,672 and the unchanged mutate 151,632 plus 24 pushes.
Its stable ELF SHA256 is
`be59d2b63a2a0848345222a1850e6905668091678cbfb9d672c6712ad95b2792`.
This artifact is labeled **current post09 inspection**, not substituted as the
exact provenance of the earlier Native09 execution. The separate
Native09 selected evidence (`.build/rust-records-task-claim-stack/evidence/native09-selected-evidence.json`, artifact histórico indisponível)
retains ten selected markers, static comparison/Source02 match and the full
failed Knowledge summary outcome. No missing Returned marker is fabricated.

### Disponibilidade histórica dos artifacts

Metadata readonly de 2026-10-12 encontrou ausentes o receipt
`.build/rust-records-task-claim-stack/validation.json` e os artifacts f382 citados
acima/abaixo. `tsk-f382` permanece COMPLETED19; execuções PASS e medições
registradas antes do alerta permanecem históricas, não falhas novas. O SHA
registrado acima não foi recalculado/verificado em arquivo atualmente disponível.
Não se atribui causa/actor, reconstrói arquivo/hash ou altera a proveniência de
Native09. A investigação/prova nova `tsk-23f2` é separada e não recria esse receipt.

### Scoped qualification and limits

Compile03 (`.build/rust-records-task-claim-stack/evidence/compile-03.log`, artifact histórico indisponível)
passed after awaiting the current ELF (26.62s; observed identifier prefix
ea5366a1cc98be28). sdk03 (`.build/rust-records-task-claim-stack/evidence/sdk-03.log`, artifact histórico indisponível)
passed three/zero failures (7.54s wall, 7.52s test): **two functional +one metric**,
not three functional regressions or repeated historical 70 qualification.
Strict03 (`.build/rust-records-task-claim-stack/evidence/strict-03.log`, artifact histórico indisponível)
passed (12.81s) with --lib --test task_store_claim_stack --no-deps -- -D warnings;
fmt03 passed. Previous Native aborts and failed experiments remain retained;
no production Records lifecycle/write/read or original fixture change was made.

This qualifies the two focal SDK regressions and diagnostic work on Linux.
It does not qualify NowNative/Runtime composition, a general stack bound,
non-Linux behavior, performance or Task lifecycle completion. A new concrete
reproduction would be required before a production Records correction; claim
semantics are not relaxed and no stack override, skip, fake grant, quota,
framework or fallback is introduced by this diagnostic.

Session revision SDK and authenticated consumer evidence, separate distinct counts and retained corrections are documented in [Native Session revision qualification](rust_task_protocol.md#native-session-revision-linux-qualification).


## Delivery commit metadata

`Task.commit` is an optional full Git object ID in the existing Task JSON payload, independent of native Lance/Git snapshot refs. `NativeTaskStore::set_task_commit` uses the existing authenticated writer, revision CAS and lifecycle publication; it preserves terminal status and check evidence. No physical table schema or dataset conversion is required. See [the Task delivery contract](task_module.md#commit-metadata-and-git-snapshots) for creation, claim handling, clearing and agent commit/amend instructions, and [the wire operation](rust_task_protocol.md) for transport inputs.
