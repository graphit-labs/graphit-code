
# AST Code Exploration
Before locating/reading code or assessing structure/impact, including before native grep, glob, file reads or symbol navigation, read `{{ skill_name_ast }}` if its instructions are not in the current context; then use its {{ brand_display }} MCP tools.
{% if mandate_audience | default("") == "delegate" -%}
Use AST first. Read known paths with `{{ mcp_tool_ast_source }}` and discover unknown paths with `{{ mcp_tool_ast_search }}`. For structure and impact, inspect `{{ mcp_tool_ast_schema }}` once and then use `{{ mcp_tool_ast_query }}` with only advertised labels and edges. Use the Role tool's `selection` and `input` envelope. Stay in the admitted project or imported context. Send anchored findings and relevant tests to the coordinator.
{% else -%}
Known file → `{{ mcp_tool_ast_source }}` (pattern, head/tail, range, entity); unknown → `{{ mcp_tool_ast_search }}` (`fts` identifiers, `hybrid` concepts); definitions, callers, metrics, impact → `{{ mcp_tool_ast_schema }}` once, then `{{ mcp_tool_ast_query }}`. For cluster neighbors pass `checkouts[].dir` as `project_dir` rather than native grep. Native reads are fine for patching once AST located the code. Record anchors and affected tests in the Task.
{% endif -%}
