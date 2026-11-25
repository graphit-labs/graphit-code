---
title: Verified literal user identity
content-type: reference
audience: developers
status: focused-and-fast-guards-qualified
updated: 2026-10-11
references:
  - target: {type: task, id: tsk-3582}
    relation: implements
  - target: {type: task, id: tsk-fef1}
    relation: implements
  - target: {type: task, id: tsk-9d27}
    relation: implements
  - target: {type: task, id: tsk-31f5}
    relation: derived_from
  - target: {type: task, id: tsk-aeda}
    relation: relates_to
  - target: {type: task, id: tsk-5060}
    relation: supports
  - target: {type: task, id: tsk-0e19}
    relation: relates_to
---

# Verified literal user identity

This source is active and passed its focused Linux qualification. It adds an opaque borrowed
`VerifiedUserSubject` from existing real Broker proofs, with separate Native and
Incoming and Web origins. It has no input constructor for caller-selected issuer/sub,
Clone, Debug, serde, Role conversion or USER storage grant.

`RoleIssuer::user_from_native` takes a verified native proof, the same verifier
and the original RequestContext. Native JWT issuer and stable sub are literal
values, not a username/profile name or display claim. Existing proof checks
retain the original UTC and monotonic expiry anchors; they perform no UserInfo.
`user_from_incoming` uses the actual locally verified access proof. Initial
verification and rechecks conserve signed expiry and original request identity.
New requests validate their JWT locally with cached trusted keys; UserInfo is
not an authentication prerequisite or instant revocation mechanism.

[Native installation authority](native_installation_authority.md) instead qualifies an
existing Runtime caller through the issuer’s effective active Broker profile and
an explicit local installation policy. It retains the original physical observation
and signed JWT proof; `VerifiedUserSubject` is not its permission input. Ordinary
Source/role permissions and PROJECT authorization remain separate.

`RoleIssuer::user_from_web` borrows the actual `AuthenticatedWebCaller`,
the same verifier and original RequestContext. This distinct Web proof validates
the registered client, discovery access audience and Broker API resource
(`issuer/v1`), with signed access
token verified locally until expiration. Its issuer/sub remain literal. Rechecking
the subject performs local proof checks; a new request must obtain a new proof.
It is not an IncomingMcp proof, Role, PROJECT decision or USER storage grant.

All `check`, `issuer`, `subject` and `origin` getters recheck the original
context/verifier/proof. Same sub under different literal issuers identifies
different users; no endpoint or case normalization merges them. Wrong verifier,
new SnapshotContext, cancellation and proof expiry deny output. A standalone
identity wrapper remains identity only and may exist when service policy denies
every operation.

Native and Incoming [service leases](service_credentials.md) return an optional
borrowed subject whose getters retain the full live lease gate: physical or
policy invalidation denies all subject output. Direct, static Broker and
Anonymous leases return None; Runtime/Local Actor and Role never become USER.
The private service-subject proof is Sync, as is its concrete service lease.
This permits borrowing the live subject in Send async consumers; it adds no
Clone, serialization, cross-request lifetime or storage authority.

ReadUser and MutateUser authorization are separate decisions. Identity conveys
neither permission, namespace, root, filesystem scope nor USER data access.
ProjectRecordAuthority is PROJECT-only. [Core USER capture](native_user_scope.md) and the separate
[authenticated USER Memory admission](user_record_admission.md) now provide
the physical/permission boundaries. Records-owned USER storage and consumers
remain necessary; the identity wrapper introduces no storage placeholder.

Source is `crates/graphit-auth/src/issuer/user_subject.rs`; fixtures in `issuer/tests/user_subject.rs` use real signed Native/Incoming JWTs and TCP metadata/JWKS/UserInfo, literal same-sub/different-issuer controls, original context/verifier failures and live service-subject invalidation. Public rustdoc examples deny construction/serde/Actor/Role conversion and lifetime escape.

The Web identity extension is present in Main under tsk-fef1. Its focused Linux
qualification passed all 8 subject tests (5 existing and 3 Web), this module's
8 rustdoc examples, strict Auth all-target Clippy and scoped formatting. The
initial Web JWT fixture lacked the discovery audience and was corrected; an
initial rustdoc attempt failed from disk exhaustion and its retry passed.
These results grant no PROJECT or USER permission. The separate PROJECT
HTTP/JWT decision producer is not delivered by this identity wrapper.

Current Main `tsk-3582` qualification on Linux passed 23 Broker cases, 21 native/Web OAuth cases (including five Web cases), eight verified-subject cases, three affected incoming-service cases and 50 Broker-filtered documentation tests. Strict Auth all-target Clippy and scoped formatting passed. The App-owned current browser suite passed ten cases with zero UserInfo authentication calls, optional profile and signed expiry. Historical mandatory-UserInfo receipts do not qualify this local-JWT contract; PROJECT authorization remains separate.

`RoleIssuer::user_from_authenticated` derives the literal Native identity from that exact Runtime caller's retained authenticated Native phase. It grants no USER permission. The separate [USER Memory admission](user_record_admission.md) requires USER and Memory permissions plus a matching Core physical capture; PROJECT and Role admissions remain separate. The separate admission source passed its focused Linux qualification under tsk-633b; actual Records USER storage remains pending.
