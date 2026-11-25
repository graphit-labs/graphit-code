---
title: "Coordinate and resume Task sessions"
description: "Preserve a complete request, decisions and progress across agents using durable Task sessions."
content-type: guide
audience: developers
keywords:
  - task
  - sessions
  - handoff
references:
  - target: {type: task, id: tsk-678a}
    relation: implements
  - target: {type: task, id: tsk-13af}
    relation: implements
---

# Coordinate work with Sessions and Tasks

A Session preserves one evolving demand across agent/chat interruptions. Its description states
what to deliver and its strategy explains execution. Tasks represent independently verifiable
outcomes; progress records evidence and next action. This is the current Rust workflow, not the
retired `task session ...` interface.

## Selection and identity

Use the current project or explicit `--project-dir` / `--project-id`, and `--project-version` only
for another retained version. MCP uses the optional `selection` object. These identify records;
the verified caller and current PROJECT permissions authorize the operation. There is no `--agent`
flag to manufacture the actor.

CLI fields are named flags; list options are repeatable. The CLI builds the MCP envelope internally:

```sh
graphit task session-heads
graphit task get-session --id '<SESSION_ID>'
graphit task get-task --id '<TASK_ID>'
```

`<SESSION_ID>`, `<TASK_ID>` and private tokens below are placeholders: substitute actual returned
values; do not send angle-bracket examples as IDs. [MCP input schemas](../reference/mcp-input-schemas.json)
contain every required field, enum and nested shape.
The JSON examples below are MCP tool arguments, not CLI syntax. CLI operations
use the named flags in the [complete option reference](../reference/cli-options.md).

## Create or resume a demand

Read Session heads and the selected Session before deciding whether a demand is new. Continue the
same Session when requirements evolve; revision changes its specification, checkpoint records
execution. Creating another Session is not a substitute for reading current work.

```json
{
  "selection": {"project_id":"<PROJECT_ID>","project_version":"main"},
  "input": {
    "idempotency_key":"deliver-parser-support",
    "title":"Deliver custom parser support",
    "description":"Allow configured extension-to-image routes; preserve authorization and fallback.",
    "strategy":"Inspect existing producer, implement its consumers, validate the Docker contract."
  },
  "ai_optimized":false
}
```

Send this to `graphit_task_session_create`. Creation returns `result.id`. Reusing the same
idempotency key does not revise an existing request. Keep meaningful durable requirements in
Session/Task; keep credentials, personal details and transient impressions out of shared records.

## Claim coordination and ready work

`graphit_task_session_claim` and `graphit_task_claim` each accept:

```json
{"input":{"id":"<RETURNED_ID>","lease_ms":600000},"ai_optimized":false}
```

Each returns `result.record` and its own private `result.token`. The lease here is a caller choice,
not a product-wide quota. Use Task `record.specification.revision` or Session `record.revision` as
appropriate. A Task token cannot mutate a Session. Existing live ownership must be respected;
there is no force-takeover capability in this catalog.

Create a Task with `session_id`, explicit `task_type`, `priority`, detailed description, stable
idempotency key and at least one `acceptance` and one `test` check. Optional `parent_id`,
`depends_on` and dates express real hierarchy/dependencies/schedule, not administrative ceremony.
Only claim ready work. Pass the returned IDs to delegates; coordinator-owned lifecycle stays
separate from read-only impact/recall.

## Progress, specification changes and checks

Owned progress/checkpoint/revise operations generally accept:

```json
{
  "input":{
    "target":{"id":"<ID>","claim":{"token":"<PRIVATE_TOKEN>","expected_revision":3}},
    "lease_ms":600000,
    "summary":"Implemented parser route; configuration and consumer tests passed.",
    "next_step":"Validate the affected image contract."
  },
  "ai_optimized":false
}
```

This is the progress/checkpoint shape. Revision instead requires `reason` and supported replacement
fields; do not send progress fields to revision. Superseding a check uses its actual `check_id`,
reason and optional replacement. `verify-task-check` records actual evidence, not planned tests.
Use each operation's schema; completion and commit metadata have different envelopes.

Re-read revisions after writes and concurrent changes. Stale revision/fence, authorization,
cancellation and deadline failures are meaningful; never fabricate a new actor or bypass the
writer. Omitted reference patches preserve relations, `[]` clears explicit relations, and a
complete typed list replaces them. Mentioning an ID only in prose creates no relation.

## Read complete evidence deliberately

Current reads return `history:null` and, for Tasks, `comments:null` unless requested. They provide
creation/current/last-event and first-page cursors. To load history, send
`history:{page_size:20,cursor:<history_start_cursor>}` inside input. Comments use their own section
and cursor. Continue through `next_cursor` until EOF for an audit/export. Do not reuse Task,
Session and comment cursors interchangeably or change selection while continuing.

Typed query uses the advertised schema and columns; search finds current Task public content,
check evidence and comments. It does not search Session strategies or every old specification.
FTS is model-free; semantic/hybrid use compatible vectors already published incrementally.

## Finish or hand off

Completion uses `input:{id,claim:{token,expected_revision}}`, not the progress target envelope.
The parent must meet its own checks and dependency/hierarchy gates. Completed/cancelled children
are terminal; cancellation does not certify a required feature. Cancelled tasks do not inflate
the Session progress denominator. Complete the Session only when its actual demand is delivered.

When stopping unfinished work, release Task and Session separately with the real current fence,
summary and next step. A closed chat, lost process or expired lease does not complete the demand.
If a mutation timed out, inspect current/history before retrying; it may already have committed.

After an authorized commit/amend, resolve the full actual SHA and use `graphit_task_commit_set`
(or CLI `task set-task-commit`) for included Tasks. This preserves status/checks and does not
authorize Git push. Product [Git snapshots](../specs/git_records.md) separately preserve Task/Memory
state at commits.

## Common failures

| Result | Action |
|---|---|
| Not found / no corpus | Check actual ID, PROJECT/version and authorization; do not recreate a store to hide it |
| Stale revision | Reread and reconcile with the latest version |
| Invalid/expired claim | Recover coordination through the supported real claim lifecycle |
| Blocked completion | Inspect nonterminal descendants/dependencies, active checks and flags |
| Invalid cursor | Resume the original pinned parameters or start a new read |
| Delivery failure after write | Inspect reported committed/uncertain effects before retry |
| Unavailable tool | Inspect the current catalog; no batch/export/force-takeover alias is implied |

[Task contract](../specs/task_module.md) · [CLI](cli_reference.md#source-records-and-live-views) ·
[MCP](mcp_tools_reference.md#task-session-tools).
