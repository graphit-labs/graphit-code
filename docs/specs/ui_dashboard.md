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

It does not add authentication, replace network controls, or maintain an independent copy of domain data.

## Frontend architecture

The SPA lives in `internal/ui/`.

| Concern | Implementation |
|---|---|
| Build | Vite |
| UI | React + TypeScript |
| Styling | Tailwind CSS plus semantic CSS tokens |
| State | Zustand stores with selective persistence |
| HTTP | Axios-based API modules |
| Markdown | Dedicated Knowledge and Memory renderers over their respective domain records |
| Graph | D3 force simulation and canvas rendering |
| Packaging | Production assets embedded into the Go binary |

`npm run build` writes `internal/ui/dist/`. The Go UI server embeds that output and serves it with same-origin API routes.

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

The app store loads machine-local ecosystem projects from `GET /api/global-projects` and persists
the selected project directory under the `graphit-app-state` browser key. That local list is not
the remote Hub project directory. Hub discovery uses its own ACL-filtered cursor API.

Every project-scoped request must use the active project directory or context. Because the selection survives browser sessions, the common header must display the active project clearly enough for a user to verify it before interpreting data.

`AppShell` mounts one `WorkspaceSelectors` group in the right side of its header on every route, including all explorers. It is the only place that changes global project and agent selection. Sidebar, mobile navigation and page toolbars do not repeat these controls or a separate selected-project badge. Each themed dropdown has a persistent label, selected checkmark and keyboard/type-ahead support; switching one leaves the other unchanged. The selectors write the existing app store and retain its browser persistence.

Controls remain visible while loading or empty, with descriptive disabled options. The single Refresh action reloads available projects and agents, then all data sources registered by the active page: catalogues, open details and runtime data. It stays busy until all finish, prevents duplicate activation and reports errors. Refresh never reloads the document, resets drafts or executes an unreviewed AI query. Duplicate project names include their directories in the options; the selected project's full directory is available on hover. Missing selections are explicitly marked unavailable rather than displaying an unrelated first option. On narrow screens the same two selectors stay in the header; the breadcrumb and Source link yield space before the context controls do.

Page-specific context choices (indexed AST/Knowledge source, Memory scope, publisher filters or a Live run configuration) remain in the relevant page. These describe the work being inspected or executed, not a second global project/agent selector. Project metadata in a dossier remains appropriate when it adds information beyond the selected name.

Switching projects updates:

- mutable project name and immutable ULID identity;
- Knowledge, AST, Memory, and Task navigation entries;
- Hub target context;
- explorer API parameters;
- ecosystem and system views that depend on project scope.

## Main experiences

### AST Explorer

The AST relationship reader chooses a related entity type from the scoped schema. It projects only that type’s available fields; a missing identity/schema is an explicit limitation with Query lab as the next action. Potential impact follows up to two incoming call hops, including intermediate types. Each typed query is bounded to 100 results.

The default **Find & inspect** workspace starts with a question, not a graph sample. Search indexed symbols and files within the selected project or imported context, then select a result to open its indexed source. **Source**, **Incoming**, **Outgoing** and **Potential impact** answer distinct questions about that implementation.

Relationship inspection first resolves the selected name/path/line to one indexed identity. Missing or ambiguous symbols produce an actionable message rather than merging unrelated results. Traversal uses the declared persistent identity (`uid` for symbols or `path` for File/Directory), never a canvas renderer ID, together with the selected relationship type. Incoming/outgoing results are limited to 100 distinct nodes; potential impact follows incoming CALLS up to two hops. These are indexed candidates, not a claim that every dynamic runtime dependency is represented. Select a related result to continue investigating its source.

**Query lab** offers direct read-only Cypher and **Draft with AI**. Generation fills an editable draft; it does not execute. The user reviews scope, relationship and limit before choosing **Run query**. Result tables retain full values and offer explicit CSV export. Project/context changes invalidate outstanding results.

**Relationship map** is an optional view of a graph result or an explicitly requested bounded index sample. Its filters preserve node labels, relationships, languages and clusters, with color overrides, 2D/3D, zoom, fit, physics and a file tree. A table of reached endpoints is not fabricated into graph edges. Source search, copy and line highlighting remain available in the source viewer.

**Indexed contexts** is a searchable directory followed by an origin dossier and **Investigate this context** action. Imported-context unlinking is confirmed and scoped to the selected project; it removes that project’s reference without deleting the shared index. Tables scroll within their containers; investigation, source and controls stack on smaller screens.

User-facing relationship names are resolved dynamically from the active project or context's `graph.icebug/icebug.json` manifest. `CanonicalRelGroup.Type` is the public name used by the translator, schema controls, filters, and canvas. Physical edge-table names are internal Icebug storage details and must not cross the explorer API boundary.

Context totals and the schema controls’ label, relationship-type, and language counts come from the
same canonical manifest. Listing a large project must not mount LadybugDB or run global `count`
queries; reverse relationship mirrors are storage-only and are excluded from the displayed totals.

The mapping is scoped to `project_dir` or imported context; the frontend must not maintain a global hard-coded relationship map.

### Knowledge Explorer

Knowledge uses a library → reading → evidence journey for maintained project and imported documentation. Select a knowledge context, filter the library by title/path/type, or use keyword search. **Answer with sources**, when enabled, separates the generated answer from its retrieved documents. Selecting a result opens the authoritative document.

The reading workspace brings the document into focus and makes the search/context region collapsible. It retains provenance, type, tags, indexed confidence, source path, references, backlinks, raw Markdown, copy and previous/next document history. Explicit wiki links can address another known context using `context/page`; normal source-file links retain their source-path meaning. Cross-context navigation and project changes invalidate stale lookups. Images open in the shared modal viewer with zoom, pan, reset and Escape/focus restoration.

**Knowledge contexts** is a searchable collection directory. Selecting a row shows origin, location, page count and changelog availability before **Read this collection** opens its library. APIs remain under `/api/wiki`; no independent copy of knowledge state is created in the UI.

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

Memory presents a compact current-record catalogue followed by a guidance dossier. Its revision strip exposes current and historical versions, with an explicit historical notice. Editing from a historical view updates the current memory; it does not restore or overwrite the selected historical version. Authoritative identity, timestamps, author, scope, hash and predecessor/successor addresses remain inspectable beside the guidance.

Create/edit uses a focused modal: content and title are primary, while classification and startup policy explain how future work will retrieve the memory. Existing tags remain unchanged during edits. The editor keeps its actions visible while long content scrolls and restores focus when closed. Removal requires explicit confirmation.

The Memory API consists of `GET /api/memories`, `POST /api/memories`,
`GET /api/memories/{id}`, `PATCH /api/memories/{id}`, and
`DELETE /api/memories/{id}?confirm=true`. Each request accepts `project_dir` and `scope` as
appropriate. `GET /api/memories/scopes` reports the directly addressable scopes.

### Task Explorer

Tasks and Sessions share a full-width, bounded catalogue followed by a reading dossier. Tasks start with next action, active acceptance-check progress and evidence; specification, comments, lifecycle and immutable revisions remain directly addressable. Accountability and relationships sit alongside the reading column and stack below it on mobile. Superseded checks remain visible but do not count toward active acceptance progress.

Sessions preserve one evolving request: next step and latest checkpoint come first, followed by request/strategy, linked tasks, checkpoint decisions/problems, specification revisions and a readable lifecycle timeline. The complete JSON record remains available for inspection. The catalogue retains status, search, active-only and cursor pagination; task/session links preserve the selected project.

The Task Explorer uses the lightweight `GET /api/tasks` catalogue for paginated discovery. Its
server-side text and lifecycle filters show status, priority, flags, and dependency blocks without
transferring checks or audit history for every task. Selecting a task loads the exact versioned
export document and presents its robust specification, current ownership/progress, checks and
evidence, dependencies, subtasks, typed
comments, lifecycle events, immutable specification revisions, and raw JSON. Users can download
either the complete project document or the selected task/subtask document.

The frontend never reconstructs authoritative Task state. Catalogue summaries and derived readiness
fields come from the Task service, while detail arrays come from the shared export service. Query,
status, project, and page size are bound into opaque cursors; stale responses are discarded when
the view changes.

`GET /api/tasks` accepts `project_dir`, `query`, `status`, `page_size`, and `cursor`. It returns a
bounded `results` array and `next_cursor`; page size defaults to 20 and is capped by the shared API
pagination limit. Results run from newest creation time to oldest, with task ID as a deterministic
tie-breaker. A cursor from another query, status, project, or page size returns `400`.

`GET /api/tasks/export` accepts `project_dir` and `id` as query parameters. `project_dir` defaults
to the server's active project; omitting `id` returns the complete project document, while an exact
`id` returns that task and its recursive subtasks. A missing task returns `404`; an unavailable Task
module also returns `404`; invalid project resolution returns `400`.

Successful responses use this versioned envelope:

```json
{
  "schema_version": 1,
  "project_id": "project-ulid",
  "task_id": "tsk-abcd",
  "tasks": [],
  "dependencies": [],
  "checks": [],
  "events": [],
  "comments": [],
  "spec_revisions": []
}
```

`task_id` is omitted for a project-wide export. The API never returns fencing tokens or
`task_control` scheduler rows.

### Hub

The registry starts with a filterable catalogue and a selected artifact dossier: inspect identity, publisher, version, origin and dependencies before choosing an installation action. Project artifacts separates Owned, Installed and Linked origins, each with its applicable maintenance actions. Switching project or agent closes scoped dialogs and discards responses from the previous workspace.

Publication is a three-step workspace: Package → Metadata & dependencies → Review & publish. Existing artifacts use a focused metadata/review dialog. Artifact descriptions support Markdown in the catalogue, inspector and both publication review flows. Review uses the shared renderer while editing and submission retain the exact authored source. Review preserves the complete submission contract; explicit publication remains the only write step. Dialogs isolate background controls, trap keyboard focus, close on Escape and restore focus.

The Hub routes present:

- ACL-filtered, cursor-paginated project and artifact discovery;
- project-installed artifacts;
- artifact details and version metadata;
- install, update, uninstall, and publish actions where supported.

Submit and Upload always publish under the active project's publisher ULID; there is no selectable
global artifact-owner namespace. The Global Registry remains the discovery catalogue. Manual AST
and Knowledge upload is an import operation: AST accepts only a `.ast` file produced by
`graphit ast export --format package`, Knowledge accepts only a `.knowledge` file produced by
`graphit knowledge export --format package`, and other file artifacts use `.zip`. The server
validates the package envelope, version, declared type, and native store before publication.

Remote operations depend on configured Hub storage, a trusted subject, and current project grants.
The UI never fetches an all-project export implicitly. It distinguishes an empty authorized result
from unavailable authentication, forbidden access, and a stale offline cache. Selecting a cached
row does not authorize details, content, installation, or a mount; those requests revalidate.

### Live Search

Live Search separates **Prepare context**, **Run & evidence**, and **Recent sessions**. Choose compatible artifacts and an Agent, then start a run in an ephemeral project. The execution view keeps streamed evidence, connection state, prompts and cancellation together. Select a recent session to resume its console; removal retains its confirmation. Reconnection and event deduplication remain transport responsibilities. Ephemeral work is separate from persistent project stores.

### System views

**Projects** is a searchable local directory with a selected project dossier. Inspect its path, registered identity and cluster labels, edit labels or unregister through explicit confirmation. Change the working project using the common header. **Daemon** starts with process/connection state and polling logs; stopping uses a focused confirmation. **Dream** separates runtime conditions, report directory and reading view, with scope changes clearing previous evidence. **Workspace** is a job chooser (Continue, Understand, Reuse & share, Operate) with project metadata and a link to project administration. Global context selection stays in the header.

- **Daemon** exposes process status and recent operational information.
- **Dream** exposes configuration and session/report state.
- **Ecosystem** lists registered projects, labels, and active project identity.

## Go server boundary

`internal/uiserver/`:

- serves the embedded SPA;
- resolves the active project and imported contexts;
- exposes JSON handlers for AST, wiki, memory, Task, Hub, live search, daemon, Dream, and ecosystem operations;
- applies one UI host and exact-origin CORS configuration;
- returns domain-friendly names rather than storage implementation names.

The built-in UI host is the IPv4 loopback address. The frontend uses same-origin `/api` URLs so it continues to work behind a correctly configured reverse proxy.

The server has no built-in user authentication. CORS limits browser origins but does not authorize
non-browser clients. A multi-user Hub deployment must put the UI behind an authenticated proxy or
another trusted identity adapter that supplies user and team identity; otherwise only a
single-user deployment subject is valid. See
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
- The production bundle builds with `npm run build`.
- Task discovery uses a bounded paginated catalogue; exact detail and explicit JSON download consume the canonical complete export contract.
- Memory routes use only the dedicated Memory API and render Markdown without a wiki-page adapter.
- The embedded server serves the SPA and same-origin API calls.
- The default network configuration remains local; documentation does not present CORS as authentication.
- Hub discovery is bounded and filtered before project or artifact metadata reaches the browser.
- A stale Hub cache never grants detail, content, install, publish, or mount access.

## Visual reference

The shared [design system](design_system.md) defines the current visual identity. Previous Observatory screenshots are not a reference for this layout.

### Current project activity

`GET /api/workspace/now?project_dir=...` returns `project_id`, `generated_at` and `sections` keyed by `sessions`, `tasks`, `memories`, and `knowledge`. Each section contains `items`, `total`, `has_more` and an optional `error`. Items contain identity/title/link, available responsible identity and update time; current work additionally includes lease, progress and next action. Fencing tokens, raw session snapshots, audit histories and complete document/memory bodies are excluded. Domain readers retain their normal project authorization. Metadata is ordered by parsed update timestamp descending with ID as the stable tie-breaker before the 20-item cap. Task/session claims must be in progress and unexpired. Knowledge is the project's own corpus; personal memory and imported corpora are not included.

### Incremental agent responses

Knowledge AI search and AST AI generation accept `Accept: text/event-stream` on their existing POST endpoints. The server reads the complete JSON request before flushing any SSE headers or events, including on HTTP/1.1 connections. Stream requests are limited to 1 MiB; unreadable bodies return HTTP 400 and oversized bodies HTTP 413 as JSON errors before the agent starts. The UI displays that error message. Ordinary JSON callers remain compatible. SSE emits `progress` with normalized public CLI events, then `final` with the existing response object or `error`, and `done`. Native CLI session IDs remain server-side for streaming clients. Request cancellation terminates the underlying CLI through the existing conversation context. Cypher generation produces only an editable draft; execution still requires Run query.

Live keeps its existing replayable SSE protocol: subscriber disconnect does not cancel the session. Its execution panel includes thinking, stdout diagnostics, stderr and tool results. `GET /api/live/sessions/{id}/knowledge/page?page=slug&context=name` opens an existing source from the retained investigation workspace. A qualified `context:slug` can select a source; an ambiguous unqualified slug returns a candidates object for explicit selection. Unknown sessions, missing pages and removed sources do not fall back to another workspace. Published sources are reopened through the authorized Hub mount. The endpoint neither installs artifacts nor creates a missing workspace.

### Live artifact catalogue and preparation

`GET /api/live/artifacts?agent=name&project_dir=...` lists the active registered project instance and its own AST/Knowledge indexes, installed contexts and agent artifacts. Responses include a structured reference (`source: project`, `project_id`, opaque `instance_id`, `project_kind`, `id`, `type`, `version`), display provenance and an optional `unavailable` reason. Filesystem source paths are server-side. The Live picker also consumes every `/api/registry` cursor, including empty filtered pages, and retains usable project sources if the Hub is unavailable.

The session creation API accepts these references in `artifacts`. An omitted `source` remains a Hub reference for existing clients. Preparation re-enumerates local sources and rejects changed identity, version, agent or instance. Local AST/Knowledge links use the source project identity and existing index; local packages are copied into the ephemeral workspace with resources, then projected through the selected agent adapter. No local source is initialized, indexed, published or edited. Published installed contexts still pass through the Hub installer and its access checks. Unsupported local package types are labelled unavailable; invalid local references fail preparation. Existing Hub partial-install diagnostics remain visible in the execution stream.

An investigation cannot select different versions of the same published artifact: preparation rejects that boundary before installation instead of silently replacing one version. Local skill copies receive a bounded, collision-resistant directory name and matching YAML `name`; descriptions and additional metadata remain intact. Adapter projections preserve executable permission bits on package scripts. These changes affect only the investigation copy and its adapter projection.

The registry response includes `available`; Live distinguishes an unconfigured Hub from an empty search result. Free-text artifact search also matches artifact type, so terms such as `knowledge` and `skill` work alongside names and tags.
