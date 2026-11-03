---
title: Rust Hub caller scopes, bindings and resource policy
type: reference
status: proposed
updated: 2026-10-29
references:
  - target: {type: task, id: tsk-0074}
    relation: supports
  - target: {type: task, id: tsk-bcf2}
    relation: implemented_by
  - target: {type: task, id: tsk-a456}
    relation: implements
  - target: {type: task, id: tsk-9a88}
    relation: implements
  - target: {type: task, id: tsk-16f7}
    relation: implements
  - target: {type: task, id: tsk-5612}
    relation: relates_to
  - target: {type: task, id: tsk-d654}
    relation: relates_to
  - target: {type: task, id: tsk-e849}
    relation: relates_to
  - target: {type: task, id: tsk-3ddb}
    relation: derived_from
  - target: {type: task, id: tsk-7a69}
    relation: derived_from
  - target: {type: task, id: tsk-1833}
    relation: relates_to
  - target: {type: task, id: tsk-84f3}
    relation: relates_to
  - target: {type: task, id: tsk-ced2}
    relation: relates_to
  - target: {type: task, id: tsk-ad05}
    relation: relates_to
  - target: {type: task, id: tsk-e493}
    relation: relates_to
  - target: {type: task, id: tsk-7a13}
    relation: relates_to
  - target: {type: task, id: tsk-c3bf}
    relation: relates_to
  - target: {type: task, id: tsk-0257}
    relation: relates_to
  - target: {type: task, id: tsk-47fd}
    relation: relates_to
  - target: {type: task, id: tsk-d75f}
    relation: relates_to
  - target: {type: task, id: tsk-9316}
    relation: implemented_by
  - target: {type: task, id: tsk-bcaa}
    relation: implemented_by
  - target: {type: task, id: tsk-b788}
    relation: relates_to
  - target: {type: task, id: tsk-4323}
    relation: relates_to
  - target: {type: task, id: tsk-376b}
    relation: relates_to
  - target: {type: task, id: tsk-520d}
    relation: derived_from
  - target: {type: task, id: tsk-97d9}
    relation: derived_from
---

# Rust Hub caller contract

This W analysis defines the next Rust consumer implementation. These requests,
defaults and bindings are **planned** except where an implemented consumer is
explicitly identified below. Existing React tests remain active. The pure renderer and owned publication libraries
already exist; their completion does not deliver CLI, HTTP, MCP or daemon cutover.
No old registry/config import, format converter or upstream fallback is part of this design.

## Sources and maintained journeys

| Consumer boundary | Required behavior |
|---|---|
| CLI and MCP requests | Capture one selection/version and preserve ordered batch results; reject malformed or unknown input instead of silently filtering it. |
| HTTP and React | Preserve captured project/agent and alias intent, loading/error state, latest-response guards and workspace invalidation; pending is distinct from success. |
| Daemon replacement | Cancel and drain work before closing or replacing its resources; no successor may race an old owner. |
| Imported module/role context | Keep static selected context separate from runtime memory and retain producer identity, version and source. |
| Publication results | Expose committed, pending and observation failures; batches include failed and not-attempted items. |

Install, alias change, update, sync, remove, recovery and
explicit Forget use the same projection derivation. Rules remain dynamic;
AST/Knowledge/Language use their domain owners, not a generic native file copier.
Broker grants and Data hydration must be real before remote payload use.
Membership, a path, a decoded pin or equal bytes never grants access or ownership.

## Scope and selection

The application boundary captures brand, installation identity/paths, OS home,
provider home, process working directory, environment, subject/grants and
cancellation once. A request captures its project identity, checkout binding,
version, config snapshot and resolved native binding before work. Libraries do
not discover these through environment or process CWD. Relative CLI paths resolve
against that one captured CWD; server requests require absolute paths or a
server-resolved authorized project ID. Shared records use project identity and
relative sources; runtime native paths are local observations, not authority.

Each boundary selects scope explicitly. The local lifecycle DTO requires
`scope: project`, `global_cache` or `global_native`; the CLI emits that value
from `--scope` (default `project`). FILE installation keeps its explicit `global`
boolean. Project selection carries `project_dir` or
authorized `project_id` (exclusive), and optional `project_version`. An empty,
null, malformed or conflicting supplied value is invalid, never omitted/global.
Global rejects project version and host selection because it installs a concrete
version in the version-keyed cache, without native project projection. Global
FILE also rejects alias, entry name and any supplied physical `mode`, including
`copy`, before cache or native publication effects. Its SDK metadata uses
`InstallationTarget::GlobalCache` (`{"kind":"global_cache"}`) and
`InstalledBackend::GlobalCache`, with one Original installation and no selection
binding. The source must be `HubCache` in the same installation root, retain its
full physical ID, and include its validated `RemoteFileOrigin`. That provenance
selects the origin to reauthorize; it is not an access grant. AST and Knowledge
cannot use this target. A Language FILE cache does not replace the separate
`GlobalNative` Language corpus contract.

FILE install and update use physical copies for PROJECT projection. The optional
`mode: "copy"` requests the same mode explicitly. Null and other values fail
validation. Core/Hub retains the captured expected installation and
`same_source` check. Omitted alias and entry name preserve their existing values
on update. The destination copy has no dependency on the source's physical path.


## Requests and actual results

Project mutation fields are `scope`, `host` (optional resolved selection), `type`
(required closed artifact type), `id` (qualified real ID), optional concrete
`version`, optional `alias` and explicit `mode` where applicable. An `id@version`
input is parsed once; a separate disagreeing version is invalid. Duplicate keys,
unknown fields, nulls and ambiguous type are invalid. Alias absence retains the
operation's explicit stored-name semantics; supplied empty alias is invalid.
Stable identity is type/realID plus the surface's installation binding, not alias.
SourceRef retains producer, concrete version and one source root. Its artifact
directory is derived for its declared origin; a distinct cache physical ID is
retained only when different from the logical ID. Version
selection and grant acquisition precede materialization. Updates preserve the
stored alias unless the request changes it. A PROJECT destination receives its
own copy of the original or rendered content.

Removal/recovery/Forget use the exact typed installation target returned by the
registry: `target={kind:native_binding,binding:kiro-v3}` for an engine-specific
installation, or `target={kind:shared_adapter,adapter:kiro}` for a common skill.
Validate target/type compatibility before effects. Engine unregister is a distinct
operation targeting a native binding and never implies shared-artifact removal.
Install selection derives and returns this target; it cannot silently reinterpret
a binding-specific removal as a shared removal. A batch builder coalesces equivalent
requests for the same shared target before calling the existing coordinator.
Equivalence requires equal type/ID, source/concrete version, alias/name, mode and
operation; incompatible requests collide during preflight. Retain every original
input index and its `coalesced_with` primary index/result, with effects counted
only on the primary operation. Do not invoke two publishers for the common skill.

Responses include resolved scope/project/version, selection and configuration
origin, artifact/operation identity, `state`, `change`, physical-effect observations,
pending/cleanup state and structured errors. `state` is `completed`, `pending`,
`failed` or `not_attempted`; an unchanged verified result is completed with
`change=verified_unchanged`. Pending is never success, even if native bytes changed.
Errors retain `committed` and unknown observation separately; unknown is not zero
effects. Forget returns the bounded
preview/provenance/residue description and logical-only abandonment result;
the action never deletes those paths or silently releases another operation.
The application adapter handles successful library `PublicationChange::Unsupported`
explicitly: it becomes public `failed/unsupported_native_capability`, not completed.
The existing batch also puts Unsupported in Completed; normalize that branch,
before MCP/CLI/HTTP success classification.

Batch results preserve input index, key, disposition and per-item errors. A
preflight failure has no effects; later failure retains prior completed effects.
Cancel/deadline stops new items, marks remaining items not_attempted, and still
records an already-applied effect. Recovery runs before reconciliation/mutation;
an uncertain effect requires the own-operation proof defined by
[shared-document transactions](native_shared_documents.md), not a blind retry.
The same-key pending operation may be recovered or explicitly forgotten;
another-key pending operation cannot be discarded by that request.

| Error class | HTTP | MCP/CLI meaning |
|---|---|---|
| invalid request/config/document | 400 | Structured invalid code and source; no fallback. |
| payload/body limit | 413 | `limit_exceeded`, budget category, no secret/input echo. |
| unauthenticated / denied grant | 401 / 403 | Subject/grant failure before protected reads/effects. |
| missing authorized artifact/context | 404 | Not an empty successful installation. |
| collision, changed source/ownership, pending | 409 | Reload state; recover or explicit Forget where applicable. |
| unsupported native capability | 422 | Exact surface/binding limitation remains visible. |
| deadline | 408 | Actual effects/pending preserved; no claim of hard native IO timeout. |
| local IO / failed reconciliation | 500 | Redacted cause, actual effect state, safe continuation. |
| admission busy | 429 | Bounded dispatcher rejected before domain effects; retry later. |

MCP application failure uses `isError=true` plus the same typed result; ordinary
domain failures are not JSON-RPC protocol errors. CLI exits 0 only when every
requested item completes with no pending cleanup; invalid input exits 2, other
failed/pending/partial results exit 1, interrupted cancellation exits 130. Output
data goes to stdout; diagnostics/progress go to stderr. An already-disconnected
HTTP client cannot receive a result: persist necessary reconciliation evidence
and make it discoverable on the next authorized state read. Do not require a
fictional HTTP status for a closed connection. Cleanup obligations survive cancel.

## Explicit finite resource profile

The Rust policy resolver provides explicit content, document and metadata limits
with configuration provenance. The application supplies one shared request
context; limits apply at their actual decoder, renderer and writer boundaries.
These defaults can be tuned for the maintained workload.

Each suffix below is a flat key under `hub.limits.`. Values are positive JSON
integers; byte quantities use bytes, durations milliseconds. Resolve each key
Command → Environment → Project → Global → Distribution, retaining the winning
`ConfigSource`; missing fields use `DistributionDefault`, distinct from a supplied
Distribution layer. Never merge
invalid winners, clamp numbers or convert 0 into automatic mode. All u64→usize,
MiB→bytes (if a UI accepts display units), totals and Instant arithmetic are checked.
Validate the complete coherent policy before any IO/lock. Constructors' lower
limits/hard bounds still apply; depth for native content cannot exceed 80.

| Key suffix group | Defaults / units | Reason and boundary |
|---|---|---|
| `registry.bytes, entries, key_bytes, json_nodes, string_bytes` | 16777216, 10000, 65536, 1000000, 16777216 | Concrete encoded document, entry, key and JSON bounds; entry count stays below the100000 maximum. Admission does not preallocate ceilings. |
| `projection.identity_bytes, path_bytes, fallback_entries, name_bytes, batch_items` | 65536, 65536, 10000, 16777216, 1000 | NativePath's 64 KiB unit-byte representation; enumeration/batch finite. Native filesystem may impose smaller names. |
| `artifact.bytes, key_bytes` | 16777216, 65536 | Actual encoded catalogue and key input bounds. |
| `copy.bytes, nodes, path_bytes, depth` | 536870912, 100000, 16777216, 128 | Concrete per-publication tree bounds; assets stream without retaining file bodies. |
| `snapshot.bytes, nodes, path_bytes, depth` | 67108864, 10000, 1048576, 128 | Bounded one source capture, admits maintained and MP workloads. |
| `retained.bytes, files, path_bytes` | 8388608, 16, 1048576 | Retain only explicitly needed descriptor inputs; other assets stream. |
| `lineage.schema_bytes, selection_bytes, total_bytes` | 128, 256, 4096 | Concrete transient schema/name fields and source-fingerprint bounds; output is validated by the real Core plan, not a duplicate digest. |
| `content.input_bytes, output_bytes, depth, nodes, scalar_bytes, collection_entries` | 8388608, 16777216, 32, 100000, 1048576, 10000 | Input/output/nodes aggregate per request; scalars and collections checked individually. Failed work is not refunded. |
| `shared.input_bytes, output_bytes, journal_bytes, nodes, depth, scalar_bytes, entries, hook_tuples, references, documents` | 1048576, 2097152, 4194304, 100000, 32, 1048576, 4096, 1024, 10000, 1000 | Shared-document and journal limits preserve refsets; one pending intent per document binding. |
| `request.deadline_ms` | 120000 | Cooperative deadline shared across capture, render and publication; cancellation uses the same Core snapshot context. |
| `transport.bytes, nodes, depth, scalar_bytes, collection_entries, inflight` | 1048576, 100000, 32, 1048576, 10000, 32 | Independent request/frame body quota and bounded dispatcher admission before decoding; not snapshot/registry IO. |

Transport body/frame bytes are capped while reading, including a supplied or
absent Content-Length; structural nodes/depth/scalars/collections are capped by
the decoder before allocating unbounded JSON. Admit at most the captured positive
inflight count, with no unbounded waiting queue; excess calls return busy before
domain work. HTTP authenticates before protected reads; an unread oversized body
can still be rejected without parsing it. MCP decoding shares these independent
limits with HTTP; packet/JSON-RPC framing failures remain protocol errors rather
than completed Hub mutations. Batch items also obey the separate batch limit.

Default CopyPolicy rejects directory links, preserves the qualified root-file link
policy and native access policy explicitly; no permission mutation of a Link source.
Default execution serializes mutating jobs per captured project/shared document;
independent reads may proceed. The application captures the policy once; library
constructors receive its explicit limits. RequestContext::new receives ContentLimits,
cancellation and deadline, validates the renderer limits and shares the same
Arc<SnapshotContext> across capture and publication. SnapshotContext holds an
optional cooperative deadline and the permanent cancellation signal; it does not
reserve an aggregate IO quota. `SnapshotContext::new` is finite, while
`SnapshotContext::service(Cancellation)` and `RequestContext::service` represent a
continuous operation with `deadline() == None`. `RequestContext::check` observes
cancellation and finite expiry, and `remaining()` returns an optional duration.
Scope capture, native readers and lock waits preserve this option.

Finite publication and interactive-login protocols call `required_deadline()`
and reject a service context with their causal invalid-input/policy error before
acquisition or listener I/O. The getter checks cancellation and expiry first.
No sentinel Instant, default TTL, Original rebasing or extra permission is used.
A service context cannot extend signed JWT expiry or turn a valid JWT into PROJECT
authority. Foreground service supervision is a separate integration consumer.

Cancellation is shared across renderer, snapshot and publication. The remaining
deadline reaches both lock acquisitions from the same RequestContext; after native
effects the Core receipt is mandatory. Hub commit, attach and cleanup can remain
observably pending for a later recovery; there is no synchronous noncancelable
cleanup guarantee. Budgets are admission bounds, not
a promise that native IO can always be interrupted. No abandoned pending journal
is deleted on a TTL. Secrets/native launch evidence use protected local storage
where necessary and never Task, shared project docs or public result payloads.

## Authority prerequisites and correction packages

Verified daemon/Broker authority issues a role credential; native role bodies,
headers, environment values and CLI role arguments cannot mint or widen grants.
Verification binds role, authorized project/scope/version/binding, grant revision,
expiry and run where applicable, before listing or dispatch. Refresh/revoke and
daemon/profile replacement invalidate the relevant sessions. Stdio proxy preserves
the verified credential; missing credentials cannot fall back to a broad daemon
key. Native configuration may reference protected runtime delivery, but static
role content never contains the bearer. Read-only domain calls may use controlled
internal caches without business mutation; install/sync/leases/config and domain
writes remain mutating regardless of name or annotation.

Use the current trusted Runtime/role and Native identity producers rather than
a profile label or an instruction body as authority. GlobalNative mutation uses
the existing Native installation proof; remote payload operations separately
require their real Hub purpose, PROJECT grant and storage credential. Registry
version indexing supplies selection, never any of these grants. A completed
analysis or producer proof does not qualify a missing product consumer.

| Delivery | Required correction before its own implementation claim |
|---|---|
| `tsk-ad05` native identity | Native and shared typed targets, exact captured binding rebuild, registry validation/discard/hash receipts, RC-only Agents, membership fallback and exact native layout. Caller coalescing belongs to 0257. |
| `tsk-e493` resource policy | Implemented library resolver for every profile field, origin, checked conversion and shared RequestContext. Independent Shared/Transport packets need actual writer/decoder/admission enforcement; calibration remains pending. |
| `tsk-bc28` MCP/shared writer | Managed shared-document writer and MCP planner; current Kiro V3 binding, preserve foreign settings and report actual publication/recovery state. |
| `tsk-aa20` hooks | Kiro V3 profile YAML composition and standalone v1 hooks; S provider→Hub→Core order, Kimi conservative refs/retarget/Forget tombstone; shared assets survive engine unregister. |
| `tsk-47fd` MCP transport and dispatch base | Source Root discovery exposes available handler metadata and authorizes each call; Role discovery verifies the credential and filters by grant. The existing Source/Role servers and stdio proxy are qualified by their concrete consumers. No new shared registry/builder is required; public catalogue, Hub and Role consumer gaps remain with their own deliveries. |
| `tsk-97d9` complete MCP role integration | Retains all catalog/Coordinator/journey obligations and waits for public handlers d75f and actual domain producers. Reconcile parent description under its own claim; core→handlers→PF integration avoids the semantic cycle. |
| `tsk-9bd4` managed runtime | Current Kiro V3 binding, protected verified role credential delivery and actual Records producers; static/runtime context split; no inferred engine or persisted live memory. |
| `tsk-0257` CLI/application | 16f7's exact version index, 5612's native semantic installation and 7a13's real remote producers before the corresponding code; shared typed target/coalescing, shared physical root across data versions, OkUnsupported normalization and actual effect result. |
| `tsk-d75f` HTTP/Hub MCP handlers | 47fd's real base endpoints plus application DTO; strict bounded decoding/admission; scope/version/authority errors and effects, not a boolean envelope. |
| `tsk-b788` React | Actual DTO/native/shared target and last publication data version; single global selectors, captured mutation target, pending/cleanup/recovery and design system rules. |
| `tsk-4323` daemon context | Actual bounded Records/supervisor producer IDs before code; captured scopes/version/binding, drain before close, no ephemeral registration. |
| NI `tsk-6b5f`, `tsk-9a3d`, `tsk-c3c2` | Qualify current Kiro V3 only, documented selected surface with controlled fixtures and genuine application authority; no installed agent, provider account or active plan is a test prerequisite. Platform-specific delivery is qualified separately. |
| `tsk-376b` caller integration | Functional journeys of maintained callers and native hosts, with affected test retirement mapping. |

These are saved correction packets, not retroactive revisions of completed work
or unclaimed consumer specifications. Graphit dependency links block known new
producers; each owner reads the packet and revises its claimed specification and
checks before implementing changed scope. Completed MP/RC/V/S evidence stays
historical, bounded and read-only.

## Physical Global instruction composition

The application adds a closed PROJECT/Global publication destination for Role install/sync, with explicit --source-project origin selection when needed. The Global path requires no PROJECT merely to locate native destinations; it uses only bootstrap-captured home/native overrides and exact agents/skills roots. Origin raw instructions do not grant execution context: global profiles contain no selected PROJECT path/ID/version or StandingContext, and Knowledge resolves the active PROJECT documentation root in execution. Core keeps its existing writer and owner checks, with concrete native-instructions/models control subdirectories so exact captured root sets coexist with FILE/cache. The new consumer and ModelProvisioner coexistence focal are pending; [native Global instructions](../development/native-global-instructions.md) defines the paths and evidence boundary. Existing remote48d and lifecycle evidence remains qualified.

## Guardrails and delivery evidence

Validate maintained behavior: captured target projects and versions, current
configured agents, producer/alias/mode selection, ephemeral contexts and stale
UI response rejection. Retire tests of eliminated mechanisms; preserve regression
coverage for active operations. There is no differential baseline, broad benchmark
or mandatory installed-agent account/plan in this contract.

Fixtures must cover A/B projects plus global scope; current Kiro V3 Markdown
profiles with composed YAML and standalone hook documents; family-scoped skills;
binding unregister versus explicit shared removal; alias change/Copy↔Link; producer
different from consumer; source absent/changed; invalid policy/type/null/overflow;
pre-effect failure, post-native logical failure and uncertain recovery; partial
batch with failed/not_attempted items; cancellation; exact Forget scope; two
concurrent versions and delayed old UI responses. Shared config/hook fixtures
include foreign extensions, edited critical values and conservative Kimi refsets.

Validate the affected functional journeys with maintained payloads and native
hosts. Performance work uses the Rust implementation and representative workloads.
Native permissions, Broker grants, actual MCP endpoints and distribution remain
separate functional checks; pure-library tests do not establish those integrations.

## Authored Resources discovery and review

`graphit_hub::list_resources` observes the selected PROJECT Resources namespace
without opening payload contents. `ResourcesQuery` accepts literal kind, ID and
concrete artifact-version filters, a positive page limit and an optional last
`ResourceIdentity` boundary. The page limit uses the existing projection entry
limit. `ResourcesPage` returns typed identities and `next_after` only when a
lookahead exists. No host path, remote provider or credential is returned.

Ordering is lexical kind/ID/version. The reader scans to real EOF while retaining
only the requested page and one lookahead. It decodes the Hub's canonical
chunked identity layout, including UTF-8 split between chunks, and opens native
handles relative to their verified parents. A payload must be a directory, as
required by the existing Resources capture and publication producers. Regular-file
payloads, links, malformed identities and I/O failures are errors; an absent Resources namespace is an empty page. The caller
retains PROJECT authority and Original checks before filesystem operations and
before delivery. An identity or cursor does not grant access.

Pages observe the current filesystem, not a transaction across requests. Refresh
to include an item added before the current boundary. The Web adapter binds its
cursor to PROJECT/data-version and filters; it must not manufacture an inventory
hash, corpus quota or snapshot guarantee. Exact review and content capture reuse
`CapturedSourceReader` for the selected Resources tuple and relative paths.

The FS enumeration API no longer exposes copy-accounting callbacks. The existing `before` callback still runs per operation and per Windows entry even when a native buffer is reused. Concrete component/path bounds, native record checks, fallible allocation and handle integrity remain enforced. See [PROJECT source capture](project_source_capture.md).

Hub producer tests qualify enumeration only. The integrated
`/api/hub/authored/list`, `/api/hub/authored/review` and
`/api/hub/authored/content` journeys remain owned by
`tsk-d75f` and the actual navigation/review/publication UI by `tsk-b788` until
their HTTP/browser evidence passes. Metadata is explicitly null where no
authored descriptor producer exists; publication drafts are separate inputs.
Read access never grants publication, Runtime authority or an arbitrary path.

### Native authored read requests

The Native CLI commands `hub authored list`, `hub authored review` and
`hub authored content` call the same Resources producer as Web. They select the
current retained PROJECT, or explicit `--project-dir`, with the existing global
`--project-version`. The Native ingress authenticates Local or Runtime credentials
and retains `ProjectRecordAuthority::admit_authenticated_read` for the captured
PROJECT/version and Hub module. It accepts one Authorization header and one
explicit credential-origin header, rejects browser Origin, and retains Original,
live PROJECT and admission checks through delivery. Cancellation, shutdown and
deadline signal the owned blocking job and await its completion before response.
No provider, remote credential, HubWrite grant or arbitrary source path participates
in these reads.

The MCP envelope is strictly `{project_dir:NativePath,project_version?:string,query:{...}}`.
The native path selects an already-retained PROJECT; it conveys no filesystem
authority. Omitted project version must resolve uniquely. Explicit null and unknown
fields fail. `query` cannot shadow `project_id` or `project_version`: the ingress
adds those from the selected PROJECT before decoding the shared read DTO.

| Tool | Required query fields | Optional query fields |
|---|---|---|
| `graphit_hub_authored_list` | positive `limit` | literal `artifact_type`, `id`, `version`, bound opaque `cursor` |
| `graphit_hub_authored_review` | exact `artifact_type`, `id`, `version`, positive `limit` | `offset` (default zero) |
| `graphit_hub_authored_content` | exact `artifact_type`, `id`, `version`, relative text-file `path` | none |

All ten artifact kinds use the existing typed Resources codec. Success bodies are
the shared `status:"ok"` bodies documented for Web below, including nullable name,
metadata and next-page fields. The cursor binds PROJECT/data-version and filters;
review/content capture the exact selected tuple. Reads do not inspect installed
artifacts or manufacture a descriptor. Relative-path, file-link, complete-byte
validation and per-request input/page/output bounds are the same shared producer
controls. A valid read does not grant publication permission.

Native errors use `CallToolResult.is_error` and sanitized finite `{status:"error",code}`
bodies. Missing source/file and non-text content retain
`authored_source_not_found`, `authored_file_not_found` and
`authored_content_not_text`; invalid inputs, denied admission, unavailable or
ambiguous PROJECT, cancellation, deadline and output overflow have distinct finite
codes. An output bound too small for the error JSON returns an empty protocol
error rather than overflowing the bound or truncating identity/content.

### Web authored read requests

The three POST adapters select a live, trusted PROJECT by `project_id` and
`project_version`, using the common selection kernel. They retain the real Web
caller, verifier, provider, installation, PROJECT capture and Hub/read admission
through delivery. No InstalledHubRead, HubWrite, STS or local administrator
admission is required. Request fields and inventory identities are data rather
than authority; `project_dir`, `source_dir`, provider/profile and Global scope
are not accepted.

| Route | Strict JSON fields beyond PROJECT selection | Successful response |
|---|---|---|
| `/api/hub/authored/list` | Required positive `limit`; optional literal `artifact_type`, `id`, `version`, `cursor` | `status:"ok"`, selection, `entries:[{artifact_type,id,version,name:null}]`, nullable `next_cursor` |
| `/api/hub/authored/review` | Required exact `artifact_type`, `id`, `version`, positive `limit`; optional `offset` defaults to zero | `status:"ok"`, selection and exact identity, `source:"Resources"`, `metadata:null`, `entries:[{path,kind,bytes}]`, nullable `next_offset` |
| `/api/hub/authored/content` | Required exact `artifact_type`, `id`, `version`, nonempty relative `path` | `status:"ok"`, selection and exact identity, `source:"Resources"`, `content:{path,content,bytes}` |

The list cursor is bounded opaque observation data bound to PROJECT/data-version
and the exact filters. Changing a filter or selection invalidates it. All ten
existing artifact kinds use the same typed Resources identity; reading AST or
Knowledge sources does not add a FILE publisher for those kinds. An absent
namespace returns an empty page. Names and descriptor metadata remain explicitly
null because no universal authored descriptor producer exists. Publication drafts
remain separate inputs.

Review captures the exact Resources directory with the Core snapshot policy and
rejects file links. It omits the empty root entry, returns UTF-8 relative paths
and `kind:"File"|"Directory"`, and paginates actual captured node metadata. Each
request makes its own capture; pages do not promise a cross-request snapshot.
Content opens a file through that retained capture and validates the complete
read against its captured length, digest and physical binding. A zero-byte file
returns an empty string. Absolute paths, traversal, empty/dot components and NUL
are rejected before source I/O. Windows also rejects backslashes and
drive/alternate-stream syntax. Unix preserves literal colon and backslash names
returned by review; they remain confined relative paths within the captured source.

Page lengths use the existing content collection bound; the Hub enumerator also
enforces its projection page bound. Input, scalar/text and final JSON output
limits use the current policy. Exact captures retain their existing snapshot
limits; inventory never captures payloads or imposes an invented corpus quota.
Output overflow returns 413 rather than shortening identities or file content.

Missing source and missing file return distinct 404 codes
`authored_source_not_found` and `authored_file_not_found`. Binary/non-text content
returns 415 `authored_content_not_text`; size overflow returns 413
`authored_too_large`. Other finite errors retain 400 invalid arguments, 401 Web
session failure, 403 denied admission, 409 concrete conflict/ambiguous selection,
499 cancellation, 504 deadline, 503 unavailable and 501 unsupported. Capture or
integrity failures return sanitized 500 `authored_read_failed`; physical paths,
credentials and native error messages are not exposed.

The Browser retains synchronous capture/read work in its owned blocking jobs.
Logout, expiry, shutdown and deadline cancel the original request and await the
worker; a disconnected client drops its cancellation guard while the Browser
still owns the job. Terminal requests discard successful payloads. A test-only
pause after a real capture permits causal integrity/cancellation checks without
constructing a fake capture or authority.

### Install the selected authored source

`POST /api/hub/authored/install` is registered in the existing mutable FILE ingress.
Its strict body carries `source:{project_id,project_version,artifact_type,id,version}`,
destination `scope:{kind:"project",project_id,project_version}`, required `binding`
and optional `alias`, `entry_name`, `mode:"copy"`. Only the maintained Native
local types agent, skill, command and workflow apply. Global, logical latest,
explicit null, unknown/duplicate fields, host paths, provider/profile and Runtime
grants are rejected. The source and destination are independent live selections.

Source PROJECT Hub/read grants only the source capture. Destination installation
requires the current private Web mutation admission plus its own PROJECT Hub/read
and physical capture. The retained Core reader supplies the canonical path to
`SourceRef::local_link`; callers cannot supply that path or producer. Existing
renderers and the owned Hub/Core publisher copy and reapply the content,
preserving omitted options and rejecting a foreign source root. The established
effect envelope reports `operation:"install_local"`, `source:"local"` and
`cache:"not_attempted"`; actual publication/pending/cleanup observations remain
explicit. Removal uses the exact installed target and preserves the original,
including when that original is unavailable.

The two new DTO units passed. The genuine M1 child completed local Copy, Link,
source edit/reapply, foreign-root rejection and source-free removal; the later
GlobalCache authority-retirement case returned 500 instead of the required 403
in emission7. The diagnostic emission8 passed the full focal in 107.86s with
unchanged production; the earlier interleaving remains under review. No speculative
handler classification change was applied. A bounded test-only witness now selects
the exact source tuple after successful cache cleanup and parent revalidation,
before the existing admission recheck and registry writer. It keeps the real
Original/captures and releases on guard drop. The strengthened complete M1
focal passed in emission9 (133.50s/133524ms, stderr empty), with the required403,
cacheCreated, no registry publication and real owned cleanup. Affected App Clippy
passed in 161819ms. This proves the selected boundary, not every interleaving.
Root CUA7 separately observed physical Copy creation, Link reapplication with
alias/entry preserved, and the actual LocalLink inventory provenance. All probe
owners joined and private credentials were removed; the lifecycle protocol's
`browser_qualified:false` remains distinct from Root's scoped browser evidence.
[Local Hub installation](../development/local-hub-installation.md#web-installation-from-an-authored-project-source)
defines the request, effects and recovery contract; no Source/compile proof alone
qualifies this separate local installation journey.

## Canonical local instruction installation

The Native CLI `hub install-local` and Runtime MCP `graphit_hub_install_local` select an absolute canonical source directory, explicit producer, kind/full ID/literal version and a live consumer PROJECT/version. The strict request does not accept remote transport/provider/profile or global scope. Agent, Skill, Command and Workflow reuse the existing native renderer and owned publication writer. SourceRef LocalLink records provenance. The checkout receives a copy. Reapply recaptures source changes and preserves omitted alias and entry name; remove/recover/forget use the existing owned PROJECT key and preserve the canonical source.

Runtime administrative admission and live PROJECT selection precede source filesystem reads; a source path or producer does not grant authority. Different origins/producers/source roots and foreign destinations conflict. Existing pending, commit, cleanup and delivery facts remain visible. See [local Hub installation](../development/local-hub-installation.md) and its receipt for the executable contract and actual qualification. This integration does not complete authored discovery, remote unpublication, Browser or Global local-source consumers.
