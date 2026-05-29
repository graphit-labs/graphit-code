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
- **Wiki Handlers (`wiki_handler.go`)**:
  Exposes JSON endpoints:
  - `GET /api/wiki/pages`: Returns the directory list tree.
  - `GET /api/wiki/page/:slug`: Returns raw page contents and metadata.
  - `POST /api/wiki/search`: Connects queries to the BM25 and semantic RRF engine.
  - `POST /api/wiki/chat`: Orchestrates multi-turn wiki agent completions.

---

## 📊 Visual Interactive Modules

### 1. 3D Force-Directed AST Canvas
- **Technology**: `d3-force-3d` engine.
- **Execution**: Renders the AST database structures in a three-dimensional spatial canvas.
- **Data Load**: The component `GraphCanvas.tsx` queries nodes (files, classes, methods) and edges, binding them to a physics model with force constraints (charge, link distance, center gravity).
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
- **Upload Modal**: `SubmitModal.tsx` provides interface forms to package rulesets and upload them to the remote Git registry.
