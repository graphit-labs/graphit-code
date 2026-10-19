# {{ brand_display }} Task

This reference describes coordinator-owned workflows. For delegated roles, apply the current protocol:
{% include "protocol.md" %}

Task preserves the demand, plan, native records, checks and work history. Use the authenticated MCP catalogue; never edit its tables or substitute another server or a CLI to bypass a missing operation. Under this protocol, the coordinator owns Task claims; delegated roles receive IDs and return anchored findings or decided drafts.

{% include "shared/persistence-boundary.md" %}

## Use the current wire contract

Read the selected tool's advertised input schema before calling it. A name on another server does not establish this server's API. Native Task calls use an optional `selection` and, when the operation needs it, an `input` object. Domain fields belong inside those objects. The optional strict boolean `ai_optimized` belongs at the argument root: omitted/`true` returns TOON text, without duplicate JSON `structuredContent`; `false` returns JSON for machine consumers. It never belongs inside `input`. There is no caller-selected `agent_id`.

Selection DATA identifies an already retained PROJECT/version; it grants no access. Prefer captured `selection.project_id` and `selection.project_version`. A directory selector is `selection.project_dir`, encoded by Core as one Unicode path string. Echo the captured path exactly; non-Unicode native paths fail explicitly. Do not reconstruct authority from a path or URI. Persist logical project identity and repository-relative locators, not a host's checkout path.

Creation uses `input.checks`, an array of `{kind:"acceptance"|"test",text}`, with at least one of each. `task_type` and `priority` are explicit fields. Claims use `input.id` and positive integer `input.lease_ms`, with a caller-requested lease accepted by the real policy. The authenticated caller determines the actor. Owner mutations require the target’s authenticated owner and current private token/fence, lease and revision; never manufacture an actor or use another target’s token.

Read replies use `operation/result`; writes use `kind/result`. Session creation returns `result.id`; Task creation returns `result.specification.id`. A participant claim returns `result.record` and the private `result.token`. Keep that token only in live private state. Task revisions come from `record.specification.revision`; Session revisions from `record.revision`.

Many owner mutations take `input.target={id,claim:{token,expected_revision}}`; heartbeat/progress/revise/check operations also require `lease_ms`. Completion instead takes `input={id,claim}`. Read each operation's schema rather than applying one mutation envelope to everything. Read the returned record after each mutation and use its current revision. Tokens are absent from normal records, heads and history; they are not Auth grants.

## Session and retrieval

Read [references/session.md](references/session.md) before starting/resuming coordination or handing it off. Continue the same evolving demand across turns and compaction. Create a new session only for a different demand. The host chat ID is not a Task Session ID. Every agent-created Task carries the returned `session_id`.

Known IDs use `{{ mcp_tool_task_get }}` or `{{ mcp_tool_task_session_get }}`. Native `{{ mcp_tool_task_list }}` uses its advertised catalogue filter/page/cursor; `{{ mcp_tool_task_session_list }}` reads Session heads. Recover current state first; request relevant Task history/comment pages when prior evidence or decisions are needed. Reuse sufficient context; do not create a Task merely to read or wait.

`{{ mcp_tool_task_get }}` with only `input.id` returns current `task`, `creation`, `last_event`, `generation`, `history_start_cursor` and `comments_start_cursor`; `history` and `comments` are null (unloaded). `{{ mcp_tool_task_session_get }}` with only `input.id` returns current `session`, `creation`, `last_event`, `generation` and `history_start_cursor`; `history` is null (unloaded). Neither selected current read returns the complete lifecycle event list. `last_event` is null at revision one.

Request needed Task or Session history with `input.history={page_size,cursor}`; Task also supports `input.comments={page_size,cursor}`. `page_size` must be positive and `cursor` is optional. Use the captured `history_start_cursor` or Task `comments_start_cursor` for the first explicit page at that generation, then the section's `next_cursor`. A loaded empty `history.events=[]` or Task `comments.comments=[]` differs from a null, unloaded section.

For historical recall, request only relevant pages. `history.before` is the validated predecessor Task or Session for the page's first diff, so a page can replace an earlier one without retaining all prior events. Task and Session cursors are distinct and stay bound to the captured record/publication, including authorized inherited-version and branch-provenance checks. An explicitly required complete audit/export must traverse history through EOF at that same pin, plus comments for Task; current state or a single page does not prove completeness.

Cursors are DATA, never authority. Current Auth, PROJECT/version and corpus identities are rechecked; the SDK snapshot binding supplies no client-selected URI or branch authority.

Before querying record properties, use `{{ mcp_tool_task_schema }}` to read the current tables and permitted column types. `{{ mcp_tool_task_query }}` requires `input.selection` with `table` (`tasks` or `sessions`) and optional typed `ids` or `status`; Tasks also permit `session_id`. Supply explicit `input.columns` and positive `input.page_size` and `input.top_k`. The outer `selection` retains PROJECT/version selectors. Query returns `operation/result`: `result.columns` gives the order of values in each `result.rows` entry. Continue with the opaque `result.next_cursor` as `input.cursor` and the same project, version, filters, columns and limits. A `no_corpus` state differs from a `published` result with empty rows; neither proves continuity with another store. Query is a typed native read, not arbitrary SQL or caller-supplied authority.

When advertised, `{{ mcp_tool_task_search }}` discovers current Task specifications, retained check evidence and comments. Send `input.text`, `input.filter`, positive `input.page_size` and `input.top_k`. Optional `input.mode` selects `fts` (the model-free default), `semantic` or `hybrid`; use only modes present in the current tool schema. Semantic modes query compatible vectors already published while background incremental embedding continues; unchanged compatible public content reuses its vector. Hybrid also retains lexical candidates without vectors. A configured embedding service is required for semantic modes; service failures are explicit, not silent FTS fallback. Continue with the returned `input.cursor` and the same selection, mode and query. Session descriptions, strategies, checkpoints and old specification snapshots are outside this corpus. Use exact Task reads after discovery; search results do not grant access or replace authoritative history.

Only use operations advertised by the current server. A native batch, session revision, search, removal or takeover operation must not be assumed from another API. If a necessary operation or reference-query capability is absent, preserve the exact requirement and producer gap in the current work record and coordinate its resolution before dependent work. Do not claim an unavailable mutation happened.

## Define work before implementation

Read [references/planning.md](references/planning.md) for a material multi-outcome plan, [references/worked-feature.md](references/worked-feature.md) for one filled feature, and [references/worked-system.md](references/worked-system.md) for several deliveries. These are authoring models, not extra wire fields.

Investigate through current AST, Knowledge, Memory and Task evidence. Save outcome, journeys, singular requirements, data/error/authority contracts, decisions, unresolved questions and execution boundaries before coding. Each independently owned leaf needs prerequisites, compatible file ownership, a complete description, typed acceptance/test checks and a concrete verification/documentation plan. A small fix can use one packet.

Create parents and prerequisites first; use their returned IDs in subsequent native calls. Without an advertised batch, submit ordered calls and inspect each result. Planning can finish while implementation stays open. Read back the graph and requirement → producer → check → integration/documentation map; repair gaps, stale contracts, overlaps and dependency cycles before affected implementation.

## Execute and close

Read [references/execution.md](references/execution.md) before a leaf, review or handoff. Claim only ready work, keep one active Task per executing agent as an operational workflow, and consume real prerequisite results. This is not a per-user or per-actor quota: distinct targets can have independent claims for the same authenticated actor. On changed scope reconcile specifications/checks before incompatible edits. An unavailable producer is a concrete prerequisite, not permission for a mock grant or invented API.

The coordinator owns acceptance, evidence, checkpoints and lifecycle. Report real commands, observations, failures and unrun cases. A fixture or serialization test proves only its boundary; it does not prove an installed host or model received context. Every code unit checks maintained docs; every docs unit checks implementation. Use proportionate fast/long regression checks and verify required failure gates without running a push.

Progress preserves result/evidence, decisions, limitations and exact next action. Renew actual leases where needed; a read or a hook does not establish ownership. Supersede an obsolete check with reason and an appropriate replacement; never pass it as a workaround. Complete a Task only after its active checks and actual gates pass. Explicitly complete the Session only after all requested delivery/integration outcomes are reconciled. When stopping unfinished work, checkpoint and release the coordinator's own claims.

## References

{% include "shared/references/agent-contract.md" %}
{% include "shared/references/agent-example.md" %}
{% include "shared/references/query-contract.md" %}

Those shared relation rules describe required persisted relationships. Creation, comments and supported owner mutations expose `input.references`: omitted preserves authored relations, `[]` clears them, and a complete array replaces them. Each entry requires `target:{type,id}` and `relation`; optional `field`, `target.scope`, `target.scope_id` and `target.context` are strings when supplied. Null, nonsequence and unknown or malformed entry fields are rejected. Claims and heartbeats have no reference patch. Submit only non-sensitive confirmed project relations; a typed target is DATA, not access or a grant. If a necessary reference-query capability is absent, preserve that producer gap and pending readback; prose alone creates no relation.

After an authorized Git commit or amend succeeds, resolve the full commit ID (`git rev-parse HEAD`) and update `Task.commit` on the Tasks whose changes are actually included using `{{ mcp_tool_task_commit_set }}`. This also applies to completed Tasks; preserve their status and check evidence. Never infer a commit from a planned command or treat this instruction as authorization to commit or push.
