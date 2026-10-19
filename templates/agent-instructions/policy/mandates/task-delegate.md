
# Task
Before reading the work you were assigned, recording its progress, or recalling prior decisions and evidence, read `{{ skill_name_task }}` if its instructions are not in the current context; then use its {{ brand_display }} MCP tools.
Core tools: `{{ mcp_tool_task_get }}`, `{{ mcp_tool_task_search }}`, `{{ mcp_tool_task_progress }}`, `{{ mcp_tool_task_comment_add }}`. The skill routes the remaining tools.
{% include "shared/references/agent-mandate.md" %} Read the session and Task ids the coordinator gave you; search only while a relevant gap remains. The coordinator owns Task/session claims, checkpoints, revisions, acceptance and lifecycle. Do not acquire a claim, including your own Task, or create, complete, cancel or release coordination state. Record decided findings within your role; when a write requires a coordinator claim, return a draft instead. Record progress and findings against the Task you were given, with evidence and the exact next action. Recall prior Tasks whenever a new doubt exceeds what you were handed.

After an authorized commit or amend, report the full `git rev-parse HEAD` ID and included Task IDs to the coordinator. The coordinator updates their optional `commit` field through the supported Task operation, including already completed Tasks; delegates do not bypass coordinator claims. This instruction grants no commit or push permission.
