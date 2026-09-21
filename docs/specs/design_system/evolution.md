# Evolution, ownership and validation

[Design system](../design_system.md) · [Foundations](foundations.md) · [Patterns](patterns.md) · [Workspaces](workspaces.md)

Audience: maintainers and agents evolving the shared identity. Baseline: 2026-09-20. This document defines a change process; it does not introduce a package release system or a new approval gate.

## Source ownership

The design reference is canonical in Graphit Code's `docs/specs/design_system.md` and `design_system/`. Implementations remain local to each product. Review the documentation on the same branch/revision as the code being changed.

| Concern | Owning source | Consumers to inspect |
| --- | --- | --- |
| Code semantic values, shared chrome and responsive composition | `internal/ui/src/index.css` | Workspace, every shell-wrapped route, explorer overrides |
| Code token utility mapping | `internal/ui/tailwind.config.js` | Components using semantic colors/radii/font utilities |
| Routes and shell | `internal/ui/src/App.tsx`; `components/layout/AppShell.tsx`, `Sidebar.tsx`, `WorkspaceSelectors.tsx`, `WorkspaceRefresh.tsx` | Existing deep links; exactly one Project/Agent selector group in the common header on every route, including mobile; no sidebar/page duplicates |
| Workspace content | `internal/ui/src/components/system/WorkspacePage.tsx` | Registered-project store and context/work/evidence destinations |
| Code theme synchronization | `internal/ui/src/hooks/useTheme.ts` | Theme provider and simultaneous desktop/mobile controls |
| Code modal lifecycle | `internal/ui/src/components/shared/ModalPortal.tsx` | Memory form, Hub dialogs, daemon confirmation, knowledge image preview; focus/scroll ownership |
| Code work surfaces | `internal/ui/src/components/shared/EngineeringUI.tsx` and `engineering.css` | All internal page compositions, tables, dossiers, responsive layouts |
| Code investigation and source | `internal/ui/src/components/ast/ExplorerPage.tsx`, `QueryBar.tsx`, `CodePanel.tsx` | Scoped search, identity resolution, source, relation tables and optional graph |
| Code feedback | `internal/ui/src/components/shared/Toast.tsx` | Feature outcomes and error announcements |
| Site composition/interaction | `docs/site/index.html`, `styles.css`, `site.js` | Anchors, audience/install tabs, copy, menu, themes |
| Broker console | Graphit Broker `internal/broker/adminui/index.html` | Permission-filtered navigation, inventories/editors, IDs, event handlers, revision/CSRF contracts |
| Broker authentication | Graphit Broker `internal/broker/oauthui/index.html` | Go template branches, form actions/names, auth policy and CSP |
| Shared mark | Code public SVGs, site logo, Broker favicon and embedded SVG/CSS glyphs | Browser icon, product navigation, authentication and public site |

Paths beginning `components/` in the Code rows are relative to `internal/ui/src/`. Broker lives in its own repository; resolve the checkout rather than putting machine-specific paths into maintained docs.

## Theme contracts

| Surface | Current selector | Persistence / initialization |
| --- | --- | --- |
| Code | `.dark` / `.light` on document root | `graphit-theme`; storage, existing root class, then system preference; custom `graphit-theme-change` plus storage listener synchronize controls |
| Broker admin | `body.dark` | `graphit-broker-theme`; defaults light |
| Site | `html[data-theme="dark"]` | `graphit-site-theme`; defaults light unless a saved choice is applied |
| Broker authentication | Light CSS only | No implemented theme toggle |

The products do not currently share a single theme preference. Do not document cross-origin synchronization or automatic system-preference following where it does not exist. Code/site tolerate optional storage failures in their handlers; Broker's localStorage calls are currently unguarded. A future preference change needs failure-path coverage.

## How to evolve the system

1. **Define the job.** Fill the [screen brief](../design_system.md#screen-brief-required-before-implementation). Identify the real data owner, scope, permission and completion signal.
2. **Find the existing pattern.** Reuse its semantic role and behavior. If no pattern fits, record why the new one is needed and which surfaces consume it.
3. **Classify the change.** Local composition, shared token/pattern, brand asset, or behavioral contract. Name every affected implementation and documentation page.
4. **Specify all relevant states.** Include keyboard, focus, narrow-screen, themes, pending, empty, error and long-content behavior before declaring the screen ready.
5. **Implement with provenance.** Use current semantic variables; preserve route/API/auth contracts. Label examples and fixtures. Keep unrelated user changes intact.
6. **Validate proportionately.** Run relevant checks below and inspect real browser output. Record actual results and remaining limitations in the delivery task, not as invented product guarantees.
7. **Update the reference.** Change exact token tables when values change, add/adjust the pattern, update source mappings and adoption gaps, and link any new page from the design-system entry point.

A shared token or mark change needs a consumer review across Code, Broker admin, Broker auth and site. If rollout spans deliveries, document old/new values, affected surfaces and completion criteria; do not silently claim that an incomplete rollout is unified.

### Changes and compatibility

| Change | Required review |
| --- | --- |
| One page's spacing or copy | Local hierarchy, responsive behavior, content/state meaning |
| Semantic color/typography/radius | Both themes, paired foregrounds, contrast, font fallback, consumers and docs |
| Navigation or information architecture | Discoverability, deep links, project/identity scope, mobile/keyboard paths |
| Dialog/form behavior | Focus, drafts, errors, confirmations, server contracts and relevant tests |
| Brand asset | Every canonical asset copy, small sizes, backgrounds and product naming |
| Authentication presentation | Every relevant template branch, CSP and form protocol preservation |

Keep compatible token names where possible. For a rename, identify consumers, migrate them and remove the old name only after checking for remaining uses. There is no automated deprecation/alias mechanism or generated token package today. Do not create fictitious version numbers or a token publishing workflow in examples.

### Contributor/agent handoff template

```text
Design intent and affected journey:
Shared pattern / tokens used:
Source files and other surfaces affected:
Current behavior versus proposed change:
Loading / empty / error / permission / stale handling:
Responsive, theme, keyboard and focus evidence:
Build/tests actually run and results:
Documentation and links updated:
Known adoption gaps or follow-up scope:
```

## Verification matrix

These checks are instructions for future work. They are not a claim that a fresh test run occurred merely because this document was edited.

| Area | Method | Expected evidence |
| --- | --- | --- |
| Exact values | Compare the token tables with each owning CSS selector | Names and values match; inherited dark values are explicit |
| Code integration | From `internal/ui/`: `npm run build` and `npm exec vitest run` when UI changes warrant them | Build succeeds; relevant component/store/theme behavior passes |
| Broker integration | From Broker root: `go test ./internal/broker` when templates/console change | Tests pass; template rendering, handlers and contracts retained |
| Site | `node --check docs/site/site.js`; check local anchors/assets after content changes | Valid script and resolvable local destinations |
| Browser layout | Desktop plus 320/390px and boundaries around 767/768, 800, 1000/1050px work surfaces, 1100/1150 as relevant | No page-wide clipping; key actions available; internal scroll intentional |
| Themes/fonts | Light/dark where implemented; simulate font unavailability | Correct paired colors, readable hierarchy, usable fallback layout |
| Global context | Navigate all Code route families; change Project and Agent, test empty/loading and long names at 320/390px | One pair of themed menus in the common header; each changes independently; selection persists across routes; no picker in drawer/page. One combined Refresh awaits global catalogue and all page data, preserves drafts and rejects stale scope callbacks |
| Keyboard/modal | Tab/Shift+Tab, Enter/Space, Escape, tab arrows/Home/End | Logical focus, visible indicator, correct closure/return and no hidden focused controls |
| State/data | Empty, filtered, loading, error, conflict, permission, long ID/title | Accurate explanation, preserved scope/draft, useful recovery |
| Auth | Render affected server branches using representative safe fixtures | Required fields, actions and recovery remain available |
| Accessibility | Contrast measurements, zoom/reduced motion, accessible names and assistive-technology checks as relevant | Report measured results; do not claim certification from a screenshot |
| Documentation | Follow entry links and compare claims against implementation | No orphan reference, stale screenshot or unsupported behavior claim |

Use controlled fixtures for presentation tests when a live service is unavailable and state that limitation. Fixture rendering is not proof of end-to-end authentication or mutation correctness. Do not commit secrets or make preview-only API data part of the production workspace.

## Adoption gaps

These are current constraints to account for when touching related code, not features delivered by this documentation change.

- There is no shared runtime component library, Storybook catalogue, machine-readable cross-repository token package or token synchronization pipeline. Equivalent CSS values are maintained in separate files.
- Code HSL and Broker/site hex values are related but not byte-equivalent. Surface token names also differ; the foundations mapping is semantic.
- Code base typography uses Public Sans, but Tailwind `font-sans` still names Manrope. Broker intentionally relies on local fallbacks; auth has no dark implementation.
- Explorer compatibility CSS still overrides legacy `glass-*`, radius, casing and font utilities. New components should use semantic styles directly; avoid spreading those compatibility names.
- Existing compact text/control sizes require case-by-case accessibility review. Current focus/reduced-motion provisions do not constitute a full contrast, screen-reader or keyboard audit of all screens.
- `ModalPortal` has no simultaneous-modal stack manager; graph/canvas nonvisual parity and status announcement coverage must be evaluated for the affected journey.
- Broker theme persistence assumes accessible localStorage. Theme storage keys and defaults differ between surfaces.
- Older screenshots elsewhere in the documentation may show the previous layout. They must not be used as the current visual specification. Replace or label them when updating the affected guide; the design-system entry is authoritative.

## Baseline and future decisions

The 2026-09-20 baseline, including the complete internal-page reconstruction, establishes the context → execution → evidence journey, cobalt/navy identity, persistent Code workspace, governed Broker inventory/editor composition and explanatory public site. It deliberately preserves backend domain contracts and uses semantic HTML/SVG diagrams instead of decorative generated imagery.

Record subsequent design decisions in the relevant delivery/decision record with rationale, affected consumers and validation. Update this reference in the same change. A proposed redesign remains labeled proposed until implemented; a historical screenshot or task discussion must not silently override the current reference.


For changes involving side-by-side regions, classify each region by the [column roles](patterns.md#allocate-space-by-the-informations-role) and update the [workspace review matrix](workspaces.md#column-review-across-code-workspaces). Inspect populated panels as well as empty states. Check available container width, long identifiers, local overflow and preserved reading measure; a wide viewport alone does not prove that a right panel has enough space.
