# Specify executable work before implementation

This reference describes coordinator-owned workflows. For delegated roles, apply the current protocol:
{% include "protocol.md" %}

The coordinator owns acceptance, checks, checkpoints and lifecycle. Read [session.md](session.md) for the evolving demand. This is an authoring model for descriptions and native typed checks, not extra MCP fields. Use the user's language and existing project terminology; omit irrelevant sections rather than writing placeholder boilerplate.

## Establish evidence and intent

Capture purpose, actors/consumers, journeys, supplied examples, boundaries, constraints and observable finish. Inventory distinct requirements before compressing a large request. A small fix can be one packet; a feature needs independently demonstrable slices; a system needs deliveries and their shared contracts.

Separate verified facts (source/version/observation), supported assumptions (basis, effect if false and validation) and open decisions (exact question, affected requirement, resolver and blocked action). Read current implementation through AST, maintained contracts through Knowledge, durable decisions through Memory/Task, and ecosystem identity through the supported Hub route.

Resolve material authority, data retention, provider, destructive-effect and compatibility decisions before dependent work. Do not invent them to make a packet ready. Preserve accepted answers and reconcile contradictions with rationale.

## Specify contracts and proof

Give journeys/requirements stable prose labels. State starting condition, action, result and priority rationale. Cover alternate/empty/error/denied/recovery states when applicable. Describe real data types, identities/ownership, required/optional values, omitted/null/empty distinctions, lifetimes, state transitions, effects, idempotency, concurrency and errors. Selectors/metadata are never grants.

Use observable obligations, not 'fast/secure/works'. Include performance, accessibility, availability, resource or other constraints only when requested or verified; do not invent targets, quotas or capacity campaigns. Distinguish release proof from outcomes requiring later production observation.

The technical plan names current components, public APIs and boundaries, exact file/interface ownership, settled choices and alternatives where material, prerequisites and useful slices. Missing contracts are refinement prerequisites, not mocks, Boolean authority callbacks or untested assumptions.

For each unit define a meaningful fixture/setup, action/command and expected result, plus the affected source/docs and evidence owner. Reuse mature APIs and existing tests. Each code/config/behavior unit checks its maintained docs; each documentation unit checks real implementation. Choose proportionate fast and long gates; an ad hoc success is not a proven installed regression barrier. Never initiate a push merely to exercise its guard.

## Save the graph

Group by outcome and accountable ownership, not tool-call/file count. Create prerequisite/parent records before their returned IDs are used. Native creation supplies session_id, title, description, task_type, priority and `checks` with acceptance/test kinds; parent_id/depends_on and explicit calendar schedule follow the actual schema. Dates must come from the request/current records, not invented duration.

A description preserves outcome/scope, current evidence, design/contracts, risks/unknowns, verification/docs and exact continuation. Every parent and leaf needs meaningful acceptance and test checks. Parent acceptance is integrated product behavior, not a count of children.

Do not assume a batch endpoint. Submit the supported native creates in dependency order, inspect each response, and bind real Task/check IDs before later calls. Stable idempotency keys identify logical creates; a successful create retry does not revise its specification. Resolve uncertain effects through the actual producer contract rather than blindly replaying them.

A parent waits for descendants; a child must not depend on that unfinished parent. Integration depends on actual producers, not an administrative closure badge. Parallel work requires satisfied prerequisites and compatible ownership/contracts/environments. Coordinate shared interfaces and docs rather than adding unnecessary all-to-all edges.

## Readiness and coverage

Save requirement/journey → producer → returned acceptance/test IDs → integration/documentation evidence. Check both directions: no requirement without delivery/proof and no unexplained task. Read back definitions and links; detect missing contracts, stale context, scope contradictions, unsupported assumptions, cycles and ownership conflicts before affected implementation.

A planning pass proves the saved plan, never product behavior. Complete planning separately; leave implementation open/unclaimed and its checks pending. If implementation is not authorized, preserve that boundary and hand off. If a required native mutation is unavailable, save the producer gap instead of claiming reconciliation.

See [worked-feature.md](worked-feature.md) for one filled outcome and [worked-system.md](worked-system.md) for multiple deliveries. Follow [execution.md](execution.md) before claiming implementation, review or handoff.
