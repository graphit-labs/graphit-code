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

Install, alias change, Copy/Link selection, update, sync, remove, recovery and
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

The CLI/MCP FILE install/update contract accepts optional `mode: copy|link` only
for physical PROJECT projection. A supplied null or unknown mode is invalid.
Install omission selects `Copy(RejectFileLinks)`; update omission preserves the
previous `InstalledBackend::Physical.mode`. An explicit mode transition uses the
existing Core/Hub producer under the writer, retaining the captured expected
installation and `same_source` check. Alias/entry-name changes preserve that
choice. Agent/Skill Link uses the real Derived payload and owned pin, preserving
producer, origin and concrete version; an unavailable Link fails without Copy
substitution. A supplied mode for a nonphysical projection is rejected. The Web
request DTO remains unchanged and its internal FILE input omits mode.

`HubWriter::put_metadata` persists each concrete global version independently;
`get_version` selects one, while `get` returns no entry for an ambiguous ID.
`remove_metadata_version` removes that logical version without deleting cached
bytes. GlobalCache cannot enter the native publication/recovery journal and does
not select any native host through `includes(binding)`. It does not render an
Agent, publish a Skill into HOME, or make content available to Live automatically.
Cache presence alone does not authorize content or update. Those application
consumers must recheck the bound origin and verify the physical cache receipt;
the SDK metadata contract alone does not qualify those product consumers.

Missing or null lifecycle scope is invalid; it never means global and is not
translated from an older payload. CLI command dispatch selects the installation
for a global operation without discovering a PROJECT from CWD; supplied PROJECT
version/binding/root fields conflict with that global selection.

An omitted project version resolves once to the captured checked-out branch,
or `main` without Git, and returns that origin. Explicit versions select domain
data; they do not create a second native workspace. Physical installation metadata
and files are shared by the captured checkout root and serialize under the same
root/installation locks across version selections. Record and return the project
data version used by the last publication separately from the artifact's source
version; the UI can show that it differs from the selected data version. Two
requests for different data versions cannot bypass physical collision/ownership
checks by including a version in an artificial project publication key.

Global cache identity includes `(type, realID, concreteVersion)`, with producer
identity retained in SourceRef. Multiple installed versions coexist. Install can
resolve an unversioned catalog reference upstream, then captures/returns its exact
version. Global content requires an exact version when ambiguous. Global removal always
requires an exact concrete version, including when only one is installed; it
never deletes all versions or resolves a latest selector. The Rust registry represents these versions independently as described
below. The authenticated local App installer is implemented in
`native_install.rs` (see [native packages](../development/native-packages.md));
The Native package CLI and remote Native installer use those existing domain
producers; each consumer retains its own qualification. Browser writes remain
separate from installed-library read admission.
No old-schema importer.

Project writes require a managed, non-ephemeral project and captured identity.
Ephemeral contexts can resolve authorized external/imported read contexts and
transient data but create no project registry, durable pins, stores, supervisor
registration or recovery journal. Mutation returns `ephemeral_read_only` before
creating directories/locks. Global installation remains a separate explicit action.

Host selection extends the existing specialized agent precedence:
flag → Command → Project → Environment → Global → Distribution → OpenCode.
Each selected item carries one of the eight `NativeBinding` values. Kiro uses
`NativeBinding::KiroV3` with canonical input/output `kiro-v3`. There is no inference from the
launcher version, executable availability, ACP protocol number or filename.
Other canonical hosts and the active `claude-code` alias retain their current normalization.
The Gemini CLI and `gemini-code` alias are retired; Google Gemini inference protocols are separate. Winning invalid/empty/type-mismatched input fails with
source provenance. Ordered membership deduplicates binding identity and does not
authorize access. Flag/Command/Project winners may select a binding outside membership. An ambient/default winner is used when
membership is empty or contains it; otherwise single selection uses the first
member, preserving the current rule. A membership containing only Kiro V3 thus
selects V3 when the ambient/default choice is outside it. Batch flag selects one binding;
otherwise all membership in order; empty membership uses the same single resolver.
The Rust `AdapterId` remains the eight host families for common layout, not the
identity of an engine-specific installation. The current
[native installation library](hub_native_installations.md) implements this
selection/identity correction; product callers still need their deliveries.

The current Rust registry uses format 3: each type/full ID holds separate
`InstalledArtifact` records, each with one concrete source version. Global
metadata upsert and removal affect only that version; `get_version` is exact
and an unversioned `get` returns nothing for an ambiguous ID. Project records
remain singletons. Global AST/Knowledge/Language use `InstallationTarget::GlobalNative`
with the reserved `global_native` key namespace and no host selection. Each
concrete version contains exactly one Native/Original installation, without
selection binding, alias or entry name. The registry rejects host targets for
these global corpora, GlobalNative in Project scope, other artifact types,
Derived/physical payloads and mixed or duplicate installations before publication.
`adapter()` returns `None` for GlobalNative; host projection via `for_artifact`
continues to return real host targets. `includes(binding)` describes identity,
never authority. Imported Source selects the exact GlobalNative ID/version
without a binding while local Source retains its native binding. GlobalNative
metadata removal preserves payloads and uses exact-version removal; it cannot
enter a physical intent. Other physical intents whose keys have no version
cannot operate on an ambiguous global ID. This library representation supplies neither admission nor authority by itself.
App `execute_install` composes actual Native authorization, owned package staging,
Icebug structural admission and Source/Knowledge Lance schema opening before
local global publication. Payload and registry effects are separate; failed saves
require exact readback under the writer lock or remain Unknown, and published
payloads are preserved. Exhaustive row semantics and remote access remain outside
that installer; remote work remains in `tsk-7a13`.

## Installed Native Web inventory

The Rust Source/Browser adapter exposes `POST /api/hub/installed/native` for
metadata of AST, Knowledge and Language libraries physically installed in the
current installation. It uses the verified Web session and explicit provider
policy described in [installed Hub admission](web_installed_hub_admission.md).
The Source's Installation capture, live owner and registry GlobalInstallationId
must agree. Admission, cancellation, deadline and physical identity are rechecked
before registry I/O and delivery. The metadata read does not open remote payloads,
request STS, require a Native profile or infer a PROJECT grant.

Request example:

```json
{"scope":"global_installation","artifact_type":"knowledge","id":"vendor/library","version":"release-1","offset":0,"limit":30}
```

The scope and positive limit are required. Type, exact ID and concrete version
are optional filters; offset defaults to zero. Unknown or duplicated fields,
explicit null, unsupported types and invalid values fail as malformed input.
The existing captured Hub limits bound each request and its output.

The response is `{scope:"global_installation",entries,next_offset}`. Entries
contain only `artifact_type,id,version,producer,target,origin,provider,alias,entry_name,display`.
The target is `global_native`; origin is the public SourceOrigin discriminator
(`HubCache`, `Resources` or `LocalLink`). Provider is the remote Native provider
name when present, otherwise null. Display is nullable author metadata with
`name,description,tags`. Neither SourceRef, pins, filesystem paths, storage
addresses nor credentials are public fields. Entries follow stable registry
order. `next_offset` is null at EOF and otherwise selects the next matching
entry; no total or available-remote-catalog count is invented.

The response is private/no-store. Malformed input is 400; denied authority or
changed installation is 403; missing consumer is 503 (unsupported native OS is
501). Registry corruption and I/O errors return a sanitized 500 error, rather
than an empty success. Authentication, cancellation and deadline errors retain
the existing Browser lifecycle. The handler owns work through the existing
jobs set so disconnect and shutdown cancel and join pending work.

The read-only React route `/hub/installed` uses this DTO and the existing shared
header refresh. Its request scope is current Web provider/subject plus the
installation endpoint and Auth cycle, independently of PROJECT/version/agent.
Account changes/logout cancel stale reads and clear details. Filters, exact
versions, author metadata, keyboard inspection and narrow layouts share the
existing design system. This page does not supply FILE catalog, install, update,
remove or publication actions. Those broader journeys remain pending in
tsk-0257/d75f/b788 until their actual consumers are qualified.

Source: `source_server/hub_catalog.rs`, `browser_auth/hub_catalog.rs` and
`ui/src/components/hub/InstalledLibrariesPage.tsx`.

## Available remote PROJECT catalog

The tsk-0257 application cut implements metadata-only project discovery in
`remote_hub_catalog.rs` and its authenticated Source adapter. CLI/Source central
registration is implemented. Real projects CLI and FILE/Native publishing
journeys pass in 81ba/36g; that evidence is preserved separately from new artifact
search. The d75f Web adapters below consume the explicit installed HubRead
admission. Catalog51y, Remote52s/55o, Installed53p, Mutations55a and bcaa HTTP55n
have the scoped individual-endpoint results below; earlier compilation and runtime
failures remain in their receipts. These passes do not close grouped Web journeys.
React and broader Hub operations retain their own pending scope in tsk-0257/d75f/b788.

The CLI contract is `hub projects --provider <provider>`, with explicit
`--transport broker|standalone`, optional standalone profile and verified
`--identity-provider`, mutually exclusive `--id` / `--name-prefix`, and
`--limit` / `--cursor`. It calls `graphit_hub_projects` through the existing
authenticated daemon. The public result is
`{status:"ok",projects:[{id,name}],next_cursor}`; transport subject and
authorization revision are not public fields. This is distinct from installed
inventory and grants no PROJECT, artifact, payload or model-execution access.

Broker discovery consumes `GET /v1/hub/catalog/projects` from tsk-81ba, whose
transport envelope binds subject/revision to authorized metadata and pagination.
The current metadata document has one key,
`v1/projects/{id}/hub/metadata/project.json`, decoded by the shared
`graphit_hub::project_metadata::HubProjectMetadata` codec. Rust publication must
supply the actual captured PROJECT name; grant IDs, Presence, installed registry,
SQL projections and artifact names cannot substitute for it. There is no legacy
key probe or legacy document importer.

Standalone reuses the captured S3 opener and credential expiry. Shared current
ACLs combine global with anonymous, or authenticated/user/team grants from a
verified identity. Exact IDs read metadata directly. Wildcard and name-prefix
selectors use the mature S3 paginated listing API with delimiter/common prefixes,
without listing objects inside project corpora or keeping a whole corpus in RAM.
A cursor contains the bound query/provider/location/ACL digest and last authorized
PROJECT ID, never backend continuation credentials or an unauthorized candidate ID.
Changed query, identity, route or grants requires `catalog_restart_required`;
a cursor is neither authority nor a snapshot guarantee.

Source checks actual runtime HubRead permission, captured service/identity,
Original cancellation/deadline and installation state before protected reads and
delivery. Retained jobs use the existing stop/disconnect lifecycle. Successful
empty results are distinct from authorization, configuration, backend and decoder
failures. CLI errors expose finite public codes while keeping causal guard and
cleanup failures private; interruption is 130 and invalid arguments are 2.
Neither transport hydrates payloads or writes an installed registry.

The new real CLI fixture publishes FILE metadata through Rust and reads it through
Broker. Additional standalone codec-authored records are explicitly fixtures for
paging, ACL union/revision, verified identity and malformed/foreign metadata;
they do not claim a standalone publisher. Executions and measurements must be
recorded before qualification, separately from the preserved installed-inventory
HTTP/UI evidence.

### Available artifact manifests

`hub search --provider <provider> --producer <PROJECT id>` calls
`graphit_hub_search` through the same authenticated daemon. Optional `--kind`,
`--id` and `--version` are literal filters, with explicit broker/standalone
transport and standalone profile/identity selection as above. A version filter
selects manifests containing that exact version; it still returns the complete
versions list and only the manifest's declared latest value, or null. No text
query, release-prefix resolution or inferred latest is applied.

The public page is
`{status:"ok",producer,artifacts:[{artifact_type,id,name,description,tags,author,latest,versions:[string]}],next_cursor}`.
No hashes, storage addresses, credentials, roots, internal subject or payload
bodies are included. One item represents one current kind/full-ID manifest,
validated by the existing RemoteArtifactManifest codec against its canonical key.
An exact ID uses direct GETs for the selected kind, or the ten maintained kinds;
other queries use the mature SDK's bounded pages. Kind order is lexical; inside
each kind, canonical encoded keys determine order. Only the response page and
current document are retained. Lookahead establishes another matching item before
issuing a continuation, and filters consume pages until actual EOF.

Artifacts cursors are positions rather than authority. Each Broker page opens
FileTransport and obtains fresh PROJECT-scoped STSv3 credentials, retaining the
real authorization_revision from that response. The position is bound to query,
provider/config revision, endpoint, PROJECT and actual storage topology/prefix,
plus that authorization revision. It does not invent a subject key from a token,
secret or runtime actor, nor call project discovery solely to obtain a nominal
identity binding. Another subject currently authorized to the same namespace may
continue a position; missing Hub/S3 authorization denies the page before registry
LIST/GET. Existing projects cursors retain their subject/revision contract.
Standalone artifacts use their current verified identity/ACL capture and provider
location; a changed effective ACL revision requires an explicit restart even if
that PROJECT remains allowed. Current deny wins before protected manifest reads.

Implementation sources are the [manifest reader](../../crates/graphit-app/src/remote_file_install/catalog.rs), [public projection](../../crates/graphit-app/src/remote_hub_catalog/artifacts.rs) and [CLI arguments](../../crates/graphit-app/src/remote_hub_catalog/client.rs). The [actual CLI/MCP focal](../../crates/graphit-app/tests/source_executable/native_publication/remote_artifacts.rs) uses Rust FILE and Native publishers. The existing standalone focal additionally reads real published manifest bytes, mutates its real ACL, and distinguishes revision restart from deny before malformed protected metadata.

### Web remote catalog

The d75f Source and Browser adapters implement the following metadata consumers.
Their central integration and real HTTP reader focal passed in App51y as
recorded below. The test uses actual Broker OAuth, HubRead, STSv3 and S3,
with codec-authored project and artifact documents. It does not qualify
publication again.

| Endpoint | Required JSON fields | Optional literal selectors |
|---|---|---|
| `POST /api/hub/remote/projects` | `transport`, `provider`, positive `limit` | `profile` for Standalone, `id` or `name_prefix`, `cursor` |
| `POST /api/hub/remote/search` | `transport`, `provider`, `producer`, positive `limit` | `profile` for Standalone, `artifact_type`, `id`, `version`, `cursor` |

`transport` is `broker` or `standalone`; no transport/provider/profile is inferred
from the active Native profile. The body uses the existing typed CLI/MCP argument
structs directly from bytes. Unknown and duplicate fields, explicit null, invalid
limits/selectors and any `identity_provider` are rejected. Broker rejects a
profile. Projects rejects `id` with `name_prefix`. URI query parameters are not
an alternate body. Requests use the current same-origin Cookie session and the
UI request-header convention; Content-Type must be `application/json`. For example:

```json
{"transport":"broker","provider":"corporate","producer":"01D39ZY06FGSCTVN4T2V9PKHFZ","artifact_type":"skill","limit":20}
```

The result is exactly the public ProjectPage or ArtifactPage described above,
including `status:"ok"` and `next_cursor`. Preserve all versions and the declared
latest value. A current deny or decoding/storage failure is never an empty page.
A changed cursor binding returns 409 `catalog_restart_required`; restart from the
first page with current filters. Invalid DTOs return 400 `invalid_request`,
admission failure 403 `permission_denied`, missing source 404 `catalog_not_found`,
bounded catalog output 413 `limit`, cancellation 499 `cancelled`, deadline 504 `deadline`, stopped
Source 503 `unavailable` and other catalog failures sanitized 500
`hub_catalog_unavailable`. An input body above the existing extractor ceiling
retains its 413 transport rejection. Existing BrowserAuth failures retain their separate
`{"error":"session unavailable"}` / authentication envelope and status, including
401, 403, 502, 503 and 504. Responses use `Cache-Control: no-store`.

The authenticated Web provider is passed to InstalledHubReadAdmission, retaining
the actual installation and Original. The selected catalog target is separate:
Broker requires the same admitted provider; Standalone selects its explicit Local
provider/profile while retaining the verified Web subject. The Web service lease
comes from `ServiceSelection::credential`, never Native credential selection.
Search opens the existing FileTransport for the requested producer and its real
PROJECT STSv3 or current Standalone ACL. A discovery result grants no payload or
PROJECT access. Admissions and leases remain borrowed through I/O and delivery,
with current provider, signed expiry, installation and Original checks.

The existing BrowserAuth jobs owner retains spawned work until joined; response drop cancels the same Original, and logout/shutdown/deadline use the existing lifecycle. No rollback or hard preemption is promised. The prepared tests cover Broker and Standalone paging, literal malformed bodies, cursor restart, target profile versus actual Web subject, deny versus empty, logout/sibling and provider retirement. The real storage relay additionally retains a metadata response through deadline/disconnect and verifies the actual jobs owner joins the reader. This Linux catalog result does not qualify FILE or mutation consumers.

Source: `source_server/web_hub_catalog.rs`, `browser_auth/hub_remote_catalog.rs`,
`remote_hub_catalog/client.rs` and `browser_auth/tests/hub_remote_catalog.rs`.
The broader HTTP FILE content/list and mutation consumers remain necessary.
HubRead is not Configure, PublishedFilesystem or HubWrite authority. Global
physical installation/sync and journal composition stay with tsk-0257; the
already-qualified metadata-only global Remove is a different operation. Neither
these two readers nor tsk-4208 publication evidence completes those obligations.

### Web FILE readonly

`POST /api/hub/files/content` takes exactly one `remote` or `installed` object.
Remote reuses RemoteContentArguments: explicit selection with transport, provider,
producer, artifact_type, full id, version and exact, plus a relative path.
Standalone profile is optional under the existing DTO. Native identity selection
is rejected; only the authenticated Web subject supplies identity.

```json
{"remote":{"selection":{"transport":"broker","provider":"corporate","producer":"01D39ZY06FGSCTVN4T2V9PKHFZ","artifact_type":"skill","id":"vendor/reviewer","version":"release/one","exact":true},"path":"SKILL.md"}}
```

Literal/latest/release-prefix resolution uses the existing FILE kernel. The
reader streams the genuine archive through digest/ZIP validation, reads UTF-8
and closes its owned temporary stage. It publishes no cache, pin or registry.
The result contains status/source, concrete producer/type/ID/version,
`content:{path,content}` and `cache:"ephemeral"`. Read/cleanup failures remain errors.

Installed content instead requires a concrete version and relative path. Scope
is `{kind:"global_cache"}` or `{kind:"project",project_id,project_version}`.
PROJECT requires an explicit binding identifying the actual target; global rejects
binding. Provider/profile come from the exact stored tuple, not request labels.

```json
{"installed":{"scope":{"kind":"global_cache"},"artifact_type":"skill","id":"vendor/reviewer","version":"release/one","path":"SKILL.md"}}
```

The source must be HubCache in the same installation. The reader reauthorizes
the producer, verifies the existing Core copy receipt and reads existing bytes.
It does not resolve latest, hydrate, repair or repin. Missing cache/receipt is
404; corruption retains its real error. The result has scope/target and
`cache:"Verified"`; a changed tuple at final readback returns 409.

`POST /api/hub/installed/files` takes that scope, required positive limit,
offset (default zero) and optional artifact_type/id/version/binding filters.
FILE means Skill, Command, Workflow, Agent, Rule, MCP, Language or Power.
Global selects only GlobalCache, separately from GlobalNative corpus inventory.
PROJECT binding filters use the target's includes rule. The response is
`{status:"ok",scope,entries,next_offset}`. Entries retain
`artifact_type,id,version,producer,target,source_origin,origin,alias,entry_name,display`.
`source_origin` is the required Hub `SourceOrigin` discriminator: `HubCache`,
`Resources` or `LocalLink`, serialized directly from the installed source.
`origin` separately retains public remote transport/provider/profile data or
`null` for a source without remote FILE provenance. No paths, endpoints, pins,
credentials or payloads appear. All page origins/producers are reauthorized;
deny is not empty success.

PROJECT ID/version select a retained live-project snapshot without CWD, supplied
filesystem path or current fallback. Missing selection is 404; ambiguous mirrors
are 409. A fresh Broker PROJECT Hub/read decision must join InstalledHubRead
from the same issuer, caller/verifier/provider, physical capture and Original.
Global uses its installation proof and checks both the registry InstallationId
and its physical parent directory against that admitted installation root,
without a synthetic PROJECT. Admission,
selection, installation, lease, transport and signed expiry are rechecked through
I/O and delivery. Local metadata is not an external payload grant.

BrowserAuth retains current cookies/Origin, JSON limits, private/no-store and owned
jobs lifecycle. Errors are sanitized invalid_request (400), permission_denied
(403), file_not_found (404), state_conflict (409), limit (413), cancelled (499),
deadline (504), unavailable (503) or file_read_unavailable (500). Authentication
and extractor body rejection keep their current separate envelopes.

Sources: `source_server/web_file_reads.rs` and its inputs/installed/listing modules,
`browser_auth/hub_file_reads.rs`, the existing FileTransport SDK and
ProjectRecordAuthority/check_project_source. The RemoteContent and Web InstalledContent/List fixtures are in `browser_auth/tests/hub_file_reads.rs`; their exact foci passed on App52s and App53p respectively. Earlier Native setup failures remain historical in the receipt. Those earlier reader results alone do not qualify mutations or later lifecycle cases; Mutations55a and Remote55o supply their separate current scopes below. Configure, ManageCache, PublishedFilesystem and Web HubWrite
are separate necessary producers; none is granted by these readers.

### Web FILE local mutations

The d75f Source and Browser modules now implement the following local operations.
Their central registration is integrated for the combined App emission;
The exact mutation focal passed in55a; the earlier App53p removal-oracle and App54a finite cleanup-observation FAILs remain preserved. This section
is the concrete Web contract; the broader planned result table later in this page
does not replace its HTTP codes or DTOs.

| POST endpoint | Operation | Body |
|---|---|---|
| `/api/hub/files/install` | Install a remote FILE into GlobalCache or a PROJECT target | Install selection below |
| `/api/hub/files/update` | Update an existing target; missing previous tuple is 404 | Same install selection |
| `/api/hub/files/remove` | Remove PROJECT publication or one GlobalCache metadata version | Local lifecycle selection below |
| `/api/hub/files/recover` | Resume the selected PROJECT pending intent | Local lifecycle selection |
| `/api/hub/files/forget` | Forget same-key PROJECT state and preserve physical residues | Local lifecycle selection |

Install/update take `{selection,scope,binding?,alias?,entry_name?,mode?}`. Selection is
the existing RemoteArguments object: explicit transport/provider/producer/FILE
artifact_type/full ID/version/exact and optional Standalone profile. Scope is
`{kind:"project",project_id,project_version}` with an explicit binding, or
`{kind:"global_cache"}` without binding, alias, entry_name or mode. New optional fields
reject explicit null. Unknown/duplicate fields, supplied identity_provider,
project_dir, global booleans and ambient current/CWD selection are invalid.
The optional mode uses the existing Native PhysicalMode enum: only "copy" or
"link" is accepted; explicit null, unknown values, wrong types and duplicate
fields are400 before protected I/O. A new PROJECT install without mode uses
Copy with RejectFileLinks. An update without mode preserves its previous
physical mode, as well as omitted alias/entry_name. An explicit mode requires a
Physical projection; incompatible projections return501/unsupported without
publishing a PROJECT tuple or simulating Link. A cache effect already confirmed
before projection rejection remains in the outcome. Physical Link retains the
remote HubCache SourceRef, producer, concrete version and provenance; it does
not become LocalLink or accept a free filesystem path.

The HTTP mode adapter passed its three DTO units and the complete genuine M1
focal in emission9 (133.50s harness, 133524ms runner, stderr empty). The affected
App Clippy gate passed in 161819ms. Earlier Mutations55a remains scoped to its
original behavior; the current focal separately qualifies the mode cases.
The affected real focal now exercises default Copy, explicit Link, omitted-mode
update retaining Link, explicit Copy and omitted-mode update retaining Copy;
it observes registry mode and actual filesystem links, not only response JSON.
A genuine SDK-published Command with KiroV3 tests unsupported projection
without fabricating an absent artifact. UI mode controls are owned separately
by b788; this HTTP source status does not qualify their browser journey.

Update preserves the stored alias/entry_name when omitted and compares the
previous producer, origin, target and tuple under the existing writer. Origin
changes or races conflict; update does not become install when the old target
is absent.

```json
{"selection":{"transport":"broker","provider":"corporate","producer":"01D39ZY06FGSCTVN4T2V9PKHFZ","artifact_type":"skill","id":"vendor/reviewer","version":"release/one","exact":true},"scope":{"kind":"project","project_id":"01D39ZY06FGSCTVN4T2V9PKHFZ","project_version":"main"},"binding":"kiro-v3","alias":"reviewer"}
```

Lifecycle takes `{scope,artifact_type,id,version?,binding?}`. PROJECT requires
the explicit binding; optional version is an exact installed-version expectation.
GlobalCache accepts Remove only and requires a concrete version; it preserves
cached bytes. GlobalNative belongs to its separate consumer. Lifecycle takes no
remote profile, producer or payload path. PROJECT Web still needs a fresh HubRead
SQL decision, including for local recovery, and therefore keeps its authenticating
Broker available. Native Runtime lifecycle retains its existing source-free
administrative ingress.

PROJECT administration uses the private policy plus the actual caller's current
PROJECT HubRead, physical root/identity/version and live selection. Global uses
its separate private global scope and physical installation/registry identity;
neither borrows the other scope. See [Web mutation authority](web_hub_mutation_authority.md).
InstalledHubRead supplies only the separate remote read service/verified subject
used by install/update; it never becomes mutation authority. Remote HubWrite
publication is a separate operation and proof.

The SDK resolves and downloads before persistent cache parents. After remote I/O,
`recheck_for_effect` supplies one fresh decision for the synchronous local phase.
Cache commit and registry/PROJECT publication run in that phase without another
network await or any RPC under a writer lock. Synchronous guards check current
policy/Original/lease/physical identity and live selection around the existing
writers. Cache commit checks the actual transport and caller guard after writer
acquisition and immediately before physical publication, closing retirement
during lock waits; the Native entry uses its real transport validator. Core and Hub retain their own cancellation/deadline/physical checks;
there is no claimed Auth callback at every internal Core barrier.

The existing copy receipt verifies the cache fast path. Cache commits return
ArtifactChange before the following guard, so a later failure retains a confirmed
cache effect. PROJECT publication reuses the existing renderer and ScopedCoordinator;
Remove/Recover/Forget reuse the local lifecycle kernel. Responses retain real
publication committed/hub_pending/core_pending/observation_failed and cleanup
facts. Forget returns the actual writer's bounded preview and possible_parents
after administration and physical admission; these paths describe residues and
grant no permission to delete them. A foreign pending key is a conflict, never
implicitly recovered or forgotten.

Success is HTTP200 with the actual outcome and `status:"ok"`. Domain failures
return `{error:{code},outcome?}`; the outcome is present when the local operation
recorded observations, including confirmed/uncertain partial effects. Codes are
invalid_arguments400, authentication_or_permission_denied403, artifact_not_found404,
installation_conflict409, remote_file_too_large413, cancelled499, deadline504,
unavailable503, unsupported501, invalid_remote_file500 or file_installation_failed500.
Existing Browser authentication/extractor failures retain their own envelopes.
A missing Source returns503 file_mutation_unavailable; unsupported OS returns501.
Responses remain private/no-store and same-origin. The final encoded success
or error envelope obeys max_output_bytes. If the complete receipt cannot fit,
HTTP413 includes inspection_required and a bounded truthful effects summary,
with selection:"request" identifying the exact target captured by the caller.
Whole identities and paths are omitted, never shortened. The fixed
X-Graphit-Inspection-Required: request-selection header also carries the duty
to inspect that target before retry/recovery; if the configured quota cannot
hold even the finite inspection JSON, the413 body is empty. An omitted receipt
never means no effect or rollback. Normal responses remain JSON-only and use
the existing body/content policy.

BrowserAuth owns a blocking worker in Shared.jobs while the HTTP owner remains
async and observes logout/stop/deadline. These signals cancel the same Original,
then await the physical worker's quiescence instead of discarding its future.
The final401/504 may retain the already-admitted operation's effect outcome.
Disconnect has no response; the retained worker still owns cleanup and join.
No hard native-I/O preemption or rollback is promised.

Sources are `source_server/web_file_mutations.rs` and its inputs/install modules, `browser_auth/hub_file_mutations.rs`, the shared `file_installation` publication and lifecycle kernels, and `remote_file_install/cache.rs`. Core publication retains pending state through its cleanup phase. Consumers must
wait for completed cleanup before treating an installed entry as settled.

### Browser FILE probe: opt-in and unqualified

The test-only selector is `browser_auth::tests::remote_native_web::hub_file_mutations::real_web_file_browser_probe_catalog_and_global_cache`. It is ignored unless explicitly selected with a real browser observer. Both this wrapper and the normal mutation wrapper `Box::pin` the single existing `run_file_mutation_fixture(browser_probe: Option<PathBuf>)`; the existing observed heap pin remains. The test-only correction checks `Physical(Removed)` and physical removal separately. No production producer is changed by this probe.

Root supplies `GRAPHIT_UI_BROWSER_PROBE_FILE` beneath a fresh private parent verified by `graphit_fs::verify_private_directory`. The base and derived `.credentials.json`, `.done` and `.result.json` paths must be fresh. Metadata, private login IPC and result are created using `graphit_fs::create_private_file` and written through the same returned handles. Synthetic login material stays exclusively in private Root IPC: never copy its values to public JSON, logs, documentation, Task or browser-session exports.

The metadata fields are `scenario`, `origin`, `path`, `scope`, `transport`, `provider`, `profile`, `producer`, `artifact_type`, `artifact_id`, `version`, `content_path` and `browser_qualified`; this contract stores no runtime endpoint, account or path value. Metadata keeps `browser_qualified: false`. The observer's `.done` is only a lifecycle signal. After all owner joins and removal of private login material, the fixture writes `.result.json` with `finished`, `owners_joined`, `credentials_removed` and `browser_qualified: false`. Neither signal nor Rust result certifies the browser journey.

Before handing off, the fixture performs real SDK search/content preflight for its prepared concrete release and GET `/hub/registry` with actual assets; it does not install before CUA. It retains the real Broker identity, STS/ACL and valid Skill publisher setup and issuer-verified per-subject Global policy. Root CUA must use legitimate local login and consent for catalogue → GlobalCache install → content → remove; no exported cookie or fabricated issuer authority substitutes for that journey. Root's real `/hub/registry` and `/hub/local` deep links/assets are integrated separately from browser qualification.

The 900-second hold retains owners only. Browser policy remains 600 seconds and login policy 30 seconds; expiration requires a new legitimate login, not an extended credential. Source formatting/diff and privacy/pinning inspection passed. Root54a compilation passed. Probe54a reached READY after actual SDK search/content and assets preflight; stat confirmed parent0700 and metadata/private IPC0600. Root subsequently reported actual CUA: legitimate LOCAL login and consent succeeded on an immediate retry after the first transaction expired during preparation; Standalone catalogue and ephemeral SKILL.md content were observed; GlobalCache install returned Created/MetadataOnly with the correct tuple and exact inventory; removal returned MetadataRemoved/CommitConfirmed with payload preserved and inventory empty; logout returned Anonymous. At viewport 390x844, document and main widths were 383 with no horizontal overflow. A stale publishersError from the Broker origin was observed under Standalone; the owning author corrected a new bundle, which had not yet been observed at that 54a snapshot.

Current source distinguishes earlier-job failure from the current receiver: draining an earlier JoinError before the new spawn uses Error::PreviousWorker and HTTP500 with error.code=file_previous_job_failed, outcome.status=error, cache=not_attempted and publication=not_attempted for the current selection. It does not attribute Unknown effects before dispatch. Error::Worker from the current receiver retains HTTP500/file_job_failed with publication.change=Unknown, committed=null and observation_failed=true. The cfg(test) unit worker_failure_envelopes_keep_current_effects_unattempted_before_dispatch in browser_auth/hub_file_mutations.rs compares both finish envelopes without panic or fault injection; it subsequently compiled and passed the exact envelope unit in55h (1 passed,0 failed;16ms runner/0.00s harness;590 filtered), without a real JoinError. Rustfmt and diff check passed. Mutations55a and browser55a evidence remain valid for their observed paths without replay; no real runtime JoinError is proved, the failing Remote55h did not qualify FILE-T2; subsequent Remote55o scoped qualification is recorded below.

Mutations55a subsequently passed the exact complete selector (exit0; 1 passed, 0 failed, 0 ignored, 585 filtered; 179.30s harness/179321ms runner), using App ELF graphit_app-5fd2cc01c5f0b24e after a successful compilation-only emission (191140ms). The pending-None poll passed; the run reached post-cache private-admin retirement with cache Created preserved and registry unpublished, then actual deadline, disconnect, logout, stop and cleanup paths. A later Root-requested test-only Clippy edit changed nested if to if let with && let entry; rustfmt and diff check passed, with no functional delta and no runtime replay. This qualifies the complete mutation selector and its actual cancellation/error/shutdown cases, not an injected or real JoinError. Probe55a subsequently passed its exact harness selector (exit0; 1 passed, 0 failed, 0 ignored, 585 filtered; 125.73s harness/125746ms runner). The private result is finished=true, owners_joined=true, credentials_removed=true and browser_qualified=false, the last retained by protocol. Stat confirmed credentials absent (ENOENT), done/result0600 and the previously verified parent0700/public metadata0600/credentials0600. Root CUA55a confirmed legitimate LOCAL login: the initial Broker origin showed legitimate Access unavailable, switching to Standalone immediately cleared the stale alert, and the selected profile/provider/producer/ID/release tuple returned the correct Web FILE reader without the earlier error. Logout returned Anonymous and discarded details. This composes actual unchanged CRUD54a with the origin/reader delta CUA55a and timely lifecycle55a; it is not a CRUD replay and does not change the failed deadline54a history or set browser_qualified=true. No concrete producer details or runtime IPC values are recorded. Source PROJECT publication is implemented by completed tsk-bcaa (revision20) under tsk-d75f, with the focused HTTP55n PASS documented below; this worker retains readers, mutations and FILE-T2 without overlapping that implementation. FILE-T2 was subsequently integrated only in browser_auth/tests/hub_file_reads.rs within the existing Remote selector and compiled in55h; Remote55h failed without establishing which new case or loop had been reached. The existing StorageRelay retains actual Standalone GET payload.zip after backend I/O; checked drain precedes every hold, with entered/pending observation, client45s/outer40s/Original30s, deadline504 or client abort-and-await, and release/join before the sibling operation. The disconnect path awaits its outcome, pops the owned reader, then asserts so an oracle failure cannot bypass cleanup; registry and children remain unchanged. Rustfmt edition2024 and diff check passed. Root55c strict passed (56439ms) before this integration and does not qualify the later delta. Actual archive GET deadline/disconnect/provider retirement subsequently passed in Remote55o; extraction and mirrors remain unproven. Mutations55a and browser55a evidence are preserved without replay.

The active FILE-T2 replacement check is chk-e97af21d9cac (superseding chk-8b4365ec8cea): all negative HTTP cases remain required, while recovered qualified common Selection evidence plus caller inspection may establish ambiguity-to409 before capture/I/O without fabricating two physical trees, a factory or copied stores. Root recovered common Selection proof from completed tsk-0074 revision16, checks chk-24c6e17ae4fc/chk-3d7061d7edeb: schema_selection_and_confirmed_errors_preserve_native_contract passed1/failed0 in0.02s in the historical socket library test, labeled schema-binary.log. Those logs/qualification artifacts are absent from this checkout; ELF hash, schema argv and exact historical assertion revision were not recovered. Root authorized reuse by purpose: current task_server/tests.rs537–572 checks normal selection and duplicated selection_ambiguous; web_file_reads.rs64–75 calls explicit Selection before CapturedPublicationScope, registry or payload I/O; browser_auth/hub_file_reads.rs62 maps AlreadyExists to HTTP409/state_conflict. This is historical qualification plus current inspection, not a new ambiguous HTTP execution or fabricated physical mirror. The active FILE-T2 check was recorded Failed after Remote55h; the subsequent55o evidence slice is PASS, with the failed history preserved.

The provider-retirement case, whose progress was unestablished by failing Remote55h, subsequently passed in55o: hold the actual archive GET until entered/pending, use real NativeProfileManager admin authority with expected-revision CAS and cascade=true to remove the selected storage provider and dependent profile, verify publication/cleanup/no pending intent and snapshot absence, then release/join and verify genuine Broker sibling bytes while preserving registry and children. The prior static prediction of HTTP500/file_read_unavailable through lease.check and io::ErrorKind::Other matched the actual passing55o assertions. Mutations55a/probe55a are not replayed, and the later PreviousWorker unit PASS qualifies only its finish envelopes.

Emission55g failed compilation (exit101/55715ms) on four peer-owned bcaa type errors; no tests executed and no qualification of this unit followed from that attempt. The owning author corrected those errors; re-emission55h was still in progress at that historical checkpoint. Mutations55a/probe55a remain valid within their recorded scopes, and the PreviousWorker unit had not yet run at that historical checkpoint.

Emission55h subsequently passed compilation (145703ms); its graphit_app-5fd2cc01c5f0b24e basename replaces the55a image and does not establish identical ELF contents. The exact worker_failure_envelopes_keep_current_effects_unattempted_before_dispatch unit was listed1 and passed1/failed0/ignored0/filtered590 (16ms runner,0.00s harness). It compares finish envelopes only, without a real JoinError or injected panic/fault. Root strict55i passed (66355ms; App lib/tests --no-deps -- -D warnings).

Remote55h failed (exit101;0 passed,1 failed,0 ignored,590 filtered;83825ms runner/83.80s harness) at assert_catalog_job_pending in hub_remote_catalog.rs169. The single real same-image RUST_BACKTRACE=1 diagnostic also failed at that helper (exit101;84821ms/84.79s); it yielded symbols without caller file/line and did not identify the exact loop or provider-retirement stage. Two earlier runner attempts failed before spawn because the Node process was unavailable and were not tests. At that55h checkpoint no new FILE-T2 case was PASS. Earlier-job completion and Notify permits were hypotheses rather than established causes; later diagnostic/fix evidence is recorded below. Mutations55a/probe55a and historical Selection0074 purpose-based reuse remain intact without replay.

Remote55o subsequently passed the exact listed selector (exit0;1 passed,0 failed,0 ignored,591 filtered;139979ms runner/139.96s harness). Root55o compilation passed181853ms with the new image at basename graphit_app-5fd2cc01c5f0b24e; this does not equate it with earlier images. The passing composition includes Local9316 and explicit Broker fixture55f. The fixture correction creates its local notification immediately after the hold decision and before backend I/O, then uses notify_waiters for release; entered still follows real backend status/body. This followed preserved FAIL55h/backtrace, FAIL55k (an unidentified Shared JoinSet completion at disconnect) and FAIL55m (current client200 before held release), without attributing a kernel defect or worker identity to those observations.

<a id="individual-endpoint-evidence-and-pending-publication-browser-probe"></a>
### Individual endpoint evidence and publication browser probe

The coordinator reports four H-checks PASS by composition: H-A1/H-A2/T1 and
replacement H-T2 `chk-2ebc2ce4887c`, using catalog51y, Installed53p, Mutations55a,
Remote55o, bcaa HTTP55n and distinct Native52m/lifecycle52s evidence. The former
batch H-T2 remains superseded with history preserved: this cut has no batch HTTP
endpoint. A future exposed batch still needs its own complete, partial, failed and
not_attempted semantics and qualification.

H-A3/H-T3 and UI-PUB-PROBE `chk-61e22a8c46b3` now have bounded local evidence PASS:
Source/documentation coherence, the single real56w probe and separate Root CUA,
and Root strict56x. The executor owns their MCP acceptance updates; this document
does not mutate Task checks or declare the grouping complete. Remaining authored,
Global, Link, unpublish and wider Web/UI journeys and the Root composition gate
retain their own scope. Windows/macOS remain unrun.

`real_web_project_file_publication_browser_probe` uses legitimate SourcePROJECT
resources and leaves publication to the browser observer. Task is attached to the
same Source with production task_policy and shared auth/live_projects; shutdown
drains Task. fixture_owner is retained before trust registration and again after
configuration, before fallible attachment. No independent Task fixture, prior
publication or second413 host is used. Private IPC/done/result and the900-second
owner hold follow55a without extending Browser/login policy; no credential,
runtime IPC path or concrete transient producer value belongs in public receipts.

Root implemented `POST /api/hub/contexts`, strict body `{}`, returning
`{contexts:[{project_dir:NativePath,project_id,project_version}],current:context|null}`.
It exposes only active local contexts with their own Web Hub/read admission:
Denied is omitted and other errors fail. current is the real authorized selection
or null, without fallback. Rechecks bracket encoding. This replaces the former
legacy-route gap for the b788 consumer without requiring Task/Now or installed
content grants. The probe directly checks200/no-store and the JSON DTO without a
status wrapper, the actual fixture ID/data version/NativePath/current, and
401 without cookie plus400 for extra fields and array input, all before private
IPC. The [Root publication guide](../development/web-file-source-publication.md)
remains the contract authority for this Root-owned producer and UI integration.

Compilation56u failed101/73811ms on E0425 unqualified serde Value; Root corrected
it and preserved that history. Compilation56v passed163994ms. The one real56w
probe passed (exit0;1 passed,0 failed,0 ignored,592 filtered;322228ms wall/322.20s
harness;stderr empty). Duration includes waiting for the observer, not operation
latency. Its private final result was finished=true, owners_joined=true and
credentials_removed=true; the fixture protocol marker browser_qualified=false is
preserved and is not a failure. Credentials were absent on stat; no secret was
opened. Actual CUA is separate evidence from the coordinator's private conversation,
not a claimed local screenshot file.

Root CUA observed legitimate login, actual Hub PROJECT/main selection without a
Task grant, review at1280x900/1024x800/390x844, one publication with metadata/payload/
catalogue published, original version/latest release/ui inspection, actual empty
Agent/KiroV3 FILE inventory, logout, viewport reset and tab close. At390x844,
document width390 and dialog width366 had no horizontal overflow. This does not
qualify413 CUA: HTTP55n/413/cancel and purpose-matched UI tests are reused separately,
as are b788's34 component/12 App tests and lint/types/build, without rerun.

### Web FILE SourcePROJECT publication

The Source/Browser producer defines POST /api/hub/files/publish with strict JSON {project_id,project_version,publication}. The PROJECT is selected from the live inventory; its explicit data version is separate from publication.version. No browser path, payload URI, global scope, binding or alias selects this source. Publication uses the existing Metadata codec, eight FILE kinds, full ID, concrete version, dependencies, expected_sha256 and explicit set_latest behavior. Package upload, Native publication, Owned source discovery/unpublish and local Linked/unlink retain their own APIs.

The actual Web principal needs a fresh PROJECT Hub/read decision for this
operation, private Web PROJECT administration, and matching Broker HubWrite with
its own IncomingBearer. Same caller, issuer, verifier, provider and Original
remain bound. Source write permission is not required. ActorMemory, Native
credentials, catalog availability and Standalone profiles cannot replace these
admissions. The publication proof's synchronous recheck is not another SQL call.

SourceRef::resources and physical PROJECT capture feed the existing finalized
ZIP preparation and consumer validation. Real scoped STS then supplies storage;
PROJECT metadata publishes before the conditional payload/catalog pipeline.
Success retains status published, publication (producer/type/ID/version/archive
SHA256/payload/catalog) and project_metadata. Failure retains public cause, stage,
recorded receipt, metadata effect and owned temporary residue. Confirmed or
uncertain writes survive final cancellation/delivery failure; no retry or rollback
is inferred. A failed prior job before dispatch instead leaves this request
not_attempted/not_written. Its own failed worker reports unknown observations.

The existing BrowserAuth job owner retains work through quiescence. Final
response limits cover effects too: HTTP413 plus
x-graphit-inspection-required: request-selection requires original-target
inspection when a full receipt cannot fit, including an empty body under very small
response limits. The actual HTTP test includes ordinary FILE read/install consumers and
one private held-real-STS logout204/current-response401/sibling witness, followed
by complete owned shutdown/drain; that corrected final case passed in App55n.
A generic Shared.jobs join_next cannot identify the publication worker.
See [Web FILE source publication](../development/web-file-source-publication.md)
for the precise qualified scope and broader journey limits.

### Web received package upload

POST `/api/hub/upload` receives the final package. This is separate from
SourcePROJECT publication. Its multipart form has exactly two parts in order:
`metadata`, a JSON string without a filename, then `file`, the single package.
The metadata part can omit Content-Type or use text/plain or application/json;
its JSON is decoded strictly. The filename is a label and never a filesystem
path. There is no `/api/upload` compatibility endpoint.

The metadata object contains `project_id`, explicit `project_version` and
`publication`. Publication is tagged by `format`: `file` uses the current eight
FILE kinds (agent, skill, command, workflow, rule, mcp, language, power), while
`native` accepts ast and knowledge. Both carry provider, artifact_type, full id,
concrete version, name, optional description/author/tags and dependencies as
`{artifact_type,id,selector}`. Only FILE accepts expected_sha256 and set_latest.
Dependency omission preserves the current catalog; an explicit list replaces it,
including an empty list clearing dependencies. Native releases remain immutable
and use the publisher's existing latest behavior. Framework is rejected. Power
requires a real ZIP; the old no-file option produced no downloadable objects and
did not provide a working dependency-only installation.

The selected PROJECT/data version identifies the actual publisher. The endpoint
retains the same Web session, PROJECT Hub/read, private PROJECT administration,
WebHubPublication/HubWrite IncomingBearer lease, provider and scoped STSv3 as
SourcePROJECT publication. Body data cannot replace these proofs. There is no
Resources lookup or namespace/host path chosen by the browser.

The owned worker streams the file into one private temporary directory/file.
Metadata is bounded by the minimum Browser/Hub input limit. The file uses the
current Hub copy byte limit; the outer multipart body additionally permits twice
the metadata limit for JSON/framing, failing closed if the sum cannot fit the
host. Exactly two parts, copy chunks of at most64KiB, SDK entry/path/depth/expanded
limits and original deadline/cancellation remain enforced. FILE validates the
ordinary ZIP with the consumer SDK, hashes the finalized bytes and publishes them
unchanged. Native packages require their physical format plus actual readonly
AST or Knowledge readers, manifest pins, pair/Source EOF/fingerprint coverage;
physical extraction alone does not qualify a package. See
[Native packages](../development/native-packages.md).

The existing Browser owner retains received bodies and work through quiescence;
logout/deadline/stop cancel then join, and disconnect retains the owned job until
cleanup. Success is `{status:"published",publication,project_metadata}` using the
actual FILE or Native receipt. Errors preserve confirmed/uncertain effects,
stage, finite cause and observed owned residue. Invalid package data is400,
limits413, and existing401/403/404/409/499/503/504/501/500 meanings remain. Final
output quotas also cover early errors, preserving session-clear/private headers.
A receipt too large for the response requires original-selection inspection via
`x-graphit-inspection-required: request-selection`; identities are not truncated.

### Web remote unpublish: U1 qualification

POST /api/hub/unpublish retracts remote catalog references without reading or removing SourcePROJECT, payload, cache or installation data.

HTTP5 remains PASS for its actual effects, Auth, cancellation and lifecycle
observations; it does not establish atomic CAS DELETE on MinIO CE. The real
negative Root105 failed in 89.902s: a Header SigV4 DELETE with wrong If-Match
returned 204 and deleted the catalog instead of returning the expected 412.
True atomic CAS DELETE remains required. That historical failure is preserved;
the associated strict integration passed in 15.900s without establishing DELETE
atomicity. The current capability gate refuses last-version and whole-artifact
DELETE on MinIO CE before effects. Filesystem is qualified separately: a genuine
wrong-If-Match DELETE returns412 and preserves catalog bytes and ETag; the
create-only negative also preserves the catalog.

Current Filesystem HTTP qualification is Root128: exit0, one passed, zero
failed/ignored,614 filtered,54131ms wall/54.11s harness. Root124 App lib
emission passed in173991ms after the coordinated copy-strategy correction.
Root120 Web failed first (exit101,101754ms wall/101.73s harness); that failure
remains historical evidence. Current App lib/tests strict passed in82352ms
under root128-app-strict, using
`cargo clippy -p graphit-app --lib --tests --no-deps -- -D warnings`.
The earlier Header SigV4 strict root60w-header-app-strict-final passed
in104928ms and remains historical.

The strict shared hub_unpublish::Arguments body contains provider, producer
(remote ProjectId), artifact_type (ArtifactType), id and target. Target is either
{kind:"version",version,expected_sha256?} or {kind:"artifact"}. Version is concrete,
not latest. Provider must equal the verified Web session provider. A supplied
digest must be non-null and satisfy the shared lowercase SHA256 contract.
Unknown, duplicate, wrong-shape and invalid-null fields are rejected before
protected I/O. There is no local PROJECT/version, SourceId, profile or binding input.

The Source uses the [source-free catalog writer](hub_write_service_credentials.md#source-free-remote-catalog-writer)
with its registered caller, verifier, issuer, physical installation, AuthStore
observation and Original. Only exact Web/HubWrite/IncomingBearer is admitted.
broker_credentials receives the remote producer directly; FileTransport/HubRead
cannot substitute. Current Broker/STSv3 authority decides the actual producer,
namespace and delete permission. [Local administration and PROJECT publication](web_hub_mutation_authority.md#source-free-remote-catalog-effects)
retain their own gates.

The catalog DELETE request uses the public object_store::aws::AwsAuthorizer
with Header SigV4 and sends If-Match for the observed ETag. Input borrows the existing
BrokerStorage; the Source supplies arguments, storage, client and policy.
The effective deadline is bounded by the Original deadline and storage expiry,
with Original cancellation retained. There is no presigned URL or new TTL.

The STSv3 response requires a `conditional_delete` boolean; absent, null,
wrong-type and duplicate fields are rejected without a compatibility default.
The consumer in `crates/graphit-app/src/native_storage_credentials.rs` retains
the issued value in `BrokerStorage` and exposes `supports_conditional_delete()`.
False remains valid storage credentials for reads and partial CAS PUT. The kernel
rejects last-version and whole-artifact DELETE before effects when false;
the capability is not additional authority. Independently, STSv3 requires an
unambiguous `copy_if_not_exists` string, either `multipart` or `if_none_match`.
The consumer maps the issued strategy to the SDK; neither capability is inferred
from a URL, provider or the other field. Root120 passed six direct credential
tests (107ms wall), covering both bool values, malformed fields, the two copy
strategies, topology, scope and expiry, plus fourteen kernel/envelope/Source
units (31ms wall). Current Root120 Native and Root128 Web results qualify the
composed boundaries; historical HTTP5 alone does not qualify these new fields.

Success200 contains status:"unpublished" and retraction with producer,
artifact_type, id, targeted_versions, desired_latest (string or null) and catalog.
Removing the declared latest does not promote an older version. Domain failures
contain error:{code} and, when applicable, outcome with the typed retraction,
stage and finite cause. Pre-effect failures report catalog not_attempted. Actual
kernel states preserve not_attempted, uncertain, rejected, updated or removed;
no error implies rollback. Late logout401, deadline504 or stop503 may carry a
successful unpublished outcome below the error envelope.

| HTTP | Finite code |
|---|---|
| 400 | invalid_arguments |
| 401 | Existing authentication failure, including unauthorized |
| 403 | authentication_or_permission_denied |
| 404 | remote_catalog_not_found |
| 409 | hub_unpublish_conflict |
| 413 | hub_unpublish_too_large |
| 499 | cancelled |
| 504 | deadline |
| 503 | unavailable |
| 501 | unsupported |
| 500 | invalid_remote_catalog or hub_unpublish_failed |

An unavailable composition returns hub_unpublish_unavailable with503 (missing
Source) or501 (unsupported platform). Earlier-job failure is500/previous_job_failed
with this retraction not_attempted. Current-job failure is500/hub_unpublish_job_failed
with catalog unknown and observation_failed:true; neither fabricates a receipt.

The final envelope obeys max_output_bytes even after effects. Oversized results
return413 and X-Graphit-Inspection-Required:request-selection. A bounded summary
preserves whole effects with inspection_required:true and selection:"request";
a smaller envelope uses effects:"inspect_selected_target". Identities and
versions are never truncated. If even the minimal envelope cannot fit, the body
is empty and the header retains the duty to inspect the original selection.
Extractor-level413 can independently return a non-JSON body.

Existing cookie/Origin policy, private/no-store responses, shared jobs,
CancelOnDrop and Original deadline remain in force. Logout/stop/deadline cancel
and retain the worker until quiescence. Disconnect may prevent a response but
does not remove the cleanup/join obligation or prove rollback.

Implementation is in source_server/web_hub_unpublish.rs and
browser_auth/hub_unpublish.rs; the real fixture is
browser_auth/tests/hub_unpublish.rs under crates/graphit-app/src.
Its canonical selector is:

```text
browser_auth::tests::remote_native_web::hub_unpublish::real_web_unpublish_version_cas_preserves_payload_cache_and_authority
```

HTTP5 qualified strict input/denials, Version-CAS, no promotion of an older
version, post-effect413 inspection, and three causal retained-STS scenarios:
deadline504, logout401 and client disconnect with owner join. It also verified
a healthy sibling, whole-Artifact and last-version Header DELETE, and repeat404.
These are observed effects, not proof of atomic CAS DELETE on MinIO CE.

The same focal preserved all bytes of four original payloads and the fifth
immutable payload, verified installed cache content, and preserved registry
bytes and installation top-level child names. SourcePROJECT was unavailable
and local administration policy empty. These observations do not claim
recursive filesystem inventory or a generic rollback guarantee.

Root120 separately qualified Native CLI/MCP Filesystem (one passed,115711ms
wall/115.69s harness) and MinIO safe refusal (one passed,115969ms wall/115.94s
harness). These are bounded scenarios, not atomic DELETE qualification for MinIO.

Root129 independently observed the actual CUA journey on Linux Filesystem:
publication of skill `fixture/web-publication`, version literal `release/ui`,
then whole-artifact unpublish. The catalog was removed, retracted_versions
contained `release/ui`, latest was null, Inspect found the original catalog
absent and the receipt remained available. Logout and tab closure were observed.
The probe finished with owners_joined and credentials_removed true; its
`browser_qualified=false` flag remains unchanged. The separate
browser-observation.json and screenshot record the CUA observation. Runner
exit0/168929ms includes interaction, waiting and cleanup, not HTTP latency.
Publisher discovery in the fixture remained unauthorized; exact publisher
access succeeded. Responsive/focus/layout and the 54 focal UI tests reuse the
previously qualified unchanged UI bundle; they were not rerun by this observation.

All failed attempts and fixture corrections remain in the canonical U1 receipt,
without counting replays as distinct tests. Qualification is limited to these
Linux HTTP, Native and observed CUA journeys. Other Native/UI journeys, macOS,
Windows, full Server SQLite, recursive filesystem cleanup and the
global migration remain outside it; d75f stays broad/open/flagged. Explicit null
digests remain rejected by the shared serde String decoder; omission remains None.

### Local FILE PROJECT lifecycle

`hub remove`, `hub recover` and `hub forget` use `--kind`, the full `--id`,
and `--scope project`, the selected local PROJECT/version and host `--binding`. Optional
`--version` is an exact installed-artifact expectation, not a release selector.
For example, `graphit --project-version main hub remove --kind skill --id org/review --binding codex --version release/one`
removes only that PROJECT's Codex installation. The MCP tools are
`graphit_hub_remove`, `graphit_hub_recover` and `graphit_hub_forget` with the same
strict selection. PROJECT does not accept remote provider, producer, alias,
AST or Knowledge. Global removal is the metadata-only operation below; global
Recover/Forget and batch/sync are not simulated.

The current Runtime administrative parent must authorize configuration, cache
management and published-filesystem effects, as in FILE installation. The handler
captures and rechecks the installation, live PROJECT identity/root/version,
binding and Original before effects and delivery. Registry metadata and paths
do not grant authority. These local actions neither contact STS nor require a
live remote provider or source cache: a user may undo an authorized local
installation after its upstream disappears.

The existing publication writer owns removal, recovery and Forget. The consumer
refuses a pending operation for another selected key instead of recovering it
as a side effect. Removal retains the Core foreign-file protections. Recovery
resumes the real same-key pending intent. Forget remains available when that
recovery fails: it detaches metadata/pending state while preserving physical
leftovers, returning the actual writer's `forget_preview` and possible parent
paths. Preview paths describe residues, not permission to delete them.

Results retain the operation and captured selection plus publication change,
known commit/pending/observation facts on failure, cleanup pending, and preview
where present. A failure after an effect does not promise rollback. Wrong
artifact version or binding does not select another installation; malformed
CLI input exits 2, cancellation exits 130, and failed or pending operations
exit 1. Global/native-library and Web consumers are not implied by these tools.

The [client selection](../../crates/graphit-app/src/remote_file_install/lifecycle.rs), [handler](../../crates/graphit-app/src/source_server/file_installation/lifecycle.rs) and [actual CLI/MCP focal](../../crates/graphit-app/tests/source_executable/native_publication/file_lifecycle.rs) form this delivery. The focal exercises real publication and installation, source loss, exact selections, foreign-file conflict, recovery and Forget.

### Global metadata removal

`hub remove --scope global-cache` and `--scope global-native` use the same
`graphit_hub_remove` tool with an explicit target and required exact version.
GlobalCache accepts FILE kinds; GlobalNative accepts AST, Knowledge and Language.
Language must identify its target because both representations exist. Neither
scope accepts PROJECT/root/version/binding, remote provider, producer or alias.
The CLI selects its installation without requiring a checkout. Missing, null or
conflicting selections fail before effects; `latest` is only a literal version.

The global cache uses the current Runtime administrative parent. Native metadata
also requires the current signed Native identity through the existing
`BrokerVerifierCache::get_native` and `authenticate_native` producer, and
`Configure`, `ManageCache` and `PublishedFilesystem` installation permissions.
The Native JWT is validated locally using trusted cached discovery/JWKS when
available; signature, audience, client/resource, temporal and local profile
checks remain active. Remote revocation/account changes may remain unobserved
until access-token expiry; this operation does not invent a TTL or call UserInfo
or introspection for proof. Missing trusted verification data can still require
authentication discovery; source-free means no HubRead/STS or remote payload
lookup, not bypassing authentication.

The existing global registry writer removes one version through
`remove_metadata_version`, under its real writer and the same Original. It
checks the exact installed target before mutation and rechecks current authority
and physical installation before delivery. A save error retains its actual cause
and committed/unknown observation without replaying the write. The result states
`MetadataRemoved` and `payload_preserved`; cached bytes, other versions and
other metadata remain. This does not request cache garbage collection or delete
payloads still referenced by another installation. Existing cache/domain cleanup
is independent of metadata removal.

GlobalCache and GlobalNative cannot enter the physical publication journal.
Recover/Forget therefore accept PROJECT only; no global journal, recovery noop or
physical cleanup is invented. AST/Knowledge package installation is existing
product functionality. Language removal consumes an existing GlobalNative registry
record; it does not claim to implement a missing Language installer.

The [global consumer](../../crates/graphit-app/src/source_server/file_installation/lifecycle/global.rs) and [compiled CLI/MCP focal](../../crates/graphit-app/tests/source_executable/native_publication/global_lifecycle.rs) implement this journey. The focal uses the existing genuine Icebug/Raw/FTS fixture and real package export/install, plus FILE publish/install, with two releases per target. It removes their source/Broker availability, then checks exact metadata removal and retained payload directories/sibling versions.

## Compiled skill publication during project install/sync

The application now composes the five maintained module skill sets and ten companions in the authenticated project install/sync path. It reuses the effective configuration already captured for the selected maintained project; `knowledge.docs_dir` is resolved by the Knowledge settings SDK. Compiled sources, Project/Global raw overrides and an explicitly selected local Hub Skill use the same instruction engine and original request context. A selected Hub installation uses its genuine `SKILL.md`/reference format and is re-observed through publication.

The existing Core generated-content writer publishes the complete skill directory directly from the validated render plan, preserves foreign files and retains known or uncertain partial effects in the operation result. The installed Graphit package is canonical for bundled templates; installed files are the effective artifact. Receipts keep provenance, version and content hashes without retaining another generated copy. Native layout and frontmatter come from the existing SkillDescriptor renderer. Shared physical roots use the existing shared-adapter namespace: Codex and Antigravity target the same `.agents/skills` owner rather than conflicting binding-specific keys. Alias changes use the writer's existing retarget operation.

The `--skill-bundle MODULE=ID@VERSION` selection reads an exact installed Skill in the current project registry. For remote FILE provenance, install/sync reuses current service admission, Native verifier cache, the verified identity when explicitly selected and the producer's independent PROJECT authorization. It verifies the existing Core-owned source-cache receipt, then keeps the lease alive through template capture and physical publication. Missing or changed cache is an error; this read does not download, update or adopt cache bytes. Standalone identity selection is explicit per module; Broker origins reject that extra selector. Origin metadata, equal bytes and a valid JWT alone never authorize remote access.

## Kiro V3 profiles and shared assets

Kiro agents and runtime profile state use the captured `kiro-v3` binding through
projection, render provenance, pending intent, registry, recovery and removal.
The native agent is Markdown at `.kiro/agents/<name>.md`, with YAML frontmatter.
The renderer accepts `KiroProfileRuntime { mcp, hooks }` and composes MCP settings,
permissions and profile-scoped `hooks[]` before one Markdown emission.
Standalone hooks remain separate JSON documents using `version: "v1"` and the
same PascalCase trigger names. Neither path converts older agent structures.

Kiro skills use one family-scoped installation at `.kiro/skills/<name>`, with
`shared_adapter:kiro` identity. Removing binding membership leaves published
skills intact; explicit asset removal uses the exact shared target. Incompatible
local versions under one stable real ID require an explicit update. An alias
changes a destination, not identity; simultaneous variants need distinct IDs and
destinations. Global versioned corpus identities remain separate.

The producer rejects unsupported surface/binding combinations before effects.
The current [native installation library](hub_native_installations.md) validates
registry target/type, the closed binding set, uniqueness and exact removal,
Forget and pin lookup. It retains the captured binding for intent reconstruction,
requires derived Agent schemas and rejects generic Markdown hook publication.
MCP and hook consumers must still use their real owned-document producers.

### Profile composition and activation

A caller provides the managed-agent identity tied to the captured native route,
logical role and verified caller credential. Controlled Runtime sessions select
that identity through the current ACP session configuration. Membership, MCP
settings and hook references do not infer a carrier or grant access. The caller
supplies its actual role; rendering does not choose a default agent or alter
personal profiles.

The pure renderer composes typed hook tuples into the selected profile's YAML.
The shared writer also exposes explicit profile-hook operations for an existing
Coordinator Markdown profile, preserving its body and foreign metadata values.
Standalone hook operations own their own versioned document. These are distinct
publication paths; callers must retain their exact profile/owner/binding and
critical-field checks during update, removal, Forget and recovery. Library
composition does not establish native activation, resume or complete caller
integration.

Manual absence of a managed agent grants no replay, resurrection or silent
bootstrap. A new explicit install still requires a fresh Missing observation and
admission. Agent names, YAML fields and role selections are data, not credentials.
The verified role/project/scope/revision/expiry boundary remains required before
store access; missing, expired, revoked or cross-project credentials fail closed.
See [Role credentials](role_credentials.md) for the issuer and consumer contract.
The actual launcher/handler transport and native execution checks remain separate
from the pure renderer. The [current Kiro agent format](https://kiro.dev/docs/custom-agents/)
and [hook schema](https://kiro.dev/docs/hooks/) define the host encoding; they do
not establish Graphit authorization or qualify this integration.

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
selection/grant acquisition precedes materialization. Update uses the stored
alias/mode unless an explicit change is part of its request. Link has no automatic
Copy fallback. Rendered artifacts require explicit Copy mode; Link is rejected before effects. A genuine local source can still be linked without creating a generated-content cache.

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
and optional `alias`, `entry_name`, `mode:"copy"|"link"`. Only the maintained Native
local types agent, skill, command and workflow apply. Global, logical latest,
explicit null, unknown/duplicate fields, host paths, provider/profile and Runtime
grants are rejected. The source and destination are independent live selections.

Source PROJECT Hub/read grants only the source capture. Destination installation
requires the current private Web mutation admission plus its own PROJECT Hub/read
and physical capture. The retained Core reader supplies the canonical path to
`SourceRef::local_link`; callers cannot supply that path or producer. Existing
renderers and the owned Hub/Core publisher apply Copy/Link and reapplication,
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

The Native CLI `hub install-local` and Runtime MCP `graphit_hub_install_local` select an absolute canonical source directory, explicit producer, kind/full ID/literal version and a live consumer PROJECT/version. The strict request does not accept remote transport/provider/profile or global scope. Agent, Skill, Command and Workflow reuse the existing native renderer and owned publication writer. SourceRef LocalLink records provenance; Copy/Link selects physical projection. Reapply recaptures source changes, preserving omitted alias/entry/mode; remove/recover/forget use the existing owned PROJECT key and preserve the canonical source.

Runtime administrative admission and live PROJECT selection precede source filesystem reads; a source path or producer does not grant authority. Different origins/producers/source roots and foreign destinations conflict. Existing pending, commit, cleanup and delivery facts remain visible. See [local Hub installation](../development/local-hub-installation.md) and its receipt for the executable contract and actual qualification. This integration does not complete authored discovery, remote unpublication, Browser or Global local-source consumers.
