---
title: "UI Dashboard Specification"
description: "Technical specification of the React web dashboard, 3D D3 canvas visualization, state stores, and Go HTTP handlers."
content-type: reference
audience: developers
keywords:
  - UI
  - React
  - Vite
  - 3D graph
  - D3
  - uiserver
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/ast_module.md"
  - "docs/specs/wiki_module.md"
---

# UI Dashboard Specification

The dashboard provides a visual interface for managing Graphit Code.
It features interactive 3D code graphs, documentation trees, and artifact registries.

---

## 🎨 Frontend Stack: Vite + React + Tailwind

The visual application is located inside `internal/ui/` and is structured as a Single Page Application:

- **Build Tool**: Vite (`vite.config.ts`), configured to output compiled assets to `internal/ui/dist/` for Go embedding.
- **Styling**: Tailwind CSS (`tailwind.config.js`) + PostCSS.
- **State Management**: Zustand-backed store (`appStore.ts`) tracking project workspaces, select nodes, search histories, active page contexts, and UI toasts.
- **API Client**: Axios wrapper communicating with backend handlers (`src/api/`).

### Visual language and responsive behavior

The dashboard uses the **Graphite Observatory** visual system. Its dark graphite navigation
acts as a stable instrument rail, while the workspace uses paper-like neutral surfaces, a
low-contrast coordinate grid, phosphor-green primary actions, and IBM Plex Mono for technical
metadata. The same semantic tokens drive light and dark themes; component behavior does not
depend on a theme-specific branch.

The responsive contract is:

- Below the `md` breakpoint, the global navigation becomes a modal drawer and retains every
  route, registry type filter, project/IDE switcher, and theme control.
- The Live Search artifact picker and session console stack vertically below the `lg`
  breakpoint. The picker remains independently scrollable and the console is never hidden
  beyond the viewport.
- AST and Wiki explorers start with their index rail collapsed on mobile. Users can expand it
  with the same control used on desktop; query and canvas/content surfaces keep the remaining
  width.
- All interactive elements use a visible keyboard focus ring, motion honors
  `prefers-reduced-motion`, and global loading/toast states announce themselves without
  changing the underlying feature flow.

---

## 🌐 Go HTTP Server: Unified uiserver

The backend server is implemented inside `internal/uiserver/`:

- **Unified Server (`unified_server.go`)**:
  Serves the React static files compiled by Vite using Go's `embed` package:
  ```go
  //go:embed all:dist
  var embeddedUI embed.FS
  ```
  It dynamically resolves server paths and binds API routes.

### Network binding and origin policy

`graphit ui` selects a free port and binds it on the resolved `ui.host`. The built-in
default is `0.0.0.0`; `GRAPHIT_UI_HOST`, project config, or global config can override it
through the standard precedence chain. The embedded frontend uses same-origin `/api`
URLs, so it works through a LAN hostname or reverse proxy without calling the browser's
own localhost.

Every endpoint is wrapped by one CORS policy. With no `ui.allowed_origins` override,
empty/same-origin requests and localhost loopback origins are accepted. A configured
comma-separated list replaces that default with exact origins; `*` is an explicit
allow-all value. The UI server has no authentication, and CORS is not access control for
non-browser clients. Restrict `ui.host` or place reachable deployments behind a firewall,
VPN, or authenticated TLS proxy. See
[S3 Credentials and UI Network Configuration](../guides/s3-and-ui-network.md).
- **Wiki Handlers (`wiki_handler.go`)**:
  Exposes JSON endpoints:
  - `GET /api/wiki/modules?project_dir=`: Returns the wikis browsable for that project — its documentation wiki, its two memory scopes, and every context it has imported.
  - `GET /api/wiki/pages?dir=`: Returns the page list of one wiki directory.
  - `GET /api/wiki/page?dir=&path=`: Returns raw page contents and metadata.
  - `GET /api/wiki/search?dir=&q=`: BM25 search inside one wiki.
  - `POST /api/wiki/ai-search`: Single-wiki AI search. Multi-turn agentic search over several sources is the live search, under `/api/live`.

  **Module discovery resolves, it does not scan.** Every wiki lives once in the global
  brand directory keyed by identity, so `discoverModules` asks `internal/store` where each
  one is: `store.KnowledgeProjectDir` for the project's documentation,
  `store.MemoryWikiDir` for the `project` and `user` memory scopes,
  `store.KnowledgeContextDirIn` for each context named in the project's lockfile, and the
  memory worktree set for imported memory contexts. Nothing under the project directory is
  probed — a wiki left at the pre-centralization path (`<project>/.graphit/knowledge/project`)
  is deliberately not reported.

  The module `id` is the UI's contract, not an implementation detail: the sidebar builds its
  Memory section from ids prefixed `memory-`, and the Knowledge Contexts page filters on
  `knowledge` and `knowledge/`. The `context` field is what the explorer route is built from
  (`/memory/explorer/<context>`), and `project`/`user` are what the cards style as
  project-scoped.

---

## 📊 Visual Interactive Modules

### 1. 3D Force-Directed AST Canvas
- **Technology**: `d3-force-3d` engine.
- **Execution**: Renders the AST database structures in a three-dimensional spatial canvas.
- **Data Load**: The component `GraphCanvas.tsx` queries nodes (files, classes, methods) and edges, binding them to a physics model with force constraints (charge, link distance, center gravity).
- **Relationship names**: Every user-facing edge type comes from the active project/context's
  `<store>/graph.icebug/icebug.json`. `CanonicalRelGroup.Type` is the public name used by the
  query translator, schema rail, filters and canvas; physical member-table names remain an
  Icebug storage detail and must never cross the Explorer API boundary. Resolution is scoped
  to `project_dir`/context and cannot use a global or frontend-owned map.
- **Interactions**:
  - Hovering highlights connected neighbors (e.g. parent directories and child functions).
  - Clicking extracts node parameters and lists them inside `NodeTree.tsx`.

### 2. Wiki Explorer Layout
- **Component**: `WikiExplorerPage.tsx`.
- **Render**: Splits the interface into a file navigation tree (`WikiTree.tsx`), a content markdown viewer, and a search panel.
- **Semantic Tags**: Automatically converts Obsidian wikilinks into SPA routes.

### 3. Registry Hub Page
- **Component**: `RegistryPage.tsx`.
- **Function**: Communicates with the hub service to display catalogs of rules and skills.
- **Upload Modal**: `SubmitModal.tsx` packages rulesets and publishes them to the configured S3-backed Hub registry.
