---
title: Role credentials
type: reference
status: partial
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-61c9a}
    relation: implements
  - target: {type: task, id: tsk-35d6}
    relation: implements
  - target: {type: task, id: tsk-749e}
    relation: implements
  - target: {type: task, id: tsk-c2c5}
    relation: implements
  - target: {type: task, id: tsk-7a35}
    relation: specified_by
  - target: {type: task, id: tsk-85d6, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-0f4c}
    relation: implements
  - target: {type: task, id: tsk-c3bf}
    relation: derived_from
  - target: {type: task, id: tsk-d654}
    relation: derived_from
  - target: {type: task, id: tsk-e849}
    relation: derived_from
  - target: {type: task, id: tsk-aeda}
    relation: relates_to
  - target: {type: task, id: tsk-97d9}
    relation: relates_to
  - target: {type: task, id: tsk-8c6a}
    relation: relates_to
  - target: {type: task, id: tsk-cd05}
    relation: relates_to
  - target: {type: task, id: tsk-ddeb}
    relation: relates_to
---

# Role credentials

The [Broker client identity verifier](broker_client_verification.md) is a separate
producer: a valid signed JWT proves identity locally until its expiry, with trusted
cached JWKS and key rotation. UserInfo is optional when profile data is absent; it
is not mandatory per request. PROJECT authorization, server permission and live
scope checks remain separate requirements before creating a role parent.

The original `graphit-auth` role library and Hub role-reference codec were qualified
under `tsk-d654`. The runtime authority producer now uses the real
[private runtime owner](runtime_key.md) under `tsk-0f4c`. References and SourceRef inputs
remain data; verified authority requires the live local issuer. Configuration
resolution and CLI/HTTP integration still have distinct delivery boundaries.
`tsk-e849` completed profile-contract refinement, and `tsk-8b55` delivered the
owned loopback consumer without a cumulative connection quota. These deliveries
are separate from the tool-owned role definition API. Managed Scout, Tracker and
Scribe can now describe their validated tool Brand explicitly through
RoleDefinition::tool_owned; this is data accepted only by the actual issuer's
administrative registration. The native installer, raw override capture,
managed launchers and host execution remain the independent consumer delivery
tsk-bdf5; a definition or codec roundtrip does not install or execute a role.
Delivery status belongs in
[product architecture](../development/product-architecture.md).
The independent [bounded HTTP transport](bounded_http_transport.md) is the
network prerequisite for Broker verification; it carries no authority itself.

## Physical scope validation

The crate-private `PhysicalScopeInput` contains actual installation, optional
project and provider captures. `RoleIssuer::validate_physical_scope` reuses
installation identity, brand, captured-record matches, exact configured roots,
provider owner and allowed roots, project `VersionContext` root and physical
revalidation under the original request. The installation capture uses
Core's installation-only revalidation: its owner/runtime roots and relevant
ancestors and receipts remain mandatory; an unrelated configured project may
be absent. Selected projects and every supplied provider retain complete
physical revalidation, including installation-owned providers. No captures
are refreshed after a filesystem change. It contains no agent `NativeBinding`,
profile or permission and cannot produce a grant. The Broker PROJECT admission
consumer uses this physical subset; its verified identity and actual PROJECT
decision remain separate requirements.

Role `ScopeInput` shares the same physical checks and retains its `NativeBinding`
and captured role lineage. Without a project, its global version remains
canonical and nonempty; with a project, its actual `VersionContext` is used.
`ScopeFacts` keeps the installation original separately from the project and
provider originals and uses the same distinction for later role rechecks.
Removing checkout A therefore does not invalidate a role scoped to intact B,
while replacing B or the installation/runtime roots still denies use. No summed
capture serialization charge or predictive retained-capacity reserve is needed.

## References, secrets and live authority

The closed roles are Scout, Tracker, Scribe, Coordinator and Dream.
`RoleCredentialReference` contains Role, a tagged RoleSource and a SHA-256
fingerprint of the qualified definition and permission constraints.
RoleSource::HubArtifact retains the actual published Agent ArtifactId;
RoleSource::ToolOwned retains a validated Brand for a tool-maintained role.
The role plus Brand names the tool-owned identity without inventing a Hub
artifact. RoleDefinition::tool_owned accepts only Scout, Tracker and Scribe;
Coordinator and Dream remain outside that managed origin. The bounded wire
contains format_version, role, source and policy_sha256. The source object is
closed and canonical: kind hub_artifact with artifact_id, or kind tool_owned
with brand. Unknown, duplicate, missing, mixed and noncanonical fields fail;
the former artifact-only wire is not accepted.
Replacing a definition preserves its source, role and native binding; the
existing generation change invalidates previously issued role credentials. Its bounded, versioned wire is data only. Agent names,
aliases, fingerprints, stored scope records and references cannot reconstruct
a grant or authenticate a role.

The selected local bearer uses 32 bytes of OS entropy, encoded canonically as
`gr1_` plus 43 URL-safe Base64 characters without padding. The private bounded
issuer retains token hashes and live metadata, not plaintext bearer keys.
Necessary owned secret buffers use Zeroizing/Secrecy; diagnostics contain
static categories, never credentials or sensitive values.

Minting and use require an authenticated parent, registered role ceiling and
original live captures for InstallationId, Brand, optional ProjectId, selected
version, NativeBinding and full scope bindings. Child permissions remain a
subset of the parent and role ceiling. Every request checks scope, reference,
issuer/run/profile revisions, expiry, deadline and cancellation before business
stores. Missing, expired, revoked, unknown or mismatched credentials fail
closed without root-key or profile fallback. Consumers enforce own-Task Actor
constraints against the verified grant. Dream and Coordinator retain their
actual Task/Memory admission and fenced-write consumers in `8c6a`, `cd05` and
`ddeb`; the managed Scout/Tracker/Scribe toolbox does not create a Role to
authorize those journeys.

Permissions distinguish module reads, Memory records, own-Actor Task records,
fenced Memory mutation, bounded Git inspection, Task coordination and delegation.
The existing Module enum identifies modules; its orchestration switches do not
grant access. Only Coordinator may coordinate Tasks or delegate, and only when
its parent grants those effects. Scope delegation initially requires the exact
same realm, owner, project/version, native binding and captured root bindings;
it may reduce permissions and lifetime. Path prefixes and root subsets do not
prove narrower authority. Additional Core observations are permitted only by
the existing scope-binding comparison and still require fresh revalidation.

Coordinator also represents explicitly granted mutations of other modules,
user records, configuration, profiles, access grants and caches. External effects
distinguish native launch, hook execution, remote requests and published files.
Those permissions grant neither arbitrary shell execution nor an unclassified
tool. Task record writes and Task coordination remain different permissions;
Memory records use ordinary or fenced writes. Other roles cannot acquire these
Coordinator effects. Each exposed tool is admitted by its consumer's effective
policy. The [Role MCP toolbox](rust_role_mcp.md) uses its existing policy for
list and call; unknown or unpermitted operations fail without a broader-key
fallback. This does not require a new universal registry or descriptor layer.

## Lifetime and issuer state

Startup captures both real sources with
`IssuerAuthorityPolicy::new(runtime, local, limits, services)` and
`RoleIssuer::new(realm, Arc<RuntimeKeyOwner>, Arc<AuthStore>, policy, ttl, request)`.
Each `SourceAuthorityPolicy` borrows explicit ordinary and administrative
permission sets and its finite `AdminSessionPolicy`. Input bounds cover the
actual permission vectors and scope/key text; no
configuration/profile/group assertion supplies permissions. There is one State,
not an issuer for each origin. Startup checks the actual runtime owner and
metadata-only AuthStore installation binding; auth.json and an active account
need not exist. The issuer retains the real owner/store Arcs, without copying the
runtime secret or inventing a path proof.

`authenticate_runtime(raw, original_request)` and
`authenticate_local(raw, original_request)` return the same opaque
`AuthenticatedCaller`. Ordinary admission uses only the selected origin's
explicit server policy and never creates or renews an administrative session.
Runtime has no artificial key TTL. Local requires current active Local provider
and profile revisions and a nonempty MCP key, compared by constant-time SHA256.
The stored anonymous flag never bypasses the key; a correct key remains valid
with that flag, and stored username, organizations and teams never grant access.
Actor attribution is minted from the issuer epoch, origin, generation and Local
binding; callers cannot choose it.

Local qualification loads the real store under the original request, derives a
fixed domain-separated binding from borrowed active identity/key digest and all
own provider/profile/token revisions, drops the typed document and decoded
secrets, and consumes its original opaque physical observation. Checks keep that
same observation before and after scope inspection. No fresh load rebases a
proof during its request. Between requests a higher document revision with the
same own binding advances only an operational watermark and preserves actor,
session/run expiry and source generation. Older qualified documents cannot roll
back that watermark. Logout, missing source, activation, incarnation, identity
or own key changes invalidate Local lineage; Runtime lineage is independent.

`bootstrap_admin(caller)` is an explicit operation that creates a finite
`AdminSessionHandle`. `AdminSessionPolicy` requires a positive lifetime and
maximum, with lifetime no greater than maximum; no role TTL supplies these
values. The handle is a process-local epoch/id selector, not a wire bearer or
standalone grant. It has no public constructor, Clone, Debug or serialization.
`admin_parent(fresh_caller, handle)` requires the same issuer, current source
generation, original caller request and an unexpired session. The resulting
`AdminParent` authorizes the existing `register_definition` → `start_run` →
`issue` lifecycle, with `verify` and grant `authorize` for children. Refresh,
revoke and close-run use the same live parent. None implicitly bootstraps a
session. Registering or replacing definitions additionally requires explicit
`ManageAccessGrants`, as well as the administrative permission subset.

`register_or_reuse_hub_definition(parent, definition)` atomically registers a
qualified Hub definition or reuses its exact existing definition. The slot is
the role, Hub source identity and native binding; equality checks the entire
private definition: reference (including source and policy hash), binding and
canonical permissions. Reuse preserves generation, grants and admissions, but
still checks the live original caller, administrative session,
`ManageAccessGrants` and permission subset. A different definition in that slot
returns `Invalid` and requires explicit `replace_definition`; a tool-owned
source returns `Denied`. Strict `register_definition` still rejects duplicates,
and explicit replacement still advances generation and revokes old grants even
when the replacement is equal. Reuse does not authorize a project or reuse a
previous request's parent or run.

Each run records its administrative session. A new session cannot adopt another
session's run, even with the same raw runtime key. Session expiry and source
changes invalidate descendants; a newly bootstrapped session cannot resurrect
old credentials. Missing-key recreation is an explicit Core maintenance action
and increments the source generation even when the bytes remain identical.
Authentication never repairs, recreates or removes a key. The old raw-secret,
LocalPurpose/key-expiry factory and manual issuer key rotation were removed.

Children present only their RoleBearer. `verify` and every grant effect recheck
the real source selected by the retained lineage: Runtime owner or Local store.
They do not require the parent's raw key. Role
definitions use a qualified Hub Agent source or the actual tool-owned Brand
source; neither source data alone grants authority. Authorization returns
obligations that handlers must enforce: `OwnActor` and `MemoryFenceRequired`.
The library does not execute those handlers.

Qualified role definitions are permission templates for one local realm.
Registering a template does not authorize a project. A fresh authenticated local
administrative parent explicitly authorizes each concrete scope through `start_run`; the private
run binds full captures, selected version and native binding. Issue rechecks that
authorized run and its template. Project roots and provider roots must be exact
members of the realm's configured allowed roots, with live directory identities;
path prefixes do not expand the realm. Delegated roles inherit that same scope.
Refreshing invalidates both the old bearer and grants verified from that bearer,
while preserving Actor and stable lineage identity for still-valid descendants.

The issuer retains clones of the original installation, project and provider
Core captures and revalidates them alongside request captures. A partial wire
record that omits observations cannot replace those proofs. Kiro V3 is the maintained native binding.

The library requires explicitly supplied `IssuerLimits`, `TtlPolicy`, separate
ordinary/administrative grants and `AdminSessionPolicy`; it
resolves no startup configuration or defaults. Local admission reads the configured
private AuthStore under the original request. The table below is
the explicit starting values for configuration consumers; they are not library
initialization behavior. `tsk-e849` completed its profile contract; these values
do not establish a builtin role Source factory.

| Setting | Initial value |
|---|---:|
| Role TTL / maximum TTL / refresh margin | 900s / 3600s / 120s |
| Permission input entries | 1024 |
| Role/source and Inventory input bytes / parent bearer bytes | 65536 / 65536 |
| Administrative lifetime / maximum | Explicit startup inputs; no default |

Input limits are positive and checked for permission entries, role source/reference
serialization and authentication key inputs. No issuer ledger or session/profile/run/credential quota
models memory usage. TTL cannot
exceed the maximum; refresh margin is below TTL and at most half of it.
Child validity is bounded by finite administrative session expiry and run lifetime, with monotonic
time preventing clock rollback from extending validity. Refresh revalidates
parent, scope and profile, replaces the nonce and preserves Actor/run identity.
It cannot revive expired, revoked or closed credentials. Failed admission
preserves prior state; adding another legitimate session, run or credential
does not evict an active grant.
After generating a refresh nonce, the issuer rechecks the old credential and
every ancestor at the current UTC and monotonic time before replacing its hash.
Expiry during entropy generation returns `Expired` with the old hash and state
preserved, even when the session, run and proposed new expiry remain valid.
`AuthError::InvalidSetting(Setting)` reports only the category and static field
name, never the rejected value. `max_scope_bytes` bounds the actual role source/reference serialization and
Inventory endpoint input; it does not sum serialized captures or version text.
Shared Core observations retain their original capture limits. Pruning removes expired or revoked sessions, orphaned credentials
and scopes without a live run. Credential ancestry traversal uses actual stored
records to reject a cycle; no configured credential quota is needed.

## Physical rechecks

Core runtime borrowing uses the original `request.snapshot()`; another snapshot
cannot rebind an existing proof. Runtime authentication checks the actual owner,
installation binding and secret. Each caller or retained grant physically checks
its original source before and after scope inspection; bootstrap and
administrative effects perform the same checks.

A physical RuntimeKey or AuthDocument I/O failure is `AuthError::Io`, not
evidence of an invalid identity. It remains fail-closed: no authentication or
authorization is granted by this error. Identity conflicts, invalid inputs and
actual permission denials keep their existing terminal categories; cancellation
and deadlines retain request semantics. Consumers can report operational
unavailability separately from `Denied`, without exposing paths or secrets.

Local admission loads the bounded AuthStore document, derives a fixed identity
and key binding, then drops the typed document and decoded secrets. Its private
Arc retains only the original physical observation and binding. Independent
callers own independent observations; dropping the final shared proof releases
its actual resources. There is no observation-slot or maximum-body reservation.
Core document and profile decoder limits remain limits of the actual input.

Each Local source check validates installation binding, then revalidates the
same original observation once. It does not read the same body a second time
immediately before the metadata-only binding check. Admission still revalidates
after extracting profile facts and again before returning the proof, and grants
retain source checks before and after scope inspection. The observation mutex
remains necessary because the retained file cursor is mutable; it is not held
across network I/O or an await. Original cancellation and deadline checks apply
after waiting. Fresh requests may load fresh observations, but a retained proof
is never rebased to a new request or file. Metadata binding does not load body
data or grant authority.

All physical scope inspections happen outside the issuer State mutex. A
brief retained Arc holds the original run facts; after bounded inspection the
commit checks the same Arc and current session/run/profile/nonce state.
State uses a normal mutex for its short critical sections. Ordinary contention
waits rather than denying authentication; poison remains a static State error.
Original request checks still precede authorization and state effects after
acquisition. Mutex waiting itself is not preemptible. All six
scope-binding comparisons use Core's `matches_scope_bounded` with the original
request deadline and cancellation. That comparison is data only, performs no
filesystem or body IO, and waits for concurrent scope observations. Original
context checks immediately after acquisition preserve cancellation and deadline;
poison remains an error. Separate
installation-only revalidation inspects the original installation capture,
while `revalidate_bounded` still inspects every used project and provider before use;
matching a binding record does not grant authority. Other Core consumers may use
the blocking revalidation API. Source generations never move backward from
a stale observation.
Final checks precede State effects; cooperative revalidation is not an atomic
transaction against a hostile filesystem.

Issuer state owns its actual sessions, profiles, scopes, runs and credentials.
There are no retained-byte counters, capacity predictions or state admission
quotas. Real source captures use shared Arcs and normal ownership. Pruning and
explicit revocation preserve current authority and release unused run scopes.

## Transport and consumer boundaries

Managed stdio launchers deliver the bearer only through
`GRAPHIT_ROLE_BEARER` in the child runtime environment or a protected live
runtime handle. Static role/MCP files, argv, shared records and logs contain no
bearer. References may persist as necessary evidence in
[shared native document transactions](native_shared_documents.md); references
remain separate from runtime secrets and live authority.

The implemented runtime issuer authenticates its real private owner explicitly.
Broker/profile contracts refined by completed `tsk-e849` use the current Rust
JWT verifier: trusted signature/key, issuer, intended audience and purpose,
client/resource binding and time claims. A signed access JWT remains valid until
its expiry; remote revocation, account blocking or claim changes may wait for
that expiry. UserInfo is optional profile enrichment, not per-request authority.
PROJECT access is a separate authorization decision; identity alone cannot
derive parent permissions. Local opaque role revocation remains immediate. Membership, body, headers
and environment selectors cannot supply that authority. Incoming remote callers
do not refresh daemon login; per-request/proxy bearer identity is preserved,
and RoleBearer is not forwarded as a downstream API credential.

Common must consume the data reference without waiting for the whole CLI/PF
delivery. CLI, MCP, HTTP and managed runtime consumers require both the local
issuer and the returned provider/profile producers, followed by real list/call
policy enforcement. Local-library delivery alone cannot qualify those journeys.

## Verification boundary

Cryptographic primitives: [OS entropy](https://docs.rs/getrandom/0.4.3/getrandom/fn.fill.html)
and [constant-time comparison](https://docs.rs/subtle/2.6.1/subtle/trait.ConstantTimeEq.html).

## Publication ordering for both origins

Explicit administrative bootstrap is finite per origin. Its process-local handle
selects one epoch/session; a fresh caller of another origin or a new session
cannot adopt an old run. Ordinary requests, role verification, delegation and
refresh never create or renew this parent implicitly. Definitions and ceilings
remain in the common State; registering/replacing them requires explicit
ManageAccessGrants permission for the authenticated origin.

Nonce, run and session entropy is generated outside State, with bounded eight
collision attempts. The original physical source is rechecked after the last
entropy and scope pass, before the short final state commit. Current source
generation/watermark, exact session/run ancestry, caps, collision and UTC/monotonic
expiry are checked again without publishing partial sessions/runs/credentials.
Refresh also checks the old credential and its entire ancestor chain after the
last entropy before replacing its hash, preserving the historical expiry guard.
The real Runtime mutation-during-entropy counterproof was reproduced before this
correction; its prior immutable evidence remains historical.

This library producer does not install listeners, transport session handles,
create profiles, publish auth.json or grant filesystem access by itself. The
NativeProfileManager consumer below implements the profile operations and its
actual pending-cleanup denial/retry fixtures. Broker incoming sources,
startup policy selection, CLI/MCP/HTTP permission consumers, native platforms
and whole-product performance have separate validation scopes.

## Native profile administration

[NativeProfileManager](native_profile_manager.md) uses the issuer's actual
AuthStore and original request. Finite AdminParent freshness alone does not
authorize profile effects: the used private seam separately verifies
ManageAuthProfiles in its origin's administrative policy. VerifiedRoleGrant
uses its actual live authorization path. Wrong issuer fails before I/O.

The manager completes pure mutation/encoding before its final fresh
permission/source/session check and immediate logical publication entry.
Recovery/cleanup similarly check fresh authority and preserve Core effects
and retained proofs. This library does not supply initial offline CLI authority,
Broker/OAuth verification, startup or listener integration.

The manager's pending-cleanup fixtures use the original issuer's real store,
cancel publication under the original mutation request, and deny cleanup with a
fresh wrong-issuer parent, expired finite parent or revoked Role. The same opaque
proof returned by denial succeeds under a separate valid fresh authority without
renewing the cancelled mutation. Test support is a Core dev-dependency feature,
excluded from normal production library builds.

## Per-call service authority: focused qualification

The [service credential boundary](service_credentials.md) extends this same
issuer's policy through a mandatory borrowed fifth constructor argument. Runtime,
Local and Role operations retain their actual RemoteRequest ceiling. Incoming
Broker identity has no Role authorize API: only an exact server-owned Remote
rule in this issuer admits its service purpose and binding. Identity alone never
grants that permission. One-way ManageAccessGrants revocation invalidates old and
new service leases. The four existing constructor callers explicitly deny all
services until configured. Focused Linux execution passed service21, subject5,
impacted issuer138 and Auth rustdoc115. The test-only lexical-scope cleanup passed a repeated service21 and strict
all-target Clippy. Matching Linux fast integration guards passed; the global full gate remains pending in tsk-aeda. Earlier Role results and failed attempts remain preserved history.

The [literal verified subject](verified_user_subject.md) borrows only actual
Native/Incoming proofs; Actor, Local display data and Role are not USER. Subject
identity remains separate from ReadUser/MutateUser and physical USER scope. The
five AI loops/native HTTP final gate and Core USER storage producer remain
necessary separate integrations; this partial library execution does not qualify them.

The historical observation-reservation cleanup receipt in `tsk-7a35` records
its then-current Linux Main. The issuer suite passed 163 cases; after removing an unused retained
policy field, the final Source passed 21 service cases and the affected retained
state boundary. Auth rustdoc passed 135 cases, strict all-target Clippy and
formatting passed. The 78-case profile-manager filter overlapped the issuer suite
and is not added as unique coverage; the empty policy filter checked compilation
only. Logs are in `.build/rust-auth-observation-cleanup-20261011`. Prior receipts
describe their historical Source. This cleanup preserves real authority, input
limits and observations. The separate ProfileManager pool was subsequently removed in
`tsk-cdb8`;
and Linux checks do not qualify runtime Windows/macOS behavior.

The `tsk-749e` issuer ledger and state-quota removal is qualified in Main on Linux.
Input bounds, revocation and Original checks remain functional. See the
the historical qualification recorded in task `tsk-749e`;
historical retention-capacity receipts describe their earlier Source.

## Run transport selection

The managed child carries the transient private fields GRAPHIT_ROLE_BEARER,
GRAPHIT_ROLE_REFERENCE, GRAPHIT_ROLE_SELECTION and GRAPHIT_ROLE_BINDING. The
stdio proxy validates the closed NativeBinding and transports it as
`x-graphit-role-binding` only for Role admission. The header is an untrusted
selection, never a grant: issuer verification binds that exact host to the
reference, bearer, run, physical scope and current parent. Altering it denies
access. When absent, the normal captured daemon binding remains the selection;
no alias or legacy translation is introduced. Runtime/Local admission does not
consume the header. Installed Skill/Hub readers use the effective binding only
after successful Role verification.
