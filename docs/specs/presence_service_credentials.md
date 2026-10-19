---
title: Presence service credentials
content-type: reference
audience: developers
status: focused-linux-qualified
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-c52b}
    relation: implemented_by
  - target: {type: task, id: tsk-868e}
    relation: specified_by
  - target: {type: task, id: tsk-60d0}
    relation: supports
  - target: {type: task, id: tsk-f37c}
    relation: supports
  - target: {type: task, id: tsk-d01a}
    relation: supports
---

# Presence service credentials

Now subscription and publication have distinct `ServicePurpose::PresenceRead`
and `PresencePublish` values. Trusted startup supplies a
`ServiceRuleInput::Remote` with the existing `Broker` binding: literal provider,
purpose, endpoint, optional resource, actual Runtime/Local origin, and either
`NativeAccess` or `LocalBrokerKey`. Empty rules deny. Neither purpose accepts
Direct AI, Local AI, BrokerInventory, Anonymous, IncomingBearer, or BrokerIncoming.

Use the same issuer that authenticated the request:

| Caller | Capture |
|---|---|
| Runtime or Local reader | `capture_presence_read_service(&caller, provider)` |
| Current Role reader | `capture_role_presence_read_service(&grant, provider)` |
| Runtime or Local publisher | `capture_presence_publish_service(&caller, provider)` |

All return the existing `ServiceSelection`; there is no separate Native
selection type. The generic capture APIs deny these purposes when no explicit
provider is supplied. Role publication is denied, including through generic
`capture_role_service`. Read permission does not imply publication.

The provider must equal the active profile's provider in the issuer's physical
AuthStore. Native mode requires a Broker provider and Broker profile. Local key
mode requires a Local provider with configured Broker and an actual Broker key.
Endpoint and resource must match that configuration. Missing profiles, wrong
resources and invalid Native credentials never downgrade to another provider or
Anonymous. A Role needs its current grant's
`External(RemoteRequest)` permission with an unconditional obligation plus the
explicit origin/provider/purpose rule. Installation of managed role files does
not add that permission.

Retain the selection and its credential lease across the finite operation.
Native consumers call `native_credential(&warm_verifier).await`; Local key
consumers call `credential()`. The checked selection `binding()` exposes the captured provider revision before
borrowing a Native credential, so the Role consumer can construct or reuse its
verifier with the same configuration and revision without recapturing AuthStore.
The lease's checked binding exposes the captured
endpoint, resource and provider revision. `provider_config()` exposes current
configuration data for constructing the correct verifier; it is not a grant.
The lease binds the same original request, actual caller or Role grant,
physical profile observation and service policy generation. Checks preserve
cancellation, deadline, Role revocation/expiry, changed profile and provider
revision. Native verification binds signed issuer, subject, audience, client,
resource and expiration using the existing JWT/JWKS implementation. It does not
refresh tokens or query UserInfo at each barrier. Expired or near-expiry tokens
return existing causal errors or `NeedsRefresh`, without a usable lease.

A Local Broker key remains opaque credential data, not a JWT subject. This SDK
does not replace Broker checks for current PROJECT capabilities, module access
or the registered instance/project/version channel. The Now subscriber,
publisher and Role dispatcher must perform their own real transport and data
operations; these APIs alone do not prove those product journeys.

Source is in
[service_credentials.rs](../../crates/graphit-auth/src/issuer/service_credentials.rs).
Eight focused fixtures in
[presence_credentials.rs](../../crates/graphit-auth/src/issuer/tests/presence_credentials.rs)
use real AuthStore, issuer grants, Local keys and signed TCP JWKS fixtures.
Current Linux qualification passed all eight Presence fixtures and all 19
affected Hub read/write fixtures. Strict Auth lib/tests Clippy, scoped rustfmt
and diff checks passed. Earlier compile failures remain in the private receipt;
no cross-platform or complete Now integration result is inferred.
