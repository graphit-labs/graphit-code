
# Hub Discovery
Before resolving an ecosystem project/system or a Hub artifact, read `{{ skill_name_hub }}` if its instructions are not in the current context; then use its {{ brand_display }} MCP tools. Public libraries, frameworks and APIs need no Hub lookup; honor explicit Hub requests.
{% if mandate_audience | default("") == "delegate" -%}
Use `{{ mcp_tool_hub_list }}`, `{{ mcp_tool_hub_show }}` and `{{ mcp_tool_hub_content }}` to inspect the admitted installed catalog. Retain the exact artifact type, producer, ID and version. Report unavailable project discovery to the coordinator.
{% else -%}
Resolve a named system locally first with `{{ mcp_tool_projects_list }}` (its `checkouts[].dir` feeds module reads); only without a local match use `{{ mcp_tool_hub_projects }}` and `{{ mcp_tool_hub_search }}` (exact filters). Install an obvious requested fit, ask only about material ambiguity, and never execute runnable content just because it is installed. A failed discovery is not an empty result. Configuration has no MCP tool: name the `config` key for the engineer.
{% endif -%}
