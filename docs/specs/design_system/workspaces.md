# Workspaces and page contracts

[Design system](../design_system.md) · [Foundations](foundations.md) · [Patterns](patterns.md) · [Evolution](evolution.md)

Audience: product designers, frontend contributors and agents extending Graphit. This reference describes the complete internal-page reconstruction accompanying the 2026-09-20 baseline. Use the route matrix to choose a composition from the engineering job, then use the shared primitives. It is not a deployment record or a substitute for domain API specifications.

## One identity, different decisions

The product connects **context → execution → evidence**. A solo developer and an enterprise team use the same engineering model: explicit scope, preserved decisions, ownership and checks around model output. Code supports investigation and continuity. Broker governs identity and access. The public site explains this model and adoption. Shared navy structure, cobalt actions, typography, bracket mark and state semantics connect the surfaces; their page compositions follow the work each user needs to do.

The internal rebuild replaces the former generic explorer compositions. A graph is now an optional investigation tool; a document has a reading workspace; a task is an evidence dossier; a grant is an explicit policy review. Do not use the previous layout or old screenshots as a template for future pages.

## Code route and journey matrix

Source of route truth: [App.tsx](../../../internal/ui/src/App.tsx). Source paths below are relative to `internal/ui/src/components/`. Parameterized routes share the same composition with scoped data, not a second design.

| Route | User's question and new composition | Actions and evidence to preserve | Owner |
| --- | --- | --- | --- |
| `/workspace` | What work should I do in this project? Intent tabs and project metadata | Continue work, understand, reuse/share, operate; actual IDs/paths/cluster labels; global context in the shared header | `system/WorkspacePage.tsx` |
| `/task/explorer`, `/:taskId` | What is this delivery's contract and why can it complete? Full-width catalogue → evidence dossier → accountability | Bounded search/status/pagination; next step; active checks/evidence; specification, comments, lifecycle, immutable revisions; relations; exact JSON exports | `task/TaskExplorerPage.tsx` |
| `/task/sessions`, `/:sessionId` | What was requested and how do I continue? Catalogue → request/strategy brief → checkpoints and linked work | Search/status/active-only/pagination; ownership, next step, decisions/problems; linked tasks; revisions, lifecycle and complete record | `task/SessionExplorerPage.tsx` |
| `/ast/explorer`, `/:contextId` | Where is the implementation and what does it affect? Find & inspect / Query lab → Relationship map results → source and typed relations | Scoped search; source navigation/copy; incoming/outgoing/two-hop potential impact; related type selection; editable AI query draft then explicit run; directory/file/language/configured-cluster grouping; directed boundary and entity neighborhoods; explicit source/impact action | `ast/ExplorerPage.tsx`, `QueryBar.tsx`, `CodePanel.tsx`, `RelationshipExplorer.tsx` |
| `/ast/contexts` | Which indexed source am I investigating? Context directory with origin and size | Project/import distinction; open investigation; refresh; confirmed project-scoped unlink of imported contexts | `ast/ContextsPage.tsx` |
| `/knowledge/explorer`, `/:moduleId`; `/wiki/explorer` | What maintained knowledge supports this decision? Library → focused reading or search results | Context and type filters; keyword/AI answer with source links; source/provenance, references/backlinks; reading history, raw Markdown/copy, image preview; refresh guarded against old context responses | `wiki/WikiExplorerPage.tsx`, `WikiMarkdown.tsx` |
| `/knowledge/contexts` | Which knowledge collection should I read? Source directory | Project/import origin and page count; open, refresh and confirmed unlink where supported | `wiki/WikiContextsPage.tsx` |
| `/memory/explorer`, `/:scopeId`, `/:scopeId/:memoryId` | What durable guidance applies and how has it changed? Record catalogue → revision dossier → curation form | Project/personal scope; search/type/tag/mandatory filters; create/edit/delete confirmation; revision selection and provenance; explicit current-versus-historical editing | `memory/MemoryExplorerPage.tsx` |
| `/hub/registry` | What reusable context can I trust and install? Filtered registry → artifact inspector | Type/publisher/search; identity/version/origin/dependencies; install with optional or required alias, remove and publish when applicable; authorization gates | `hub/RegistryPage.tsx`, `ArtifactCard.tsx` |
| `/hub/local` | What context does this project own or consume? Owned/Installed/Linked catalogues → origin-specific inspector | Publish/update/unpublish own artifacts; update/remove installations; unlink local context; metadata/dependency review before publication | `hub/ProjectArtifactsPage.tsx`, `hub/modals/SubmitModal.tsx` |
| `/hub/upload` | What contract am I publishing for reuse? Package → Metadata & dependencies → Review & publish | Type-specific package extension; virtual power packages; identity, version, author, tags, dependencies; selected project/agent and explicit final submit | `hub/UploadPage.tsx` |
| `/live` | What evidence boundary should an agent investigate? Prepare context → Run & evidence → Recent sessions | Compatible artifact selection, brief, start/follow-up/cancel; streamed output/activity and source tabs with concise citations; reconnect/dedup; reopen/remove sessions; ephemeral scope | `live/LiveSearchPage.tsx`, `live/LiveEvidence.tsx` |
| `/system/ecosystem` | Which project identity is relevant, and should its working context come from Workspace or Hub? Unified directory → identity/presence/cluster dossier | All/Same Cluster filters; Workspace/Hub presence; use local or remote context; inspect/copy path; local label add/remove and registration removal confirmation | `system/EcosystemDashboard.tsx` |
| `/system/daemon` | Is the background service available and how do I connect? Runtime band → logs and process details → agent connection and control | Poll/refresh; actual endpoint/key fields; copy where available; explicit stop confirmation and restart guidance | `daemon/DaemonDashboard.tsx` |
| `/system/dream` | Is Dream enabled or idle, and what happened in its latest run? State → latest run → conditions | Scoped polling/refresh, state/config, latest operational run; Memory holds the semantic result. No invented quality metric or narrative output | `dream/DreamDashboard.tsx` |

`/wiki` is a compatibility redirect to Live or Knowledge according to the existing agent feature gate. The default redirect opens AST contexts in AST app mode and Workspace otherwise. Live remains feature-gated. Preserve these routing contracts when evolving navigation.

### Investigation boundaries

Search results, source, relationship lists, table queries and graph samples have different meanings. A renderer node ID is not a persistent entity identity. Related queries use the scoped schema's declared identity and properties, including path identities for File/Directory, and a selected endpoint type. Missing metadata, missing identity and ambiguous symbol resolution must remain explicit. A 100-result relationship list or bounded graph sample is not a complete repository view. See [AST contracts](../ast_module.md).

An AI answer or generated query is not authoritative source. Show the source separately, retain its location/provenance, and keep generation distinct from execution. Likewise, a passed check must have recorded evidence; superseded checks remain inspectable but do not count toward active acceptance progress.

## Broker journey matrix

All administration workspaces live at `/admin` and use permission-filtered local sections. Their owner is Graphit Broker `internal/broker/adminui/index.html`; standalone sign-in/device branches belong to `internal/broker/oauthui/index.html`. The [administration guide](https://github.com/graphit-labs/graphit-broker/blob/main/docs/administration.md) owns operational details.

| Workspace | Decision and composition | Preserved boundary |
| --- | --- | --- |
| Project access | Authorized directory → capability dossier → connection commands | Exact IDs and all-project scope are distinct; Hub owns project metadata; refreshed-away selection clears the dossier |
| Resource grants | Searchable policy → read-only inspection → Audience / Resources / Storage / Review composer | Stable grant ID, additive policy, explicit all-project meaning, S3 constraints, CSRF/ETag and retained conflict draft; no simulated effective access |
| Local identities | People/service/state directory → stable identity / membership / authentication profile → dedicated editor | Subject and kind immutable; backend last-admin protection; password/MFA rules remain authoritative |
| Service credentials | Selected service → credential inventory → one-time secret after confirmed creation | Revoke confirmation; scopes are metadata; clear secret on dismissal/scope change; delayed responses cannot attach it to another service |
| Role definitions | Role directory → allowed-action inspector → grouped permission editor | Built-ins read only, custom name immutable while editing, supported actions come from server |
| Identity assignments | Canonical subject and explicit role → assignment inventory | IdP claims and bootstrap rules remain visible; admin permissions do not imply resource grants |
| Configuration | Section index and search → complete redacted YAML reader | Read only; match count/first location, refresh/copy; change deployment configuration through its documented source |
| Admin sign-in | Current local challenge or configured provider choice | Password replacement, MFA and recovery replace initial choices while a challenge is active |
| OAuth/device | Brand bar → Identify / Verify / Connect context → one server-selected transaction | Password/MFA enrollment/MFA/recovery/device/approved/fatal branches; exact form names, same request URL, challenge fields, provider redirects and CSP |

Write controls are distinct from read visibility. The server authorizes every request. A disabled or hidden button never grants security. Session changes clear prior inspectors; table overflow stays inside the table region. Authentication is light-only; administration supports light/dark.

## Shared Code primitives

Implementation: [EngineeringUI.tsx](../../../internal/ui/src/components/shared/EngineeringUI.tsx) and [engineering.css](../../../internal/ui/src/components/shared/engineering.css). They are local React primitives, not a separately versioned package.

| Primitive | Inputs and responsibility | Usage rule |
| --- | --- | --- |
| `WorkPage` | `children`, optional `className`; scrollable scoped work surface | One per page; let the shell own global navigation |
| `WorkHeader` | `title`, `description`, optional `context` and `actions` | State the job first; `context` is local domain/record context, never global Project/Agent selectors |
| `WorkSection` | `title`, optional `description`, `actions`, `id`, `className`; children | Group one decision or evidence region; stable IDs support dossier navigation |
| `WorkSearch` | Controlled `value`, `onChange`, accessible `label`, optional `placeholder` | Already renders a label; do not nest it inside another label |
| `WorkTabs` | Controlled `value`, `onChange`, `[id,title]` pairs and group label | Peer views; selected/roving focus with arrows/Home/End. Caller owns content and state retention |
| `FactList` | `[label, ReactNode]` pairs | Definition list; null/empty becomes an em dash, zero remains zero |
| `RecordLink` | Title, metadata, click action, optional selected state/children | Select an inspectable record; actual destinations should use links where appropriate |
| `WorkNotice` | Title, optional content; neutral/error/success tone | Meaningful context or result; error uses alert role. It does not invent async announcement for other tones |
| `WorkEmpty` | Title, explanation, optional recovery action | Explain this region's absence; distinguish filtering from retrieval failure |
| `SessionTaskProgress` | Completed and total linked-Task counts; explicit text, semantic progressbar and zero state | Use for session progress in catalogues and NOW. Text is authoritative, the bar is complementary when total is positive, and zero renders `No tasks` without a progressbar |

Example composition:

```tsx
<WorkPage>
  <WorkHeader title="Review delivery" description="Inspect the contract and its evidence." />
  <WorkSection title="Acceptance evidence" id="evidence">
    <FactList items={[["Revision", task.revision], ["Owner", task.owner]]} />
    {/* Render authoritative checks and evidence here. */}
  </WorkSection>
</WorkPage>
```

Use `.work-button` with `primary` or `danger` for the intended action. `.work-toolbar`, `.work-table-wrap`, `.work-table`, `.work-panel`, `.work-field`, `.work-form` and `.work-form-grid` provide consistent local structure. `.work-dossier`, `.dossier-nav`, `.dossier-layout`, `.dossier-content` and `.dossier-context` serve durable records. Use the appropriate domain grid rather than forcing every screen into a dossier.

Dialogs combine `ModalPortal` behavior with `.work-dialog` presentation. Header and footer remain outside the scrolling body. `ConfirmModal` accepts an explicit title, target message, consequence, action label and cancellation; choose domain-specific wording rather than the default irreversible warning for reversible actions. `AliasModal` names local installation context. `SubmitModal` preserves metadata/dependencies and requires a review step. Forms reset when their owning project/agent workspace changes.

## Async scope and interaction rules

These are extension requirements, with implementations and regression coverage in the affected pages:

1. Bind requests and selections to project/context/agent or authenticated identity as applicable. Invalidate old request generations when that boundary changes.
2. Guard both the response and any follow-up work after an `await`. A rejected old reload must not initiate a new page navigation or reset current global metadata.
3. Capture the intended target before a mutation. Close incompatible dialogs when scope changes. Successful old operations may not reload an old workspace into the current one.
4. Keep inspection, draft, review, submission and confirmed result distinct. Preserve drafts on validation or revision conflict; explicit reset/cancel follows the domain contract.
5. Move focus to newly opened reading/editing work when appropriate; restore it after dialogs. Do not scroll with guessed absolute coordinates.
6. Keep zero, unavailable and empty distinct. Never replace a zero count with a dash or a loading failure with a claim of no records.

## Verification by journey

Future changes should name the route, data condition, action and observable outcome in their delivery record. Relevant baselines include Task/Session catalogue/export tests; AST identity/query/source tests plus canonical schema Go tests; Wiki context/refresh/AI provenance tests; Memory revision/curation tests; Hub scoped-intent/stale-response tests; Live transport tests; and Broker handler/template tests.

Inspect desktop and narrow screens for the changed job, including long identifiers, reading content, horizontal tables, modal body/footer and navigation. Exercise permitted and read-only modes where relevant. Use safe fixtures for visual work and distinguish those observations from real mutation/authentication tests. Record actual build/test/browser results in Task, not as permanent claims that every future version has been tested.

## Global context ownership

Every Code route inherits exactly one Project/Agent group from `AppShell` → `WorkspaceSelectors`. Keep it at the right of the common header, including mobile. Page headers and navigation must not mount another global picker or selected-project badge. Themed Radix menus retain accessible labels, keyboard selection, Escape/focus return and empty/loading states. The single header refresh combines the context catalogue with all active-page loaders. Domain source/scope filters remain local; project metadata may appear in a dossier when it provides additional information.

The Project menu treats Workspace and Hub as two working-context origins for the same immutable
identity. Group options by origin, show a checkout path only for Workspace, and show “Hub” plus the
project ID for remote targets. Persist the typed target, not just a path. When the Hub source is
temporarily unavailable, mark a retained remote target stale and keep it selected; never substitute
the last Workspace path. Operations > Projects merges both sources into one row per ID and puts the
origin choice in the detail action, keeping identity comparison separate from execution scope.

Task, Session and project Memory send the Hub project ID directly. Knowledge and AST add two local
selectors — artifact and version — and visibly show the resolved exact `id@version`; “Latest” must
never remain an ambiguous request value. Remote-compatible pages clear results on `activeProjectKey`
changes. Checkout-only pages use one shared unavailable composition with the selected Hub identity
and a global-selector next action, preserving the target instead of falling back to a local path.

## Combined refresh by route

The header always refreshes the project/agent catalogue once. `WorkspaceRefreshProvider` then awaits the active page's registered work without remounting it. Page loaders return promises; `refreshAll` waits for every source before propagating an error. Generic local Refresh buttons are prohibited.

| Page | Additional data refreshed |
| --- | --- |
| Workspace, Ecosystem | None: both derive the refreshed global project catalogue |
| Tasks / Sessions | Current filtered catalogue and selected record's complete detail |
| Memory | Filtered catalogue and selected revision trace; editing draft remains mounted |
| Knowledge / Wiki explorer | Collection directory, selected collection pages and open document; no new AI answer |
| Code / Knowledge contexts | Context directory; retained selected context is reconciled with the returned inventory |
| Hub registry / Project artifacts | Catalogue, installed/owned artifacts and resolved metadata; filters and inspection selection retained where available |
| Publication | None: preserve package, metadata and review step; refresh global destination catalogue only |
| Daemon | Current service status/log snapshot; automatic polling remains active |
| Dream | Conditions and latest-run metadata; polling remains active |
| Live | Available artifacts and recent sessions; current transcript, question and SSE subscription stay intact |
| AST investigation | Schema/context metadata, previously submitted search, open source, current relation evidence and last explicitly executed graph query/sample; never run an edited draft or generate AI output |

Refreshing is not a change of scope: keep filters, selection and drafts. A scope change invalidates the previous registration. Domain response guards remain mandatory because a request may already be in flight. A removed record/context should resolve to an empty inspection state rather than another record's detail.


## Column review across Code workspaces

All Code routes and their internal reading/editing compositions were reviewed against the information roles in [patterns](patterns.md#allocate-space-by-the-informations-role). Palette, typography, navigation and domain contracts remain shared; the amount of space follows the user's task.

| Surface | Decision | Reason |
| --- | --- | --- |
| Code contexts / Knowledge contexts | Keep detail-first `work-split`; adopt available-width stacking | Origin, scope and next action belong to the selected context |
| Hub registry / Project artifacts | Detail-first `artifact-directory`; wide identity facts use two columns | Inspection and maintenance need more space than selection |
| Projects | Balanced detail-first `ecosystem-layout` | The four-column identity directory needs comparison width; the dossier owns origin choice, selected-project metadata and cluster administration |
| Live preparation | Detail-first `live-preparation` | The right side is the investigation brief and execution composer |
| Publish: package, metadata, review | Content-first `publish-layout`, fluid support | The form remains primary; publication context stays readable |
| Task / Session / Project and Personal Memory dossiers | Content-first `dossier-layout`, fluid support | Evidence/request/guidance primary; accountability and provenance alongside; catalogues remain full width above |
| Memory editor | Content-first `work-editor-layout`, own modal breakpoint | Text editing primary; type, flags and metadata remain usable |
| Knowledge library / Wiki alias | Content-first `knowledge-directory` | Right side describes the collection; document opens in the Reading view |
| Knowledge reading / answers | Content-first `reader-body` / `knowledge-answer` | Preserve reading measure; give provenance and cited documents adequate space |
| AST Query lab | Content-first `query-lab-layout` | Query authoring primary, vocabulary alongside; execution opens results in Relationship map |
| Daemon / Dream | Content-first `operations-layout` | Daemon logs or latest-run metadata primary; conditions and controls alongside |
| Live run | Content-first `live-run-layout` | Transcript and output primary; run context alongside |
| Workspace | Content-first `workspace-launchpad` | Work entry points primary; project metadata alongside |
| AST symbol investigation | Keep bounded results rail; source/relation pane remains flexible | Source code needs long lines; a proportional wide search list would take space from the investigation |
| AST map, catalogue tables, equal-purpose paired sections, form grids, dependencies and runtime status | Keep domain-specific structure | These are not catalogue/detail or content/support pairs; preserve local table/code overflow and mobile reflow |

Verify both populated and empty inspectors, long IDs/paths, publication steps, source reading and the memory editor. Check at wide desktop, 1280px, 1024px and narrow mobile sizes, measuring the workspace container rather than assuming the viewport equals content width.

AST map entry loads the bounded sample once per scope when no explicit search or query is active. Search and Cypher execution select Relationship map immediately and show loading, result, empty or error there; never replace these with a sample. Place grouping, language, entity and relationship filters above a stable boundary catalogue and evidence reader. The reader fills the remaining workspace with its own scroll area; narrow layouts stack it below the catalogue. Relationship map keeps scalar rows in an expanded Query rows table and mixed rows in an expandable table below the graph. The source/impact inspector also stays in the map. Query lab preserves the editable draft. Explicit new results reset map filters; header refresh repeats the active result origin and selected neighborhood without resetting exploration. Entity selection queries direct incoming/outgoing relationships separately from the catalogue, including out-of-result neighbors. Following and Back query the selected entity; the original result and its filters remain unchanged. Show loading, failed branches, retry and paginated continuation in the reader.

### Workspace Now

The initial Now tab shows active sessions and tasks plus recently maintained project memories and Knowledge. Work is active only with `in_progress` status, a responsible identity and an unexpired lease. Each session record uses `SessionTaskProgress` to show the authoritative completed/total linked-Task count: cancelled Tasks remain in the total and only `completed` increments the numerator. The view includes all project contributors: the global agent selector identifies a CLI context, not the owner identity of a Task claim. Memory uses its recorded author identity; Knowledge does not invent an author. Personal memory is excluded.

Now refreshes every five seconds while mounted, shares its loader with the header refresh, and marks both the live request and response `no-store` so an unchanged project URL cannot reuse an older snapshot. It deduplicates in-flight requests and aborts/ignores responses after a project or agent change or navigation. Keep the previous snapshot visible while updating; identify stale data after a failed refresh and retry automatically. Individual source errors remain visible without hiding successful sections: a failed session-progress read does not suppress successfully loaded active Tasks. Each section shows at most the latest 20 records, sorted before limiting, with its total and links to the full record. Preserve the timestamp precision supplied by the source: old date-only Knowledge values remain dates.

### Live source selection

The header's selected agent determines both the prepared workspace adapter and the executable
used throughout the Live session. A missing CLI fails before workspace preparation and identifies
the required executable; do not silently substitute another installed agent. Every turn uses the
same managed investigation directory. Codex's explicit non-Git workspace option preserves this
directory without changing its sandbox or approval policy; see [AI engine](../ai_engine.md).

Live's evidence picker combines artifacts from the active registered project instance with all authorized pages of the Hub catalogue. Search matches identity, name, description, tags and project information; source and type filters refine that set. Origin stays visible beside each artifact and selected item. Identical names from a project and the Hub are distinct selections. A source failure leaves successful sources usable and shows the failed source explicitly. Project/agent changes clear selections and invalidate old catalogue responses; header refresh reconciles selections with the refreshed catalogue.

Local code/Knowledge contexts reuse existing indexes without publishing or compiling the source. Skills, agents, rules, commands and workflows are copied with their resources into the investigation before adapter setup. Other local types remain visible with an unavailable explanation. Package resource symlinks must be materialized before selection; they are not followed into unrelated files. Published references still use Hub authorization and installation. Each local selection is revalidated against its registered project instance and the selected agent during preparation, so a stale or removed source fails explicitly.
