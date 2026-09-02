<GRAPHIT_SYSTEM_MANDATE>
Graphit is the project knowledge and code-navigation layer.
For each action, match only the current action against the module triggers below. If one matches, read that skill once in the session immediately before acting; do not preload unrelated skills or reread one already loaded.
Within a matched domain, prefer Graphit MCP over native search, file walking, web/model knowledge, or IDE memory. This applies to every agent and subagent. If the required Graphit tool is unavailable in the current agent, continue with that agent's default native tools. Do not substitute the Graphit CLI for MCP.
Adapter hooks load mandatory memory and reassert this routing at supported lifecycle boundaries. They cannot classify semantic intent, so these triggers still apply after interruptions, corrections, compaction, handoff, and resumed work.
The daemon indexes writes asynchronously. Use `graphit_sync` only when proven cross-module freshness is required or before completing code-changing work; do not sync after every edit.


<mem_rule>
# Memory
When the next action involves durable preferences, corrections, decisions, constraints, or learned system behavior, read `graphit-memory` once before that action and use its Graphit MCP tools.
Triggers:
- planning a material change, getting stuck, or relying on an earlier decision/preference
- the user corrects, teaches, or states a durable preference or constraint
- a non-obvious project fact or trade-off should survive this session
- reading, writing, classifying, reconciling, or deleting memory, including another project's memory
Core tools: `graphit_memory_search`, `graphit_memory_insert`, `graphit_memory_update`, `graphit_memory_list`, `graphit_wiki_source`. The skill routes the remaining tools.
</mem_rule>
<ast_rule>
# AST Code Exploration
When the next action involves code discovery or structural analysis, read `graphit-ast` once before that action and use its Graphit MCP tools.
Triggers:
- locating or understanding code, a symbol, callers/callees, imports, inheritance, tests, complexity, or change impact
- using grep, glob, find, semantic search, code symbols, or file-by-file reads to discover code
- editing an entity whose dependents and test reach are not yet known
- reading code from another repository or a named AST context
- writing Cypher, recovering from a graph/schema failure, or requiring a provably fresh graph
Core tools: `graphit_ast_search`, `graphit_ast_query`, `graphit_ast_schema`, `graphit_ast_source`. The skill routes the remaining tools.
</ast_rule>
<hub_rule>
# Hub Discovery
When the next action involves external systems, reusable artifacts, ecosystem projects, or Graphit configuration, read `graphit-hub` once before that action and use its Graphit MCP tools.
Triggers:
- using an external library, framework, SDK, API, service, or repository
- answering from model knowledge or web search about an external system
- resolving another project, shared solution, reusable artifact, or artifact destination
- inspecting or changing Graphit Hub, cluster, or framework configuration
Core tools: `graphit_hub_search`, `graphit_hub_show`, `graphit_hub_list`, `graphit_hub_projects`, `graphit_config_get`. The skill routes the remaining tools.
</hub_rule>
<doc_rule>
# Knowledge & Documentation
When the next action involves project knowledge, documentation, task logs, or backlog, read `graphit-knowledge` once before that action and use its Graphit MCP tools.
Triggers:
- starting or resuming implementation work, completing a step, changing direction, or modifying code
- answering why the project works this way or reading architecture, decisions, specifications, or provenance
- searching, reading, creating, or maintaining documentation or another project's wiki
- recording deferred work or requiring proven wiki/index freshness
Core tools: `graphit_knowledge_search`, `graphit_wiki_search`, `graphit_wiki_source`, `graphit_wiki_xrefs`, `graphit_backlog_add`. The skill routes the remaining tools.
</doc_rule>
</GRAPHIT_SYSTEM_MANDATE>
