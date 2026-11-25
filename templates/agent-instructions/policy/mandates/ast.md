
# AST Code Exploration
Before locating/reading code or assessing structure/impact, including before native grep, glob, file reads or symbol navigation, read `{{ skill_name_ast }}` if its instructions are not in the current context; then use its {{ brand_display }} MCP tools.
{% if mandate_audience | default("") == "delegate" -%}
Use AST first. Read known paths with `{{ mcp_tool_ast_source }}` and discover unknown paths with `{{ mcp_tool_ast_search }}`. For structure and impact, inspect `{{ mcp_tool_ast_schema }}` once and then use `{{ mcp_tool_ast_query }}` with only advertised labels and edges. Use the Role tool's `selection` and `input` envelope. Stay in the admitted project or imported context. Send anchored findings and relevant tests to the coordinator.
{% else -%}
Use AST first: known file → `{{ mcp_tool_ast_source }}` (pattern, head/tail, range, entity); unknown location → `{{ mcp_tool_ast_search }}`; symbols/callers/metrics/impact → `{{ mcp_tool_ast_schema }}` once per target, then `{{ mcp_tool_ast_query }}`. Cluster neighbors are also managed: pass their authorized `checkouts[].dir` as `project_dir`, never switch to native grep/walk for being outside cwd. Read the needed target skill, source and dependents/tests; record evidence in the coordinating Task. `{{ mcp_tool_ast_schema }}`/`{{ mcp_tool_ast_query }}` are the Cypher GRAPH; for a structured question over the indexed rows — every entity in a path, what is a dependency — use `{{ mcp_tool_ast_fts_schema }}` then `{{ mcp_tool_ast_fts_query }}`.
{% endif -%}
