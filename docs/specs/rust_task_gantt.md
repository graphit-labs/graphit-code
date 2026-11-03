---

title: Native Task calendar and record views
doc_type: spec
description: Authored Task dates, native working contexts and read-only record views in React.
references:
  - target: {type: task, id: tsk-fc44}
    relation: implements
  - target: {type: task, id: tsk-b6cd}
    relation: uses_api
  - target: {type: task, id: tsk-bf2d}
    relation: implements
  - target: {type: task, id: tsk-bfa7}
    relation: uses_api
  - target: {type: task, id: tsk-744f}
    relation: uses_api
  - target: {type: task, id: tsk-2447}
    relation: uses_api
  - target: {type: task, id: tsk-cb27}
    relation: implements
  - target: {type: task, id: tsk-17a7}
    relation: relates_to
  - target: {type: task, id: tsk-ef75}
    relation: uses_api
  - target: {type: task, id: tsk-51f2}
    relation: uses_api
  - target: {type: task, id: tsk-db07}
    relation: uses_api
---

# Native Task calendar and record views

Current path contract: PROJECT roots and relative paths cross JSON/API/CLI as literal Unicode strings. Serde preserves spaces, accents, escapes and separators without lossy conversion. Non-Unicode input fails explicitly; `{os,units}` is rejected. Historical evidence below describes the code that was exercised then and does not qualify the removed path codec.

The React Tasks, Gantt and Sessions tabs use the authenticated browser Task routes. The Task workspace first loads `POST /api/tasks/contexts`, which returns the retained native contexts and the current tuple. Project and Version selectors choose from that inventory. A catalogue validates the selected tuple through `POST /api/tasks/context`, then echoes the captured native path, project ID and resolved version in the JSON selection of each read. These values select data; the server checks its current cookie, local access JWT and fresh Broker PROJECT admission for every request. JavaScript receives no Runtime key, role grant or participant claim token.

The routes return only retained project/version tuples. A missing, ambiguous or unauthorized worktree is an error. The physical path is a literal Unicode JSON string: two directories with the same project ID and version remain separate selections. The client does not infer the server's OS, consult Git or load the unrelated Hub inventory for Task. An explicitly selected UI version is sent in JSON. Changing the physical selection, version, query, Task status, flag or dependency filter cancels stale catalogue work and prevents its response from replacing the current view.

Within the same mounted module, API origin and signed-in identity, navigation and
unrelated URL parameters preserve the discovered context inventory and selection.
Choosing a tuple already in that inventory updates the selection without another
discovery request. An unknown URL selection triggers fresh discovery before it
can be used; malformed selectors fail explicitly. This inventory is only selector
data: each subsequent reader still authorizes on the server. Module, API or
identity changes and unmount cancel the old work and clear its inventory; 401/403
also invalidate it. Explicit Refresh continues to reload discovery and readers.
Selecting another known tuple during refresh cancels the older request so its
late result cannot restore the previous selection.

The single Refresh action lives in the header's `WorkspaceRefreshProvider`; Gantt registers its catalogue loader and has no second generic toolbar Refresh. The header awaits the authorized context inventory and every registered reader for the current scope, keeping Busy until all settle and preserving drafts and selections. Local Retry is shown only for a causal read error; filters and Load more remain independent. An equivalent tuple returned as a new JSON object preserves the selected object. Explicit refresh cancels a pending automatic catalogue timer, so it cannot abort the read being awaited. Changing scope or unmounting cancels old work; late results and denials do not replace or invalidate the current view. Anonymous, denied, unavailable, empty and loading states remain distinct. See the [Foreground browser contract](rust_foreground_tasks.md).

## Calendar

Gantt displays only authored civil dates. A supplied start and end form an inclusive interval; a same-day interval lasts one calendar day. A start-only or end-only schedule is a labelled marker with no invented duration. Due dates are independent markers and may precede or follow the planned interval. Tasks without dates remain visible in a separate section.

Calendar geometry uses UTC and the existing D3 scale/day functions. The canonical date parser preserves years 0000 through 9999 and rejects normalization of invalid dates. Axis ticks remain adaptive across long ranges; the chart does not allocate one element per day. Invalid schedules are displayed as errors beside valid rows.

Header refresh rereads the current catalogue and updates these authored calendar
values; it does not edit dates, move dependencies or create a duration. Local
Retry recovers a displayed read failure, rather than adding another generic Refresh.

Dependencies show actual predecessor IDs. A predecessor outside the loaded pages is labelled as such; it is not declared missing. Dependencies never move dates or create durations. Task and dependency buttons use native keyboard activation and open the same Task dossier. The chart has its own horizontal scrolling surface and sticky labels; text dates and statuses remain available without relying on bar color.

## Records and paging

Task discovery uses the native lightweight catalogue and its opaque cursor object. Load more appends rows for the same physical selection and filter. The visible count describes loaded pages, not an assumed whole project. Gantt never requests full audit histories for each row.

Status labels stay on one line. The existing table wrapper provides local scrolling
when complete titles, IDs or owners need more width; no identity is truncated to
make the status fit. This uses the shared `WorkStatusBadge` and existing semantic
tokens, while generic metadata badges retain their wrapping behavior.

Task detail reads native `TaskDetail`: current specification, creation, ordered lifecycle snapshots and actual comments. It renders Markdown descriptions, progress, check evidence, revision reasons and comment bodies without rewriting the stored text. Current and historical specifications display their actual dates. Exact JSON export preserves the native detail. Explicit Export all tasks walks native catalogue pages and exact records; no full export is loaded for ordinary discovery.

Sessions uses complete lightweight `session-heads` summaries, then loads an exact native Session and its related Task catalogue only when selected. Search explicitly filters loaded titles/IDs, status and active state. It does not claim a search across audit history or fabricate a server cursor. Related tasks retain native pagination. Checkpoints, request/strategy revisions, ownership and lifecycle use the original Session event records.

The native Task catalogue combines title query, real status, Session ID, flagged and blocked filters before pagination. This browser catalogue title filter does not promise matches in IDs, descriptions, comments or audit history. The separate native CLI/Source lexical Task search covers current specifications and comments as described in [Native Task FTS](native_task_search.md). The flag control selects all, flagged or not flagged Tasks; the dependency control selects all, blocked open Tasks or their complement. `blocked` means `status == open && !ready`, so terminal and in-progress Tasks are not blocked solely because readiness is false. Nullable or omitted boolean filters impose no condition. Both boolean fields belong to the cursor filter: changing either rejects a previous cursor. The browser sends the complete filter and never filters only its loaded page. Flags and readiness remain visible in returned rows.

Record, lifecycle and ownership timestamps are UTC epoch milliseconds. Task and Session dossiers render the stored values directly with `new Date(milliseconds).toLocaleString()`, including creation and lease expiry; they do not multiply the values or infer units. Authored calendar dates remain separate civil-date values.

## Verification

Affected checks cover Unicode JSON selection and literal path/cursor roundtrips, denied/ambiguous reads, paging, cancellation of stale requests, keyboard navigation, records and explicit exports. Existing calendar/renderer guards cover leap days, month/year boundaries, same-day intervals, partial and due-only schedules, invalid rows, year limits and dependencies outside loaded pages. The date suite runs with UTC, America/Sao_Paulo and America/New_York.

The native integration starts the real BrowserAuth/TaskServer fixture, creates records and a comment under actual Runtime OwnTask admission, closes and reopens Lance, authenticates the cookie and performs fresh PROJECT reads. While that server remains alive, the React/Vitest consumer uses its actual HTTP sockets to read Gantt, Task detail/comments and Session. Test-only Origin/cookie forwarding supplies the browser boundary in the DOM runner; no response is mocked. This qualifies the component/data path, not deployment of the application's complete UI server.

The earlier native-to-React receipts predate the retained-context bootstrap. The current fixture mounts that bootstrap and examines the actual Export all tasks JSON Blob against both native HTTP records, including selection, schedule, dependencies, comments and lifecycle. It restores download and fetch hooks, unmounts the readers and cancels the context request. Task cb27 qualifies this current path with one Rust fixture and one React test passing together; its receipt is `.build/rust-task-gantt-current-integration/qualification.json`. The earlier compilation failure remains recorded beside that receipt. The two-record fixture does not establish export across more than one catalogue page; native cursor and component pagination checks cover those bounds separately. Component tests and static assets do not qualify a hosted IAB journey. That journey has separate historical observations in Task17a7 below; native FullNoLoss reference delivery has since been accepted separately, as reconciled below.

The historical hosted Task journey is recorded in `tsk-17a7` revision 39. Root
CUA observed it against
the compiled Foreground and shipped Vite assets: signed login, exact native
context, authored leap-day dates in 2028, Task dossier, linked Session, scope change and
deep reload. At 720px, status labels remain on one line while the complete owner
stays in the table's local horizontal scroll; header navigation remains usable.
The historical compiled badge probe passed and recorded host join, discovery
cleanup and zero UserInfo calls. Its observations remain in the Task history;
the former local receipts `17a7-root-browser-observations.json` and
`17a7-badge-candidate.json` are absent from the current checkout. These records
are historical proof, not a new browser execution. That hosted journey did not
qualify native FullNoLoss references. Their later ef75/51f2 acceptance is
separate; WEB-A6/T5 integrated acceptance belongs to Task17a7.

In the `tsk-17a7` revision-42 checkpoint, before the single-header delta, Source/Tracker review was
NOFAULT. All 58 focused React tests in six files passed (7.78 s harness,
8.133 s wall). The real build helper passed in 13.145 s, with all three shipped
assets byte-equal to its outputs; lint of six affected files passed in 10.686 s.
These component/build checks do not establish a current browser journey or
transfer the historical hosted observations to the current build.

The delta44 Source/Tracker readiness review was NOFAULT. One meaningful Gantt
test adds to the original 58-case inventory, making 59 distinct cases, not a sum
of reruns. The 15 focal Gantt/Explorer cases passed (7.13 s harness, 7.757 s wall).
The added case mounts the real header providers, activates the unique Refresh
with the keyboard, returns a cloned NativePath and defers the catalogue. It
checks that the 260 ms timer does not abort that read, Busy remains until settle,
and the authored calendar changes from three inclusive days to one. This is a
component fixture with mocked Task API, not a new browser journey.
Refresh build01 failed on TS2769 in its own test query option; the query was
corrected to `name: /^Refresh$/`. Buildrefresh02 passed in 10.561 s; the latest
five Gantt cases passed in final02 (4.47 s harness, 4.772 s wall), and scoped lint
of two files passed in 0.754 s. These reruns do not add to the 59 React cases.

The historical locked/offline host01 attempt failed before its harness after 0.062 s:
the Knowledge manifest's `graphrs =0.12.0` pin was absent from Lock/cache. It
provides no current browser proof. The pin issue was subsequently resolved by
Root, but host02 failed on six external Records compile errors before its
harness (15.908 s wall). Both failures remain historical without browser proof.

Host03 then passed one compiled-host/browser case with zero ignored: build
44.53 s, harness 376.71 s and wall 421.400 s, including the interactive hold.
IAB went from anonymous to signed local Broker OAuth, then read the exact native
main context and persisted/reopened Task data: two Tasks, an inclusive leap-day
interval from February 28 to March 1 with February 29 visible, due March 2,
undated data and actual dependencies. Keyboard navigation opened Task dossier,
checks/lifecycle and Session request/strategy with two linked Tasks. Switching to the other
PROJECT removed the previous content and showed its empty state; restoring the
selection and deep reload preserved the exact Session selector.

The current browser showed one Gantt Refresh. Keyboard activation made it
disabled/busy with Loading tasks, then reenabled it with the dates returned.
At 720×900, calendar overflow stayed local while the header and filters remained
usable; Escape restored focus to Project. The fixture restored the viewport,
logged out to anonymous with no previous Task rows, and closed the tab.
Default/narrow screenshots were observed through CUA, not saved as file artifacts.
The historical `browser-observations.json` and `browser03.result.json` under
`.build/rust-task-gantt-owner/` recorded
`finished`, `host_joined` and `discovery_clean` as true, zero UserInfo calls and
48 PROJECT calls. These local receipts were removed during the targeted
`.build` cleanup; the Task retains their historical observations. They are not
recreated or presented as current downloadable evidence.

The reused historical distinct total is 62: 59 React cases, this one hosted case and two
static unit cases. Only one test was added to the prior React inventory; reruns
do not increase the total. Staticunit02 passed both cases with zero ignored
(0.05 s harness, 56.299 s wall). Static01 failed before its harness on seven
foreign Role tuple E0308 errors (369.968 s wall, including queue time); the
external correction preceded staticunit02.

Strict03 passed the current affected App check in 17.803 s wall, with no test
harness:

```text
cargo xtask cargo clippy --locked --offline -p graphit-app --lib --test foreground_tasks --no-deps -j4 -- -D warnings
```

That historical qualification has 62 distinct cases, zero ignored and one added test; no new Gantt execution or rerun sum is claimed.
Strict01 remains historical: a foreign AST constructor call with eight arguments
failed (14.503 s wall). Strict02 failed on external Records TaskRefs cleanup errors:
undefined budget and extra arguments at `references.rs:597/1188` (77.724 s wall).
The external AST/Role/Records corrections preceded strict03; this unit did not
edit those modules or add lint allowances. Failed attempts do not count as
passed tests. All 16 attempt logs, including six historical failures, remain
part of the qualification record. The owned UI change remains one Gantt hunk,
one test and the three shipped assets. Reference links visibly
returned HTTP404 in that snapshot; the later ef75/51f2 acceptance does not erase
this failure. Those independent UI proofs did not qualify FullRefs and do not
complete integration17a7, commercial authentication or Windows/macOS.

## Current integrated acceptance reconciliation

Task17a7 brings together integrated acceptance and the Session back fix; the
qualified results and limits are described below. Reference backend ef75 is
COMPLETED65 with 15 checks
PASS and 32 distinct tests in 33 executions. React reference delivery51f2 is
COMPLETED79 with 15 checks PASS, 60 units using mocked transport and accepted
combined Browser coverage: main10 passed 19 stages but its final status remains
failed; focal12 passed four narrow/logout stages, and expiry02 passed three
stages with a real 30-second session. Failed10/11 are preserved. This is not
one complete passing Browser run. Reader delivery db07 is COMPLETED187 with
28 checks PASS through explicit evidence layers. Suites and reruns are not
added together. The Gantt62 inventory above is reused without a new execution.

See the [reference contract](rust_foreground_record_references.md).

SessionExplorerPage.tsx:71–75/118 now retains the validated literal selection
in catalogueHref for Sessions back and physical scope changes. The new case
in SessionExplorerPage.test.tsx:86–120 mounts the real TaskWorkspaceBootstrap
with mocked transport. It switches between NativePaths with the same ID and
version, including byte255, then checks Session→Sessions, literal URI, a fresh
workspace/remount, Mirror heads/detail and an explicit physical switch. RED
previously observed null selection; the final Session suite passed seven cases
in 3.84 s (1.244 s tests), with scoped ESLint exit0. This is unit evidence,
not a new Browser journey. CSS, design, tokens, backend and the Auth contract
are unchanged by this fix.

The authorized final `node scripts/build-rust-ui.mjs` passed `tsc -b`, Vite
with 3,793 modules in 4.89 s and copying of all three assets. The current index
is 2,612,971 bytes, SHA256
`ea32c572488c42a79f9e4d7b87433ddaa4f8ac26aa6d748ac061c2e53c3b319a`.
Logo and favicon are 249 bytes each, SHA256
`537271954e96afaff8307be5c272579e88f45ecc0ac11c7199b87d30e9cd1233`.
All three shipped assets are byte-equal to dist. This qualifies TypeScript,
build and bytes for the Session correction; it does not assert a new HTTP
execution, a Rust binary recompiled with these assets or a new Browser run.
The qualified hosting mechanism is reused. The previous db07 index of
2,612,921 bytes/SHA9678 remains historical. Task17a7 maintains authority over
its 13 checks and lifecycle; this layered qualification does not complete the
end-to-end validation, ses-c98a or other grouping tasks.
