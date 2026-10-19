---
title: Web FILE publication from captured PROJECT source
status: qualified-linux-http
references:
  - target: {type: memory, id: 01M55K0R3ZP80EGRECP5CJN1H0, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: derived_from
  - target: {type: task, id: tsk-0257, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-b788, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
  - target: {type: task, id: tsk-bcaa, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-63a3, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: supports
  - target: {type: task, id: tsk-4208, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: derived_from
---

# Publish a FILE from a live PROJECT

This producer reads the maintained resource tree of a selected, live PROJECT. It is separate from uploading a package, publishing Native libraries, discovering Owned sources, or creating/removing a local Linked source. Those journeys retain their own producer and consumer requirements.

The HTTP operation is POST /api/hub/files/publish with application/json and the
normal authenticated browser cookie, same origin and X-Graphit-Request header.
The body contains exactly project_id, project_version and publication. PROJECT
selection uses the actual live inventory; its version is distinct from the
concrete artifact version inside publication. A browser path, global scope,
binding, alias, payload URI or current working directory cannot select the source.

The concrete publication.version selects both the remote artifact version and the
maintained Resources source version for the publication artifact_type/full id.
That Resources directory must already exist at the literal selected version.
Selecting another resource artifact version selects another Resources tree; the
header project_version remains the separate live PROJECT data-context selection.
SourceRef::resources receives the validated publication version, and physical
capture of a missing source fails NotFound before storage credentials, project
metadata publication or payload/catalogue writes.

The Hub header discovers local source selections with POST /api/hub/contexts and the strict empty JSON object {}. The response contains contexts and current, using the existing NativePath/project_id/project_version context codec. Each context requires its own current Web Hub/read admission. Explicit denials are omitted; authentication, transport or invalid-response failures remain errors. current is null when the live current selection is unavailable or denied; the server does not substitute another project. This discovery does not require Task/Now permissions or an installed-content grant. The browser retains its own explicit selection, including the literal native binding used for installation.

The current React consumer selects this source through POST /api/hub/authored/list,
then POST /api/hub/authored/review and POST /api/hub/authored/content. List requests
carry project_id/project_version, optional artifact_type/id/version/cursor and a
positive limit; review adds the selected artifact tuple and independent offset/limit.
Content selects only a relative path returned in that resource tree. Responses
echo the captured context/selection, and review names source Resources. All ten
artifact kinds are readable; publication remains FILE-only. LIST entries contain
exactly the identity fields artifact_type/id/version and required name:null.
REVIEW separately contains metadata:null, not the publication Metadata input or
installation display metadata. Missing LIST name is not replaced by metadata;
no compatibility/default is applied. The user supplies publication metadata
explicitly.

The inventory uses current lexical tuple pagination with memory proportional to
the page, not payload hydration or a global snapshot/hash. The App cursor is bound
to PROJECT/data version and filters. New items before a cursor require refresh
from the first page. Independent captures can observe source changes; real
missing/conflict errors do not imply rollback. Empty text succeeds; absent files
return404/authored_file_not_found, non-text415/authored_content_not_text and limits
413/authored_too_large. PROJECT Hub/read admission does not grant publication.
No host path, STS capability, free-path grant or invented source digest is exposed.
The new list/review/content consumer has focused API/component proof; productive
HTTP and real browser/cleanup integration remain pending. The earlier qualified
publication proof above does not establish authored discovery.

Publication metadata requires provider, artifact_type, full id, concrete version
and name. FILE kinds are skill, command, workflow, agent, rule, mcp, language and
power. Description, author, tags, dependencies, expected_sha256 and set_latest
retain the existing Metadata codec rules. Missing dependencies preserve the
current dependencies and [] clears them. A changed archive at the same concrete
version requires the old catalog SHA256. set_latest defaults to false, preserving
the current pointer; it does not infer latest from version ordering.

The Web caller needs a current PROJECT Hub/read admission, the installation's
private Web PROJECT administration entry, and its own matching Broker HubWrite
IncomingBearer service. Reading the source does not require write permission on
that PROJECT. Provider, verifier, caller, issuer and Original remain bound across
the source admission and publication. ActorMemory, a Native profile, an installed
catalog entry or a valid JWT alone supplies none of those authorities. Standalone
publication by provider/profile is not implemented by this ingress.

The producer selects SourceRef::resources and captures that directory physically.
It reuses the existing ZIP preparation, source revalidation and manifest codecs,
then obtains real scoped Broker storage credentials, publishes the actual PROJECT
metadata and finally publishes payload/catalog with conditional writes. Guards
retain the same Original cancellation/deadline, current policy, service lease,
physical roots, live selection and issued storage expiry. A synchronous recheck
is not another SQL authorization request or a renewable grant.

Success preserves the existing object with status published, publication receipt
(producer, artifact_type, id, version, archive_sha256, payload and catalog), and
project_metadata. Errors use a finite error code and an outcome when domain facts
were recorded. stage, publication, project_metadata, public cause and owned
staging residue survive later failure. Payload/catalog can be published even if
a final source or delivery guard fails. There is no automatic retry or rollback.

The browser job owner retains publication work through quiescence after logout,
stop, cancellation or deadline. Failed final delivery does not erase observed
storage effects. Final response limits apply to the entire result; when a receipt
cannot fit, HTTP413 and x-graphit-inspection-required: request-selection require
inspection of the original PROJECT/type/ID/version. Very small response limits may return
that status/header with no JSON. Never infer that omitted effects mean no effect.

A failed job harvested before dispatch reports the current request as publication
not_attempted and project_metadata not_written. Only failure of this request's
own worker can make its unpublished observations unknown.

The causal fixture retains one actual successful Broker STS response with private
hold/entered/release commands. It requires logout204 and the current publication
response401 while that STS response is still held, then releases it, checks a
healthy sibling and confirms the cancelled version is absent from the catalog.
Shared.jobs contains session timers as well as workers; a generic join_next does
not identify the publication worker and is not an exact quiescence oracle. The
real owned shutdown still drains all jobs. The barrier is confined to the Broker
example and also releases during shutdown; it alters no productive authorization,
credentials or storage response. The corrected real case passed in App55n (1 test, 71.50 seconds);
common owner/cancelOriginal lifecycle evidence remains separately reused.

The focused real test uses the existing Broker/Web/STSv3/MinIO fixture and ordinary
FILE catalog/content/install consumers. Existing Auth and ZIP/CAS tests remain
reused evidence; compilation, focused tests and integration are recorded
separately. SourcePROJECT publication does not complete the broader React Hub UI.

Sources: [Hub context producer](../../crates/graphit-app/src/source_server/web_hub_contexts.rs),
[authenticated context/read ingress](../../crates/graphit-app/src/browser_auth/hub_file_reads.rs),
[Web Source producer](../../crates/graphit-app/src/source_server/web_file_publication.rs),
[HTTP owner](../../crates/graphit-app/src/browser_auth/hub_file_publication.rs),
[real Web fixture](../../crates/graphit-app/src/browser_auth/tests/hub_file_publication.rs),
[existing FILE publisher](../../crates/graphit-app/src/file_publication.rs).

## Local installation from the selected Resources source

The prepared React consumer also uses POST /api/hub/authored/install for
agent, skill, command and workflow. The body is source
{project_id, project_version, artifact_type, id, version}, scope
{kind:project, project_id, project_version}, required binding and optional
alias, entry_name and mode. Source and destination are captured separately;
source identity comes from the authored selection and destination from the current
PROJECT installation scope/header. There is no host path, provider, STS, remote
Hub/write requirement or free source grant. The source needs its own Web PROJECT
Hub/read admission and the consumer its existing Web mutation admission.

Mode uses copy|link; Copy uses the existing RejectFileLinks policy. An omitted
mode preserves the prior physical mode and a new installation uses Copy.
Omitted alias/entry_name preserve existing options on reapply. Null is invalid,
not omission. LocalLink is the persisted source provenance independently of
whether its physical projection uses Copy or Link. Existing protections reject
foreign source/producer/directory state rather than adopting it.

Successful responses echo operation install_local, source local, scope,
cache not_attempted, producer (source PROJECT), artifact_type/id/version,
project/project_version (consumer PROJECT/data version), binding, publication
as a Debug string and possible_parents as NativePath[]. They do not echo the
requested mode or names. Business errors retain only observed outcome facts;
early failures can have only error.code. PublicationError uses the existing
committed/hub_pending/core_pending/observation_failed fields; later failures can
retain a confirmed publication string. cleanup_pending is present only when
observed, and worker uncertainty can use change Unknown/committed null. There
is no generic pending object or parallel effect parser. Existing bounded-response
and original-target inspection rules apply to the captured consumer inventory.

This local-install UI delta has 40 new and 16 affected shared cases passing.
One production bundle was built and its three assets promoted to the App; the
previous a68 authored bundle remains historical evidence. The actual Root
browser selected-source read/publication and Copy/Link journey passed with
Auth-confirmed owned cleanup. This qualifies that positive slice, separately
from the unresolved historical run7 M1 security failure and unobserved Global journeys.

## Installed source provenance

Every `/api/hub/installed/files` entry includes required `source_origin`, with
exact values `HubCache`, `Resources` or `LocalLink`. It is independent of physical
Copy/Link. The existing `origin` field remains remote Broker/Standalone details
or null. `HubCache` with null remote origin is valid and never becomes a local
source by inference. `Resources` and `LocalLink` have null remote origin; a
remote object on either contradicts the persisted source model.

The UI rejects missing, null, unknown or contradictory provenance. Rows and the
inspector display the actual source origin and remote transport separately.
Remote Update/content require both HubCache and a concrete remote origin.
Remove/Recover/Forget keep their captured PROJECT lifecycle independently.
Changing source origin on refresh invalidates the old inspector selection.
Focused checks cover all three source-origin values and malformed pages. The
Root browser also observed actual LocalLink provenance with null remote
transport after installation; Auth confirmed owned cleanup. This positive
slice does not qualify unobserved browser branches, Global or the independent
M1 security scenario: run7 returned500 where403 was required; diagnostic8
passed with unchanged product source and only a fixture assertion decode-order
change. The cause of run7 remains unknown. The later causal9 scenario passed with an explicit test-only interleaving witness; that proof does not explain the earlier failure.

## Package publication

Package upload is an active journey, separate from capturing maintained PROJECT
resources. Its HTTP contract is POST /api/hub/upload with exactly two multipart
parts: metadata JSON first without a filename, then one file last. Metadata is
{project_id, project_version, publication}; the PROJECT and data version come
from the authorized Hub context, and publication.provider is the signed-in
Broker provider. No project_dir, agent or raw publisher namespace is accepted.

publication uses format file or native, with artifact_type, id, a concrete
version and name. Optional fields are description, tags, author and dependencies
[{artifact_type,id,selector}]. Omitted dependencies preserve existing dependencies
(or start empty for a first publication); an explicit empty array clears them.
FILE accepts agent, skill, command, workflow, rule, mcp, language and power as real
ZIP packages. Only FILE accepts expected_sha256 and set_latest. Native accepts
ast with an .ast package or knowledge with a .knowledge package, and rejects
those two FILE fields. The server validates package contents and Native stores;
the browser's extension check does not establish package validity.

Power requires a real ZIP. The previous package-free control created catalogue
entries without downloadable objects and did not provide a functional install
journey. Framework had no supported install type or consuming producer; that
option is removed. Neither case gains a dummy archive, hidden file or legacy
compatibility path.

Confirmed results require status published, matching producer/type/ID/version,
a published catalogue and project_metadata published or unchanged. FILE receipts
contain archive_sha256 and payload published. Native receipts contain root_sha256
and have neither payload nor archive_sha256. These digests identify different
objects and must not be substituted. Errors preserve actual HTTP status,
observed effects and the original request; an uncertain result, conflict or
inspection-required response uses explicit original-catalogue inspection before
another attempt. Catalogue absence does not prove rollback or absence of residue.

The three-step form reuses the publication metadata fields, outcome facts and
original-target inspection used by maintained resources. Drafts and package files
survive failures. A sent mutation remains attached to its captured PROJECT/data
version when the header changes; identity or API changes abort and discard the
old owner. Reviewing metadata does not publish, and no mutation retries itself.

nativeRecordMultipartResponse shares native POST URL resolution, same-origin
credentials, X-Graphit-Request and AbortSignal. FormData and File are passed
unchanged, with the browser supplying the multipart boundary. The client does
not read ArrayBuffer/base64 or recompress packages. JSON requests retain the
existing NativeInteger serializer.

The transport seam has eight focused tests passing. The form/client and affected focal tests are implemented against this contract.
The current production bundle includes upload and collective maintenance, with
the scoped early-failure correction described below. CUA09 observed a real
241-byte FILE ZIP publication, a matching archive digest and original-catalogue
inspection at the concrete version and latest pointer. The three steps, pending
state, keyboard flow and 390-pixel layout were exercised in the browser. Native
package qualification uses the producer HTTP proof; this browser observation
does not claim a Native upload. The probe owner cleanup is confirmed: finished, owners joined and temporary
credentials/package removed; browser logout and tab closure were observed.
Transport fixtures alone do not qualify ZIP/Native packages or publication.

## Collective FILE maintenance

POST /api/hub/files/update without selection updates all remote FILE installations
in the captured scope to their resolved latest releases. PROJECT requires its
authorized project_id, project_version and literal binding; global_cache omits
binding. Page filters and pagination do not restrict this operation. Sources and
publishers come from the installed records; the client sends no provider, ID,
alias, entry name or physical mode. Names and mode are preserved by the producer.
Resources and LocalLink are excluded. Native Global remains read only in Web.

The user confirms the scope before sending. The response has operation update_all,
status, scope, a boolean global flag and ordered items. Each item identifies its
type, ID, producer, previous version and actual target. Items are ok, error or
not_attempted; processing stops at the first error and later items retain
not_attempted without invented effects. Item outcomes are sparse observed FILE
effects, not full single-mutation receipts with fabricated operation or scope.
An empty successful list means there were no eligible remote installations.

HTTP failures use the existing {error:{code},outcome} wrapper. A late failure can
retain successful earlier items; status and observed effects must remain visible.
Before candidate enumeration, a failure can instead carry an early summary with
status error, operation update, the captured scope, cache not_attempted and
publication not_attempted. The client preserves these known facts separately
from an item outcome and correlates the exact scope. It does not invent an empty
item list or successful operation. HTTP499,503 or504 alone does not require
inspection when this exact no-attempt summary is observed. Unknown effects,
foreign scopes and explicit inspection signals still require inspection. A generic
summary without the complete scoped producer proof cannot authorize this exemption;
parallel effects cannot rescue an uncorrelated outcome.

A bounded413 response can omit the item list and still require inspection through
JSON or X-Graphit-Inspection-Required. This never means that nothing ran.

A sent update keeps its captured scope and binding across header/scope changes.
The confirmation closes when its context becomes incompatible; identity or API
changes abort and discard the old owner. Only a fully confirmed result in the
original current context refreshes its inventory. Uncertainty uses an explicit
read of the original scope/binding inventory, protected by the result epoch;
absence does not prove rollback or absence of physical residue. No update retries
automatically. The UI/API focal tests and affected owner/NativePath regressions pass. CUA09
observed GlobalCache update from release/one to release/two while an unrelated
ID filter showed an empty list. The result retained Created cache and MetadataOnly
publication effects; the installed-file reader returned the V2 package contents.
Escape returned focus to the trigger and Tab wrapped inside the confirmation,
including at390pixels. PROJECT UpdateAll is covered separately by producer HTTP
and focused consumer tests; it was not repeated in this browser fixture. The probe owner cleanup is confirmed: finished, owners joined and temporary
credentials/package removed; browser logout and tab closure were observed. The unused /api/update_all client wrapper is removed; the active consumer uses the current Hub endpoint.
