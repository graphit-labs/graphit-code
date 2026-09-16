package task

const taskExecutionReference = `# Execute, revise, validate and hand off

Read before claiming implementation, resuming another agent's work, evaluating completion or preparing a handoff. For a material change to scope/contracts, also read [planning.md](planning.md). Reuse retained references; this is not a reason to reload the entire project plan at each tool call.

## Starting an executable unit

1. Get the leaf, its parent specification and named prerequisite outcomes. Read current description, checks, progress/next_step and relevant decisions. Do not reread every unrelated audit event. A ready API status means dependencies are complete; also verify semantic readiness: the specification and contracts actually answer the questions the work needs.
2. Match the saved plan against current code and docs with selected AST/Knowledge evidence. A repository-relative path is a locator, not proof that a contract is unchanged. Resolve project_dir on the current host for the call; never copy a previous host's absolute checkout root into descriptions, evidence or handoffs. Reuse existing authoritative evidence when still applicable; investigate only missing/stale context.
3. Claim the ready unit with the current agent identity and private returned claim_token. Confirm the description/check revision and edit boundaries. Do not delegate overlapping source/docs ownership or claim a parent and its child with one identity.
4. Follow the agreed outcome, contract and validation plan. Adapt implementation mechanics when evidence warrants; revise scope/constraints/checks first if the intended result changes. Native TODOs may organize execution but do not replace saved Task state.

## Scope changes and newly discovered work

A new discovery is not permission to silently expand the implementation. Record what changed, evidence, affected requirement/task/check IDs and its effect on behavior, contracts, ordering or validation. Preserve completed evidence historically. If the request already authorizes the revised work, update it without a redundant approval request; ask only for genuinely unresolved user decisions or additional authorization.

Example: while planning an export, the existing importer accepts a different identifier format than the request assumes. Before generating implementation leaves, record the observed schema, resolve compatibility, and adjust the data contract and validation matrix. If discovered after the graph exists, add a bounded contract-refinement task, update coverage and affected prerequisites, and keep incompatible leaves from executing. Do not keep the old task body and hope a progress note will override it.

Use task_revise with a live claim, latest expected_revision and reason. Changing description/title/type resets active checks; validate the revised scope before recording new passes. Check supersession retains old evidence and requires a reason/replacement when applicable. Do not silently reuse evidence for changed behavior.

Only ready work can be claimed and one live claim is allowed per agent. To revise another ready task, release current work with continuation, claim/revise/release the target, then resume. A blocked target cannot be claimed just to rewrite its description: preserve the complete correction package in a claimed refinement task, add its prerequisite edge to the open target if needed, and label reconciliation pending. Once prerequisites finish, the executor must apply that package to the target description/checks before coding. Avoid making refinement depend on the blocked implementation it is supposed to unblock.

Cancelled/superseded work must not remain as a misleading open backlog. Cancel with reason and replacement when its history matters; remove only justified duplicate/mistaken records with exact confirmation, never to erase failed evidence. Search matching work before creating residual tasks so repeating the same review does not duplicate the backlog.

A completed/cancelled delivery cannot receive new children, and completed work cannot be claimed for revision. If final review finds a defect after a producer delivery closed, create the bounded correction under the nearest still-open delivery/system ancestor, referencing the closed producer and failed requirement/check. If no ancestor remains open, create a new corrective delivery with that provenance. Make the still-open review depend on the correction: save/release its claim before dependency_add, then execute the correction and resume/reconcile/recheck the review. The correction must not depend on that review or an ancestor waiting for it. Do not bypass lifecycle rules or call a nonexistent reopen tool.

## Every work unit checks code and documentation together

For a code/configuration/behavior unit, identify affected maintained user and technical docs, update them with the behavior, and compare examples/contracts with tested source. For a docs unit, inspect the authoritative behavior, resolve whichever side is stale within authorized scope, and validate commands, links and claims. Read the Knowledge skill before that maintenance. A module-unit checkpoint is not done while its affected documentation is knowingly contradictory; a final integration task cannot postpone this obligation.

Record the concrete surfaces and evidence, including a reasoned no-impact result when no maintained documentation changes. 'Docs reviewed' alone is insufficient. For example: 'API Query parameters and Error responses sections now match StrictOptionalBool and request tests; documented empty/repeated flag examples return INVALID_QUERY.' If an unresolved mismatch must block completion, flag it with the objective clearing condition.

## Evidence, progress and convergence

The task's acceptance_criteria are obligations; its tests are verification methods. Record each active check using the returned checks[].id as check_id, actual pass/fail and evidence. Name fixture/environment, command or inspected artifact, expected versus actual result, and relevant source/check references. A command exit code alone is not enough if it does not exercise the required outcome. A plausible implementation, reviewed plan or mocked integration does not prove end-to-end behavior.

Use the least costly meaningful verification for the risk: existing tests, focused new regressions, manual inspection or a runnable scenario. No blanket requirement for new test files, exact test count, coverage percentage or a new framework. If the defined check cannot run, preserve the limitation and exact recovery action; keep that check unresolved rather than reporting assumed success.

Before delivery completion compare current behavior against the requirement/contract map, not merely changed lines or the existence of planned files. Classify substantive gaps as missing, partial, contradictory or outside authorized scope. Link each gap to evidence and the affected requirement. Correct authorized gaps or create/resume the concrete residual unit; do not count an unrelated passing suite as proof. If there are no gaps, do not manufacture cleanup work.

At a meaningful completed unit, task_progress should preserve:

- Outcome/delta: what behavior, artifacts or analysis result now exists and what changed in the plan.
- Evidence: executed checks and source/doc references, actual results and unrun/failed work.
- Decisions: consequential choice, rationale and impact; use a typed comment when useful across subsequent steps.
- Next action: exact target, prerequisite and condition for finishing, not 'continue implementation'.

For analysis-only tasks save the substantive report: question/scope, method and sources, evidence, findings, reasons for conclusions, uncertainty, alternatives where material and implications. Preserve a rejected hypothesis when it would prevent costly repetition. Promote reusable non-obvious conclusions to Memory; neither a memory nor a one-line completion replaces the Task result.

## Handoff and completion packet

Release interrupted/blocked work with a packet that distinguishes completed, remaining and blocked work. Include task/parent/prerequisite IDs, current revision or changed-contract reference, modified artifacts, accepted decisions, actual validation, unrun/failed checks, exact next action and risks that change the approach. Do not expose claim_token in descriptions, comments, handoffs or user output; the next owner receives a new token by claiming.

Complete only when active checks pass, dependencies and descendants are complete, flags are resolved and the code/documentation comparison is recorded. Summarize delivered requirements and evidence references, residual limitations and follow-on implications. Release is not completion; writing a complete plan does not prove delivery; a system parent stays open until its actual deliveries and integration gates finish.

## Filled small-fix example: normalize before the empty guard

The following fictional TypeScript paths and commands illustrate a single-outcome repair. Adapt to the actual language/test harness; no epic or invented preparatory task is needed. Assume discovery already established: src/search.ts/search checks the original string for emptiness, then trims before calling repository.find; src/search.test.ts covers empty and ordinary input, and the existing search API documentation promises whitespace-only requests behave like empty input.

~~~yaml
project_dir: /example/catalog
title: Skip repository lookup for whitespace-only searches
type: bug
idempotency_key: search-whitespace-empty-guard
description: |
  ## Outcome and scope
  R1: whitespace-only input returns [] with zero repository calls.
  R2: nonempty input keeps existing trimming and result behavior.
  Fix the empty guard only; no ranking, tokenization, API type or Unicode
  normalization redesign. Preserve the string.trim convention already used.
  ## Starting context
  src/search.ts/search currently guards query.length before trim and then
  passes the trimmed value to repository.find. An input of spaces reaches
  find(''), contradicting docs/search/api.md Empty queries. The test harness
  in src/search.test.ts exposes a repository spy and a supplied result list.
  No unresolved product decision remains; reproduce the observed mismatch.
  ## Plan and contracts
  Compute the trimmed value once before the empty check. Return [] for its
  empty result; otherwise call repository.find once with the trimmed value
  and preserve returned rows and ordering. Extend the existing cases rather
  than creating an additional harness. Do not change the established trim
  semantics or classify all non-ASCII whitespace by a new custom policy.
  ## Verification and documentation
  Run npm test -- --run src/search.test.ts. Empty string, spaces and
  tab/newline input must each return [] without repository calls. Input
  ' abc ' must call find('abc') once and return the supplied rows unchanged.
  Compare docs/search/api.md Empty queries against the corrected function
  and tests; its promise remains valid, so record no textual doc change
  with those references. If other docs contradict this contract, reconcile
  the affected surface before completion.
acceptance_criteria:
  - '[A1] R1: inputs whose existing trim result is empty must return [] without calling repository.find.'
  - '[A2] R2: nonempty trimmed input must query once and return the existing repository result unchanged.'
  - '[A3] The documented empty-query contract must agree with verified function behavior.'
tests:
  - '[T1] With a repository spy, call search for empty, spaces and tab/newline input separately; each yields [] and zero calls.'
  - '[T2] Let repository.find return rows [b,a]; search with padded abc. Expect one call with abc and output [b,a], preserving values/order.'
  - '[T3] Run npm test -- --run src/search.test.ts; compare src/search.ts and tested cases with docs/search/api.md Empty queries and record the matched contract.'
ai_optimized: true
~~~

This one packet contains specification, evidence, implementation plan, scope boundaries, acceptance and validation. It does not claim any test ran merely by defining it. If discovery instead reveals unrelated search ranking and authorization changes, stop treating it as this small repair and create the necessary delivery graph before implementation.
`
