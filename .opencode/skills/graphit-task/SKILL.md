---
name: graphit-task
description: 'Deterministic tasks: shared LanceDB work, dependencies, claims, progress, handoff, completion, and prior-task search.'
---

# Graphit Task

Graphit Task is the only task-control source of truth. Always use it instead of the host agent's native task/TODO/planning mechanism, even when that mechanism is available. The shared LanceDB tables own task state; do not create Markdown task logs or TODO/backlog files.

## Enter work

Before material work, search prior tasks with `graphit_task_search` and follow `next_cursor` until the relevant history is covered; use `graphit_task_list` with `ready: true` to find unblocked work and `graphit_task_get` to read dependencies, progress, next step, and audit history. Resume an existing task instead of duplicating it.

Create missing work with `graphit_task_create`, a stable `idempotency_key`, robust self-contained description (objective, context, scope, constraints, intended result), explicit acceptance criteria, and concrete tests/validations. Use `parent_id` for subtasks and dependency IDs for ordering; an unclaimed `open` task is the backlog. Add/remove dependencies with `graphit_task_dependency_add`/`graphit_task_dependency_remove`; never encode relations only in prose.

Claim with `graphit_task_claim` before changing project state. A claim returns a fencing token; keep it private and pass it to progress, heartbeat, release, and complete. A rejected claim means another agent owns the task—choose other ready work. Never bypass a claim or edit task tables directly.

## Advance and hand off

After each independently reportable unit, call `graphit_task_progress` with what landed and the exact next step. Add relevant typed comments with `graphit_task_comment_add` for decisions, problems, lessons, and system knowledge. Record every acceptance/test result through `graphit_task_check` with concrete evidence; a parent cannot complete before all subtasks, and no task can complete with an unchecked/failed check.

When a risk or unresolved condition must gate completion, call `graphit_task_flag` with its reason; flagged work may be released and claimed by another agent, but cannot complete until `graphit_task_unflag` records resolution. Call `graphit_task_complete` only after every check passes, all subtasks complete, and no flag remains. If stopping, blocked by external input, or handing off, call `graphit_task_release` with current evidence and next step; supported stop hooks release host-identifiable claims, leases recover crashes, and hooks reconcile invalid completion state.

On a direction change, clean obsolete work immediately: use `graphit_task_cancel` when its history remains useful, or `graphit_task_remove` only when deletion is certainly correct. Removal requires the exact task ID and a reason and refuses referenced tasks. Never leave superseded work open or flagged as garbage.

Every mutation is revision-checked and audited. Dependencies gate readiness; expired or stopped claims become open for takeover. Search is discovery; `get` and filtered `list` read authoritative state.

Tool index: `graphit_task_search`, `graphit_task_list`, `graphit_task_get`, `graphit_task_create`, `graphit_task_claim`, `graphit_task_progress`, `graphit_task_heartbeat`, `graphit_task_comment_add`, `graphit_task_check`, `graphit_task_flag`, `graphit_task_unflag`, `graphit_task_release`, `graphit_task_complete`, `graphit_task_cancel`, `graphit_task_remove`, `graphit_task_dependency_add`, `graphit_task_dependency_remove`.
