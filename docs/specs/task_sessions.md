# Durable Task sessions

This reference describes the session entity in the Task module for integrators and maintainers. For agent and CLI procedures, see the [session guide](../guides/task-sessions.md); for task checks, dependencies and execution, see the [Task specification](task_module.md).

A session preserves the user's current demand and the reasoning needed to continue it across turns, compaction, disconnection and agent handoff. It belongs to one Graphit project. Its `ses-*` ID is distinct from a host conversation ID, an MCP transport session, an agent identity and a task ID. A new turn does not automatically create or close a session.

## Intent, execution and coordination

The session owns the detailed current request, scope and strategy. Tasks own executable specifications, dependency/subtask graphs, acceptance criteria, validation and individual execution claims. A session coordinator may hold one session claim and one task claim independently. Other agents may work on tasks in the same session without taking its coordination claim. Delegation carries the durable session ID and task IDs; it never shares claim tokens.

Create a session with a title, self-contained description, strategy and stable idempotency key. The description should identify requested outcomes, constraints, known context, open questions and completion conditions. It must be revised when the request changes. Repeating a create with the same project/key returns the existing session without replacing its content or returning its claim token. The default key is the canonical title; use a new explicit key for a genuinely different demand with the same title.

Task creation resolves `session_id` in this order:

1. The explicitly supplied session, agreeing with the parent when present.
2. The parent's session, for a subtask.
3. The caller's live coordinated session, for a root task.

A parent and child must have the same association. A task cannot be reassigned to another session by revision, and a parent change cannot cross sessions. The session must exist and be `open` or `in_progress` at task creation. MCP task creation, including each create in a batch, requires a resolved session; the agent receives an error instead of silently creating unassociated work. Manual CLI/service creation can omit association when there is no parent/current session. An unassociated manual parent cannot be used to bypass the agent requirement. Reusing a task idempotency key from an unassociated record in agent work, or from a different explicitly/inferred session, is rejected; choose a key for the actual work unit instead of silently reusing another demand.

Task `get`, list/search/catalog and export preserve `session_id`; list/search can filter on it. Search applies the filter before the result cap, including comment matches. `session_get` returns linked task summaries; retrieve selected task specifications/evidence with Task tools. Reading another project's sessions uses that project's resolved runtime `project_dir`, never a path persisted in shared descriptions.

## Storage and relationships

Sessions use the same authorized project LanceDB store and scheduler lease as tasks. Task-disabled projects also disable session tools and hooks. Existing project S3 authorization, bounded operation timeouts and strong read consistency apply.

| Table | Authority and queryable fields |
|---|---|
| `task_sessions` | Current snapshot. Indexed ID, idempotency key, status, owner and full-text search; explicit title, description, strategy, timestamp/revision columns and complete snapshot JSON. |
| `task_session_events` | Immutable lifecycle events by session/revision, including actor, status transition, summary and next step. |
| `task_session_checkpoints` | Immutable descriptive checkpoints by session/revision; sequence, actor, time, summary, problems, decisions, strategy and next step. Full-text search includes all descriptive fields. |
| `task_session_revisions` | Immutable intent revisions with reason, actor/time and before/after title, description and strategy. Both prior and current intent are searchable. |
| `tasks.session_id` | Indexed authoritative task-to-session relationship, empty only for unassociated manual work. Task specification history and export preserve the association. |

The session snapshot includes its latest event and any associated checkpoint/specification revision. A single-row revision CAS commits the decision. History tables materialize it idempotently using session/revision keys. Before the next mutation replaces that recovery anchor, the prior event is materialized. A failed projection may follow a successful authoritative write: an error or lost response is not proof that nothing changed.

Reads do not mutate the store. `session_get` overlays the latest authoritative event if its projection is missing. Lifecycle reconciliation repairs missing projections and releases expired claims. Historical checkpoints/revisions are preserved through heartbeat, release and subsequent updates. Maintenance includes the session tables and their indexes.

This schema changes the Task store and export schema (version 2). Automatic migration and backward compatibility are not provided. An incompatible existing store fails explicitly; opening it does not delete or reset its data. Validate with a fresh store and plan adoption explicitly before replacing a binary used against an older store.

## Public state and concurrency

| Operation | Starting state and requirements | Result |
|---|---|---|
| create | Required title, description, strategy and actor | `open`; durable identity and initial intent revision |
| claim | `open`, or same live owner reclaiming its token | `in_progress`; coordinator, lease, fencing token/epoch |
| revise | Live claim, current `expected_revision`, nonempty reason, actual intent change | New current intent and immutable before/after revision |
| checkpoint | Live claim; nonempty `summary` and `next_step` | Append descriptive checkpoint and advance latest progress/continuation |
| heartbeat | Live claim | Renew lease without changing semantic intent/checkpoint |
| release | Live claim; descriptive summary and next step | `open`, clear ownership; append handoff checkpoint |
| lease expiry | Expired claim reconciled or claimed again | `open`, preserve intent/history; former token cannot write |
| force_takeover | `in_progress`, exact ID confirmation/current revision, reason, explicit positive lease | Rotate owner/token/epoch; record recovery rationale |
| complete | Live claim, final summary, all linked tasks terminal and valid | `completed`; clear ownership and preserve final outcome |
| cancel | Live claim, cancellation reason, all linked tasks terminal and valid | `cancelled`; clear ownership, no task cancellation cascade |

The four statuses are exactly the Task statuses: `open`, `in_progress`, `completed`, `cancelled`. Blockers and problems are descriptive evidence, not additional states. There is no implicit reopen or session deletion operation. New work after closure belongs to a new session that can reference the previous outcome.

Only one live coordinator session is permitted per actor. Task claims remain independent. A competing coordinator is rejected; an expired/stale token cannot mutate the session, even before a hook reconciles expiry. A revised intent requires the latest revision; reread after conflicts instead of replaying a stale update. Force takeover is an explicit exceptional recovery, not a way to claim another live agent's work casually.

`session_get`, list, search and export never expose claim tokens. Lifecycle mutation responses expose only the participating coordinator's token while still claimed. Session history does not embed private claim state. Task export includes sessions and their histories, filtered to referenced sessions for a task/subtree export; a full export also includes sessions with no tasks.

## Checkpoints and changes of direction

`summary` states progress and evidence, with task/check references. `problems` states concrete failures or blockers. `decisions` includes rationale. `strategy` records the approach at that checkpoint. `next_step` names the next action, target and completion condition. Optional fields may be omitted when no relevant change occurred; do not fill them with boilerplate. Checkpoints complement task evidence rather than duplicating all task descriptions.

Changing the current strategy or demand uses `session_revise`, with `expected_revision` and reason. The `strategy` field of a checkpoint is a historical observation; it does not silently replace the current session strategy. Reconcile affected task packets/dependencies/checks before executing a changed plan.

For example, after the user adds audit requirements, revise the current session description and strategy with that requirement, record why it changes the approach, update/create the affected tasks, then checkpoint the resulting plan with returned IDs and unresolved validations. A later coordinator reads the current description plus relevant history and does not have to infer which of two plans is authoritative.

## Closure and handoff

Both `complete` and `cancel` share the same closure gate: they reject any linked `open`/`in_progress` task, or a completed task whose checks/dependencies/subtasks are no longer valid. A cancelled task is terminal, but cancellation does not prove its requirement was delivered: the final summary must explain the current agreed scope and any explicitly cancelled work. Neither operation silently cancels children.

Finish required task checks and documentation evidence first, then close the session and confirm the result before declaring the user's demand finished. Completing the last task alone leaves the session active. A session with no tasks can close only as an explicitly summarized outcome such as an analysis; agents are still instructed to represent executable work in tasks.

On interruption, record a handoff containing current intent, evidence, decisions, outstanding problems and exact next action. Release the coordinating claim when handing over. The replacement lists active sessions or searches the relevant topic, gets the chosen session and selected tasks, claims with its own identity and continues from the saved state. Do not choose the first unrelated open session merely because it exists.

Native Stop/SessionEnd hooks are execution boundaries, not proof of completion. They may release ownership only when the actor can be correlated safely, preserving substantive checkpoints and recording a lifecycle event. They never invent the user's description or mark a session completed. Without a reliable native identity, explicit tool handoff and lease expiry preserve isolation. See [adapter enforcement](../architecture/adapter-hook-enforcement.md).

## Verification and implementation ownership

- `internal/task/session_types.go`: public entity, revision/checkpoint and request/response shapes.
- `internal/task/session_table.go`: schemas, snapshot encoding, history projection and CAS recovery.
- `internal/task/session_service.go`: lifecycle, ownership, discovery, closure and relationship resolution.
- `internal/task/types.go`, `table.go`, `service.go`, `batch.go`: Task association, filters and export.
- `internal/task/session_lancedb_test.go`: fresh-store/reopen handoff, parent/child/batch association, filtered search, stale revision/token/expiry/takeover, closure/cancel gates, projection recovery, historical search and concurrent claims.
- MCP and CLI adapters expose the same service contract; their integration tests exercise transport inputs and public results. Hook and generated-instruction tests cover the supported agents separately.
