---
title: Live Search UI Overhaul & Collapsible Sidebar
status: done
created: 2026-08-15
updated: 2026-08-15
tags: [ui, live-search, theme, light-mode, dark-mode, collapsible-sidebar]
---

# Live Search UI Overhaul & Collapsible Sidebar

## Objective
Redesign the Live Search interface in `internal/ui/src/components/live/LiveSearchPage.tsx` to align with the application's glassmorphism design system, typography, paneling, support both Light and Dark modes seamlessly, and make the artifact/session sidebar collapsible to maximize chat console area.

## Implementation Details
- **Header Banner**: Standard top banner header with `Radio` / `Sparkles` icon pill (`w-12 h-12 bg-primary/10 border-primary/20`), descriptive subtitle, and active IDE target status pill.
- **Collapsible Sidebar**:
  - Added `sidebarCollapsed` state with smooth transitions (`transition-all duration-300`).
  - Added toggle buttons (`PanelLeftClose` / `PanelLeftOpen`) in both the sidebar panel and the main console header.
  - When collapsed, sidebar shrinks to a 64px vertical strip showing compact badge indicators for selected artifacts count (`chosen.length`) and active sessions count (`sessions.length`).
  - Main chat console flexes dynamically to consume 100% of remaining viewport width when collapsed.
- **Artifact Selection Picker**:
  - Filter input & dropdown styled with backdrop blur and focus rings.
  - Color-coded badges for artifact types (`knowledge`, `skill`, `agent`, `rule`, `ast`, `mcp`, `command`).
  - Checkmark indicators and hover effects for chosen items.
- **Session History List**:
  - Rendered sessions in thread cards with state badges (`ready`, `running`, `preparing`, `failed`) and hover delete actions.
- **Stream & Transcript Console**:
  - User requests rendered as distinct prompt cards (`bg-primary/5 border border-primary/15 rounded-2xl p-4 shadow-sm`).
  - Execution activity logs rendered as a timeline feed (`border-l-2 border-primary/30 pl-4 py-1 space-y-2`) with monospace tool badges and detail chips.
  - Agent answers rendered inside clean reading cards (`bg-card/70 border border-border/40 rounded-2xl p-5 shadow-sm font-sans`).
- **Footer Input Bar**:
  - Floating glass panel input box with `btn-premium` send/start button.

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/ui/src/components/live/LiveSearchPage.tsx` | Modified | UI redesign and collapsible left sidebar implementation |

## Verification
- Ran Vitest suite: 42/42 tests passing.
- Ran Vite production build: 4,172 modules transformed, single-file bundle built successfully with zero errors.
