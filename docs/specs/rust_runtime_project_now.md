---
status: source-qualified
references:
  - target: {type: task, id: tsk-cab0}
    relation: relates_to
  - target: {type: task, id: tsk-9d34}
    relation: corrects
  - target: {type: task, id: tsk-bcf2}
    relation: uses_api
  - target: {type: task, id: tsk-2f38}
    relation: implements
  - target: {type: task, id: tsk-b98d}
    relation: implements
  - target: {type: task, id: tsk-62a0}
    relation: uses
  - target: {type: task, id: tsk-08bd}
    relation: uses
  - target: {type: task, id: tsk-3ddd}
    relation: uses
  - target: {type: task, id: tsk-a542}
    relation: uses
  - target: {type: task, id: tsk-dd0f}
    relation: relates_to
  - target: {type: task, id: tsk-795b}
    relation: derived_from
  - target: {type: task, id: tsk-7240}
    relation: derived_from
---

# Native Runtime PROJECT Now composition

This reference is for Rust Runtime consumers that already hold an authenticated
Runtime or Local caller, or an already verified Role grant, its original request, and the selected PROJECT/version
captures. `graphit-daemon::project_now` composes the four local Now sections from
the qualified Auth, Records and Data APIs. Source qualification includes two PROJECTs on the same issuer and caller
at the Linux library boundary described below.

The compositor source is enabled for Linux, macOS and Windows with the same real read-head APIs.

The approved wire belongs to logical project **graphit-broker**,
`docs/rust-now-local-wire.md` (wiki `Rust_Now_local_wire_contract`, contract
`tsk-795b`). The local domain rules are maintained in
[PROJECT head sources](rust_now_project_heads.md). This module implements that
bounded local contract; remote presence and host integration have their own
delivery requirements.

## Calling the library

`read_project_now(authority, caller, request, scope, limits).await` returns
`Result<NowLocal, ProjectNowError>`. The function borrows the existing
`ProjectRecordAuthority`, `AuthenticatedCaller`, `RequestContext` and `ScopeInput`.
`ProjectNowLimits` contains explicit `TaskStoreReadLimits`, `MemoryHeadReadLimits`
and `KnowledgeHeadReadLimits`. Each producer keeps its own limits; there is no
default scan budget or common resource reservation.

The operation derives the original `Arc<SnapshotContext>` from
`request.snapshot()`. It calls `admit_authenticated_read` independently for Task,
Memory and Knowledge with the same authority, caller, request and scope. Task
and sessions share the Task admission and SDK pin. Memory and Knowledge each
read their own pinned corpus to EOF. The function does not issue a Role, credential
or grant, choose a version, read maintained inventory, or mutate record ownership.

The Role entrypoint is `read_project_now_role(authority, role, request, scope,
limits).await`. It accepts a genuine `VerifiedRoleGrant` already verified for
this request and calls `admit_read(role, request, module)` independently for the
same three modules. The grant retains its authenticated scope; the selected
PROJECT/version passed to the compositor is compared with each admission before
reading a corpus. Both entrypoints reuse the same private read/projection/final
recheck implementation. No Role is converted into a Runtime or Local caller.

For the managed roles, Scout can read all three sections, Tracker has Task and
Knowledge, and Scribe has Task and Memory. A missing module permission yields
`failed`/`forbidden` for that module and preserves other authorized sections.
Task and sessions continue to share their one Task admission. A current Role
without `RemoteRequest` can read these local heads; this function neither calls
remote Presence nor grants that permission. Revoked or expired grants cannot
return the earlier head data. HTTP/MCP verification, run ownership and transport
are separate consumers.

The Role entrypoint is qualified under `tsk-2f38` at the Linux library boundary:
three native Role journeys passed, including real run issuance, module denials,
revocation, expiry, foreign request/PROJECT/version, binding verification and
physical replacement. The existing Runtime populated and Local fault-isolation
smokes also passed after extracting the shared compositor. Both public rustdoc
examples and scoped daemon lib/tests Clippy passed. Receipts are in
`.build/rust-managed-roles-20261011/role-now/`. These results do not qualify a
Role MCP transport, a remote Presence request or a running native agent.

Use `NowLocal::project_id()`, `generated_at()` and `sections()` for read-only
access. `NowSections` exposes each section; `NowSection` exposes availability,
items, total, has_more and error. Item fields are the allowlisted observations.
The outer snapshot and sections have private fields and no deserialization or
public construction API. `serde_json::to_writer` can serialize the returned
snapshot without serializing the underlying stored records or admission handles.

## Identity and final checks

The caller-held `ScopeInput.project` contains an existing `ProjectCapture`. Its
captured `StoreOwner::Project` supplies the canonical project ID. Core
`revalidate_bounded` validates that capture's physical identity;
`matches_scope_bounded` compares its existing binding as data. The function uses
the public `ScopeContext::from_snapshot` with the original optional deadline
and cancellation handle. A service Original has no synthetic absolute deadline;
its cancellation remains live. It checks that `VersionContext::scope()` equals the captured owner root.
These observations do not grant Read authority.

Missing PROJECT input, a non-PROJECT owner, a mismatched version root or binding,
or invalid physical capture causes a whole `ProjectNowError::Scope`. Once the
identity is valid, all three module denials can produce four failed sections with
`forbidden`, empty items and null counts. A denied module is never represented as
an empty successful corpus. `Denied` can also mean an issuer, request, binding or
revocation refusal; the finite code does not claim a permission-only explanation.

Every successful admission is checked against its exact module, the original
snapshot pointer, project owner, brand, owner/runtime roots and complete
`VersionContext`. Each check uses Auth’s `project_and_request` to revalidate
once and borrow both observations for these comparisons. The next corpus read
and final delivery remain separate admission barriers; no observation is cached
across them. Before return, successful admissions are checked again. A late
failure replaces the affected section's data and counts with its finite error;
a late Task failure replaces both Task and session sections. The input PROJECT
capture and original request are checked again after those checks.

An original cancellation or deadline always aborts the whole operation with
`ProjectNowError::Request`, preserving the local IO cause (`Interrupted` or
`TimedOut`). The check precedes classification of SDK failures, including
`AuthError::Request`, which has no cancellation/deadline discriminator. Failure
to obtain a representable observation clock is `ProjectNowError::Clock`.
Whole errors expose a local `Error::source()` and are not serializable wire DTOs.

## Compositor diagnostics

The daemon's shared logging initialization can enable
`RUST_LOG=graphit_daemon::project_now=debug`; its default filter is off.
`read_admitted_now` uses a DEBUG span with `skip_all` and static begin/end
messages for Task, Memory and Knowledge. Each admitted section also marks its
existing admission check with static begin/end messages. No argument, PROJECT
identity, path, URI, body, query, SDK error or counter is recorded.
These markers retain the original barriers, awaits, results and final checks.
An end marker means that boundary returned, including a section-level failure;
it does not prove a successful corpus read or final delivery. An early Original
failure can return before the section end marker. Admission issuance before
entering this compositor and final checks after the Knowledge marker remain
outside these section boundaries. This module installs no subscriber or filter.

## Exact local wire

All fields are present:

```text
{ project_id, generated_at,
  sections: { tasks, sessions, memories, knowledge } }

Section { availability, items, total, has_more, error }
```

| Availability | Items | Total | has_more | Error |
| --- | --- | --- | --- | --- |
| `available` | Selected heads | Real count | Real comparison | null |
| `no_corpus` | [] | 0 | false | null |
| `failed` | [] | null | null | Finite code |

`no_corpus` comes only from the producer's validated absence result. A published
corpus scanned to EOF with no matching heads is `available` with []/0/false/null.
An incomplete Memory corpus or any storage, schema or Auth error is a failure.
A present Task index without valid SDK metadata fails both Task sections as
`unavailable`; it is distinct from a validated absent index.

| Item | Exact fields |
| --- | --- |
| Task | id, session_id, title, owner, lease_expires_at, updated_at |
| Session | id, title, owner, lease_expires_at, updated_at, completed_tasks, total_tasks |
| Memory | id, title, updated_by, updated_at |
| Knowledge | slug, title, updated_at |

Task/session owner and lease are present on included active records. The owner is
the actual persisted Auth attribution; Memory retains its historical updated_by.
Knowledge updated_at can be null and has no author field. IDs retain the actual
SDK canonical serialization.

The local wire excludes USER Memory, bodies, references, audit, embeddings,
progress summaries, next steps, progress metadata, revisions, claim tokens, SDK
generations, paths, branch/corpus names and raw errors. It has no instance,
version or href field. The selected version stays inside the admitted reads.

## Activity, counts and precision

Tasks and sessions are included only for `in_progress` with a real owner and lease
strictly after the same checked UTC observation in milliseconds. Lease equality,
expiration or absence is inactive. Observation never claims, expires or renews.
The Task producer computes session counts from all linked tasks on the same pin,
including cancelled tasks in the denominator and only completed tasks in the
numerator. A valid session with no tasks retains the real 0/0 counts.

Memory supplies current PROJECT heads excluding superseded/tombstoned records.
Knowledge supplies its three-column head projection. For each section, complete
authorized EOF precedes activity filtering, totals, numeric updated time sorting
descending, canonical ID/slug sorting ascending for ties, and selection of 20.
Unknown Knowledge times sort last. `has_more` is total greater than returned length.

Every timestamp is a canonical signed decimal string of UTC epoch nanoseconds,
including generated_at. Task/Memory u64 milliseconds use checked wide arithmetic
without narrowing through i64. Knowledge preserves its actual signed nanoseconds
and unknown null. Sorting uses native numeric/time values, never those strings.
The observation clock supplies generated_at and activity time from one instant.

Every section total and both session counters must be unsigned JSON integers at
most 9,007,199,254,740,991. An actual wider count fails its affected section with
`limit_exceeded` and null counts. Session counters are checked throughout the
active set before top-20 selection. This precision bound adds no scan quota and
does not clamp, skip, truncate or round counts.

## Error isolation

The finite codes are `forbidden`, `unavailable`, `invalid_data`, `limit_exceeded`
and `counts_unavailable`. Typed matches follow the Broker contract:

- Auth Denied/Expired, WrongAdmission and DifferentRequest → forbidden.
- Auth Limit, Memory Limit, Knowledge LimitExceeded/Overflow and count overflow
  → limit_exceeded.
- Memory Invalid/IncompleteCorpus, Task Invalid/MissingSession/IncompleteCorpus,
  Knowledge InvalidField/DuplicateSlug → invalid_data.
- Exactly Task CountsUnavailable → counts_unavailable.
- Other actual typed wrappers → unavailable.

Generic SDK/Arrow errors are not automatically corruption. No error is classified
by its text or serialized using Display, Debug or nested causes. Exhaustive enum
matches require reconciliation when a producer adds a variant.

The Task producer's row-local session error can preserve valid Tasks. Invalid Task
heads make session counts unavailable. Global Task schema, linkage, acquisition,
SDK or Auth failures fail both sections. Independent Memory and Knowledge failures
preserve other sections only while their admissions remain valid.

## Qualification boundary

Tests in `crates/graphit-daemon/src/project_now/tests.rs` distinguish public native
composition from complementary pure projection/precision tests and direct final
boundary tests with real admissions. Temporary installation/project identities,
runtime key owners, AuthStore/RoleIssuer policy and native Record/Data writes are
real SDK objects. They do not use personal profiles, credentials or trust settings.
The fixture's `KiroV2` binding is an admission input, not a running Kiro host.

The native scenarios invoke the public compositor with Runtime and Local callers.
Local authentication follows actual AuthStore publication into a temporary fixture,
including activation and wrong-key rejection. Record/Data writes populate all four
sections before the final revocation test verifies that every item and count is
dropped. A native Task index acquisition failure preserves populated Memory and
Knowledge, and an incomplete Memory corpus preserves absent Task and Knowledge.
The two-PROJECT scenario shares one installation, issuer, Runtime caller and
original request. It confirms four absent corpora on the second PROJECT while the
first is populated, then checks separate IDs, Task/session links, Memory IDs and
Knowledge slugs after real writes in both. The version scenario materializes a
named Task branch with a real write and verifies session denominators of four
there and three on the default pin. Knowledge first publication follows its real
default/named confirmation behavior; the test does not invent an update API.

Task/session row-local failures, top-20 ordering and extreme numeric bounds also
have pure projection tests; those are complementary proofs, not native corruption
or installed-host scenarios.

Before startup correction `tsk-dd0f`, `DaemonAuth::start` allowed only the
installation root, which Core rejects as a PROJECT root. `RoleIssuer` requires an
exact allowed PROJECT root. The startup source now accepts explicit
`trusted_projects: &[ProjectPaths]` from its service owner and propagates their
roots into the same realm and AuthStore. Qualification of that startup and its
consumers belongs to `tsk-dd0f`. These Now tests still use the already-public SDK
issuer setup with explicitly captured PROJECT roots, as the Records fixtures do;
they do not qualify the separate DaemonAuth integration.

The local compositor has no atomic snapshot across the three corpora. Private
corpus guards remain inside each reader until it returns; owned heads do not
retain them. Final admission and PROJECT checks do not promise continuous corpus
confinement or absolute revocation linearization. Synchronous native IO and CPU
work remain cooperative with the original request, without hard preemption.

This Linux library qualification does not prove installed daemon,
App/CLI, Windows/macOS, Kiro V2 lifecycle, V3 preview, Broker HTTP/WS, remote
ordering/reconnect, live presence or terminal signal/TTY behavior. Those remain
independent product journeys. Current runtime bindings target Kiro V3; the historical V2/preview requirements are superseded.

Main validation passed: all 11 Now tests (seven native scenarios and four pure
projection/precision tests), the eight maintained-inventory tests affected by the
startup signature, the executable Now rustdoc example, and daemon Clippy across
all targets with warnings denied. Owned formatting, local links, whitespace and
final newlines were also checked. The earlier 10-test result is preserved in the Task
checkpoint as history; the current 11-test log proves the additional PROJECT
isolation scenario. Historical failed fixture and interface runs
remain in the evidence directory; they are not counted as native qualification.

Portable verification commands from the project root:

```text
cargo test --locked --offline -j1 -p graphit-daemon --lib project_now::tests
cargo test --locked --offline -j1 -p graphit-daemon --doc project_now
cargo clippy --locked --offline -j1 -p graphit-daemon --all-targets -- -D warnings
rustfmt --edition 2024 --check crates/graphit-daemon/src/project_now.rs crates/graphit-daemon/src/project_now/tests.rs
```

Actual warm-cache commands, logs, coverage and hashes are recorded in
`.build/rust-runtime-local-now-20261011/` and `tsk-b98d`. Passing producer tests or
reading this contract does not replace this consumer's execution evidence.

O consumo pelo Source e o transporte para clientes estão documentados em [Now e instâncias no Source Rust](rust_source_now.md).

## Reauthorizing a pending publisher observation

The publisher's internal `reauthorize_project_now` consumer creates new real
Task, Memory and Knowledge admissions for its captured caller, Original and
PROJECT/version before every send. It checks the physical PROJECT identity,
clones the genuine observation, masks denied modules without retaining their
items or counts, and applies the compositor's final admission checks. It does
not read a corpus or change `generated_at`. The retained genuine observation
remains unchanged; permission changes cannot expose a section through an old
admission. Original failure or physical identity failure ends the publication.
This consumer implements NOW-LIVENESS-A in tsk-cab0 while one actual recomposition
remains owned by the publisher.
