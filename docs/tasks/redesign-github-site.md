---
title: Redesign the public website and documentation
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [website, documentation, ui, screenshots]
---

# Redesign the public website and documentation

## Objective

Rebuild the Graphit Code website published through GitHub Pages and refresh the repository's active public documentation. The website, root README, documentation hub, and maintained guides must use one coherent current narrative aligned with the Graphite Observatory visual system.

The implementation may reorganize information, copy, page structure, and media while preserving the direct `docs/site/` publication pipeline, discoverable product capabilities, accurate technical boundaries, and the literal `v1.4.0` release token replaced by the Pages workflow.

Stale and user-provided screenshots must be removed everywhere. New captures must come from the current Graphit Code project itself; no Linux-derived screenshot may remain. Historical task logs, changelogs, and accepted decisions may be editorially rewritten when stale branding, broken references, illustrative links, or superseded prose contradict the current product. Useful technical facts remain grounded in the implementation; coherence takes priority over preserving old wording verbatim.

## Plan & Task Breakdown

- [x] **T1 — Publication and content inventory** — Spec: identify the Pages entry point, deployment workflow, assets, links, and public documentation surfaces; complete when the existing delivery path and stale media are mapped.
- [x] **T2 — Information architecture and narrative** — Spec: define a reader journey through value, operating loop, capabilities, trust boundaries, installation, documentation, and contribution; every public claim must be supportable by code or maintained documentation.
- [x] **T2b — Active documentation audit** — Spec: separate current guides, architecture, and specifications from historical records; complete when the README and documentation hub provide an intent-based route through maintained material.
- [x] **T3 — Public design system** — Spec: carry Graphite Observatory typography, graphite, technical-paper surfaces, phosphor green, grid, brand glyph, motion, and accessibility into the public site.
- [x] **T4 — Responsive implementation** — Spec: rebuild the site while preserving the publication pipeline; complete when desktop/mobile navigation, themes, calls to action, examples, and install controls work.
- [x] **T5 — Fresh graphit-code screenshots** — Spec: activate `graphit-labs/graphit-code` in the current Observatory, capture AST, Knowledge, and Memory views, visually inspect them, replace every legacy reference, and remove old assets.
- [x] **T6 — README and maintained documentation refresh** — Spec: rewrite the root README, documentation hub, onboarding, user manual, architecture overview, and UI specification; remove outdated claims and organize current and historical documentation explicitly.
- [x] **T7 — Validation and delivery** — Spec: validate scripts, local links, assets, responsive behavior, theme/menu/tab interactions, production UI build, wiki structure, and worktree scope; finish this log and memory, then run one final `graphit_sync`.
- [x] **T8 — Historical documentation coherence pass** — Spec: audit the complete indexed documentation set, including task logs, changelogs, decisions, examples, and cross-references; repair broken targets, normalize stale product language and branding, remove obsolete media references, and rewrite or clearly supersede contradictory claims without fabricating technical facts.
- [x] **T9 — Current commit on `main`** — Spec: stage only the website/documentation work owned by this task, preserve unrelated code and documentation edits already present in the shared worktree, and create one new commit without rebasing, amending, force-pushing, or changing earlier Git history.

## Implementation Details

- Replaced the former monolithic `docs/site/index.html` with semantic sections for the hero, operating loop, capabilities, trust boundary, installation, documentation, and contribution.
- Added `docs/site/styles.css` for the shared Graphite Observatory public design system, light/dark themes, responsive breakpoints, focus behavior, reduced motion, and reusable cards/frames.
- Added `docs/site/site.js` for persisted themes, mobile navigation, installation tabs, copy actions, reveal motion, and the current footer year.
- Replaced `docs/site/assets/logo.svg` with the exact product glyph used by the application favicon and brand-glyph.
- Captured three 1440×960 JPEGs from the running Graphit UI after explicitly switching the persisted workspace from Linux to `graphit-labs/graphit-code`:
  - `docs/site/assets/observatory-ast-explorer.jpg`
  - `docs/site/assets/observatory-knowledge-explorer.jpg`
  - `docs/site/assets/observatory-memory-explorer.jpg`
- Removed the four legacy blue/glass screenshots: `ast-explorer-3d.png`, `memory-explorer.png`, `hub-project-artifacts.png`, and `hub-upload.png`.
- Rewrote `README.md` as a concise repository front door with the current product model, first run, security boundary, documentation routes, source build, and first-party screenshots.
- Rebuilt `docs/README.md` as an intent-based documentation map that separates maintained guidance from historical evidence.
- Rewrote `docs/guides/getting_started.md`, `docs/guides/user_manual.md`, `docs/architecture/architecture_overview.md`, and `docs/specs/ui_dashboard.md` to match current storage, retrieval, UI, and network behavior.
- Audited the complete Markdown tree, removed 133 translation-process annotations from historical task logs, replaced illustrative link syntax that the wiki compiler mistook for live references, and repaired renamed task, decision, guide, and changelog targets. The repository-level Markdown validator now reports zero missing local targets.

## Use Cases

### UC-01: Evaluate Graphit from the public site
- **Actor**: Prospective user.
- **Preconditions**: The GitHub Pages deployment serves `docs/site/`.
- **Main Flow**:
  1. Open the landing page.
  2. Read the value proposition and operating loop.
  3. Inspect real graphit-code AST, Knowledge, and Memory captures.
  4. Continue to installation, documentation, GitHub, or contribution.
- **Alternative Flows**: Switch theme or use mobile navigation.
- **Error Scenarios**: With JavaScript unavailable, semantic content and anchor navigation remain readable; interactive tabs stay on the first panel.
- **Postconditions**: The reader understands scope, trust boundaries, and a relevant next action.
- **Affected Files**: `docs/site/index.html`, `docs/site/styles.css`, `docs/site/site.js`, `docs/site/assets/logo.svg`, screenshot assets.

### UC-02: Install Graphit from the public site
- **Actor**: New user.
- **Preconditions**: The user knows the target operating system.
- **Main Flow**:
  1. Open the Install section.
  2. Select macOS/Linux, Windows, or source.
  3. Copy the displayed commands.
  4. Continue with the Getting Started guide.
- **Alternative Flows**: Manually select command text if clipboard permission is unavailable.
- **Error Scenarios**: Clipboard failure changes the action label to `Select text` without hiding the command.
- **Postconditions**: The user has version-appropriate installation and first-run commands.
- **Affected Files**: `docs/site/index.html`, `docs/site/site.js`, `docs/guides/getting_started.md`.

### UC-03: Navigate maintained documentation
- **Actor**: User, operator, contributor, or agent.
- **Preconditions**: The repository documentation is available.
- **Main Flow**:
  1. Open `docs/README.md`.
  2. Choose an intent such as install, daily use, MCP integration, architecture, network operation, private distribution, or contribution.
  3. Follow the maintained guide or specification.
  4. Enter historical logs only when rationale or provenance is needed.
- **Alternative Flows**: Start from the root README or one of the public-site documentation cards.
- **Error Scenarios**: Local validation reports a missing file or anchor before delivery.
- **Postconditions**: Current guidance remains distinct from point-in-time historical records.
- **Affected Files**: `README.md`, `docs/README.md`, maintained guides, architecture, and UI specification.

### UC-04: Inspect first-party product evidence
- **Actor**: Reader or maintainer.
- **Preconditions**: Graphit UI is running with `graphit-labs/graphit-code` active.
- **Main Flow**:
  1. Open the AST Explorer and verify Go/TypeScript counts plus the project label.
  2. Open the Knowledge Explorer on System Architecture Overview.
  3. Open the Memory Explorer on the Graphite Observatory design decision.
  4. Capture and visually inspect each view before publishing.
- **Alternative Flows**: Collapse explorer rails to remove irrelevant or non-English historical entries.
- **Error Scenarios**: If the workspace label or data reflects Linux, reject and retake the image.
- **Postconditions**: All published captures show the current Graphit Code project.
- **Affected Files**: The three `observatory-*.jpg` assets and every Markdown/HTML reference to them.

### UC-05: Read historical implementation records without editorial noise
- **Actor**: Maintainer or agent tracing earlier work.
- **Preconditions**: The repository documentation and compiled knowledge wiki are available.
- **Main Flow**:
  1. Open a historical task log from the documentation hub or wiki search.
  2. Read the implementation record without translation-service commentary interrupting the technical narrative.
  3. Follow renamed decisions, guides, changelogs, and successor tasks through valid repository links.
  4. Use maintained specifications for the current behavior when a record describes an earlier state.
- **Alternative Flows**: Search the compiled wiki by title, then open the source path reported in provenance.
- **Error Scenarios**: The wiki linter may classify a valid relative repository link as unresolved when its filename differs from the generated title slug; repository-level validation remains authoritative for source links.
- **Postconditions**: Historical evidence remains useful, readable, and connected without claiming to be the current specification.
- **Affected Files**: Historical Markdown task logs, affected specifications/guides, and `docs/tasks/redesign-github-site.md`.

## Test Cases & Acceptance Criteria

### Feature: Public site experience
Ref: UC-01

#### Scenario: Desktop hero renders current product evidence
```gherkin
Given the static site is served from docs/site
When a user opens it at 1440 by 960 pixels
Then the Graphite Observatory hero is visible
  And the AST capture identifies graphit-labs/graphit-code
  And every site asset returns HTTP 200
```

#### Scenario: Mobile navigation preserves all destinations
```gherkin
Given the site viewport is 390 by 844 pixels
When the user opens the Menu control
Then System, Capabilities, Install, and Docs are visible
  And the page has no horizontal overflow
```

#### Scenario: Theme switching retains hierarchy
```gherkin
Given the site is open in dark mode
When the user activates the theme control
Then the workspace changes to the paper-light theme
  And the graphite header and phosphor primary signals remain legible
```

### Feature: Installation controls
Ref: UC-02

#### Scenario Outline: Select an installation platform
```gherkin
Given the Install section is visible
When the user selects "<tab>"
Then the command panel contains "<marker>"

Examples:
  | tab           | marker          |
  | macOS / Linux | install.sh      |
  | Windows       | install.ps1     |
  | From source   | make install    |
```

### Feature: Documentation integrity
Ref: UC-03

#### Scenario: Maintained local links resolve
```gherkin
Given the refreshed README and maintained documents
When local Markdown links and site href/src references are resolved from their source files
Then every referenced local file exists
  And every site fragment points to a declared id
```

### Feature: Screenshot provenance
Ref: UC-04

#### Scenario: No legacy or Linux capture remains
```gherkin
Given the public site, README, and documentation tree
When screenshot references and asset names are audited
Then no legacy screenshot filename remains referenced
  And each observatory screenshot is a 1440 by 960 JPEG captured from graphit-code
```

### Feature: Historical documentation integrity
Ref: UC-05

#### Scenario: Historical records contain no translation-process commentary
```gherkin
Given the repository Markdown tree
When translation-service annotations and local Markdown targets are audited
Then no translation-process commentary remains
  And every relative Markdown file target exists
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/site/index.html` | Rebuilt | New public information architecture and semantic markup |
| `docs/site/styles.css` | Created | Shared Graphite Observatory visual and responsive system |
| `docs/site/site.js` | Created | Theme, menu, tabs, copy, and reveal behavior |
| `docs/site/assets/logo.svg` | Replaced | Exact product glyph and favicon |
| `docs/site/assets/observatory-*.jpg` | Created | Current first-party product evidence |
| Four legacy `docs/site/assets/*.png` files | Deleted | Remove stale/user-provided UI captures |
| `README.md` | Rewritten | Current repository narrative, onboarding, and media |
| `docs/README.md` | Rewritten | Intent-based maintained documentation hub |
| `docs/guides/getting_started.md` | Rewritten | Clear installation and first-run workflow |
| `docs/guides/user_manual.md` | Rewritten | Current module mental model and daily operation |
| `docs/architecture/architecture_overview.md` | Rewritten | Current system and trust boundaries |
| `docs/specs/ui_dashboard.md` | Rewritten | Current Graphite Observatory behavior and contracts |
| Historical task logs and renamed cross-references | Normalized | Remove translation-process noise and restore valid source navigation |

## Trade-offs & Decisions

- Chose a dependency-free static site because GitHub Pages already publishes `docs/site/` directly; adding a framework would create a build/deploy layer with no product benefit.
- Chose three curated first-party screenshots rather than a broad gallery; each image now explains one distinct signal and can be audited for active-project identity.
- Kept active documentation concise and routed historical records, then extended the coherence pass into those records after the user explicitly authorized editorial rewriting of history.
- Kept the public UI local-first claim qualified: optional remote publication is explicit, and the UI is documented as unauthenticated.
- Preserved Git history exactly as requested: the website and documentation refresh lands in one new commit on `main`; no previous commit is amended, rebased, or force-updated.

## Technical Debt

- [ ] `graphit ui --repo <path>` uses the requested path for project metadata while the AST backend can still resolve its store from process working directory. Current guidance uses `cd <project> && graphit ui`; a product fix belongs in a separate task.
- [ ] The public website loads Manrope and IBM Plex Mono from Google Fonts; self-hosting would remove that runtime request but increase repository assets and maintenance.
- [ ] The global knowledge lint fell from 62 to 48 findings. Repository-level validation reports zero missing Markdown targets; the remaining findings are generated-title/canonical-slug mismatches for valid relative source links plus one generated-index orphan, so removing them cleanly requires a wiki resolver/linter change rather than further source-document distortion.

## System Knowledge

- GitHub Pages publishes `docs/site/` directly on pushes to `main`, releases, and manual runs.
- The workflow replaces the literal `v1.4.0` in `docs/site/index.html` with the latest release tag.
- The workspace selection is persisted in the browser under `graphit-app-state`; starting the server in one repository does not automatically override an earlier selected workspace.
- Browser screenshot output is JPEG data; assets must use `.jpg` rather than relying on content sniffing behind a `.png` name.
- The current Graphit Code AST screenshot shows Go as the dominant language and displays `graphit-labs/graphit-code` in the graph status bar.

## Progress Log

### 2026-08-31
- Opened the task log before implementation and consulted memory, knowledge, AST, and publication configuration.
- Confirmed the direct static Pages pipeline and the release-token substitution requirement.
- Audited the former monolithic glass/blue site and its four stale screenshots.
- Defined a shorter public narrative: hero and proof, operating loop, capabilities, trust boundary, installation, documentation, and contribution.
- Expanded scope when requested to include the root README and all maintained documentation while preserving historical records.
- Rebuilt and restarted the current UI after the user reported product updates.
- Diagnosed the `--repo` AST-store mismatch while attempting Linux capture.
- Recorded the user's replacement of the Linux requirement with the first-party graphit-code project.
- Explicitly switched the persisted workspace to `graphit-labs/graphit-code` and rejected the Linux data shown before that switch.
- Captured and visually verified the AST, Knowledge, and Memory explorers from graphit-code.
- Implemented the responsive public site, exact glyph, light/dark themes, mobile navigation, install tabs, and copy behavior.
- Rewrote the root README, documentation hub, Getting Started, User Manual, Architecture Overview, and UI Dashboard specification.
- Deleted all four legacy screenshots and updated every public media reference.
- Browser QA passed at 1440×960 and 390×844 in light and dark themes; mobile navigation and install-tab selection worked.
- Local HTML/Markdown reference validation, JavaScript syntax validation, `git diff --check`, and the current `internal/ui` production build passed.
- Knowledge lint completed and its pre-existing historical/example target findings were recorded as debt. The temporary static QA server was stopped; the rebuilt Graphit UI remains active on port 8080 in the graphit-code workspace as requested.
- Reopened the task after the user explicitly authorized rewriting historical documentation for coherence. Replaced the former preservation invariant in project memory and this log; historical prose, links, branding, and examples are now in scope under T8 while technical facts remain evidence-based.
- The user clarified that “history” does not authorize rewriting Git commits. T9 now requires a new commit on top of `main`; earlier commits remain byte-for-byte untouched, and unrelated worktree changes must not be staged.
- Completed T8: removed 133 translation-process annotations, repaired every truly missing local Markdown target, and reduced knowledge lint from 62 to 48 findings. The remaining findings were classified against the repository validator rather than hidden by converting valid source navigation into wiki-only slugs.
- Restarted the static site preview on port 4173 and confirmed HTTP 200 there and on the Graphit UI at port 8080.
- Prepared T9 as a new commit on top of `main`, explicitly excluding unrelated AST/query work and overlapping documentation edits already present in the shared worktree.
