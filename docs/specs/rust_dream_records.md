---
references:
  - target: {type: task, id: tsk-ddeb}
    relation: implements
  - target: {type: task, id: tsk-04be}
    relation: uses
  - target: {type: task, id: tsk-633b}
    relation: uses
  - target: {type: task, id: tsk-c62d}
    relation: uses
  - target: {type: task, id: tsk-f2ba}
    relation: supports
  - target: {type: task, id: tsk-6aa2}
    relation: supports
  - target: {type: task, id: tsk-c62d}
    relation: supports
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
    relation: implements
  - target: {type: task, id: tsk-1f4a}
    relation: derived_from
  - target: {type: task, id: tsk-c775}
    relation: supports
  - target: {type: task, id: tsk-3ddd}
    relation: uses
  - target: {type: task, id: tsk-1228}
    relation: uses
  - target: {type: task, id: tsk-a59d}
    relation: relates_to
  - target: {type: task, id: tsk-613a}
    relation: relates_to
  - target: {type: task, id: tsk-0f4c}
    relation: supports
  - target: {type: task, id: tsk-85d6}
    relation: supports
  - target: {type: task, id: tsk-cd05}
    relation: relates_to
  - target: {type: task, id: tsk-97d9}
    relation: relates_to
  - target: {type: task, id: tsk-09f5}
    relation: relates_to
  - target: {type: task, id: tsk-9d92}
    relation: supports
---

# Rust Dream records

Audience: Records, Auth, AI and Runtime maintainers. Status: Source-free Dream contract refinement under `tsk-dcee`. PROJECT borrowed Auth admission is qualified (`tsk-1f4a`, supported by `tsk-c775`); ordinary native PROJECT Memory creation and lifecycle are implemented and qualified under `tsk-3ddd` and `tsk-1228`. F1 native PROJECT direct fenced create/update/delete and reobservation (`tsk-b2ca`) are implemented and Linux-qualified: 70 distinct functional tests, six Records doctests separately and scoped strict/owned formatting passed. F2 native importance transitions (`tsk-8dd2`) are implemented and Linux-qualified with 74 distinct functional tests, six Records doctests separately and scoped strict/edition-2024 formatting passed, and automatic consolidation (`tsk-f5d5`) remains Source-free refinement. Auth qualification and ordinary Memory qualification do not qualify Dream handlers, executor, dispatcher or product integration. Earlier P-V/P-D, mandatory ledger, quota/reservation and content-hash requirements were historical proposals, superseded by the concrete native leaves below; they are not prerequisites for those leaves.

Shared contracts live in [Rust record transactions](rust_record_transactions.md) and [Rust Memory records](rust_memory_records.md). Existing product behavior remains in the [Dream specification](dream_module.md). This proposal preserves behavioral purposes without copying Go HMAC tokens, run-ID regexes, JSON state or routine internals.

The local human-activity composition under tsk-cd05/tsk-9d92 is qualified separately from the native APIs: Source11 and CLI06 are reused, Human10 checks actual Source/CLI/RuntimeCaller, native fold/delete/idle, filesystem without pulse and joined shutdown, and drain09 covers retry/activity during native work plus queued cancellation. This operational timestamp does not carry Task/session data or authority and does not reset a partial Application's original Run/Actor. See [the Foreground composition evidence](rust_project_dream.md#source-and-qualification-boundary). LC/native session correlation remains separate. Installed vendor authentication and model access are not prerequisites for controlled Graphit qualification.

## Exact Dream capability boundary

| Capability | Domain effect |
|---|---|
| `Read(Module::Memory)` | Bounded reads of authorized Memory; mandatory is a selection filter, not `ReadMandatory` |
| `WriteFencedMemory` | Exactly Memory insert, update, delete, promote and demote, with real domain preconditions |
| Denied | Task/session discovery or writes, claims, Knowledge, AST, Hub, Git, shell/native edits, user access and indirect listing/calling of prohibited handlers |

`Role::Dream` has only those two permission ceilings in `crates/graphit-auth/src/policy.rs::Permission::allowed`. It always denies `ReadUser`, regardless of a future user-subject producer. Mandatory flags and parent Coordinator views cannot elevate it. Tool list and direct call must enforce the same classified descriptor, including aliases; enums/annotations alone are not dispatch proof.

An actual PROJECT ReadMemory grant permits nonmandatory reads needed for contextual recall and consolidation; Dream is not universally MandatoryOnly. Bootstrap mandatory selection (`queryMandatory`, proposed label with signature awaiting owner acceptance) and contextual `exclude_mandatory: true` are domain queries under the same actual issuer-verified Role/grant/operation, selected issuer/private AuthStore and original request. Missing permission, revocation or foreign issuer/request denies before corpus access; Role enums cannot admit it and USER remains denied. Standalone Auth proof is independent of Data. Native PROJECT captured reads have their own Memory qualification; Dream consumer selection needs its own actual admission and affected fixtures. Automatic consolidation is a separate consumer of admitted mutators, with its own preservation and recovery contract.

Scheduler/activity context comes from the authorized Runtime controller, not Task discovery exposed to Dream. The Source Root UserPromptSubmit ingress passes only a timestamp to the exact retained PROJECT/root/version slot; it does not depend on GraphitSessionId, grant Dream ReadTask or forward Task/session payloads. Native correlation and Task recovery remain outside this scheduler input. Prefurnished context is not a grant. Authored references are unresolved logical provenance, not permission or proof of target existence. Direct handlers do not gain target reads, reverse-graph scans or cross-corpus access from accepting references.

### Direct Memory tools versus automatic consolidation

The direct behavioral contract permits insert with all six classifications (omitted classification defaults to `fact`), independent `mandatory`/`important` flags, tags and validated references. Update accepts only title, body and references: omission preserves references, while an explicit list replaces them, including `[]`. It preserves classification, flags and creation metadata; actual actor/clock attribution can update. Delete conditionally archives the final record and tombstones it, including important or mandatory records; archival failure blocks deletion. It has no blanket flag or last-record protection.

Promote/demote change only `important`, preserving `mandatory`, classification, content, tags, references and creation metadata. The expected revision is checked before an already-requested-state no-op; that no-op succeeds without a new revision, publication or audit event. These operations have their own typed mutation identity, not a generic update alias.

Automatic consolidation is the Source-free contract `tsk-f5d5`, with registered Records application leaf `tsk-74e7`; the four native APIs/Source are Linux-qualified with 60 Memory SDK integration plus 25 Memory library functions (85 distinct), six Records doctests separately and corrected scoped strict exit0. Focal/private repetitions overlap; initial fixture/style failures remain historical without weakened assertions. Native qualification does not deliver a live Dream/Runtime/IAB/HTTP journey or non-Linux/performance/whole Root proof. Its [four implemented native APIs and exact DTOs](rust_memory_records.md#qualified-native-automatic-application) require actual Dream ReadMemory and FencedMemory admissions bound to the same PROJECT/version/Original. Derive strongest class and independent mandatory/important OR only from live members observed under held Writer/CAS; preserve survivor tags/references/creation, explicit nonblank body and omitted title. Each survivor/removal effect has a distinct explicit key in the actor/project/version namespace.

One fold constructs survivor and removal events, then publishes all their batches in one strict same-corpus PreparedDelta. The survivor and final archive/tombstones become visible in the same SDK manifest, rather than sequential publication/progress. Even an equal survivor gets a FoldSurvivor anchor only for effective removals. Exact full-group historical reobservation works after deletions; a subset or corrupt group is invalid, not logical partial success. Existing get/history validate only their own Memory chain; cross-member/event completeness is checked by reobserve_automatic_fold, using the stored request anchor/observations rather than reconstructing them from live members. The applied fixture contract preserves this local-versus-cross-group distinction without claiming a runtime result. Atomicity is SDK-only: Confirmed and real DeltaCommitFailure keys/causes remain unchanged, and independent caller actions retain earlier receipts if a later action fails. No Auth+SDK transaction, group ledger or blind uncertain replay is introduced.

Bare automatic deletion protects important **or** mandatory records and the last active record through validated metadata EOF under the same Writer/pin. Exact prior-event lookup precedes new CAS/protection; refusal creates no event/version/key reservation. These protections do not restrict qualified direct delete/demote and introduce no universal critical predicate. The delivered Foreground PROJECT caller in `tsk-f2ba` and `tsk-6aa2` consumes actual paired Auth admissions, validated complete proposals and retained lifecycle; `tsk-db94` reconciles that executed packet. Its separate local Linux Source/CLI and NativeProcess fixture qualification is documented in [Rust PROJECT Dream caller](rust_project_dream.md#source-and-qualification-boundary). Prompt, Kiro or embedding APIs alone do not deliver that journey, and no Runtime umbrella blocks the Records producer.

Canonical behavioral evidence is the active direct-tool contract (`tools_memory.go` lines 20–44, 162–211, 227–286 and 520–575), importance behavior (`memory.go` lines 329–375, 488–530 and 861–929) and the distinct automatic path (`consolidate_apply.go` lines 229–318, 376–406 and 442–477), recalled in `tsk-dcee`. These anchors establish behavior; Rust does not copy their internal storage, formats or execution details. The qualified direct F1/F2 handlers rely on their native receipts; this behavioral inspection does not qualify automatic application.

## First Auth factory and original lineage

P-F/P-A PROJECT borrowed admission is implemented and qualified as an Auth library under completed `tsk-1f4a` r15, including current-source Git failure/restoration proof and final Source/documentation Tracker review. The [Auth library boundary](project_record_admission.md) is PROJECT-only inside Auth, independent of Data: `ProjectRecordAuthority::new(&RoleIssuer)` is a pure selector, not admission. The proposed Dream consumer uses only `admit_read` with `ProjectReadModule::Memory` and `admit_write` with `ProjectWriteOperation::FencedMemory`, retaining private borrowed handles and `MemoryFenceRequired`. Task/Knowledge selectors in the public authority do not enlarge Dream's role ceiling or five classified Memory mutators. The implementation derives the private AuthStore from the selected issuer, compares selected/grant issuer and original request pointers plus real `same_request`, and rechecks original lineage. No substitute store or private profile seam is a public admission. Standalone Auth proof uses real physical sources, issuer/grant and original request independently of a Memory writer.

Source comparison confirms `RoleIssuer` retains private `Arc<AuthStore>` and `VerifiedRoleGrant` retains issuer, scope, original `RequestContext` and `SourceProof` in `crates/graphit-auth/src/issuer.rs`. Actual borrowed/opaque admissions in `issuer/records.rs` use those permissions/obligations, source generation, live PROJECT scope/version and original `SnapshotContext`; admit/getters/recheck validate the original lineage and current permission. Consumers must keep and recheck the handle before each query/effect. Role/Actor/project/request getters return observations, not new admission. A serialized invocation, role label, headers/environment, hash or scope DTO is never authority.

Linux qualification passed 18 fast record cases, 90 shared issuer cases including the ordinary 30-second original-deadline case, 87 documentation cases and strict Clippy. The fast filter is `issuer::tests::records::`; shared `issuer::` retains the deadline case. Historical r11 receipt SHA-256 `b1319c6e40b5d8d2d6d02139c24cc0887654c9c64d6fd364d1fe2b8a0add1b4d` and post-layout Source freeze `1b6abc9b9f059b6d830c6a0ad8bf170f9dc96be6ab2753b4da509f141711979e` preserve qualification and failure/layout lineage: production remained unchanged after PASS, with only five test-formatting hunks. These results qualify this PROJECT library boundary, not Dream handlers, USER or execution. Ordinary Memory CAS has its own independent native qualification.

Current completion additionally includes `tsk-c775`'s matching Auth case in the six-case Git proof: real `issuer::tests::records::broken_fast_contract` FAILED with panic/sentinel/exact phase, Cargo exit 101 and Git exit 1, then restored positive verification with isolation, unchanged Source freeze and scratch removal. The grouped raw log is `.build/rust-six-suite-fast-guards-2026-10-09/qualification-1791575907926208406/grouped-six.log`, SHA-256 `4378f7a7404b64451fe76498422f7cb27d15be400b7fc1bec8432c864351429c`. Final Source/documentation Tracker review returned NOFAULT. Group evidence is not Auth-only cost, Dream execution or qualification of other OSes.

The separate Native Coordinator USER consumer is implemented under `tsk-ddeb`;
integrated Linux functional qualification passed and affected strict qualification remains in the central queue. Actor, declared ProfileData,
realm/name or body metadata cannot substitute for actual USER admission. The
[native USER contracts](native_user_memory_store.md) expose literal owner and
real non-Dream UserReadAdmission/UserWriteAdmission boundaries. The
consumer uses those real producers and the [Native Parent factory](user_record_admission.md)
without elevating `Role::Dream`, whose PROJECT admission remains PROJECT-only
and whose ceiling permanently denies `ReadUser`. Extra ReadUser does not deny
legitimate PROJECT Coordinator ReadMemory; installation/global-only grants
without ProjectCapture cannot substitute for PROJECT admission. Conversely,
USER uses an actual installation/USER capture without selecting a PROJECT.
Producer qualification does not qualify this Runtime consumer; its separate
[contract and scoped evidence](rust_project_dream.md#coordinator-user-lane)
are recorded independently.

Auth administrative session, Auth run, harness session and durable TaskSession are separate identities. A new bootstrap/session/run cannot adopt old credentials, request identity or observations. Selected issuer, original grant/source proof, scope/version and real `same_request` lineage must survive every admitted tool effect; no new context renews deadline, cancellation or budget.

## Native USER fenced and automatic Source

`tsk-c62d` now implements the separate
[USER fenced/automatic methods](native_user_memory_store.md#user-fenced-and-automatic-implemented-sdk-qualification-pending).
Actual UserWriteAdmission must select FencedMemory with MemoryFenceRequired;
automatic application pairs real read/fenced-write admissions on the same
Original, literal owner and runtime root through capture and SDK boundaries.
These USER APIs are distinct from the qualified PROJECT Dream boundaries above:
**Role::Dream still cannot ReadUser**, so no live Dream USER journey follows.

The native implementation shares held writer/CAS/strict delta and exact USER
Confirmed generation/key/cause. Pure AutomaticMemoryEffect/MemoryFoldReceipt have
no owner or grant; sdk_version Some describes only this call's actual commit,
None describes historical retry/reobserve. Fold publishes survivor and removals
in one SDK delta, preserving survivor refs/tags and derived strongest class/OR
flags; bare automatic deletion protects important/mandatory and the last live
record. Full historical reobserve is read-only and needs no live removed members,
writer or birth. These rules do not imply atomic Auth+SDK or a group ledger.

The current native Linux focused scope has **40 distinct functional passes**:
USER26 (11 new SDK +three new real postcommit library +ten causal SDK +two
existing Confirmed/index library) and PROJECT14 (ten SDK +four causal library).
The current new scope is fenced3 +automatic8 (metadata EOF/concurrency included)
and three postcommit functions. Source04 automatic8 passed (46.09s), library3
passed (13.54s); Source05's updated output-receipt fixture passed one case
(4.61s) already included in those three. No overlapping run is added.

Current automatic publication retains its actual UserMemoryPublication through
receipt construction and final check; failed delivery recheck returns
UserConfirmed with real owner/generation/key/typed cause. The executed boundary
uses actual SDK commit and cancellation followed by fresh historical recovery;
there is no lost-ack interception or atomic Auth+SDK claim. Current Source04
compiled exit0 (22.82s), strict --lib --test user_memory_store --no-deps --
-D warnings passed exit0 (26.38s), six Records doctests passed separately (2.10s),
and owned fmt13/diff15 passed exit0. Earlier source03 preliminary ten-SDK/
two-postcommit and strict targets03 evidence remain historical, not added to 40.
No Task closure or USER Runtime/executor/App, host parser, semantic, non-Linux,
performance or global FullNoLoss qualification follows. Role::Dream still cannot
ReadUser. Existing PROJECT/F1/F2/automatic receipts are unchanged.

### Coordinator USER application

`tsk-ddeb` implements the Linux consumer in [Foreground USER Dream](../../crates/graphit-app/src/foreground_dream/user.rs) and [Daemon UserApplication](../../crates/graphit-daemon/src/dream/user.rs). It uses the existing `tsk-c62d` automatic/reobserve SDK; the producer's forty functional passes above are unchanged and do not prove this consumer. The Daemon `UserApplication` module and export now select Linux, macOS and Windows, matching the existing native USER Memory producer targets. The same cursor, physical USER capture/binding, paired admissions and retained-receipt recovery implementation is used on all three systems. This source change has not yet received its affected Windows compiler check; native Windows/macOS execution remains unqualified. Seven unchanged USER cases retain their existing proof.

The Native caller reads the complete admitted USER corpus, including
nonmandatory records, under actual `ReadUser` plus `ReadMemory`, the same
Original and literal installation/USER capture. `NoCorpus` or published-empty
returns before program/model resolution or write admission. For a populated
corpus, the actual `tsk-04be` [Parent admission](user_record_admission.md) pairs
ordinary `MutateUser` with administrative `WriteFencedMemory` and
`MemoryFenceRequired`. `Role::Dream` is not a USER principal. Caller, credential,
metadata and prompt fields are not grants.

The compiled [two-slot USER prompt](../../templates/agent-instructions/policy/prompts/user-dream-consolidation.md)
contains only an attempt ID and bounded admitted Memory projection.
`propose_guarded` reuses [the native Codex driver](../../crates/graphit-daemon/src/dream.rs)
with the caller's concrete `CodexSink` guard; read/write admissions, Parent
NativeLaunch and the same Original are checked at every event. The caller keeps
that authority and physical owners in the same frame until native join and
RemainingProcess observation. Cleanup cannot renew authority, convert failure
to success or turn a prefix/proposal into an effect.

Existing `recipe::prepare` remains pure and shared. `UserApplication` owns only
prepared recipe/input/keys, cursor, actual receipts and physical capture/binding.
Every apply checks current borrowed read/write authority and the same actual
USER binding before SDK reobservation or effect. It rejects A-to-B retargeting.
For the first attempt at each cursor, it calls the actual `automatic_*` SDK
method directly. The SDK already reobserves internally under the writer before
any new effect, with its existing idempotency, CAS and Fenced checks.
The application's in-memory attempted-operation state routes recovery only;
it contains no grant or replacement authority. Failure, uncertainty or
cancellation retains the cursor, input/key and earlier receipts. A resumed
operation uses the actual `reobserve_*` method under fresh legitimate USER
admissions and invokes `automatic_*` with that same input/key only on `None`.
A receipt or domain refusal advances the cursor and clears the attempted state.
A new instance replaying the same recipe retains the automatic SDK's
idempotency. No SDK deadline, timeout or guard is changed. SDK CAS, bounds,
references/tags, strongest class/OR flags, protected deletion and Confirmed
causes remain the real producer contracts above, without an Auth+SDK transaction.

The installation lane uses one explicit startup attempt even with zero
PROJECTs, then accepted real human activity plus idle and bounded retries/backoff.
Startup is not a human metric; success/noop waits for new activity. Source
forwards only the accepted `Instant` after existing authentication/Original/
active physical PROJECT checks, independent of USER read and TaskSession.
The USER lane does not follow PROJECT selection or retirement and is cancelled
and joined by Foreground shutdown. See [the complete cadence and lifecycle](rust_project_dream.md#coordinator-user-lane).
The executed fixtures use actual Linux AuthStore/signed Broker/Core/SDK/ProcessHost and a controlled Codex peer. Original request and test deadlines remain unchanged; internal identity reuse is synchronous and does not cache authority across stages. Windows/macOS, installed vendor authentication and global migration remain outside this scoped qualification.

## Native fenced leaves: implemented F1 and F2

Auth `ProjectRecordAuthority::admit_write` with `ProjectWriteOperation::FencedMemory` returns a borrowed `ProjectWriteAdmission`; `recheck` requires `Obligation::MemoryFenceRequired`. The obligation contains no Task ID, claim token, lease, revision or content hash. The Memory domain enforces actual absence/collision checks or expected-head revision and strict conditional publication. No caller DTO, factory, role label or metadata field supplies authority.

Current ordinary `NativeMemoryStore` writes accept only `ProjectWriteOperation::Memory`; they continue to reject `FencedMemory`. Native `memory_store/native.rs::check_write` checks the original request, exact original Arc and real admission before I/O. The qualified creation, update/delete/history and reobservation implementation already supplies the held Core writer, complete chain validation, strict Data delta and `MemoryStoreError::Confirmed` boundary. F1 now shares that implementation through private `WritePolicy::{Ordinary, Fenced}`, rather than creating a second writer or grant. The fenced policy requires the real FencedMemory operation, original Arc and MemoryFenceRequired obligation on every check; ordinary methods retain Memory-only checks.

### F1: create, update, delete and scoped reobservation

Native leaf `tsk-b2ca` (COM17) implements the five methods and typed inputs below in `crates/graphit-records/src/memory_store/fenced.rs`; it depends on `tsk-dcee`, `tsk-1f4a` and `tsk-1228`. Functional evidence is 48 affected MemoryStore integration tests (82.10 s, affected-store.log (`.build/rust-records-memory/fenced-direct/affected-store.log`, historical artifact)) plus 22 library memory_store tests (46 filtered, 6.18 s, affected-lib.log (`.build/rust-records-memory/fenced-direct/affected-lib.log`, historical artifact)): 70 distinct functions, including four new SDK integration functions and one new private Confirmed function. Focal five overlaps and is not added. The initial VersionContext API fixture compilation issue was corrected. Owned Source formatting/diff passed. Final scoped Clippy (`.build/rust-records-memory/fenced-direct/clippy-3.log`, historical artifact) passed exit0 with --lib --test memory_store --no-deps -- -D warnings (3m17 including normal queue). Records doctests (`.build/rust-records-memory/fenced-direct/docs.log`, historical artifact) passed six cases separately: four ordinary plus two compile_fail (5.02 s). Final owned formatting/diff passed. The first locked Clippy attempt stopped before compilation on a manifest/lock mismatch; Root reconciled it. The second attempt failed on the isolation fixture drop_non_drop line, then only that fixture line was removed. Targeted isolation (`.build/rust-records-memory/fenced-direct/focal-isolation-3.log`, historical artifact) passed one case (47 filtered, 2.60 s); production Source was unchanged, so the 70 current functional passes were reused rather than added or rerun. These logs and the historical receipt `.build/rust-records-memory/fenced-direct/validation.json` are unavailable in this checkout; the qualified results and scope are preserved in completed Task `tsk-b2ca`. Their current existence or hashes are not inferred.

All five implemented methods are async, taking `original: &Arc<SnapshotContext>`, borrowed `&ProjectWriteAdmission<'_>` and existing `MemoryStoreWriteLimits`:

| Implemented `NativeMemoryStore` method | Input and output |
|---|---|
| `fenced_create` | Existing `CreateMemory` → `Result<Memory, MemoryStoreError>` |
| `reobserve_fenced_create` | Existing `CreateMemory` → `Result<MemoryCreateObservation, MemoryStoreError>` |
| `fenced_update` | `MemoryId`, new `FencedUpdateMemory` → `Result<Memory, MemoryStoreError>` |
| `fenced_delete` | `MemoryId`, existing `DeleteMemory` → `Result<Memory, MemoryStoreError>` |
| `reobserve_fenced_mutation` | `MemoryId`, new `FencedMemoryMutation` → `Result<Option<MemoryRevision>, MemoryStoreError>` |

`FencedUpdateMemory` has exactly `expected_revision: u64`, `idempotency_key: String`, `reason: String`, `title: Option<String>`, `body: Option<String>` and `references: Option<Vec<MemoryReference>>`. It accepts no classification, flags, tags or authored provenance. F1 uses `FencedMemoryMutation::{Update(FencedUpdateMemory), Delete(DeleteMemory)}`; F2 extends that same enum with Promote/Demote as described below. These are typed Rust inputs exported from `memory_store/fenced.rs`, not a JSON host parser. Unknown-field rejection and omission/null parsing require a future host consumer and have not been proved here. Private exact conversion shares existing mutation semantics and cannot widen the typed input.

Create retains existing classification/flags/tags/reference validation, server-generated ID, collision/absence and exact-attempt idempotency. Update retains ordinary `NoChange`/CAS behavior, preserves omitted references and explicitly replaces supplied references. Delete archives through the existing domain and tombstones, without an added flag restriction. Actual head/event/reference-generation publication remains one strict delta, with full before/after history and real actor attribution.

Every path checks the closed admission mode, actual `FencedMemory` operation and obligation before I/O or replay lookup and at all existing rechecks. Reobservation needs current authority for the admitted shape and exact actor/project/version/key/input; historical success never enlarges authorization. Read reobservation does not birth, repair, materialize or replay. A fresh original and admission do not renew an old context or reconstruct an old issuer identity.

### F2: importance transitions after F1

Native leaf `tsk-8dd2` (COM17) depends on qualified F1 `tsk-b2ca` (COM17). Native importance transitions are implemented and Linux-qualified. Affected MemoryStore (`.build/rust-records-memory/fenced-importance/memory-store.log`, historical artifact) passed 52 functions (82.93 s), and library memory_store (`.build/rust-records-memory/fenced-importance/memory-lib.log`, historical artifact) passed 22 (5.25 s): 74 distinct functions. Four new importance functions are included; focal-4 passed those four with 48 filtered (36.75 s) and is not added. Scoped strict (`.build/rust-records-memory/fenced-importance/clippy.log`, historical artifact) passed --lib --test memory_store --no-deps -- -D warnings; edition-2024 owned rustfmt and diff passed. Records doctests (`.build/rust-records-memory/fenced-importance/doctests.log`, historical artifact) passed six cases separately (four compile/runtime and two compile_fail), with merged compilation 5.07 s and total 5.20 s; Cargo elapsed 11m43 included the normal queue and is not a performance benchmark. The historical receipt `.build/rust-records-memory/fenced-importance/validation.json` and referenced local logs are unavailable in this checkout; the qualified results and scope are preserved in completed Task `tsk-8dd2`. Their current existence or hashes are not inferred.

The implemented `ChangeMemoryImportance` in `memory_store/model.rs`, exported through the existing model::* export, contains exactly `expected_revision: u64`, `idempotency_key: String` and `reason: String`. Async `fenced_promote` and `fenced_demote` take the common original/admission/id/input/limits and return `Result<Memory, MemoryStoreError>`.

`FencedMemoryMutation`, `MemoryMutation` and `StoredMutation` now contain distinct Promote/Demote operations; their private stored input is `StoredImportance` with deny_unknown_fields. This closed persistence codec does not implement or qualify a JSON host parser. Their exact operation/input identity is audited for idempotency and recovery. The methods share Fenced mutation policy, captured writer, CAS/publication and extended reobserve. A transition changes only important, preserving mandatory, classification, title/body, tags, references/reference generation and creation metadata; actor and clock attribution advance through the existing domain. It publishes head plus revision together, and derivation rejects a persisted no-op transition. Exact lookup of an already-published historical attempt precedes new-head CAS and returns its original after state. For a new attempt, expected revision is checked before the already-requested state: recheck before delivery, then return current without event, SDK version, revision, materialization or receipt. No event means reobservation remains None, not a fabricated receipt. Whole-chain validation recognizes the distinct operations without compatibility or fallback.

F1 depends on this refinement, qualified Auth `tsk-1f4a` and ordinary lifecycle `tsk-1228`. F2 follows F1 because writer/model/codec files are shared. Root-approved ownership is narrow Records Memory modules/tests and affected documentation; Root retains shared manifests/lockfiles and other owners retain Core/Data/Auth/AI/Knowledge/Runtime/App. The coordinator maintains leaf lifecycle/checks. Automatic consolidation has a separate Source-free refinement `tsk-f5d5`, created OPEN r1, depending on `tsk-dcee`, `tsk-1f4a` and `tsk-1228`; its single-delta contract is closed in `tsk-74e7`, whose four native APIs have separate Linux qualification (85 distinct functional tests, six Records doctests and scoped strict0), with actual paired-admission checks inside capture and per-member chain discard after batch construction. Runtime `tsk-db94` keeps the separate actual caller correction. Consolidation and execution are separate deliveries, not umbrella dependencies or a wait for a Runtime executor.

## Isolation, publication and recovery

Use the actual PROJECT owner, requested logical version and same original Arc bound to the current admission. Keep captured directory guards and the held Core writer; validate complete input/history and expected heads under the same pin before conditional publication. Recheck original/admission/guard at operation boundaries before and after await/effects/delivery. These checks do not provide continuous SDK pathname confinement or atomic Auth+SDK/Data+OS behavior.

Inference stays outside writer critical sections. After proposing an effect, recheck current authority and head preconditions without renewing the request. Failure before commit cannot be called successful; physical intent or branch materialization may already exist. Known publication followed by failed recheck retains `Confirmed { version, cause }`; SDK uncertainty/conflict preserves its real cause and recovery key. Fresh scoped reobservation preserves an earlier commit and never implies rollback, regrant or automatic replay. References remain unresolved provenance, with no target existence check or incoming-link rewrite.

## Runtime and automatic consolidation are separate contracts

Runtime owns launch, ownership, cancellation, activity/idle rotation, retry and stop/wait contracts. Prefurnished scheduler context is not Memory authority. Records does not supply an executor, dispatch permission, scheduler or universally required ledger. No new ledger, reservation/quota framework, hash proof or factory is needed to make F1/F2 independently executable under actual Dream grants.

If a controller persists start/finish observations, a failed or uncertain start must block execution until its outcome is established under its real producer contract. Running alone proves neither current ownership, orphanhood nor adoption/dispatch permission. An uncertain launch must not automatically replay the executor. Memory commits survive failed or uncertain finish; restart uses current authority and ownership without replaying Memory from historical observations. Final Runtime APIs and placement remain owner decisions, not handlers delivered by this page.

Telemetry, when available through verified mediation, distinguishes attempts from committed effects and unknown observation from zero use. Do not persist bodies, prompts, model output, credentials or sensitive denied input as operational telemetry. No ledger implementation or explicit operational quota is introduced by this refinement.

## Proof scopes and remaining gaps

F1 has the qualified Linux functional, strict and Records doctest evidence above. F2 has its own qualified Linux 74-functional/six-Records-doc/strict/formatting evidence above; automatic native APIs have their own 85-functional/six-Records-doc/scoped-strict0 qualification; The separate Runtime consumer is qualified in the local Linux Source/CLI and NativeProcess fixture under `tsk-f2ba` and `tsk-6aa2`, with the human ingress evidence above; this does not qualify installed vendor authentication, LC, USER or Windows/macOS. F1 passes come from its affected executions, not an inference from historical Auth or ordinary Memory qualification:

| Leaf | Required observations |
|---|---|
| F1 admission | Real Dream grant permits Fenced and ReadMemory; ordinary methods deny Fenced, and fenced methods deny ordinary/other-module admissions, foreign Original, cancellation and revocation before prohibited effects. |
| F1 fields | Create all six classes with independent flags/tags/nonempty references; update title/body/reference omission versus `[]`, preserving flags/classification/creation; direct important/mandatory delete archives and tombstones with valid whole history. |
| F1 publication | Stale/concurrent CAS, exact-attempt and payload conflicts, historical reobservation and inherited/default/project/version isolation. The private Confirmed function passed both actual cancellation and actual Auth revocation cases after a real SDK commit through production publication, followed by fresh actual Auth admission/reissue, reobservation and public history. It is not public SDK interception, public-mutation fault injection, a lost-ack injection or atomic Auth+SDK proof. |
| F2 transitions | Distinct Promote/Demote codec/history; stale already-state still conflicts, current already-state succeeds with no SDK/revision/audit advance, exact key/payload/operation conflicts, fresh same-authenticated-actor reobservation, unchanged other fields/references and default/named isolation. |
| Consolidation | Native Source tsk-74e7 is Linux-qualified: observed strongest class/OR flags, preserved survivor tags/refs, distinct keys and FoldSurvivor anchor, one SDK delta, full-group reobservation, protected/last-record concurrency and causal Confirmed/DeltaCommitFailure. 85 distinct functional cases plus six Records doctests separately and scoped strict0; focal/private reruns are not added. Opaque module-owned observations are not grants; Runtime/live Dream remain separate. |
| Runtime consumer | Classified list/direct-call/alias ceiling, real caller mediation/authority and actual launch/cancel/stop/recovery contracts, without elevating Dream to Task, USER or native edits. |

Focused native qualification invocations are `cargo test --locked -p graphit-records --test memory_store fenced`, affected `memory_store` integration and relevant library filters when shared helpers change, `cargo clippy --locked -p graphit-records --lib --test memory_store --no-deps -- -D warnings`, `rustfmt --edition 2024 --check` on the changed Rust files, and Records doctests separately. Use the coordinated normal build window; no broad per-leaf gate or SDK/Auth replay from this documentation work.

Source ↔ page comparison covers real borrowed Auth admission (`crates/graphit-auth/src/issuer/records.rs`), Memory-only ordinary checks (`crates/graphit-records/src/memory_store/native.rs`), existing creation/strict publication (`memory_store/write.rs`), revision/CAS/reference-generation/reobservation (`memory_store/revision.rs`), the implemented five F1 fenced handlers/typed inputs and F2 importance methods (`memory_store/fenced.rs`), and distinct importance/persistence types (`memory_store/model.rs`). Their executed Linux receipts remain in [Native Memory store](native_memory_store.md); F1 supplies Linux-qualified fenced Source with 70 distinct affected functional passes, six Records doctests separately and scoped strict/owned fmt0. F2 native importance Source is Linux-qualified in its distinct 74-functional/six-Records-doc/strict/formatting scope. Importance uses the shared Confirmed path and the actual private F1 unit included in the 22 library cases; this does not claim a new importance-specific Confirmed injection or any SDK interception. No host JSON parser, five JSON handlers, App/CLI/MCP/HTTP delivery, executor, live Runtime Dream consolidation journey or caller, USER, Unknown injection, atomic Auth+SDK behavior, non-Linux qualification or performance claim follows.
