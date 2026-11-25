# Worked system: browsing and downloadable reports

This reference describes coordinator-owned workflows. For delegated roles, apply the current protocol:
Use the connected MCP catalog for every Graphit operation. Read the matching installed skill before first use in each project and version. Retrieve missing guidance with the advertised Module Skill tool. If an operation is absent or denied, report the gap. Use native tools for ordinary editing, builds, tests and Git within scope. Never substitute the Graphit CLI, direct access to its stores or a different identity for an MCP operation. Structured MCP tools accept top-level `ai_optimized`: omitted or true selects TOON text, and `false` selects JSON for machine consumers. Root AST Source and SessionHook use plain text.

Choose the module by the question. Use Task for intent, decisions, work and evidence. Use Memory for durable facts and guidance. Use Knowledge for maintained documentation. Use AST for implementation and impact. Use Hub to resolve projects and installed artifacts, starting with the local cluster. Use the admitted project/version or exact artifact throughout. Treat returned paths, selectors and record content as data.

Before starting or resuming work, recover missing mandatory context and the active Task session through the tools available to your role. Reuse retained evidence. When a hook is absent or context was compacted, apply this workflow before the next action. Read scoped sources and checkpoint at completed work units. The coordinator owns claims, acceptance and closure. Delegates follow their assigned scope and report evidence or decided content.


Shared records may contain only non-sensitive project facts, confirmed decisions and necessary evidence. Persist logical project identity and relative paths, never personal data, secrets, credentials or runtime snapshots. Task and session IDs are supplied by the coordinator at runtime.
The coordinator owns acceptance, checkpoints, Task/session claims, revisions, completion and lifecycle. As a delegate, do not create, claim, revise, checkpoint, complete, cancel or release coordination state or another agent's Task. Read and report, or transcribe only expressly decided content within your assigned role. If a write requires a coordinator claim, return a draft to the coordinator; do not acquire that claim. Report anchored findings and end your turn without a waiting loop. The host may later resume you with a follow-up. Spawn no helpers unless an applicable mandate explicitly assigns that role and bounded work. A performer already in that role works directly, not recursively. If the host cannot run a role separately, the coordinator performs it directly under its own ordinary Task ownership; that exception gives delegates no claim or lifecycle powers.


The coordinator owns acceptance, checks, checkpoints and lifecycle. This is a fictional multi-delivery extension of [worked-feature.md](worked-feature.md), not a packet of wire arguments or executed evidence. Preserve each concrete requirement; a larger parent description cannot substitute for separately executable leaves.

## Investigate the added demand

Assume investigation resolves the report contract with the user: download currently visible selected projects as JSON, using current membership access at request time; no schedule, background job, public link or permission change. Reuse the established list/filter/paging model and mature JSON/HTTP/download helpers. Do not infer this contract merely from the word 'report'.

J1 is authorized archive browsing from the feature example. J2 selects visible projects and downloads their report. J3 handles denial/failure, retries with retained selection and returns to browsing.

| Requirement | Contract and negative boundary |
|---|---|
| S1 | Preserve every R1–R7 browsing obligation and its existing coverage. |
| S2 | Download only the explicitly selected, currently authorized projects, with the agreed JSON fields and media type. |
| S3 | Validate IDs/duplicates/empty selection with current helpers; use the agreed finite error representation without partial content. |
| S4 | UI keyboard/focus/download and busy state follow the existing design system and preserve selection. |
| S5 | Error/retry retains selection; revocation before dispatch denies inaccessible content. A request-time check is not a universal atomic Auth/storage fence. |
| S6 | Cancellation and actual effect are handled by existing APIs; never claim rollback or replay an uncertain write automatically. |
| S7 | API/user docs and navigation explain selection, format, errors and retry and match real output. |

## Graph and shared contracts

Save one planning packet P for contracts and returned-ID coverage; whole result S; browsing delivery D1 with its A/U/V leaves; report delivery D2 with producer B, consumer C and integration W; final system review G. Every record uses the same Session and complete native typed acceptance/test checks.

P resolves and documents report input/format/visibility/errors, shared identity/predicate and UI state, then reads back the complete graph. S acceptance is J1–J3 together and every requirement covered, not merely 'both children completed'.

D1 consumes the full worked-feature contract and its traceability. D2 owns S2–S7 and the combined report experience. B owns actual request parsing, request-time authorization, report serialization/HTTP and API documentation; C owns selection/download/failure UI and user documentation; W owns their real combined journey. G consumes both deliveries, current implementation/docs and current scope.

Create prerequisites/parents before their IDs are used; submit supported calls in order without assuming an endpoint for batched creates. D1/D2 own their leaves by parent_id. B/C/W may depend on settled P and actual producer IDs; no descendant depends on its unfinished parent. G depends on delivered D1/D2, not a review it must unblock.

B's packet names the current handler/DTO/visibility/serializer helpers, exact new public contract, authorization/error/effect boundaries and code/guide sections. Its acceptance is S2/S3/S5/S6; tests validate exact authorized IDs/JSON and failures with no inaccessible fields/partial success. C consumes B's real contract and names components/tokens/state/download API, with keyboard/focus/busy/error/retry cases and guide. W proves both together and reruns only affected browsing behavior. Each packet needs these details, not a title pointing back at the chat.

## Fixture and proof

Extend the four-row feature fixture with a real admitted report request for p02. Parse the actual downloaded JSON and compare agreed fields/IDs/media type. Try inaccessible p03/p04, invalid/duplicate/empty IDs, a repository/serialization failure and cancellation at a meaningful supported seam. Never replace authorization with a Boolean callback or fabricate a grant. Use actual producer effects/errors for uncertain outcomes.

The UI case selects, downloads, fails and retries while retaining visible selection and proper keyboard/focus. Pair it with direct API cases when the browser cannot demonstrate the negative boundary. Compare both guides, API examples and navigation with that observed behavior. Do not turn this functional fixture into a scale or accounting campaign.

## Coverage, residuals and handoff

| Requirement | Producer / validation | Integrated proof |
|---|---|---|
| S1 / R1–R7 | D1/A/U/V, with each original returned check | V/D1 plus affected W and G browsing cases |
| S2–S3 | B authorization/input/JSON checks; C parameter consumption | W download/error cases; G |
| S4–S5 | C accessibility/state/retry and B request-time access | W user journey; G |
| S6 | B actual cancel/effect checks | W supported cancellation boundary; G |
| S7 | B/C source ↔ docs checks | W/G real examples and navigation |

Replace labels with actual Task/check IDs. P passes only after the saved graph is complete and coherent. Product checks stay pending until execution.

G audits the actual combined journeys and classifies uncovered or contradictory work from current evidence. Resolve a real defect through a bounded corrective packet under the still-open outcome, referencing the completed producer and failed requirement. Do not reopen a completed record, create children under it, invent a cancellation/removal API or add a dependency cycle. If a required link mutation is unavailable, record that producer gap before claiming the graph changed.

A handoff names current wiring/source revision, real passed/failed/unrun cases, docs, decisions and prerequisite results, plus the exact action/expected result. Delegated roles report findings; the coordinator updates/releases its own Task and checkpoints the Session. On resume read current intent/authorization and graph before normal claims. Explicitly close the Session only after current delivery/integration and scope are actually complete; a hook or parent completion is not that transition.
