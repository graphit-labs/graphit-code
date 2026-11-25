# {{ brand_display }} Task

Task is the durable record of a demand: what was asked (the Session), how it will be delivered (Tasks with typed checks), what happened (progress, checkpoints, comments and check evidence) and which commit delivered it. It exists so that any agent can resume after interruption, compaction or handoff from the records alone, so write every description and checkpoint for a reader who never saw this conversation.

Roles: the coordinator owns Task claims, acceptance, checkpoints and lifecycle. A delegated role reads the session and Task IDs it was given, reports anchored findings, and returns a draft for any write that needs a claim; it never creates, claims, revises, completes, cancels or releases coordination state. Use the authenticated MCP catalogue; never edit Task tables or substitute another server or a CLI for a missing operation.

{% include "shared/persistence-boundary.md" %}

## Workflow

1. Recover: find the session for this demand and read its latest checkpoint and linked Tasks. Read [references/session.md](references/session.md) before starting, resuming or handing off coordination.
2. Decide: continue the same evolving demand in its session, revise it when scope changed, and create a new session only for a different demand. The host chat ID is not a Task Session ID.
3. Specify: investigate through AST, Knowledge, Memory and Task, then save the plan as dependency-ordered Tasks with acceptance and test checks. Read [references/planning.md](references/planning.md) for a multi-outcome plan, [references/worked-feature.md](references/worked-feature.md) for one filled feature and [references/worked-system.md](references/worked-system.md) for several deliveries.
4. Execute: claim a ready leaf, implement, record check evidence and progress. Read [references/execution.md](references/execution.md) before a leaf, review or handoff.
5. Checkpoint: at each meaningful outcome, checkpoint the Session with evidence, decisions and the exact next step.
6. Close: complete Tasks whose checks passed, associate the delivered commit, and complete the Session once every linked Task is terminal; cancel explicitly when the demand is withdrawn.

Claim nothing merely to read or wait, and reuse context you already recovered.

## Tool map

Use only operations advertised by the current server, and read a tool's input schema before its first use; a name on another server does not establish this server's API.

| Need | Tool |
| --- | --- |
| Find or read a session | `{{ mcp_tool_task_session_list }}`, `{{ mcp_tool_task_session_get }}` |
| Open and coordinate a session | `{{ mcp_tool_task_session_create }}`, `{{ mcp_tool_task_session_claim }}`, `{{ mcp_tool_task_session_heartbeat }}`, `{{ mcp_tool_task_session_checkpoint }}`, `{{ mcp_tool_task_session_revise }}` |
| Finish or hand off a session | `{{ mcp_tool_task_session_complete }}`, `{{ mcp_tool_task_session_cancel }}`, `{{ mcp_tool_task_session_release }}` |
| Read Tasks | `{{ mcp_tool_task_get }}` (one record), `{{ mcp_tool_task_list }}` (catalogue filtered by status, session_id, flagged, blocked), `{{ mcp_tool_task_query }}` with `{{ mcp_tool_task_schema }}` (projected columns), `{{ mcp_tool_task_search }}` (text) |
| Create and own a Task | `{{ mcp_tool_task_create }}`, `{{ mcp_tool_task_claim }}`, `{{ mcp_tool_task_heartbeat }}`, `{{ mcp_tool_task_release }}` |
| Record work | `{{ mcp_tool_task_progress }}`, `{{ mcp_tool_task_comment_add }}` (kind `note`, `decision`, `problem`, `lesson` or `knowledge`), `{{ mcp_tool_task_check }}` |
| Change the plan | `{{ mcp_tool_task_revise }}`, `{{ mcp_tool_task_update_links }}`, `{{ mcp_tool_task_check_supersede }}`, `{{ mcp_tool_task_flag }}` |
| Finish a Task | `{{ mcp_tool_task_complete }}`, `{{ mcp_tool_task_cancel }}`, `{{ mcp_tool_task_commit_set }}` |
| Follow relations | `{{ mcp_tool_task_references }}` |

There is no batch create, session search, generic export or force takeover. If a needed operation or reference-query capability is absent, record the exact requirement and producer gap in the current work record and coordinate its resolution before dependent work; never claim an unavailable mutation happened.

## Wire contract

Native Task calls use an optional `selection` and, when the operation needs it, an `input` object; domain fields belong inside them. The optional strict boolean `ai_optimized` belongs at the argument root: omitted/`true` returns TOON text, without duplicate JSON `structuredContent`; `false` returns JSON for machine consumers. There is no caller-selected `agent_id`; the authenticated caller is the actor.

Selection DATA identifies an already retained PROJECT/version; it grants no access. Prefer captured `selection.project_id` and `selection.project_version`. A directory selector is `selection.project_dir`, one Unicode path string; echo the captured path exactly. Persist logical project identity and repository-relative locators, not a host's checkout path.

- Create: `input.checks` is an array of `{kind:"acceptance"|"test",text}` with at least one of each; `task_type` and `priority` are explicit; `session_id`, `title`, `description` and `idempotency_key` are required; `parent_id`, `depends_on` and `schedule` are optional.
- Claim: `input.id` and a positive integer `input.lease_ms` accepted by the real policy. A claim returns `result.record` and the private `result.token`; keep the token only in live private state, never in descriptions, links, artifacts or handoffs.
- Owned mutations take `input.target={id,claim:{token,expected_revision}}`; heartbeat, progress, check, comment, revise, flag and link operations also require `lease_ms`. Completion takes `input={id,claim}`. Read each operation's schema rather than applying one envelope to everything.
- Replies: reads use `operation/result`, writes `kind/result`. Session creation returns `result.id`; Task creation returns `result.specification.id`. Task revisions come from `record.specification.revision`, Session revisions from `record.revision`. Read the returned record after each mutation and use its current revision; never guess an increment.

## Retrieval

Known IDs: `{{ mcp_tool_task_get }}` or `{{ mcp_tool_task_session_get }}` with only `input.id` return the current record, `creation`, `last_event`, `generation` and start cursors; `history` and `comments` stay null (unloaded). Request history with `input.history={page_size,cursor}` (Task also `input.comments`), starting from the returned start cursor and continuing with `next_cursor`. A loaded empty list differs from an unloaded null section. Request only the pages that answer the question; a complete audit traverses through EOF at the same pin.

Discovery: `{{ mcp_tool_task_search }}` searches current Task specifications, retained check evidence and comments with `input.text`, `input.filter`, positive `input.page_size` and `input.top_k`; optional `input.mode` is `fts` (model-free default), `semantic` or `hybrid` when the schema offers it. Semantic modes need a configured embedding service and fail explicitly rather than falling back. Session descriptions, strategies and checkpoints are outside this corpus: find sessions through `{{ mcp_tool_task_session_list }}`. Follow search hits with exact reads.

Catalogue: `{{ mcp_tool_task_list }}` with `input.filter` (`status`, `session_id`, `flagged`, `blocked`, `query`) and `page_size` shows, for example, open unblocked work in a session. For column projections read `{{ mcp_tool_task_schema }}` first, then `{{ mcp_tool_task_query }}` with `input.selection` (`table` `tasks` or `sessions`, `status` or null, optional typed `ids`, and for Tasks `session_id`), explicit `input.columns`, positive `input.page_size` and `input.top_k`; `result.columns` orders each `result.rows` entry. Continue any cursor with unchanged selection and parameters. A `no_corpus` state differs from a published empty result. Cursors are DATA, never authority.

Recall prior investigations, decisions and evidence whenever a new doubt arises during the work, not only at the start; stop once the gap is closed.

## References and commits

{% include "shared/references/agent-contract.md" %}
{% include "shared/references/agent-example.md" %}
{% include "shared/references/query-contract.md" %}

Creation, comments and supported owner mutations expose `input.references`: omitted preserves authored relations, `[]` clears them, and a complete array replaces them. Each entry requires `target:{type,id}` and `relation`; optional `field`, `target.scope`, `target.scope_id` and `target.context` are strings. Claims and heartbeats have no reference patch.

After an authorized Git commit or amend succeeds, resolve the full commit ID (`git rev-parse HEAD`) and update `Task.commit` on the Tasks whose changes are actually included using `{{ mcp_tool_task_commit_set }}`. This also applies to completed Tasks; preserve their status and check evidence. Never infer a commit from a planned command or treat this instruction as authorization to commit or push.

For an explicitly withdrawn demand, resolve linked Tasks individually before calling `{{ mcp_tool_task_session_cancel }}` with the current revision, reason and live coordinator claim when owned. Do not complete a cancelled demand or silently cascade cancellation. A terminal retry must omit claim and references.
