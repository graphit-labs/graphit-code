---
references:
  - target: {type: task, id: tsk-0257, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: derived_from
  - target: {type: task, id: tsk-bcaa, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: derived_from
  - target:
      type: task
      id: tsk-b788
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: derived_from
  - target:
      type: task
      id: tsk-d75f
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: derived_from
---
# UI Dashboard Specification

The Graphit Observatory is the embedded web interface for Graphit Code. It presents the same project, context, registry, daemon, knowledge, memory, and AST state exposed by the CLI and MCP tools.

## Scope

The UI provides:

- workspace and Agent selection;
- Hub registry, local artifacts, and publication workflows;
- project and imported knowledge explorers;
- project and user memory explorers;
- project and imported AST explorers;
- a project Task explorer with complete lifecycle and audit detail;
- live multi-source agent sessions;
- daemon, Dream, and ecosystem status.

It requires its configured Broker browser session for authenticated Browser data APIs and does not maintain an independent copy of domain data. Without trusted `task.browser` configuration the service is headless. Sessions use opaque HttpOnly cookies with credential state in daemon memory, not persistent encrypted token cookies.

## Frontend architecture

The SPA lives in `ui/`.

| Concern | Implementation |
|---|---|
| Build | Vite |
| UI | React + TypeScript |
| Styling | Tailwind CSS plus semantic CSS tokens |
| State | Zustand stores with selective persistence |
| HTTP | Axios-based API modules |
| Markdown | Dedicated Knowledge and Memory renderers over their respective domain records |
| Relationships | Deterministic boundary catalogue and directed entity neighborhoods |
| Packaging | Versioned production assets embedded into the Rust binary |

`node scripts/build-rust-ui.mjs`, from the repository root, builds the React UI and copies the three production assets from `ui/dist/` to `crates/graphit-app/assets/ui/`. The Rust UI server embeds those versioned assets and serves them with same-origin API routes. A standalone `npm run build` in `ui/` only refreshes `ui/dist/`; it does not update the assets embedded by Cargo.

## Visual system

Graphit Code, Broker and the public site share the [Graphit design system](design_system.md): cobalt actions, a navy navigation rail, cool neutral workspace surfaces, Public Sans with system fallbacks, and IBM Plex Mono for source and identifiers. Dark mode preserves the same semantic hierarchy. The bracket mark represents bounded engineering context.

The default `/workspace` entry organizes the product around establishing context, coordinating execution, and inspecting evidence. It offers engineering jobs and useful project details from the existing app store; the shared header owns working-context selection. It does not invent delivery or quality metrics.

Persistent navigation groups Engineering (workspace, sessions, tasks and evidence, live search), Project context (code, knowledge, memory), Shared ecosystem (Hub and imported contexts), and Operations. All explorer routes remain deep-linkable inside the same shell. The internal compositions are rebuilt around the domain journeys below; their backend API contracts remain authoritative. The shared header keeps the current domain on the left and the Project and Agent selectors on the right on every route. The mobile modal navigation restores focus and closes on Escape or route selection.

## Responsive contract

- Below `md`, global navigation becomes a modal drawer with every route, filter and theme action preserved. Project and Agent remain in the shared header, outside the drawer.
- Below `lg`, Live Search stacks its artifact picker and session console.
- AST investigation and knowledge reading surfaces stack their supporting context below the primary work on narrow screens.
- Full-screen explorers must not introduce horizontal page overflow.
- Keyboard focus remains visible.
- Motion honors `prefers-reduced-motion`.
- Loading and toast states communicate status without removing the underlying route context.
- Selecting the catalogue row that is already selected keeps its detail rendered: the detail panel never toggles off, and a detail that failed to load is requested again.

## Workspace identity

The app store combines machine-local projects from `GET /api/global-projects` with the
ACL-filtered `GET /api/project-catalog` result. It persists the selected origin,
immutable project ID and preferred versions under the `graphit-app-state`
browser key. Checkout paths are obtained from current results and held in memory.

A Workspace request can send the exact `project_dir` resolved from the active
local context. Hub requests use `project_id` or an exact remote context. The
header shows the origin so the user can verify the target. A temporarily
unavailable Hub catalogue keeps its selected Hub target marked stale.

`AppShell` mounts one `WorkspaceSelectors` group in the right side of its header on every route, including all explorers. It is the only place that changes global project, project version and agent selection. Sidebar, mobile navigation and page toolbars do not repeat these controls or a separate selected-project badge. Each themed dropdown has a persistent label, selected checkmark and keyboard/type-ahead support; switching one leaves the other unchanged. The selectors write the existing app store and retain its browser persistence.

Controls remain visible while loading or empty, with descriptive disabled options. The Project menu
groups Workspace and **Hub · remote** targets. Workspace options expose their directory; Hub options
expose the immutable project ID, and the selected value includes “Hub”. The single Refresh action
reloads available projects and agents, then all data sources registered by the active page:
catalogues, open details and runtime data. It stays busy until all finish, prevents duplicate
activation and reports errors. Refresh never reloads the document, resets drafts or executes an
unreviewed AI query. Duplicate project names remain distinct by origin and directory or project ID.
Missing selections are explicitly marked unavailable rather than displaying an unrelated first
option. On narrow screens the same selectors stay in the header; the breadcrumb and Source link
yield space before the context controls do.

### Project version

Every store of a project is kept per [project version](project_identity.md#project-version): its Git
branch, or `main` without Git. The **Version** menu, between Project and Agent, selects which one the
UI reads and writes. It lists what `GET /api/project/versions` derives on the fly from the stores
themselves — nothing records versions separately:

| Project | Versions listed |
|---|---|
| Workspace | the default and current branch; the branch each worktree has checked out (group **Checked out**); every other version with data — native Lance branches of its Task, Memory and Dream tables and local AST/Knowledge layers (group **Other versions with data**) |
| Hub | **Default version**, the project's main line, plus every native Lance branch of its Task and Memory tables (**Shared versions**) |

Each option carries one sentence: default branch, where it is checked out, which data it has
(code index, docs, tasks, memory, Dream runs), and whether the branch no longer exists in Git. The
menu shows loading, a disabled "Select a project" without a project, and keeps the selection with an
explanation when listing fails.

The selection is remembered per project target. "" — the default — follows the version checked out
in the selected directory (for a Hub project, its default version). The API client adds
`project_version` to every request that names the active project by `project_dir` or `project_id`
in its query, JSON body or upload form, copying a body-only address to the query so the server
selects the version before the handler runs; streams, raw fetches and uploads included. Requests
about another project are untouched. Changing the version remounts the routed pages, so every page
reloads with that version's data. For a Hub project, AST and Knowledge read the artifact published
from the branch (`branch/<version>`) when there is one and otherwise the latest, saying so.

A Hub project has no checkout to tell its default version's name from a new version, so Task and
Memory writes by `project_id` use a version's branch only when that version is confirmed real — it
already has a Lance branch in the project's Task or Memory store. A version with no branch anywhere
reads the main lineage and its writes are refused with `409`; writing from a checkout of the
version creates it.

Page-specific context choices (indexed AST/Knowledge source, Memory scope, publisher filters or a Live run configuration) remain in the relevant page. These describe the work being inspected or executed, not a second global project/agent selector. Project metadata in a dossier remains appropriate when it adds information beyond the selected name.

Routes that require a checkout render the shared **Local workspace required** state while a Hub
target is selected. The state names the retained remote project and directs the user to the global
Project selector; it does not execute the route with an empty path or the previous Workspace path.
Local artifact management/publication, Live workspace search and Dream analysis use
this guard. Workspace Now uses the authorized native working-context header and the
Rust Web Now stream, including instances publishing the same project/version. Task, Session, project Memory, Knowledge and AST remain available remotely.

Switching projects updates:

- mutable project name and immutable ULID identity;
- Knowledge, AST, Memory, and Task navigation entries;
- Hub target context;
- explorer API parameters;
- ecosystem and system views that depend on project scope.

## Main experiences

### AST Explorer

The AST relationship reader chooses a related entity type from the scoped schema. It projects only that type’s available fields; a missing identity/schema is an explicit limitation with Query lab as the next action. Potential impact follows up to two incoming call hops, including intermediate types. The explicit source/impact inspector bounds each typed query to 100 results. The map neighborhood instead provides continued pages for every declared direct endpoint partition.

The default **Find & inspect** workspace starts with a question, not a graph sample. Search indexed symbols and files within the selected project or imported context, then explore the returned candidates in **Relationship map**. Selecting **Inspect source & impact** opens indexed source in that same workspace. **Source**, **Incoming**, **Outgoing** and **Potential impact** answer distinct questions about that implementation.

A failed **Read entity source** request reports its causal error beside Source and preserves the previously loaded source for that same file and scope. **Read entity source** retries explicitly; **Read complete file** requests the full file. Selecting another file or scope clears the previous source. Ambiguous or unavailable entity selection remains an error; the browser does not fabricate a snippet or claim a full-file response is the selected entity.

Relationship inspection uses the primary key declared for the selected label, not a renderer ID or a guessed name, path, line or UID. Search observations resolve that key to one published node before traversal; a missing, ambiguous or changed key/UID produces an explicit error. Local syntax entities use the reserved `entity_key`; imported artifacts retain their own label/primary-key metadata. In the explicit source/impact inspector, incoming/outgoing results are limited to 100 distinct nodes; potential impact follows incoming CALLS up to two hops. These are indexed candidates, not a claim that every dynamic runtime dependency is represented. Select a related result to continue investigating its source.

**Query lab** offers direct read-only Cypher and **Draft with AI**. Generation fills an editable draft; it does not execute. The user reviews scope, relationship and limit before choosing **Run query**, which opens Relationship map immediately with loading feedback. Query lab retains its editor draft. Result tables in the map retain full values and offer explicit CSV export. Project/context changes invalidate outstanding results.

**Relationship map** loads a bounded sample only when no explicit request is active, or explores the current search/Cypher result. Empty, scalar and failed requests are never silently replaced by a sample. A stable catalogue groups entities by directory, file, language or configured cluster. Selecting an entity loads its incoming/outgoing declared endpoint partitions, up to 100 relations per partition and page. Source and typed relationship inspection remain in the same workspace. Filters do not hide queried neighbors; entity actions retain keyboard access and narrow screens stack the panels.

AST search uses public entity identities without graph/search/Source/model generation fields. Text, graph and semantic publication advance independently; a fresh response remains usable while another file is indexed. Schema metadata supplies labels, relationship types and primary keys, and does not rewrite returned IDs. Physical NODE/REL/endpoint IDs remain scoped to their own response; a query rejects mixed physical-ID scopes within that one result. Neighborhood partitions may observe different graph publications and join nodes by their public primary keys, after checking the anchor label, key and observed UID. Each opaque continuation cursor remains bound to its own query, literal project/artifact version and anchor; it is never transferred between partitions. Parallel edges retain their native identity within each response, even when UIDs coincide.

Project/context changes and cancellation discard late responses. Header refresh reloads the active result and neighborhood. Missing schema metadata remains an explicit limitation rather than inferring a primary key from UID. Published file observations are independently queried when their disclosure opens; an older schema does not invalidate them. A missing or ambiguous file observation is shown locally and does not replace the literal Source. Counts describe returned indexed evidence, not repository completeness. See [Investigate code relationships](../guides/code_investigation.md).

**Indexed contexts** is a searchable directory followed by an origin dossier and **Investigate this context** action. Imported-context unlinking is confirmed and scoped to the selected project; it removes that project’s reference without deleting the shared index. Tables scroll within their containers; investigation, source and controls stack on smaller screens.

For a Hub project, the directory is built from `/api/projects/{projectID}/contexts`. The explorer
shows separate artifact and published-version selectors. “Latest” is a presentation choice only:
the client resolves `qualified_latest` and every schema, search, graph, file and generated-query
request carries `project_id` plus the exact `id@version`. The server opens that published store
read-only on demand; it neither installs the context nor creates a local project claim. Unlink and
other local mutation actions are absent for these rows.

Labels, declared endpoint types, primary keys and exact counts come from the Native schema endpoint of the selected project or exact imported artifact version. The frontend does not maintain a global hard-coded relationship map or read storage manifests directly. Native integer parsing preserves counts and IDs without converting large values to imprecise JavaScript numbers.

### Knowledge Explorer

Knowledge uses a library → reading → evidence journey for maintained project and imported documentation. Select a knowledge context, filter the library by title/path/type, or use keyword search. **Answer with sources**, when enabled, separates the generated answer from its retrieved documents. Selecting a result opens the authoritative document.

The reading workspace brings the document into focus and makes the search/context region collapsible. It retains provenance, type, tags, indexed confidence, source path, references, backlinks, raw Markdown, copy and previous/next document history. Explicit wiki links can address another known context using `context/page`; normal source-file links retain their source-path meaning. Cross-context navigation and project changes invalidate stale lookups. Images open in the shared modal viewer with zoom, pan, reset and Escape/focus restoration.

**Knowledge contexts** is a searchable collection directory. Selecting a row shows origin, location, page count and changelog availability before **Read this collection** opens its library. APIs remain under `/api/wiki`; no independent copy of knowledge state is created in the UI.

For a Hub project, Knowledge exposes artifact and published-version selectors. “Latest” resolves to
`qualified_latest` before the library opens, and pages, documents, keyword search and AI-assisted
search all send the immutable `project_id` + `id@version` pair. The handler mounts the published
LanceDB URI read-only for that request and never treats the reference as a filesystem directory or
persists an installation. Switching project, artifact or version clears previous pages, documents,
answers and outstanding requests before the new source is shown.

Persisted record links show projection warnings once per requested source in a collapsed, keyboard-accessible disclosure. Expanding it retains every returned warning and locator detail. Outgoing and incoming completeness remain separate, and the partial-view notice stays visible when either requested direction is incomplete.

### Memory Explorer

Memory is a separate domain surface at `/memory/explorer/<scope>/<memory-id>`. It does not render
the Wiki Explorer, consume wiki modules, or call `/api/wiki`. Its `/api/memories` handlers open the
selected authoritative Memory table through `MemoryService`.

The explorer provides:

- project and user scope selection;
- direct full-text search and filters for type, tag, importance, and mandatory status;
- canonical ordering by mandatory, important, and normal groups, with newest memories first inside
  each group;
- create and edit forms over Markdown source;
- dedicated Markdown rendering in the detail view;
- authoritative metadata including scope, project, author, timestamps, revision, links, and hash;
- a navigable current/superseded revision chain;
- importance and mandatory controls; and
- explicit confirmation before removal.

Memory presents a compact current-record catalogue followed by a guidance dossier. Its revision strip exposes current and historical versions, with an explicit historical notice. Editing from a historical view updates the current memory; it does not restore or overwrite the selected historical version. Authoritative identity, timestamps, author, scope, hash and predecessor/successor addresses remain inspectable beside the guidance. Persisted record relationships follow that metadata in the supporting column, after the guidance and classification in mobile DOM order, so their asynchronous loading extends the dossier without moving earlier content.

Create/edit uses a focused modal: content and title are primary, while classification and startup policy explain how future work will retrieve the memory. Existing tags remain unchanged during edits. The editor keeps its actions visible while long content scrolls and restores focus when closed. Removal requires explicit confirmation.

The Memory API consists of `GET /api/memories`, `POST /api/memories`,
`GET /api/memories/{id}`, `PATCH /api/memories/{id}`, and
`DELETE /api/memories/{id}?confirm=true`. Each request accepts exactly one project address:
`project_dir` for a workspace checkout or immutable `project_id` for a Hub project, plus `scope` and
an optional `project_version` as appropriate. The addresses are mutually exclusive. Project memory
follows the selected project and version — bound to the requested checkout, never to the server's
own working directory;
user memory remains the authenticated user's private scope. `GET /api/memories/scopes` reports the
directly addressable scopes.

### Task Explorer

Tasks and Sessions share a full-width, bounded catalogue followed by a reading dossier. Tasks start with next action, active acceptance-check progress and evidence; specification, comments, lifecycle and immutable revisions remain directly addressable. Accountability and the session action sit alongside the reading column, followed by persisted record relationships; that supporting column stacks below the reading content on mobile. Superseded checks remain visible but do not count toward active acceptance progress.

Sessions preserve one evolving request: next step and latest checkpoint come first, followed by request/strategy, linked tasks, checkpoint decisions/problems, specification revisions and a readable lifecycle timeline. The complete JSON record remains available for inspection. Continuity metadata and its notice lead the supporting column; persisted record relationships come last in that column and in mobile DOM order. This prevents asynchronously loaded links and backlinks from shifting the session's primary work or continuity context. The catalogue retains status, search, active-only and cursor pagination; each row shows the authoritative completed/total linked-task count with a compact progress bar when tasks exist, while an empty session says `No tasks` without implying completion. Task/session links preserve the selected project.

Task discovery uses the native `POST /api/tasks/catalogue` reader with an authorized
Project/Version selection, filters and opaque catalogue cursors. Catalogue rows contain
summaries, without audit snapshots or full comment bodies. Selecting a task calls
`POST /api/tasks/get` with `{id}` and immediately displays the current specification,
checks, ownership, progress and creation evidence. A `null` result means the task is absent;
`history: null` and `comments: null` mean those sections have not been loaded, not that
no events or comments exist. The current JSON disclosure retains those distinctions.

**Load history** and **Load comments** explicitly request one 20-item page through the
same TaskGet reader. Their first request echoes the corresponding opaque start cursor
from the selected current result, preserving its generation. Later pages echo
`next_cursor` unchanged. Each section keeps one content page and a stack of previous
cursor strings; it replaces the content when moving forward or back. Lifecycle and
specification revisions share the same history page. The first revision comparison uses
that page's authoritative `before` predecessor, and subsequent comparisons use the
preceding event in the page. Empty-state messages describe the loaded page only.
Loading and errors remain local to the affected section, with an explicit retry that
retains the requested cursor. Current authorization is revalidated by the producer;
401/403 invalidates the selected workspace. Ordinary publications do not silently
change the pinned audit snapshot.

Changing task, native project path, version or current generation invalidates section
requests and discards their page content. Aborted responses cannot replace the new
dossier or invalidate its workspace. Navigation uses native buttons with visible focus
and disabled/busy states; the existing reading/support columns stack on narrow screens.
Record-reference bindings continue to use the exact selected project/version and Task ID.

**Export complete JSON** and **Export all tasks** explicitly traverse the pinned history
and comment pages. Only the download assembles complete audit content, as serialized
Blob parts rather than a browsing cache of snapshots. The project export traverses the
catalogue and reads each Task's current result before its pinned sections; it does not
claim one transaction spanning every Task in that catalogue. Failed, missing or denied
reads do not produce a partial download. Scope changes and unmount abort export work.
Native integer parsing/serialization preserves revisions, sequences and generations
without rounding. The UI does not decode audit cursors or reconstruct authoritative
Task state, permissions or completion decisions.

### Session current state and audit

Selecting a Session reads current state through the native SessionGet reader with
`input: {id}`. It returns `session`, `creation`, nullable `last_event`, `generation`,
`history_start_cursor` and `history: null`. The request, strategy, latest checkpoint
and next action appear immediately; linked Task discovery has independent loading,
error and retry states. A slow or failed linked catalogue does not erase the current
Session. An absent Session is distinct from unloaded history.

**Load history** explicitly requests `{id, history: {page_size: 20, cursor}}` through
that same reader. The first request echoes `history_start_cursor`; continuation
passes `next_cursor` unchanged. Checkpoints, specification revisions and lifecycle
share one returned `{before, events, next_cursor}` page. Moving forward or back
replaces that content; only previous cursor strings are retained. The first revision
comparison uses the native `before` Session, including at a page boundary. Empty
messages and checkpoint counts describe the loaded page, never the whole history.
Creation remains visible while historical events are unloaded.

The current-response JSON disclosure preserves `history: null`. **Export complete
JSON** is a separate action that traverses the Session's pinned pages to EOF and
assembles serialized Blob parts for the download. A missing, unloaded, failed or
denied page prevents the download. No cursor is decoded or used to select authority,
a corpus path or another version.

Changing the selected Session, native PROJECT/version, API base, Web identity or
current response discards the old dossier's history and aborts its pending reads and
export. Explicit cancellation and unmount ignore late results and errors. Loading,
retry and page controls reuse the existing dossier layout, focus styles, native
buttons and responsive reading/support columns. The original request, admission,
EOF, branch provenance and integrity checks remain responsibilities of the native
[Session reader](native_task_store.md) and [shared protocol](rust_task_protocol.md).
These consumer sources await the coordinated native build and installed Browser
qualification; earlier Task Get qualification does not establish that result.

### Hub

The Rust consumer source separates three workspaces. `/hub/installed` remains the
read-only installation-wide AST, Knowledge and language library inspector.
`/hub/registry` browses explicit Broker or Standalone sources, their publisher
projects and cursor-paginated artifact catalogues. `/hub/local` currently inspects
FILE installations in an explicit PROJECT or the installation cache. The latter
two consumers are being integrated under `tsk-b788`; component fixtures and type
checks do not establish qualified HTTP or browser behavior.

Native context inventories for Task, Memory, Knowledge, AST, Hub and Now return
`projects: [{project_id, name}]` as display metadata for their own authorized
contexts. The shared Project trigger and options show this name. Record URLs
carry only `project_id` and `project_version`. The Browser resolves that locator
against the current module's authorized contexts before sending the physical
selection to the API. When multiple checkouts match, the selected or current
checkout can disambiguate. Otherwise the user chooses a checkout.
Missing names retain the exact ID, and duplicate names remain distinguishable
by the existing checkout or remote-origin details. Metadata is cleared with its
module, identity and API scope, including failed inventory loads.

The FILE Hub routes bootstrap the shared header with POST /api/hub/contexts and an empty object. These are active local Source PROJECT tuples authorized by Hub/read, with exact native path, project ID and data version; Task, Now, AST and installed-inventory grants do not replace that decision. The current tuple is selected only when returned as authorized; current:null requires an explicit Project choice. Version options come from those tuples, with no legacy global-projects, project-catalog or project/versions requests. The Agent menu retains the eight current literal bindings as an explicit destination choice. Without a PROJECT, remote catalogues and GlobalCache remain available while PROJECT actions and publication require a selection. Identity/API changes invalidate bootstrap responses; choosing another version preserves an already submitted mutation under its original tuple.

The registry shows literal type/ID/version filters, declared versions and latest,
publisher and Markdown descriptions before installation. Install/update selects
an explicit FILE version and PROJECT/binding or GlobalCache destination. PROJECT
uses the captured header identity, project ID, real resolved version and current
binding; GlobalCache does not invent a PROJECT. Supported wire bindings include
`kiro-v3`, `opencode` and `antigravity`, alongside the other current native
bindings. No `main` version or grant is inferred.

PROJECT install and update show the captured destination and explain that the
checkout receives copies. Agent, Skill, Command and Workflow content is
rendered for the selected agent before installation. Blank alias and entry name
on update preserve their previous values. GlobalCache sends no PROJECT
projection options.

FILE operation feedback receives literal Unicode path strings, including possible parent residues and every owned/pending Forget preview source, destination, staging, old destination, payload and backup. It renders that text directly, preserving spaces, accents and escapes. Invalid Unicode is rejected by the service; there is no byte-array decoder or hexadecimal fallback. Parent residues deduplicate by their exact path string. Preview modes report stored ArtifactMode observations. PROJECT publication requests Copy. These observations retain captured identity, effect/pending flags and explicit inspection; Forget changes logical ownership and never grants deletion of the displayed paths.

Every installed FILE entry supplies an explicit source_origin: HubCache, Resources or LocalLink. Inventory rows and the inspector display that provenance, independently of physical Copy/Link. The separate origin field carries only the remote Broker/Standalone transport or null; null never classifies the source as local. HubCache with null remote origin remains HubCache and has no remote Update/content controls. Resources and LocalLink require null remote origin; contradictory or missing/unknown source origins fail the inventory response rather than gaining a default. Remote Update and installed content require both HubCache provenance and a concrete remote origin. Existing PROJECT Remove, Recover and Forget remain available independently of remote eligibility and submit only the captured PROJECT/data version, artifact type/ID/concrete artifact version and binding. They reuse the existing lifecycle endpoints; Remove preserves the source and Forget retains physical files and residues. Provenance participates in selection identity, so a refreshed entry with a changed source origin closes its previous inspector. These fields do not grant source reading or accept a free host path. The Root browser observed LocalLink provenance with null remote transport after real local installation, and Auth confirmed owned cleanup. Focused tests cover the other source-origin and rejection branches; separate M1 security and Global obligations remain open.

Installed FILE actions preserve scope: PROJECT supports update, remove, recover
and logical forget; GlobalCache supports remove of a concrete version and
preserves cached payload. File content uses an explicit relative path and retains
remote-ephemeral versus installed-Verified provenance. Catalogue availability and
paths do not authorize reads, mutation or deletion of physical residues.

Publisher discovery belongs to its effective source, provider and storage profile. Changing that tuple cancels and clears the old discovery, including errors and cursors; use Browse publishers explicitly for the new source. A discovery failure in the same tuple remains visible when a separate catalogue search succeeds. Draft edits alone do not request discovery or grant access.

Mutation feedback retains the captured target, source/producer when supplied,
and confirmed, pending, cleanup and unknown effects, including while the request
is running. Changing PROJECT/version/binding or installation destination keeps
the owned mutation and its original receipt; its completion cannot reload another
selection. Incompatible dialogs close permanently. Changing API base, Web identity
or logging out aborts the client owner and discards its private result. Successful receipts must match the submitted selection and
report recognized publication effects; install/update also require a confirmed
cache result. A known unchanged result with `committed:false` can complete.
Unknown changes and incomplete receipts require inspection, including HTTP413
with only the inspection header. A late inspection cannot release a later
mutation. Within the same scope, completion refreshes the current query or filters. Errors retain drafts and effects, and no mutation is retried
automatically. Context changes invalidate old responses and incompatible dialogs;
shared header refresh preserves applicable filters, selection and drafts. Dialogs
retain focus isolation, Escape and focus restoration.

Project resources uses the authorized PROJECT and data version from the shared header. Choose Browse project resources, filter the current Resources inventory by type, exact ID or artifact version, select a resource, and inspect its paginated tree. Select a listed relative file to read bounded text; there is no host path or installed-cache reader in this journey. All ten artifact types remain readable; AST and Knowledge do not expose FILE publication. LIST entries return required name:null; REVIEW returns metadata:null and says that no source metadata descriptor is supplied. These are separate DTO fields: missing name is not replaced with metadata, and no descriptor is fabricated. Empty text, missing404, non-text415, limit413, denied and transport failures remain distinct. Each list/review/content request captures current source independently, with no immutable snapshot or invented digest. Refresh from the first list page to include new items before the lexical cursor; no universal cursor reset is inferred.

The local installation consumer starts from the selected authored Resources entry and supports agents, skills, commands and workflows. Install local resource captures that source PROJECT/data version/type/ID/artifact version separately from the current PROJECT installation scope and literal Agent binding; no extra project picker or host path is accepted. Other resource types remain readable and retain their existing FILE publication eligibility. Global installation cache is not a local-install destination. These local Agent, Skill, Command and Workflow installations render the source for the selected agent and offer Copy or Default (Copy), with no Link option. LocalLink records source provenance; it does not imply physical Link. Omitting mode uses Copy; omitted alias/entry name preserve existing options. Empty form fields omit those names, while nonempty local names are sent literally and invalid values remain visible for explicit correction. Explicit null is never treated as omission. Local installation reads the source and mutates the captured destination under separate Web admissions; it does not publish, request storage credentials or require a remote Hub/write grant.

Local installation reuses the existing mutation owner, confirmation modal, optional fields, requested-mode feedback and captured-target inventory inspection. Success must correlate operation install_local/source local/producer and both destination representations before completion. The response does not echo alias, entry name or mode: feedback labels requested intent, and refreshed inventory supplies its observed facts. Confirmed publication can survive a later error; named reconciliation/cleanup/observation flags and bounded-response inspection remain intact. No generic pending field, rollback, silent fallback or observed mode is invented. The source consumer has 40 focused new cases and 16 affected shared cases passing. Its single production build and exact three-asset promotion passed; the previous a68 authored bundle remains historical evidence. The selected-resource workflow includes explicit confirmation and owned cleanup.

SourcePROJECT FILE publication starts from a selected, reviewed FILE resource in Project artifacts. Its FILE type/full ID/concrete artifact version are captured source facts, distinct from the header PROJECT data version; the form no longer accepts a manually entered source tuple. Enter the publication name and optional metadata/dependencies, review the captured source and draft, then explicitly Publish artifact. Omitted dependencies preserve them; replacing with an empty list clears them. The latest pointer changes only when selected, and replacing different destination bytes at the same version requires the expected archive SHA256. That catalogue precondition is not a source snapshot digest. Publication captures the current selected Resources tree independently and can fail if it disappeared or changed. No path, package upload, binding or GlobalCache destination selects this source. Read admission does not grant publication authority.

Confirmed success requires a matching producer/type/ID/version receipt. Uncertain metadata or storage effects, cleanup residue, an unobserved result or HTTP413 require explicit inspection of the original remote catalogue before another attempt. Absence from that catalogue does not prove rollback or absence of physical residue. Inspection belongs to the result epoch; changing PROJECT or the installation scope retains the submitted tuple while closing the incompatible dialog. Identity/API changes tear down the owner.

Remote unpublication is an independent Registry action for every published artifact type. Use the authenticated Broker catalogue, inspect a full artifact ID, and choose **Unpublish version** after selecting a literal version, or **Unpublish artifact** for the whole catalogue entry. Each opens an explicit confirmation; whole-artifact confirmation explicitly covers all published versions, including releases added concurrently. An optional expected archive SHA256 applies only to the selected version. Catalogue visibility does not grant write authority, and no local PROJECT, Agent binding or installation destination is required.

Only the remote catalogue is retracted: payloads, source and existing installations remain. If the retracted version was latest, latest becomes absent; the client never chooses another version. The receipt lists the versions actually observed by the server and retains the resulting latest/catalogue state. A failed HTTP delivery may still contain a confirmed updated/removed catalogue receipt, which remains visible without a success message. Uncertain, unknown, oversized or otherwise unobserved results (including cancellation without a confirmed receipt) require explicit **Inspect original catalogue** before another unpublication. Inspection reads the captured Broker/provider/producer/type/ID, never installed inventory; an older inspection cannot unlock a later result.

The operation owner survives closing the inspector, header PROJECT/Agent changes and catalogue refresh. Changing the effective remote tuple closes an incompatible dialog while keeping its submitted request/result. Identity, logout and API-base changes abort and discard private results. No automatic retry occurs. Incorrect If-Match DELETE returns 412 without effect in the Filesystem backend; MinIO without ConditionalDelete support is refused with not_attempted.

The required broader publication journey remains Package → Metadata &
dependencies → Review & publish, with a focused metadata/review dialog for an
existing artifact. Descriptions use the shared Markdown renderer while drafts
and submission retain authored source; review never publishes implicitly.
Submit/Upload must use the active project publisher ULID, never a selectable
global artifact-owner namespace. AST import requires an exported `.ast` package,
Knowledge an exported `.knowledge` package, and other FILE artifacts a `.zip`;
the producer must validate envelope, version, declared type and native store.
These requirements remain pending producer/consumer qualification.

### Live Search

Live Search separates **Prepare context**, **Run & evidence**, and **Recent sessions**. Choose compatible artifacts and an Agent, then start a run in an ephemeral project. The execution view keeps streamed evidence, connection state, prompts and cancellation together. Select a recent session to resume its console; removal retains its confirmation. Reconnection and event deduplication remain transport responsibilities. Ephemeral work is separate from persistent project stores.

### System views

**Projects** is a unified identity directory with one row per project ID and explicit Workspace/Hub
presences. All Projects and Same Cluster compare cluster metadata from both sources. The dossier
shows the local path when present, Hub revision and remote-context capabilities; **Use workspace**
and **Use Hub context** change the one global selector. Cluster editing and unregistering remain
local-only operations with explicit confirmation. A partial source failure keeps the available
catalogue visible and identifies the unavailable source. **Daemon** shows the global process PID
and state, alongside the portable MCP agent configuration. **Dream** separates runtime
conditions from latest-run metadata, with scope changes clearing previous evidence.
**Workspace** is a job chooser (Continue, Understand, Reuse & share, Operate) with project metadata
and a link to project administration. Global context selection stays in the header.

- **Daemon** reads process status and supplies the MCP stdio configuration.
- **Dream** exposes configuration, session state, and the latest operational run. Memory holds the semantic result.
- **Ecosystem** lists registered projects, labels, and active project identity.

### Daemon status

The browser reads `GET /api/daemon/status` through its authenticated same-origin
Web session. The Rust response is direct JSON: `{ "pid": 123, "state": "running" }`.
`pid` is a non-null unsigned 32-bit integer; `state` is `running` or `stopping`.
The read uses `no-store`; an absent or invalid session returns HTTP 401, and the
server rejects a foreign Origin. The screen refreshes every five seconds without
overlapping a pending read. Header refresh supersedes an older read. Identity/API
changes and unmount abort pending requests and discard their late responses.
Read failures clear the observed process result and expose the causal error with
an explicit retry action.

**Connect an agent** provides this portable configuration, with `graphit` available
in the agent's PATH:

```json
{
  "mcpServers": {
    "graphit": { "command": "graphit", "args": ["mcp", "--stdio"] }
  }
}
```

## Browser and Source server boundary

`graphit-app` Browser/Source composition:

- serves the embedded SPA;
- resolves the active project and imported contexts;
- exposes JSON handlers for AST, wiki, memory, Task, Hub, live search, daemon, Dream, and ecosystem operations;
- applies one UI host and exact-origin CORS configuration;
- returns domain-friendly names rather than storage implementation names.

The built-in UI host is the IPv4 loopback address. The frontend uses same-origin `/api` URLs so it continues to work behind a correctly configured reverse proxy.

When Global `task.browser` is configured, the server requires its verified Broker session for Browser data APIs; the opaque cookie points to credential state held in daemon memory. The header shows the verified identity or anonymous session state. CORS still limits browser origins but does not authorize
non-browser clients. A multi-user Hub deployment must also protect its listener and configure Broker Hub grants per user. See
[S3 Credentials and UI Network Configuration](../guides/s3-and-ui-network.md) and
[Hub Access Control](hub_access_control.md).

## Error and loading behavior

- Requests increment and decrement a shared loading counter.
- The global working indicator remains visible while at least one request is active.
- Route-level empty and error states explain what is missing and provide a relevant retry or navigation action.
- Switching projects must not render stale data as if it belongs to the new project.
- A failed context load retains enough route identity for the user to recover.

## Acceptance criteria

- Every documented route is reachable from desktop and mobile navigation.
- Light and dark modes preserve the same semantic hierarchy.
- The active project is visible on project-scoped surfaces.
- AST relationship names match the active manifest's friendly names.
- Screenshot and public documentation examples use the current Graphit Observatory UI.
- The production bundle and embedded assets are generated together with `node scripts/build-rust-ui.mjs`.
- Task discovery uses a bounded paginated catalogue; exact detail and explicit JSON download consume the canonical complete export contract.
- Memory routes use only the dedicated Memory API and render Markdown without a wiki-page adapter.
- The embedded server serves the SPA and same-origin API calls.
- The default network configuration remains local; documentation does not present CORS as authentication.
- Hub discovery is bounded and filtered before project or artifact metadata reaches the browser.
- A stale Hub cache never grants detail, content, install, publish, or mount access.

## Visual reference

The shared [design system](design_system.md) defines the current visual identity. Previous Observatory screenshots are not a reference for this layout.

### Current project activity

Workspace Now uses cookie-authenticated Rust endpoints: `POST /api/now/web/contexts` resolves exact retained native paths, project IDs and versions allowed by the Broker; `POST /api/now/web/subscribe` receives NDJSON full snapshots for the selected context. Both require the configured same Origin. The native header provides Project and Version selection without the legacy workspace catalogue or agent-based identity. The stream displays this daemon's published local heads and other authorized instances of that project/version, with instance IDs and leases. Broker PresenceRead PROJECT authorization is independent of JWT authentication; its module mask preserves separate Task, Memory and Knowledge permissions. Forbidden sections expose no items or known counts; missing corpora and empty available sections remain distinct.

Session items expose the authoritative completed/total linked-task count: every non-cancelled linked Task contributes to `total_tasks`, while only `completed` contributes to `completed_tasks`. Cancelled Tasks remain visible in the linked inventory and history. Now presents that count as text plus a compact progress bar when applicable tasks exist, and as `No applicable tasks` without a bar for zero, including sessions with only cancelled Tasks. Native integer parsing preserves exact counts. Record links carry the admitted native context into authorized dossiers; complete progress/next-action bodies, raw snapshots, audit history, fencing tokens and document/memory bodies are excluded from the broadcast. Metadata remains the producer's allowlisted, ordered, 20-item section view. Task/session claims must be in progress and unexpired; personal Memory and imported corpora are excluded.

Each instance shows `Observed at` from the snapshot’s original `generated_at` (UTC epoch nanoseconds), formatted in the viewer’s locale. `Live updates` describes the stream connection; receiving the same snapshot again does not mean its records were observed again. The reader preserves `generated_at` exactly, and the view changes its observation time only when the producer sends a different timestamp. No client receipt clock or heartbeat resets that time.

Updates arrive through the Broker realtime channel without periodic Now requests. The UI cancels the old stream on context change or unmount, ignores late responses and withdraws disconnected snapshots. It shows reconnection state and reconnects after transient failures; authentication, forbidden and invalid-selection failures require an explicit retry. The shared header refresh refreshes working contexts and restarts the owned stream. See [Rust Now and Presence integration](rust_now_presence_integration.md#web-now-consumer).

`GET /api/project-catalog` returns `workspace_projects` and `hub_projects` independently, with
`workspace_error` and `hub_error` for partial failures. Clients deduplicate both sources by immutable
project ID. `GET /api/projects/{projectID}/contexts` returns Hub project metadata, live Task/Memory
capability markers, and Knowledge/AST entries with `latest`, `versions`, and `qualified_latest`.
Artifact reads use a concrete `id@version`; choosing `latest` first resolves it to that exact value.

### Incremental agent responses

Knowledge AI search and AST AI generation accept `Accept: text/event-stream` on their existing POST endpoints. The server reads the complete JSON request before flushing any SSE headers or events, including on HTTP/1.1 connections. Stream requests are limited to 1 MiB; unreadable bodies return HTTP 400 and oversized bodies HTTP 413 as JSON errors before the agent starts. The UI displays that error message. Ordinary JSON callers remain compatible. SSE emits `progress` with normalized public CLI events, then `final` with the existing response object or `error`, and `done`. Native CLI session IDs remain server-side for streaming clients. Request cancellation terminates the underlying CLI through the existing conversation context. Cypher generation produces only an editable draft; execution still requires Run query.

Live keeps its existing replayable SSE protocol: subscriber disconnect does not cancel the session. On reconnect, `Last-Event-ID` (or `last_event_id` when the header is absent) is an exclusive cursor: both disk replay and newly arriving events skip IDs through that value, even when the cursor is ahead of the log at subscription time. Its execution panel includes thinking, stdout diagnostics, stderr and tool results. `GET /api/live/sessions/{id}/knowledge/page?page=slug&context=name` opens an existing source from the retained investigation workspace. A qualified `context:slug` can select a source; an ambiguous unqualified slug returns a candidates object for explicit selection. Unknown sessions, missing pages and removed sources do not fall back to another workspace. Published sources are reopened through the authorized Hub mount. The endpoint neither installs artifacts nor creates a missing workspace.

### Live artifact catalogue and preparation

`GET /api/live/artifacts?agent=name&project_dir=...` lists the active registered project instance and its own AST/Knowledge indexes, installed contexts and agent artifacts. Responses include a structured reference (`source: project`, `project_id`, opaque `instance_id`, `project_kind`, `id`, `type`, `version`), display provenance and an optional `unavailable` reason. Filesystem source paths are server-side. The Live picker also consumes every `/api/registry` cursor, including empty filtered pages, and retains usable project sources if the Hub is unavailable.

The session creation API accepts these references in `artifacts`. An omitted `source` remains a Hub reference for existing clients. Preparation re-enumerates local sources and rejects changed identity, version, agent or instance. Local AST/Knowledge links use the source project identity and existing index; local packages are copied into the ephemeral workspace with resources, then projected through the selected agent adapter. No local source is initialized, indexed, published or edited. Published installed contexts still pass through the Hub installer and its access checks. Unsupported local package types are labelled unavailable; invalid local references fail preparation. Existing Hub partial-install diagnostics remain visible in the execution stream.

An investigation cannot select different versions of the same published artifact: preparation rejects that boundary before installation instead of silently replacing one version. Local skill copies receive a bounded, collision-resistant directory name and matching YAML `name`; descriptions and additional metadata remain intact. Adapter projections preserve executable permission bits on package scripts. These changes affect only the investigation copy and its adapter projection.

The registry response includes `available`; Live distinguishes an unconfigured Hub from an empty search result. Free-text artifact search also matches artifact type, so terms such as `knowledge` and `skill` work alongside names and tags.
