---
title: Native profile manager
type: specification
status: implemented-library
references:
  - target: {type: task, id: tsk-cdb8}
    relation: implements
  - target: {type: task, id: tsk-aad4, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-e849}
    relation: derived_from
  - target: {type: task, id: tsk-a1ba}
    relation: relates_to
---

# Native profile manager

## Main qualification of owned snapshots

Under `tsk-cdb8`, the manager keeps its original issuer, authority and request
while each observation owns its actual authentication snapshot. The artificial
snapshot pool, document-maxima reservation and lease are removed. Broker login
and refresh retain their original physical checks, remote effects and publication
semantics. Main passed 76 focused ProfileManager and descendant Broker tests,
133 Auth doctests, strict all-target Auth Clippy and formatting on Linux.
The two retired cases tested only pool capacity or injected atomic accounting;
functional simultaneous-observation and original-cancellation cases replace them.
The first compile failure and corrected fixture expectation are retained in
`.build/rust-auth-profile-manager-owned-snapshots-20261011`. The earlier
qualifications below describe their historical source and guard selections.

`graphit_auth::profile_manager::NativeProfileManager` administers the real
[private authentication store](auth_store.md) retained by a `RoleIssuer`.
The original library slice is qualified under tsk-aad4. The new
[Broker refresh consumer](native_broker_profile_refresh.md) has been activated
under completed tsk-d76c with focused tests, rustdoc, strict Clippy, the matching
Git guard and final review qualified for that refresh source. The
[initial login library](native_broker_profile_login.md) source has been activated
under tsk-a1ba from the privately qualified overlay; its current affected login Git guard
is qualified below. CLI initial
offline authority, initial browser OAuth, application startup and listeners
remain separate required integrations; the running CLI has not been replaced
here.

## Admission and captured views

Construct the manager with `NativeProfileManager::new(&RoleIssuer)`. This is infallible and performs no I/O. It never accepts an
independently captured store or a path. `begin` accepts only
`ProfileManagerAuthority::Admin(&AdminParent)` or `::Role(&VerifiedRoleGrant)`.
Both proofs must belong to that same issuer and grant current
`ManageAuthProfiles`. An ordinary authenticated caller is insufficient.
Finite Admin permission comes from its origin's immutable administrative policy;
Role permission comes from the live credential and ancestry.

The original request comes from the proof. Callers cannot replace its cancellation or deadline. Wrong issuer is rejected before I/O. Fresh authorization performs physical source checks even if permission or a live session is subsequently denied.

The manager loads once through AuthStore, then verifies authority again after parsing. Each observation owns its actual snapshot and borrows its manager and original authority. There is no snapshot pool, slot reservation or charge based on document maxima. An absent file yields
an empty view; reading never creates defaults, keys, profiles or sessions.

`ManagedAuthSnapshot` is opaque and consumed by mutation. It cannot be cloned,
formatted or serialized, and exposes no raw document, ProfileRecord,
credentials, Secret or physical observation. Provider/profile iterators return
borrowed redacted views: names, nonsecret provider configuration, declared
identity, revision numbers and timestamps. These are captured data, not new
authority. Views perform no hidden I/O and cannot outlive the snapshot. Copies
that callers explicitly create belong to their own allocation domain.

## Typed operations and publication

Provider add/update/remove, explicit cascade, default Local provider creation,
activation and logout delegate to the existing
[provider/profile data rules](auth_profiles.md). Callers supply document,
provider and profile expectations as appropriate. Missing, conflicting, stale,
invalid or overflowing data fails before publication. A competing file write
is checked again under Core's publication lock. No operation reloads or rebases
its consumed snapshot after conflict.

`install_local` accepts `LocalProfileInput`, `ProfileExpectation` and an explicit
timestamp. It constructs only Local credentials. All supplied credentials
replace the previous credentials; omitted keys are removed. No MCP key is
generated, and keyless stored data does not authenticate a caller. Named and
anonymous data obey existing service/S3 rules. Missing provider fails without
creating a default. Broker credentials cannot enter this API.

Local replacement and activation execute on the same observation and publish
once. Activation uses the document revision produced by replacement. Failure
of either operation discards the owned document and preserves the prior file.

AuthStore's private prepare first validates the original observation, executes
the pure typed mutation and encodes a bounded Zeroizing buffer. It drops the
typed document and Secret fields. Only after all mutation and encoding does the
manager perform its final actual permission/source/session check. It then
enters Core publication directly with the original observation and buffer.
No publication lock or issuer State lock spans mutation or encoding.

This is fresh authorization at entry to the logical publication operation.
There is no filesystem transaction with issuer authority or permission check
before every internal write/rename. Core keeps its physical CAS, request
checks, effects and cleanup proofs. Applied logout/provider edits may invalidate
their own Local authority: Applied remains truthful, and the next admission
requires a fresh valid proof.

## Recovery and retained cleanup

`recover_stage` freshly authorizes immediately before Core's managed-stage
discard. It never parses or promotes a stage. `cleanup_pending` accepts the
actual opaque Core PendingCleanup and fresh authority. Admitted calls return
the complete AuthStageRecovery, including any remaining pending proof.
Rejected admission returns opaque `ProfileCleanupDenied`: `error()` supplies a
static typed category and `into_pending_cleanup()` returns that same proof for
a fresh request. No proof is cloned, reconstructed or discarded by denial.

Publication preserves Applied, NotApplied, Unchanged, Unknown, actual failures
and pending cleanup. Pre-Core authorization failure has zero publication
effects. There is no postpublication check that could recast Applied.

## Ownership and validation

Snapshots own parsed data and the physical observation until consumed or dropped. The manager retains only its issuer reference. Multiple observations can coexist; real authority contention, expiry, cancellation and physical conflicts still fail with their existing typed causes. Mutation consumes the original snapshot and preserves Core publication/recovery outcomes.

The configured document/decoder bounds and AuthStore ProfileLimits continue to protect actual input/output and copied names. Broker HTTP, JWT and native pending-operation limits remain their own functional contracts. Dropping an owned snapshot or future closes its resources through RAII; no logical byte accounting is refunded or used to manufacture authority. Recovery and cleanup use their actual proofs and original request.

Qualification uses affected ProfileManager/Broker library tests, privacy rustdocs, strict Auth Clippy, formatting, diff checks and code/document consistency. Native Windows/macOS and complete application integration remain separate. Historical qualification below describes the source at that time; quota-only tests are retired by tsk-cdb8.

## Current qualification evidence

The six pending-cleanup fixtures cover a previously present document and initial
absence, each with wrong-issuer, expired finite-parent and revoked-Role cleanup
denials. A private used `cfg(test)` AuthStore seam arms Core's default-off
`auth-test-support` cancellation guard against the same physical store and
original request. The real manager prepare/final-authority/publication path
returns NotApplied with Cancelled publication and cleanup failures and an actual
PendingCleanup. The scratch exists, while the previous bytes or absence remain.
Dropping the guard only disarms it and does not remove scratch or refund I/O.

Denied cleanup returns that same opaque proof. A separately authenticated fresh
request to the original issuer then performs actual Applied cleanup, preserving
the destination. The cancelled mutation is never renewed or retried. Auth does
not observe Core's private unit-test native-replace counter; the qualified Core
before-replace boundary and Auth's actual cancellation result establish the
publication failure here.

Core's support feature is enabled only by Auth's dev-dependency. Normal library
checks and the normal/build feature tree exclude it; development/test/all-target
builds can unify that feature under Cargo's workspace resolver. This adds no
production callback or store getter. The original first-slice evidence and
source freeze remain immutable history. Task tsk-aad4 completed its independent
final review and Root qualification. That completed library slice does not
qualify the activated Broker refresh consumer or complete the product migration.

## Broker refresh integration: focused checks passed

`refresh_broker` consumes this manager's selected snapshot under the original
Admin/Role request. It captures all four CAS values before HTTP, requires a
mandatory original-observation/authority/provider-binding gate before discovery
and immediately before POST, and shares the existing OAuth refresh engine.
All manager instances on one issuer now serialize rotation through its shared gate and recheck the original snapshot after acquiring it. Stale losers fail `NotSent`; exact-token uncertain/drop effects are retained without replay, while a new token revision remains usable. Stage recovery reports `RefreshBusy` during a rotation. This adds no cross-process singleflight claim. After bounded conversion and encoding it checks final tokens, actual authority,
then final tokens/context immediately before Core commit. Remote uncertainty
and full local effects/cleanup are separate. Its [qualified contract](native_broker_profile_refresh.md)
records the focused suites, matching Git guard and final review for completed
tsk-d76c. Those results do not qualify the new initial-login activation.
Earlier qualification above is history.

### First compilation failure, preserved historical snapshot

The first focused qualification stopped during test compilation with nine
fixture import errors (exit 101), before any test executed. OAuth, rustdoc and
Clippy phases were not started. The raw failure and source freeze remain in
`.build/rust-auth-broker-refresh-2026-10-09/qualification-1791582498178608520/`.
The correction only imports the three actual profile types and aliases the
existing HTTP `Limits` type in the new fixture; it changes no assertion or
production API. At the end of that first attempt, corrected source was uncompiled and
unqualified, with all 15 product checks pending and all new cases unrun.
The subsequent attempt and its current limits are recorded below.

### Second attempt: preserved historical setup failure

The second focused attempt compiled the library/test target and ran 46 cases:
24 existing cases passed and all 22 new refresh fixtures failed at the shared
seed operation before exercising refresh. Their Broker provider used Disabled
embedding/rerank selectors, whereas the existing profile validator requires
both Broker selectors. The proposed correction changes only those two fixture
selectors; it does not change validation, assertions, API or the original request.
The raw failure and executed source freeze are preserved in
`.build/rust-auth-broker-refresh-2026-10-09/qualification-1791582807724366190/`.
At the end of that second attempt production had compiled, but the refresh
contract was unqualified and the corrected fixture awaited execution. OAuth,
rustdoc and Clippy were UNRUN, with six new compile-fail examples and 15 product
checks pending. Those setup failures established no refresh-contract PASS.

## Initial Broker login: source activated, current affected Git guard qualified

The [initial login contract](native_broker_profile_login.md) adds
`BrokerLoginSelection`, opaque `ManagedBrokerLogin`, `BrokerLoginError` and
`BrokerLoginErrorKind`. Call `begin_broker_login` with this manager's actual
`ManagedAuthSnapshot`, captured `NativeOAuth`, explicit profile/provider/S3
selection and redirect. The pending's `authorization_url` freshly checks the
same physical snapshot, service, original request and authority. Pass that
exact pending, callback and stored timestamp to `finish_broker_login`; it
accepts no caller-made token, claim, CAS expectation or replacement context.

Only verified native identity and credentials create or replace the selected
profile, then activate it in one prepare/commit publication. Replacement may
change provider/subject, preserves only `created_at`, and stores an absent new
refresh token as `None`. It never inherits an old refresh secret. By contrast,
`refresh_broker` requires rotation and preserves prior ID/profile fields only
when the verified refresh returns no new ID/claims pair.

The activated source uses one mandatory `NativeFlowGate` with the same
physical/authority/original checks in the exchange and refresh engines. Scalar,
count and checked copied-byte admission precedes owned reservations; final
tokens-authority-tokens checks precede commit. Real remote uncertainty and the
exact Core publication/opaque cleanup proof remain separate, with no retry or
postcommit reclassification. This extends the consumer; it does not expose a
new source of authority or change the original manager/request binding.

Root activated the privately qualified initial-login source under tsk-a1ba.
Its current affected login Git guard is qualified by the proof below. The private affected commands passed
20 new login cases, 17 existing OAuth cases, 46 existing manager cases,
36 profile parent cases and 15 focused Rustdoc examples (14 compile-fails plus
one positive compiled example). A fixture-only lexical guard scope repair then
passed its one affected case, strict Auth all-targets Clippy and scoped format.
The full receipt preserves its earlier Clippy fault; the successful tail does
not relabel that historical run. No CLI, native Windows/macOS or product
performance qualification follows from these command results.

## Historical activated-source Git guard evidence

The [initial-login current guard ledger](native_broker_profile_login.md#historical-activated-source-git-guard-evidence)
records Root receipt211de23: actual broker_login20 normal passes plus the sole
injected sentinel failure (Cargo101/configured Git1), baseline/restored full40,
unchanged source/driver/locks and retained historical faults. The affected guard
selection preserves all existing manager/common-gate/refresh requirements. It
establishes no CLI, native three-OS, nested SDK quiescence or performance proof.

## Interactive loopback library integration

The [interactive login library](native_broker_interactive_login.md) accepts explicit
InteractiveLoginConfig and five input/attempt limits. Its consuming attempt and
received result use this manager's actual managed snapshot and captured
NativeOAuth. Receive closes the sockets; publish delegates one managed finish.
It uses the existing authority checks and publication engine.
