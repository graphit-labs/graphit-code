---
title: Web installed Hub read admission
description: Explicit Web authority for installed Agent, Skill and Rule readers.
content-type: reference
audience: developers
status: implemented
updated: 2026-10-17
references:
  - target: {type: task, id: tsk-0074}
    relation: supports
  - target: {type: task, id: tsk-d75f}
    relation: used_by
  - target: {type: task, id: tsk-e302}
    relation: implements
  - target: {type: task, id: tsk-0af2}
    relation: implements
  - target: {type: task, id: tsk-58db}
    relation: uses_api
  - target: {type: task, id: tsk-69f2}
    relation: supports
---

# Web installed Hub read admission

`IssuerAuthorityPolicy::with_web_installed_hub_read(provider)` explicitly
permits installed Hub reads for one server-configured Web provider. The default
is deny. Provider names must be nonempty, unique and within the existing issuer
scope-string limit. USER, PROJECT and imported AST/Knowledge permissions do not
imply this authority. Foreground enables it explicitly for configured Browser providers, independently
of whether Knowledge conversations are enabled.

`RoleIssuer::admit_web_installed_hub_read(caller, verifier, original, provider,
installation)` accepts the actual registered `AuthenticatedWebCaller`, its
verifier and Original request, and a physical Installation `ScopeCapture`.
The physical Auth provider must match that proof's Broker binding and revision.
A profile or Native identity is not required. A valid JWT alone does not select
or authorize storage.

The opaque `InstalledHubReadAdmission` borrows those proofs and retains the
Auth store's physical observation after discarding its decoded document.
`request()` and `installation()` expose borrowed data; consumers call
`recheck()` before I/O and delivery. Rechecks preserve the original deadline,
cancellation, signed expiry, installation identity and current Auth observation.
The factory grants no mutation, generic service, USER or PROJECT conversion.
Explicit HubRead transport capture below requires an additional service rule.
PROJECT access keeps its independent [authorization](auth_broker_project_records.md).
Web FILE mutations and remote publication require the separate
[installation mutation policy and proofs](web_hub_mutation_authority.md);
this read admission and its provider-level enablement grant neither.

`InstalledHubReadAdmission::check_project_source(&project)` joins a separately
admitted PROJECT Hub/read source with the installed-artifact reader. Both proofs
must retain the same issuer instance, Original snapshot, actual registered Web
caller instance, verifier instance and configured provider name. Knowledge,
Incoming, Runtime/Local, Role, another Web caller or another Original cannot
substitute for this PROJECT Hub proof. The method rechecks both existing
admissions; it performs no network request, creates no permission and converts
neither proof. Resolving a PROJECT descriptor requires this join in addition to
the actual physical source capture. The paired check is qualified under
tsk-01eb. The two affected Auth PROJECT-join guards also passed in
`auth-hub-project-join31zk`.

## Installed Native inventory

The Foreground Web consumer `POST /api/hub/installed/native` uses this admission
and the Source server's physical Installation capture to read its local global
registry. It checks the owner, InstallationId and registry root before reading
and before delivery. Its explicit `global_installation` scope lists only installed
AST, Knowledge and Language versions with `GlobalNative` targets. It exposes
public metadata, without opening payloads or storage transport. This operation
needs no PROJECT admission, active Native profile, embedding model or GLOBAL STS.
Reading an installed corpus still requires the separate AST/Knowledge authority.

The React route `/hub/installed` uses the current Web provider/subject and
installation endpoint for its request identity. A project, version or agent
selection does not change this global inventory. Logout, account changes and
request cancellation discard obsolete results. Details and filtering retain
actual artifact versions; storage errors and denied reads are not empty lists.
See the [typed HTTP contract](hub_consumer_contract.md#installed-native-web-inventory).
The consumer's HTTP and Browser qualification is tracked in tsk-0257; Auth
factory qualification below does not establish that consumer's completion.

## Explicit HubRead transport

`RoleIssuer::capture_installed_hub_service(&read, provider)` returns the
existing service selection with its purpose fixed to HubRead. It borrows this
installed admission and requires the same issuer instance. The Broker provider
must match the admitted Web provider, its current revision, endpoint and exact
optional resource. An explicit Web Broker HubRead rule must select
IncomingBearer without an incoming principal selector. Its credential lease
uses the verified Web caller and reports a Web user subject; no active Native
profile supplies identity or credentials. Generic Web embedding authority
remains restricted to its separate PROJECT embedding contract.

`capture_installed_hub_standalone_s3_service(&read, provider, profile)` selects
an explicit Web StandaloneS3 rule instead. Its target provider can differ from
the Broker provider that authenticated the installed reader: the target must be
configured Local without Broker, and its explicit optional profile must belong
to that target at the current provider revision. No active profile is inherited.
The existing `user_from_web` proof retains the actual caller identity separately
from transport/profile data. Even an explicitly unsigned transport has a Web
subject; that identity is not itself an S3 or ACL grant.

Both selections and their finite leases retain the installed read, Original,
verifier, physical installation and current Auth observation. Checks before and
after waits, storage I/O and delivery reject changed provider/profile state,
installation replacement, cancellation, deadline and signed expiry. They do not
add UserInfo, introspection, refresh, TTL or ambient credential fallback.

The GLOBAL registry is local to the admitted installation. A remote entry keeps
its actual producer ProjectId for existing PROJECT STSv3 authorization; this
bridge creates no GLOBAL STS request or synthetic project. A PROJECT source
still needs the separate `check_project_source` join before registry access,
I/O and delivery. See [Hub transports](hub_service_credentials.md#installed-web-reads).

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

The four new `issuer::tests::installed_hub::services` tests cover these producer contracts with the existing signed OAuth/JWKS and AuthStore fixtures.

Authentication uses existing local JWT validation and discovery/JWKS caches,
including key rotation. UserInfo is optional profile enrichment; this admission
performs no UserInfo or introspection request. Revocation, account blocking and
remote claim changes may take until the access token expires to be observed.
The Broker default remains 10 minutes, configurable from 1 minute to 1 hour;
this API creates no TTL. PKCE, state, nonce, ID-token validation and token
renewal are unchanged.

The consumer must still resolve the full installed artifact tuple in the actual
Hub registry, retain the matching captured reader, and validate its physical
scope and source fingerprint. Provenance fields are data, not a PROJECT grant.
A LocalLink outside the admitted installation needs its own real authority;
this API does not allow a fallback across that boundary. Knowledge and AST
read contexts use their own admissions and mounted readers, without copying
the corpus into the agent workspace.

This factory does not by itself qualify Live preparation, native Skill/Rule
discovery, MCP tools or Browser conversation execution. Those consumers belong
to the [Live delivery](../architecture/rust-knowledge-ai-live.md).

Source: `crates/graphit-auth/src/issuer/installed_hub.rs`, issuer policy in
`src/policy.rs`, and provider composition in
`crates/graphit-app/src/foreground_source.rs`. Focused tests reuse the existing
signed Web OAuth fixture and physical Auth store; implementation validation is
recorded in tsk-0af2.
