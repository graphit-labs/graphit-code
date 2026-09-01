---
title: Complete redesign of the frontend experience
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [frontend, ui, ux, react, accessibility]
---

# Complete redesign of the frontend experience

## Objective

Rebuild the Graphit Code dashboard UI/UX in full, without taking the current appearance as a reference and without removing, hiding or degrading existing functionality. The request grants visual freedom; the interpretation adopted is to turn the product into a high-density, highly legible technical workspace — an “observatory” for code, knowledge, agents and ecosystem — while preserving API contracts, routes, actions, states, feedback and flows.

The approach starts from the documented functional inventory and from the AST graph, then establishes a coherent design system before redesigning the shell and each surface. This was chosen over piecemeal changes because the request is systemic: merely swapping colors and spacing would perpetuate the current hierarchy and patterns; rewriting behavior, on the other hand, would create unnecessary risk of functional regression.

## Plan & Task Breakdown

- [x] **T1 — Functional and impact inventory** — Spec: map routes, pages, components, stores, APIs, actions and test coverage in `internal/ui/`; done when every existing surface and interaction is accounted for; the invariant is not to infer functionality from the current appearance.
- [x] **T2 — Visual foundation and responsive shell** — Spec: rebuild tokens, typography, colors, motion, focus, surfaces, `AppShell`, `Sidebar` and `ProjectSwitcher`; done when desktop/mobile navigation, theme and project context work with a consistent hierarchy; the invariant is to keep every existing destination and control.
- [x] **T3 — Exploration surfaces** — Spec: redesign the AST Explorer, canvas, node tree, query bar, results, code, schema, Live Search, Wiki and contexts; done when reading, searching, selection and exploration preserve their states and actions; the invariant is to prioritize density without sacrificing legibility.
- [x] **T4 — Operational surfaces** — Spec: redesign Hub, uploads, projects, modals, daemon, dream, ecosystem, backlog and shared states; done when catalogs, forms, confirmations, status and feedback keep their behavior; the invariant is that destructive or asynchronous actions remain unambiguous.
- [x] **T5 — Functional and visual verification** — Spec: run tests, typecheck/build and visual inspection on the representative routes and at desktop/mobile widths; done with no failures introduced and no evident overflow/contrast/focus problems; the invariant is to treat the build as technical proof and the browser as proof of experience.
- [x] **T6 — Documentation, memory and synchronization** — Spec: update this log, the UI specification and the project memory, then synchronize the indexes; done when decisions, files, tests and debt are recorded and the indexes reflect the final state.
- [x] **T7 — Favicon identity** — Spec: replace the legacy favicon with a vector symbol coherent with the Graphite Observatory and update the document metadata; done when the browser and shortcuts no longer reference the previous brand.
- [x] **T8 — Logical relationship names in the AST Explorer** — Spec: resolve the physical names of the relationship tables through the canonical mapping of the Icebug manifest at the API boundary, including schema, filters and canvas links; done when no Explorer surface exposes the physical limitation and tests cover aggregation and translation.
- [x] **T9 — Brand glyph fidelity** — Spec: replace the free interpretation of the favicon with an exact vector transcription of the sidebar's `.brand-glyph`; done when shapes, proportions, colors and positions match the CSS and the attached visual reference, including the two lower dots being visually separated.

## Implementation Details

The frontend was rebuilt on the **Graphite Observatory** concept. `index.css` now defines light/dark tokens, Manrope + IBM Plex Mono, solid technical-paper surfaces, a coordinated grid, a phosphorescent accent, focus, selection, reduced motion, scrollbar and global states. `AppShell`, `Sidebar` and `ProjectSwitcher` compose a charcoal navigation that is independent of the theme, resizable on desktop and drawer-based on mobile, preserving every destination and filter.

The operational pages received coherent spacing, editorial hierarchy and domain markers. AST and Wiki use `explorer-frame` and collapse the index by default on a mobile viewport. Live Search moves from two columns to a vertical flow below `lg`, with a height-limited picker and an always-accessible console. Empty states, toasts and the loader were rebuilt with their semantics and ARIA preserved or extended.

## Use Cases

### UC-01: Navigate the Graphit workspace
- **Actor**: developer using the dashboard.
- **Preconditions**: UI server running and SPA loaded.
- **Main Flow**:
  1. The developer identifies the current area from the global navigation.
  2. Selects an existing destination of the product.
  3. The corresponding route opens without losing the global project context.
- **Alternative Flows**:
  - On a narrow viewport, navigation uses the responsive pattern defined by the shell.
  - The developer switches the active project in the global selector.
- **Error Scenarios**:
  - Loading failures still produce visible and recoverable feedback.
- **Postconditions**: the chosen area is active and every original action remains available.
- **Affected Files**: `internal/ui/src/App.tsx`, `internal/ui/src/components/layout/AppShell.tsx`, `internal/ui/src/components/layout/Sidebar.tsx`, `internal/ui/src/components/layout/ProjectSwitcher.tsx`.

### UC-02: Explore code, knowledge and results
- **Actor**: developer investigating a project.
- **Preconditions**: a valid project selected and the corresponding sources available.
- **Main Flow**:
  1. The developer opens AST, Live Search or Wiki.
  2. Runs a search, a query or navigation through the tree.
  3. Inspects the graph, tabular result, code, schema or Markdown content.
  4. Selects entities and switches panels without losing the relevant context.
- **Alternative Flows**:
  - The system presents empty, loading and error states according to the source.
- **Error Scenarios**:
  - Invalid queries and API failures keep clear messages without corrupting the previous state.
- **Postconditions**: the requested information is displayed, or the failure is explained with a recovery path.
- **Affected Files**: `internal/ui/src/components/ast/`, `internal/ui/src/components/live/`, `internal/ui/src/components/wiki/`.

### UC-03: Operate ecosystem services and artifacts
- **Actor**: developer administering Graphit.
- **Preconditions**: backend available and adequate permissions/configuration.
- **Main Flow**:
  1. The developer opens Hub, Daemon, Dream, Ecosystem or Contexts.
  2. Reviews state and operational data.
  3. Runs an existing action, fills in a form or confirms an operation.
  4. Receives progress, success or error feedback.
- **Alternative Flows**:
  - Lists with no data show guidance instead of empty areas.
- **Error Scenarios**:
  - The API may fail; the UI preserves useful data and presents an actionable error.
- **Postconditions**: the result of the operation is reflected on the corresponding surface and by toast when applicable.
- **Affected Files**: `internal/ui/src/components/hub/`, `internal/ui/src/components/daemon/`, `internal/ui/src/components/dream/`, `internal/ui/src/components/system/`.

## Test Cases & Acceptance Criteria

### Feature: Navigation and responsive shell
Ref: UC-01

#### Scenario: Navigation preserves existing destinations
```gherkin
Given the dashboard loaded with a project selected
When the developer walks through every destination available in the navigation
Then every existing route remains accessible
  And the current destination has unambiguous visual and semantic indication
```

#### Scenario: Shell adapts to a mobile viewport
```gherkin
Given a viewport of 390 by 844 pixels
When the dashboard is loaded and the navigation is opened
Then every destination and the project selector remain accessible
  And the content does not create unintended horizontal overflow
```

### Feature: Information exploration
Ref: UC-02

#### Scenario: AST query preserves results and panels
```gherkin
Given a project with an AST index and the exploration page open
When the developer runs a valid query and selects a result
Then the result remains available in the supported formats
  And the detail and code panels keep their existing interactions
```

#### Scenario: Query failure is recoverable
```gherkin
Given the exploration page open
When the developer runs a query that receives an error from the backend
Then the UI presents the error message
  And a new query can be run without reloading the page
```

### Feature: Ecosystem operations
Ref: UC-03

#### Scenario: Asynchronous operation communicates state
```gherkin
Given an operational surface with an existing action available
When the developer starts the action
Then the UI communicates the in-progress state
  And presents success or error on completion
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/redesign-frontend-experience.md` | Created | Open the work with objective, invariants, plan, use cases and verifiable criteria. |
| `docs/specs/ui_dashboard.md` | Modified | Record the current visual language and responsive contract. |
| `docs/decisions/graphite-observatory-ui.md` | Created | Formalize the visual direction and its consequences. |
| `internal/ui/src/index.css` | Rebuilt | Define the Graphite Observatory design system and the global accessibility/motion behaviors. |
| `internal/ui/tailwind.config.js` | Modified | Align typographic families and motion to the new system. |
| `internal/ui/src/components/layout/AppShell.tsx` | Modified | Restructure the workspace canvas, width, grid and resizer. |
| `internal/ui/src/components/layout/Sidebar.tsx` | Modified | Create the new identity, navigation hierarchy, active states and accessible drawer. |
| `internal/ui/src/components/layout/ProjectSwitcher.tsx` | Modified | Adapt the project and IDE selectors to the charcoal navigation. |
| `internal/ui/src/components/shared/EmptyState.tsx` | Modified | Standardize empty states as a technical signal and improve the visual composition. |
| `internal/ui/src/components/shared/Toast.tsx` | Modified | Recreate global feedback and add an ARIA live region. |
| `internal/ui/src/components/shared/GlobalLoader.tsx` | Modified | Make global processing legible and non-intrusive. |
| `internal/ui/src/components/ast/ExplorerPage.tsx` | Modified | Apply the visual frame and mobile collapse of the rail/query bar. |
| `internal/ui/src/components/wiki/WikiExplorerPage.tsx` | Modified | Apply the visual frame and mobile collapse of the index. |
| `internal/ui/src/components/live/LiveSearchPage.tsx` | Modified | Reorganize the picker and console responsively and guarantee mobile access. |
| `internal/ui/src/components/ast/ContextsPage.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/src/components/wiki/WikiContextsPage.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/src/components/hub/RegistryPage.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/src/components/hub/ProjectArtifactsPage.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/src/components/hub/UploadPage.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/src/components/daemon/DaemonDashboard.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/src/components/dream/DreamDashboard.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/src/components/system/EcosystemDashboard.tsx` | Modified | Apply page hierarchy and spacing. |
| `internal/ui/public/favicon.svg` | Created | Replace the legacy favicon with the Graphite Observatory glyph. |
| `internal/ui/index.html` | Modified | Reference the new favicon with cache busting and align the page metadata. |
| `internal/ast/ladybug_relationship_names.go` | Created | Resolve and aggregate logical types from the manifest specific to each backend/project. |
| `internal/ast/ladybug_relationship_names_test.go` | Created | Prevent physical names from leaking and prove the mapping is isolated between projects. |
| `internal/ast/server.go` | Modified | Publish only logical names in `/api/schema` and `/api/graph`. |
| `docs/specs/ui_dashboard.md` | Modified | Record the public boundary between logical names and Icebug's physical tables. |

## Trade-offs & Decisions

- The new identity will start from Graphit's purpose — connected exploration of code and knowledge — and not from generic admin-panel conventions.
- API, store and route contracts will be preserved by default; changes will be concentrated in composition, styles, accessibility and microinteractions.
- The visual direction will be defined after the functional inventory, preventing a prematurely chosen aesthetic from hiding rare controls or states.
- Navigation uses fixed charcoal in both themes so it works as a stable spatial reference; only the workspace switches between light paper and dark graphite.
- Live Search stacks its panels on mobile instead of hiding the console or creating horizontal scroll; this accepts a taller document in exchange for full access to the actions.

## Technical Debt

- [ ] Add dedicated interaction tests for `AppShell`, `Sidebar`, `ProjectSwitcher`, responsive Live Search and the mobile states of the explorers. The graph does not show tests reaching those components and the current report covers only 5.61% of total statements, even though the 42 existing tests pass.
- [ ] Run the visual smoke test against `graphit ui` with a real backend and representative data as well. Vite in isolation proved composition and responsiveness, but naturally answered empty on the `/api` routes; the JSON errors observed in the console came from that absent backend, not from the presentation changes.

## System Knowledge

- The UI is a React/Vite/Tailwind SPA embedded by the Go server.
- The dashboard integrates 3D AST exploration, wikis/memory, Hub, daemon, dream, ecosystem and live search; navigation is therefore an orientation tool across domains, not just a list of pages.
- JSX rendered as a component call does not appear as a `CALLS` edge in the current graph; that is why layout and page components show no reachable callers/tests even though they are active routes. The inventory was confirmed through the imports table, `App.tsx`, the build and a smoke test in the browser.

## Progress Log

### 2026-08-31
- Memory searched; there was no UI-specific memory relevant to guiding the redesign.
- Wiki consulted: `UI Dashboard Specification` (confidence 0.90) and `System Architecture Overview` (confidence 1.00).
- AST schema consulted before any structural query; the initial inventory found 54 TSX/TypeScript/CSS files in the frontend.
- Inventory of 202 frontend functions and 207 imports completed. The index does not store the source of those files, so direct reading was used only after the tool's explicit fallback.
- Design system, shell, navigation, surfaces, feedback and explorers rebuilt without changing API/store/route contracts.
- `npm run build` passed after the initial foundation.
- Browser inspection passed in light/dark theme, on desktop and at the 390×844 viewport; every known route rendered without horizontal overflow.
- Mobile inspection found the Live Search console inaccessible outside the viewport; the composition was fixed to stack picker and console. AST and Wiki now start with the rail collapsed on mobile.
- Final suite completed: `npm test` with 5 files/42 tests passing; `npm run lint` with no warnings; `npm run build` completed; `git diff --check` clean.
- ADR `docs/decisions/graphite-observatory-ui.md` created and `docs/specs/ui_dashboard.md` updated.
- Work completed without changing routes, API contracts, store or domain actions.

### 2026-08-31 — Post-delivery corrections
- The user pointed out that `internal/ui/index.html` still referenced the legacy `logo.svg` as the favicon and that `/api/schema`/`/api/graph` exposed the physical names of Icebug's canonical relationships.
- Memory, the previous task log, the AST schema and the ecosystem were consulted again before the correction. The canonical manifest is the source of truth: `CanonicalRelGroup.Type` is the public logical name; `Members[].Table` and `ReverseMembers[].Table` are physical engine details.
- Direction set: fix it in the backend, aggregating statistics and normalizing links through the same `CanonicalManifest` instance already loaded by the translator/planner; the frontend will continue consuming only public names and will not receive a duplicated map.
- T7 implemented: created `public/favicon.svg` with the Observatory's green/black glyph; `index.html` now uses the new asset with a cache version, and a coherent title, description and `theme-color`.
- T8 implemented in the backend: the canonical manifest resolves each physical table to `CanonicalRelGroup.Type`; `/api/schema` aggregates forward members under the logical type and `/api/graph` normalizes the links before serialization. Reverse mirrors do not inflate the public count.
- Refinement requested: the map is not global. `dbForContext` resolves `project_dir`/`context`, the cache is keyed by `projectDir + storeDir`, each backend points `IcebugDir` at `<store>/graph.icebug` and `loadCanonicalManifestLocked` reads the `icebug.json` in that folder. The regression test uses two real manifests with the same physical table and different logical names to prove the per-project isolation.
- Validation completed: focused tests cover per-project reading, link translation, aggregation without mirrors and the logical schema response; the full `go test ./internal/ast -count=1` suite passed; frontend with 5 files/42 tests, lint and production build passing; `git diff --check` clean; favicon rendered at 256×256 for visual inspection.
- New visual correction requested: the favicon must be exactly the `.brand-glyph`, not merely use its colors. The attached reference and the CSS were inspected; the new version will be a direct transcription of the rounded square, ring and three dots, with no additional graphic elements.
- T9 implemented: the SVG now uses the same proportional 40×40 canvas, an outer radius of `0.6rem`, an inset of `0.45rem`, a 1 px ring and circles derived literally from the two `box-shadow` values and the `::after`. The URL advanced to `v=3` to invalidate the interpretive favicon already cached by the browser.
- The SVG was rasterized at 256×256 and compared with the attached reference: the ring, the upper-right dot and the overlapping lower-left pair preserve the composition of the `.brand-glyph`, without the nodes and diagonals of the rejected version.
- The comparison was corrected by the user: although the composition was right, the lower circles came out too large and too close together. Measuring the 50 px reference, normalized to 40×40, places the centers at approximately `(7.0,31.5)` and `(10.5,28.7)`, with a visual radius close to `2.4`; T9 was reopened to preserve the separation between the shapes.
- Adjustment applied: the three dots now use radius `2.4`; the lower centers moved from `(7.2,32)`/`(9.92,30.08)` to `(7,31.5)`/`(10.5,28.7)`, recovering two distinct silhouettes. The upper dot follows the measured proportion at `(33.2,10.8)` and the cache advanced to `v=4`.
- A new rasterization at 256×256 confirms the two lower dots are separated, with short tangency and a proportion equivalent to the reference; T9 completed again with measured geometry, not inferred.
- The user explicitly authorized committing directly to `main`. The worktree state was reviewed: every modified file belongs to the Graphite Observatory redesign, the AST Explorer corrections and the favicon; no unrelated file will be included.
