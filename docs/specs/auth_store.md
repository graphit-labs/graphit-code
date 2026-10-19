---
references:
  - target: {type: task, id: tsk-1750}
    relation: implements
  - target: {type: task, id: tsk-7a35}
    relation: specified_by
  - target: {type: task, id: tsk-aeda}
    relation: relates_to
---

# Typed private authentication storage

The Rust AuthStore connects the typed profile document to the actual Core
installation authentication store. It accepts a captured Core store, not a
path, filename, owner declaration, or serialized observation. It carries data;
it does not authenticate a caller or issue a role or permission.

The implementation is in `crates/graphit-auth/src/store.rs`; its fixtures are in
`store/tests.rs`. The schema and its pure revision operations are documented in
`docs/specs/auth_profiles.md`. Core owns private file capture, publication and
stage recovery, documented in `docs/specs/private_auth_document.md`; installation
directory creation is documented in `docs/specs/private_installation_bootstrap.md`.

Core's `AuthDocumentStore::recheck_installation(real_scope, context)` now provides
a metadata-only binding to the same captured installation owner (including its
selected InstallationId), Brand, canonical root and actual private root FD.
Own and candidate realm checks use bounded locks and preserve the original
deadline/cancellation, without body I/O or a new root capture. The selected ID
comparison does not prove independently persisted identity.json or caller authority.
Native access/path limits and each realm's bounded inspection are separate domains.

AuthStore's load/mutation/publication contract below is unchanged. The bridge does
not validate its content snapshot or permit a fresh request to reuse that snapshot.
The current RoleIssuer library consumer calls the Core binding and still verifies
the original AuthStore snapshot, key binding and explicit source permissions.
The NativeProfileManager consumer is described below. Application startup and
listener integration remain subsequent work. AuthStore itself supplies no grant.

The [Project record admission](project_record_admission.md) consumer derives
this same private AuthStore from its selected RoleIssuer and rechecks its captured
installation with the original request. Callers cannot inject a replacement store.
The borrowed project admissions also revalidate the original physical role grant
and exact operation permission; the metadata bridge itself grants no record or
USER authority. Task tsk-1f4a completed Linux library qualification, including
functional tests, doctests and strict Clippy; its matching actual Git guard was
qualified in completed tsk-c775. This historical PROJECT admission proof does
not qualify the newly activated Broker refresh Store helpers described below,
nor supply USER authority or complete Records/Data and product integration.

## Reading and original request identity

`AuthStore::new` validates positive `ProfileLimits` using a temporary empty
`AuthDocument`. It creates no file, provider, profile, key, or login.
`load` reads through Core with the original `HubRequestContext::snapshot()`.
For a present file it performs one bounded typed JSON parse. For an absent
file it returns a fresh empty document with revision 1. It never substitutes
an empty document for malformed, stale, inaccessible or nonprivate data.

After decoding, Core revalidates the original physical observation. This
final check runs even when the typed decoder returned an error. A final
physical or request error takes precedence over a schema error; no typed
result escapes before this check. A present document of N bytes therefore
requires two physical passes, reserving 2 × (N + 1) read bytes and reading
2 × N bytes. Both passes use the original shared request budget, cancellation
and deadline. There is one JSON parse, no second parse or generic JSON DOM.

`AuthSnapshot` owns the opaque Core observation and typed document. It exposes
only a borrowed `document()` and `was_present()`. It implements neither Clone,
Debug nor serialization/deserialization; consumers cannot extract or rebuild
the physical observation through this API. Fresh requests, different store
captures and replaced physical files cannot reuse it.

## Mutating and publishing

`mutate` consumes the snapshot and revalidates it before invoking the closure.
The closure applies pure typed data operations; the schema enforces document,
provider, profile and token revision comparisons, positive own revisions no
greater than the document revision, and whole-document prospective capacity
before each mutation. New provider/profile/token birth revisions use the
prepared new document revision. An actual active-profile change increments
only the newly selected profile revision; token generations, timestamps and
identity fields remain unchanged. These data bindings do not supply authority. Closure errors discard the
owned data and do not publish. `publish` also consumes and validates its
snapshot before encoding.

Remote OAuth or other network producers must complete before calling these
operations. No publication lock is held while observing data, producing remote
results, running the mutation closure or encoding. Core acquires its lock only
for the final publication and repeats its physical comparison under that lock.
The fixtures deliberately interleave a second publication after the first
precheck to exercise this final comparison.

The document encoder produces canonical bounded `Zeroizing<Vec<u8>>`. The
typed document, including its Secret fields, is dropped before passing the
encoded body to Core. The encoded buffer is erased on exit, including failure.
The original Core observation remains alive until Core consumes it; it is
needed for physical comparison and is protected by Core's existing ownership.
No secret clone, public Serde wrapper or plaintext encoding cache is added.
These are ownership and bounded allocation properties, not a measured hard
RSS ceiling. The existing schema caveat about serde's escaped-string scratch
and caller-owned input buffers remains unchanged.

An already stale snapshot fails the precheck before its closure or encoder
runs. A first publication conflicting with an absent observation returns
`AuthStoreError::Core(Conflict)`. Replacement of a present document also produces
Core Conflict when a fresh identity check proves that the pathname no longer
refers to the retained open file, including when its old inode has no remaining
link. A competing write after that precheck is reported by the actual Core
publication as NotApplied with Conflict, preserving the winning bytes. A current
file that fails its private-file policy remains Core Denied. AuthStore never
normalizes these errors or fabricates a Core publication effect.

Once Core is invoked, the returned `AuthDocumentPublication` is preserved
intact: Applied, Unchanged, NotApplied or Unknown, publication failure, cleanup
effect, cleanup failure and an actual opaque `PendingCleanup` when retained.
The caller must inspect the effect even when a failure is present. AuthStore
does not collapse it into a Boolean, retry, reload, repair, or return an
invented current snapshot. Encoder/precheck errors use `AuthStoreError` and
occur before publication. There is no implicit retry on either path.

An explicit no-op publication can create a fresh empty auth.json. An explicit
`ensure_default_local` mutation creates only the local CPU provider and does
not create a profile, credential, active account or login.

## Explicit stage recovery and consumer authority

`recover_stage` forwards to Core's managed-slot discard operation. It neither
parses nor adopts the stage, promotes it to auth.json, infers historic ownership,
or restores a creation proof from a path or JSON. `cleanup_pending` consumes an
actual Core proof and forwards it to the same captured store. A newly authorized
request supplies its own original cancellation, deadline and bounded IO
budget. Both methods preserve the actual `AuthStageRecovery`, including effect,
failure and retained pending cleanup proof.

The consumer must verify current ManageAuthProfiles authority before
publication, recovery and cleanup. An installation path, Core capture, typed
profile, stored token, document body or request context alone does not prove
that authority. This leaf has no permission Boolean, caller factory, role
emitter, grant DTO or OAuth verifier. Application permission producers,
CLI/login/runtime consumers and role integration remain required migration
work. Runtime agent filesystem views must exclude Core's shared
`publication::AUTH_PRIVATE_NAMES`; consumers must not maintain a second list.

There is no Go-format reader, importer, fallback login or backward compatibility
layer. The current Rust product consumes this store; this leaf's historical validation is limited to its storage boundary.

## Fixtures and qualification boundaries

The test factory runs actual Core installation bootstrap, revalidates the
opaque PrivateInstallation under the original context, captures an installation
scope with its exact root, captures the Core auth store, and revalidates the
private installation again. First auth.json creation uses Core.publish and the
shared private SDK birth operation. Negative fixtures deliberately modify
existing files or manufacture untrusted stage entries; they never claim an
SDK birth proof.

The focused suite covers:

- absent reads, explicit empty publication and provider-only initialization;
- private file/directory modes on Unix and unchanged publication;
- bounded document/decoder input, malformed data and final-error
  precedence, cancellation, deadline and invalid positive caps;
- fresh request, different store and stale physical observations rejected before
  the mutation closure;
- four competing publications for accounts, tokens, providers and profile
  metadata, preserving the winner and bounded directory contents;
- all four logical revision mismatches, prospective document capacity and
  closure errors before publication;
- fresh physical captures between A→B→A, unrelated profile edits, same-name
  logout/recreation and provider removal/recreation, with later own birth
  revisions and old snapshot/CAS rejection;
- activation decimal growth exceeding whole-document capacity and valid
  document MAX denials, with zero written bytes, unchanged auth.json and no
  staging entry;
- replaced inode with identical bytes, same-size changes with restored mtime,
  replaced/private parent and hardlink denial on Unix;
- occupied stage, explicit discard, no stage adoption, and preservation of
  foreign symlink/hardlink/public/oversized stages and auth.json;
- bounded real process death after loading the typed snapshot and before
  publication, followed by recapture/reload with old bytes intact;
- compile failures for AuthSnapshot Clone, Debug and Deserialize.

Core's own focused publisher/recovery suite proves real PendingCleanup origin
and store binding, native Applied/NotApplied/Unknown and cleanup ambiguity,
private birth/write/fsync crash phases and cancellation. AuthStore preserves
those opaque types and results directly; it does not introduce product fault
seams or a flaky filesystem watcher to recreate Core's private test hooks.
The AuthStore subprocess test proves only the prepublication process-death
case and must not be described as native publication crash coverage.

Qualification commands, with temporary incremental compilation disabled:

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p graphit-auth --lib store
CARGO_INCREMENTAL=0 cargo test --locked -p graphit-auth --doc store
CARGO_INCREMENTAL=0 cargo clippy --locked -p graphit-auth --all-targets -- -D warnings
cargo xtask check
```

Record executed counts and outcomes separately from this contract. The local
fixtures qualify the host platform and actual private filesystem operations;
they do not prove three-platform runtime migration, live OIDC flows,
ManageAuthProfiles producers or whole-product performance.

## Local authority's consumed physical observations

The unified RoleIssuer receives the actual AuthStore Arc at startup. Internal
`recheck_installation` forwards the real Core metadata-only installation binding;
it does not require auth.json or an active Local account. Core retains its
configured physical document cap internally for actual reads and publication;
there is no maximum-body getter or reservation bridge.

Ordinary Local admission and new Local role verification load the real AuthStore
under its existing document and decoder limits. There is no separate issuer
observation or maximum-body reservation. The public AuthStore.load behavior is
unchanged: one typed decode and original physical revalidation, including parse
errors. After deriving a fixed Local identity/key binding, the consumer drops
the AuthDocument and every decoded Secret, retaining only its original opaque
Core physical snapshot in a shared private LocalObservation. It has no Clone,
Debug, public constructor or serialization. The last shared proof Drop releases
that actual observation; independent admissions retain independent snapshots.

A private try_mutex serializes checks of the same original observation outside
issuer State. The consumer physically revalidates it before and after candidate
installation/scope inspection under the same original request; another request
cannot rebind it. A later request can load a fresh physical
snapshot after an unrelated document edit, while unchanged own provider/profile/
token identity preserves the Local session lifetime. The original request's
snapshot is never revived. The document revision is an operational watermark,
separate from own incarnation/activation revisions and source generation.

This consumed observation adds no publication, repair, network, login, defaults or authority to AuthStore itself. Mutation/recovery consumers still need actual ManageAuthProfiles checks. Core publication/recovery effect types and original public storage/CAS behavior remain intact. The NativeProfileManager consumer is documented below.

## Native manager consumer

[NativeProfileManager](native_profile_manager.md) derives this actual store from
the RoleIssuer and admits only finite AdminParent or VerifiedRoleGrant with
ManageAuthProfiles. Its private prepare/commit path shares the typed mutation
and encoder used by AuthStore. Prepare validates the original observation,
mutates and encodes, then drops typed secrets before the consumer's final fresh
authority check and immediate logical Core publication. These private encoded
bodies retain their actual store/request binding and cannot be publicly created
or extracted. AuthStore remains a data API, not an authority producer.

The manager forwards Core recovery/effects and retains the actual pending
cleanup proof if fresh authorization is denied. Application startup, CLI
offline authority and network/profile integration remain pending.

AuthStore has a private used `cfg(test)` forwarding seam to Core's explicit
`auth-test-support` publication-cancellation guard. It passes this store's actual
physical owner and the original request snapshot without exposing a store
getter. Auth enables the default-off Core feature only in its dev-dependency;
normal library builds and normal/build dependency features exclude it. The six
manager fixtures produce real pending scratch for both present and absent
destinations, preserve it through fresh authority denial, and remove only scratch
using the retained proof and a separately authorized cleanup request. They never
renew the cancelled mutation or fabricate a Core creation proof.

## Original-observation borrow for Broker refresh: focused checks passed

The [activated Broker refresh consumer](native_broker_profile_refresh.md) uses a
crate-private `snapshot_parts` helper to borrow the original typed document
and a private `AuthSnapshotRecheck` over that snapshot's actual physical field.
Recheck forwards to Core revalidation with the same original SnapshotContext;
it never captures/reparses a replacement or exposes a path/FD/DTO proof. The
old refresh secret is borrowed through the finite await without a copy. Those
borrows end before the same snapshot enters existing prepare/commit. A private
USED `profile_limits` getter supplies already configured caps for admitted
new token/claim copies; neither helper grants authority. These helpers are
activated and passed the focused library/rustdoc suites and strict Clippy;
Root integration gate and final review remain pending.

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

## Per-call credential phase: focused qualification

The [service credential boundary](service_credentials.md) uses the actual
AuthSnapshot/snapshot_parts/AuthSnapshotRecheck producers. Each service selection
owns one original snapshot under the actual document/decoder limits until its
borrowed leases end. Credential-bearing modes borrow the chosen secret directly
from that typed document while a private rechecker mutex serializes admission checks
before/after finite awaits. Legitimate contention waits for the lock, then checks
the same original request. The mutex is held only for that recheck, never across
an await. Those credential-bearing phases never replace or clone the selected
secret, refresh credentials, ensure a default or publish auth.json. Drop releases the actual
owned snapshot without an observation permit or maximum-body reservation.

Physical-observation modes reject changes to their original observation.
For actual Runtime callers, LocalAuthorized, Direct, BrokerNative, BrokerKey and
BrokerAnonymous instead load a
current private document at the same lease barriers and under the same Original,
using existing `AuthStore::load` and installation rechecks. It compares the
captured active profile name and exact provider configuration. Credential-bearing
modes also require the captured profile/token/provider revisions. They discard the
current observation without adopting new authority
or replacing the original selection. Runtime key and service-policy revocation,
physical installation, cancellation and deadline checks remain unchanged.
An unrelated provider or inactive-profile publication therefore does not invalidate
Runtime inference; changing the selected credential still does. LocalAuthorized
uses no profile secret and does not depend on token rotation. Local-origin, Role,
Web/imported, Incoming and StandaloneS3 retain their original physical gates.
AuthStore publication, refresh, recovery and Core private-file semantics are
unchanged. NoActive is returned only for a valid
rechecked active-less document under explicit Local policy and grants no temporary
CPU lease. Restoring persisted defaults requires explicit administration outside
AI. Focused Linux execution passed service21, subject5, impacted issuer138 and
Auth rustdoc115. A test-only lexical cleanup passed repeated service21 and strict all-target
Clippy. Matching Linux fast integration guards passed; the global full gate remains pending in tsk-aeda. Earlier failed attempts
and store/refresh qualifications remain distinct.

The observation-reservation cleanup in `tsk-7a35` is qualified in current Main
on Linux. The issuer suite passed 163 cases; after removing an unused retained
policy field, the final Source passed 21 service cases and the affected retained
state boundary. Auth rustdoc passed 135 cases, strict all-target Clippy and
formatting passed. The 78-case profile-manager filter overlapped the issuer suite
and is not added as unique coverage; the empty policy filter checked compilation
only. Logs are in `.build/rust-auth-observation-cleanup-20261011`. Prior receipts
describe their historical Source. This cleanup preserves real authority, input
limits and observations; the separate ProfileManager pool remains a later unit,
and Linux checks do not qualify runtime Windows/macOS behavior.

The `tsk-1750` cleanup removed the unused public Core maximum-document getter
and its getter-only test. The configured document byte limit remains enforced
inside reads and publication. Current Main passed 54 affected `auth_document`
cases (one subprocess helper remains ignored and is run by its parent), six
compile-fail documentation cases, strict Core all-target Clippy with
`auth-test-support`, and formatting on Linux. This check does not qualify other
packages or native Windows/macOS execution. Logs are in
`.build/rust-core-auth-document-unused-max-getter-20261011/main-affected.log`.
