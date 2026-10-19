---
references:
  - target: {type: task, id: tsk-ea469}
    relation: implements
  - target: {type: task, id: tsk-3e62}
    relation: specified_by
  - target: {type: task, id: tsk-dd92}
    relation: requires_contract
  - target: {type: task, id: tsk-add4}
    relation: requires_contract
---

# Task Module Specification

Task is the deterministic PROJECT work record and coordination service. A Session holds an evolving
demand/strategy; Tasks hold executable outcomes, dependencies and checks. Neither a chat title nor
an agent's assertion substitutes for the authoritative state. This page describes the current
Rust CLI/MCP/HTTP/Browser contract; [native protocol](rust_task_protocol.md),
[MCP](rust_task_mcp.md) and [storage](native_task_store.md) contain detailed codecs/invariants.

## Identity, versions and storage

Task IDs and Session IDs are canonical nonzero ULIDs. A Task belongs to one Session and may have a
parent plus dependencies. PROJECT ID/version selects one authorized branch of the same
`<global>/projects/<project-id>/task/index.lance` dataset. Sessions, checks, comments, claims,
references, lifecycle history and searchable projection/embedding rows stay in that corpus.
Versions are native Lance branches, not copied stores. Worktrees with the same project/version
reach the same records; different versions remain isolated.

When valid Broker discovery enables S3, open the authoritative authorized `task` dataset remotely
with its restricted STSv3 credentials. No local mirror or silent local write after an auth/S3
failure is allowed. An already captured reader can retain a snapshot; a new operation observes
committed heads under the SDK contract. Credential renewal does not authorize an obsolete
snapshot indefinitely. [Storage layout](../architecture/storage_layout.md) defines maintenance.

## States and readiness

| State | Meaning |
|---|---|
| `open` | Retained work that may be ready or blocked by real prerequisites |
| `in_progress` | Active lifecycle state; current ownership still comes from the lease/fence |
| `completed` | Completion gates were satisfied and the completion operation committed |
| `cancelled` | Terminal cancellation with its recorded reason, not delivered functionality |

A claimed record has an authenticated owner, claim epoch, heartbeat/expiry and private participant
token. A read does not claim it; a stale agent or expired fence cannot mutate it. Readiness is
derived from dependencies/hierarchy and real state, never from a client-supplied badge.

Completed and cancelled descendants are terminal and do not by themselves block a parent. A
nonterminal descendant with pending work still blocks according to the hierarchy. Cancelling a
child does not prove the parent's own required functionality; its active checks must still pass.
Session/Now progress excludes cancelled tasks from the denominator and counts completed work.
Administrative historical leftovers are not a percentage of delivered functionality.

## Specification and acceptance

Task creation requires title, detailed description, explicit type/priority, Session, idempotency
key, and at least one acceptance plus one test check. Optional schedule dates feed Gantt; do not
invent dates from priority. Parent/dependency links must remain valid and acyclic. Repeating a
creation key returns the existing creation outcome, not a specification update.

Revision changes the authored specification with a reason and retained before/after history.
Checkpoints/progress do not revise it. Unknown fields are rejected; omitted fields preserve the
current value, while explicit null clears only fields whose schema permits it. Authored reference
patches preserve on omission, clear with `[]`, and replace with a complete valid list.

A check records its actual evidence and pass/fail status. Supersede an obsolete check with its
reason and optional replacement; never mark a no-longer-required benchmark or mechanism as
passed. Completion requires current gates, dependencies/descendants, flags and ownership to
permit it. An incomplete functional result cannot be closed merely to reduce a task count.

## Claims, leases and concurrency

Claim operations return `{record,token}` only to that participant. Keep tokens private and out of
Git, shared docs, task descriptions and logs. Task and Session claims are separate; neither token
can be used for the other record. There is no client-chosen actor or generic force-takeover grant.

Most owned mutations use `target:{id,claim:{token,expected_revision}}` plus positive `lease_ms`.
Completion uses its advertised `{id,claim}` shape instead. A stale revision, invalid fence,
missing permission, cancellation or deadline is an error; reread and reconcile before retry.
Heartbeat renews the real lease. Release preserves summary/next action and relinquishes ownership.
Session coordination and Task execution are separate lifetimes.

Cooperating writers use the native transaction/CAS path. A lost response can follow a confirmed
commit; it is not evidence of rollback. Observe state and reported effects before retrying a
mutation. Creation/comment idempotency keys must remain stable for the same logical operation.

## Reads, history, query and search

Current `get` returns the record, creation, last event, generation and first-page cursors.
History/comments are null when unloaded. Request each needed section explicitly with positive
page size and its opaque cursor; a loaded empty page differs from null. Task history, Session
history and Task comments use distinct pinned cursors. Continue through EOF for a complete audit.
One current read does not load all historical events.

Catalog discovery supports typed filters and pagination. `schema` supplies actual query tables
and types before `query` selects columns from Tasks or Sessions. A projection is not arbitrary SQL
or a grant to another dataset. A cursor must retain its original project/version/filter/order
and snapshot binding; current authorization remains checked.

Search defaults to FTS and supports semantic/hybrid modes. Its corpus contains current searchable
Task specifications, retained check evidence and comments under the implemented projection;
Session strategies/checkpoints and historical specifications are not an implicit search corpus.
Inspect returned match locations and exact current/history pages for evidence.

Background jobs persist nullable vectors alongside current public projections, reuse compatible
identity/content/model fingerprints, and attach only to still-current inputs. Claims, heartbeats
and status metadata that leave the embedded input unchanged do not cause inference. Partial
published coverage is usable; hybrid retains lexical-only candidates. FTS needs no model.
`modules.embedding=false` prevents semantic work while preserving CRUD and textual search;
requested semantic failures remain explicit. See [Task search](native_task_search.md).

## Commit metadata and Git snapshots

`Task.commit` is an optional full lowercase Git object ID (40 or 64 hexadecimal characters).
After an authorized commit or amend, resolve the actual HEAD and associate it with included
Tasks through `graphit_task_commit_set` / `task set-task-commit`. Use the current revision and
live claim when owned. An unowned completed/cancelled Task can receive this metadata without
reopening it or changing checks/status. Null clears a wrong association. Metadata alone does
not create a Git commit, capture a snapshot or authorize push.

Product Git hooks independently tag Task/PROJECT Memory snapshots for each observed commit,
restore them on checkout and merge record families with explicit conflicts. USER Memory is not
versioned this way. There is no atomic transaction across Git and both stores, and historical
state cannot be invented for pre-install commits. See [Git records](git_records.md).

## Current interfaces

| Consumer | Contract |
|---|---|
| CLI | `graphit task <operation>` with operation-specific typed flags and repeatable list options |
| Root MCP | `graphit_task_*` tools use optional `selection` and operation-specific `input` |
| Native HTTP | Authenticated `/api/tasks/native/<operation>` endpoints use the typed service |
| Browser | Verified actor, Task/Session detail, Gantt, search and explicit paged audit reads |
| Role tools | Catalog/admission filtered by genuine Role grant; no Runtime impersonation |

CLI and HTTP remain JSON. Structured MCP defaults to TOON; `ai_optimized:false` requests JSON.
`isError`, protocol validation, valid `not_found` and domain failures are distinct. Tool names and
schemas are in the [current reference](../guides/mcp_tools_reference.md); no generic batch,
`task export`, `task session ...` subtree, Session search or force-takeover command is advertised.
A UI download assembled from paged records is not a separate `graphit_task_export` tool.

## User workflow

1. Find the existing Session for the evolving demand; create a new one only for a different demand.
2. Capture requirements, consumers, decisions, boundaries and real validation before execution.
3. Reuse or create a Task with its Session and meaningful acceptance/test checks.
4. Claim ready work and preserve the returned fence privately.
5. Reconcile changed requirements/checks, implement, validate the affected consumers and record
   concrete evidence. Do not repeat unchanged proof solely because caches were removed.
6. Complete only after real gates pass; otherwise checkpoint/release with a concrete next action.
7. Complete Session coordination after its requested outcomes are actually reconciled.

The [Session guide](../guides/task-sessions.md) supplies exact envelopes and examples.

## Implementation map

`graphit-records/src/task_store/` owns models, gates, writer, reads, search and histories.
`graphit-app/src/task_entry.rs` owns CLI spelling; `task_server/catalog.rs` owns operations and
`task_server/browser/` adapts verified Web requests. `ui/src/components/task/` renders state and
Gantt. `graphit-records/src/git_records.rs` owns record-family Git integration. These consumers
must preserve domain gates rather than reproduce a second state machine in the UI.
