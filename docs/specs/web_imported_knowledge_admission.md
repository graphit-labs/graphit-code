---
title: Web imported Knowledge read admission
description: Borrowed Web proof and physical installation authority, independent of USER and PROJECT grants.
content-type: reference
audience: developers
status: implemented
updated: 2026-10-12
references:
  - target: {type: task, id: tsk-58db}
    relation: implements
  - target: {type: task, id: tsk-8d63}
    relation: derived_from
  - target: {type: task, id: tsk-5995}
    relation: uses_api
  - target: {type: task, id: tsk-41d1}
    relation: supports
  - target: {type: task, id: tsk-0e19}
    relation: relates_to
---

# Web imported Knowledge read admission

Installed Agent, Skill and Rule reads use a separate
[Web installed Hub admission](web_installed_hub_admission.md).

`IssuerAuthorityPolicy::with_web_imported_knowledge_read(provider)` explicitly
enables imported Knowledge reads for one server-configured Broker provider.
The default denies them. Empty or duplicate provider names are invalid; the
existing issuer scope-string limit applies. The policy is independent of
`with_incoming_user_permissions`, which enables only separately admitted USER
Memory/Knowledge permissions. It cannot authorize imported installation reads.
Neither policy configuration nor a valid JWT alone admits storage.

`RoleIssuer::admit_web_imported_knowledge_read(caller, verifier, original,
provider, installation)` returns the opaque `ImportedKnowledgeReadAdmission`.
The caller is the actual `AuthenticatedWebCaller` from registered Web OAuth;
its verifier and original request must be those retained by that proof. Provider
selection comes from server configuration. The physical Auth document must
contain that provider with the matching Broker endpoint and revision. The Web
proof retains its own registered client/resource; the provider's MCP resource
cannot substitute for them.

The installation argument is a real `ScopeCapture`. Admission validates its
Installation owner, ID, brand, issuer realm, binding and physical state with
`project: None`. USER and PROJECT captures are rejected. This API neither
requests a PROJECT authorization nor creates a USER namespace, profile or corpus.
It offers no mutation API or conversion to USER, PROJECT or Native authority.

The grant borrows issuer, Web proof, verifier, original, provider and capture.
Auth loads and checks the provider once at admission, discards the parsed
Auth document, and retains only its physical `LocalObservation`. Rechecks detect
configuration and installation replacement without decoding profiles again.
`request()` and `installation()` return borrowed selection data without I/O;
consumers must call `recheck()` at actual I/O and delivery boundaries. That check
preserves the original cancellation/deadline, signed expiry, policy and current
physical captures. Request termination is `AuthError::Request`, invalid/currently
expired proof is `Expired`, and denied policy or binding is `Denied`.

JWT authentication remains local with the existing trusted discovery/JWKS cache
and key rotation. Rechecks do no UserInfo, introspection, refresh or remote ACL
polling. UserInfo is optional profile enrichment. Revocation, account blocking
and remote claim changes can take until access-token expiration to be observed.
The existing Broker default is 10 minutes, configurable from 1 minute to 1 hour;
this admission introduces no TTL. PKCE, state, nonce, ID-token validation and
refresh are unchanged. See [PROJECT authorization](auth_broker_project_records.md)
for the separately enforced project ACL boundary.

Embedding and optional Rerank selection borrow this same admission. Web remote AI
requires an explicit Broker service rule retaining this provider, revision, endpoint,
resource and incoming bearer; it cannot select arbitrary Direct credentials or an
unrelated active profile. A configured matching Local rule still selects the real
active local model. Native imported reads retain their own caller's RemoteRequest
permission for Broker/Direct execution. These service leases grant no PROJECT,
USER or mutation authority and preserve the admission's existing cancellation,
expiry and physical rechecks.

Auth supplies only this installation read authority. The Knowledge import reader
in tsk-41d1 remains a separate required consumer. It must validate the global Hub
registry at this installation, artifact ID, `ConcreteVersion`, GlobalNative /
Native / Original identity and original source version, a real non-symlinked
captured directory, and the actual module snapshot/pin and EOF. Selection is data,
not authority. Auth does not choose an artifact layout or convert its version to
a project branch. No imported page, browser reference or full reference composition
is qualified by the Auth factory's tests.

Source: `crates/graphit-auth/src/issuer/imported_knowledge.rs`, policy in
`src/policy.rs`, public exports in `issuer.rs` and `lib.rs`. Focused Linux tests use
the existing signed Web OAuth server and physical issuer fixture: five cases
cover offline reads with UserInfo 503 and zero calls, borrowed getters, default
deny, foreign verifier/original/resource/issuer and USER/PROJECT/installation
captures, live configuration and installation replacement, and signed expiration,
cancellation and deadline. The grant cannot be cloned or converted to USER or
PROJECT in the three focused compile-fail documentation examples.
