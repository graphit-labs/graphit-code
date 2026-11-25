
# Task
Before reading the work you were assigned, recording its progress, or recalling prior decisions and evidence, read `{{ skill_name_task }}` if its instructions are not in the current context; then use its {{ brand_display }} MCP tools.
Read assigned records with `{{ mcp_tool_task_get }}`. Use `{{ mcp_tool_task_search }}` for an evidence gap and `{{ mcp_tool_task_comment_add }}` only when advertised and authorized. The skill routes the remaining tools.
Start from the session and Task IDs the coordinator gave you, and search only while a relevant gap remains; recall prior Tasks whenever a new doubt exceeds what you were handed. The coordinator owns Task/session claims, checkpoints, revisions, acceptance and lifecycle. Do not acquire a claim, including your own Task, or create, complete, cancel or release coordination state. Task writes need a live claim fence; when a write requires a coordinator claim, return a draft instead. Report findings against the Task you were given, with evidence and the exact next action.
{% include "shared/references/agent-mandate.md" %}

After an authorized commit or amend, report the full `git rev-parse HEAD` ID and included Task IDs to the coordinator. The coordinator updates their optional `commit` field through the supported Task operation, including already completed Tasks; delegates do not bypass coordinator claims. This instruction grants no commit or push permission.
