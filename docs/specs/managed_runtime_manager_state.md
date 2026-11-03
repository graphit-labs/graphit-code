---
title: Managed Runtime manager state and turn fences
type: specification
status: proposed
updated: 2026-10-18
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-d7ab}
    relation: implements
  - target: {type: task, id: tsk-9bd4}
    relation: relates_to
  - target: {type: task, id: tsk-9a90}
    relation: derived_from
  - target: {type: task, id: tsk-a0f9}
    relation: derived_from
  - target: {type: task, id: tsk-be6e}
    relation: relates_to
  - target: {type: task, id: tsk-32db}
    relation: relates_to
  - target: {type: task, id: tsk-c847}
    relation: uses
  - target: {type: task, id: tsk-136a}
    relation: uses
  - target: {type: task, id: tsk-85d6}
    relation: uses
  - target: {type: task, id: tsk-0f4c}
    relation: uses
  - target: {type: task, id: tsk-1f4a}
    relation: relates_to
  - target: {type: task, id: tsk-84d5}
    relation: relates_to
  - target: {type: task, id: tsk-a59d}
    relation: relates_to
  - target: {type: task, id: tsk-613a}
    relation: relates_to
  - target: {type: task, id: tsk-4323}
    relation: relates_to
  - target: {type: task, id: tsk-5a55}
    relation: relates_to
  - target: {type: task, id: tsk-b96c}
    relation: relates_to
  - target: {type: memory, id: 01M4GMYQ6H5SEFTA86F5B5D7DT}
    relation: supports
---

# Managed Runtime manager state and turn fences

This is a future consumer contract for Runtime integrators and the Auth,
Data/Records, Core/FS, AI/Process, Hub and Knowledge producer owners. It
transcribes the revised specification of `tsk-d7ab` revision 12 under
`tsk-9bd4` / `ses-c98a`. Tracker NOFAULT at revision 3 covers packet
readiness only. The investigation prerequisite `tsk-9a90` completed at revision
18 establishes consumer requirements, not an implementation.

**No usable Rust manager API, executor, state machine or native product behavior
is delivered by this document. All MM-F1–MM-F10 fixtures below are future and
unexecuted. No runtime test pass, native qualification or performance gain is
claimed.** Producer APIs, their final Task IDs and qualification remain necessary
before implementation. P-MM, P-MA, P-MD, P-MO and P-MMI are contract request
labels, not exported Rust types or invented producer Task IDs.

This contract specializes the K-L managed Live session requirements in
[the existing architecture](../architecture/rust-knowledge-ai-live.md), source
lines 103–158, and its boundaries with K-E. It does not replace the eventlog,
replay or subscription contract. Native hook correlation and drain in
`tsk-be6e` revision 11 are a separate seam from this Live manager. Runtime's
[current exports](../../crates/graphit-runtime/src/lib.rs) contain only output
and roles; no existing independently consumed pure manager kernel was found.
The Auth, Data and fencing APIs each have their own qualification scope.

## MM-R1: identities, operations and authority

Keep managed-session identity, preparation identity, turn identity and fence,
manager-state generation, Auth issuer/run/request lineage, eventlog
generation/cursor, native conversation or CLI ID, and durable Task session
distinct. None is automatically interchangeable with another.

Owner derives from actual verified Auth identity, never a client-supplied string
or metadata. Each create, read, send, cancel-preparation, cancel-turn, close,
remove, outcome and history operation requires real admission and recheck for
its current operation and target before I/O, state mutation or effects.
`RecordTaskWrite`, a role enum, `Actor`, `RunHandle`, a read-only view or a phase
string does not establish managed-session policy. USER `Subject` remains absent;
PROJECT Records admission does not confer manager authority.

## MM-R2: states and transition preconditions

The state names describe required semantics, not a delivered Rust enum.

| Observed state | Required behavior and transition conditions |
|---|---|
| Preparing | Reject send. Qualified preparation may lead to Ready; preparation failure leads to Failed. |
| Ready | Admit at most one current turn. A concurrent send receives Busy or a safe state conflict; no unbounded queue. |
| Running | Reject another send. Normal termination, turn error or cancellation permits Ready only after real outcome and valid cleanup for the covered resources, with an admitted transition. |
| Failed | No implicit retry, new preparation or Failed-to-Ready transition. Retry, reopen or a new generation needs an actual producer policy. |
| Closed | Irreversible: admit no new preparation or turn. Remove never revives state. Logical closure does not imply physical cleanup or durability. |

After restart, a fresh authorized read of Preparing or Running observes
interrupted/Failed semantics. It restores no executor, issuer, request, grant,
fence capability or assumption of quiescence.

## MM-R3: linearization and one active turn

Data/Runtime must provide a real state view with generation/current turn and
mutations with CAS, idempotency and receipts. Observed readiness alone never
authorizes spawn. Two sends using the same expected generation produce one
winning committed admission and one Busy or safe conflict; they do not start
two children.

If commit acknowledgment is lost or uncertain, block conflicting new effects
and transitions and reobserve through the producer. Do not blindly retry or
spawn a duplicate. The initial question needs operation deduplication bound to
the payload and executes once after qualified preparation even without a
connected subscriber. The prompt exists only within its admitted realm and
budget; it never becomes a Task or shared-document payload.

## MM-R4: turn fences and late effects

Late cancellation, terminal events, cleanup, completion and outcome require the
real target/fence of their turn or preparation and the expected manager-state
generation required by the producer. Session, run or log generation is not an
automatic substitute for a turn fence.

Atomic recheck/CAS must prevent an old turn from cancelling, signalling or
transitioning its successor. Authorized history for the old turn is data for
that turn, not an effect on the current one. A duplicate operation with the same
identity and payload may reobserve its real receipt. A payload mismatch, stale
target or missing target yields a safe error without a new effect.

A consumer-generated `u64`, UUID, `Actor` or string comparison is not authority.
A boolean `cancel_if_current` callback is not a substitute for the producer's
atomic target admission and receipt.

## MM-R5: contexts, lifetimes and post-effect admission

Initial HTTP admission, preparation, the originally admitted run, subscription
and post-effect observation require their own real contexts and policies. A
subscriber request does not become run authority. Reconnect or retry does not
renew the execution deadline or budget. Do not create `Background`, a fresh
grant or a fresh request to bypass expiry.

Close/cancel prevents new admissions under the current fence and requests
cancellation through the real handle. A signal, token, dropped future or leader
exit is not proof of quiescence. Cleanup and its evidence writing/observation
after expiry require actual finite producer admission and horizons distinct
from continuing execution. No unbounded `WithoutCancel` or boolean authority,
cleanup or fence callbacks satisfy this requirement.

## MM-R6: logical state and owned physical resources

Closed means no new preparation or turn, not `Cleanup::Complete`, durable receipt
or model receipt. A logically Closed session with incomplete cleanup retains
ownership/resources and an honest outcome. Ready requires proof of quiescence
for the relevant resources that a new turn would reuse, not merely an observed
enum or leader exit.

Process `Cleanup::Complete`/`Cleanup::NotSpawned` covers only the tree and pipes actually
observed by that producer. Artifacts, workspace, sink, subscribers and additional
jobs require their own producers. `Cleanup::TimedOut`,
`Cleanup::Failed(IoFailure)`, dropped futures or unknown observations retain
pending/incomplete status and prohibit a successor on undrained resources.
Dropped futures and leader exits are observations, not `Cleanup` variants.

Remove requires owned-resource provenance and actual quiescence, or an explicitly
admitted retention/incomplete policy. A grace period never permits unconditional
deletion of live work. Completion of one producer does not prove cleanup of all
manager resources.

## MM-R7: persistence, uncertainty and outcome

Published state, intent, commit, acknowledgment, actual child start, terminal
observation, cleanup and persisted receipt are separate facts. If the eventlog
cannot append a terminal event, P-MO must offer an authorized outcome/cleanup
channel outside that log. Do not invent a persisted terminal event, Done or
rollback.

CAS/publication failure or evidence-persistence failure preserves a reobservable
uncertain/pending outcome. Do not assume rollback or blindly replay. A later
observer obtains fresh admission to read the specific outcome and its fence;
it does not recover the expired run's authority. GC, retention, pinned views,
corrupt state, provider failure, denial, missing and legitimate empty outcomes
retain the distinctions required by `tsk-9a90`.

## MM-R8: finite bounds and safe errors

Callers must supply finite positive limits for every applicable domain:

- Input/parser/prompt bytes and aggregate operations, admitted attempts and work.
- Active/pending preparations and turns, and owned resource handles.
- Queues, in-flight observations and retries.
- State/outcome metadata bytes and history retention.
- Session, preparation, run, subscription and cleanup durations.

Each bound needs an explicit unit/domain, exact-cap and cap-plus-one behavior,
and checked overflow before allocation or effect. Admitted failed work is not
refunded. Occupancy is released only after actual resource closure or consumption.
Logical bounds do not prove whole-process RSS limits or OS containment.

Safe semantic errors must distinguish Preparing, Busy, Failed, Closed, stale
target, state conflict, denied, revoked, missing, corrupt, unavailable, cancelled,
deadline exceeded, quota, incomplete and unknown commit. Final Rust error names
and enums belong to the producer. Errors and Task logs must not expose private
payloads, native CLI IDs, paths, keys or fences.

## MM-R9: ownership and integration boundaries

| Owner | Required responsibility |
|---|---|
| Runtime | Contract, composition and manager |
| Auth | Actual admission and manager authority |
| Data/Records | Durable state, CAS and recovery |
| Core/FS | Physical resources and original context |
| AI/Process | Execution and owned sink |
| Hub | Selected binding and captured context |
| Knowledge | Caller and source view |

Kiro V3 is the current supported binding; V2 and the old preview flag are not
maintained. Adapter conformance follows the current documented protocol and
controlled Graphit integrations, without requiring an installed vendor binary,
account or plan. A state-machine contract alone does not qualify execution. Runtime hooks
after mutation/final sync, `tsk-be6e` and `tsk-9a90` event replay remain separate
seams. Do not create a broad-parent-to-Daemon-4323-to-Runtime dependency cycle.
Root owns session coordination, manifests, lockfiles, guards and compilation
windows; this refinement supplies no changes to those surfaces.

## Producer inventory and concrete requests

This inventory records the decided snapshot, not an activation or qualification
of absent APIs.

| Producer snapshot | Available evidence and remaining requirement |
|---|---|
| Auth `tsk-85d6` / `tsk-0f4c` | Actual original issuer, caller, grants, run and Actor exist. Managed-operation policy and opaque owner handles are not delivered. |
| Records `tsk-1f4a` | PROJECT Records exports are activated but still unqualified; they are neither P-MA nor USER Subject. |
| Data `tsk-84d5`, `tsk-a59d`, `tsk-613a` | Proposals do not deliver complete manager state/current-turn CAS, receipts or recovery; P-MD remains absent. |
| P-MO | Authorized cleanup/outcome outside the eventlog remains absent. |
| Process `tsk-32db` revisions 4/5 | In progress and staged: decided incremental contract `run(request, &SnapshotContext, Retention, &mut IncrementalSink)`, a borrowed cancellation-safe future, zero queue and cleanup. It is not activated and qualified. |
| Process `tsk-c847` | Historically compiled finite [current `capture`](../../crates/graphit-process/src/lib.rs), lines 170–180, still takes `Request` and `&CancellationToken`. Root must serialize retirement only after 32db qualification. |

The consumer must neither preserve the old Process API as fallback nor treat
staged 32db as a delivered producer. Dropping a sink future is safe only under
the cooperative producer contract with no detached work. Consumer adapters that
copy or persist data need their own real joins and caps.

| Request label | Required producer response before composition |
|---|---|
| P-MA | Admission, owner, operation, current preparation/current turn, revocation and legitimate post-effect evidence authority. |
| P-MD | State view, expected generation/current-turn CAS, idempotency/payload binding, lost-ack handling, receipts and recovery. |
| P-MO | Authorized cleanup/outcome outside the eventlog. |
| P-MMI | Runtime composition/controller after those APIs and Process/AI/Hub/FS are qualified; no new source stub. |

Root must obtain the returned Task IDs and ownership for missing leaves, including
actual policies, types, errors, bounds and receipts. No boolean callbacks,
fabricated grants/SourceRefs, in-memory set receipts, unused enums/tables or
stand-in Rust executor/FSM/DTO/traits implement these missing dependencies.

## Future fixtures MM-F1–MM-F10: not executed

These are required future product scenarios, not tests run by this investigation.
Execution requires the real Auth/Data/Core/Process/AI/Hub/FS producers, native
harness, three-OS qualification, guards and an authorized compilation window.

| Fixture | Future scenario and required observation |
|---|---|
| MM-F1 | Validate selected CLI/binding before workspace creation. Unavailable selection fails without fallback or spawn. Preparing rejects send; qualified preparation yields Ready and failed preparation yields Failed. |
| MM-F2 | Two concurrent sends from Ready produce one admitted CAS/child and one Busy/conflict. The initial question executes once after preparation despite subscriber disconnect. |
| MM-F3 | Old-turn late cancellation, terminal/completion/cleanup/outcome cannot mutate or cancel the successor under its current fence. Authorized historic outcome remains independently readable for the old turn. |
| MM-F4 | Normal exit, turn error and cancellation distinguish real `Cleanup::Complete`/`Cleanup::NotSpawned` from `Cleanup::TimedOut`/`Cleanup::Failed(IoFailure)` and leader-only exit/dropped-future observations. Ready requires sufficient resource quiescence; incomplete cleanup retains resources. |
| MM-F5 | Cancel preparation or close preparation/run during I/O, callbacks and workspace work. Closed never revives or admits a new turn; logical closure differs from physical cleanup; remove never deletes live work. |
| MM-F6 | Lose state-CAS/append/receipt acknowledgment before and after commit; exercise mismatched payload, retries, revocation, eventlog-full and terminal-append I/O failure. Preserve honest unknown outcome and outside-log observation, without blind spawn, duplicate execution or fabricated rollback. |
| MM-F7 | Subscriber cancellation, reconnect and slow consumption do not cancel the engine. Internal owned-sink failure cancels the real run. Original deadline/cancellation never renew; 32db sink and cleanup contracts remain honest. |
| MM-F8 | Restart Preparing/Running yields interrupted/Failed under fresh admission. Restore no issuer/request/grant/fence/executor; distinguish old view/generation, wrong owner, corruption, missing state and provider failure. |
| MM-F9 | Exercise each explicit zero, exact cap, cap plus one, checked overflow and admitted-failure nonrefund. Release occupancy only after actual closure; no allocation/effect precedes admission. |
| MM-F10 | Exercise realm/payload/privacy and same-issuer/original-request/target-fence mismatch. Native ID, Actor and Task session are no authority shortcut; no USER admission without Subject or personal content in shared context. |

## Requirement and investigation check coverage

The seven returned checks belong to investigation and future documentation. They
do not qualify a Rust API, executed fixtures, native delivery or product behavior.
Acceptance, checkpoints, revisions and Task/session lifecycle remain with the
coordinator; this document reports no check status change.

| Requirement/fixture coverage | Returned investigation checks |
|---|---|
| MM-R1–MM-R2/MM-R9; MM-F1/MM-F8/MM-F10 | MM-A1 `chk-62ba4c5b7194`; MM-T1 `chk-df24bb59ea85` |
| MM-R3–MM-R7; MM-F2–MM-F8 | MM-A2 `chk-91ef5777ff74`; MM-T2 `chk-963089c54fa5` |
| MM-R8/producer inventory; MM-F9/MM-F10 | MM-A3 `chk-750d660a3c39`; MM-T1/MM-T2 |
| Packet/document/readback/readiness/ownership/persistence boundaries | MM-A4 `chk-830930bcb714`; MM-T3 `chk-d7f127dc5ce6` |

The next implementation decision requires real producer responses and ownership,
qualified APIs and fixtures, then independent readiness before editing source.
An existing independently consumed pure kernel was not found; an unused state
enum or transition table would not constitute delivery. This future contract
does not change frozen role/output source, shared documents, personal profiles,
credentials, trust, manifests or lockfiles.
