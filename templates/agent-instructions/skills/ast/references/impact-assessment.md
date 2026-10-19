{% if tool_transport | default("root") == "role" -%}
# Role impact assessment from current graph evidence

This companion uses the same authenticated Role PROJECT/version and `{selection?, input}` envelope as the AST entry. It grants no maintenance, foreign PROJECT or coordinator lifecycle authority.

## Establish the symbol and contract

Inspect `{{ mcp_tool_ast_schema }}` once. Resolve the requested symbol by observed name/path, then read only the needed source through `{{ mcp_tool_ast_source }}`. A query example for a schema containing Function, executed with `{{ mcp_tool_ast_query }}`, is:

```json
{"input":{"query":"MATCH (n:Function) WHERE n.name = $name RETURN n.name AS name, n.path AS path, n.start_line AS start_line, n.end_line AS end_line","parameters":{"name":"main"}}}
```

Adapt labels and properties to the actual schema. start_line/end_line describe an extent; line_number can locate only the name. Homonyms need their real identities and paths. A stored metric is evidence, not a rewrite rule or proof of semantic simplicity.

## Trace the relevant consumers

If the schema declares CALLS, query incoming/outgoing relationships around the resolved symbol, with explicit depth and result limits appropriate to the question. Project only the fields needed for the next read. Do not invent a relationship when the producer does not index it. Inspect the actual reached implementations and tests; an empty direct-call result does not rule out runtime dispatch or external consumers.

`{{ mcp_tool_ast_search }}` in fts mode can locate candidate tests without model resources. Validate candidates in Source. Keep observed facts separate from potential impact and missing evidence; a lookup is not a test execution.

## Report a useful result

Return the logical PROJECT/version, relative paths and symbols, the inspected contract, affected callers/consumers, concrete risk and the smallest useful validation. Do not persist host roots or private identities. The coordinator owns Task claims, acceptance and delivery decisions. Preserve unresolved differences instead of changing a contract solely to match current code.

A source example uses the existing Role selector shape:

```json
{"input":{"path":"main.rs","selectors":{"start_line":1,"end_line":30,"line_numbers":true}}}
```

Replace sample paths and coordinates with observed ones. Native typed results and schema/query/search responses remain wrapped in result. Unsupported relationships, absent source/index/model, changed capture and denied permission are reported with their actual cause; do not substitute a Runtime key, another PROJECT or an invented query result. Stop when the scoped assessment has enough evidence.
{%- else -%}
# Complete impact assessment

Read this when an investigation must become a decision, executable plan or handoff. Sections: scope and evidence; interpret the result; contract reconciliation; recovery. A single source lookup does not require this workflow.

## Case: changing the AST instruction templates

Request: “Understand `skill_templates(Module::Ast)`, its consumers and the impact of changing the AST instructions.”

This case uses the current Rust selection and rendering path in {{ brand_display }}. It identifies real source and test targets without asserting a caller count, stored complexity or test execution. Re-query the current target before acting. Replace /work/graphit-code only in the live MCP payload with the current host's verified root. Never save that expansion in the analysis, docs or handoff; persist {{ brand_display }} project identity, revision and repository-relative file/symbol references. For a globally installed artifact without a checkout, use its resolved context instead of project_dir.

The analysis task's scope is template selection, rendering and affected consumers. It does not authorize changing instructions. Reuse its known requirements and relevant Knowledge pages; record uncertain intent instead of deciding that the implementation is automatically the contract.

### 1. Establish identity and read the definition

Call {{ mcp_tool_ast_schema }} with {"project_dir":"/work/graphit-code"} once. Use only labels, properties and relationships returned by the actual schema. For a schema exposing Function and the selected source fields, run this orienting query with {{ mcp_tool_ast_query }}:

~~~json
{"project_dir":"/work/graphit-code","query":"MATCH (f:Function) WHERE f.name = 'skill_templates' AND f.path = 'crates/graphit-runtime/src/skill_instructions.rs' RETURN f.uid, f.path, f.line_number, f.end_line, f.cyclomatic_complexity LIMIT 5","ai_optimized":true}
~~~

Treat returned identities and metrics as live evidence, not values supplied by this example. If a metric is absent, report it unavailable. Read the selected definition with {{ mcp_tool_ast_source }}:

~~~json
{"project_dir":"/work/graphit-code","path":"crates/graphit-runtime/src/skill_instructions.rs","entity":"skill_templates","entity_type":"Function","line_numbers":true}
~~~

The `Module::Ast` arm selects `skills/ast/entry.md`, its description and the `references/impact-assessment.md` companion from the captured template set. The corresponding Markdown sources live under `templates/agent-instructions/skills/ast/`. A selection function's control flow does not establish the correctness of its instructions; a wording change can still break a tool route or misdirect an agent.

### 2. Trace consumers, then inspect their actual behavior

If the current schema declares CALLS, use a bounded direct-caller query with {{ mcp_tool_ast_query }}:

~~~json
{"project_dir":"/work/graphit-code","query":"MATCH (caller)-[:CALLS]->(f:Function) WHERE f.name = 'skill_templates' AND f.path = 'crates/graphit-runtime/src/skill_instructions.rs' RETURN DISTINCT caller.name, caller.path LIMIT 20","page_size":20,"ai_optimized":true}
~~~

Read reached definitions rather than treating names as proof. Independently inspect these real rendering and test targets:

| Source/test target | Evidence and implication |
| --- | --- |
| `skill_templates`, `crates/graphit-runtime/src/skill_instructions.rs` | Selects the AST body, description, references and captured candidates. A template edit changes the selected default source; it does not by itself prove every installation or MCP consumer was updated. |
| `render_instruction`, `crates/graphit-runtime/src/instructions.rs` | Resolves captured origins and variables, renders through the existing template engine and returns the body with its origin and selections. Inspect its precedence and request/error handling before changing rendering behavior. |
| `complete_fixtures_cover_five_bodies_ten_references_and_five_descriptions`, `crates/graphit-runtime/tests/skill_instructions.rs` | Obtains the selected candidates through `skill_templates`, calls `render_instruction` through its `render` helper, and compares the complete rendered body against `tests/fixtures/skill-instructions/catalog.json` for two brands. It covers 20 distinct catalog entries; it does not establish runtime validity of every documented query. |

For the renderer, call {{ mcp_tool_ast_source }} with path `crates/graphit-runtime/src/instructions.rs` and entity `render_instruction`. For the consumer test, read `crates/graphit-runtime/tests/skill_instructions.rs` around the named test and its `render` helper. Compare the authored template, rendered fixture and consumer assertions before concluding that a change is integrated.

If a remaining question concerns propagation, use a bounded two-hop caller query. Do not expand the whole graph simply because more edges exist. Report the actual result and continuation state; indexed callers alone do not rule out dynamic dispatch, textual references or external consumers.

### 3. Produce a useful result, not a search transcript

A Task analysis can contain:

> **Conclusion:** `skill_templates(Module::Ast)` selects the captured AST instruction templates; `render_instruction` renders selected candidates and variables. The complete-fixture test is an inspected consumer of both paths. No caller count or complexity value is inferred from these observations.
>
> **Potential impact:** changed default wording affects the rendered AST instruction body or companion; preserve project override precedence and tool-variable substitution. Complete body fixtures detect unexpected output changes but do not prove that a query recipe executes successfully.
>
> **Evidence:** project {{ brand_display }}, actual inspected source revision, relative identities `crates/graphit-runtime/src/skill_instructions.rs::skill_templates` and `crates/graphit-runtime/src/instructions.rs::render_instruction`; the named consumer test and its catalog. Record actual query scope, rows and remaining pages separately.
>
> **Uncertainty:** dynamic or external consumers are outside the graph's demonstrated coverage. Source inspection alone is not a test pass or product delivery.
>
> **Next action if a change is authorized:** update the authored template and corresponding rendered fixtures; validate one representative query/payload per changed recipe; run affected tests; compare selected and rendered content; reconcile affected instruction documentation. Reuse valid evidence for unchanged recipes.

The focused test command for this Rust consumer is:

~~~sh
cargo test --locked -p graphit-runtime --test skill_instructions
~~~

Inspect the current test setup before execution. Record the actual command result; the example does not claim it ran. A successful fixture comparison and a live query result establish different properties.

## When intended behavior and code disagree

Suppose a maintained contract says “project overrides take precedence,” but inspected installation source actually selects a default before consulting the project. That is a hypothetical discrepancy, not a finding from the case above.

Record: authoritative page/section and Task requirement -> observed source branch -> reproducible scenario -> consequence -> correction or unresolved decision. A regression scenario could create a temporary project with an override, install its skill and compare the produced content. Do not weaken the documented requirement merely to match code, or change code based on an outdated page without resolving its authority.

Check both directions: every changed behavior/contract has accurate documentation; every changed documentation claim has supporting code or is explicitly planned. Preserve each unresolved difference in Task, with its owner/prerequisite and next validation. If implementation is requested, create or revise only the missing scoped work and reuse existing coverage; an audit with no gap needs no artificial task.

## Incomplete evidence and recovery

- Same-name definitions: resolve path/UID and source interval before callers. A broad name match can combine unrelated impacts.
- Empty callers: verify target identity and graph scope, then inspect known interfaces/tests. Report no indexed match, not unused code.
- Missing metric: inspect branches and report the metric unavailable; do not replace it with a guessed score.
- Missing implementation edge: schema describes this graph. Use supported explicit edges when present; for implicit Go interfaces, compare signatures and distinguish candidates from verified implementations.
- Query rejection: simplify to supported endpoint projections and separate top-level predicates; refresh schema once only if it changed. Do not escalate to arbitrary Cypher.
- Missing/stale source: use the main skill's targeted index/freshness recovery. A database/service failure is not a source finding.

Stop when the scoped decision has sufficient authoritative evidence. Preserve reusable structural discoveries in Memory only when future agents benefit; keep this full investigation and its next action in Task.
{%- endif %}
