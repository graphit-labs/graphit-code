# Components and interaction patterns

[Design system](../design_system.md) · [Foundations](foundations.md) · [Evolution](evolution.md) · [Workspaces](workspaces.md)

Audience: contributors turning an engineering journey into a usable screen. The contracts below are **design rules for new and changed UI**. “Current implementation” identifies examples to reuse; it does not claim a complete reusable component library or universal adoption.

## Information architecture

Code's navigation is grouped by engineering purpose:

| Group | Destinations / intent |
| --- | --- |
| Engineering | Workspace, sessions, tasks and evidence, live search when enabled |
| Project context | Code intelligence, knowledge, project memory, personal memory |
| Shared ecosystem | Hub registry, project artifacts, publication, installed code/knowledge contexts |
| Operations | Projects, daemon, Dream |

Keep existing route and feature gates. `AppShell` supplies global navigation and the common header. Its right-hand `WorkspaceSelectors` group is the single owner of global Project and Agent controls on every route. Explorers supply only local domain context and work tools; do not repeat global selectors in page headers, sidebars or mobile drawers. Do not add another application sidebar inside a feature. The active project and active agent are separate concepts: changing either must not silently imply the other changed.

Broker groups Workspace, Identity & access, and Operations. Authorized resource access and administrative privileges are distinct; visibility follows existing permission checks. Place inventory before editing so the operator can inspect state before acting. The site uses a content journey, not application navigation: proposition → engineering approach → product roles → adoption context → installation → documentation.

## Page compositions

| Composition | Use when | Required hierarchy | Narrow-screen behavior |
| --- | --- | --- | --- |
| Workspace overview | User needs orientation and a starting action | Purpose, job chooser, useful project metadata and administration link | Stack sections and retain meaningful links |
| Catalogue/detail | User selects one durable record from many | Scope/search/filters, bounded results, exact detail | Sequential catalogue and detail with a clear return path |
| Code investigation | User needs to locate implementation and understand a change | Scoped search → symbol source → typed relation evidence; Query lab and map are peer tools | Results, selected source and supporting controls stack; tables and code scroll locally |
| Administrative inventory/editor | User changes access or identity configuration | Current inventory, consequences/scope, grouped fields, submit outcome | Stack form context and inputs; scroll/focus to the actual editor |
| Authentication | User completes one server-defined identity step | Compact brand bar, transaction context, current server-selected form, recovery outcome | One readable column without hiding the required step |
| Editorial/product | User is evaluating or adopting Graphit | Clear claim, supporting model/evidence, relevant action | Reflow diagrams and sections; preserve install/docs access |

Do not use cards for every label/value pair. A card should group a meaningful object, action or section. Use tables for comparison, lists for selection, prose for explanation and source panels for exact text. Dashboard metrics require a real data owner and an actionable interpretation.

### Allocate space by the information's role

Use Code contexts as the catalogue/detail reference. A selection list should not consume the space needed to understand or act on its selected record. The shared column tokens live in `components/shared/engineering.css`:

| Role | Token and proportion (left / right) | Use |
| --- | --- | --- |
| Catalogue → detail or composition | `--work-columns-detail`: `1fr / 1.65fr` (about 38% / 62%) | Contexts, artifact inspectors, project administration and the Live investigation brief |
| Content or form → supporting context | `--work-columns-support`: `1.65fr / 1fr` (about 62% / 38%) | Publication, dossiers, provenance, execution conditions and query vocabulary |

Do not reserve a fixed 220–350px right column on a wide workspace. Support can be secondary and still be legible. Choose the role from the work performed there, not from the HTML `aside` tag: Live preparation's right side contains the brief and Start action, while publication's right side contains supporting context. Keep prose line lengths bounded and wrap long identifiers. Artifact identity fields use two columns only when their inspector has at least 600px of content width.

`WorkPage` establishes the named `work-surface` inline-size container. At 900px or less of **available workspace content**, these two-column compositions stack in document order; this accounts for the global navigation and page padding. The memory editor uses its own `work-dialog` container and stacks at 760px. Existing narrow-screen navigation and spacing rules still apply. Tables/code may scroll within their own region; the page must not gain horizontal overflow. AST search results keep a bounded 220–300px list to reserve room for source code; equal-purpose pairs, form rows and runtime strips retain their own structures.

## Component contracts

### Navigation and project selection

Current implementations: Code `components/layout/AppShell.tsx`, `Sidebar.tsx`, `WorkspaceSelectors.tsx`; Broker admin sidebar and `syncMobileMenu`; site `#site-nav`.

- Use links for destinations and buttons for local actions. Expose the current destination, expanded state and meaningful names.
- Keep project identity inspectable when a compact label truncates. Refresh dependent data after switching; never present prior-project results as current.
- A mobile **modal** navigation surface must constrain keyboard focus, make background controls inactive, close on Escape and restore focus to its trigger. Code uses `<dialog>`; Broker implements inert background and a focus loop.
- The site's expandable menu is nonmodal: its links remain in normal navigation flow. Escape closes it and restores the trigger. Do not copy modal semantics without implementing modal behavior.
- Closing a menu by choosing a route must leave the destination usable. Test long project names, many destinations and short viewport heights.

Project and Agent use themed Radix Select menus: Tab reaches each trigger; Enter/Space opens it; arrows and type-ahead move through options; Escape closes and restores trigger focus. The menu is portaled, uses semantic surface/action tokens, highlights the active option and marks the selected item with a check. Project options include their directories. Menus stay within the viewport and scroll independently. Both remain visible in the shared header at narrow widths; labels and selection are never replaced by unlabeled icons. Keep the controls disabled with loading/empty text when options are unavailable, and retain the shared Refresh action. Disambiguate repeated project names by directory. Selecting a project leaves the agent unchanged and vice versa; global context changes still invalidate scoped requests and dialogs. A domain filter such as publisher project or indexed source is separate and belongs with the data it filters.

### Combined refresh

`WorkspaceRefreshProvider` in AppShell owns the single Refresh action in the header. It refreshes the global project/agent catalogue once, then runs every `usePageRefresh` callback still registered for the active route/project/agent. Pages register their data loaders, not a remount or browser reload. Catalogue and selected-detail loaders participate together; polling and mutation revalidation continue independently. Workspace, Ecosystem and publication forms need only the global catalogue refresh; publication drafts are preserved.

Return promises from every callback and wait for all sources, including when another fails. The button remains busy until the combined work settles and ignores duplicate activation. A context change during catalogue loading prevents the old callbacks from running; domain loaders must still reject stale responses after they start. Surface-specific errors remain near their data and uncaught refresh failures appear near the header. No page toolbar or section adds another generic Refresh button.

### Actions

| Kind | Use | Content and state |
| --- | --- | --- |
| Primary | Main next step for the current decision | Verb + object; cobalt fill with paired foreground |
| Secondary | Related alternative | Neutral/outlined treatment with a clear label |
| Tertiary/link | Navigation or lower-emphasis action | Keep discoverable; an icon is supporting information |
| Destructive | Irreversible or consequential domain change | Name target/consequence; use destructive role and existing confirmation contract |
| Icon-only | Familiar compact operation in dense tools | Accessible action name, visible focus, sufficient hit area |

Prefer one emphasized primary action per decision region, not an arbitrary limit of one button per page. While a mutation is pending, prevent duplicate submission and preserve a meaningful progress label. A disabled control needs an understandable reason nearby where the restriction is not obvious. Do not use disabled opacity to conceal an authorization failure.

### Fields and forms

Current examples: Memory form, project switchers, Broker grant/identity editors, OAuth templates.

1. Name the purpose and scope before fields. Keep labels persistent; placeholders are examples, not labels.
2. Group fields by the decision they support. Put explanatory text next to the relevant group; separate advanced settings when the existing domain allows it.
3. Retain entered values when validation or the network fails. Associate errors with the field and provide an actionable message.
4. Make required, optional and read-only meaning clear. A read-only configuration must not look editable.
5. Confirm success only after the authoritative response. On conflict, explain that state changed and direct the user to refresh/review before retrying; do not silently overwrite.
6. Preserve backend validation, CSRF/revision protections, permissions and existing destructive confirmation behavior.

Broker `focusEditor` focuses the correct field and scrolls its actual panel into view. Reuse this relationship when rearranging forms; a fixed pixel scroll offset is not a reliable navigation contract. Authentication field names, form methods/actions and Go template branches are protocol surfaces and must survive a visual change.

### Lists, tables and filters

- Keep the scope and active filters visible. Distinguish “no records yet” from “no results match these filters”.
- For bounded catalogues, show available pagination/load-more behavior; a loaded page length is not necessarily the total.
- Rows need stable identity and a predictable selection affordance. Keep names and identifiers distinct.
- Use explicit headings for tabular data. Right-align comparable numbers where helpful; render IDs and paths in monospace.
- Give long IDs/capabilities a contained wrapping or scrolling strategy. The whole page should not acquire horizontal overflow.
- Preserve selection/context when refreshing where possible. Do not announce stale cached data as authoritative access.

### Status, badges and evidence

Use compact badges for domain state, accompanied by text. Map positive/attention/negative semantics only when the domain supports them; “claimed” or “in progress” is not success. Display actor, revision and time where they explain an event. Keep task checks and lifecycle history inspectable rather than collapsing them into an unexplained percentage.

Success feedback must correspond to a confirmed operation. A toast is transient feedback, not a durable audit record. Important errors or decisions need a persistent place in the screen. Existing Code `components/shared/Toast.tsx` and Broker message regions are local implementations; review announcement behavior when extending them.

### Metadata badges

Use `WorkBadge` from `EngineeringUI` for types, tags, policy flags and compact metadata. Project artifacts is the visual reference: a discreet rectangle with a 4px radius, 11px medium-weight text, normal casing, a thin border and a subtle surface. Do not introduce rounded pills, uppercase tracking, miniature bold text or a separate color palette per feature. Wrap badge groups when space is limited; keep labels readable.

The default `neutral` tone is for types and tags. Use `info` for active work and policy (including Mandatory), `warning` for attention or priority (including Important), `success` for confirmed success, and `danger` for failure. All tones use theme tokens in light and dark mode. Mandatory is a startup policy, not a successful outcome. Keep a text label alongside any decorative icon.

`WorkStatusBadge` shares the same primitive: open/cancelled and unknown states remain neutral; in progress/running/preparing use info; completed/passed/ready use success; failed/error use danger; blocked uses warning. A session navigation chip keeps keyboard activation and visible focus. Badges do not replace actionable controls. Configurable graph category colors remain data encodings, outside this status palette.

### Tabs and progressive disclosure

Tabs switch peer views within one context; links navigate to a different destination. Keep names specific enough to predict the panel. The site audience/install implementation uses `aria-selected`, `aria-controls`, roving tabindex and arrow/Home/End navigation. Maintain the pairing between each tab and its panel.

Do not hide information required to understand an action under an unlabeled disclosure. Preserve the user's current query, filters or editor content when switching views if the domain supports it. Replacing a panel must not strand focus in hidden content.

### Dialogs, drawers and image previews

Current implementation: Code `components/shared/ModalPortal.tsx`, used by Memory, Hub alias/publication/confirmation dialogs, daemon confirmation and knowledge image preview. It portals to `document.body`, makes `#root` inert, locks body scrolling, handles Escape, loops focus and restores prior focus. The child supplies its dialog semantics and accessible name. Work dialogs compose `.work-modal-backdrop`, `.work-dialog`, header, `.work-dialog-body` and footer; the body scrolls independently. The portal itself is behavior, not styling.

- Name the dialog and expose a visible close or cancel control.
- Set initial focus deliberately; keep Tab/Shift+Tab within a modal and restore focus after closing.
- Keep actions reachable on short screens; allow modal content to scroll.
- Avoid nested modals. The current portal has no stack manager for multiple simultaneous dialogs; extending it requires an explicit ownership/focus design.
- A source pane that replaces the work area is not automatically a modal dialog. Preserve a clear close/return action and test keyboard order with the underlying tools.
- Do not solve overlay problems by incrementing z-index inside an isolated explorer. Use the established portal or native top layer.

### Reading with provenance and durable guidance

Knowledge uses `Library`, `Reading` and `Search results` as peer views. Library tables answer what exists; reading gives the document the largest region; source metadata and references explain where its statements come from. Collapse the search/context tools during focused reading without losing access to them. A generated answer is a separate result with sources, never a replacement for the maintained document. Navigation history is local reading history, not a fabricated document audit trail.

Memory uses a current-record catalogue and revision dossier. The revision chain is explicit and historical content is labeled. Curating a memory is a content decision: title/body first, then type, importance and mandatory startup policy. The modal's scrolling body keeps submit/cancel visible; immutable metadata remains in the dossier. Current implementations are `WikiExplorerPage`, `WikiContextsPage`, `MemoryExplorerPage` and the shared modal lifecycle.

### Investigation and query evidence

Current AST implementation: `ExplorerPage`, `QueryBar`, `TabularResults`, `ContextsPage`. Search is the first action and queries the selected index; no sample graph is fetched merely by entering the page. A selected symbol opens indexed source, then Incoming/Outgoing/Potential impact views. Resolve one indexed identity before traversal. Empty, ambiguous or stale identities must not silently merge evidence.

AI-generated Cypher is an editable draft. Generation and execution have separate labels, loading states and explicit user actions. Scope changes invalidate results and pending generation. A bounded traversal returns evidence about the indexed endpoints; label its limit and dynamic-analysis gaps. Do not manufacture graph edges from a result table.

The optional map retains schema filters, colors, languages, clusters, 2D/3D, zoom, fit, physics and file navigation. Its graph and tables are different representations with distinct data contracts. The context directory leads with source origin and scope before the investigation action.

### Source, commands and DX

Code and installation commands are first-class content. Preserve whitespace and exact characters; distinguish commands from output and placeholders. Use monospace, line numbers where useful and source location/highlight context when available. Keep language/filename visible and support contained scrolling for long lines.

Copy actions must copy the intended value, report success only when copying succeeds, and expose manual selection when clipboard access fails. Site copy feedback currently resets after 1600ms. Never put real credentials into examples, screenshots or generated previews. A shortened identifier may be a display affordance, but exact values must remain accessible for engineering operations.

### Authentication and permissions

Broker authentication is server-rendered and currently light-themed. Password, MFA, recovery, provider, consent and device states are determined by the server. The same focused form composition adapts to each branch; UI changes must not bypass or invent a branch.

Keep resource grants separate from administrative roles. Hiding a navigation item is not access enforcement. Distinguish unauthenticated, unauthorized and empty authorized inventories where the API discloses that distinction; never infer additional access from presentation or cached state.

## State model for every feature

These are required design cases, not a statement that every route currently has a dedicated component for each case.

| State | What the view should communicate | Recovery/action |
| --- | --- | --- |
| Initial/no selection | What can be inspected and what selection is needed | Choose project/record; retain navigation |
| Loading | Which region is pending without fabricating progress | Preserve context; avoid layout jumps and duplicate actions |
| Empty, not initialized | Why useful content is absent | Explain a real next step such as initializing/registering a project |
| Filtered empty | Existing filters produced no matches | Change or clear filters |
| Ready | Current data, scope and allowed actions | The normal workflow |
| Editing/dirty | Local draft differs from saved state | Save/cancel under the domain's rules |
| Submitting | Request is pending, outcome not yet known | Avoid duplicate submission; preserve draft |
| Success | Authoritative operation completed | Show resulting state and the next useful action |
| Validation failure | Specific input cannot be accepted | Correct the affected field without losing the rest |
| Network/server failure | Retrieval or mutation could not be confirmed | Retry/read current state; do not treat an uncertain mutation as definite failure |
| Permission/session failure | Current identity cannot perform the action | Use the supported sign-in/access path |
| Stale/conflicting data | Another revision superseded the current basis | Refresh, compare, then retry deliberately |

Do not reuse the same “No data” message for all these states. UI error wording should identify the failed operation and recovery without exposing secrets or dumping internal traces.

## Accessibility and content rules

This is a review contract, **not a claim of a completed accessibility audit**.

- Provide logical heading order, landmarks, labels and an accessible name for every control. Skip links must land at the main work area.
- Keep all meaningful operations available by keyboard; make focus visible and avoid trapping it outside an intentional modal.
- For new work, target at least 4.5:1 contrast for ordinary text and 3:1 for large text and meaningful control boundaries. Measure actual rendered combinations, including opacity, selection, hover and both themes.
- Target 44px touch hit areas for primary mobile controls. Dense desktop tools may be visually smaller but must remain usable; legacy sizes are not evidence of mobile compliance.
- Never rely only on color, hover, animation or an icon. Do not suppress focus outlines without an equivalent visible replacement.
- Support zoom, long content, system-font fallback and reduced motion. Keep horizontal scrolling scoped to content that needs it.
- Announce asynchronous outcomes appropriately without repeatedly interrupting assistive technology. Live regions and graph alternatives need feature-level verification; their completeness is not asserted here.
- Give charts/diagrams textual explanations. The canvas graph is not, by itself, a sufficient nonvisual explanation; preserve tree/source pathways and evaluate remaining gaps.

Use direct, concrete language. “Open sessions”, “Review checks”, “No projects match this filter” and “Could not copy; select the command manually” explain an action or state. Avoid “AI magic”, vague “Something went wrong”, unsupported enterprise guarantees, and “Completed” before checks have passed. Keep product-facing copy about the user's decision; implementation details belong in diagnostics or technical docs.

AST relationship evidence uses an explicit related entity type and the persistent identity declared by its schema. Renderer IDs are presentation-only. A relationship list is bounded evidence, not a synthesized graph. See [AST schema and traversal contracts](../ast_module.md).

### Domain select menus and empty readers

Use `StyledSelect` for all single-choice domain controls, including filters and modal forms. It uses the workspace Radix menu and semantic colors, supports empty-valued “All” options, disabled options, keyboard navigation and focus return. Keep a visible associated label or an accessible name. Do not use browser-native popup menus for these controls.

An empty catalogue shows one recovery message. Show “Select a record” only when the current catalogue has records and no record is open. Memory readers omit an initial Markdown heading only when it exactly repeats the displayed record title. Project Knowledge takes its scope from the common header; only the installed-context explorer offers a local context picker.


### Persisted record links

Task, Session, Memory and Knowledge readers show outgoing references and incoming backlinks from the relationship API. A text identifier is not evidence that a relationship exists. Links resolve exact typed identities in the active authorized scope. An unresolved target remains visible with its identity and status, without a clickable destination. Empty, loading, partial and failed relationship reads have distinct states; the common header refresh reloads this section with the page. The [record relationship contract](../record_relations.md) defines authoring and persistence.


Domain selects inside modals keep ownership of their portaled menu: the first Escape closes the select and returns focus to its trigger; a subsequent Escape closes the modal. Task/Session status filters and publication dependency types use the same select primitive. Knowledge's project route does not repeat a context picker; an explicitly imported context route retains its domain context picker.

### Markdown in records and search results

Task and session prose, memory bodies and Knowledge documents share the Markdown renderer. Keep identifiers, titles, dates and statuses literal. Search snippets preserve meaningful newlines and Unicode and use a compact presentation of the same renderer. Place rich previews outside the record-opening button so links and code controls remain independently usable. Knowledge previews receive the same document navigation callback as the full viewer; other domains do not infer persisted relationships from text. Editing, raw views, copying and exports retain source Markdown.

Artifact and project descriptions are authored prose and support Markdown. Render them consistently in catalogues, inspectors and publication reviews using `MarkdownContent`; use a `.markdown-preview` wrapper for compact contexts. Keep form textareas and submitted payloads as the unmodified source. Never place the rich body inside a record-selection button, and do not turn a description into an inferred wiki scope or persisted entity relationship.

| Content | Presentation contract | Surfaces |
| --- | --- | --- |
| Task and session specifications, progress, checks/evidence, comments and history | Shared Markdown renderer | Record dossiers, revisions, Workspace Now |
| Memory bodies, historical revisions and snippets | Shared Markdown renderer; compact snippets outside selection controls | Project and personal memory |
| Knowledge documents, search snippets and AI synthesis | Wiki viewer with document navigation; explicit raw view retains source | Knowledge and installed contexts |
| Live final answers and cited documents | Wiki viewer scoped to the investigation | Agent output and source tabs |
| Dream report body | Wiki viewer | Dream report reader |
| Artifact descriptions | Shared Markdown renderer, including publication preview | Hub catalogue, registry/project/imported inspectors, Upload and Submit reviews |
| Project descriptions | Shared Markdown renderer | Workspace context and ecosystem inspector |
| Titles, identifiers, statuses, tags, dates, commands, Cypher, source files, query rows, JSON and YAML | Literal or structured/code view | All domains, including Broker administration |
| CLI progress, public thinking fragments, stdout/stderr and opaque tool input/output | Literal diagnostics; no document or Markdown chunk contract is implied | Execution details and activity |

Use the producer's content contract, not a syntax-detection heuristic, to choose a viewer. New Markdown-producing fields must join this map and reuse the existing renderer. Preserve raw/edit/copy/export actions; a renderer does not repair malformed source prose.

Author specifications and evidence as readable source: retain spaces between words, use paragraphs and headings for distinct requirements, lists for parallel conditions, and code formatting for identifiers or commands. The renderer preserves the authored meaning; it must not invent structure or reconstruct words from compressed prose. When a record looks malformed, compare its persisted source with the rendered DOM before changing the shared component.

### Agent execution and source reading

Knowledge answers, Live investigations and AI Cypher drafts use the same execution component. Lead with one current status, replacing the preceding label as preparation or tool activity advances. Preparation/status messages keep their explicit text (for example, “Creating ephemeral project” then “Installing the Framework’s skills”). Thinking may arrive in fragments: show “Processing” and retain that public text in the details. Animate three trailing dots only while running; disable animation for `prefers-reduced-motion`. Announce label changes with a polite, atomic status region, excluding the decorative dots.

Keep execution details collapsed by default. Show public progress, tool activity, stdout diagnostics and stderr there; this is observable execution, not a promise to expose hidden model reasoning. Empty events never create “Working” rows. The compact panel retains up to 300 recent events and 12,000 characters per diagnostic; Live’s Execution activity keeps its full available log. Transport and persistence must not truncate tool payloads for presentation. Keep the complete answer in the document viewer. Callers supply actual completion, failure and cancellation outcomes; stopping a spinner alone does not prove success. Knowledge and Cypher offer Stop generation; changing scope aborts the request and invalidates stale output. Live uses its session cancellation action and continues a run across a disconnected subscriber.

In Execution activity, each tool call and its result share one timeline item with separate Input and Result disclosures. Group by native `tool_call_id` within a turn; repeated snapshots update the same item. Same-name concurrent calls and reused IDs in different turns remain separate. An empty result is still a received result. Label missing calls/results honestly. For legacy logs without IDs, pair only a named result with exactly one pending unidentified call of the same name in the complete turn; leave ambiguous matches separate. Never infer legacy pairs from a truncated buffer or use an event sequence/session ID as a call identity.

Render Knowledge and Live answers with the wiki Markdown viewer. A Live citation opens a source reader inside the investigation, with the installed context and artifact version visible. Resolve only sources installed in that investigation; ambiguous unqualified slugs require an explicit source choice. Missing or no-longer-authorized sources show an error instead of substituting a document from the current project.

Independent assistant messages remain separate paragraphs in Agent output. The stream adapter supplies paragraph boundaries where the CLI protocol identifies a new message; fragments within that message concatenate unchanged. The viewer must not insert a paragraph per network chunk or infer boundaries from punctuation. Persist the same normalized text used during streaming so replay keeps the layout. Older logs without boundary information retain their original text.

In Live, source readers are tabs beside Agent output and Execution activity, never modal overlays. Citation labels show the page name without the artifact qualifier; preserve explicit author labels and the complete destination. Keep context, artifact and version in the reader, with the full citation available on hover. Reuse a tab for the same resolved context and page; identical titles in different contexts remain distinct. Nested links open another source tab within the originating context. Each source tab has a visible × close button beside its title, including inactive and loading tabs; fixed answer/activity tabs have no close action. Use sibling buttons for selection and closing, never nested buttons, with an accessible close label and visible keyboard focus. Closing the active source returns to the answer; closing an inactive source preserves selection. Restore focus to the selected tab, abort its pending request and ignore late responses. Loading, source choice, errors and retry stay inside the reader.

Keep answer, activity and open reader panels mounted when switching evidence tabs, with independent scroll positions. Streaming updates must not pull a reader back to the answer. Abort closed readers and all outstanding source requests on investigation changes; late responses must not reopen tabs. Long tab titles truncate with a full-title tooltip, and the tab row scrolls horizontally on narrow screens. Preserve the shared keyboard tab navigation and visible focus treatment.
