---
title: Outgoing Hub read service credentials
content-type: reference
audience: developers
status: implemented
updated: 2026-11-07
references:
  - target: {type: task, id: tsk-0074}
    relation: supports
  - target: {type: task, id: tsk-d75f}
    relation: used_by
  - target: {type: task, id: tsk-e302}
    relation: implements
  - target: {type: task, id: tsk-0af2}
    relation: uses_api
  - target: {type: task, id: tsk-5651}
    relation: supports
  - target: {type: task, id: tsk-aeda}
    relation: implements
  - target: {type: task, id: tsk-6873}
    relation: implemented_by
  - target: {type: task, id: tsk-ca95}
    relation: specified_by
  - target: {type: task, id: tsk-3582}
    relation: uses
  - target: {type: task, id: tsk-f044}
    relation: supports
  - target: {type: task, id: tsk-8cc0}
    relation: relates_to
---

# Outgoing Hub read service credentials

`ServicePurpose::HubRead` admits an outbound read transport through the existing
[service credential](service_credentials.md) mechanism. The caller supplies the
maintained provider name explicitly. Hub discovery, metadata, hashes and S3
configuration do not supply permission. Read admission does not grant publish.

The implementation lives in
[service_credentials.rs](../../crates/graphit-auth/src/issuer/service_credentials.rs);
focused real AuthStore/JWT/JWKS fixtures live in
[hub_credentials.rs](../../crates/graphit-auth/src/issuer/tests/hub_credentials.rs).
Auth library qualification is separate from App SDK install/update consumers
and the Rust Broker resolve/STS route producers refined in `tsk-8cc0`.
This library does not deliver those routes or make a download by itself.

## Explicit capture and policy

`RoleIssuer::capture_hub_service(&AuthenticatedCaller, provider_name: &str)` and
`capture_role_hub_service(&VerifiedRoleGrant, provider_name: &str)` return the
existing `ServiceSelection`. They fix the purpose to HubRead. Generic
`capture_service`/`capture_role_service` cannot choose a Hub provider implicitly.
Empty or oversized names produce the existing Invalid/Limit errors.

The actual caller or verified Role must have
`External(RemoteRequest) == Unconditional` and a matching explicit startup rule.
The rule uses `RemoteServiceBindingInput::Broker`: origin, provider, HubRead,
configured endpoint, exact optional resource and either NativeAccess or Anonymous.
Matching does not depend on Embedding or Rerank being enabled. Local, Direct,
BrokerInventory and generic IncomingMcp bindings cannot admit HubRead. The
specific imported Web read below uses its own IncomingBearer rule and proof.

The active profile must belong to the named provider. Its actual provider revision
and the captured AuthStore observation remain authoritative for this selection.
Credentials from another provider are never borrowed. A Local caller still needs
its actual Local source proof; this capture cannot relabel it as a native caller.

Duplicate origin/provider/purpose rules fail except for one explicit HubRead
NativeAccess/Anonymous pair with identical endpoint/resource bindings. Repeating
either credential or changing that binding fails preflight. Mode selection follows
the real active profile and provider configuration; NativeAccess admission alone
never authorizes Anonymous, and an invalid active native token cannot downgrade.

## Imported Web reads

`capture_imported_ast_hub_service` and
`capture_imported_knowledge_hub_service` borrow the actual imported admission
and fix HubRead. Native/Local callers retain their real RemoteRequest requirement.
Web callers require an explicit imported-provider opt-in and a
`ServiceOrigin::Web` Broker HubRead rule using IncomingBearer and no incoming
principal selector. These imported HubRead captures reject Remote AI, HubWrite,
Inventory and Presence purposes.

The provider comes from the verified Web admission. Its revision,
endpoint/resource and registered caller remain checked; selection does not read
an unrelated active profile. `selection.credential()` returns the ordinary finite
lease in the borrowed-bearer transport mode; its private proof and
`user_subject()` origin stay Web. Keep that lease through STS and SDK I/O.
A valid JWT, this Auth admission, registry metadata and authorization revisions
do not themselves grant fresh Broker project/S3 permission. The
[service reference](service_credentials.md#imported-hub-reads) records focused
producer tests; integrated remote consumers remain tracked in tsk-5651.

## Installed Web reads

The [installed Hub admission](web_installed_hub_admission.md#explicit-hubread-transport)
has its own closed HubRead captures:

- `RoleIssuer::capture_installed_hub_service(&read, provider)` requires the
  admitted Broker provider and an exact Web/IncomingBearer HubRead rule.
- `capture_installed_hub_standalone_s3_service(&read, provider, profile)` requires
  an explicit Web StandaloneS3 rule for the configured target provider/profile.
  This storage target may differ from the authenticating Web Broker provider.

The private installed authority retains its original Web admission. Every
selection/lease check revalidates the issuer, Original, verifier, Installation
identity and current provider/profile state. Broker credentials use the existing Web
proof. Standalone retains the existing verified Web subject separately from
selected transport keys or explicitly unsigned transport; profile names and
claims never become identity. No Native profile, RemoteRequest grant, imported
AST/Knowledge proof or generic Web embedding authority substitutes for the
installed Hub admission and its explicit rule.

GLOBAL registry reads remain local to the installation. Remote Broker storage
uses each real producer ProjectId and existing PROJECT STSv3; there is no GLOBAL
STS protocol or fabricated PROJECT authority. PROJECT source reads additionally
retain the separate installed/PROJECT HubRead join. The Auth bridge is a producer
for HTTP/React consumers, whose integrated validation remains separate.

The d75f Web remote catalog adapters use these captures for `POST /api/hub/remote/projects` and `POST /api/hub/remote/search`. The Web authenticating provider admits the reader; the query selects the catalog target. They call ordinary `ServiceSelection::credential`, retain the Web subject and recheck the read and lease through actual catalog/FileTransport I/O and delivery. No `identity_provider` input or active Native profile can replace that subject. The [HTTP contract](hub_consumer_contract.md#web-remote-catalog) records typed fields, finite errors and lifecycle.

The [FILE readonly consumers](hub_consumer_contract.md#web-file-readonly) retain
this installed read authority. PROJECT list/content requests first resolve the
explicit PROJECT ID and retained version against the live checkout selection.
An ambiguous selection returns HTTP409 `state_conflict` before registry or
payload reads. A selected PROJECT also requires its own Web Hub/read admission
and `check_project_source` join with the same caller, verifier and Original.
`PublicationScope` supplies logical ownership and configured paths; it grants no
access by itself. Current membership and authorization are checked before reads
and delivery. Configured paths use ordinary OS resolution; content fingerprints
and SDK snapshot checks retain their own integrity guarantees.

Remote content uses an owned temporary SDK stage. Installed content verifies the
existing cache without hydration. Reads do not install or mutate artifacts.
[Local Web FILE mutations](hub_consumer_contract.md#web-file-local-mutations)
require their separate mutation admission and report actual publication effects.
A known physical change survives a later metadata or authorization failure;
explicit reruns inspect current content and preserve foreign-file conflicts.
Request cancellation, deadline, logout and shutdown stop admissions and retain
ownership of started jobs until those jobs finish cleanup.

Producer tests use the real signed OAuth/JWKS and AuthStore fixtures in
`crates/graphit-auth/src/issuer/tests/installed_hub/services.rs`. Product
qualification and execution evidence belong to the consuming Tasks.

## Native access and anonymous configuration

An actual `ProviderKind::Broker` and matching active Broker profile select
BrokerNative. `native_credential(&BrokerVerifier).await` retains the existing
[native JWT verifier](broker_native_login_verification.md). It calls
`verify_refresh(access_token, None, stored_subject, same_original)` to verify the
current native access token; the API name does not mean a token refresh occurs.
The trusted issuer, signature, discovered native client ID, API audience, optional
configured resource, token type and times are required. Existing SDK JWKS caching
and key rotation apply. There is no per-request UserInfo, introspection, token POST
or compensating refresh/network barrier.

`ServiceCredentialLease::user_subject()` yields the opaque verified native
issuer/subject. Actor, profile username, teams and metadata remain data. IncomingMcp
and Inventory purposes cannot replace the native Hub purpose or subject proof.

No active profile can select BrokerAnonymous only through the explicitly named
`ProviderKind::Local { broker: Some(...) }` configuration, its `allow_anonymous`
flag, and a matching HubRead Anonymous rule. The existing profile validator rejects
`ProviderKind::Broker` with `allow_anonymous`; this library preserves that contract.
An explicitly active Local anonymous profile is also supported. A present active
Broker key fails anonymous admission. With no active profile, inactive profile
credentials are not selected. `credential()` yields None and `user_subject()`
yields None; the original outbound policy and Config observation still apply.

A provider can be configured without a profile, including an explicit public
bucket in its S3Location. No default provider, environment credential or fake STS
credential is inserted. The consumer must separately establish actual standalone
global/anonymous access and an explicitly public/admitted bucket before using the
SDK without a signature. It cannot derive public access merely from S3Location.
An advertised Broker Hub authority resolves its current public/private SQL rules;
failure or denial never switches to standalone. `/v1/s3/credentials` denies
anonymous, which does not erase separately admitted public standalone reads.

## Configuration data and finite consumer recipe

`ServiceSelection::provider_config(&mut self)` checks the live selection and
borrows the selected `ProviderConfig` from its captured Auth snapshot. The consumer
may copy S3Location data before calling `native_credential` or `credential`.
That copy is not a grant and cannot outlive the need to check the lease.

The consumer authenticates its actual caller, captures the named provider, copies
needed configuration, selects BrokerNative or BrokerAnonymous, and keeps the
resulting credential lease alive for the finite SDK operation. It checks the same
lease before and after protected waits/effects. Native mode borrows the verified
access token; anonymous mode supplies no credential. Broker fresh resource/SQL
authorization remains separate from local JWT validity and must run before
protected metadata or payload access. Cached bytes, selectors and authorization
revisions cannot become reusable grants.

All existing lease getters recheck the same Original, current Auth provider/profile state,
service generation, current source/Role authority and native expiry/verifier
binding. Local profile/provider replacement or logout, cancellation and the
original deadline fail causally through ServiceUseError. Upstream revocation,
account blocking and claim changes can remain unseen until the signed access
token expires; no per-request UserInfo or introspection compensates for this
accepted window. NeedsRefresh belongs to
the existing administrative login/refresh workflow. WrongMode remains a typed
error; this capture performs no hidden refresh or profile mutation.
