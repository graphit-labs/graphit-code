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

Graphit Observatory uses:

- a theme-independent graphite navigation rail;
- paper-like neutral workspace surfaces in light mode;
- deep graphite workspace surfaces in dark mode;
- phosphor green for primary signals and mint for secondary technical states;
- Manrope for hierarchy and IBM Plex Mono for technical metadata;
- a low-contrast coordinate grid and restrained instrument framing;
- the product `brand-glyph` as the favicon and primary identity mark.

Semantic tokens define meaning across both themes. New features extend existing tokens instead of introducing route-specific palettes.

## Responsive contract

- Below `md`, global navigation becomes a modal drawer with every route, workspace/Agent control, filter, and theme action preserved.
- Below `lg`, Live Search stacks its artifact picker and session console.
- AST and wiki rails may collapse so the canvas or document remains usable on narrow screens.
- Full-screen explorers must not introduce horizontal page overflow.
- Keyboard focus remains visible.
- Motion honors `prefers-reduced-motion`.
- Loading and toast states communicate status without removing the underlying route context.

## Workspace identity

The app store loads machine-local ecosystem projects from `GET /api/global-projects` and persists
the selected project directory under the `graphit-app-state` browser key. That local list is not
the remote Hub project directory. Hub discovery uses its own ACL-filtered cursor API.

Every project-scoped request must use the active project directory or context. Because the selection survives browser sessions, explorer screens must display the active project clearly enough for a user to verify it before interpreting data.

Switching projects updates:

- mutable project name and immutable ULID identity;
- Knowledge, AST, Memory, and Task navigation entries;
- Hub target context;
- explorer API parameters;
- ecosystem and system views that depend on project scope.

## Main experiences

### AST Explorer

The AST Explorer contains:

- a schema rail grouped by language and entity type;
- friendly relationship filters;
- Cypher and AI-assisted query modes;
- example queries;
- a two- or three-dimensional graph canvas;
- zoom, fit, reset, physics, and layer controls;
- node details and source navigation.

User-facing relationship names are resolved dynamically from the active project or context's `graph.icebug/icebug.json` manifest. `CanonicalRelGroup.Type` is the public name used by the translator, schema rail, filters, and canvas. Physical edge-table names are internal Icebug storage details and must not cross the explorer API boundary.

Context totals and the schema rail's label, relationship-type, and language counts come from the
same canonical manifest. Listing a large project must not mount LadybugDB or run global `count`
queries; reverse relationship mirrors are storage-only and are excluded from the displayed totals.

The mapping is scoped to `project_dir` or imported context; the frontend must not maintain a global hard-coded relationship map.

### Knowledge Explorer

Knowledge uses the Wiki Explorer workspace for maintained project and imported documentation. It
provides page navigation, keyword and AI-assisted search, source-backed Markdown rendering,
provenance, cross-references, metadata, and refresh actions through `/api/wiki`.

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

The Memory API consists of `GET /api/memories`, `POST /api/memories`,
`GET /api/memories/{id}`, `PATCH /api/memories/{id}`, and
`DELETE /api/memories/{id}?confirm=true`. Each request accepts `project_dir` and `scope` as
appropriate. `GET /api/memories/scopes` reports the directly addressable scopes.

### Task Explorer

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

The Hub routes present:

- ACL-filtered, cursor-paginated project and artifact discovery;
- project-installed artifacts;
- artifact details and version metadata;
- install, update, uninstall, and publish actions where supported.

Remote operations depend on configured Hub storage, a trusted subject, and current project grants.
The UI never fetches an all-project export implicitly. It distinguishes an empty authorized result
from unavailable authentication, forbidden access, and a stale offline cache. Selecting a cached
row does not authorize details, content, installation, or a mount; those requests revalidate.

### Live Search

Live Search lets the user choose compatible artifacts, select a target Agent, enter a prompt, and observe a streamed agent run inside an ephemeral project. Recent sessions and execution status remain visible without mixing them into persistent project stores.

### System views

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

## Reference captures

![AST Explorer](../site/assets/observatory-ast-explorer.jpg)

![Knowledge Explorer](../site/assets/observatory-knowledge-explorer.jpg)

![Memory Explorer](../site/assets/observatory-memory-explorer.jpg)
