---
title: Project record admission
type: reference
status: implemented-library
updated: 2026-10-18
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-62a0}
    relation: implements
  - target: {type: task, id: tsk-2ba9}
    relation: implements
  - target: {type: task, id: tsk-72ce}
    relation: supports
  - target: {type: task, id: tsk-1f4a}
    relation: implements
  - target: {type: task, id: tsk-0f4c}
    relation: derived_from
  - target: {type: task, id: tsk-85d6}
    relation: derived_from
  - target: {type: task, id: tsk-3490}
    relation: derived_from
  - target: {type: task, id: tsk-c775}
    relation: validated_by
---

# Project record admission

This Rust library boundary lets a record handler select an existing `RoleIssuer`
and admit an operation using its original `VerifiedRoleGrant` or its actual
Runtime/Local `AuthenticatedCaller` and original
`graphit_hub::policy::RequestContext`. The implementation belongs to
`crates/graphit-auth/src/issuer/records.rs`; its real Runtime and Local-source
fixtures belong to `issuer/tests/records.rs`; the ordinary original-deadline
fixture belongs to the sibling `issuer/tests/records_deadline.rs`. Under
`tsk-1f4a`, Linux functional qualification passed the focused and shared issuer
suites, doctests and strict Clippy. Its matching current-source Git guard passed
as part of the six-suite group in completed `tsk-c775`, and the independent
source/documentation review confirmed this qualification. This qualified PROJECT
library boundary does not complete Records, USER or product integration.

## Selecting and using an admission

`ProjectRecordAuthority::new(&issuer)` is a pure borrowed selector. It creates no
issuer, grant, profile, store, request, key or file and performs no I/O.
`admit_read(&grant, &original_request, module)` and
`admit_write(&grant, &original_request, operation)` compare the actual selected
issuer and request references, the original snapshot identity and the presence
of the grant's captured project. Equal realm IDs, cancellation signals, deadlines
or serialized scope values do not establish that identity.

The factory checks the issuer's own private AuthStore against its captured
installation and then rechecks the original physical source, scope/version,
credential chain, expiry, run/source/profile generations and selected permission.
Physical source and scope inspection runs outside the issuer state mutex. Its
final credential/generation check and the selected permission use the same guard;
record admission, role authorization, delegation, actor and refresh inspection do not release
and reacquire it merely to repeat the same state check. The guard is dropped
before returning authority or starting I/O. Original cancellation/deadline checks
remain in place, including after contention. The returned admission retains the
grant's request; the caller cannot substitute a fresh context or renew its
original deadline.

| Read selector | Required permission |
|---|---|
| `Memory` | `Read(Memory)` |
| `Task` | `Read(Task)` |
| `Knowledge` | `Read(Knowledge)` |
| `Ast` | `Read(Ast)` |
| `Hub` | `Read(Hub)` |

The local semantic MCP route uses the same original PROJECT admission with
`Ast`; selecting a project or resolving a model does not grant access. Broker
PROJECT requests classify this selector as `ast`. This read integration adds
no AST write permission or indexing authority.

| Write selector | Required permission | Returned obligation |
|---|---|---|
| `Memory` | `WriteMemory` | `Unconditional` |
| `OwnTask` | `WriteOwnTask` | `OwnActor` |
| `FencedMemory` | `WriteFencedMemory` | `MemoryFenceRequired` |
| `Task` | `WriteTask` | `Unconditional` |
| `CoordinateTask` | `CoordinateTask` | `Unconditional` |
| `Knowledge` | `MutateModule(Knowledge)` | `Unconditional` |

The actual verified permission subset must contain that permission. A Coordinator
role observation does not supply permissions omitted from its grant, and
`WriteTask` does not authorize `CoordinateTask`. Existing closed role ceilings
continue to apply: Tracker cannot read Memory, Dream can read Memory and perform
fenced Memory writes, and Scribe's own-task writes retain `OwnActor`.

Read admissions expose `recheck() -> Result<(), AuthError>`,
`project() -> Result<&ProjectCapture, AuthError>` and
`request() -> Result<&RequestContext, AuthError>`. The read check returns only success after validating the original permission.
A Runtime/Local caller is not assigned a role for this operation.
Write admissions expose the same project/request getters, `recheck()` returning
`Obligation` and `actor()` returning the original server-minted `Actor`.
The project/request/actor getters repeat authority checks before returning data.

`task_actor()` is available only for `OwnTask`, `Task` and `CoordinateTask` and
rechecks the current admission. For Runtime it derives the Task participant from
server-owned InstallationId plus origin. For Local it also includes the canonical
provider/profile identities captured by the same successful authentication. It
excludes secrets, issuer epoch and credential generation; current credential,
revocation, PROJECT and original-request checks still apply. Fresh Runtime/Local
authentication after issuer restart or credential rotation preserves that Task
participant. Managed Role grants keep their execution-specific Actor; Broker Task
writes remain denied. This API supplies no USER subject or broader authority.
Records Task uses this accessor for attribution and claim comparison. The generic
`actor()` and its Memory/Knowledge consumers retain their existing identity contract.
No persisted owner is rewritten or adopted by this change.

Both admissions also expose
`project_and_request() -> Result<(&ProjectCapture, &RequestContext), AuthError>`.
It performs the admission's current `recheck()` once and returns borrowed
references from that same proof. A guard that needs both PROJECT metadata and
the original snapshot identity can compare those references without invoking
separate accessors that would each repeat physical validation. The existing
individual getters retain their checks. The tuple creates no grant, cache,
unchecked accessor or replacement request; it is only the result of that one
current observation. Write obligations and Actor checks remain the consumer's
responsibility.

This composition does not remove checks within a revalidation. In particular,
Runtime/Local authorization still surrounds `validate_scope`, which inspects
filesystem captures and takes locks. Consumers retain their checks before and
after I/O, waits and effects; they must obtain another current observation at
the next effect boundary rather than reuse an earlier tuple as authority.

`module() -> ProjectReadModule` and `operation() -> ProjectWriteOperation`
are const, read-only classification getters. They return the originally admitted
class without I/O or revalidation, allowing a Knowledge consumer to reject a
Memory/Task admission before touching payload or storage. The enum remains
readable after cancellation or revocation; it grants no access. Consumers must
still call `recheck()` before accessing data or applying effects under the
original admission.

These handles borrow the issuer, original proof, request and captures. They have
private fields and provide no Clone, Debug, serialization, deserialization,
ID constructor or recovery constructor. Compile-fail examples protect these
boundaries and the absence of a `User` read selector.

## Authenticated Runtime and Local callers

Use `admit_authenticated_read(&caller, &original_request, scope, module)` or
`admit_authenticated_write(&caller, &original_request, scope, operation)` with the
actual caller and a borrowed `ScopeInput` containing `project: Some(...)`. These
methods create no role, grant, run, session or credential. They retain the same
source proof and captures; there is no ScopeFacts copy or unrelated store opener.

The selected issuer, request reference and snapshot identity must match the
caller's original values. The existing AuthStore installation check and
`AuthenticatedCaller::authorize` validate the real Runtime/Local source and its
ordinary origin policy. `RoleIssuer::validate_scope` checks the selected
installation, exact allowed project/provider roots, owner/brand, physical binding
records and the project's VersionContext scope. A final caller authorization
checks the original source again after the capture checks. Getters and recheck
repeat this process under the same request; an equal deadline or cancellation
signal cannot replace it.

The operation selectors and permission table above apply unchanged. Role
ceilings and credential subsets continue to apply to verified role admissions;
Runtime/Local permissions come solely from their configured ordinary policy.
Write `actor()` returns the actual `AuthenticatedCaller::actor()`, and `OwnActor`
or `MemoryFenceRequired` obligations still belong to the record consumer.
The producer's metadata observation does not provide an atomic transaction with
Data or exempt the consumer's final effect recheck.

## Project, Memory and user boundaries

Only captured PROJECT scopes are accepted. An installation-only grant or
authenticated input without a project is denied
even if a Coordinator has both `ReadMemory` and `ReadUser`. Additional `ReadUser`
does not deny a legitimate captured PROJECT grant. An Actor, profile name or
declared identity is not a trusted user Subject; this boundary supplies no USER
admission or cross-user access.

Mandatory recall is a consumer query selection for bootstrap. `ReadMemory`
also permits the existing nonmandatory search and Dream consolidation journeys
within the original proof and project; this producer adds no universal mandatory-only
ceiling and creates no `ReadMandatory` permission. Record views must
preserve original authority, scope, generation and complete bounded paging.

## Errors, effects and costs

Foreign issuer/request, missing project, physical replacement, revocation,
generation changes and absent permission deny access without returning captured
project/request or actor data. Original cancellation/deadline failure reports
`AuthError::Request`; expiry reports `Expired`; busy/poisoned issuer state reports
the static `State` error. Existing physical resource errors retain their bounded
Auth error mapping. No error includes credential contents.

Admission performs bounded physical revalidation and state lookup, with no
publication, remote requests or record reads/writes. The AuthStore installation
bridge performs metadata checks without body I/O; Runtime and Local source
revalidation preserve their original request cancellation and deadline.
Local source revalidation is bounded against the original observation and does
not load or rebase an unrelated profile as new authority. Publishing even an
unrelated profile B change invalidates the original physical observation of an
already admitted request: its access getters and recheck deny. Between genuinely fresh
requests, unchanged profile A preserves its source generation, Actor, credential,
run and session expiry. The caller must verify the existing bearer against a new
original request and create new admissions; the old handles remain denied and
cannot adopt that request. Replacing A's selected key invalidates both generations
of observed admissions and prevents fresh verification of the old bearer.

The state check is an observation, not an atomic Data transaction. Before effects,
the consumer must recheck this admission and enforce classified operations,
actor comparison, expected revision/CAS, Memory fences and actual record scope.
This library provides no Records snapshot completeness, cursor, logical write
transaction, recovery authority, Runtime session ledger or native frontdoor.
Revocation after a returned observation requires the consumer's effect fence;
an earlier successful getter is not a permanent authorization receipt.

## Qualification

The joint `project_and_request()` observation under `tsk-cab0` passed 31 scoped
Linux cases: 28 current admission fixtures, the original-deadline fixture and
the two existing Broker read/write fixtures. They check borrowed PROJECT/request
identity across Role, Runtime, Local and Broker proofs, denial after cancellation,
expiry, revocation and physical replacement, and no extra PROJECT HTTP request
from the joint accessor. Execution took 33.748523 seconds, including the real
30-second deadline; the Auth library test emission took 21.759630 seconds
separately. Strict Auth library/tests Clippy passed in 14.257525 seconds.
Records are in
`.build/rust-records-data/knowledge-index-source/native-auth-refresh/auth-admission-composition/`.
This qualifies the Auth API and its affected fixtures; Data integration and the
large Source consumer have their own receipts. Existing qualification below is
preserved as historical evidence.

The authenticated-caller extension is integrated in Main under `tsk-62a0`.
All 25 focused admission tests passed, including the seven actual Runtime/Local
extension cases and 18 preserved role cases. The separate original-deadline
fixture passed for both original role and authenticated admissions after the
same real cutoff. All 133 Auth doctests passed, and strict all-target Auth Clippy,
formatting and the owned diff checks passed. Read recheck now returns Unit:
productive Data, Task and Memory consumers already discard its prior Role
observation, so they need no compatibility shim. Qualification logs are in
`.build/rust-auth-authenticated-record-admission-20261011`. This qualifies the
Linux Auth producer; it does not assert native other-OS or full frontend coverage.

The earlier classification-getter extension was integrated and qualified in Main under
`tsk-2ba9`: the 18 focused admission fixtures passed, including the real Runtime
and Local selector matrices and original cancellation/revocation cases; all 135
Auth doctests passed, and owned formatting and strict all-target Auth Clippy
passed. Logs are retained in `.build/rust-auth-admission-class-20261011`.
Data Knowledge consumer integration and native Windows/macOS execution were not
qualified by this extension. The earlier qualification below describes the
original admission producer.

The focused fixtures cover real five-role Runtime and Local grants, all three
read selectors and six write selectors with independently specified role ceilings,
exact Actor/obligation preservation and original I/O/no-write checks,
equal-realm issuer and equal-deadline request rejection, global grants, physical
project/installation/key changes, version mismatch, credential/run invalidation,
definition generation, expiry, original cancellation/deadline, state contention
and poisoning, final-source changes and real Local selective-profile updates.
The `issuer::tests::records::` filter covers the 18 fast cases. The separate
`issuer::tests::records_deadline::` sibling uses the existing ordinary 30-second
request policy and waits through its actual original cutoff after successful
admission. It is not ignored: it passed in the executed `issuer::` suite, and
full Auth/default pre-push suites include it. It adds about 30 seconds to those
full suites. Use the focused `issuer::tests::records::` filter for affected validation;
pre-commit runs only the native safety and formatting checks.

The Linux qualification executed all four functional commands successfully:

| Phase | Executed result | Command wall time |
|---|---|---|
| Focused `issuer::tests::records::` | 18 passed; suite 1.53 s | 5.951 s |
| Shared `issuer::` | 90 passed, including the 18 fast cases and real original-deadline sibling; suite 34.10 s | 34.243 s |
| Auth doctests | 87 passed, including 15 new admission compile failures; suite 3.20 s | 4.933 s |
| Strict all-target Auth Clippy | Passed with warnings denied | 6.511 s |

No executed test was ignored. The 18 focused cases recur in the 90-test issuer
suite; these totals do not describe 108 unique cases. Raw logs, timing, exact
arguments and the source freeze are preserved in
`.build/rust-auth-project-admission-2026-10-09/qualification-1791568814782381467/`.
Its `worker-result-before-edition-layout.json` has SHA-256
`f847e01fce79ca412edb2a7c419d256d914918bc97b0ebaa352279a258da376c`.
Command times include compiler/test harness work and are not product latency,
aggregate RSS or a hard resource limit.

Production `issuer/records.rs` remained unchanged (SHA-256
`3c94683414cab082525fdc9fefbcf89c597e230b9316857fee588464c8db914c`).
After those runs, five edition-2024 formatting hunks changed only the test file
from `75d218cb5f2a480f9584eff794b060432f2ecc45b68084a60610dfb9f49e2046`
to `a767f4679d49669204b9dc1d7deeb78205b9c6ed2f881ae3d446c7c4fad5a519`.
Scoped formatting/diff checks passed; no production behavior or test oracle
changed in that formatting step. The preserved post-layout receipt is
`.build/rust-auth-project-admission-2026-10-09/worker-spec5-postlayout-receipt.json`
(SHA-256 `b1319c6e40b5d8d2d6d02139c24cc0887654c9c64d6fd364d1fe2b8a0add1b4d`).
The four Cargo passes therefore refer to the pre-layout test hash; the subsequent
grouped Git proof below verified the current post-layout bytes. The earlier incorrect Local fixture
expectation remains a recorded failure; correcting it did not weaken physical
revalidation in production.

Executed functional invocations and the actual grouped guard invocation were:

```text
cargo test --locked -p graphit-auth --lib issuer::tests::records:: -- --nocapture --test-threads=2
cargo test --locked -p graphit-auth --lib issuer:: -- --test-threads=2
cargo test --locked -p graphit-auth --doc -- --test-threads=2
cargo clippy --locked -p graphit-auth --all-targets -- -D warnings
cargo xtask verify-fast-guards graphit-ast/scalar_policy graphit-auth/records graphit-knowledge/settings graphit-ai/cohere_embeddings graphit-ai/voyage_embeddings graphit-ai/google_embeddings
```

The matching `graphit-auth/records` proof is part of completed `tsk-c775`
(revision 14), whose final source/documentation review passed. Its fifteen-phase
positive baseline ran all 18 Records tests without the 30-second deadline sibling.
The actual injected `issuer::tests::records::broken_fast_contract` then produced
18 passing tests and one failure, its matching panic section/thread/sentinel,
the exact `cargo test --locked -p graphit-auth --lib issuer::tests::records::`
phase, Cargo exit 101 and Git pre-commit exit 1. The same positive gate passed
after restoring all six probes; compiled workspace isolation passed, all 361
frozen source/compiler inputs matched and owned scratch was removed.
Production source and the current `a767f467…a519` test hash were preserved.

The raw log is
`.build/rust-six-suite-fast-guards-2026-10-09/qualification-1791575907926208406/grouped-six.log`
(SHA-256 `4378f7a7404b64451fe76498422f7cb27d15be400b7fc1bec8432c864351429c`).
The adjacent `grouped-six-receipt.json` and `proof-summary.json` retain exact
arguments, per-target proof, freezes and cleanup. The complete six-suite group
took 290.497384 s with maximum process RSS 815,460 KiB. These are compiler/test/
hook costs for the whole group, not Auth-only latency or product resource limits.
No separate standalone Auth guard rerun is required for this matching proof.

Independent review confirmed that the current source, documented PROJECT
contract and actual functional/Git evidence agree. This qualification covers
that library boundary on Linux. Existing whole-system final guards remain
required; library, hook and documentation qualification do not establish native
Windows/macOS execution, a USER Subject/admission, Records/Data CAS or view
completeness, native frontdoor integration or product performance. Those broader
integrations keep this document's overall status partial.

Related contracts: [Typed authentication storage](auth_store.md),
[Role authority](role_credentials.md), [Private authentication document](private_auth_document.md).
