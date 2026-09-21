# Graphit design system

The shared design reference for **Graphit Code, Graphit Broker and the public GitHub Pages site**. Use it when designing a screen, changing a component, writing product copy or reviewing a contribution, including work performed by AI agents.

Baseline: the enterprise redesign of 2026-09-20. This reference describes the source in the accompanying repositories, not a promise that the same revision is deployed everywhere. Documentation is maintained in English, following the repository convention.

## Read the system

| Your next decision | Reference |
| --- | --- |
| Understand the product, choose a journey and design a screen | Principles and screen brief below |
| Choose color, typography, spacing, shape, iconography or responsive dimensions | [Foundations and tokens](design_system/foundations.md) |
| Choose a page journey, find a route owner or use shared work primitives | [Workspaces and page contracts](design_system/workspaces.md) |
| Build navigation, forms, tables, explorers, feedback or authentication | [Components and interaction patterns](design_system/patterns.md) |
| Extend the system, verify a change or find the owning source | [Evolution, ownership and validation](design_system/evolution.md) |
| Understand Code routes and backend contracts | [UI specification](ui_dashboard.md) |

**How to interpret this reference:** “Current baseline” records verified implementation. “Design rule” is the expectation for new or changed UI. “Adoption gap” identifies an existing difference; documenting a rule does not imply every legacy component already satisfies it. There is no shared component package or automatic token distribution across the three surfaces today.

## Product concept

Graphit enables AI software engineering for an individual, a team or an enterprise. Its differentiator is the engineering layer around the model: source-backed context, durable decisions, explicit ownership, resumable work and evidence attached to completion. The LLM remains probabilistic. Describe determinism in the lifecycle and control rules; never imply that a model's output becomes deterministic or that generated software is guaranteed correct.

The core sequence is **establish context → coordinate execution → inspect evidence**. This is a journey through connected capabilities, not a claim that all work follows a rigid three-step wizard.

The product's goal is software engineering, not optimizing token counts. Knowledge acquired with AI represents an investment: preserved decisions, maintained documentation and verified work let an individual, team or enterprise build progressively on what is already known. Explain the concrete recording and maintenance mechanisms; do not promise automatic retention of everything a model has seen. Token savings are neither the primary value proposition nor a guaranteed outcome.

| Stage | Question the user needs answered | Product evidence |
| --- | --- | --- |
| Context | What code, knowledge and prior decisions apply here? | AST/source, maintained knowledge, project/personal memory |
| Execution | What are we delivering, who owns it and what happens next? | Sessions, task scope, claims, checkpoints and next actions |
| Evidence | Why is this work considered complete? | Checks, lifecycle history, linked tasks and preserved decisions |

### Surface responsibilities

| Surface | Primary job | Composition |
| --- | --- | --- |
| Code | Inspect and coordinate engineering work in a project/ecosystem | Persistent navigation, project context, work area, job-specific catalogues, reading dossiers, investigation and execution tools |
| Broker | Govern identity, resource access and shared services | Authorized inventory first, scoped administration, explicit editing forms |
| Site | Explain the value, boundaries and adoption path | Product proposition, engineering model, Code/Broker responsibilities, solo/team/enterprise, installation and docs |
| Broker authentication | Complete the current identity step | Compact brand bar, transaction progress and one focused server-rendered step |

Solo, team and enterprise share the same engineering model. Enterprise introduces coordination and governance needs; it is not a different decorative theme. The site should explain these differences without inventing product tiers or entitlements.

## Design rules

1. **Start from the work.** Define the user's decision, the source of truth and the next action before choosing a layout.
2. **Make context persistent.** Keep project, scope and identity visible where they affect an operation. A display name is not a substitute for an immutable identifier in an exact operation.
3. **Show evidence, not implied confidence.** Every status, count and completion claim must come from domain data. Label illustrative workflows; do not present sample statistics as live telemetry.
4. **Keep continuity visible.** Intent, owner, next step, decisions and checks must be inspectable across sessions. A chat transcript is not the information architecture.
5. **Use hierarchy to manage density.** Navigation → page purpose → current context → primary work → supporting detail. Use spacing, alignment, borders and typography before extra colors or cards.
6. **Make state changes legible.** Distinguish selection, editing, pending submission, confirmed completion and failure. Keep backend permissions and lifecycle rules authoritative.
7. **Use one visual language, adapted to the job.** Code can be dense, Broker deliberate, and the site spacious. Roles, mark, content principles and interaction expectations stay consistent.
8. **Preserve access to the work.** Responsive layouts may change composition; they must retain core actions, readable content and keyboard access.

## Brand and expression

The visual direction is calm, precise and editorial: navy structure, cobalt action, cool neutral surfaces and a restrained bracket mark. Engineering relationships are the visual subject. The new layout does not depend on ornamental gradients, glows, glass effects, stock AI imagery or artificial activity feeds.

The mark uses opposing brackets to express bounded context. Use the supplied SVG or the existing CSS glyph; do not redraw it per screen. Preserve aspect ratio and white bracket geometry. The small application tile is 34px with an 8px corner radius in the current chrome. For new placements, keep at least one bracket-stroke width of internal breathing room and a quarter-mark width clear of adjacent unrelated content; these are placement rules, not a new asset generator.

Use “Graphit” for the umbrella, “Graphit Code” and “Graphit Broker” for product references. “Observatory” still appears in legacy Code titles/documentation; it is not a separate design system. Do not rename public routes or contracts as a side effect of a copy change.

| Asset | Maintained source |
| --- | --- |
| Code logo | [logo.svg](../../internal/ui/public/logo.svg) |
| Code favicon | [favicon.svg](../../internal/ui/public/favicon.svg) |
| Site logo | [logo.svg](../site/assets/logo.svg) |
| Broker favicon | Graphit Broker: `internal/broker/favicon.svg` |
| Product navigation glyphs | Code `index.css`; Broker embedded admin/auth CSS |

Use diagrams when they explain actual relationships. Label conceptual diagrams as illustrations and give them a readable text equivalent. Product screenshots must represent the current layout, use non-sensitive data and state when fixtures are shown. Raster imagery is optional and must clarify the product rather than replace its explanation.

## Screen brief: required before implementation

Fill this out in the implementation task or design proposal; do not create a separate permanent document for every small screen.

```text
User and goal:
Starting state and prerequisites:
Project / identity / resource scope:
Authoritative data and allowed actions:
Question this screen answers:
Primary action and observable completion:
Supporting information and progressive disclosure:
Loading / empty / filtered-empty / error / forbidden / stale states:
Keyboard, narrow-screen and long-content behavior:
Evidence and validation required:
Existing pattern to reuse; any justified exception:
```

Example — inspect task evidence: start from a selected project and a bounded task catalogue; open exact task detail; show scope, owner, state, checks and history; distinguish missing selection from no matching results and failed retrieval. At narrow widths, present catalogue and detail sequentially. Do not invent a “quality score” or move completion authority into the view. Domain behavior remains in the [Task specification](task_module.md).

## Change policy

The canonical reference lives here and in its linked reference pages. Broker documentation links to it; avoid maintaining a divergent prose copy. Each change updates implementation, this reference and its validation evidence in the same delivery. Differences that cannot yet be removed must remain explicit in [adoption gaps](design_system/evolution.md#adoption-gaps).
