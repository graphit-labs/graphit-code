---
title: Native installation authority
content-type: reference
audience: developers
status: focused-linux-qualified
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-5060}
    relation: implements
  - target: {type: task, id: tsk-b4c1}
    relation: derived_from
  - target: {type: task, id: tsk-5612}
    relation: supports
  - target: {type: task, id: tsk-2ed1}
    relation: implements
---

# Native installation authority

The Auth SDK qualifies an existing Runtime caller against the effective Native
Broker profile of the same issuer. This is installation authority, separate from
ordinary Source permissions, managed roles, browser identity and PROJECT decisions.
The active daemon and installation ingress remain consumer integrations tracked
by tsk-5612 and tsk-c2e2; this SDK does not start another daemon or publish an artifact.

The application opts in through
`IssuerAuthorityPolicy::with_native_installation_permissions`. The policy is empty
by default and accepts only `Configure`, `ManageCache` and
`External(PublishedFilesystem)`. It captures the supplied permission set before
issuer startup; every other permission and duplicate entry is invalid. Existing
Runtime/Local ordinary and administrative policy stays separate.

`AuthenticatedCaller::authenticate_native(self, &BrokerVerifier).await` consumes
the actual Runtime caller and returns that same caller with a private Native phase.
Local callers cannot enter it. It loads the issuer's captured AuthStore, selects
the effective active Broker provider/profile, matches endpoint, resource and
provider revision to the verifier, and verifies the stored signed access JWT
against the stable subject. It uses the existing `verify_refresh` validation API
with no ID token: this neither refreshes tokens nor invokes UserInfo. Discovery
and trusted JWKS may be fetched by the existing cache during this initial
verification. Stored display/profile assertions do not grant permission.

The phase retains the original Runtime proof, verifier, locally checked signed
JWT proof and the selected provider/profile identity and revisions. The decoded
AuthDocument and credentials are dropped after verification. The phase retains
no whole-document bytes or directory observation.

Before each existing installation effect barrier, call
`caller.authorize_native_installation(permission)`. A permitted operation returns
`Obligation::Unconditional`; an absent phase or permission is denied. The method
checks the same Runtime/installation lineage, cancellation/deadline, original
selected provider/profile and JWT expiry. It performs no HTTP, refresh, UserInfo or
repeated JWT decoding. Changing the selected credential/provider, logout or switching the active
profile invalidates the phase. Unrelated profile changes preserve it. Remote account changes follow the
signed JWT lifetime; they do not trigger per-request polling or instant revocation.
Neither Broker access TTL nor native role TTL changes here.

`caller.authorize(permission)` retains its ordinary policy semantics. Native
qualification does not add installation powers to that method or to Source/record
handlers. Verified roles and browser/Incoming proofs cannot be converted into this
Native phase, and the phase does not grant PROJECT access.

Errors retain the existing `ServiceUseError` variants for Auth, Store, Profile
and Broker failures. No active profile, a Local active provider, malformed or
expired JWT, mismatched issuer/subject/provider/resource, missing credential content,
cancellation and deadline never produce a partial authority result. There is no
automatic refresh, retry or fallback to another profile/daemon.

The implementation is in `issuer/native_installation.rs`, with policy capture in
`policy.rs`. Focused physical/JWT fixtures are in
`issuer/tests/native_installation.rs`; the public
[verified subject](verified_user_subject.md) remains identity only.

Focused Linux qualification passed nine physical/JWT cases, the modified profile-switch
case after its publication assertions, and two compile-fail public-contract doctests.
Strict Auth library/tests Clippy, formatting and whitespace checks passed. This
qualifies the SDK; the active App installation consumer remains a separate delivery.

## Readonly verifier configuration

The Runtime consumer can obtain the active Native configuration without an
administrative session: `caller.native_provider()` returns only the public
`BrokerService` and the effective provider revision. It uses the same issuer,
Runtime proof and original request, loads the captured AuthStore and selects its
active Broker provider/profile. The load reads bounded private credential content under the Original. A Local/anonymous caller, missing active profile or active Local
profile cannot use this recipe. Malformed data, missing credential content,
cancellation and deadline preserve the existing causal errors.

Construct `BrokerVerifier::new` with the returned literal endpoint/resource and
revision plus the application's actual client/network/verifier policy, then call
`authenticate_native`. The getter performs no JWT verification, discovery,
UserInfo, refresh or token copying; it does not return a grant or authority proof.
An installation effect still requires the independently verified Native phase
and `authorize_native_installation`. If the provider changes, the later
qualification recaptures the current profile and may reject the old verifier's
binding. Metadata does not promise a transaction across those operations.

The readonly recipe passed four focused physical/JWT cases and strict Auth
library/tests Clippy, formatting and whitespace checks under tsk-2ed1. Its
App/Runtime installation consumer remains tsk-c2e2, not a new Auth command.
