# Task: `Cannot find property` blamed the query, when the culprit was the rebuild

**Status: completed** on 2026-08-05. Fourth in the series started in
`docs/tasks/schema-before-query.md`.

## The symptom

```
MATCH (f:File)-[:CONTAINS]->(e) WHERE f.path IN ['internal/ast/antlr_adapter.go', ...]
  AND label(e) IN ['Function','Method','Struct']
  RETURN f.path, label(e) AS type, e.name, e.line_number, e.end_line ORDER BY f.path, e.line_number
→ Binder exception: Cannot find property line_number for e
```

## The query was right

My first hypothesis was about modelling: `e` without a label, the `CONTAINS` group includes
`FROM Directory TO File`, `File` has no `line_number`, therefore the binder would refuse. I probed against a
**complete** graph built in a temporary directory, and all six forms bind:

```
MATCH (n) RETURN n.line_number                                  OK
MATCH (n) RETURN n.is_exported                                  OK
MATCH (f:File)-[:CONTAINS]->(e) RETURN e.line_number            OK
MATCH (f:File)-[:CONTAINS]->(e) WHERE label(e) IN [...]         OK
MATCH (f:File)-[:CONTAINS]->(e:Function) RETURN e.line_number   OK
```

**LadybugDB's rule is "some candidate table has the property", not "all of them have it".** What
brought the query down was the **state of the graph**: the `index --reset` was in flight, and at that moment the
database had only `File`, `Directory`, `Field` and `Parameter` — the last two in reduced form
(`uid, name, lang, is_stub`), without `line_number`. No table had the property, so the
binder refused a correct query.

Checked at the same moment: `MATCH (f:File) RETURN count(f)` returned **2**.

## That makes the error worse than a failure

An honest failure tells you to fix what is wrong. This one tells you to fix what was right — and
that is what it did to me before I probed. Add to that the fact that the message is **identical** to the one for an
invented name (`n.line`, `n.type`), and the agent has no way to tell them apart.

## What was done

**`internal/ast/ladybug.go`** — `explainBinderErrorLocked` now handles both families of binder
error. For `Cannot find property X`, the distinction is made from evidence: which tables in this
graph carry the property.

- **None carries it** → either the name does not exist, or the schema is partial. The message says both
  things and shows the tables present, because *a handful where there should be thirty* is the
  signature of a rebuild:
  > no label in this graph has a property "line_number". Either the name is wrong — `line` is
  > `line_number`, and the node type is `label(n)`, not a property — or the graph is mid-rebuild
  > and its schema is still partial, which is what these 4 tables look like: Directory, Field,
  > File, Parameter
- **Some carry it** → it is a wrong label pin, and the message names who has it:
  > "relative_path" exists, but not on every label this pattern can match — it is on File. Pin the
  > label in the pattern (`MATCH (n:Function)`), because `WHERE label(n) IN [...]` filters after
  > binding and does not help here

**The skill asserted the opposite of the engine**, in two places, and that is the root cause of my having
started from the wrong hypothesis:

- *"you may ONLY access properties shared by ALL labels: name, path, line_number, end_line,
  docstring, lang"* — false in both directions. `is_exported` and `cyclomatic_complexity` bind in a
  match without a label; and the list treated as safe properties that `File`/`Directory` do not have.
- *"narrow with `label(n) IN [...]` before touching the property"* — does not work: binding happens
  before filtering. The label has to be in the pattern, and several labels mean several `MATCH`
  joined by `UNION ALL`.

The `Binder exception` recovery protocol went from two causes to three, and the third
is marked as the one that makes a correct query look wrong.

## Tests

`TestQueryDistinguishesAWrongPropertyFromAPartialSchema` sets up both states in the same test:
a partial schema (only `initSchema`) rejects `line_number` with the rebuild message; after
`initSchemaForLabels` the same query binds — which proves the message was about the schema, not about
the query. And `n.line`, which does not exist in any state, keeps failing while offering the real name.

`TestQueryExplainsAPropertyMissingFromSomeLabels` covers the other branch with `relative_path`, which only
exists on `File`.

`TestASTRuleContentNamesTheInventedProperties` got the inverse: it fails if the skill goes back to saying
that a match without a label only reaches what all labels share. **That test pinned the
wrong assertion** — it was the one that failed when I corrected the text, which is the right behaviour for a
content test and worth recording: a documentation test also needs to be checked against the
system, not only against the previous text.
