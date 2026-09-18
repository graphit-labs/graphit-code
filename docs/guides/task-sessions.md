---
title: "Coordinate and resume Task sessions"
description: "Preserve a complete request, decisions and progress across agents using durable Task sessions."
content-type: guide
audience: developers
keywords:
  - task
  - sessions
  - handoff
---

# Coordinate and resume Task sessions

A Task session preserves one evolving user request, its current specification and strategy, descriptive checkpoints, and the tasks that deliver it. Use it when coordinating work that another agent may need to resume. A session is durable project data; it is not an MCP connection, agent identity, terminal process, or chat turn.

The Task module must be enabled and its configured LanceDB store available. MCP agents use the tools below. Human operators may use the equivalent `graphit task session` commands from the intended project's directory. Project identity and relative source paths belong in shared descriptions; a host's checkout path belongs only in the live `project_dir` argument.

## Find existing work before creating it

At the start of a request, after interruption, and whenever you need prior reasoning:

1. If the session ID is known, call `graphit_task_session_get` immediately. Otherwise list with `active: true`, or search by the request's business outcome, decision or problem.
2. Read the selected session's current description and strategy, checkpoints, specification revisions and task summaries. Use `graphit_task_get` for relevant task detail, or `graphit_task_list` with `session_id` to inspect its work graph.
3. Confirm that the session matches the current request. A similar title does not authorize replacing another request. Reuse sufficient context; avoid exporting the entire project for one question.
4. Claim an open/released session before coordinating. A different active owner must release it, its lease must expire, or an explicitly justified force takeover must succeed.

`graphit_task_session_list` supports `status`, `owner`, `active`, `page_size` and `cursor`. Active means `open` or `in_progress`. Search accepts `query`, `top_k`, `page_size` and `cursor`; `top_k` caps the complete ranked result set. Both return `results` and `next_cursor`. Preserve filters and query when following a cursor. Search can recover earlier checkpoints and revisions, not only the latest summary; `get` establishes current authoritative state.

CLI equivalents:

```bash
graphit task session list --active
graphit task session search 'archive export retry' --limit 5
graphit task session get "$SESSION_ID"
graphit task list --session "$SESSION_ID"
graphit task search 'retry decision' --session "$SESSION_ID"
```

Variables in these examples stand for actual returned values. Never copy illustrative IDs or another agent's private claim token into a live call.

## Create a complete request and plan its tasks

For a new request, `graphit_task_session_create` requires `title`, `description` and `strategy`. Supply a stable `idempotency_key` for retries of that logical request. Creation returns an open session; claim separately with `graphit_task_session_claim`. Record the returned ID and keep its claim token private.

The description must capture the requested outcome, current scope, requirements and success criteria, constraints, known sources and unresolved questions. The strategy explains how to investigate, decompose, implement and validate the request. A generic title or copied short prompt is insufficient for handoff.

Illustrative complete initial content for a request to add archived-record exports:

```json
{
  "title": "Export archived records with predictable retry behavior",
  "description": "The user needs authorized operators to export archived records without changing the default active-only listing. Preserve existing access controls and export columns. R1: an explicit archived filter selects archived records only. R2: repeated delivery of the same export request must not produce duplicate jobs. R3: document the filter and retry behavior for operators and API consumers. First inspect the current listing/export implementations and documented contracts; the exact retry key location is unresolved. Success requires contract tests for active/default and archived filters, duplicate-delivery evidence, and updated user/API documentation.",
  "strategy": "Read the existing listing and export contracts through Knowledge and implementation through AST; recall prior retry decisions. Resolve the key contract before saving the execution graph. Create separate scoped tasks for filter behavior, retry handling, and integrated validation/documentation, with explicit dependencies, acceptance and tests. Checkpoint findings and reconcile the request if evidence changes the plan.",
  "idempotency_key": "archive-export-request-01",
  "ai_optimized": true
}
```

Add the current `project_dir` only to the MCP call envelope; the JSON's descriptive content is portable. This is an illustrative domain, not a fixed architecture or task count.

CLI creation accepts the same narrative through `--description` and `--strategy`:

```bash
graphit task session create 'Export archived records with predictable retry behavior' \
  --description "$REQUEST_SPECIFICATION" --strategy "$REQUEST_STRATEGY" \
  --idempotency-key archive-export-request-01 --agent coordinator-a
graphit task session claim "$SESSION_ID" --agent coordinator-a --lease 2h
```

After investigation, save executable task specifications, checks and dependencies as described in the [Task module](../specs/task_module.md). Session context complements those task packets; it does not replace their detailed specification or allow implementing a whole project under one generic task.

New MCP tasks must resolve to a session. Pass `session_id`, inherit the parent task's session, or let Graphit infer the session currently coordinated by the same actor. Explicit and inherited associations must agree. Children share their parent's session; a task's association is immutable. An explicit nonterminal session allows separate workers to create tasks without taking the coordinator's claim. A terminal session accepts no new tasks.

For planned independent creations, use `graphit_task_batch` with `session_id` on each operation when needed. Parent/dependency IDs must already exist; create dependency stages in separate calls and inspect every result. Human CLI creation can remain unassociated when neither an explicit, inherited nor active-coordinator session applies; use `graphit task create ... --session "$SESSION_ID"` to associate it.

## Checkpoint and revise while working

The coordinator and task workers hold independent claims. Completing a worker's task does not close the session. Each worker records task-level evidence; the coordinator checkpoints independently meaningful outcomes in the session, referencing those tasks.

`graphit_task_session_checkpoint` requires `id`, the session `claim_token`, `summary` and `next_step`. Optional `problems`, `decisions` and `strategy` preserve the reasoning needed for continuation. Use descriptive evidence, not “working” or a log of every tool call. Checkpoint `strategy` records the observed approach in that checkpoint; replacing the session’s current strategy requires `revise`. All three context fields are strings, so each can contain structured Markdown.

A checkpoint JSON body for `graphit task session checkpoint "$SESSION_ID" - --claim-token "$SESSION_TOKEN" --agent coordinator-a`:

```json
{
  "summary": "The filter contract is implemented and its focused tests passed in the filter task. The retry task has reproduced duplicate delivery in its isolated fixture. The integrated user/API documentation check remains pending.",
  "problems": "The existing request identifier is generated after queue submission, so it cannot currently deduplicate submissions. No production data was changed during the reproduction.",
  "decisions": "Use the caller's request key at the submission boundary. Preserve the existing job identifier for retrieval; replacing it would break consumers. Record the exact validation contract in the retry task before implementation.",
  "strategy": "Finish the retry contract and implementation, then execute the integration task against both filter and duplicate-submission behavior.",
  "next_step": "Read the retry task's reproduction and submission contract, revise its specification with the agreed key validation, then implement and run its duplicate-delivery cases. Resume integration only when both prerequisite tasks are complete."
}
```

Use actual returned task IDs and source/test evidence in the real checkpoint. The same fields go directly into the MCP checkpoint call alongside the session ID, token and project scope.

When the user adds requirements, changes direction, or investigation invalidates the plan, first call `graphit_task_session_revise` with the latest `expected_revision`, `reason`, and replacement `title`, `description` or `strategy`. It preserves before/after specifications. Then reconcile affected task descriptions, checks and dependencies before executing the changed work. A checkpoint alone does not revise the current request specification.

CLI revision consumes a strict JSON patch:

```bash
graphit task session revise "$SESSION_ID" request-patch.json \
  --expected-revision "$SESSION_REVISION" --reason 'User added a CSV contract requirement' \
  --claim-token "$SESSION_TOKEN" --agent coordinator-a
```

`request-patch.json` contains only supplied replacement fields: `title`, `description`, `strategy`. Omitted fields remain unchanged. Read the current revision again after intervening mutations or a stale-revision error. Claims default to one hour; heartbeat/checkpoint/revise can renew without shortening a longer active lease. `graphit_task_session_heartbeat` renews coordination without inventing progress.

## Hand off and resume safely

Before stopping, preserve the request's actual state: completed and remaining work, blockers, decisions, evidence, and an exact continuation. Release task claims as appropriate through their own tools; releasing the session does not release other workers' tasks.

```bash
graphit task session release "$SESSION_ID" --agent coordinator-a \
  --claim-token "$SESSION_TOKEN" \
  --summary 'Filter task complete; retry contract resolved, implementation pending; integration waits on retry.' \
  --next-step 'Read the retry task, claim it with the worker identity, implement its saved contract and pass its checks before integration.'
```

The next coordinator reads the session and relevant tasks, claims the session using its own stable identity, and uses the newly returned token. It does not reuse the prior coordinator's token. Expiry and supported stop hooks preserve an open resumable request; hooks do not write fictional narrative or mark the work complete. Where a host does not expose a safe identity, use explicit handoff; the lease remains the fallback.

An unexpired claim whose owner is demonstrably unrecoverable can be recovered through `graphit_task_session_force_takeover`, requiring `confirm_id`, `expected_revision`, a reason, a positive replacement `lease` and a different new coordinator. CLI: `graphit task session force-takeover`. Read first; normal release/claim is preferable when the owner can cooperate.

## Close only after the request is resolved

Finish and validate each associated task. Cancel obsolete work explicitly with its reason; cancellation is not delivery evidence. Read the session's tasks and reconcile the final scope before `graphit_task_session_complete`, which requires the current coordinator token and a meaningful final `summary` mapping outcomes to evidence and explaining cancelled scope or limitations.

Complete and cancel both refuse an associated task still `open` or `in_progress`, or a completed task whose checks/dependencies/subtasks are no longer valid. Neither operation silently completes/cancels children. An analysis-only session without tasks can close with its descriptive result; do not invent tasks/checks to satisfy a count. A finished chat turn or disconnected MCP connection never means the session is complete.

CLI completion: `graphit task session complete "$SESSION_ID" --claim-token "$SESSION_TOKEN" --agent coordinator-a --summary "$FINAL_EVIDENCE"`. For an obsolete request, use `cancel` with the same ownership fields and `--reason`, after explicitly resolving all associated tasks. Session states are exactly `open`, `in_progress`, `completed`, `cancelled`.

## Diagnose rejected actions

| Result | Meaning and recovery |
|---|---|
| Task creation requires a session | Create/claim the described request or pass its known nonterminal ID. Do not create an empty automatic session or switch identities. |
| Unknown/conflicting session or parent | Read the session and parent; use the correct shared association. Existing tasks cannot move to another session. |
| Coordinator already claimed or stale token | Read current state. Coordinate release, wait for actual expiry, or use justified explicit takeover; never bypass fencing. |
| Stale revision | Reread, reconcile the request against the current state, then revise with that revision. |
| Session has nonterminal tasks | Inspect `task_list(session_id)` and finish/cancel that work explicitly before closing. |
| Request timed out | Read the current record/history before retrying. Retry creation with its original idempotency key; a repeated creation does not revise saved content. |
| Invalid JSON or unknown patch field | CLI revision/checkpoint accept one strict JSON object. Correct the input; do not silently drop unrecognized fields. |
| Invalid cursor | Restart with the intended filters/query or use the cursor's original parameters; never reuse it for a different session filter. |

[CLI reference](cli_reference.md#task) · [MCP tools](mcp_tools_reference.md#task-session-tools) · [Session contract](../specs/task_sessions.md).
