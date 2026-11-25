---
title: Outgoing Hub write service credentials
content-type: reference
audience: developers
status: library-consumer-pending
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-d75f}
    relation: implements
  - target: {type: task, id: tsk-63a3}
    relation: implements
  - target: {type: task, id: tsk-7706}
    relation: implemented_by
  - target: {type: task, id: tsk-6873}
    relation: uses
  - target: {type: task, id: tsk-0688}
    relation: supports
  - target: {type: task, id: tsk-53c6}
    relation: relates_to
  - target: {type: task, id: tsk-ed33}
    relation: supports
  - target: {type: task, id: tsk-7616}
    relation: relates_to
  - target: {type: task, id: tsk-9606}
    relation: relates_to
---

# Outgoing Hub write service credentials

`ServicePurpose::HubWrite` admits explicitly configured outbound Hub write
transport through the existing [service lease](service_credentials.md).
`RoleIssuer::capture_hub_write_service(&AuthenticatedCaller, provider_name)` and
`capture_role_hub_write_service(&VerifiedRoleGrant, provider_name)` return the
existing ServiceSelection. Provider names are required and bounded. The generic
capture methods cannot choose a HubWrite provider implicitly.

The implementation is in
[service_credentials.rs](../../crates/graphit-auth/src/issuer/service_credentials.rs),
with focused fixtures in
[hub_write_credentials.rs](../../crates/graphit-auth/src/issuer/tests/hub_write_credentials.rs).
The actual publisher/caller is required by `tsk-ed33`, following refinement
`tsk-0688`. Its Broker write/STS and SDK publication contracts remain pending.
This Auth boundary does not itself publish a native dataset or grant S3 access.
`tsk-ed33` replaces the unimplemented `tsk-53c6` caller and preserves its real
publication and STS prerequisites.

## Registered Web publication

The specific `capture_web_hub_write_service(&WebHubPublicationAdmission, provider)`
path uses an actual registered Web principal and a private installation policy
entry. The proof retains PROJECT Hub/read for its source; source write is not
required. Its exact Web/HubWrite/IncomingBearer rule selects that session's
bearer, provider revision, endpoint and optional resource, independently of an
active Native profile. It cannot borrow NativeAccess, LocalBrokerKey or
anonymous credentials. The existing installed Hub reader cannot select this
path. See [Web Hub mutation authority](web_hub_mutation_authority.md) for the
private configuration, proof factories and pending integration qualification.

## Source-free remote catalog writer

RoleIssuer::capture_web_hub_catalog_write_service(caller, verifier, original,
provider, installation: ScopeCapture) returns the existing borrowed
ServiceSelection for a remote catalog effect. Its private authority retains the
issuer instance, registered Web caller, verifier, Original, exact provider and
physical Installation capture. The existing Gate/ServiceSelection owns the
AuthStore observation and service generation; no public proof or serialized
authority is introduced.

Only an exact Web/HubWrite/IncomingBearer rule is accepted, with the current
provider revision, endpoint and optional resource. It permits only
External(RemoteRequest). It supplies no PROJECT source, local administration,
cache or installation effect. HubRead, Native credentials, profiles and
Standalone bindings cannot substitute. The remote Broker and current STSv3
grants remain responsible for producer, namespace and delete permissions.

Unpublish changes only catalog references; it does not read or remove
SourcePROJECT, payload, cache or installation data. The existing
capture_web_hub_write_service publication path keeps its retained PROJECT
source-read admission and private installation administration policy. See
[Web Hub mutation authority](web_hub_mutation_authority.md#source-free-remote-catalog-effects).

Wrong issuer, caller, physical capture, provider or rule denies selection.
Changed observations invalidate the lease. Original cancellation/deadline and
provider revision preserve their existing ServiceUseError causes; there is no
credential downgrade, context replacement, automatic retry or rollback claim.

Implementation: [web_hub_catalog_write.rs](../../crates/graphit-auth/src/issuer/service_credentials/web_hub_catalog_write.rs),
with the existing service-selection gate.

## Explicit purpose and credential mode

The Runtime/Local caller or verified Role needs unconditional External(RemoteRequest)
permission and its own exact startup HubWrite rule. The rule uses the existing
RemoteServiceBindingInput::Broker with the configured provider, endpoint and exact
optional resource. The active profile must belong to that provider. HubRead,
InventoryPublish, IncomingMcp, BrokerInventory, Local and Direct bindings cannot
substitute for HubWrite. Duplicate rules remain invalid; the existing special
HubRead native/anonymous pair remains unchanged.

NativeAccess selects the actual Broker provider and active Broker profile.
AI services may be disabled independently; Native Hub storage transport requires
S3 enabled on that profile. `s3_disabled: true` returns `Disabled` for HubRead and
HubWrite before network access. The focused Native Hub fixtures explicitly enable
S3 so JWT verification, waits and revocation reach their intended boundaries.
`native_credential(&BrokerVerifier).await` verifies the current access token through
the [native JWT verifier](broker_native_login_verification.md): issuer, signature,
client, API audience, optional resource, access token type and times. It retains
the existing SDK JWKS cache and performs no UserInfo, introspection, token refresh
or compensating network barrier. Native user_subject comes only from this verified
credential. Invalid native credentials cannot downgrade to another mode.

LocalBrokerKey is admitted only by its own explicit HubWrite Broker binding with
an actual Local provider containing Broker configuration and an active Local
profile with a key. It returns the existing opaque credential lease. The caller
must use the separately configured opaque Broker ingress; token-format detection
or native relabeling is forbidden. user_subject remains absent in this mode.

NoActive and Anonymous write are denied by this minimal producer. Anonymous
Resolve selectors do not establish write admission, and Broker STS denies
anonymous callers. Public anonymous writes need their own real producer before
they can be enabled. Existing [HubRead anonymous behavior](hub_service_credentials.md)
remains available under its current explicit policy.

## Live checks and publication boundary

provider_config checks the current selection before borrowing configuration data.
S3Location, SourceRef, descriptor digests and object prefixes remain data. They do
not grant publication. Lease getters and check retain the same Original, source
or Role authority, physical AuthStore observation, service generation, provider
revision and native token expiry. Replacement, revocation, cancellation and the
original deadline preserve their existing causal errors. Administrative login
and refresh remain separate operations.

The actual publisher must obtain the current Broker S3 write/publish admission and
real scoped credentials before any protected SDK put, then check the same lease
before and after protected waits and before final delivery. Broker Resolve
producers `tsk-7616` and `tsk-9606` return current selector observations; they do
not issue reusable STS/write grants. Project/corpus capture and native installation
write permission remain separate where required. Native publication still needs
immutable SDK snapshots, exact [native root integrity](remote_native_root.md),
conditional catalog publication and truthful partial-effect handling.

Focused Linux qualification passed eight HubWrite fixtures and the existing
HubRead (11), service credentials (27) and user subject (8) fixtures. The commands
were `cargo test -p graphit-auth --lib issuer::tests::<module>` for those four
modules and `cargo clippy -p graphit-auth --lib --tests --no-deps -- -D warnings`.
Scoped formatting, whitespace, frontmatter and six local documentation links also
passed. The library is qualified; publisher/compiled CLI/Broker STS/S3 integration
remains pending in its own deliveries.
