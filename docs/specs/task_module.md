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
| `task_control` | Scheduler lease plus resumable hard-removal intents used to serialize and recover cross-table mutations. |

The task snapshot embeds the dependency/check lists and the last event/comment because it is the
single CAS decision record. The other tables make those fields independently queryable. If a
process stops after the snapshot commit but before a projection commit, the next command or hook
reconstructs the missing projection idempotently.

Task IDs begin with four hexadecimal digits of SHA-256 over project identity and caller
`idempotency_key`, prefixed by `tsk-`. If that ID already belongs to another key, allocation
deterministically extends the hash one digit at a time; the conditional insert fails closed rather
than overwriting on even a full-hash collision. Existing longer IDs remain stable because creation
resolves the idempotency key before allocating a new ID. Check and comment IDs retain their own
deterministic namespaces. Repeating a create/comment request with the same key returns the existing
record instead of duplicating it.

## Task specification

Creation requires all of the following:

- a concise title;
- a robust, self-contained description stating objective, context, scope, constraints, and intended
  result;
- at least one observable acceptance criterion;
- at least one concrete test or validation;
- priority `0` through `4`;
- optional type, parent task, and blocking dependency IDs;
- a stable idempotency key (canonical title is the fallback).

Acceptance criteria and tests are structured checks, not prose interpreted at completion time.
Each starts as `pending` and must be recorded as `passed` with non-empty evidence. A failed check
retains its evidence and keeps completion closed.

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
current token. Release or lease expiry clears ownership while preserving progress, comments,
checks, and `next_step`, so another agent can resume without reconstructing history.

Completion is rejected unless every invariant is true:

- the caller owns the current fenced claim;
- the task is not flagged;
- every acceptance and test check is `passed` with evidence;
- every direct or nested subtask is validly completed;
- the task's dependencies were complete when it was claimed.

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

The CLI group is `graphit task`; its subcommands cover create, list/ready, get, search, claim,
progress, heartbeat, comment, check, flag/unflag, dependency add/remove, release, complete,
cancel, and confirmed remove.
The MCP tools expose the same operations as `graphit_task_*` and return compact TOON by default for
read-heavy calls.

`task_search` uses LanceDB full-text indexes over task specs/check evidence and comment bodies.
Search is discovery; `task_get` is the authoritative retrieval call and includes the snapshot,
ordered events, and ordered comments.

## Source map

| Concern | Location |
|---|---|
| Domain service and invariants | `internal/task/service.go` |
| Schemas and projections | `internal/task/table.go` |
| Hook identity and lifecycle maintenance | `internal/task/hook.go` |
| Skill and mandate | `internal/task/rule.go`, `internal/task/rule_compact.go` |
| MCP interface | `internal/mcpstdio/tools_task.go` |
| CLI interface | `cmd/graphit/commands/task.go` |
