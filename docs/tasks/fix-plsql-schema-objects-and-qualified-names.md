# Fix: PL/SQL schema objects, qualified names and unbacked rel table groups

**Date:** 2026-07-28
**Status:** Done

## Problem

`graphit ast index <schema> --reset --grammar=.sql=antlr-plsql` over an Oracle export
of 35 358 `.sql` files ended with:

```
! Completed with 1 error(s) out of 35358 files
  › ! Write errors: 1 chunk(s)
  • rebuild: schema: schema "CREATE REL TABLE GROUP IF NOT EXISTS CONTAINS(FROM Directory": Binder exception: Table Table does not exist.
```

The message understates the damage twice over. "1 error out of 35358 files" is not one bad
file: `writeErrors` is incremented once for the whole full rebuild
(`internal/ast/pipeline.go:499`). And because `--reset` deletes the database directory
before indexing (`cmd/graphit/commands/runners.go:210`) while the rebuild only swaps its
temporary database in at the very end, the abort left **no database at all** — 573 s of
parsing with an empty graph to show for it. The parse cache survived (`jsonCache.Save()`
runs before the write), so a rerun skipped the parse and failed identically.

## Root causes

Five distinct defects, each verified against the real parse cache:

1. **The Table query never matched.** `plsql.yaml` asked for
   `//create_table/tableview_name`, but the grammar spells it
   `create_table : CREATE ... TABLE (schema_name '.')? table_name ...`. Result: **0**
   `Table` entities beside **75 121** `Table -CONTAINS-> Column` edges. `create_view`
   had the same mismatch — it names the view with a bare `id_expression` — so a corpus
   of views yielded nothing but their comments.

2. **The CONTAINS rel table group was built unfiltered.** Node tables come from entity
   labels; containment pairs come from an entity's declared *context type*, which need
   not name any label that exists. Every other group in `initSchemaForLabels` filtered
   through `labelSet`; CONTAINS did not, so `FROM Table TO Column` reached LadybugDB
   with no `Table` node table — and a rejected DDL statement aborts the whole rebuild.

3. **Qualified names resolved to the qualifier.** `extractNameFromMatch` read
   `FirstTerminalText()`, and a qualified name is `identifier ('.' id_expression)*`, so
   the first terminal is the *schema*. Measured on the corpus: all 1 861 sequences named
   `GC`, every DML edge pointing at one node called `GC`, all 75 121 columns attributed
   to a phantom table `GC`. `declarationName` had the mirror bug — it took the first
   `_name` child, which is `schema_name` — plus two blind spots: a package-level
   `function_body`/`procedure_body` declares a plain `identifier` (context came out as the
   keyword `FUNCTION`/`PROCEDURE`, 14 occurrences in a 120-file sample) and `create_type`
   hides its name inside `type_definition` (context came out as `CREATE`, 7 occurrences).

4. **Every declaration was its own parent.** The context tracked for
   `//create_table/table_name` is the enclosing `create_table` — the table itself — so
   each top-level object got a CONTAINS self-loop and a uid spelled `path::X.X`.

5. **DML targets could never match a declaration.** A reference is cached as a bare
   object name (`ConvertToCache`) while an entity uid is file-scoped, so
   `emittedAny(ref.TargetUID)` was always false: every target became a stub, and each
   table existed twice — one node with its columns, another with its inbound SELECTS,
   which no query could join. The DML target label was hardcoded to `Table`, so edges to
   views and sequences were dropped and a corpus with no `CREATE TABLE` file got no DML
   edges at all.

A sixth instance of defect 2 surfaced while testing: `HAS_PARAMETER` derived its owner
labels from the *call* sources and fell back to a hardcoded `FROM Function` — so a package
whose procedure takes a parameter and calls nothing produced the same binder exception.

## Fix

**Extraction** (`internal/ast/antlr/common/tree.go`, `internal/ast/antlr_adapter.go`)

- `TreeNode.QualifiedNameText()` walks the direct children as a dotted name and returns
  its last component; `DeclaredNameText()` does the same for a declaration node, skipping
  leading keywords and descending one level when the name is delegated to a child rule.
  Both return `""` when the node is not name-shaped, so callers fall back to
  `FirstTerminalText` and non-name patterns (a comment's text, `COMMIT`, an expression)
  keep their behaviour.
- `MatchResult.Context` became a linked `ContextNode` chain. `resolveParentContextAntlr`
  now skips the link that *is* the entity being placed and answers with what actually
  encloses it — the package around a procedure, nothing around a table. A constructor
  still gets its class, because the check requires name *and* label to match.

**Queries** (`internal/ast/queries/plsql.yaml`): `//create_table/table_name` and
`//create_view/id_expression`.

**Schema** (`internal/ast/ladybug.go`, `internal/ast/rebuild_index.go`,
`internal/ast/json_rebuild.go`)

- `initSchemaForLabels` builds a `nodeTables` set first and filters *every* rel table
  group through it — CONTAINS, CALLS, HAS_PARAMETER, HAS_FIELD, READS/WRITES_FIELD,
  INHERITS/IMPLEMENTS, the annotation groups and DML. A group left with no pair is
  skipped instead of emitted empty.
- `rebuildIndex.scan()` drops containment pairs whose ends have no node table (the writer
  already discarded those edges, so nothing is lost), and derives `paramOwnerLabels` from
  what actually owns a parameter.
- `resolveRefTarget` resolves a reference against the schema-level objects of the whole
  corpus, so DML lands on the declared node; unresolved names still become stub Tables,
  and `Table` is created as a node table whenever stubs are needed. `dmlTargetLabels` is
  derived from what references resolve to, replacing the hardcoded whitelist (which also
  never matched the `DBLink` label the queries emit).

## Verification

New tests, all hermetic — they install this repository's `plsql.yaml` into a temporary
project so they assert against the checkout and not whatever runtime is installed:

- `internal/ast/antlr/common/qualified_name_test.go` — 15 cases over the two name
  readers, including the non-name shapes that must keep the old behaviour.
- `internal/ast/oracle_schema_extraction_test.go` — real grammar, real query file:
  Table/View entities exist and are named after the object, columns hang off the table,
  DML targets and sequence/trigger names are the object and not `ACME`, a package-level
  procedure belongs to its package and its locals to it.
- `internal/ast/schema_contains_guard_test.go` — the unbacked pair is dropped, the DDL
  survives one passed in explicitly, and a DML target joins the declaring node.
- `internal/ast/oracle_schema_graph_test.go` — full write path against a real LadybugDB,
  then Cypher: `Table -CONTAINS-> Column`, `Procedure -UPDATES-> Table` on the
  non-stub node, a stub for the table only DML mentions, and zero nodes named after the
  schema, zero CONTAINS self-loops.

`go test ./internal/ast/... ./internal/hub/... ./internal/mcpstdio/... ./internal/knowledge/...`
passes.

## Known remaining gaps

Not part of this change; each needs grammar verification and a fixture of its own.

- **Dead patterns in every grammar.** A static audit of each `queries/*.yaml` against its
  `.g4` finds patterns whose rule does not exist or is not a direct child: 12 more in
  plsql (`create_type_body` — the rule is `type_body`; `function_call` and
  `procedure_call` — so a function called inside an expression yields no CALLS edge;
  `constant_declaration`; the four `*_type_definition` rules; `tablespace_name`;
  `//create_trigger/tableview_name` — the table is nested under `simple_dml_trigger`, so
  no trigger→table edge is produced), 6 in tsql, 6 in db2, 6 in postgresql, 6 in cobol85.
- **PostgreSQL qualified names.** It spells qualification as `colid indirection` rather
  than a dotted child list, so `QualifiedNameText` does not see it and behaviour there is
  unchanged.
- **`COMMENT ON COLUMN`** references the bare column name, which resolves to nothing and
  becomes a stub Table.
- **`CREATE DIRECTORY`** maps to the label `Directory`, colliding with the filesystem
  `Directory` node table, whose schema is different.
- **`CREATE INDEX ... ON <table>`** produces no edge to the table.
