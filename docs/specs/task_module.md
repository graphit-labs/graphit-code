# Task Module Specification

The Task module is Graphit's authoritative work-control plane for coding agents. It replaces
repository Markdown task logs, backlog files, and host-native TODO/task state with shared LanceDB
tables. Every agent working on the same project sees the same durable sessions, queue, claims,
dependencies, subtasks, checks, comments, progress, and audit history. A session preserves the evolving
user demand and strategy across Tasks, turns and agents; Tasks preserve executable specifications and
verification. A host chat/session identifier is not a Graphit session identifier.

Project work includes investigation, diagnosis, comparison, evaluation, research, impact analysis,
and other analysis-only activity even when no source file changes. Those tasks use the same create,
claim, checkpoint, evidence, and completion lifecycle. Completion requires a reusable analytical
report in Task so later agents and analyses do not need to reconstruct known evidence or reasoning.

The module is enabled by default and can be disabled with `modules.task=false`. When disabled,
Graphit omits the Task mandate, lifecycle hooks do not open its store, and Task operations return a
disabled-module error.

## Storage and configuration

Tasks use the active provider's S3 location and active profile credentials directly. A Broker
provider obtains an in-memory STS grant/topology for the enclosing project and never shares it with
another project. With a bucket configured, every read and write
opens the authoritative database at:

```text
s3://<provider-s3-bucket>/<provider-s3-prefix>/v2/projects/<project-ulid>/<task.prefix>/
```

`task.prefix` defaults to `tasks` and follows normal inline, environment
(`GRAPHIT_TASK_PREFIX`), project, and global configuration precedence. Without a bucket, the same
schema lives in the global Graphit data directory for local development. There is no repository
replica, download, background upload, or Markdown synchronization path.

The enclosing project ULID is the Task authorization unit. A trusted subject must retain project
access for every remote search, read, claim, mutation, and export; knowing a Task or project ID is
not sufficient. See [Hub Access Control](hub_access_control.md).

Connections request zero read-consistency interval. After winning the scheduler lease, an operation
explicitly advances every opened table handle to the latest committed manifest before reading task
state. The lease serializes graph and ownership decisions; task-row revision CAS and per-claim
fencing tokens remain independent barriers against stale writers.

## LanceDB tables

| Table | Authority and purpose |
|---|---|
| `tasks` | Authoritative current snapshot, including spec, `parent_id`, dependency/check JSON, flag, owner, lease, fencing epoch/token, progress, last event/comment, and monotonic revision. |
| `task_dependencies` | Queryable materialization of each directed dependency, with active state and source revision. |
| `task_checks` | Queryable acceptance and test checks, status, evidence, verifier, verification time, and source revision. |
| `task_comments` | Append-only typed comments (`note`, `decision`, `problem`, `lesson`, `knowledge`) with deterministic ID and ordered sequence. Comment text has a LanceDB full-text index. |
| `task_events` | Append-only lifecycle audit keyed by task and zero-padded revision. |
| `task_spec_revisions` | Immutable, queryable specification history with mutation kind, optional affected check ID, actor, reason, timestamp, source task revision, and before/after state. |
| `task_control` | Scheduler lease plus resumable hard-removal intents used to serialize and recover cross-table mutations. |

Session snapshots, checkpoints, events and specification revisions share this Task database and
its project authorization/coordination boundaries. The indexed `tasks.session_id` relationship
connects Tasks to their session. See [Task sessions](task_sessions.md) for the session tables and
state invariants, and [Working with sessions](../guides/task-sessions.md) for MCP/CLI procedures.

The task snapshot embeds the dependency/check lists and the last event/comment because it is the
single CAS decision record. The other tables make those fields independently queryable. If a
process stops after the snapshot commit but before a projection commit, the lifecycle reconciliation
path reconstructs the missing projection idempotently; observational commands never repair it.

Task IDs begin with four hexadecimal digits of SHA-256 over project identity and caller
`idempotency_key`, prefixed by `tsk-`. If that ID already belongs to another key, allocation
deterministically extends the hash one digit at a time; the conditional insert fails closed rather
than overwriting on even a full-hash collision. Existing longer IDs remain stable because creation
resolves the idempotency key before allocating a new ID. Check and comment IDs retain their own
deterministic namespaces. Repeating a create/comment request with the same key returns the existing
record instead of duplicating it.

## Task specification

Agent-created Tasks must belong to a nonterminal session. Supply `session_id` explicitly; creation
can also resolve it from the parent Task or the actor's live coordinated session. MCP rejects a
creation with no resolved session. Manual CLI creation outside an agent session may leave the field
empty. Parents and children share the same session, a Task is not moved between sessions, and
historical Tasks remain available as evidence without being reassigned to new work.

Creation requires all of the following:

- a concise, action-oriented plain-text title that identifies one outcome;
- a self-contained Markdown execution packet in `description`: outcome and scope; exact relevant
  requirements, constraints and deliverables; verified starting behavior, known paths/symbols and
  their roles; prerequisite records and outputs; implementation steps, contracts and integration
  points; relevant risks and verification setup/expected results. Shared detail is referenced by
  exact Task ID and section, with essential local contracts retained in the leaf. Omit irrelevant
  sections and repeated background, not known execution constraints. A new agent must be able to
  implement and verify the outcome from this record and its explicit references without guessing
  scope or rediscovering known decisions;
- at least one singular acceptance criterion, written as an imperative statement of what the system
  **must** do or **must not** allow under an applicable condition, with a measurable or observable
  expected result;
- at least one test or validation. Behavioral checks use Given-When-Then to state known
  preconditions, one action or event, and observable outcomes. Mechanical validations instead name
  the method or command, target and conditions, and expected evidence or result;
- priority `0` through `4`;
- optional type, parent task, and blocking dependency IDs;
- a stable idempotency key (canonical title is the fallback).

Requirements are necessary, singular, feasible, consistent, unambiguous, and implementation
independent unless an implementation choice is itself a constraint. Acceptance criteria state
required behavior rather than implementation procedure. Test checks include meaningful failure
paths without duplicating equivalent scenarios.

Descriptions, checks, evidence, progress, comments, reasons and handoffs support Markdown. Store
knowledge once in its appropriate record; checkpoints add meaningful changes and reference saved
context instead of repeating the whole specification. Evidence names the command, source,
observation or artifact, relevant conditions and actual result. A next step names the action, target,
prerequisite and completion condition. Comments retain consequential decisions, problems and lessons
with rationale. Handoffs consolidate current state, evidence references, unresolved issues and the
next action. “Analysis done” or “tests pass” without evidence is insufficient. Compactness removes
repetition, not execution-relevant facts.

An analytical record preserves its question, method and sources, evidence, conclusions, decision
rationale, uncertainty and actionable implications. Later execution-relevant discoveries enter the
current description through `task_revise` before changed implementation, delegation or handoff;
progress history does not substitute for a current specification. Reusable cross-task conclusions
may also enter Memory without replacing the Task result.

Acceptance criteria and tests are structured checks, not prose interpreted at completion time.
Each starts as `pending` and must be recorded as `passed` with non-empty evidence. A failed check
retains its evidence and keeps completion closed. A claimed owner may revise supported specification
fields only with the current fencing token, expected task revision, and a reason. Every successful
change appends an immutable `task_spec_revisions` row containing the complete before/after spec.
Changing the title, description, or type resets active checks to `pending`; their earlier results
remain in the immutable before state, but completion requires evidence against the revised scope.

Obsolete checks are superseded, never deleted or rewritten. Supersession records actor, reason,
time, and an optional replacement check. Superseded checks and earlier evidence remain visible but
do not gate completion; at least one active acceptance check and one active test check must remain.

## Lifecycle and deterministic gates

```text
open --claim--> in_progress --complete--> completed
  |                    |
  +------release-------+
  +----lease/stop-------
  +------cancel--------> cancelled
                       ^
 in_progress --cancel--+
```

An unclaimed `open` task is backlog. A task is ready when it is open and all dependency tasks are
completed. Claim is atomic, assigns one owner, increments `claim_epoch`, returns a unique fencing
token, and refuses an agent that already owns another live task. All owner mutations require the
current token. The default lease is one hour. Heartbeats, progress, checks, and comments extend it
but never shorten a longer active lease. Release or lease expiry clears ownership while preserving progress, comments,
checks, and `next_step`, so another agent can resume without reconstructing history.
The stdio MCP proxy carries a stable host-agent identity across daemon reconnections, so backend
session replacement does not invalidate the active owner's token or attribution.

Completion is rejected unless every invariant is true:

- the caller owns the current fenced claim;
- the task is not flagged;
- every active acceptance and test check is `passed` with evidence;
- every direct or nested subtask is validly completed;
- every dependency remains validly completed, including after a specification revision.

Before requesting that deterministic transition, the agent must also perform a semantic bidirectional
consistency review. Code, configuration, interface, or behavior changes require identification and
update of every affected current documentation/Knowledge surface. Documentation-only changes require
comparison with authoritative implementation or behavior and correction of whichever side is stale.
The task records the concrete targets inspected and comparison evidence. An unresolved divergence is
flagged and blocks completion rather than being silently deferred. The shared completion/checkpoint
reminder carries this obligation on every supported context-capable agent boundary; an adapter that
lacks such a boundary must carry it in its adapter-specific mandate or compensation.

Full reconciliation runs at session boundaries. It repairs projections, expires leases, and reopens
any completed snapshot that violates flag/check/subtask invariants. Explicit operations repair the
single task they read or mutate; claim also expires its target inline. High-frequency post-tool and
stop hooks keep the same scheduler lease, latest-manifest refresh, and fencing guarantees while
touching only the identified agent's task; stop releases that claim immediately. This validation is
deterministic and does not ask the model to infer state transitions.

Cancellation is a durable terminal transition with a required reason. Cancelling in-progress work
also requires the current owner and fencing token; open work can be cancelled directly. Hard
removal is intentionally separate: any agent may request it only with the exact task ID repeated as
confirmation and a non-empty reason. Removal is rejected while another task references the target
as a dependency or parent. A committed intent in `task_control` is a tombstone that blocks new
dependencies and subtasks; it makes deletion of the authoritative snapshot, events, comments,
checks, and dependency projections resumable if the process stops midway, without creating orphans.

The deterministic no-garbage protocol applies whenever direction changes: remove a mistaken task
when deletion is certainly correct; otherwise cancel it so its history remains available. Agents
must not abandon superseded work in `open` or `flagged` state.

## Flags, subtasks, dependencies, and comments

A claimed task may be flagged only with a non-empty reason. The flag blocks completion, not work:
the owner may continue or release it, another agent may claim it, resolve the reason, record the
resolution, and unflag it.

`parent_id` creates a native subtask relation and is indexed on `tasks`. Parents cannot accept new
subtasks after reaching a terminal state and cannot complete before all descendants. Dependencies
are separate directed ordering edges; missing targets, self-dependencies, and cycles are rejected.

Cleanup, validation, review, documentation, commit preparation, release checks, and similar
delivery-support or finalization work belong to the relevant delivery task as subtasks instead of
unrelated top-level tasks. A pass/fail condition remains a check; the validation or finalization is a
subtask when it is itself a work unit that needs ownership, resumability, or an audit trail.

Agents append comments whenever a decision, problem, lesson, system discovery, or relevant note
will help current work or takeover. Comments are ordered, searchable, idempotent, and returned with
`task_get`; they do not replace progress checkpoints or durable Memory records when knowledge must
outlive the task context.

## Agent contract

The resident mandate routes project work, including analysis without file changes, to the Task
skill immediately before the first relevant action. The skill supplies session coordination,
specification, planning, decomposition, validation and lifecycle procedures. It is reused while available in context; other
module skills load only at their relevant evidence-gathering boundary.

At the start of a demand, get the known session or search/list relevant `open`/`in_progress`
sessions and read selected current descriptions, strategies, checkpoints and associated Tasks.
Resume matching work rather than creating duplicate sessions. If none matches, create a detailed
description of requested outcomes, scope, constraints, known context, unknowns and completion
conditions, plus the initial strategy, then claim coordination. The coordinator's session claim
is independent of Task claims. Delegated workers share `session_id`, claim their own Tasks and
report their evidence without taking the coordinator's claim or receiving its private token.

For known work, read its exact ID. For a new question or changed scope, run a focused session/Task
history search and read selected authoritative records and prerequisite results. Reuse that history for the same
question. Knowledge queries do not each trigger another Task sweep: Task history is consulted when
prior implementation, rationale or active-plan context is needed. Page only while relevant context
is missing or completeness is explicitly required; `top_k` caps the entire result window. Task and
Knowledge sources retain their distinct authority. Disabled or unavailable Task tools do not block
Knowledge retrieval or justify substituting the Graphit CLI for MCP.

Before material investigation the agent creates or resumes a bounded planning/research unit in
the session and claims it. It uses AST to establish current implementation, Knowledge for documented intent, and
relevant Task/Memory context before deciding the full delivery graph. Material uncertainty becomes
explicit refinement; the initial task is not a generic container for immediate whole-project coding.
Before implementation or delegating implementation it saves every executable unit's specification,
plan, acceptance criteria, validations and relationships, then verifies readiness and coverage.

Each completed code/configuration/behavior unit checks and updates its affected user and technical
documentation; each documentation unit checks authoritative implementation and behavior. Record
concrete sources, sections and comparison evidence, including a justified no-impact conclusion when
appropriate. Resolve contradictions before closing the unit rather than postponing them to final
integration. The shared checkpoint hook reinforces this invariant without duplicating adapter-specific
compensation in the generic skill. Completion also requires the deterministic checks, descendants,
dependencies and flags to permit it; otherwise release with current state and an executable next step.

At meaningful outcomes the coordinator also appends a session checkpoint: progress and concrete
evidence, encountered/resolved problems, decisions with rationale, current strategy and exact
`next_step`. It references Task evidence instead of copying the complete task audit trail. New
user instructions or discoveries that change intent/approach require `session_revise` with the
current revision and reason, followed by reconciliation of affected Task specifications/checks
before incompatible execution. Checkpoints alone do not replace the authoritative current intent.

Before interruption or handoff, save the session's complete continuation and release coordination;
release unfinished Task claims separately. Another agent can discover the open session, read its
current scope and checkpoint, inspect selected Tasks and obtain its own claims after release/expiry.
Do not create a new session simply because the agent, turn or host changed. Hooks route/recover state
and remind checkpoint obligations; they never fabricate substantive progress or close the session.

Delivery is final only after an explicit successful session completion. This requires terminal
associated Tasks and a final summary; the agent additionally reconciles the current demand, actual
validation and documentation evidence. Cancelled Tasks are not delivered requirements: their
removal from scope needs a recorded reason. Finishing a Task, a planning phase, a turn, compaction
or a Stop event is not session completion. For a planning-only request that saves future delivery
Tasks, report planning complete and preserve/release the open session with implementation pending.

## Feature planning and backlog handoff

Planning may originate in any tool. Incorporate externally defined requirements, decisions,
specifications, tests, subtasks, interdependencies and milestones into self-contained Graphit tasks.
An external plan or link alone does not replace the official records. Reconcile later planning
changes through the same Task workflow and read back the saved records to verify completeness.

An instruction to plan or implement project work authorizes recording that work in Task without a
second registration question. A planning-only request does not authorize implementation. Respect
explicit limits or refusal concerning future work outside the request.

Start with a bounded investigation of the request and current state; resolve questions that change
scope, correctness or contracts before finalizing the delivery map. Then, before implementation,
persist these stages in Task:

1. **Specification:** extract every requirement, correction, constraint and exclusion; assign stable
   requirement IDs for multipart work; define prioritized journeys with rationale, observable success,
   data and interface contracts, and relevant failure/boundary/recovery behavior. Distinguish facts,
   supported assumptions and material unknowns without inventing business policies or numeric targets.
2. **Plan:** ground affected paths/symbols, interfaces, data contracts, prerequisite outputs,
   integration boundaries, sequence and validation strategy in current sources. Record consequential
   choices with evidence, rationale and material rejected alternatives; include nonfunctional,
   migration or rollout constraints when applicable. Identify code and documentation ownership and
   fixture/action/expected-result validation for every unit.
3. **Decomposition:** establish parent deliveries, prerequisite work, independently verifiable slices,
   integration and finalization subtasks before coding. Multiple outcomes or dependencies cannot be
   represented by one executable whole-project task. A small single-outcome fix can keep all stages
   in one task. Split for separate ownership, validation or resumption, not for each tool call.
4. **Readiness review:** read back definitions and relations, then maintain a coverage map from each
   requirement ID to delivery task IDs and returned acceptance/test check IDs. Verify complete
   coverage, meaningful outcomes, observable checks, consistent contracts, acyclic ordering and
   execution packets usable without conversation history. Correct material gaps before affected code.

The planning/refinement task remains separate from delivery completion. Its checks verify the saved
specification, plan, coverage and handoff, not implementation success. Deliveries may depend on it.
Later changes update specifications, coverage and checks; shared requirements/contracts remain in
one planning record with explicit references from affected leaves. Creating tasks alone does not
prove that the plan covers the request or that delivery is complete.

The generated Task skill is a focused entrypoint with four selectively loaded references. Their
canonical content lives in Go source and is installed alongside `SKILL.md` for every supported agent:

| Reference | Exact loading boundary | Substance |
|---|---|---|
| `references/planning.md` | Before creating or materially revising a multi-outcome specification/plan | Evidence, journeys, ambiguity/refinement, requirements/contracts, grounded design, decomposition and readiness/coverage models. |
| `references/worked-feature.md` | Before saving the first such backlog | Complete filled specification and plan, decisions/alternatives, data/error contracts, fixture matrix, every parent/leaf packet, legal dependency graph, check-ID traceability and handoff evidence. |
| `references/worked-system.md` | Additionally, before saving a system or multiple-capability backlog | Filled umbrella/delivery/subtask hierarchy, discovery-driven decomposition, cross-delivery producer/consumer contracts, dependency ordering and final system acceptance. |
| `references/execution.md` | Before implementing/resuming, reviewing completion or handing off | Semantic readiness, scope-change reconciliation, per-unit code/documentation checks, evidence and residual-work review, plus a full proportional single-task fix. |

References are reused while retained, not eagerly embedded or reloaded for each action. Without
local skill-file access, `graphit_module_skill` retrieves the requested reference by `module: task`
and its exact `reference` name, retaining `project_dir` when known. Omitting `reference` returns the
entrypoint and reference names rather than every body. Languages, paths, commands and example task
counts are illustrative; use the user's language, real project contracts and returned task/check IDs.

Discovery may change the number or order of tasks. Revise affected specifications, contracts,
coverage, checks and dependency edges before affected implementation; preserve known results and
clearly label reconciliation still pending for blocked tasks. A repeat review with no residual gap
must not manufacture new work. Requirement-quality evidence is distinct from product verification,
and post-release metrics cannot be passed with unit-test evidence.

A completed or cancelled delivery cannot receive new children. If final review finds a defect in
an already completed producer, create its corrective unit under the nearest still-open delivery
ancestor, or a new corrective delivery when none remains open, and preserve producer/check
provenance. Release a claimed review before adding the correction as its prerequisite; reconcile
and recheck it after the correction completes. The correction must not depend on that review or
its waiting ancestor. There is no general-purpose reopen operation.

Use the existing model to represent the plan:

| Planning concept | Task representation |
|---|---|
| Whole system or feature | Delivery task, optionally typed `epic` or `feature`, with complete scope and checks. |
| Independently executable part | Subtask through `parent_id`, with its own specification, acceptance criteria and tests. |
| Prerequisite or cross-feature interdependency | Directed `depends_on` edge to a real task ID; create the referenced task first. |
| Milestone | Task, optionally typed `milestone`, with observable exit criteria and validation checks; depend on required deliveries or group them as subtasks. |
| Order of execution | Dependency graph; priority ranks work but does not establish prerequisites. |
| Decision, uncertainty or scope change | Current specification plus typed comments and immutable revision history. |

There is no separate milestone/calendar primitive. A milestone's checks must cover the integration
contracts and relevant success, failure and boundary cases. Check hierarchy and ordering together:
a descendant depending on an ancestor that waits for its descendants creates a completion deadlock,
even though the individual parent and dependency graphs may each be acyclic.

Every executable task must be self-contained enough for another agent to implement correctly and
completely without the original conversation. Record agreed behavior, scope, interfaces, inputs and
outputs, constraints, known code context, prerequisites and deliverables, expected test results and
exact supporting task IDs/sources. Before coding, a resuming agent reads prerequisite records and
linked planning requirements, reconciles any pending changes into the current specification/checks,
and resolves material gaps. Tests remain pending until actually run; a defined feature or milestone
is not a completed delivery.

The planning workflow respects the existing ownership gates. Creating future tasks does not require
claiming them. Revision and comments do require a live claim, claim requires completed dependencies,
and an agent can own only one task at a time. To refine a ready backlog task, release planning,
claim the target, revise against its current revision, release it to `open`, then resume planning.
Never remove real dependencies or change actor identity to bypass these gates.

For an already blocked task, preserve the complete change package and affected IDs in the claimed
planning/refinement task. Where needed, make that refinement a prerequisite of the open target,
without introducing a cycle. Refinement completes when its change package is verified; it must not
wait for a target revision that its own unfinished dependency prevents. Once prerequisites complete,
the implementing agent must read the package and reconcile the target before coding. Report that
reconciliation as pending until applied, rather than claiming an unchanged snapshot is current.

Before ending planning, read back tasks and relations through `graphit_task_get`/`graphit_task_list`.
Verify every requirement has a destination, milestone exits and integration tests are specified,
ordering and interdependencies are correct, open questions are explicit, and the handoff stands on
its own. Report the recorded IDs, milestones, dependencies and unresolved refinement. Leave future
deliveries `open` and unclaimed; complete only the planning work whose own checks have passed.

## Interfaces

The CLI group is `graphit task`; its subcommands cover batch, create, list/ready, get, search, export, claim,
force-takeover, revise, progress, heartbeat, comment, check/check supersede, flag/unflag, dependency add/remove,
release, complete, cancel, and confirmed remove.
The MCP tools expose the same operations as `graphit_task_*` and return compact TOON by default for
read-heavy calls.

Session tools are `graphit_task_session_create`, `get`, `list`, `search`, `claim`, `revise`,
`checkpoint`, `heartbeat`, `release`, `complete`, `cancel` and `force_takeover`, with equivalent
subcommands under `graphit task session`. `get` returns the session, events, checkpoints, immutable
specification revisions and associated Task summaries; read selected full Tasks through `task_get`.
List supports status/owner/active filters and pagination. Search retrieves current/historical
session content with a bounded ranked result window. Task list/search filter by `session_id`.
Create supplies detailed `description` and `strategy`; revision uses `expected_revision` and
`reason`. Checkpoint and release require `summary` and `next_step`; complete requires a final
summary. Session owner mutations use the independent session claim token. See the linked session
specification for exact lifecycle/recovery conditions.

`graphit_task_batch` and `graphit task batch <file|->` accept one to 100 mutations. The CLI input is
a JSON object containing `operations` and an optional default `lease`; `-` reads the object from
standard input. Each operation names an `action` and the same fields used by its single-task
counterpart. Items run sequentially in input order, every item is attempted, and the result contains
the original index, optional correlation key, normalized action, task ID, success flag, value or
explicit error. A failed item therefore cannot make later independent outcomes ambiguous. Batch is
a transport optimization, not a weaker lifecycle path: every item invokes the same LanceDB-backed
service method and retains claim fencing, dependency, check, flag, cancellation, and confirmed
removal rules. `revise` and `check_supersede` batch actions use the same fencing and revision checks
as their focused tools. A batch cannot be used to claim multiple live tasks for one agent. The
`force_takeover` action retains exact-ID confirmation, current-revision fencing, reason, explicit
per-operation replacement lease, and different-owner requirements.

After specification and decomposition, agents should prefer `graphit_task_batch` to register
multiple fully written tasks whose parent/dependency IDs are already known. Each `create` keeps
its session association, complete description, acceptance criteria, tests and stable idempotency key. Create parents
first, then batch siblings, then create tasks that require the newly returned IDs. Existing
prerequisites need not be completed for blocked tasks to be created. The optional `key` correlates
results only; it is never interpolated into IDs, dependencies, tokens or revisions. The Task
skill's worked-feature reference supplies two complete creation packets in a single payload.

Inspect every `results[].ok` and preserve successful IDs/checks; there is no rollback on an item
failure. Retry only failed or uncertain creates with their original `idempotency_key`, since an
error can follow an authoritative write. An existing key returns the saved task without applying
changed fields; reconcile specification changes through the normal read/claim/revise lifecycle.
Bulk creation never claims tasks or changes their implementation readiness, and a large plan is
split into bounded batches rather than compressed into generic task descriptions.

`task_search` uses LanceDB full-text indexes over task specs/check evidence and comment bodies. It
accepts `page_size` plus the opaque `cursor` returned as `next_cursor`; `top_k` remains the cap for
the complete ranked result set. The cursor is bound to the query, project, page size, and cap, so a
changed request fails instead of silently skipping or duplicating work. Search is discovery;
`task_get` is the authoritative retrieval call and includes the snapshot, ordered events, ordered
comments, and immutable specification revisions.

`graphit task export [task-id]`, `graphit_task_export`, and `GET /api/tasks/export` call the same
domain operation. With no ID it emits every project task. With an exact ID it emits that task and
all recursive subtasks. The versioned normalized JSON contains decorated task snapshots plus every
public dependency, check, event, comment, and specification-revision entity in stable key/sequence
order. Fencing tokens and `task_control` scheduler rows are deliberately excluded because they are
coordination secrets rather than transferable task data.

The Observatory Task Explorer uses `GET /api/tasks` for lightweight paginated discovery. The
endpoint accepts `project_dir`, `query`, `status`, `page_size`, and an opaque query-bound `cursor`;
responses contain only catalogue summaries and never include audit entities. Catalogue and export
are read-only LanceDB paths: neither acquires the scheduler mutation lease nor repairs projections.
Catalogue results are ordered by creation time from newest to oldest, with task ID as the stable
tie-breaker before pagination.
Selecting an exact task or explicitly downloading the project uses the complete export contract,
so the browser does not maintain a second authoritative task projection.
The detail view renders every Markdown-capable current and historical field through the shared safe
Markdown component, while compact metadata remains plain text. Stored and exported values remain
the original source Markdown.

`graphit_task_force_takeover` and `graphit task force-takeover <id>` recover an unexpired
`in_progress` claim only when its process or session is confirmed unrecoverable. The caller supplies
the current revision, exact task-ID confirmation, a durable reason, a different new owner, and a
replacement lease. The atomic mutation rotates the private fencing token, increments the claim
epoch, preserves task state, and adds a `force_takeover` event naming the ownership transition and
reason. It rejects stale revisions, the current owner, expired or non-active claims, invalid
confirmation, and agents that already own other live work. Expired claims continue through normal
`claim`; force takeover must not preempt a reachable owner.

## Storage lifecycle

Task queries are observational operations. `Get`, `List`, `Search`, catalogue, and export neither
acquire the scheduler lease nor repair projections, create indexes, or commit table versions.
Projection rows are written only after an authoritative mutation and only when the keyed projection
is absent. Explicit reconciliation remains the recovery path for missing or stale projections.

Every Task storage operation receives a deadline. `task.operation_timeout` configures ordinary
operations and defaults to 30 seconds; a shorter caller deadline always wins. Deadline failures are
reported as Task storage errors so callers can distinguish an unavailable backing store from domain
validation. Scheduler release uses its own bounded cleanup context, preventing a failed store from
holding a caller indefinitely.

The daemon owns one `task_maintenance` loop per enabled project. It folds newly written rows into
indexes, compacts fragments, and prunes obsolete LanceDB versions every 15 minutes. Maintenance
runs under the same cross-process scheduler lease as mutations and has a five-minute deadline.
`task.version_retention` controls the pruning window and defaults to 15 minutes. Each legitimate
mutation still advances the LanceDB table version because a version is the immutable transaction
snapshot; pruning removes snapshots older than the retention window after compaction has made the
current layout efficient.

## Source map

| Concern | Location |
|---|---|
| Domain service and invariants | `internal/task/service.go` |
| Ordered batch dispatch | `internal/task/batch.go` |
| Schemas and projections | `internal/task/table.go` |
| Hook identity and lifecycle maintenance | `internal/task/hook.go` |
| Skill and mandate | `internal/task/rule.go`, `internal/task/rule_compact.go` |
| Generated durable session lifecycle and worked handoff | `internal/task/rule_session.go` |
| Generated planning, worked feature, worked system and execution references | `internal/task/rule_planning.go`, `internal/task/rule_examples.go`, `internal/task/rule_system.go`, `internal/task/rule_execution.go` |
| MCP interface | `internal/mcpstdio/tools_task.go` |
| CLI interface | `cmd/graphit/commands/task.go` |
| Observatory API and explorer | `internal/uiserver/task_handler.go`, `internal/ui/src/components/task/TaskExplorerPage.tsx` |
