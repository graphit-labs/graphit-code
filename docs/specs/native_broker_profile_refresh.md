---
title: Native Broker profile refresh
type: specification
status: implemented-library
updated: 2026-10-19
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrected_by
  - target: {type: task, id: tsk-cdb8}
    relation: implements
  - target: {type: task, id: tsk-d76c}
    relation: implements
  - target: {type: task, id: tsk-aad4}
    relation: derived_from
  - target: {type: task, id: tsk-8c08}
    relation: derived_from
  - target: {type: task, id: tsk-051d}
    relation: derived_from
  - target: {type: task, id: tsk-3586}
    relation: derived_from
  - target: {type: task, id: tsk-e57d}
    relation: derived_from
  - target: {type: task, id: tsk-85d6}
    relation: derived_from
  - target: {type: task, id: tsk-aeda}
    relation: relates_to
  - target: {type: task, id: tsk-a1ba}
    relation: relates_to
---

# Native Broker profile refresh

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

This library consumer updates the captured active Broker profile through
[NativeProfileManager](native_profile_manager.md), the existing
[native OAuth verifier](broker_native_oauth.md), and the same
[private AuthStore](auth_store.md). Root activated its 11 owned source, fixture
and documentation paths byte-for-byte from staging for Task tsk-d76c
specification 4. The activation-only snapshot was SOURCE_ACTIVATED_UNCOMPILED_UNQUALIFIED.
A later attempt compiled production and the test target; its 22 new fixtures
failed during shared setup. After the two-selector fixture correction, all
focused suites and strict Clippy passed. The corresponding fast hook guard and
final review later qualified completed tsk-d76c; the exact affected-source
proof is recorded below. Historical failures remain preserved.
Earlier qualified manager, OAuth, JWT, profile rotation and physical-store
results remain historical prerequisites; they do not prove this integration.
CLI startup, initial login, browser/listener and incoming Broker integration
remain separate integration work.

## Caller contract

The public method exercised by the focused qualification is:

```rust,ignore
pub async fn refresh_broker<'a>(
    &'a self,
    snapshot: ManagedAuthSnapshot<'a>,
    oauth: &'a NativeOAuth<'a>,
    now: i64,
) -> Result<AuthDocumentPublication, BrokerRefreshError>;
```

Create the consumed snapshot with this manager's `begin` and an actual finite
AdminParent or VerifiedRoleGrant granting current `ManageAuthProfiles`. Its
issuer, original request and store determine the operation. The caller supplies
no credential, profile name, claims, CAS expectation, path or replacement
context. The selected active provider and credentials must both be Broker;
the captured refresh token must be present and nonempty. Expired stored access
or ID credentials are data and do not remove this administrative capability.
Local authority requires an active Local source; switching that source to
Broker invalidates it. Local-to-Broker is a denied path, not a Local refresh
permission. Runtime Admin and Runtime Role are the positive fixtures.

`now` is the typed profile timestamp, not an authentication clock. It must be
nonnegative and no earlier than the selected profile's previous update.
Document/profile/token revision overflow is rejected before network effects.
The implementation captures the document, provider, profile and token
expectations before the first await. It preserves name, provider, issuer,
subject, creation timestamp and S3 selection.

Renewable OIDC `OidcTokens.expires_at` is UTC microseconds since the Unix epoch,
as returned by `FinalNativeTokens::expires_at_micros` and stored by refresh.
The access-only Device body deliberately stores Unix seconds from
`FinalDeviceTokens::expires_at`. Native service admission selects the conversion
for that existing body before checking expiry and its existing margin. An
expired or near-expiry renewable profile returns typed `NeedsRefresh` before
JWT/discovery/network work; Device retains its zero-margin `LoginRequired`
behavior. JWT `exp` and OAuth lifetimes remain in seconds. A JWT that fails
verification while its stored profile is still current remains a causal
verification failure, not permission to refresh or to use another credential.

## Remote operation and local publication

Before discovery and again after discovery, form/header construction and the
clock anchor, the mandatory Manager gate checks the original context, exact
service instance, literal endpoint, optional resource and provider revision,
original physical observation, and fresh actual authority. The metadata issuer
must match the stored issuer. The last gate enters the existing bounded POST
execute path directly. No issuer or publication lock spans HTTP awaits.

The OAuth public protocol and Manager share one private `refresh_guarded`
engine for form creation, POST, closed response decoding, rotating refresh,
JWT verification and final tokens. Its required gate cannot be omitted.
The public protocol gate checks its actual service and original request; it
does not assert Manager authority. `matches_request` compares the actual
SnapshotContext through `same_request`, preserving legitimate original
partitions; there is no public context getter or equivalent-context fallback.

AuthStore's private `snapshot_parts` splits a borrowed immutable typed document
from a private rechecker borrowing that snapshot's original physical field.
The rechecker delegates directly to the existing Core revalidation. It does
not reread/parse a new document, copy the old refresh secret, reconstruct a
path proof, or provide authority. The old secret remains borrowed only for the
finite awaited operation. Dropping the future releases that borrowed state
and its owned captured snapshot. All borrows end before the same snapshot is
consumed by prepare.

Verified FinalNativeTokens must retain the same service, request, provider
revision, issuer and subject. Checked getters supply bounded new TokenUpdate
strings. No returned ID/claims pair means Preserve of the prior ID, username,
organization and ordered teams. A verified ID/claims pair means Replace of all
those fields together. Access claims never produce username or permissions.
The existing `rotate_tokens` performs the single mutation under the four
original expectations, incrementing document/profile/token once while leaving
the provider revision unchanged.

After prepare finishes mutation and encoding, the implementation checks final
tokens, freshly authorizes the original proof, then checks tokens/context again
without I/O or allocation immediately before existing Core commit. There is
no postpublication check, reload, rebased expectation or automatic retry.
Fresh logical admission is not an atomic transaction with authority changes
or time across every filesystem write.

## Shared rotation within one issuer

Every `NativeProfileManager` for the same `RoleIssuer` shares one async rotation gate. It retains the original cancellation/deadline while waiting, then rechecks the captured auth document and actual administrative authority before any remote operation. A competing winner makes an old snapshot fail with a typed `NotSent` conflict; the manager never rebases the caller's CAS values or resends its old rotating token. Records and Now/Inventory may validate a fresh service for the same account and newly published token revision after that conflict.

The issuer retains one fixed-size credential-epoch digest and a closed uncertain error. The digest covers length-delimited provider/profile names, issuer, subject, provider revision and token revision; document or profile metadata revision changes alone do not permit replay. A dropped refresh is conservatively `MayHaveApplied`; a joined `NotSent` result clears that attempt, and a committed new token permits later use normally. This coordinates owners sharing one issuer, without claiming cross-process singleflight. Stage recovery returns `RefreshBusy` while that gate is held, so it cannot remove an active refresh's publication stage.

## Effects, errors and cleanup

`BrokerRefreshErrorKind` contains Manager, OAuth, ServiceMismatch or
IdentityMismatch categories. Errors expose `kind()` and `remote_effect()`;
Display is static and contains no token, URL, body or vendor description.

| Result | Meaning and caller action |
| --- | --- |
| Error with NotSent | The token POST was not invoked, or native bounded-client pre-send busy admission proved no send. Discovery GETs may have occurred. Resolve the typed admission failure. |
| Error with MayHaveApplied | POST execution was entered or local admission failed after verified remote rotation. The old refresh token must not be retried automatically. Local publication has not been fabricated. |
| Ok publication | Remote refresh returned verified tokens; inspect the complete Core publication effect, failure and cleanup fields to establish the local result. |
| PendingCleanup | Retain the actual opaque Core proof. A separately authorized cleanup request can consume it through existing Manager cleanup; it does not renew or retry the cancelled refresh. |

Applied, Unchanged, NotApplied and Unknown, failure/cleanup categories and
PendingCleanup pass through unchanged. Even a received HTTP error status or a
lost response does not prove rollback of remote rotation. Pre-Core local
denials preserve the original destination or the competing winner. The
prepared real cancellation fixture reaches Core's before-replace boundary and
expects NotApplied with Cancelled publication/cleanup and actual retained
scratch. Core's already qualified Unknown/late-Applied forwarding contract is
not presented as a newly executed Auth private seam.

## Resource and privacy boundaries

The manager holds the actual captured snapshot across HTTP, prepare and publication. Drop or error closes the owned snapshot through RAII. Physical source and snapshot rechecks retain the original cancellation and deadline without a snapshot pool or I/O accounting.

The configured NativeFlowLimits, VerifierLimits and HTTP request/response/
header/connection bounds remain independent. New owned copies are admitted
against actual AuthStore ProfileLimits before growth: scalar bytes, checked
aggregate P bytes, collection entry count and fallible reservations. The
snapshot name is counted before await. Profile encoding applies its existing
complete-document cap. The shared gate adds only one credential-epoch digest and closed error per issuer; it adds no background job, global memory ceiling, SDK, deadline or transport. Secrets/form/encoded buffers
retain existing Secret/Zeroizing ownership; allocator or vendor erasure is
not claimed. Fixtures contain synthetic isolated credentials only.

## Focused qualification and source ownership

The new Tokio fixture module contains 22 named tests, with positive controls
and matrices for account/ABA and all four CAS changes before/after POST,
Role revocation/run closure, Admin expiry/cancellation, original real deadline,
literal resource/binding, metadata/JWT identity, lost/status/malformed/capped
responses, owned snapshot Drop, scalar/collection/document bounds, and the
post-encoding token/authority and actual Core cleanup boundaries. Six new
compile-fail examples reject ordinary callers, supplied credentials, supplied
CAS/context and public access to gate/rechecker types. All 22 new Tokio cases
and the six new compile-fail examples passed in the third focused attempt.
No existing test is retired or ignored.

Source owners are `profile_manager/broker_refresh.rs`, its child fixture module, narrow declarations in profile_manager.rs and profile_manager/tests.rs, mandatory gate/shared execute in broker/oauth.rs, and the private split/limits helpers in store.rs. This consumer does not change Broker/profile/JWT/Core producer APIs or manifests.

The third exclusive window ran locked Auth tests filtered to
`issuer::tests::profile_manager::` (46 PASS), `broker::oauth::` (17 PASS), all
Auth rustdoc tests (93 PASS, including six new compile-fail examples), and
strict all-targets Clippy (PASS), with two jobs, no incremental compilation,
debug information disabled and two test threads. All 22 new refresh cases
passed. At that attempt, the matching module fast hook guard and independent
final review were pending; their later proof below qualified tsk-d76c for that
source. Failure
history from the first two attempts remains below and in the evidence ledger.

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

### Current affected guard scope

The matching `graphit-auth/profile_manager` Library entry selects
`issuer::tests::profile_manager::` in
`crates/graphit-auth/src/profile_manager/tests.rs`, including the qualified
`tests/broker_refresh.rs` fixture. Root qualified this actual Git pre-commit
phase on Linux: 46 tests passed and only the injected
`issuer::tests::profile_manager::broken_fast_contract` sentinel failed.
Cargo returned 101 and Git returned 1. The restored 35-phase fast gate passed.

The independently reviewed individual proof is in
`.build/rust-core-birth-anchor-20261009/root-qualification/seven-individual-guard-proofs-1791622887400100385/receipt.json`,
SHA256 `446e707b9905496e82e515f8fcd9faa93478f579d75f9b885e743a0301e662cc`,
ordinal 7. The original eight-guard operation remains FAULT because a separate
SourceReader test also failed; the receipt qualifies seven guards individually.
All 922 copied input files remained unchanged, private scratch was empty,
the direct child was awaited and private TMP was removed. None of these facts
certifies nested SDK quiescence.

The six refresh sources remain unchanged from their focused receipts.
Source/documentation review preserves the refresh original-context, physical
identity and commit rules. The old T8 whole `cargo xtask check` obligation for
this partial library remains superseded history; its affected replacement uses
this matching guard and scoped review. It does not waive the final full gate.

## Activated initial login: common gate, distinct replacement rules

Root activated the [initial-login consumer](native_broker_profile_login.md)
under tsk-a1ba from its privately qualified overlay. Its current affected login
Git guard is qualified by the proof below. The source replaces the private refresh-only gate names with the
used common `NativeFlowGate`/`GuardedNativeError`/`ProtocolNativeGate`, and
`ManagedRefreshGate` implements that internal common trait. The real original,
physical snapshot, authority/provider binding and final tokens-authority-tokens
checks described above remain required. There is no public gate, optional
admission, authority callback or second refresh engine.

Initial login replaces verified profile credentials and may select a different
provider/subject. If its exchange provides no refresh token, the replacement
stores `None` and discards any prior refresh secret. `refresh_broker` keeps its
separate stable-subject and rotation rules: a fresh different refresh token is
mandatory; only an absent new ID/claims pair yields `ProfileIdentityUpdate::Preserve`
for the previous ID, username, organization and ordered teams. A returned
verified ID/claims pair replaces those fields together.

Root activated the privately qualified initial-login source under tsk-a1ba.
Its current affected login Git guard is qualified by the proof below. The private affected commands passed
20 new login cases, 17 existing OAuth cases, 46 existing manager cases,
36 profile parent cases and 15 focused Rustdoc examples (14 compile-fails plus
one positive compiled example). A fixture-only lexical guard scope repair then
passed its one affected case, strict Auth all-targets Clippy and scoped format.
The full receipt preserves its earlier Clippy fault; the successful tail does
not relabel that historical run. No CLI, native Windows/macOS or product
performance qualification follows from these command results.

The historical tsk-d76c guard above qualifies its actual earlier source, not
the activated renamed common-gate/login source. The new affected private run includes
all46 existing manager cases, preserving the22 refresh fixtures. The current
activated source additionally passed full40 baseline/restored and its affected
login sentinel rejection, as recorded below. Existing
refresh qualification is not a proof of CLI initial login or another platform.

## Historical activated-source Git guard evidence

The [initial-login current guard ledger](native_broker_profile_login.md#historical-activated-source-git-guard-evidence)
records Root receipt211de23: actual broker_login20 normal passes plus the sole
injected sentinel failure (Cargo101/configured Git1), baseline/restored full40,
unchanged source/driver/locks and retained historical faults. The affected guard
selection preserves all existing manager/common-gate/refresh requirements. It
establishes no CLI, native three-OS, nested SDK quiescence or performance proof.
