# Cypher stops being a footnote and the dead-code query stops lying

**Date:** 2026-07-28
**Scope:** `internal/ast/rule.go`, `internal/ast/rule_cypher_test.go`
**Origin:** Engineer observed the agent used `ast_search` a lot and almost never Cypher query

---

## Why the agent didn't use query

It wasn't lack of examples — the cookbook already had dozens. It was framing, in two lines:

```
### Phase 2.3: Hybrid Search (RECOMMENDED — Best Results)

**This is the RECOMMENDED default for text-based discovery.**
```

An agent reads *"recommended"*, *"best results"*, *"default"* — and stops there. Phase 3, where the
question actually gets answered, came after and with nothing saying search output is **input**
to it.

Header became **"the best way to FIND NAMES, never the answer"**, with rule and reason:

> Its output is input to Phase 3, not a result to report. Search result is a ranked guess
> by textual similarity. It doesn't know what calls what, what would break, nor how complex something
> is — it never traversed an edge.

And Phase 3 gained an honest title: *"where the question gets answered"*.

## What came before the phases

An opening section saying what this is: **LadybugDB, property graph database, with
Cypher** — `MATCH`, variable-length paths, aggregation, `UNION`, `OPTIONAL MATCH`. With
a table contrasting the two tools on the **same** question:

| question | `ast_search` returns | a query returns |
|---|---|---|
| "How does authentication work?" | ~15 entities whose text looks like "authentication" | the call chain from entry point to token check, each hop named |
| "Who uses `saveUser`?" | where the string appears | every caller, transitively, with file and line — and nothing that just mentions the name in a comment |
| "Is it safe to delete?" | nothing about safety | exact inbound edge count, which **is** the answer |
| "What is the riskiest code?" | nothing — risk isn't a word in source | functions ordered by `cyclomatic_complexity`, and which are caller-less |

And the five requested families, each with queries run against the real graph before inclusion:
entity relations, true find usage, refactoring (impact radius), complexity and
risk, and system understanding you've never read.

## The bug: every callable exists TWICE, and the dead-code query lies

```
MATCH (t:Function {name: 'Apply'}) RETURN t.name, t.path, t.line_number, t.is_stub
→ is_stub false | line 53 | internal/textslice/textslice.go
→ is_stub true  | line  0 | (empty path)
```

`CONTAINS` links `File` to the **declaration**. `CALLS` points to the **stub**, keyed by bare name.
They are **different nodes**.

Consequence, verified in this repository:

```
MATCH (f:Function {name: 'Apply'}) WHERE NOT ()-[:CALLS]->(f) RETURN f.name, f.path
→ Apply | internal/textslice/textslice.go
```

`Apply` has **13 callers** and the "unused code" query from the skill itself classifies it as
dead — because `NOT ()-[:CALLS]->(f)` is true for **every** declaration, always. It was in three
places:

- *Finding unused code* (mandatory use table)
- *Find orphan functions (dead code candidates)* (cookbook)
- *Safe-to-delete check* — `OPTIONAL MATCH (caller)-[:CALLS]->(f) ... count(caller)`, which counts
  zero on a function with fifty callers

**An agent following any of them deletes live code.** It's the most dangerous finding of this entire
review.

### The form that works

`WITH collect()` works, so it fits in a single query — comparing by **name**, never by
node identity:

```
MATCH ()-[:CALLS]->(s:Function) WITH collect(DISTINCT s.name) AS called
MATCH (f:Function) WHERE f.is_stub = false AND NOT f.name IN called AND f.entry_point_score < 10
RETURN f.name, f.path, f.cyclomatic_complexity ORDER BY f.cyclomatic_complexity DESC
```

Verified: `Apply`, `ReadPage`, `ListPages` and `firstHeading` — all with callers — leave the
result. And `MATCH (caller)-[:CALLS]->(t:Function {name: 'firstHeading'}) RETURN count(caller)`
returns 3, the correct number.

## The silence that reads as "there is nothing"

From the same cause comes a second class of failure: **mixing edge types around the same node
returns zero rows and no error.**

```
(caller)-[:CALLS]->(e)<-[:CONTAINS]-(f:File)    → 0 rows
```

Not an engine limitation — it's that no node has both edges. Isolated:
`(caller)-[:CALLS]->(Apply)` → 13 rows; `(Apply)<-[:CONTAINS]-(File)` → 1 row; together → 0.

Two pre-existing queries fell into this and **always** returned empty:

- *Find circular dependencies between files* — `IMPORTS` and `CONTAINS` alternating on the same path
- *Find parent interface usages* — `IMPLEMENTS` and `CALLS` in clauses joined by crossed `WHERE`

Fixed: the first became a question a single edge answers; the second became two queries,
explicitly, with reason written. And *Move-file impact*, which used `OPTIONAL MATCH` with `WHERE`
referencing the previous clause, became two steps with the name list in `IN`.

Documented as three consequences of projecting a query around a node, plus the practical rule:
**add `f.is_stub = false` to any query returning `path` or `line_number`**, otherwise you
report results with empty path and line `0` as if it were a real location.

## One of my own, in the same pattern

The query I had written for "public surface of a module" filtered by `e.is_exported` with
`e` unlabeled — and returned 51 rows, **34 of which were comments**, because `is_exported` is also
`true` on `Comment` nodes. Fixed with `label(e) IN [...]`, and the trap documented: when the
filter is about *declarations*, say which labels you want.

Also swapped a dependencies query that depended on `m.is_dependency`, empty in this graph, for
one that orders by how many files import each module — and returns a useful answer.

## Tests

`TestASTRuleContentHasNoAlwaysTrueDeadCodeQuery` distinguishes **prose from runnable example**: the
broken forms remain cited in text, as what not to write, and the
helper `runnableQueries` only looks at lines starting with `MATCH ` — those an agent lifts straight to the
tool. My first test was blind to that difference and failed on its own warning.

Plus `TestASTRuleContentExplainsTheStubDuality`,
`TestASTRuleContentFramesSearchAsGroundingNotAnswer` (which forbids the return of *"RECOMMENDED default
for text-based discovery"*) and `TestASTRuleContentCoversTheQueryOnlyQuestions`, about the five
families.

`golangci-lint` clean.

> **Note:** another session is working on ANTLR/Oracle in this same tree. Nothing of it was committed nor
> reverted; this commit stages only `internal/ast/rule.go`, the new test and this changelog.
