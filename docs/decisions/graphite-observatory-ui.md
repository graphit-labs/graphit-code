# Graphite Observatory visual language

## Status

Accepted — 2026-08-31

## Context

The dashboard spans code-graph exploration, documentation and memory navigation, live agent
sessions, artifact lifecycle, daemon monitoring, autonomous Dream reports, and ecosystem
management. A generic translucent admin-dashboard treatment did not express those
relationships or provide a stable sense of place between dense technical surfaces. The
redesign also had to preserve every route, action, API/store contract, theme, and resizable
explorer behavior.

## Decision

Adopt **Graphite Observatory** as the shared visual language:

- a theme-independent graphite navigation rail provides a stable spatial anchor;
- paper-like light surfaces and deep graphite dark surfaces share semantic HSL tokens;
- phosphor green is reserved for primary intent, selection, focus, and live signal;
- Manrope carries editorial hierarchy and IBM Plex Mono carries paths, state, counts, and
  other machine-oriented metadata;
- a quiet coordinate grid and orbital geometry reference graph traversal without becoming
  decorative noise;
- cards use solid layered surfaces and restrained shadows rather than generalized glass blur;
- the global drawer keeps the complete navigation on mobile, Live Search stacks its picker
  and console, and full-screen explorers begin with their index rail collapsed.

The implementation is centralized in `internal/ui/src/index.css` and
`internal/ui/tailwind.config.js`; components consume the same semantic tokens so feature
behavior remains independent from presentation.

## Consequences

- Light and dark workspaces feel related while navigation remains visually stable.
- Dense operational screens gain stronger hierarchy and technical metadata becomes easier to
  scan.
- Responsive layouts favor complete access over preserving desktop geometry; Live Search is
  taller on mobile because its two primary surfaces are stacked.
- Future UI work should extend semantic tokens and shared patterns before adding isolated
  colors, typefaces, glass effects, or page-specific visual systems.
- Visual components still lack dedicated interaction tests; current protection is the
  TypeScript build, 42 existing unit tests, lint, and route-level browser smoke checks.
