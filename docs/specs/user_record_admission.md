---
title: USER Memory and Knowledge record admission
content-type: reference
audience: developers
status: focused-linux-qualified
updated: 2026-10-16
references:
  - target: {type: task, id: tsk-04be}
    relation: implements
  - target: {type: task, id: tsk-633b}
    relation: implements
  - target: {type: task, id: tsk-438e}
    relation: implements
  - target: {type: task, id: tsk-319e}
    relation: uses
  - target: {type: task, id: tsk-8c29}
    relation: derived_from
  - target: {type: task, id: tsk-7fc2}
    relation: supports
  - target: {type: task, id: tsk-0e19}
    relation: implements
  - target: {type: task, id: tsk-69f2}
    relation: implements
---

# USER Memory and Knowledge record admission

`RoleIssuer` admits USER Memory for the current call's verified literal
issuer/subject and both required permissions. READ requires `ReadUser` and
`Read(Memory)`; mutation requires `MutateUser` and `WriteMemory` or
`WriteFencedMemory`. USER is independent of PROJECT, profile display names,
Actor attribution and service-purpose authority. A valid JWT or a
`VerifiedUserSubject` alone grants no USER storage access.

For Native calls, authenticate the existing Runtime caller with
`AuthenticatedCaller::authenticate_native`. `RoleIssuer::user_from_authenticated`
derives identity from that exact retained phase, including its physical profile
observation and signed expiry. The new factory accepts no caller-selected
subject that could be paired with a different authenticated caller. Ordinary
Runtime permissions remain unchanged. A Local/static key or a Role without
this Native phase cannot become USER.

`admit_authenticated_user_memory_read(caller, original, user_scope)` and
`admit_authenticated_user_memory_write(caller, original, user_scope, operation)`
apply the actual caller's existing policy. The issuer and original request must
be the same ones retained by that caller. `user_scope` is the existing
`ScopeCapture` data pairing a real [Core USER capture](native_user_scope.md) with
its binding. The USER owner must match the verified literal identity, issuer
installation ID, brand and actual installation birth root.

For a Native Runtime controller with an existing live `AdminParent`,
`parent.admit_native_user_memory_fenced_write(original, user_scope)` returns
the same `UserWriteAdmission`, restricted to `FencedMemory` and
`MemoryFenceRequired`. This operation combines two distinct existing policies:
the retained caller must authorize ordinary `MutateUser`, and its Parent must
authorize administrative `WriteFencedMemory`. An administrative grant never
adds `WriteFencedMemory` to the ordinary caller, and a Role Dream grant remains
unable to read or mutate USER storage.

The admission borrows that exact Parent and its actual Native caller. Every
barrier rechecks Parent session state and expiry, both policy halves, the same
issuer/Original, Native profile observation and locally verified signed token,
literal identity and the existing physical USER binding. Wrong identity,
installation, PROJECT scope, absent Native phase, Local origin, ended/expired
Parent, changed profile, cancellation or deadline deny before storage effects.
The actor remains the actual caller's existing attribution. Parent session
invalidation is a local administrative check; it adds no token revocation
polling, blacklist, introspection, refresh or UserInfo request. Access tokens
remain trusted until their signed expiry under the existing JWT contract.

Internally, each UserIdentity construction and admission barrier obtains one
checked literal subject identity and compares its issuer and subject together.
That value is local to the synchronous check: it is not cached across stages,
callbacks or awaits. Public identity getters still perform their own current
validation. This removes duplicate identity reconstruction inside a single check
without changing Native, Parent, policy, Original or physical USER validation.

The controller retains the Parent while using this borrowed handle. This factory performs no Memory CRUD, proposal/fold, CAS or recovery. The existing USER Memory SDK still enforces fencing and confirmation; planning reads require their independent Native `ReadUser` plus `Read(Memory)` admission. This does not by itself deliver the Runtime/daemon USER Dream consumer.

For Incoming calls, the server explicitly configures
`IssuerAuthorityPolicy::with_incoming_user_permissions(provider, permissions)`.
The default has no Incoming USER permissions. Rules accept the finite USER,
Memory and Knowledge permissions: ReadUser, MutateUser, Read(Memory),
WriteMemory, WriteFencedMemory, Read(Knowledge) and MutateModule(Knowledge).
They reject other domains and duplicate providers or permissions. Configuration is
data and does not authenticate callers. The provider name is explicit in
`admit_incoming_user_memory_read(caller, verifier, original, provider, user_scope)`
and its write counterpart with an operation argument.

The Incoming proof must come from that request's actual `BrokerVerifier`.
The configured Broker provider's current literal endpoint, MCP resource and
revision must match that verifier and proof; the mature JWT producer enforces
its registered client, discovery access audience and security contract.
The admission retains the AuthStore physical observation and rejects a changed
configuration. It does not select the daemon's active profile, borrow Native
credentials or require an active user profile. Incoming A cannot read B's
namespace even when B is the daemon's active profile. HubRead/HubWrite and
Inventory policies do not substitute USER or Memory permissions.

`admit_web_user_memory_read(caller, verifier, original, provider, user_scope)`
uses the actual distinct `AuthenticatedWebCaller` from the registered browser
client. It shares the explicit inbound provider USER policy above, requires
ReadUser and ReadMemory, and returns the same opaque UserReadAdmission. It never
converts Web into an Incoming MCP proof.
The mature Web proof validates its registered client, discovery access audience
and its own Broker API resource. The existing provider predicate compares the
real endpoint/revision; it does not borrow the provider MCP resource as a Web
grant. Captured AuthStore/USER/installation state and the original request are
rechecked as in the other admissions. No active CLI profile or UserInfo is
required. This Web extension passed its separate Linux qualification under tsk-5995:
four new Web cases, eight Native/Incoming admission controls, eight subject
guards and two local PROJECT/Web provider-predicate guards (22 distinct). Strict
Auth lib/tests Clippy, scoped formatting/diff and documentation checks passed.
App browser/reference composition remains a separate required consumer.

`admit_web_user_memory_write(caller, verifier, original, provider, user_scope,
operation)` admite `Memory` ou `FencedMemory` somente quando a política do
servidor habilita explicitamente esse provider com `MutateUser` e a permissão
de escrita correspondente. Uma regra somente de leitura e ausência de provider
continuam negando escrita. O retorno é `UserWriteAdmission`; os mesmos controles
de principal Web, Original, AuthStore e captura do próprio USER permanecem.
Esse caminho não lê ou muda PROJECT/Task, nem usa outra identidade da instalação.
`FencedMemory` preserva a obrigação `MemoryFenceRequired` do store.

O ator Web é produzido pelo servidor com installation e issuer/sub verificados,
estável para o principal e independente de username/display name. O helper é
compartilhado com a escrita PROJECT Web; Native e Incoming conservam sua
atribuição existente. JWT válido, identidade literal e Actor continuam sem
conceder permissão por si mesmos. Esta API Auth não completa as rotas HTTP CRUD
nem o frontend consumidor.

USER Knowledge has separate factory pairs for Incoming MCP and registered Web
callers. `admit_incoming_user_knowledge_read(caller, verifier, original, provider,
user_scope)` and `admit_web_user_knowledge_read` with the same arguments require
both `ReadUser` and `Read(Knowledge)`. Their corresponding
`admit_incoming_user_knowledge_write` and `admit_web_user_knowledge_write` require
both `MutateUser` and `MutateModule(Knowledge)`. Neither pair implies the other.
Memory permissions and `UserReadAdmission`/`UserWriteAdmission` cannot substitute
for `UserKnowledgeReadAdmission`/`UserKnowledgeWriteAdmission`.
There is no Native Knowledge constructor. Runtime/Local callers, Roles, Actors
and a caller-selected identity cannot enter these typed factories.
Incoming requires its real MCP access proof and resource; Web requires the
registered browser access proof with its own client/resource binding. Each proof
retains its exact verifier and Original, without conversion between caller kinds.
Both use the explicit server provider USER policy and require no active Native
profile or UserInfo call. USER Knowledge admission does not authorize
`External(NativeLaunch)`; execution requires its separate current authorization.

Both Knowledge admissions reuse the same live identity, provider observation,
installation/brand/realm and physical USER checks below. Their `user`, `identity`
and `request` getters recheck; read `recheck` returns `()`, write `recheck` returns
`Obligation::Unconditional`. The owner is installation ID + verified literal
issuer/subject, so a fresh proof under a new issuer epoch reaches the same USER
namespace. The epoch remains an ephemeral authority gate, never durable identity.
Each authorized PROJECT Knowledge turn still requires its independent fresh
[PROJECT admission](auth_broker_project_records.md); USER conversation metadata,
IDs, selected slugs or model names confer no PROJECT permission. Stateless Ask
remains separate. This Auth producer does not qualify the private Knowledge
conversation store or its AI/App integration.

The private service-subject trait requires `Sync`, which the actual
`ServiceCredentialLease` satisfies through its borrowed proofs and existing
synchronized observation. USER admissions can be borrowed by Send async
consumers without another executor or a Mutex around the grant. They remain
opaque, non-cloneable authority tied to their original request.

Opaque `UserReadAdmission` and `UserWriteAdmission` expose live-checked `user`,
`identity` and `request` getters. Read `recheck` returns `()`; write `recheck`
returns the existing obligation. `UserMemoryWriteOperation::FencedMemory`
requires `MemoryFenceRequired`, which Records must enforce. `operation` is
checked before returning the mutation classification. `actor` is attribution
after both permissions pass: Native uses the actual caller's Actor, and
Incoming and Web use server-minted attribution of the installation and verified
literal principal, with their respective actor domains.
It never derives a principal or grant from an input Actor.
`Actor::identifier()` exposes canonical 26-character ULID attribution from the
first 128 digest bits. Records authorship, idempotency and caller binding use
that identity. It neither encodes the full SHA-256 value nor a creation time;
USER's installation/issuer/subject namespace and current admission checks remain
separate and mandatory.

Every barrier checks the original cancellation/deadline, caller proof and
permissions, literal identity, captured configuration and USER physical binding.
Permission denial precedes Memory payload or namespace creation. Locally
verified JWTs remain valid until signed expiry; rechecks do no UserInfo, refresh,
introspection or network fetch. New requests use the existing trusted JWKS cache
and actual verifier. UserInfo is optional profile enrichment.

Source is `crates/graphit-auth/src/issuer/user_records.rs`, the finite policy
builder and the authenticated identity helper. The existing PROJECT provider
validator explicitly rejects USER. Focused fixtures use actual signed JWTs,
AuthStore configuration and Core captures, including permission combinations,
Native/Incoming identity, incoming A with active B, missing profile/default deny,
foreign Original/verifier, config/logout, physical replacement, expiry and
cancel/deadline. Focused Linux qualification passed eight USER cases, 13 Native installation
guards, eight subject guards and four new opaque admission rustdoc examples.
Strict Auth lib/tests Clippy, scoped formatting/diff and documentation checks
passed; receipts are recorded by tsk-633b.

Records-owned USER Memory SDK read/write/list/search/history/references and
`include_user` consumers remain required. Their generation must express USER
identity and the real SDK pin without manufacturing ProjectId/GitVersion.
This Auth admission delivers no corpus, search result or reference projection.
See [verified literal user identity](verified_user_subject.md) for the separate
identity-only API.

The tsk-0e19 focused Linux run passed 17 USER admission cases, including four
Knowledge cases and one actual type-bound guard. Knowledge covers Incoming A
with daemon profile B, separate permission halves and Memory-only denial,
foreign proof/provider/Original/USER/installation, provider and physical
replacement, current expiry/cancellation/deadline, zero UserInfo calls and
stable owner under a new issuer epoch. Seven focused compile-fail examples
reject Memory conversion, Runtime/Local input, cloning, restoration and exposed
mutation diagnostics/serialization. Existing Memory APIs retain their behavior.
