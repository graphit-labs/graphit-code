# Task: the AST skill must require calling the schema before the first query

**Status: completed** on 2026-08-04.

## The problem, with a real case

In one session, the agent went straight to Cypher and broke twice in a row, on properties that
don't exist:

```
MATCH (n) WHERE n.path CONTAINS 'internal/hub/' RETURN n.type, n.name, n.path, n.line
→ Binder exception: Cannot find property type for n

MATCH (n:Function) WHERE toLower(n.name) CONTAINS 'event' RETURN n.name, n.path, n.line
→ Binder exception: Cannot find property line for n
```

`type` doesn't exist — the node's type is `label(n)`. `line` doesn't exist — it's `line_number`.

The material to prevent this **was already in the skill**: the "Property Reference" table in
Phase 1 and the warning about `n.type` in Phase 2.3. What was missing was **positional**: nothing
said that the call to `graphit_ast_schema` comes *before* writing the query. Phase 1 opened with
"labels are dynamic, call the schema to find out which ones exist" — an optional discovery
suggestion, not a mandatory step. So the table was only read after the error — when it was read
at all.

It's worth recording the full failure mode: a made-up property does **not degrade** to an empty
result — it crashes the query. And an agent that guessed once tends to guess again — that's
exactly what happened between the first and second query above.

## What was done — `internal/ast/rule.go`

Everything in `ASTRuleContent()`, which is the source of `SKILL.md` for all three IDEs, and in
`MandateTrigger()`.

1. **Phase 1 became a step, not a reference.** The title changed to `Phase 1: Know the schema —
   call the schema tool BEFORE your first query`, opening with "the first AST call you make is
   the schema, not a query" and "before writing Cypher — not after failing." It also states when
   to repeat it: a new property in this session, or a change of `project_dir`/`context` (labels
   are per-project — a repository without SQL has no `Table`).

2. **The schema output is the authority, the table is a summary.** The previous wording —
   "labels are dynamic, **but** property names are fixed and universal" — invited treating the
   table as sufficient and skipping the call. Now the table is titled "the common labels — the
   schema tool is still the authority".

3. **New table: `Properties that do NOT exist`.** Fourteen rows of plausible guesses → the real
   name: `n.type`/`n.kind`/`n.label` → `label(n)`; `n.line`/`n.start_line` → `n.line_number`;
   `n.complexity` → `n.cyclomatic_complexity`; `n.body`/`n.code` → the source tool;
   `n.is_public` → `n.is_exported`; `n.params` → `HAS_PARAMETER`; `n.callers` → count the edge;
   `r.line`/`r.file` → `r.line_number`/`r.source_file`. Saying "don't guess" without giving what
   to write leaves the agent stuck — it's the right-hand column that makes the rule actionable.

4. **`Binder exception` recovery protocol.** The error names the property that didn't resolve;
   the instruction is to call the schema and rewrite **once**, never guessing a second name. It
   also separates the two causes, which call for different fixes: (a) the property doesn't
   exist; (b) it exists, but not on every label the pattern matches — in a label-less
   `MATCH (n)`, you can only rely on the shared set, and `n.is_exported` there is a crash, not an
   empty column.

5. **Two new triggers in the mandate** (and therefore in `AGENTS.md`, in context every session):
   "about to write Cypher and haven't called `ast_schema` for this `project_dir`/`context` yet"
   and "the query failed with `Binder exception: Cannot find property`".

6. **Bullet in `Cypher Guidelines`:** "Schema before Cypher", with the reason — guessing doesn't
   return empty, it breaks.

7. **Note about `File.source`.** A side effect of promoting the schema output to authority: it
   lists `source` on `File`, and Phase 4h says source isn't a graph property. Both are correct,
   and the reconciliation lives in the code — `internal/ast/ladybug.go:222`:

   > `File.source` no longer holds file text — the search index owns that, as the only copy that
   > is actually queryable. The column survives for the synthetic `__config__` node, where
   > `RunEnrichment` stores the detected project config.

   File text moved out in `2cf35f0` ("a single store for file text"): `rebuild_index.go` and
   `json_rebuild.go` write `File` without `source`. The **column** deliberately remains in the
   DDL, and its only writer is `enrichment.go:413`. Verified in the live graph:
   `size(c.source)` on `__config__` returns 71, while on `internal/ast/rule.go` it comes back
   empty — that's expected, not broken. Without this note, the new instruction would create its
   own failure mode: a `RETURN f.source` expecting the file.

## Tests — `internal/ast/rule_schema_first_test.go`

Three tests, the third being the one that matters long-term:

- `TestASTRuleContentDemandsSchemaBeforeFirstQuery` — the instruction exists **and** appears
  before Phase 3, otherwise anyone reading top-to-bottom has already written the query by the
  time they hit the rule.
- `TestASTRuleContentNamesTheInventedProperties` — the properties that actually broke are named,
  the `Binder exception` text appears, and "don't guess a second name" is there.
- `TestASTRuleContentRunnableQueriesUseRealProperties` — **the skill validated against itself**:
  every copyable query it publishes is checked against the real set of properties (nodes +
  edges, pulled from `graphit_ast_schema`). An example with a made-up property teaches exactly
  the mistake the rule exists to prevent. The test caught the two counter-examples I had just
  pasted in; that's why they got a `❌` at the start of the line, which also keeps them from being
  copied by mistake — `runnableQueries` only collects lines that start with `MATCH `.

## Propagation

`ast.InstallRule` + `ast.InstallSkill` for the three IDEs (claude, antigravity, kiro) —
regenerated `AGENTS.md` and the three copies of `SKILL.md`. Nothing edited by hand outside of
`rule.go`.

`go test -tags fts5 ./internal/ast/` passes in full. Without the `fts5` tag, the package fails on
~8 search-index tests (`no such module: fts5`) — that's the `Makefile`'s tag requirement, not a
regression.
