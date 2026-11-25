---
title: Native Broker profile refresh
type: specification
status: implementation-in-progress
updated: 2026-11-07
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

This contract covers renewal of the selected Broker account through
`NativeProfileManager`, the existing Native OAuth verifier and the private
[AuthStore](auth_store.md). Authorization remains with the real current
`AdminParent` or `VerifiedRoleGrant` granting `ManageAuthProfiles`.

## Caller and token contract

`refresh_broker` consumes the manager's original administrative snapshot and
uses its actual OAuth service and original request. Callers supply neither a
refresh token nor replacement identity, CAS values or a replacement context.
The selected provider/profile must be Broker, with a nonempty refresh token.

The operation retains the original profile/provider names, provider revision,
token revision, issuer and subject. `now` is a nonnegative profile timestamp,
not an authentication clock. Token, profile and document revision overflows
are admitted before dispatch. Expired access credentials do not revoke the
administrative capability to renew them.

Renewable OIDC `expires_at` uses UTC microseconds, supplied by verified Native
tokens. Device access-only profiles use Unix seconds and require a new login
instead of refresh. JWT `exp` and OAuth lifetimes use seconds. A current stored
credential that fails signature/claim verification is a verification failure;
it does not authorize another credential or a silent refresh fallback.

Native refresh verifies the access JWT and optional ID token. It does not call
UserInfo. Missing refreshed ID/claims preserves the existing ID and identity
metadata; a verified replacement updates the identity metadata together.
Issuer, subject, provider identity, creation time and S3 selection remain bound
to the original operation.

## Exclusive renewal of one profile

Every issuer/process using the same AuthStore takes the same per-profile OS
lock before renewing. The lock name is derived from the admitted profile name;
it contains no credential and requires no registry. Other profiles retain their
own concurrency. Acquisition is nonblocking, with async waiting governed only
by the original cancellation and deadline. No global document lock spans HTTP.

After acquiring the profile lock, current profile/provider/token versions and
identity are checked before discovery and POST. A stale contender fails with
`NotSent` and does not send the consumed predecessor. Consumers may obtain a
fresh checked service from a newly published token revision; this is not a
retry of the old rotating credential.

## Durable uncertainty and publication

The profile's optional `refresh_uncertain` flag belongs to its existing token
revision. Omitted means Ready. It is not a second epoch, token copy or generic
recovery journal.

Immediately before token POST, the manager writes `refresh_uncertain=true`
using a short latest-document transaction. Only a confirmed local write allows
dispatch. A drop, process exit, lost response or later validation failure leaves
that state in place, preventing another issuer/process from blindly resending
the predecessor.

A marker may conservatively survive a crash before any bytes are sent. A proven
`NotSent` failure can clear it only with the original request, current authority,
the same target expectations and the profile lock still held. Cancelled/expired
requests do not receive a fabricated cleanup deadline or authority. A fresh
legitimate login is the recovery when uncertainty cannot be resolved.

After remote tokens and identity are verified, publication reads the current
AuthDocument under a short publisher lock and validates the original target's
provider revision, token revision, issuer and subject. It applies only that
profile's token update, preserving unrelated document/profile changes and the
user's current active selection. The same atomic write publishes the new token
revision and clears uncertainty; there is no later clear step.

Conventional paths, bounded private credential I/O, atomic replacement and
necessary content/version CAS implement this storage boundary. It does not
retain directory/ancestor identities, confinement proofs, filesystem receipts,
linear cleanup capabilities or a generic recovery state machine. Configured
paths may resolve through links or to another volume.

The qualified persistence boundary is Drop and process exit after the ordinary
file sync and atomic rename. It does not claim recovery from OS crash, power
loss, or exactly-once remote execution. Those cases have not been qualified.

## Results and privacy

Success reports the ordinary atomic-write effect, Applied or Unchanged.
`BrokerRefreshError` preserves the closed category and `RemoteEffect`:

| Effect | Meaning |
| --- | --- |
| NotSent | Token POST was not invoked, or bounded-client admission proved no send. Metadata GETs may have happened. |
| MayHaveApplied | POST may have consumed the predecessor, or verified rotation could not be published locally. Do not retry the old token. |

OAuth Display uses only the closed Native error and remote-effect categories.
It never includes tokens, endpoint URLs, response bodies or vendor text.
Existing OAuth/HTTP bounds, profile/document encoding limits, current authority
checks and original cancellation/deadline continue to apply. Secret/form/encoded
buffers retain their existing Secret/Zeroizing ownership.

## Verification scope

The original shared-issuer tests cover simultaneous managers, cancellation and
dropped HTTP futures. The independent-issuer regression uses the same real
Native stores, authority and HTTP fixture; before correction it observed two
POSTs for one predecessor. Qualification must also cover uncertainty surviving
an independent owner, unrelated-document mutation during HTTP, no replay after
abort, and clearing only with a newly published credential.

These tests use isolated synthetic credentials. They do not qualify installed
model inference, a live provider, every platform or an installed daemon merely
because the Auth library compiles. Execution results belong in Task evidence.
