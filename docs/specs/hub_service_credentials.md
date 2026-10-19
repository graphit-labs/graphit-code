---
title: Outgoing Hub read service credentials
content-type: reference
audience: developers
status: library-focused-qualified-consumer-pending
updated: 2026-10-17
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

Focused Linux qualification passed 11 HubRead, 27 existing service and 8 subject
tests (46 total), strict Auth lib/tests Clippy, scoped formatting and all 16 local
links across this specification and the service reference. Receipts are in
`.build/rust-hub-service-credentials-6873/affected-tests-final.log` and
`.build/rust-hub-service-credentials-6873/strict-final.log`. Initial invalid profile
fixtures remain recorded separately; they did not require changing profile
validation. App/Broker integration and fresh SQL/STS journeys remain pending.

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
selection/lease check revalidates the issuer, Original, verifier, installation,
provider and physical observation. Broker credentials use the existing Web
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

The isolated d75f [FILE readonly consumers](hub_consumer_contract.md#web-file-readonly) reuse this read authority. PROJECT installed list/content additionally obtain ProjectRecordAuthority Web Hub/read and join check_project_source with the same Original and physical selection. Remote content uses a temporary SDK stage; installed content verifies existing cache without hydration. Reader and publisher capabilities remain separate. The [local Web FILE mutations](hub_consumer_contract.md#web-file-local-mutations) have Source/Browser implementations. Core legitimately retains pending state through its cleanup phase; consumers must
observe completed cleanup before treating the entry as settled.

Current source distinguishes earlier-job failure from the current receiver: draining an earlier JoinError before the new spawn uses Error::PreviousWorker and HTTP500 with error.code=file_previous_job_failed, outcome.status=error, cache=not_attempted and publication=not_attempted for the current selection. It does not attribute Unknown effects before dispatch. Error::Worker from the current receiver retains HTTP500/file_job_failed with publication.change=Unknown, committed=null and observation_failed=true. The cfg(test) unit worker_failure_envelopes_keep_current_effects_unattempted_before_dispatch in browser_auth/hub_file_mutations.rs compares both finish envelopes without panic or fault injection; it subsequently compiled and passed the exact envelope unit in55h (1 passed,0 failed;16ms runner/0.00s harness;590 filtered), without a real JoinError. Rustfmt and diff check passed. Mutations55a and browser55a evidence remain valid for their observed paths without replay; no real runtime JoinError is proved, the failing Remote55h did not qualify FILE-T2; subsequent Remote55o scoped qualification is recorded below.

Mutations55a subsequently passed the exact complete selector (exit0; 1 passed, 0 failed, 0 ignored, 585 filtered; 179.30s harness/179321ms runner), using App ELF graphit_app-5fd2cc01c5f0b24e after a successful compilation-only emission (191140ms). The pending-None poll passed; the run reached post-cache private-admin retirement with cache Created preserved and registry unpublished, then actual deadline, disconnect, logout, stop and cleanup paths. A later Root-requested test-only Clippy edit changed nested if to if let with && let entry; rustfmt and diff check passed, with no functional delta and no runtime replay. This qualifies the complete mutation selector and its actual cancellation/error/shutdown cases, not an injected or real JoinError. Probe55a subsequently passed its exact harness selector (exit0; 1 passed, 0 failed, 0 ignored, 585 filtered; 125.73s harness/125746ms runner). The private result is finished=true, owners_joined=true, credentials_removed=true and browser_qualified=false, the last retained by protocol. Stat confirmed credentials absent (ENOENT), done/result0600 and the previously verified parent0700/public metadata0600/credentials0600. Root CUA55a confirmed legitimate LOCAL login: the initial Broker origin showed legitimate Access unavailable, switching to Standalone immediately cleared the stale alert, and the selected profile/provider/producer/ID/release tuple returned the correct Web FILE reader without the earlier error. Logout returned Anonymous and discarded details. This composes actual unchanged CRUD54a with the origin/reader delta CUA55a and timely lifecycle55a; it is not a CRUD replay and does not change the failed deadline54a history or set browser_qualified=true. No concrete producer details or runtime IPC values are recorded. Source PROJECT publication is assigned separately to ready leaf tsk-bcaa under tsk-d75f; this worker retains readers, mutations and FILE-T2 without overlapping that implementation. FILE-T2 was subsequently integrated only in browser_auth/tests/hub_file_reads.rs within the existing Remote selector and compiled in55h; Remote55h failed without establishing which new case or loop had been reached. The existing StorageRelay retains actual Standalone GET payload.zip after backend I/O; checked drain precedes every hold, with entered/pending observation, client45s/outer40s/Original30s, deadline504 or client abort-and-await, and release/join before the sibling operation. The disconnect path awaits its outcome, pops the owned reader, then asserts so an oracle failure cannot bypass cleanup; registry and children remain unchanged. Rustfmt edition2024 and diff check passed. Root55c strict passed (56439ms) before this integration and does not qualify the later delta. Actual archive GET deadline/disconnect/provider retirement subsequently passed in Remote55o; extraction and mirrors remain unproven. Mutations55a and browser55a evidence are preserved without replay.

The active FILE-T2 replacement check is chk-e97af21d9cac (superseding chk-8b4365ec8cea): all negative HTTP cases remain required, while recovered qualified common Selection evidence plus caller inspection may establish ambiguity-to409 before capture/I/O without fabricating two physical trees, a factory or copied stores. Root recovered common Selection proof from completed tsk-0074 revision16, checks chk-24c6e17ae4fc/chk-3d7061d7edeb: schema_selection_and_confirmed_errors_preserve_native_contract passed1/failed0 in0.02s in the historical socket library test, labeled schema-binary.log. Those logs/qualification artifacts are absent from this checkout; ELF hash, schema argv and exact historical assertion revision were not recovered. Root authorized reuse by purpose: current task_server/tests.rs537–572 checks normal selection and duplicated selection_ambiguous; web_file_reads.rs64–75 calls explicit Selection before CapturedPublicationScope, registry or payload I/O; browser_auth/hub_file_reads.rs62 maps AlreadyExists to HTTP409/state_conflict. This is historical qualification plus current inspection, not a new ambiguous HTTP execution or fabricated physical mirror. The active FILE-T2 check was recorded Failed after Remote55h; the subsequent55o evidence slice is PASS, with the failed history preserved.

The provider-retirement case, whose progress was unestablished by failing Remote55h, subsequently passed in55o: hold the actual archive GET until entered/pending, use real NativeProfileManager admin authority with expected-revision CAS and cascade=true to remove the selected storage provider and dependent profile, verify publication/cleanup/no pending intent and snapshot absence, then release/join and verify genuine Broker sibling bytes while preserving registry and children. The prior static prediction of HTTP500/file_read_unavailable through lease.check and io::ErrorKind::Other matched the actual passing55o assertions. Mutations55a/probe55a are not replayed, and the later PreviousWorker unit PASS qualifies only its finish envelopes.

Emission55g failed compilation (exit101/55715ms) on four peer-owned bcaa type errors; no tests executed and no qualification of this unit followed from that attempt. The owning author corrected those errors; re-emission55h was still in progress at that historical checkpoint. Mutations55a/probe55a remain valid within their recorded scopes, and the PreviousWorker unit had not yet run at that historical checkpoint.

Emission55h subsequently passed compilation (145703ms); its graphit_app-5fd2cc01c5f0b24e basename replaces the55a image and does not establish identical ELF contents. The exact worker_failure_envelopes_keep_current_effects_unattempted_before_dispatch unit was listed1 and passed1/failed0/ignored0/filtered590 (16ms runner,0.00s harness). It compares finish envelopes only, without a real JoinError or injected panic/fault. Root strict55i passed (66355ms; App lib/tests --no-deps -- -D warnings).

Remote55h failed (exit101;0 passed,1 failed,0 ignored,590 filtered;83825ms runner/83.80s harness) at assert_catalog_job_pending in hub_remote_catalog.rs169. The single real same-image RUST_BACKTRACE=1 diagnostic also failed at that helper (exit101;84821ms/84.79s); it yielded symbols without caller file/line and did not identify the exact loop or provider-retirement stage. Two earlier runner attempts failed before spawn because the Node process was unavailable and were not tests. At that55h checkpoint no new FILE-T2 case was PASS. Earlier-job completion and Notify permits were hypotheses rather than established causes; later diagnostic/fix evidence is recorded below. Mutations55a/probe55a and historical Selection0074 purpose-based reuse remain intact without replay.

Remote55o subsequently passed the exact listed selector (exit0;1 passed,0 failed,0 ignored,591 filtered;139979ms runner/139.96s harness). Root55o compilation passed181853ms with the new image at basename graphit_app-5fd2cc01c5f0b24e; this does not equate it with earlier images. The passing composition includes Local9316 and explicit Broker fixture55f. The fixture correction creates its local notification immediately after the hold decision and before backend I/O, then uses notify_waiters for release; entered still follows real backend status/body. This followed preserved FAIL55h/backtrace, FAIL55k (an unidentified Shared JoinSet completion at disconnect) and FAIL55m (current client200 before held release), without attributing a kernel defect or worker identity to those observations.

Focused producer tests are in `crates/graphit-auth/src/issuer/tests/installed_hub/services.rs`. HTTP/React integration remains with the consuming deliveries.

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

All existing lease getters recheck same Original, physical AuthStore observation,
service generation, current source/Role authority and native expiry/verifier
binding. Local profile/provider replacement or logout, cancellation and the
original deadline fail causally through ServiceUseError. Upstream revocation,
account blocking and claim changes can remain unseen until the signed access
token expires; no per-request UserInfo or introspection compensates for this
accepted window. NeedsRefresh belongs to
the existing administrative login/refresh workflow. WrongMode remains a typed
error; this capture performs no hidden refresh or profile mutation.
