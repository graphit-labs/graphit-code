---
references:
  - target: {type: task, id: tsk-a59d}
    relation: implements
  - target: {type: task, id: tsk-d5a9}
    relation: derived_from
  - target: {type: task, id: tsk-1f4a}
    relation: derived_from
  - target: {type: task, id: tsk-0f4c}
    relation: supports
  - target: {type: task, id: tsk-85d6}
    relation: supports
  - target: {type: task, id: tsk-0a36}
    relation: relates_to
  - target: {type: task, id: tsk-cd05}
    relation: supports
  - target: {type: task, id: tsk-84d5}
    relation: relates_to
  - target: {type: task, id: tsk-c775}
    relation: supports
  - target: {type: task, id: tsk-658f}
    relation: relates_to
  - target: {type: task, id: tsk-9840}
    relation: relates_to
  - target: {type: task, id: tsk-dff0}
    relation: relates_to
  - target: {type: task, id: tsk-ea74}
    relation: supports
  - target: {type: task, id: tsk-9f12}
    relation: supports
  - target: {type: task, id: tsk-8e98}
    relation: relates_to
  - target: {type: task, id: tsk-0264}
    relation: uses
  - target: {type: task, id: tsk-08bd}
    relation: uses
  - target: {type: task, id: tsk-b6f8}
    relation: uses
  - target: {type: task, id: tsk-62a0}
    relation: uses
  - target: {type: task, id: tsk-c182}
    relation: uses
  - target: {type: task, id: tsk-8f4d}
    relation: uses
  - target: {type: task, id: tsk-aede}
    relation: uses
  - target: {type: task, id: tsk-1962}
    relation: specified_by
---

# Rust record transaction boundary

Audience: Records, Auth, Core, Data and authenticated-host maintainers. This page distinguishes delivered PROJECT transaction producers from the closed **TaskComment implementation contract**, refined in `tsk-a59d` and registered as `tsk-1962`. Comment native append/read and typed protocol dispatch are **Linux-qualified** in the affected tests/docs/scoped strict below. Task completion is not inferred from qualification, and this is not complete Records/App product cutover. Task/session lifecycle status is read from the Tasks, not inferred from this document.

## Delivered producer map

There is no general missing Task CAS/writer boundary. The following producers are delivered in Rust; their qualifications remain scoped to their actual operations and platforms.

| Producer | Delivered boundary |
| --- | --- |
| `tsk-0264` | Native Task/Session creation and complete captured reads, immutable creation and head consistency. |
| `tsk-08bd` | Task claims/fences/leases, heartbeat/progress/release, Session coordination/checkpoint and immutable lifecycle history. |
| `tsk-b6f8` | Task specification revision, checks/supersession, links/flags, Task completion/cancellation and Session completion under the actual writer. |
| `tsk-62a0` | Runtime/Local authenticated caller admission using the original request and selected PROJECT context. |
| `tsk-c182` | Typed `task_protocol::execute_write` dispatch into the native operations, preserving the original Arc and actual admission. |
| `tsk-8f4d` | MCP mutations and read-only HTTP host integration; this does not qualify every executable listener or browser ingress. |
| `tsk-aede` | Cancelled-child withdrawal from completion closure, while a cancelled dependency still blocks; Rust scope only. |
| `tsk-1f4a` / `tsk-c775` | Actual PROJECT borrowed Auth admissions and the qualified source-failure guard. |

The maintained [native Task store](native_task_store.md) and [native Task protocol](rust_task_protocol.md) describe the delivered APIs. Source anchors are [task_store/model.rs](../../crates/graphit-records/src/task_store/model.rs), [read.rs](../../crates/graphit-records/src/task_store/read.rs), [lifecycle.rs](../../crates/graphit-records/src/task_store/lifecycle.rs), [write.rs](../../crates/graphit-records/src/task_store/write.rs) and [task_protocol/write.rs](../../crates/graphit-records/src/task_protocol/write.rs). The inspected `TaskDetail` has Task, immutable creation, lifecycle events and real ordered `comments: Vec<TaskComment>`. The codec now has seven row families, including TaskComment, on the same shared 29-column schema; no second comment dataset exists.

### Actual publication and recovery boundary

The caller supplies the original `Arc<SnapshotContext>` bound to its actual PROJECT admission. `ProjectRecordAuthority::admit_authenticated_read/write` uses the real authenticated caller, original request and closed module/operation. Scope/version, branch, actor, permission and `same_request` cannot be manufactured from a DTO. Equal cancellation/deadline values do not identify the same request. [Project record admission](project_record_admission.md) and [role credentials](role_credentials.md) maintain those producer contracts.

The native writer retains its real `ArtifactWriter`, captured guard and SDK pin. After admission it captures the authoritative state, validates current domain/CAS/fence/lease conditions, prepares one strict SDK merge and commits it. Head and immutable event publish in that delta. Owner mutation checks compare expected revision, actual actor, private fence and the unexpired lease; actor and epoch-millisecond time are server-derived. Session coordination and Task ownership are distinct claims; one actor's current Task catalog is checked under that writer boundary. `ClaimToken` is private and is not serialized in public records or history.

`write::commit` rechecks original/Auth/guard and owner lease immediately around exposed preparation/commit boundaries. A known SDK commit followed by failed recheck returns `TaskStoreError::Confirmed { version, cause }`, retaining the committed version and causal error. SDK uncertainty keeps its cause/recovery identity; it is not NoEffect, absence, rollback or permission to retry a stale decision. Rechecks do **not** promise continuous pathname confinement or atomic Auth revocation versus SDK commit. A receipt or historical actor observation never grants authority.

Reads retain one captured generation, validate typed rows and complete continuous history to EOF, and reject incomplete or inconsistent projections instead of returning a prefix. They do not repair, expire claims, publish an index or silently advance to latest. Recovery uses fresh actual admission and its original request; it does not reconstruct old process-only authority, renew the old request or replay automatically.

Ordinary PROJECT Memory creation/catalogue/lifecycle/index/search have their own [storage](native_memory_store.md) and [search](native_memory_search.md) receipts. Their delivered writers and views supersede the old claim that an actual Records P-V/P-D producer was generally absent. They do not qualify USER, fenced Dream runtime/ledger integration, hard removal/reference arbitration, or a complete Records product.

### Qualified PROJECT Auth library API

The following public API is implemented in `crates/graphit-auth/src/issuer/records.rs` and qualified by completed `tsk-1f4a`; [Project record admission](project_record_admission.md) maintains the producer contract. `ProjectRecordAuthority<'i>::new(&'i RoleIssuer) -> Self` is a pure selector with no I/O, AuthStore argument or grant; construction is not admission.

| Actual API / selector | Qualified library contract |
|---|---|
| `admit_read<'a>(&'a self, grant: &'a VerifiedRoleGrant<'a>, original_request: &'a RequestContext<'a>, module: ProjectReadModule) -> Result<ProjectReadAdmission<'a>, AuthError>` | Real grant and original request borrowed; selectors `Memory`, `Task`, `Knowledge`, `Ast`, `Hub` require the corresponding `Read(Module)` permission; `Ast` is consumed by local semantic MCP retrieval and adds no write authority |
| `admit_write<'a>(&'a self, grant: &'a VerifiedRoleGrant<'a>, original_request: &'a RequestContext<'a>, operation: ProjectWriteOperation) -> Result<ProjectWriteAdmission<'a>, AuthError>` | Real grant and original request borrowed; closed operation mapping below |
| `ProjectWriteOperation::{Memory, OwnTask, FencedMemory}` | `WriteMemory`, `WriteOwnTask`, `WriteFencedMemory`, respectively |
| `ProjectWriteOperation::{Task, CoordinateTask, Knowledge}` | `WriteTask`, `CoordinateTask`, `MutateModule(Knowledge)`, respectively; `WriteTask` does not imply `CoordinateTask` |
| `ProjectReadAdmission::recheck() -> Result<graphit_hub::native::Role, AuthError>` | Actual Role observation from the current credential/profile definition under the same issuer, bounded lock and checks |
| `ProjectWriteAdmission::recheck() -> Result<Obligation, AuthError>`; `actor() -> Result<Actor, AuthError>` | Live operation obligation; actual actor from the original real grant |
| Both handles: `project() -> Result<&ProjectCapture<'a>, AuthError>`; `request() -> Result<&'a RequestContext<'a>, AuthError>` | Recheck the original binding before returning borrowed observations |

Handles retain private borrowed issuer/grant and closed selectors, with no `Clone`, `Debug`, Serde or construction from IDs. Admission, getters and rechecks compare the selected issuer pointer with the grant's issuer, the supplied original `RequestContext` reference with the retained request, and real original `SnapshotContext::same_request`. Retain the grant's original request; never substitute a fresh context. Derive the physical private AuthStore from the selected issuer and require an actual PROJECT capture. Recheck physical source, installation/issuer realm, scope/version/source generation, credential lineage, expiry/revocation, context and exact permission before returning.

Role, Actor, ProjectCapture and RequestContext getter results are observations, not admission or new authority. Consumers retain the opaque handle and recheck before each corpus query/effect. No arbitrary Permission, USER selector, `Configure`, `ManageAccessGrants`, `ManageAuthProfiles` or `ReadUser` admission is offered. A valid PROJECT Coordinator grant is not denied merely because it also grants `ReadUser`; installation/global-only grants without ProjectCapture deny. USER remains unavailable without a qualified trusted-subject producer.

Standalone Auth qualification used physical Runtime/Local sources, `RoleIssuer`/`AuthStore`/`RuntimeKeyOwner`, real grants and original requests, independently of Data or a Memory writer: 18 focused fast tests, 90 issuer tests including the real 30-second deadline, 87 doctests and strict Clippy passed on Linux. Completed `tsk-c775` proved a current-source Auth failure blocked Git with nonzero exit, then restored the positive channel; raw group-6 evidence SHA-256 is `4378f7a7404b64451fe76498422f7cb27d15be400b7fc1bec8432c864351429c`. Historical qualified `issuer/records.rs` SHA-256 is `3c94683414cab082525fdc9fefbcf89c597e230b9316857fee588464c8db914c`; earlier r11 receipts/layout observations remain historical. No tests or guard were rerun for this documentation update, and no other OS or product qualification is inferred.

`Obligation::OwnActor` is checked against the actual record actor; `MemoryFenceRequired` remains a real fenced Memory consumer obligation, not erased by dispatch. Auth administrative sessions, Auth runs and durable Task sessions are distinct identities. Observing a session never permits adopting another run or claim. The following qualified role ceilings remain independent of Comment or a parent Coordinator's grant:

| Role ceiling | Memory read | Task read | Relevant writes / private-user boundary |
|---|---|---|---|
| Scout | Allowed | Allowed | Denies `ReadUser` |
| Scribe | Allowed | Allowed | `WriteMemory`; `WriteOwnTask` with `OwnActor`; denies `ReadUser` |
| Tracker | Denied | Allowed | Denies `ReadUser` |
| Dream | Allowed | Denied | `WriteFencedMemory` with `MemoryFenceRequired`; denies `ReadUser` |
| Coordinator | Actual grant required | Actual grant required | USER denied while trusted subject producer is absent; future private-user access additionally requires actual `ReadUser` **and** `Read(Module::Memory)` plus matching qualified subject |

These are ceilings, not grants. Mandatory flags cannot override denial or add `ReadUser`; a parent Coordinator grant cannot elevate another role's view.

## TaskComment contract: tsk-1962

This section records R1–R5 accepted in `tsk-a59d` and inspected in the initial `tsk-1962` Source, **implemented and Linux-qualified with affected tests/docs/scoped strict passed**. Root approved `crates/graphit-records/src/task_comment.rs`, `task_store/comments.rs`, their Task store/protocol tests and only the required model/codec/read/lifecycle/dispatch/export hunks. Root owns shared manifests/lockfiles and composition; App/UI/Runtime/Data/Core/Auth remain separate owners. Current Source anchors are [task_comment.rs](../../crates/graphit-records/src/task_comment.rs), [comments::assemble](../../crates/graphit-records/src/task_store/comments.rs), [rows codec](../../crates/graphit-records/src/task_store/rows.rs) and [protocol output](../../crates/graphit-records/src/task_protocol/output.rs). No MCP/App Comment method is implemented by this leaf.

### R1: append-only data and literal Markdown

`TaskCommentKind` is closed: `note`, `decision`, `problem`, `lesson`, `knowledge`. Preserve body Markdown literally, including indentation; reject only `body.trim().is_empty()` for blankness, without canonical trimming. Require the existing canonical nonempty idempotency key. IDs use the native opaque nonzero ULID mechanism generated server-side through `write::fresh`, without deterministic Go identifiers.

The implemented `TaskComment` fields are `id: TaskCommentId`, `task_id: TaskId`, `session_id: SessionId`, `idempotency_key: String`, `sequence: NonZeroU64`, `kind: TaskCommentKind`, `body: String`, `actor: String`, `at: u64` and `revision: u64`. Sequence is contiguous 1..N per Task; revision is the Task revision after the insertion. Actor derives from the actual admission and at from server epoch-millisecond time, never caller input. Comments remain readable after release, expiry/reclaim or terminal transition.

### R2: ordinary owner append and one publication

Implemented async API, Linux-qualified in the affected scope below:

```text
NativeTaskStore::add_task_comment(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    id: TaskId,
    input: TaskCommentUpdate<'_>,
    limits: TaskStoreWriteLimits,
) -> Result<TaskCommentOutcome, TaskStoreError>

TaskCommentUpdate<'a> { claim: ClaimUpdate<'a>, lease: Duration,
                    idempotency_key: String, kind: TaskCommentKind, body: String }
TaskCommentOutcome = Created { task: Task, comment: TaskComment }
                   | Existing { task: Task, comment: TaskComment }
```

Comment uses private `acquire_unmaterialized` over the shared acquisition implementation: retain the actual Writer, guard and captured pin without materializing an inherited target before full validation and same-pin idempotency lookup. Other existing operations retain their ordinary `write::acquire` path. For Comment, only a validated new insertion calls the private materialization helper before preparation/commit; Existing does not materialize.

The caller must have ordinary OwnTask or Task authority under the existing policy, with the Task InProgress and the actual live actor/private fence. This method does not claim a Task, create a corpus, select USER or construct authority. A new comment requires expected revision equal to current, a positive lease and checked arithmetic: lease becomes `max(current_lease, now + lease)`, Task revision increments exactly once, and comment sequence increments once. Only a valid new insertion materializes the captured target, before preparing/committing the merge. Head, immutable `CommentAdded { id, sequence }` event and standalone comment body row publish in **one strict SDK delta**. The event does not duplicate body into audit. A failed guard/recheck during SDK branch materialization may leave branch metadata already present; this is not a promise of NoEffect metadata or atomic Auth/SDK publication.

### R3: retry identity, CAS and causal recovery

Read complete history/ordered comments from the same write pin before effect. Idempotency is scoped by Task + key. Existing key requires exactly equal kind, literal body and actor; mismatch is `IdempotencyConflict`. Prove the current actor, actual private fence and live lease independently of the special retry revision rule. Also require current `claim_epoch` equal to the historical CommentAdded event's `record.ownership.claim_epoch`. Under that **same claim epoch, live fence and actor**, an identical retry accepts expected revision equal to `comment.revision - 1` (original pre-comment revision) or the currently observed revision. Other revisions fail `RevisionConflict`. A valid new token held by the same actor after release/reclaim does not satisfy historical claim identity and must return `FenceMismatch`; no private nonce is added to the public comment.

Return Existing with the observed Task and original comment, without branch materialization, SDK version advancement or lease extension. A replaced fence, foreign actor, revoked/cancelled request or expired lease is denied before lookup; possession of a key is not permission to read or recover. Final and SDK boundary rechecks still apply. An acknowledged commit or known-commit recheck failure cannot be reported as no effect: retain Confirmed version/cause. Reobserve/get using a fresh RequestContext, newly authenticated caller and new admission for the same Task participant, with the current owner's valid fence; unknown outcome never authorizes blind replay or adoption. Task Runtime/Local identity is derived by the rechecked `task_actor()` admission and remains stable across issuer restart with a new valid credential, within the installation and canonical identity contract of the [native store](native_task_store.md#claims-renewal-and-handoff). Generic `actor()` remains epoch-bound and managed Role participants remain execution-specific. Never reuse a caller/admission from the cancelled request.

### R4: complete same-pin comment/event validation

The private comment-write `read::task_detail_at` audit includes real `Vec<TaskComment>` ordered by sequence. It never fabricates `[]` as a substitute for stored rows. Validate the entire same-pin stream: unique IDs/keys/sequences, contiguous sequence, strictly increasing Task revisions, nondecreasing at and consistent task/session/native actor bindings. Equal epoch-millisecond timestamps on ordered comments are legal. Every CommentAdded event must have exactly one corresponding comment with equal id/sequence/revision/actor/at, and every comment must have its event. Comment at must exactly equal the event observation/updated timestamp; its historical event record must be InProgress with owner equal to actor, positive claim epoch and a lease live at that event time. Orphan, missing, late corrupt or partial input fails the whole detail; no healthy empty or partial comments escape.

Use private `read::task_detail_at(pin, ..., before)` for both authorized read and comment write, retaining the existing continuous lifecycle validation. Do not recapture latest or call a public getter with a fabricated read admission. Reading a terminal Task does not resurrect it or discard its comment history.

### R5: same schema, neutral headers and typed protocol

The implemented `Row::TaskComment` uses `task_comment/<TaskULID>/<sequence>` and the **same schema**. Record ID is the real TaskId, session ID is its actual session, idempotency/revision correspond to the comment, and created_at/updated_at equal at. Non-head headers use defined typed neutral values: empty title/task_type, zero priority/claim_epoch, status `comment`, empty dependencies/checks, flagged false and null optional values. Payload contains the comment without a private token. Decode/reencode compares every column and all payload bindings; no schema fallback or conversion of old/corrupt corpuses is introduced.

Catalogue/heads retain their Task/Session filters and do not count comment rows. The implemented protocol is `WriteRequest::AddTaskComment(CommentInput { target: OwnedTarget<TaskId>, lease_ms: LeaseMillis, idempotency_key, kind, body })`, classified as OwnTask, and `WriteResponse::Comment(Box<TaskCommentOutcome>)` through the actual typed `execute_write`. The Box is Serde-transparent on the wire: it adds no extra JSON wrapper. That historical read-response qualification used `GetTask(Option<Box<TaskDetail>>)` and `GetSession(Option<Box<SessionDetail>>)`. The current selected read dispatcher instead returns `TaskGetRead` and `SelectedSessionRead`, mapping present values with `.map(Box::new)`; their explicit pages are specified in [Native Task store](native_task_store.md). Unused public full-audit getters are removed; complete read audits traverse explicit selected pages through EOF, while the private writer audit remains on its actual captured pin. Wire JSON stays null/detail. The historical Comment response-layout reconciliation passed its recorded Task protocol, Records-doc and scoped strict checks; those receipts do not qualify the later selected-read or public full-getter removal changes. Task detail read responses carry real stored comments. Actor/scope/branch are not input fields. App/MCP catalogue, UI comments and specification-history presentation remain productive consumers owned elsewhere; existing Revised events are the specification-history input, not a new Go-format DTO.

## Comment fixtures, documentation and qualification gate

The affected Linux Task store suite passed **21 cases**, Task protocol **seven cases** and library `task_` **32 cases**: **60 distinct tests**. Focal counts of seven and one are already included, not extra tests. Records doctests separately passed **six cases: four ordinary and two compile_fail**. Scoped strict `--lib --test task_store --test task_protocol --no-deps -- -D warnings` passed with exit 0. These scopes qualify the native Comment product boundary on Linux; Task completion and unrelated consumer delivery are not inferred. Existing producer receipts remain separate. Post-boxing protocol7 PASS and Records doctests6 PASS retain the same 60-test + six-doc count; they are repeat validation, not new distinct cases. The qualification command results (`../../.build/rust-records-task/comments/qualification.json`; historical local artifact unavailable) record post-reconciliation protocol exit0 at 18.45 s and scoped strict exit0 at 37.06 s, command timings rather than benchmarks. The earlier strict exit101 remains in that history before the compact response reconciliation and passing rerun.

| Fixture | Required real evidence |
| --- | --- |
| F1 | Real Auth/Core/Lance temporary corpus: create/claim, indented Markdown append, persist/reopen ordered detail, second kind, release/reclaim and terminal readability; server ID/actor/time, revision/sequence and lease rules, including two ordered comments with equal at accepted; no public token. |
| F2 | Exact key retry with original pre-comment/current revision returns Existing without generation or lease advance; changed kind/body conflicts. The same actor after release/reclaim with a valid new token but another claim epoch is rejected as FenceMismatch. Concurrent adds have one strict winner/continuous sequence. Stale/fresh foreign fence, expired/revoked/cancelled request, invalid body/key/lease and another Task with the same key preserve their exact outcomes. |
| F3 | Default/named/inherited branches, project isolation and old pin; late corrupt headers/keys/sequence/event bijection fail the whole detail. Known SDK commit then cancellation conserves Confirmed version/cause and persisted comment; fresh actual authority can observe it. Private production boundary composition is not SDK interception or lost-ack E2E proof. |
| F4 | Actual typed AddTaskComment dispatch/classification, strict input rejection and ordered read result; affected native Task store/protocol, library/docs/strict/formatting and Source↔docs checks, with no App consumer qualification inferred. |

Focused validation commands for the initial Source, in the serial Root build window:

```sh
cargo test --offline --locked -p graphit-records --test task_store -j1 \
  --target-dir .build/rust-records-data/knowledge-index-source/target
cargo test --offline --locked -p graphit-records --test task_protocol -j1 \
  --target-dir .build/rust-records-data/knowledge-index-source/target
cargo test --offline --locked -p graphit-records --lib task_comment -j1 \
  --target-dir .build/rust-records-data/knowledge-index-source/target
cargo test --offline --locked -p graphit-records --lib task_store -j1 \
  --target-dir .build/rust-records-data/knowledge-index-source/target
cargo test --offline --locked -p graphit-records --doc -j1 \
  --target-dir .build/rust-records-data/knowledge-index-source/target
cargo clippy --offline --locked -p graphit-records --lib --test task_store \
  --test task_protocol --no-deps -j1 --target-dir .build/rust-records-data/knowledge-index-source/target \
  -- -D warnings
```

Executed evidence includes rejection of corrupted historical Session attribution by the real SDK-backed codec, inherited exact retry without branch birth, actual Role credential revocation, concurrent keys and readable completed/cancelled comment history. The private production Confirmed composition uses an actual published SDK comment, actual cancellation and the production `after_commit(version, native_check_error)` boundary; fresh request/caller/admission under the **same issuer and actor** returns Existing. This is not intercepted SDK lost acknowledgement or absolute Auth/SDK linearization.

Failure history remains: initial E0433 fixture compilation ran zero cases; the second focal had four passes and three fixture failures caused by different issuer/admin-session setup. Corrected fixtures retain the same issuer, authenticate a new caller in the fresh request/admission, and derive the revoked run from that same administrative session. The private library run had 31 PASS/one fixture failure (`MissingRequiredKind`); supplying both required checks yielded the final 32 PASS. These corrections do not change Auth Source or weaken actor/lineage checks.

The intended receipt is `.build/rust-records-task/comments/validation.json`, with actual counts, retained failure history, logs/results/hashes and limits. No Comment command was run by this documentation transcription; the functional/strict evidence above comes from the producer's actual executions. Other OSes and productive App/UI/Runtime listeners are not qualified here. The maintained native store/protocol docs describe the Linux-qualified APIs and their exact limits; lifecycle/check completion remains in the Task.

## Residual obligations and historical proposal boundary

Earlier P-A/P-F/P-V/P-D/P-R packet names described analysis boundaries. They are not new factories, grant DTOs, required public APIs, journal/anchor engines, quotas, ledgers or resource frameworks. Earlier claims of universally missing snapshots/writers, G5/metadata/layout/model proposal status and prospective Go-shaped persistence details are historical planning context, **not the current native contract**. Original context, actual admission, causal recovery and isolation constraints remain; they are enforced by the delivered producer APIs and narrow new leaves rather than porting Go internals.

| Earlier proof | Current bounded evidence / residual |
| --- | --- |
| T1–T2 | `tsk-08bd` supplies actual one-actor Task claims, fence/epoch/live lease behavior. Explicit force takeover is not a delivered API. |
| T3 | `tsk-b6f8` supplies revision/check/supersession/history behavior; Comment idempotency/event uniqueness is implemented and Linux-qualified in `tsk-1962`, with affected functional/docs/scoped strict proof. |
| T4 | Existing writer preserves Confirmed version/cause after known commit and causal SDK uncertainty. Comment affected recovery fixtures passed within the boundary limits below; no new projection/journal framework or continuous Auth/SDK atomicity is implied. |
| T5 | Actual PROJECT/branch/original-request isolation is delivered by native producers and Auth. Qualified USER subject/admission remains a separate prerequisite. |
| T6–T7 | Shared typed-reference generations, hard removal and incoming-reference concurrency require their own accepted producers; prose is not a typed relation. None is part of Comment append. |
| T8 | PROJECT Memory lifecycle/index/lexical search have separate qualified archive/current/delete receipts; no Memory mutation scope moves to Comment. |
| T9 | Real denial/cancel/deadline guards and known-commit Confirmed exist. Comment tests must exercise their own boundaries without claiming atomic Auth/SDK interception. |
| T10 | Session association/completion and Rust child/dependency closure are delivered in their qualified scope. Session cancellation, hard removal and complete projections remain separate. |

The old live-Go scheduler barrier is unchanged: Rust `tsk-aede` does not authorize changing a Go store, running tool or cancelled Go work. Dream has Memory-only capability under its actual fenced admission and future runtime/ledger contract; prefurnished text and observational TaskSession recovery do not grant Task discovery, claim or run adoption. Knowledge corpus/embeddings/interoperability have their own actual Data/Knowledge/AI contracts; this page neither imports their formats nor invents a multi-table publication primitive. Full product cutover, USER, Dream and unrelated host/frontend obligations remain with their owners.
