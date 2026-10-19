---
title: Web Hub mutation authority
description: Installation policy and retained Web authority for FILE effects and publication.
content-type: reference
audience: developers
status: library-focused-qualified-consumer-pending
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-63a3}
    relation: implements
  - target: {type: task, id: tsk-d75f}
    relation: used_by
  - target: {type: task, id: tsk-0af2}
    relation: uses_api
  - target: {type: task, id: tsk-e302}
    relation: uses_api
  - target: {type: task, id: tsk-7706}
    relation: uses_api
---

# Web Hub mutation authority

This contract serves installation operators and Rust FILE consumers. The Auth
producer is implemented and focally qualified under `tsk-63a3`; HTTP mutation
consumers remain under `tsk-d75f`. The bounded Linux U1 HTTP unpublish journey is now qualified separately in the
[consumer contract](hub_consumer_contract.md#web-remote-unpublish-u1-qualification);
its current Header SigV4 App strict passed in104928ms (exit0). This does not
qualify every HTTP mutation journey.

## Private installation policy

The installation's existing private `auth.json` may contain
`web_hub_administrators`. Each entry contains an exact configured provider name,
verified issuer, verified subject, and a nonempty set of `project` and/or
`global` scopes. Missing policy or an empty array denies local FILE mutation
and publication from a PROJECT source governed by this policy. It is not a
universal gate for source-free effects on a remote catalog.
An entry applies to every locally selected PROJECT for which that principal
also obtains the required current Broker PROJECT authorization. It does not
itself grant access to any PROJECT or remote storage namespace.

For example, this **fictional policy fragment** enables PROJECT operations for
one controlled test principal; it must be part of the complete
[Auth document](auth_profiles.md), retaining the installation's actual providers,
profiles, format, epoch and revision:

```json
{
  "web_hub_administrators": [
    {
      "provider": "example-broker",
      "issuer": "https://broker.example",
      "subject": "fixture-web-administrator",
      "scopes": ["project"]
    }
  ]
}
```

Operators with physical access to the installation's private Auth configuration
maintain this field using the existing private document deployment procedure.
Preserve its private permissions and complete schema, and advance
`document_revision` when changing it. No HTTP owner-registration or policy
setter is exposed. Provider/issuer/subject comparisons are literal, without
wildcards, case folding or substitution from display names. Repeated principal
tuples, repeated scopes, empty scopes, unknown fields/scopes, nulls, duplicate
fields and malformed issuers are rejected by the existing bounded Auth codec.
The ordinary document bytes, nodes, scalar and collection bounds apply.

The producer reads this field only through the captured installation
[AuthStore](auth_store.md). PROJECT configuration, request bodies, headers,
Hub metadata, the active Native profile and installation identifiers cannot
grant administration. A configured principal must still match a current Broker
provider and an actual registered `AuthenticatedWebCaller`. An unknown provider
entry grants nothing. Changing the document invalidates retained observations,
even if a change does not alter the selected provider.

## Local FILE effects

`RoleIssuer::admit_web_hub_project_mutation(WebHubProjectMutationInput)` receives
the actual Web caller, verifier, Original, explicit provider,
`BrokerProjectScope` and existing `BrokerProjectAccess` backend. The factory
requires policy scope `project`, validates the physical installation and
PROJECT root/id/version, and obtains the Broker's current Hub/read decision.
It returns an opaque borrowed `WebHubMutationAdmission`.

The authority for local effects is the conjunction of explicit private
installation administration policy and current PROJECT Hub/read. Read alone
never constructs this mutation proof. The Broker deliberately has no PROJECT
Hub/write operation; this path neither requests it nor retries another action.
Native local FILE operations classify their effects as Configure, ManageCache
and PublishedFilesystem, without MutateModule(Hub).

Install/update hydrate installation cache and update local registry/projections;
remove/recover/forget use the local publication lifecycle and its CAS/outcome
proofs. These operations do not write shared records or PUT to the remote source
PROJECT. Remote publication keeps its separate source-read/destination-write
contract below. A future shared-data effect needs its own authorization contract
before a consumer can implement it.

Consumers retain their explicit live PROJECT selection and check `is_active`
before capture, effects and delivery. Auth's physical `ProjectCapture` does not
contain that activity flag. Consumers cannot substitute a path, CWD, another
version, a mirror selected after capture, or a PROJECT that has retired.

`admit_web_hub_global_mutation` instead requires policy scope `global` and the
physical installation capture. Its proof carries no PROJECT. Global permission
does not imply PROJECT permission, and PROJECT permission does not imply Global.

Both proofs expose `request()`, `installation()`, `project()` and `recheck()`.
Before each application mutation phase, the consumer awaits
`recheck_for_effect()`, including after download and before persistent parent
creation or cache commit. Registry/projection publication is a separate phase.
Synchronous guards recheck the retained admission, Original and selected live
root/id/version through Core's publication barriers; network waits never run
under a publication lock. A confirmed cache effect keeps its actual
ArtifactChange, pending cleanup and recovery state even if a later phase fails.
For PROJECT, the asynchronous check repeats the current Hub/read decision using the same bearer
and Original, then revalidates physical captures. The synchronous `recheck()`
checks local observations and caller liveness; it does not turn an earlier SQL
authorization result into a permission lease. Authorization changing after the
last check remains an ordinary external-service race; no atomic transaction
across the Broker and filesystem is promised.

A fresh Hub/read failure, including an unavailable Broker, blocks the Web local
phase without a fallback. Native lifecycle retains its separate source-free
administrative contract. This distinction does not create a PROJECT write
permission or allow a read-only Web caller to perform a mutation.

These proofs permit the consumer's classified FILE lifecycle only. They do not
become `AdminParent`, Runtime credentials, a generic Role, Memory write
admissions, or the installed Hub read admission. Existing lifecycle effects,
uncertain outcomes, cleanup and recovery remain owned by their real consumers.

## Publication from a PROJECT source

Publishing a remote release reads the PROJECT source. It requires policy scope
`project` and a separately admitted PROJECT Hub/read source from the same
issuer instance, registered Web caller, verifier, provider and Original.
`admit_web_hub_publication(WebHubPublicationInput)` joins these proofs in a
distinct `WebHubPublicationAdmission`; it never asks for write on the source.
Its `source()` and `recheck()` retain and revalidate that read admission.

`capture_web_hub_write_service(&publication, provider)` selects the existing
`ServiceSelection` only through an exact server-owned Broker rule:
`Web / HubWrite / IncomingBearer`, with the caller's provider, current revision,
endpoint and optional resource. Foreground builds this rule for configured
Browser Broker providers; the rule alone cannot construct publication authority.
The selected credential is the bearer from that Web session and its verified
subject. No active Native profile, LocalBrokerKey, anonymous fallback or
Web-to-Runtime conversion is involved. Wrong provider, purpose, endpoint,
resource, origin or credential mode is denied.

The credential lease preserves admission/Original checks through I/O and
delivery. HubWrite authorizes transport selection; remote publication still
requires the Broker's real STS/S3 permissions, SDK integrity checks and owned
publication lifecycle. It does not create those grants or a release by itself.
See [HubWrite service credentials](hub_write_service_credentials.md).

## Source-free remote catalog effects

Remote unpublish changes only the remote catalog. It does not read or delete
SourcePROJECT, payload, cache or installation data. Publication from a PROJECT
source retains the separate administration and source-read admission above.

The [source-free remote catalog writer](hub_write_service_credentials.md#source-free-remote-catalog-writer)
captures a registered Web caller, verifier, Original, exact provider and physical
Installation. Its existing ServiceSelection retains the AuthStore observation
and service generation. It selects only Web/HubWrite/IncomingBearer and permits
External(RemoteRequest), without a new public proof or local filesystem effect.
The remote Broker and current STSv3 grants decide producer, namespace and delete
authority. HubRead, Native credentials, profiles and Standalone are not substitutes.

## Failure and lifetime boundaries

Policy mismatch denies the local FILE and PROJECT publication paths requiring
that policy. Caller/capture mismatch and changed observations deny the
corresponding admission or selection;
request cancellation or deadline remains `AuthError::Request`, and expired
verified access remains expired. PROJECT authorization preserves the backend's
typed denied, invalid response, HTTP and request failures. The HTTP consumer
maps these to its finite public error envelope without exposing private policy,
tokens, provider endpoints or storage roots.

Local JWT validation retains the existing signed-expiry window. There is no
new TTL, per-request UserInfo/introspection call or immediate remote logout
revocation claim. For PROJECT-backed local FILE effects, PROJECT authorization
is independently checked at admission and before each local mutation effect. Cookie/Origin/same-origin checks,
logout ownership, disconnect cancellation, live selection and joining retained
jobs remain the Browser consumer's responsibilities. Cancellation after an
effect does not imply rollback.

Implementation: `crates/graphit-auth/src/issuer/web_hub_mutation.rs`, the bounded profile codec, the existing service-selection gate, and the Foreground startup rule. Focused fixtures live in `crates/graphit-auth/src/issuer/tests/web_hub_mutation.rs`, registered under the existing `broker_records` test module. They exercise the actual private store, registered Web/JWT verifier and PROJECT authorization HTTP boundary with controlled Graphit fixtures. No external agent account, subscription or binary is needed.

The coordinator's integrated strict51z passed for Auth, App, Data, Core and FS
with `--lib --test managed_roles -- -D warnings` (exit0, wall43859ms). The receipt
links its result and compiler log. This qualifies lint/type composition of that
snapshot; it does not prove HTTP mutation execution or complete the migration.
The producer remains `tsk-63a3 COMPLETED r42`, with 92 runtime functions and two
compile-fail proofs unchanged. HTTP mutation qualification remains in `tsk-d75f`.
The unchanged readonly and local-JWT producers retain their own qualification
receipts.
