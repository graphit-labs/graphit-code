# Tarefa: a skill de AST tem que mandar chamar o schema antes da primeira query

Status: Completed on August 4, 2026.

## O problema, com o caso real

In a session, the agent went straight to Cipher and broke two times consecutively, in properties that shouldn't have been compromised.
existem:

```
MATCH (n) WHERE n.path CONTAINS 'internal/hub/' RETURN n.type, n.name, n.path, n.line
→ Binder exception: Cannot find property type for n

MATCH (n:Function) WHERE toLower(n.name) CONTAINS 'event' RETURN n.name, n.path, n.line
→ Binder exception: Cannot find property line for n
```

The inline 0 does not exist - the type of node is inline 1. Inline 2 does not exist - it is inline 3.

The material already exists to prevent this: the "Property Reference" table in Phase 1 and the...
aviso sobre `n.type` na Fase 2.3. O que faltava era **posicional**: nada dizia que a chamada de
The ``graphit_ast_schema`` comes before writing the query. The Phase 1 starts with "the labels are dynamic,"
Call the schema to discover what exists" — an optional suggestion for discovery, not a step
Required. Therefore, the table was only read after an error occurred, when it was read.

It is worth noting the complete failure mode: an invented property does not degrade the result
vazio, ela derruba a query. E um agente que chutou uma vez tende a chutar de novo — foi
exatamente o que aconteceu entre a primeira e a segunda query acima.

## O que foi feito — `internal/ast/rule.go`

Everything in **INLINE_0**, which is the source of **INLINE_1** for all three IDEs, and in **INLINE_2**.

Phase one became a step, not a reference.
Call the schema tool before your first query, opening with "The first call to the AST that you"
It is the schema, not a query", and "before writing Cypher — not after failing." Also says
When repeating: property new in this session, or swap of `project_dir`/`context` (labels are
for project - repository without SQL doesn't have `Table`).

The output of the schema is authority, the table is summary. The previous text—“the labels are”
Dynamic, however, property names are fixed and universal – inviting you to treat them as such.
   tabela como suficiente e pular a chamada. Agora a tabela se chama "the common labels — the
   schema tool is still the authority".

New Table: `Properties that do NOT exist`\. Four plausible shots → Name
   real: `n.type`/`n.kind`/`n.label` → `label(n)`; `n.line`/`n.start_line` → `n.line_number`;
   `n.complexity` → `n.cyclomatic_complexity`; `n.body`/`n.code` → a tool de source;
   `n.is_public` → `n.is_exported`; `n.params` → `HAS_PARAMETER`; `n.callers` → contar a aresta;
Speak "don't kick" without saying what you're writing.
It leaves the agent in a bind—this is the right column that makes the rule actionable.

4. Recovery Protocol for Inline 0. The error names the property that did not connect;
The instruction is to call the schema and rewrite it once, never second-guessing a second name. Separate.
Both causes require different corrections: (a) the property does not exist; (b) it exists, but
Not in all the labels that the standard fits — only with `MATCH (n)` without a label can you touch the set.
Shared, and `n.is_exported` there is a crash, not an empty column.

Two new triggers in the mandate (therefore, in context, throughout the entire session): "go"
Write Cypher and still haven't called `ast_schema` for this `project_dir`/`context`. And the query.
   falhou com `Binder exception: Cannot find property`".

6. **Bullet in Inline 0:** "Schema Before Cypher," with the reason—chucking doesn't return
   vazio, quebra.

Here's the translation:

7. Note on `File.source`. Side effect of promoting authority to exit schema: she
List **INLINE_0** in **INLINE_1**, and Phase 4h states that the source is not owned by the graph. Both are.
Certainly, and reconciliation is in the code — `internal/ast/ladybug.go:222`:

   > `File.source` no longer holds file text — the search index owns that, as the only copy that
   > is actually queryable. The column survives for the synthetic `__config__` node, where
   > `RunEnrichment` stores the detected project config.

The text from the files came out as INLINE 0 ("just a text file storage only"): INLINE 1
And `json_rebuild.go` writes `File` without `source`. The column remains in the DDL for a purpose, and...
The only writer is __`enrichment.go:413`__. Confirmed in the living graph: __`size(c.source)`__ on __`__config__`__.
It is 71, in `internal/ast/rule.go`, it's empty — connect, doesn't break. Without this note, the new instruction
It would create its own way of failure: an INLINE_0 expecting the file.

## Testes — `internal/ast/rule_schema_first_test.go`

Three, being the third thing that matters in the long run:

- INLINE 0 - the instruction exists **and** appears before the

This is already English, so no changes were made.
Phase 3, otherwise anyone reading from top to bottom has already written out the query when they hear the rule.
- `TestASTRuleContentNamesTheInventedProperties` - the properties that truly broke
Noted, the text of `Binder exception` appears, and "don't second-guess your name" is there.
- `TestASTRuleContentRunnableQueriesUseRealProperties` — **a skill validada contra ela mesma**:
Every copyable query she publishes is checked against the set of actual properties (we +)
  arestas, tirado de `graphit_ast_schema`). Um exemplo com propriedade inventada ensina exatamente
  o erro que a regra existe pra impedir. O teste pegou os dois contra-exemplos que eu tinha
finished with; therefore they put _`❌`_ at the beginning of the line, which also prevents them from
Copied by mistake — `runnableQueries` only collects lines that start with `MATCH `.

Propagation

The three IDEs (Claude, Antigravity, Kiro) have regenerated.
Inline 0 and the three copies of Inline 1, nothing edited outside Inline 2.

`go test -tags fts5 ./internal/ast/` passa inteiro. Sem a tag `fts5` o pacote falha em ~8 testes de
Search Index (`no such module: fts5`) is the tag of `Makefile`; it's not regression.
