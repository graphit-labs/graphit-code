# Foundations and tokens

[Design system](../design_system.md) · [Patterns](patterns.md) · [Evolution](evolution.md) · [Workspaces](workspaces.md)

Audience: designers, frontend contributors and agents selecting visual values. Baseline: 2026-09-20. Tables below record actual source values. Guidelines that are not yet shared CSS variables are explicitly marked as design rules.

## Color roles

Cobalt means action or current selection; navy establishes persistent structure; neutral surfaces distinguish canvas from content. Teal signals success, amber a condition requiring attention, and red an error or destructive consequence. An entity type or graph relationship can have a categorical color, but must retain its textual identity. Never infer permissions or completion from color alone.

The brand anchors are cobalt `#3155D9`, navy `#142449`, cool canvas `#F3F6FC`, ink `#202D45`, white and teal `#16806A`. These are brand references, **not exact hexadecimal equivalents of every Code HSL token**. Code, Broker and site currently implement semantic equivalents with small value differences.

### Code: exact semantic tokens

Source: [index.css](../../../internal/ui/src/index.css), `:root` and `.dark`. Color values are HSL channels, consumed as `hsl(var(--token))`; `--radius` is a length. [Tailwind configuration](../../../internal/ui/tailwind.config.js) maps these tokens to classes such as `bg-card`, `text-foreground`, `border-border` and `ring-ring`.

| Token | Light | Dark |
| --- | --- | --- |
| `--background` | `220 50% 97%` | `220 42% 10%` |
| `--foreground` | `220 37% 20%` | `220 35% 94%` |
| `--card` | `0 0% 100%` | `220 36% 14%` |
| `--card-foreground` | `220 37% 20%` | `220 35% 94%` |
| `--popover` | `0 0% 100%` | `220 36% 16%` |
| `--popover-foreground` | `220 37% 20%` | `220 35% 94%` |
| `--primary` | `227 69% 52%` | `222 100% 76%` |
| `--primary-foreground` | `0 0% 100%` | `222 48% 15%` |
| `--secondary` | `222 38% 93%` | `220 32% 20%` |
| `--secondary-foreground` | `220 37% 20%` | `220 35% 94%` |
| `--muted` | `220 28% 94%` | `220 30% 18%` |
| `--muted-foreground` | `220 17% 43%` | `220 22% 70%` |
| `--accent` | `225 70% 95%` | `224 37% 22%` |
| `--accent-foreground` | `227 65% 39%` | `222 100% 82%` |
| `--destructive` | `0 64% 47%` | `0 84% 72%` |
| `--destructive-foreground` | `0 0% 100%` | `220 42% 10%` |
| `--border` | `220 28% 84%` | `220 25% 30%` |
| `--input` | `220 24% 75%` | `220 25% 40%` |
| `--ring` | `227 69% 52%` | `222 100% 76%` |
| `--radius` | `0.5rem` | `0.5rem` |
| `--success` | `167 71% 29%` | `164 60% 63%` |
| `--warning` | `34 85% 38%` | `40 90% 66%` |
| `--info` | `211 76% 44%` | `210 90% 73%` |

Use foreground/background pairs together. `muted` is a surface in Code; `muted-foreground` is text. Use `primary-foreground` on primary fills rather than assuming white works in both themes. Do not put raw hex values into ordinary feature components when a semantic role exists.

```tsx
<section className="rounded-lg border border-border bg-card p-6 text-card-foreground">
  <h2 className="text-lg font-semibold">Task evidence</h2>
  <p className="mt-2 text-sm text-muted-foreground">Inspect checks before completion.</p>
  <a className="mt-4 inline-flex text-primary" href="/task/explorer">Open tasks</a>
</section>
```

This is a composition example, not a new exported component or API.

### Broker administration: exact tokens

Source: Graphit Broker `internal/broker/adminui/index.html`, `:root` and `body.dark`. Values without a dark override are inherited, including dimensions. `--muted` is text here, so do not mechanically copy a Code token name.

| Token | Light | Dark |
| --- | --- | --- |
| `--background` | `#f3f6fc` | `#101a2d` |
| `--foreground` | `#202d45` | `#e9eef8` |
| `--card` | `#fff` | `#18253d` |
| `--card-solid` | `#fff` | `#18253d` |
| `--muted` | `#5b6983` | `#a6b7d6` |
| `--muted-surface` | `#edf1f9` | `#20314d` |
| `--primary` | `#3155d9` | `#9bb4ff` |
| `--primary-strong` | `#2546bd` | `#bdd0ff` |
| `--primary-foreground` | `#fff` | `#142449` |
| `--accent` | `#e8edfc` | `#253b67` |
| `--accent-foreground` | `#294ba8` | `#c2d1ff` |
| `--border` | `#d5deee` | `#34476a` |
| `--input` | `#b7c5df` | `#4b628a` |
| `--danger` | `#b93438` | `#ffa0a8` |
| `--danger-soft` | `#fcebed` | `#422934` |
| `--ok` | `#16806a` | `#74d8bc` |
| `--ok-soft` | `#e5f5ef` | `#203d3e` |
| `--warn` | `#926110` | `#efd091` |
| `--warn-soft` | `#faf1db` | `#3d382c` |
| `--sidebar-width` | `238px` | `238px` |
| `--radius` | `8px` | `8px` |

Authentication in `internal/broker/oauthui/index.html` is light-only. Its local tokens are `--ink: #202d45`, `--muted: #5b6983`, `--line: #d5deee`, `--blue: #3155d9`, `--paper: #fff`, and `--ground: #f3f6fc`. The brand bar is navy `#142449`; fields use `#b7c5df`, error/success use the corresponding admin light values. There is no dark override or theme switch. Token names differ from administration; copy semantic roles, not names.

### Public site: exact tokens

Source: [styles.css](../../site/styles.css), `:root` and `html[data-theme="dark"]`. A dark value repeated below means inheritance when there is no override.

| Token | Light | Dark |
| --- | --- | --- |
| `--bg` | `#fff` | `#101a2d` |
| `--surface` | `#f3f6fc` | `#18253d` |
| `--text` | `#202d45` | `#e9eef8` |
| `--muted` | `#5b6983` | `#a6b7d6` |
| `--line` | `#d5deee` | `#34476a` |
| `--blue` | `#3155d9` | `#9bb4ff` |
| `--accent` | `#eaf0ff` | `#23365b` |
| `--navy` | `#142449` | `#142449` |

The site's white base (`--bg`) and neutral sections (`--surface`) suit editorial reading; do not reverse them simply to match the application's canvas. Components may also use explicit brand colors for fixed navy compositions. New colors require a named purpose and both-theme review.

### Cross-surface mapping

| Role | Code | Broker admin | Site |
| --- | --- | --- | --- |
| Page canvas | `background` | `background` | `bg` |
| Content surface | `card` | `card` | `surface` for contrasted sections |
| Main text | `foreground` | `foreground` | `text` |
| Secondary text | `muted-foreground` | `muted` | `muted` |
| Main action | `primary` | `primary` | `blue` |
| Border | `border` | `border` | `line` |
| Positive / attention / negative | `success` / `warning` / `destructive` | `ok` / `warn` / `danger` | No general status token set yet |

## Typography

| Role | Current baseline | Design rule |
| --- | --- | --- |
| Interface/body | Code 14px/1.55; Broker admin 14px/1.6; auth 15px/1.6; site 15px/1.7 | Public Sans with system fallbacks; favor legibility over fitting another column |
| Work page heading | 30px/1.2, weight 650; mobile 26px | Reserve this hierarchy for workspace orientation |
| Broker page title | 36px, weight 650; auth 32px | One clear primary heading |
| Site hero | `clamp(48px, 5.4vw, 76px)`, weight 600; smallest breakpoint overrides 49px | Editorial emphasis belongs on the site, not every admin page |
| Section/component headings | WorkSection 16px/650, record titles 13px/600; Broker h2 17px/650 | Distinguish sections through scale, weight and spacing |
| Code, commands, identifiers | IBM Plex Mono; Broker mono 12px | Preserve alignment and exact characters; support wrapping or contained scrolling |
| Metadata | Existing chrome contains 10–12px text | New critical content must not depend on tiny metadata; target 12px or larger for secondary text |

Code imports Public Sans and IBM Plex Mono through Google Fonts in CSS; the site loads them in HTML. Broker names these families but makes no remote font request: system/monospace fallbacks keep its CSP intact. Test fallback rendering. Do not add an external font to Broker without changing and reviewing its resource policy separately.

Current Code work headings use tracking `-0.04em`; site headings `-0.05em`; auth `-0.04em`. Keep monospace text at normal tracking. Use sentence case for user-facing labels, preserve conventional acronyms (AI, API, AST, CLI), and reserve small uppercase captions for established patterns.

Adoption gap: Tailwind `font-sans` still names Manrope, while the base Code CSS uses Public Sans. New UI should inherit the base font; any future cleanup must reconcile the utility and its consumers instead of assuming it already matches.

## Spacing, density and shape

**Design rule for new work:** start with a 4px spacing rhythm: 4/8 for tightly related elements, 12/16 for control groups, 24/32 for panels and 48/64 for major sections. These are recommended composition values, not exported `--space-*` tokens. Existing optical values such as 18, 21 and 27px remain in the baseline; do not describe an exact universal spacing scale that does not exist.

Use dense spacing for evidence and source inspection, comfortable spacing for forms, and larger section separation for the site. There is no user-selectable density mode today. Keep related labels/values close; separate unrelated actions; align repeated field and table columns. Long names may truncate in navigation with a way to inspect the full value; never make an exact identifier impossible to retrieve.

| Shape/layer | Current baseline | Extension rule |
| --- | --- | --- |
| Controls | Code main action/nav radius 6px; base radius 8px; Tailwind lg/md/sm 8/6/4px | Reuse the local control pattern |
| Panels | Work panels/tables 7px; work dialogs 8px; Broker radius 8px | Subtle rounding; do not make every container a pill |
| Borders | Typically 1px using semantic border role | Borders separate surfaces; they are not the only focus cue |
| Elevation | Opaque surfaces, restrained shadows; compatibility `.glass-panel` has a faint shadow | Add elevation for overlap, not decorative depth |
| Mark | 34px square, 8px corners in product chrome | Preserve the supplied glyph geometry |

## Layout and responsive baseline

Breakpoints are based on each surface's composition. They are **not a single shared device taxonomy**.

| Surface | Current rules |
| --- | --- |
| Code desktop | 238px fixed rail and 64px header; new work surfaces use available width and 32px padding, overriding the legacy shell page max-width |
| Code intermediate | Shell rail 208px at 768–1150px; work surfaces use 24px padding below 1050px and stack header/context actions |
| Code ≤767px | Modal navigation; header 56px; work surface padding 20px 16px; dossiers and forms stack |
| Code mobile navigation | 290px wide, max 90vw, height 100dvh |
| Code work dialog | Width up to 950px, max-height 90dvh; header/footer remain reachable and body scrolls. Backdrop padding 24px, 12px on mobile |
| AST investigation | Results/source and Query lab stack at ≤1000px. The optional map is 600px high, capped at 65dvh; map settings are disclosures |
| Broker admin | Rail 238px; form/split layouts collapse at ≤1100px; mobile navigation at ≤767px with 270px drawer |
| Broker auth | Workspace max-width 1000px; transaction/header/card max-width 650px; card padding 34px 42px, reduced to 26px 22px at ≤600px |
| Site | Main wrap max-width 1200px and width `calc(100% - 96px)`; adjustments at ≤1150px, mobile composition/menu at ≤800px, small-screen adjustments at ≤420px |

Code/Broker declare a 320px minimum. Treat 320px as a review case, not certification that every existing route already fits. At 390px, preserve primary actions, wrap explanatory copy, and put unavoidable horizontal scrolling inside tables/code. At 200% zoom, reflow should expose the same work. Test actual available workspace width; a 1280px viewport does not leave 1280px after navigation.

## Layering and motion

Code's current sidebar/header use z-index 40/30; legacy explorer wrappers establish `isolation: isolate`; new work surfaces use normal document composition. `MobileSidebar` uses a native modal dialog in the browser top layer. `ModalPortal` places memory forms, Hub dialogs, confirmations and image previews under `document.body`, outside the isolated explorer. Keep overlays out of trapped stacking contexts; increasing a child's z-index does not escape its parent.

Broker mobile scrim/sidebar/trigger use 29/30/40. The site mobile menu uses 20 and is an expandable navigation region, not a modal. These values are local layering contracts; do not create a global arbitrary z-index escalation across repositories.

Color/border transitions use 150ms. Code also retains Tailwind animations of 200ms (accordion), 250ms (fade) and 300ms (source slide). Reduced-motion CSS disables nonessential transitions/animations; preserve it when introducing movement. A pending state must remain understandable without animation. Avoid looping decorative motion and animated layout shifts in dense workspaces.

## Icons and diagrams

Code uses Lucide outline icons. Broker/site use their inline SVG and CSS equivalents. Match stroke weight, apparent size and alignment to the surrounding control; commonly 16px for navigation and 20px for the mobile trigger. Decorative icons must not repeat the accessible name; standalone actionable icons need a name describing the action. Never use an icon or color as the sole sign of a destructive operation, permission boundary or task status.
