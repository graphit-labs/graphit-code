---
title: Native Broker profile login
type: specification
status: implemented-library
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-5fb0}
    relation: implements
  - target: {type: task, id: tsk-cdb8}
    relation: implements
  - target: {type: task, id: tsk-a1ba}
    relation: implements
  - target: {type: task, id: tsk-3e54}
    relation: derived_from
  - target: {type: task, id: tsk-d76c}
    relation: uses
  - target: {type: task, id: tsk-aad4}
    relation: uses
  - target: {type: task, id: tsk-8c08}
    relation: uses
---

# Native Broker profile login

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


This library consumer starts one verified native Broker login and publishes the
selected profile with its activation in one physical AuthStore publication.
Root activated the tsk-a1ba source from the privately qualified overlay.
The executed qualification belongs to that private overlay:
20 login tests, 17 existing OAuth tests, 46 existing ProfileManager tests,
36 profile parent tests and 15 focused Rustdoc examples passed. The existing
profile writer parent also exercised its three bounded subprocess cases.
Strict Auth all-targets Clippy and scoped formatting passed after a fixture-only
lexical mutex scope repair. The affected login Git guard for the activated source
is qualified by the current proof below. CLI startup, browser/listener, incoming Broker integration, native
Windows/macOS and product performance remain separate work.

Use an actual [NativeProfileManager](native_profile_manager.md) over its issuer's
[private AuthStore](auth_store.md), a current AdminParent or VerifiedRoleGrant
with ManageAuthProfiles, and the [native OAuth service](broker_native_oauth.md)
matching the selected Broker provider's exact endpoint, resource and revision.
Initial login may choose a new subject and provider. Subsequent
[profile refresh](native_broker_profile_refresh.md) retains its existing identity
and rotation contract.

## API and authority

BrokerLoginSelection borrows required profile and provider names and takes an
explicit s3_disabled flag. It contains data, never an authority or token grant.

A selected Broker profile with `s3_disabled` rejects native HubRead and HubWrite
credentials before remote validation or storage requests. Embedding and inventory
retain their own authorization; this setting grants no storage authority.
NativeProfileManager::begin_broker_login consumes that manager's
ManagedAuthSnapshot plus the actual NativeOAuth, selection and redirect.
ManagedBrokerLogin::authorization_url requires a mutable pending and returns a
borrowed sensitive URL only after fresh original physical/authority checks.
NativeProfileManager::finish_broker_login consumes that exact opaque pending
with callback and stored timestamp; it accepts no caller-supplied tokens,
claims, expectation, service or replacement request.

Pending values cannot be cloned, logged, serialized, constructed from data or
consumed twice. The privacy compile-fail reads the private pending field through
a well-typed borrowed ManagedBrokerLogin, so the oracle isolates field privacy.
Their native secrets and pending permit precede the captured snapshot in drop order. Finish retains the owned AuthSnapshot directly, with no manager lease wrapper. Do not log
authorization URLs, callbacks, tokens, claims or profile secrets.

Begin checks manager identity, fresh authority and the original request before
I/O. Scalar name limits precede the existing name validator and provider lookup.
Missing/non-Broker provider, service literal mismatch, revision overflow or
insufficient insertion slots denies before discovery. Discovery may perform
GETs; successful begin then rechecks the same physical snapshot and authority
before exposing the pending. A fresh request with identical values cannot
replace the original operation or its cancellation/deadline.

## Verified exchange and atomic local state

The existing OAuth PKCE/state/nonce engine remains the single protocol producer.
Private NativeFlowGate, GuardedNativeError and ProtocolNativeGate are used by
both initial exchange and refresh; there are no compatibility aliases.
exchange_guarded performs callback/form preflight and calls the mandatory
manager gate immediately before token POST. The gate checks the same manager,
service/provider binding, physical snapshot, original request and fresh
administrative authority. No network operation runs under an issuer/state/file
lock. Native initial exchange requires verified access and ID tokens and makes
no UserInfo request.

Only FinalNativeTokens and VerifiedNativeLogin profile claims supply new
identity and credentials. A replacement may deliberately change provider and
subject; old claims, access/ID/refresh secrets and S3 selection are replaced.
Only created_at is preserved by the existing replacement operation. A missing
new refresh token remains None. now is validated stored time, not token lifetime;
negative time or time before the previous updated_at denies before POST.

The physical snapshot binds the complete original document, including provider,
profile, token revision and active-selection ABA. ProfileExpectation remains its
existing three-field data selector, constructed internally; it grants nothing.
Preflight checks document revision+2, profile revision+1 plus activation change,
and token revision+1 before discovery. New-profile insertion obeys ProfileLimits.

After verification, all scalar lengths, checked total copied bytes and team
backing/count are admitted against the real ProfileLimits before the first
owned copy. Name/provider/issuer/subject/tokens/claims and the activation-name copy
are bounded by actual data limits and allocation is fallible. The manager snapshot is owned without a reservation pool; OAuth/HTTP/JWT and profile data limits remain functional input/operation bounds.

One store.prepare mutates the original document: replace_profile_data followed
by activate using the resulting document revision. These are two logical
document transitions, encoded and published once. Replacing an already active
target increments its profile revision once; activating a different target adds
one more profile revision. Tokens remain actually retained through preparation
and the final tokens -> authority -> tokens checks before store.commit.

## Effects and explicit cleanup

BrokerLoginError has closed static categories and RemoteEffect. Before POST,
gate/selector/CAS failure is NotSent. Native transport errors retain their actual
producer effect: Busy is NotSent; sent or ambiguous requests are MayHaveApplied.
After POST, token/identity/mapping/prepare/final-authority failures remain
MayHaveApplied and cannot publish locally. No code exchange is retried, no
snapshot is reloaded/adopted, and no remote rollback is promised.

Once Core commit runs, return its exact AuthDocumentPublication, including
Applied/NotApplied/Unknown, failure, cleanup and the actual opaque PendingCleanup.
No postcommit authority check reclassifies an applied effect. Use the existing
manager cleanup_pending with fresh real authority for explicit cleanup; denied
cleanup returns the same proof. It never renews/replays the original exchange.

## Verification scope

Private fixtures use the actual AuthStore/issuer/Admin/Role, bounded loopback
TCP server and signed Ed25519 access/ID tokens with the nonce from the actual
opaque authorization URL. They cover absent/replaced profiles and optional
refresh, deliberate subject/provider changes, physical CAS/active ABA before URL,
during discovery and after POST, revocation/cancellation, native pending leases,
real Core cancelled publication/cleanup, scalar/count/slot/revision/encoded
boundaries and canary diagnostics. Server shutdown explicitly stops acceptance
and aborts/awaits its owned connection tasks. These assertions passed in the private qualification described below; the
affected login guard on the activated Root source is qualified below.

Affected commands use --locked, jobs2, incremental0, dev/test debug0 and
testthreads2: OAuth and manager library filters, profile filter for validator
visibility, Auth Rustdoc, strict Auth all-targets Clippy and scoped fmt.
The configured hook rejected the affected broker_login sentinel with Cargo101
and Git1; baseline and restored full40 passed with Source fences, including
profile_manager coverage, as recorded below.
Earlier refresh/matching receipts do not cover this new code.

Source targets: profile_manager/broker_login.rs, broker/oauth.rs and the minimal
manager/gate/validator/test wiring. Root owns activation, shared exports,
Main/manifests/Lock and coordinated links in existing maintained pages. Independent private Stage review and the actual affected command receipts
provide separate evidence. Native platform, CLI and product performance
qualification remain pending.

## Private qualification evidence

The private full affected run is recorded under
.build/rust-auth-initial-login/implementation-20261010-a1ba-r2/qualification-run-1791628908288152896/
(final receipt SHA127e1d368ccf3c0bd0a283199d85e873514955f2c40eae01a362d24938d0945f).
It passed the actual listed20/17/46/36 suites and focused15 Rustdoc examples
(14 negative privacy examples plus one positive compiled example), then stopped
at strict Clippy on a mutex guard scope in one new test. Production was unchanged.
Only that fixture block was repaired; the actual affected initial test, strict
Clippy and seven-file formatting passed in tail-qualification-1791629114523588879/
(final receipt SHA5035b8ddddde438fb86887bdcc70de70f21720bbaa391ce2871f4fdfa2d81e67).
The remaining passed suites were preserved without repeating unaffected tests.

Earlier compile-import and fixture-oracle failures remain in their raw receipts
and retained TMP directories. Functional assertions preserve the native claim
order; no production sorting or limit increases were introduced. The aggregate
copy boundary uses64 teams within existing collection64, with actual encoded
document slack below copy -1, scalar-fit controls, successful publication at the
exact limit and unchanged persisted Auth bytes on denial. The synthetic
allocation-attempt counter has been removed; the case verifies the actual
publication boundary and does not measure allocator activity. Each actual
qualification observed unchanged Root compiled-input
projection and full private WORK inventory. Successful empty TMP removal used
only rmdir; fault TMP was retained. This establishes observed command results,
not nested SDK/process-family quiescence, native three-OS or performance proof.

## Historical activated-source Git guard evidence

Root executed the configured pre-commit against the activated source in an isolated
copy under the unchanged Main40 catalogue. Its initial and restored full40 gates
passed, including the existing profile_manager group. The affected
graphit-auth/broker_login group passed20 normal tests and failed only the injected
broken_fast_contract sentinel: Cargo returned101 and the configured Git hook
returned1. The Git Trace2 session is20261010T123358.572780Z-H690e6844-P00190ce4.

The five-group terminal receipt (`../../.build/rust-auth-initial-login/recovery-worker-20261010/five-current-guards-1791635166341582506/receipt.json`; historical local artifact unavailable)
has SHA211de23e81c63ae93a08522139a336d5885f9578c97e9870c68b4ad33a9c3db3;
command.log SHA83c30b07cc6345e659d936fd3f6551cdaa2d09a886e9934aca885db45d1d48aa;
git-trace.jsonl SHA9038bbd303233870d799cece23db0e6ddf444e7a2b68a9c5e66f9a9e96385a31.
Main40 SHA9968c43440f23a9e83eaa4006c4776c81a4161f75d580abcf40e5f7ccad17713,
public driver SHA8242a0679d506a31a3c888ff2cd90615da9b84e91423450292a24865ca34c017
and Cargo.lock SHAee8c2367eb151b8281e953165960e9b6b6dae220dd906f9ab553c3f13edbdcf5
were fenced. The copied948 files/150 directories and45 query YAML files stayed
unchanged; the one directly started child was awaited and the empty owned TMP
was removed. This proves hook rejection and restoration, not a commit or push,
nested SDK quiescence, product performance or native Windows/macOS operation.

The old profile_manager sentinel check was superseded only to select this affected
login group; its original E0433 FAILED audit remains. All existing functional,
authority, shared NativeFlowGate and refresh requirements remain required. The
119 private functional passes and15 Rustdoc examples remain historical evidence;
they are not119 additional current reruns. Earlier failed qualification receipts,
including the earlier Clippy fault, remain unchanged. CLI startup, automatic
browser/listener, incoming Broker, native three-OS and performance remain separate.

## Interactive loopback library integration

The [interactive login library](native_broker_interactive_login.md) composes this
managed begin/finish flow with one Tokio loopback listener and borrowed httparse
HTTP parsing. It retains the same snapshot, service, NativeOAuth and original
request, fresh authority checks, one token POST and one Core publication. Its
12 functional cases and the existing 20 login regressions validate this consumer.
Browser opening and full CLI command wiring belong to their respective callers.

## Device access-only profile

The [Native Broker Device login contract](native_broker_device_login.md) defines the separate `--device` journey, typed `broker_device` profile, optional metadata, actual-expiry service use and local logout. Its implementation is under qualification under tsk-5fb0. Existing Local username and Code ID/refresh requirements apply to their complete `ProfileData` body; `ProfileRecord::data()` returns that optional body, while `name()`/`provider()` select either body. No Device username, ID token or refresh token is fabricated.
