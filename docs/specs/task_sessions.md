# Task Sessions contract

A Session records one evolving demand across agent/chat interruptions. It retains title,
description, strategy, immutable creation, current revision/status, ownership and the latest
checkpoint. Tasks reference its canonical ID. The current Rust wire contract is in
[rust_task_protocol](rust_task_protocol.md); [the user guide](../guides/task-sessions.md)
contains actual CLI/MCP envelopes.

## Storage and selection

Sessions share the authorized PROJECT Task dataset and native Lance branch with their Tasks,
including direct S3 when selected. They do not have a separate copied database, Session text-search
corpus or export format. Current/queryable fields and immutable events are encoded by the current
schema/producer; obsolete SQL-like `task_session_checkpoints` table examples are not another public
storage contract. Query `graphit_task_schema` before typed projection queries.

Project/version/directory selectors identify retained data; they grant no authority. Current
authentication and Task-module read admission authorize Session reads. Creation requires the Task
operation, while claims, revisions, checkpoints, release and completion require CoordinateTask
admission. USER identity cannot be chosen through a Session field. Cross-project reads require
that project's own admission. Paths persist as Unicode strings, never `os/units`.

## Operations and states

| Operation | Preconditions / effects |
|---|---|
| create | Canonical nonempty title/description/strategy and stable idempotency key; open initial record |
| claim | Actual available record and positive lease; returns its private token and current revision |
| heartbeat | Live owner/fence/revision; renews actual lease |
| revise | Live owner, lease, reason and real supported specification/reference change; preserves immutable before/after history |
| checkpoint | Live owner and positive lease; summary/next_step records progress, not a specification replacement |
| release | Live owner/current revision; records handoff and clears ownership |
| complete | Live owner/current revision and real linked Task closure gates; preserves all history |

There is no Session search, batch mutation, force-takeover, cancel or delete operation in the current
Root catalog. The shared state type can encode cancelled historical data without advertising a new
Session cancellation API. Never infer a tool from another runtime's schema or from a state enum.

Task and Session use `open`, `in_progress`, `completed`, `cancelled` states. A stale/expired token
cannot write, even before maintenance observes it. Distinct targets have their own claims; a
one-active-Task executor guideline is not a global user quota. Tokens appear only in participant
claim responses/private client state, never ordinary reads or history.

## Specification, progress and references

Revision accepts only its advertised replacement fields and `reason`; omission preserves and null
is rejected for required text. It must actually change the specification/references. Checkpoint
accepts summary/next_step and lease; there are no separate wire `problems`, `decisions` or `strategy`
fields on that operation. Include necessary evidence/rationale in summary; use revise to change
current strategy. Unknown fields fail instead of being discarded.

Typed references have target type/ID, relation and optional qualifiers. Omission preserves authored
relations, `[]` clears and a complete list replaces. A prose mention creates no database relation.
Structural Task-to-Session links remain managed by the domain model.

## Reads, pagination and progress

Current `session_get` returns Session, creation, last event, generation and first history cursor.
History is null unless explicitly requested. A loaded empty page is different from null. Continue
with the actual opaque Session history cursor and unchanged selection until EOF for full evidence.
Current reads do not load every lifecycle event or all linked Task dossiers.

Session heads derive completed/total counts from linked Tasks. Cancelled Tasks remain retained and
authorized in the inventory but are excluded from the denominator; only completed Tasks increment
the completed count. With no non-cancelled Tasks, counts are 0/0. These counts are workflow state,
not a delivery percentage or migration estimate.

## Closure, interruption and recovery

Nonterminal linked work prevents completion according to the current gates. Cancelled children are
terminal, but cancellation does not certify delivery of a required outcome. The demand must be
reconciled before the coordinator completes the Session. Closing a chat or completing one Task does
not implicitly close coordination.

On interruption, checkpoint/release with result/evidence, real blocker and exact next action. A
replacement reads the same demand/current strategy plus relevant history and resumes normally.
After timeout, observe current state before retrying a mutation; it may have committed. Native
post-event hooks request asynchronous rescans and do not fabricate completion. Git snapshot/merge
preserves Session records as part of the Task corpus, with the same conflict/history rules.
