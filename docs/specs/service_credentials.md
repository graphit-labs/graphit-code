---
title: Per-call service credentials
content-type: reference
audience: developers
status: focused-and-fast-guards-qualified
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-5fb0}
    relation: implements
  - target: {type: task, id: tsk-63a3}
    relation: implements
  - target: {type: task, id: tsk-e302}
    relation: implements
  - target: {type: task, id: tsk-5651}
    relation: supports
  - target: {type: task, id: tsk-0f32}
    relation: implemented_by
  - target: {type: task, id: tsk-749e}
    relation: implements
  - target: {type: task, id: tsk-b541}
    relation: supports
  - target: {type: task, id: tsk-7a35}
    relation: specified_by
  - target: {type: task, id: tsk-9d27}
    relation: implements
  - target: {type: task, id: tsk-31f5}
    relation: derived_from
  - target: {type: task, id: tsk-d1ac}
    relation: supports
  - target: {type: task, id: tsk-9840}
    relation: relates_to
  - target: {type: task, id: tsk-aeda}
    relation: relates_to
  - target: {type: task, id: tsk-6442}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: implements
---

# Per-call service credentials

This Rust library source is active. Earlier focused Linux execution passed 21
service tests, 5 subject tests, 138 impacted issuer tests and 115 Auth rustdoc
examples, including the 22 new compile-fail examples. The test-only lexical-scope cleanup passed a fresh 21-test service suite and
strict all-target Clippy. Matching Linux fast integration guards passed; remaining integrated product journeys are tracked in tsk-aeda.
Earlier failures and producer qualifications remain separate evidence.

For AI, the same `RoleIssuer` admits one Embedding or Rerank operation from an actual
Runtime/Local caller, verified Role, or authenticated incoming Broker caller.
Configuration data never supplies a caller's authority. A credential-bearing lease borrows
one original physical observation and the selected credential. It exposes a live
gate so the finite consumer can check authority immediately before effects and
after awaits. A raw key borrow alone does not enforce permission at native send.

## Explicit policy and authority

`IssuerAuthorityPolicy::new` has a mandatory fifth borrowed argument,
`ServiceAuthorityInput`. Trusted startup supplies rules, positive
`ServiceAdmissionLimits`, and a representable native refresh margin. Empty rules
mean deny-all. The four existing constructor callers receive an explicit empty
configuration; there is no permissive overload or optional guard.

| Source | RemoteRequest authorization | Selected data |
|---|---|---|
| Runtime/Local caller | Actual caller `authorize(External(RemoteRequest)) == Unconditional`, plus exact server rule | Original active profile/provider |
| Role | Actual verified Role authorization and original source/session/run ancestry, plus exact server rule | Original active profile/provider |
| Incoming Broker | Explicit Remote rule in this same issuer's server-owned policy; identity alone supplies no permission | Policy-selected provider, original verifier and bearer |
| Web PROJECT reader | Exact verified Web caller and ReadAst or ReadKnowledge admission under the same Original; explicit Web Local or Broker AI rule | Matching configured local model, or the admitted project reader's same Broker/provider/revision/resource and incoming bearer |
| Imported AST/Knowledge AI reader | Its own live imported admission, exact issuer, installation and Original; Native retains real RemoteRequest permission for remote AI, Web requires an exact Broker AI rule | Native uses its admitted active Local/Broker/Direct profile; Web uses a matching local model or its own admitted Broker bearer/provider |
| Imported AST/Knowledge Hub reader | Its own imported admission and fixed HubRead rule; Native retains actual RemoteRequest permission, Web retains its configured imported-read proof | Native/Local selects its admitted profile; Web uses its own verified bearer/provider, independently of active profiles |
| Web Hub publisher | Private installation policy, its own publication proof retaining PROJECT Hub/read, and an exact Web HubWrite rule | Its own registered Web bearer/provider/resource; no Native profile or source write |
| Installed Hub Web reader | Its own installed admission, same issuer/Original/installation and an explicit Web HubRead rule | Exact Broker Web bearer or explicit Standalone S3 target/profile; verified Web identity stays separate from transport |

Local execution requires its explicit Local rule and matching persisted active
provider and `AiService::Local` execution configuration. The provider may be Local
or Broker: embedding and rerank selectors are independent of its authentication
kind. Provider, execution device and device ID must still match the rule. A local
lease supplies no Broker credential and does not mint RemoteRequest. Every remote rule fixes
origin, provider, purpose and complete binding. Direct bindings also fix protocol,
endpoint, model, dimensions, optional OpenAI `request_dimensions` and optional Google
`batch_endpoint`. The latter is required for `GoogleGemini1`/`GoogleGemini2`; both
options follow the [profile producer validation](auth_profiles.md), are retained in
the startup binding and borrowed in `ServiceBindingView::Direct`. Live profile
matching compares these exact fields, so changing either invalidates the captured
lease. Families and both routes remain literal; no SDK defaults or route derivation
are introduced. Broker bindings fix endpoint and the exact
optional outbound/native resource. Incoming additionally fixes verified issuer,
exact stable subject or explicit AnyVerified selector, and a mandatory incoming
MCP resource. The incoming resource and stored outbound resource are distinct;
the stored resource may be None. Duplicate origin/provider/purpose rules fail,
except the explicitly paired HubRead modes described below.

### Web Hub publication

`capture_web_hub_write_service` accepts only `WebHubPublicationAdmission`, which
joins private installation administration policy with the same Web caller's
PROJECT Hub/read source. The exact Broker Web/HubWrite/IncomingBearer rule is a
separate service gate. The existing lease keeps the same Original, provider,
resource, revision and physical observation, selecting the caller's own bearer
without active-profile or Runtime substitution. FILE mutation proofs and HubRead
proofs do not convert into that publication proof. Configuration, operations and
qualification status are in [Web Hub mutation authority](web_hub_mutation_authority.md).

### Web embedding and optional reranking

`RoleIssuer::capture_web_embedding_service(caller, admission, original)` retains
the actual `ProjectReadAdmission` in its Web authority. It checks the exact
issuer/caller and Original snapshot, accepts only Ast or Knowledge, and repeats
the existing JWT, installation, project capture and Auth observation checks at
every lease barrier. A valid access token alone never grants PROJECT access;
ReadTask, Memory, USER and imported scopes are not admitted by this API.

The startup rule uses `ServiceOrigin::Web` and the explicit Embedding or Rerank
purpose. `capture_web_rerank_service` has the same caller/admission/Original
contract as the embedding factory. A matching Local rule selects the configured
active local model and returns `LocalServiceLease`, with no remote credential.
For remote execution, only Broker bindings with `IncomingBearer` and no substituted
principal are accepted. Endpoint, resource, provider and provider revision must match
the project reader's own verified Broker proof; an unrelated active profile cannot
supply a Direct key or another Broker identity. `ServiceCredentialLease` retains
that exact bearer and rechecks the PROJECT read and live Auth observation. No
Runtime or Local caller is constructed, and enabling a Rerank rule does not activate
reranking in a consumer. Hub publication and standalone policies are unchanged.

The focused Auth tests use signed JWT/JWKS, actual PROJECT authorization and an
unavailable UserInfo endpoint. They cover Ast/Knowledge, wrong issuer/caller/
Original/module/provider, profile mutation and cancellation. This qualifies the Auth
producer; Graph/Knowledge HTTP and the actual CPU model are separate consumer
qualifications under tsk-9d92. No token lifetime, refresh or revocation policy
changes here.

Source: [service credentials](../../crates/graphit-auth/src/issuer/service_credentials.rs)
and [record admission](../../crates/graphit-auth/src/issuer/records.rs).
Fixture: [Web embedding](../../crates/graphit-auth/src/issuer/tests/broker_records/web_embedding.rs).

### Imported embedding and optional reranking

`RoleIssuer::capture_imported_knowledge_embedding_service(&read)` borrows the
actual `ImportedKnowledgeReadAdmission`; the corresponding AST factory borrows
`ImportedAstReadAdmission`. Both retain the original Runtime, Local or verified
Web origin and the same request and installation. They recheck the exact issuer,
read permission, physical scope and live Auth observation at service capture and
lease barriers. Web reads additionally retain their configured imported-provider
policy and verified caller. Neither factory constructs PROJECT or USER authority.

The corresponding `capture_imported_{knowledge,ast}_rerank_service` factories
retain the same admission for the explicit Rerank purpose. Local execution requires
its matching rule and real active model configuration; its lease carries no remote
credential. Native Runtime/Local reads may select configured Broker or Direct AI
only when that same caller has unconditional `RemoteRequest` permission and the
exact service rule matches. This does not grant another project's read authority.

Web reads may use only their own admitted Broker provider, revision, endpoint,
resource and incoming bearer for remote AI; arbitrary Direct Web bindings are
invalid. A matching configured Local rule still uses the actual active local model.
Foreign issuer, caller, Original, installation and provider substitutions remain
invalid. Cancellation, token expiry, provider/profile changes and installation
replacement invalidate the read or lease at their existing barriers, without
UserInfo or another network revalidation step. Imported HubRead retains its separate
purpose-specific factories. No token lifetime or OAuth protocol changes are made.

The focused imported authority tests preserve the existing read-boundary cases and
cover Native Broker/Direct selection with and without real remote permission, both
AI purposes, same-Broker Web selection, foreign provider/resource rejection, local
selection and cancellation. These are Auth producer checks; actual NN, Browser and
HTTP consumers retain their own integration obligations.

### Imported Hub reads

`capture_imported_ast_hub_service(&read, provider)` and
`capture_imported_knowledge_hub_service(&read, provider)` fix HubRead and retain
the admission's own Original, installation and module. Native/Local admissions
still require the caller's actual RemoteRequest permission and an exact rule.

For Web, trusted startup explicitly enables the imported provider and supplies
`Web + Broker(HubRead, endpoint, resource) + IncomingBearer`, without an incoming
principal selector. The private ImportedHubRead authorization checks the actual
registered Web caller, verifier, provider revision and complete binding. It grants
neither general RemoteRequest, HubWrite nor PROJECT access. Selection uses the
provider retained by the read with no profile; an unrelated active account cannot
supply credentials. The existing borrowed-bearer transport mode is
`BrokerIncoming`, while the credential proof and verified subject origin remain
Web. Lease barriers recheck this proof, physical Auth observation, cancellation
and expiry. No UserInfo validity check or alternate credential is introduced.

[Broker inventory service credentials](broker_inventory_service_credentials.md)
use distinct `InventoryRead` and `InventoryPublish` purposes and a typed
`BrokerInventory` binding. Actual Runtime/Local callers may use a Local
Broker key; an actual Runtime caller may use NativeAccess from the active Broker
profile through `native_credential`. LocalProof still requires the active Local
profile/MCP key, so LocalNative mismatches deny without changing profiles or
using another AuthStore. Role and Incoming entrypoints reject these purposes.
The native verifier binds the actual Broker provider endpoint/resource/revision
and trusted registered client/access audience; no inventory audience or scope
is invented from an MCP resource.

[Outgoing Hub read service credentials](hub_service_credentials.md) use distinct
`HubRead` purpose with the existing Broker binding and an explicitly named provider.
`capture_hub_service` and `capture_role_hub_service` preserve actual RemoteRequest
authority, active-provider matching and current Config observation independently
of AI settings. NativeAccess uses the existing native verifier; NoActive may select
only explicitly admitted Anonymous from a Local Broker configuration with
`allow_anonymous`. The checked `provider_config` getter exposes data for the finite
SDK consumer, which must retain its lease. One explicit NativeAccess/Anonymous
pair with identical endpoint/resource is permitted; it never downgrades an invalid
native credential. IncomingMcp/Inventory do not grant HubRead, and HubRead does not
grant publish. App SDK and Rust Broker route integration remain separate pending
deliveries; the linked specification records focused library qualification.

[Presence service credentials](presence_service_credentials.md) use distinct
PresenceRead and PresencePublish purposes with explicit provider capture. Current
Runtime/Local callers may read or publish using their admitted Native or Local
Broker credential; a Role may read only with its actual RemoteRequest grant and
configured purpose. Role publication and implicit/anonymous/incoming fallback
remain denied. These service captures do not supply PROJECT or channel authority.

Incoming does not implement Role authorization and is never converted to a fake
Runtime caller. The issuer privately computes the precise existing
RemoteRequest/Unconditional grant from its matching server rule and rechecks it
at every lease gate. This policy grammar contains explicit allow rules and
absence=deny; conditional Incoming grants are not representable. Incoming uses
its own bearer even when a different daemon account is active or none is active.

`revoke_service_policy(AdminParent)` and `revoke_service_policy_with_role` require
fresh same-issuer ManageAccessGrants authority. A checked generation increment
permanently disables old and new service admissions in this issuer epoch. There
is no reset, regrant or automatic policy reload; a replacement requires explicit
startup of another issuer. Existing Role revocation and expiry remain effective.

## Capture, borrow and consumer obligations

### Explicit standalone S3 transport

`ServiceRuleInput::StandaloneS3` fixes the admitted Runtime/Local origin (or the
explicit installed Web authority below) and an
explicit provider/profile pair. An explicitly absent profile admits only unsigned
anonymous transport; it never selects AWS or a default profile. A present profile
is required for LocalS3 or AWS-chain credentials. `capture_standalone_s3_service` captures exactly
that selection in the existing AuthStore observation without changing the active login
or trying another profile. The selected provider must be Local with no announced
Broker. Runtime/Local callers retain their actual RemoteRequest permission; this
transport capture grants no access to a remote project.

The ordinary credential lease exposes `standalone_s3()` with the captured location
and either local S3 credentials, an explicitly enabled AWS chain/profile, or the
explicit admission without a profile. The profile validator's separate Broker
anonymous contract remains unchanged. Existing barriers check Original, cancellation,
physical revision and policy. Credential expiry is bounded by the stored expiry
and its corresponding monotonic deadline; no new TTL or automatic fallback is
introduced. `credential()` rejects this mode, and `user_subject()` returns no
identity from the transport profile.

The installed Web exception uses `capture_installed_hub_standalone_s3_service(&InstalledHubReadAdmission, provider, profile)` and an explicit Web StandaloneS3 rule. Its provider/profile target is independent of the Broker provider that verified the installed reader. The retained read supplies the exact issuer, Original, verifier, installation and Auth observation; no active Native profile or generic Web embedding proof can replace it. The lease retains `user_from_web` and `user_subject()` reports that verified Web identity, never the transport profile. The corresponding Broker capture is `capture_installed_hub_service` with a closed Web/IncomingBearer HubRead rule. See the [installed Hub contract](web_installed_hub_admission.md#explicit-hubread-transport).

Authenticated standalone ACL evaluation receives a separate checked
`VerifiedUserSubject`. Its username and teams are data from the validated access
JWT, retained until the same signed expiry; changing upstream claims is observed
when a new token is validated. Optional ID-token profile claims and plain
ProfileData are never team authority. These attributes grant nothing themselves:
the current standalone ACL selects projects, and PROJECT authorization remains
separate. Existing discovery/JWKS caching is reused with no UserInfo request at
these barriers.

`capture_service`, `capture_role_service` and `capture_incoming_service` retain
the actual original request; the incoming request must be the same snapshot as
its Broker proof. `ServiceSelection` owns its actual AuthStore snapshot under
the existing document and decoder limits, without a separate observation or
maximum-body reservation.
The selected binding, provider revision, authority and policy generation never
rebase. Credential-bearing modes retain the original profile/token/document
observation. No capture writes, ensures a default, bootstraps administration or
refreshes credentials.

Credential admission and Runtime local selection barriers coordinate their short
read and logical validation with the publisher's existing private `.auth.json.lock`.
Readers take shared locks and can inspect concurrently; publication takes the
exclusive lock. This coordination is local to each inspection and introduces no
permission or credential cache.
The lock covers capture, decoding, binding/credential comparison and final physical,
lock and Original checks. It is released before `selection.mode()` reacquires a
barrier, and before any RPC, network verification or await; no store, selection or
lease retains it. A managed token publication therefore waits for that inspection,
rather than invalidating an otherwise unchanged local Models credential mid-read.
This coordination also applies to Local source, Native installation, Broker record,
imported read and Web user/Hub admission captures; their returned proofs still keep
their original physical observations.

Reading never creates a lock or authentication document. With neither present,
the caller retains the single captured absence observation and its normal NoStore
or bootstrap result. It never reloads a document that appeared in the meantime.
A present document requires a valid existing publisher lock. Missing, malformed,
or replaced lock identity during inspection fails closed. Changes made without the
publisher lock still fail physical revalidation; `Conflict` does not trigger a
retry or replacement of Original.

For `LocalAuthorized`, `Direct`, `BrokerNative`, `BrokerKey` and
`BrokerAnonymous` from an actual Runtime caller, each existing lease
barrier reads a current private AuthStore observation under the same Original.
It compares the captured active profile name and exact provider record.
Credential-bearing modes additionally compare the selected profile, token and
provider revisions. Publishing an unrelated provider or inactive profile does
not invalidate this selection. Missing active data, logout,
provider replacement, revision or execution changes deny the old selection.
The current document is discarded after comparison; it never replaces the
captured selection, Runtime authority or request. No new grant is adopted.
Runtime key owner, secret, generation, installation, service-policy revocation,
cancellation and deadline remain live checks at the same barriers. Local
execution uses no Native bearer, claims or JWT expiry, so a profile token rotation
with the same selected configuration no longer invalidates it merely by replacing
auth.json. The private document read and installation checks remain mandatory.
Local-origin, Role, Web/imported, Incoming and StandaloneS3 modes keep their original
physical observations. PROJECT authorization remains separate. No selected secret
is replaced, copied or refreshed by this comparison; a changed selected credential
rejects the old lease. AuthStore write CAS and physical privacy/integrity checks
remain unchanged.

`mode()` returns LocalAuthorized, Direct, BrokerNative, BrokerKey,
BrokerIncoming, BrokerAnonymous or StandaloneS3. `local()` returns the explicit authorized
execution configuration. `credential()` handles direct/static/incoming or
explicit anonymous or standalone S3 admission. `native_credential(&BrokerVerifier)` verifies the
borrowed native access JWT with the existing producer under the same original
context. Calling the wrong getter returns WrongMode.

`ServiceSelection::original()` returns the captured request after checking current
authority, profile state, cancellation and deadline. Consumers compare its snapshot
with their request using `Arc::ptr_eq` before constructing or using a Native verifier.
This accessor performs no credential verification or network request. The returned
context is metadata; JWT verification still requires `native_credential`, and project
access still requires its independent authorization. The resulting credential lease
must remain live and checked around effects and awaits.

A credential lease's `check`, `binding`, `original`, `credential` and
`user_subject` recheck the original authority and the applicable selected-record
or physical observation gate above. The
observation mutex is held only for that recheck, never across an await. Legitimate
contention waits for the mutex and then rechecks the original request. The phase stays mutably
borrowed for the entire lease, while an immutable `&str` can remain borrowed
during a finite consumer future and allow fresh `check(&self)` calls.

`credential()` returns Some(nonempty borrowed key/token), or None only for
explicit Anonymous policy with stored `allow_anonymous` and no present Broker
key. Missing keys are typed failures; present invalid credentials do not fall
back to anonymous or CPU. Rust permits a trusted consumer to deliberately copy
an exposed `&str`; this API's privacy and lifetime prevent implicit owned
credential escape, not malicious copying by an authorized caller. Transport
header copies remain subject to transport bounds.

Native verification calls existing `verify_refresh(access, None, expected_sub,
original)`. It performs no UserInfo or token POST. An ID token preserved by a
successful Code refresh without a new ID token is historical profile data and is
not reverified as a new pair. For a Code profile, stored expiry and actual
verified UTC/monotonic expiry must exceed the configured refresh margin.
Expired/near-expiry Code credentials return NeedsRefresh without a lease;
InvalidToken remains a Broker error.

A typed [Broker Device profile](native_broker_device_login.md) has no refresh
capability and uses zero refresh margin. Its Native lease remains usable until
the actual stored or signed expiry, with the same Original and policy, physical
profile/provider/token and proof barriers. After expiry, selection and credential
use report LoginRequired; the caller must perform a new login. No automatic
renewal, UserInfo check or PROJECT authority is introduced.

Refresh requires explicit administrative authority outside AI. Only known
applied publication with complete cleanup permits a new phase after the old
borrows end, using the same original context. Unknown, may-have-applied and
pending cleanup outcomes retain their effects and do not authorize old tokens.

## Bounds and failures

Both admission limits are positive and representable. Rule count and the
aggregate binding bytes bound work. Every string is checked against existing
issuer `max_scope_bytes` before copying. The single issuer-policy constructor
validates the four permission vectors, rule count and actual binding strings
before copying. URL normalization keeps the real text bounds. Fallible string
and vector allocation remains; capacities are not charged to an issuer ledger.
There is no retention quota or public owned policy builder.

ServiceAdmissionLimits retains the positive rule count and cumulative binding
text limits. Independent selections own independent observations; Drop releases
their actual resources. Core document/schema, Broker verification and transport
bounds apply independently. No State/AuthStore lock spans IO, and capture does
not retry, cache or reload. Runtime/Local/Role RemoteRequest ceiling is read once
for rule selection, not fabricated as an effect or credential.

Errors are typed Auth, Store, Broker and Profile errors plus Disabled,
MissingCredential, NeedsRefresh and WrongMode. Cancellation/deadline preserve
existing Profile categories. NoActive requires a valid physically rechecked
active-less document and explicit applicable Local rule. It returns no ephemeral
CPU lease and performs no ensure/write; restoring persisted defaults remains
an explicit administrative composition. Error payloads contain no secrets or
identity/binding strings. A concurrent denial after native verification may
follow metadata GETs; the protected guarantee is zero downstream service
POST/output, not zero metadata IO.

## Source, fixtures and remaining integration

Active production is `crates/graphit-auth/src/issuer/service_credentials.rs`;
its compiler privacy/lifetime examples and actual TCP/Core/store fixtures reside
under `issuer/tests/service_credentials.rs` and `issuer/tests/user_subject.rs`.
They separate startup constructor Invalid controls from runtime admission-denial
fixtures whose policy tuples are valid. Incoming mismatch controls use another
real TCP issuer with coherent issuer/endpoint, or a policy outbound resource
different from the actual provider. They exercise two distinct service keys, Runtime/Local/Role ceilings, explicit
local/static/anonymous modes, NoActive/Disabled/missing keys, signed Native and
Incoming purposes, policy/account/token/provider/physical invalidation, Role
revoke/run close/expiry, original cancellation and exact/next actual input
bounds. Incoming JWT identity now validates locally until signed expiration,
without mandatory UserInfo authentication. Those cases now have the focused Linux results listed above; no native Windows
execution is inferred.

The [Direct Cohere/Voyage embedding lease consumer](rust_direct_embedding_leases.md)
is qualified in Main under `tsk-174e` (37 affected tests and strict Clippy).
The [Direct native rerank consumer](rust_direct_native_rerank.md) is qualified
under `tsk-4ec8` for the six captured Cohere/CohereV2, Voyage/VoyageV1 and
Jina/JinaV1 protocol enums (41 affected tests plus the wire test and strict Clippy).
The embedding extension in `tsk-0f32` composes the same real lease with
OpenAI/Compatible and literal Google Gemini1/2 bindings, including captured
`request_dimensions` and `batch_endpoint`; all 41 affected functional tests
and selected strict Clippy passed on Linux. These consumers hold the service
lease across the original
finite future, gate native sends and output, and preserve effects already
possible. Broker/Local modes and complete model-factory/product integration
remain separate coordinated work under `tsk-d1ac` and `tsk-e6ff`; the focused
Auth fixtures alone do not prove those integrations.
USER storage additionally requires the dedicated Core USER physical producer;
neither service admission nor [verified identity](verified_user_subject.md)
substitutes for it. [AuthStore](auth_store.md) and
[Role credentials](role_credentials.md) document the existing producer boundaries.

## Historical S1 Core fixture base

The private S1 fixture mirror was rebased onto the then-current Core PROJECT
capture contract from tsk-9840. Its three project scopes keep the original
`NativeCaptureRequest`; Windows replacement controls target the retained owner
root or source root. The only S1 additions to that current fixture base are the
explicit deny-all service policy input and the two child test modules.
The historical spec6 fixture and that Root base were preserved separately.
The Root Hub integration also added the existing semver dependency edge to the
Lock; Root later added the existing Knowledge dev-FS edge. S1 changes neither
edge. The original private rebase had no compiler or runtime result; current
partial execution is recorded above and in the evidence ledger.
The 26 new source tests and 22 compile-fail examples have the focused Linux
proofs above. Task acceptance now records all 13 active checks passed, including the two
replacement matching fast-guard checks. The obsolete leaf fast/full checks remain
superseded history; remaining integrated product journeys are tracked in tsk-aeda.

## Current fixture oracles

A provider revision change first denies the old active profile as `Stale`.
Replacing and activating that profile against the current provider then exposes
`Disabled`; the fixture checks both in order. A Direct profile without its
required service key fails real publication as `InvalidDocument` before writes.
A valid Broker provider that permits anonymous calls still returns
`MissingCredential` when explicit policy selects `LocalBrokerKey` and no key is
present; a present key under an Anonymous rule remains denied.

The older exact-capacity pool fixture is historical evidence. The current
issuer no longer reserves maximum bodies per observation. Its replacement
checks simultaneous selections, independent Drop and original cancellation;
startup still validates the real runtime owner and creates no auth.json.

## Matching fast-guard qualification

This closes the matching guard obligation of this library leaf. Remaining
integrated product journeys are tracked in tsk-aeda. Delivery checks cover the
affected suites and required consumers, reusing valid results. Platform evidence
belongs to the relevant packaging and integration paths. Performance
investigation addresses concrete problems or decisions that require measurement.
S4 AI/HTTP integration and the dedicated Core USER producer remain separate; no
full or cross-platform PASS is inferred from this Linux proof.

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
task's recorded qualification; its regenerable `.build` handoff is no longer
available. Historical retention-capacity receipts describe their earlier Source.

Concurrent legitimate lease checks wait for the observation mutex instead of
returning an authorization error solely for contention. The same original
request is checked immediately after acquisition, before physical profile
revalidation. Poison, changed observations, revoked generations, cancellation
and expiry remain errors; waiting itself is not preemptible.
