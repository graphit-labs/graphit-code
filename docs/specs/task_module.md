# Task Module Specification

The Task module is Graphit's authoritative work-control plane for coding agents. It replaces
repository Markdown task logs, backlog files, and host-native TODO/task state with shared LanceDB
tables. Every agent working on the same project sees the same queue, claims, dependencies,
subtasks, checks, comments, progress, and audit history.

The module is enabled by default and can be disabled with `modules.task=false`. When disabled,
Graphit omits the Task mandate, lifecycle hooks do not open its store, and Task operations return a
disabled-module error.

## Storage and configuration

Tasks use the Hub S3 configuration directly. With `hub.bucket` configured, every read and write
opens the authoritative database at:

```text
s3://<hub.bucket>/<hub.prefix>/<task.prefix>/project/<project-id>
```

`task.prefix` defaults to `tasks` and follows normal inline, environment
(`GRAPHIT_TASK_PREFIX`), project, and global configuration precedence. Without a bucket, the same
schema lives in the global Graphit data directory for local development. There is no repository
replica, download, background upload, or Markdown synchronization path.

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

Creation requires all of the following:

- a concise, action-oriented plain-text title that identifies one outcome;
- a self-contained Markdown specification with detail proportional to the work. It states the
  objective and value, context, in-scope and out-of-scope boundaries, requirements or observable
  behavior, constraints and assumptions, interfaces or dependencies, material risks and edge cases,
  and the intended result;
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

Descriptions, check text and evidence, progress and next steps, comments, reasons, and completion or
release summaries support Markdown. Evidence identifies the command, observation, or artifact,
relevant conditions, and actual result. Progress states completed facts and changed constraints; a
next step names the exact action, target, and completion condition. Comments preserve durable
context, rationale, impact, and references. Reasons state cause, impact, and the clearing condition or
replacement when applicable. IDs, titles, types, statuses, priorities, actors, and timestamps remain
compact plain text.

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

The dynamic mandate always directs an enabled agent to Graphit Task instead of its host's native
task/TODO/planning mechanism. Before material work the agent searches prior tasks, reads the target,
creates or relates missing work, and atomically claims it. During work it checkpoints progress,
records useful comments, and submits check evidence. It completes only after every deterministic
gate passes, or releases with an exact next step when stopping.

The installed Task skill contains the operational detail. The mandate stays a compact router so
its always-loaded token cost remains small.

## Interfaces

The CLI group is `graphit task`; its subcommands cover batch, create, list/ready, get, search, export, claim,
revise, progress, heartbeat, comment, check/check supersede, flag/unflag, dependency add/remove,
release, complete, cancel, and confirmed remove.
The MCP tools expose the same operations as `graphit_task_*` and return compact TOON by default for
read-heavy calls.

`graphit_task_batch` and `graphit task batch <file|->` accept one to 100 mutations. The CLI input is
a JSON object containing `operations` and an optional default `lease`; `-` reads the object from
standard input. Each operation names an `action` and the same fields used by its single-task
counterpart. Items run sequentially in input order, every item is attempted, and the result contains
the original index, optional correlation key, normalized action, task ID, success flag, value or
explicit error. A failed item therefore cannot make later independent outcomes ambiguous. Batch is
a transport optimization, not a weaker lifecycle path: every item invokes the same LanceDB-backed
service method and retains claim fencing, dependency, check, flag, cancellation, and confirmed
removal rules. `revise` and `check_supersede` batch actions use the same fencing and revision checks
as their focused tools. A batch cannot be used to claim multiple live tasks for one agent.

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
| MCP interface | `internal/mcpstdio/tools_task.go` |
| CLI interface | `cmd/graphit/commands/task.go` |
| Observatory API and explorer | `internal/uiserver/task_handler.go`, `internal/ui/src/components/task/TaskExplorerPage.tsx` |
