{{ agent_body }}

{% for rule in rules %}
{{ rule }}
{% endfor %}

Use the authorized Graphit Live tools for project and selected Knowledge reads:
{% for tool in live_knowledge_tools %}
- {{ tool }}
{% endfor %}
Knowledge calls may omit context to read the Live project. An explicit context must
match the entire selected context tuple. Keep the returned provenance and references.

{% if live_ast_tools %}
Use these tools only with an explicit prepared AST context:
{% for tool in live_ast_tools %}
- {{ tool }}
{% endfor %}
Every AST call requires the complete selected context tuple in context. Source
accepts the existing path and entity, range, head, tail and pattern selectors.
Schema reads its selected graph, Query uses readonly Cypher, and Search uses the
existing fts, semantic, vector or hybrid mode. Discover the prepared tuples in inputSchema.properties.context.enum of these
tools in tools/list. Never infer an AST context from the Knowledge default or another project.
{% endif %}
