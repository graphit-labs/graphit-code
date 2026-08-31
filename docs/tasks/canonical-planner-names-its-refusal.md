---
title: The canonical planner names why it refused, instead of blaming multi-hop
status: done
created: 2026-08-31
updated: 2026-08-31
tags: [ast, ladybug, icebug, canonical-planner, diagnostics]
---

# The canonical planner names why it refused, instead of blaming multi-hop

## Objective

Every refusal by the canonical bounded-traversal planner is reported with one fixed
sentence:

```
canonical catalog: only bounded reachability over [CALLS CONTAINS HAS_FIELD HAS_PARAMETER
IMPORTS READS_FIELD REFERENCES WRITES_FIELD] is planned (RETURN DISTINCT <endpoint> |
count([DISTINCT] endpoint.uid)); this multi-hop form is not supported remotely
```

That sentence is wrong about the cause and useless as a fix. It was produced for this
query, which is **one hop**:

```cypher
MATCH (f:File)-[:CONTAINS]->(e) WHERE f.path ENDS WITH 'plpgsql_splice.go'
  AND label(e) IN ['Function','Method','Variable','Constant']
RETURN label(e) AS type, e.name, e.line_number ORDER BY e.line_number
```

Isolated empirically against this project's own graph — same file, same relationship,
varying one thing at a time:

| Query | Result |
|---|---|
| `RETURN DISTINCT e.name, e.line_number` | 87 rows |
| `RETURN e.name, e.line_number` | the error |
| `RETURN DISTINCT label(e) AS type, e.name` | the error |

So the real causes were `label()` in the projection and the missing `DISTINCT`. Neither
appears in the message, and "multi-hop" — the only cause it does name — was not one of
them.

The fail-closed contract itself is correct and stays: forwarding an unplanned form hands
the query to an upstream recursive plan that MEASURED enumerates the whole component
(`internal/ast/ladybug_icebug_canonical.go:21`). What changes is that the refusal says
which rule was broken and what to write instead.

### Reasoning

`parseCanonicalTraversal` already knows exactly which rule rejected the query — it has ten
distinct `return canonicalPlan{}, false` sites, each guarding a different semantic it
cannot preserve. The information is computed and then discarded: the function returns a
bare `bool`, so by the time `LadybugBackend.Query` decides to fail closed, all it can do is
re-derive a guess from `namesLogicalRel`, which only answers "does this query mention a
logical relationship type" and knows nothing about hops, projections or predicates.

The fix is to stop discarding it.

### Why this approach over the alternatives

| Considered | Rejected because |
|---|---|
| A second function that re-parses the query in the error path to explain it | Two definitions of the same ten rules, drifting apart from the first edit. The project already has a recorded correction about exactly this shape of duplication |
| Reopen `tree-sitter-cypher` for a real parse | The standing backlog item (`docs/tasks/backlog/reopen-tree-sitter-cypher-for-compatibility-if-errors-fail.md`) says to reconsider it only if the volume or character of rejections justifies a native grammar dependency. This finding is about the *character* of the rejections, and it is answered by naming a reason the parser already has — no grammar needed |
| Loosen the planner to accept `label()` | Not possible without changing semantics: on a canonical catalog the label IS the physical table, and one logical type spans many. The planner would have to synthesise a column from the member it happened to traverse, which is a different query from the one written |
| Just delete the word "multi-hop" | Removes the false statement and adds nothing. The user still cannot tell which of ten rules they broke |

## Plan & Task Breakdown

- [x] **T1 — A refusal carries a reason and a fix** — Spec: introduce `canonicalRefusal`
  (`what` + `fix`) in `internal/ast/ladybug_icebug_canonical.go`; `parseCanonicalTraversal`
  returns it. Three outcomes must stay distinguishable: planned; recognized as a traversal
  but refused (with reason); not a traversal shape at all (falls through to the engine, no
  error). Done when each of the ten rejection sites names its own rule.
  Constraint: the third outcome must not become an error — a node-only query naming no
  logical type still runs on the mounted tables.

- [x] **T2 — Propagate it to the error the caller sees** — Spec: `tryCanonicalBoundedTraversal`
  returns the refusal as its error; `LadybugBackend.Query` (`internal/ast/ladybug.go:755`)
  emits `canonical catalog: <what> <fix>`. The `namesLogicalRel` fallthrough keeps a message
  of its own — that one is a genuinely unrecognized *shape*, so it lists the planned forms —
  but stops claiming the form was multi-hop. Done when the query at the top of this log
  reports `label()` and the missing `DISTINCT`.

- [x] **T3 — Tests, one per rule** — Spec: table test over the refusal rules asserting the
  reason each produces, plus a regression that the accepted forms still plan and that a
  node-only query still falls through. `TestMountedCanonicalUnsupportedMultiHopFailsClosed`
  is renamed and re-pointed: its query (`RETURN collect(caller.uid)`) is a rich *projection*,
  not a multi-hop form, which is the same mislabelling in the test name.
  Done when `go test ./internal/ast/...` passes.

- [x] **T4 — Documentation** — Spec: the planner's supported/refused forms in
  `docs/specs/ast_module.md`, and this log. Done when a reader can see which shapes plan and
  what to write instead of `label()`.

## Implementation Details

- `canonicalRefusal{relType, what, fix}` in `internal/ast/ladybug_icebug_canonical.go`
  implements `error`, rendering as `canonical catalog: <what> <fix>`.
- `parseCanonicalTraversal` now returns `(canonicalPlan, *canonicalRefusal, bool)`. The three
  outcomes are explicit: `(plan, nil, true)` planned; `(zero, refusal, false)` refused by a
  named rule; `(zero, nil, false)` not a traversal, which stays a fall-through and never an
  error. A local `refuse` closure captures `relType` so every message can name the type and so
  the caller can scope the refusal.
- `tryCanonicalBoundedTraversal` surfaces a refusal **only when the relationship type is
  logical** — `k.canonicalGroup(refusal.relType) != nil`. See the regression below; this is the
  part the first attempt got wrong.
- `LadybugBackend.Query` (`internal/ast/ladybug.go`) returns a refusal unwrapped —
  `errors.As` — because a `ladybug query:` prefix buries the one sentence the reader can act
  on. The `namesLogicalRel` fallthrough keeps a message of its own for a genuinely
  unrecognized *shape*, and it no longer claims that shape was multi-hop.
- Agent-facing: `internal/ast/rule.go` gained a recovery protocol beside the
  `Binder exception` one, since an agent writing Cypher against a Hub context is who meets
  this error.

## Use Cases

### UC-01: An agent or user projects `label()` over a logical traversal
- **Actor**: whoever writes the Cypher — the MCP `ast_query` tool, the CLI, the explorer.
- **Preconditions**: the backend is a canonical catalog (mounted bundle, Hub context, remote
  graph); the query traverses a logical type.
- **Main Flow**:
  1. `Query` sanitizes PK/uid equality and calls `tryCanonicalBoundedTraversal`.
  2. `parseCanonicalTraversal` matches the traversal shape, sees `label(` in the RETURN, and
     refuses with the reason and the pattern-pinned form to write instead.
  3. The type is logical, so the refusal is surfaced verbatim as the query error.
- **Alternative Flows**: the same projection over a *physical* member table is not refused —
  the engine runs it.
- **Error Scenarios**: this IS the error scenario; the contract is that it is actionable.
- **Postconditions**: no query ran against the engine; nothing was enumerated.
- **Affected Files**: `internal/ast/ladybug_icebug_canonical.go`, `internal/ast/ladybug.go`.

### UC-02: A traversal over a physical member table with unreadable endpoints
- **Actor**: the export/mount test suite, and any caller querying member tables directly.
- **Preconditions**: the query names a physical table, e.g.
  `MATCH ()-[r:calls__function_function]->() RETURN count(r)`.
- **Main Flow**: the planner cannot read anonymous `()` endpoints and produces a refusal; the
  caller finds no manifest group for that type and discards it, falling through to the engine,
  which answers the query.
- **Postconditions**: the query returns rows, exactly as before this change.
- **Affected Files**: `internal/ast/ladybug_icebug_canonical.go`.

### UC-03: A node-only query on a canonical catalog
- **Actor**: any caller.
- **Preconditions**: no logical relationship type in the query.
- **Main Flow**: the traversal regex does not match, no refusal is produced, and the query runs
  on the mounted tables as written — `label(n)` included.
- **Postconditions**: unchanged behaviour. The label restriction is about traversal, not labels.

## Test Cases & Acceptance Criteria

### Feature: A refusal names its rule
Ref: UC-01 — `internal/ast/ladybug_icebug_refusal_test.go`

#### Scenario Outline: each rule produces its own reason
```gherkin
Given a query over a logical relationship type that breaks "<rule>"
When parseCanonicalTraversal reads it
Then it does not plan the query
  And it returns a refusal naming "<rule>"
  And the refusal does not mention "multi-hop"

Examples:
  | rule                                    |
  | label in the projection                 |
  | projection without DISTINCT             |
  | projection richer than a property       |
  | the RETURN projects both ends           |
  | the RETURN projects neither end         |
  | a predicate comparing both ends         |
  | a predicate referencing neither end     |
  | nothing anchors the traversal           |
  | an inverted hop range                   |
  | both ends bind the same variable        |
```

#### Scenario: the refusal reaches the caller unwrapped, and its suggested fix works
```gherkin
Given a mounted canonical bundle
When a traversal projecting label(e) is queried through the backend
Then the error says "label is not projectable"
  And the error is not prefixed with "ladybug query:"
When the pattern-pinned form the refusal suggests is queried instead
Then it plans and returns rows
```

### Feature: A refusal is scoped to logical types
Ref: UC-02, UC-03

#### Scenario: a physical member table is the engine's to run
```gherkin
Given a mounted canonical bundle
When "MATCH ()-[r:calls__function_function]->() RETURN count(r) AS c" is queried
Then no refusal is raised
  And the count returns rows
```

#### Scenario Outline: a non-traversal is never refused
```gherkin
Given the query "<query>"
When parseCanonicalTraversal reads it
Then it neither plans it nor refuses it

Examples:
  | query                                                            |
  | MATCH (n:Function) WHERE n.name = 'a' RETURN n.name, n.path      |
  | MATCH (n) WHERE n.uid IN ['fn_a'] RETURN DISTINCT label(n) AS type |
```

#### Scenario: the accepted forms still plan
```gherkin
Given each form documented as planned in docs/specs/ast_module.md
When parseCanonicalTraversal reads it
Then it plans without a refusal
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/ast/ladybug_icebug_canonical.go` | Modified | `canonicalRefusal`; ten named rules; refusal scoped to logical types; dead count check removed |
| `internal/ast/ladybug.go` | Modified | Surface a refusal unwrapped; the fallthrough message stops claiming multi-hop |
| `internal/ast/ladybug_icebug_refusal_test.go` | Created | One case per rule, the scoping regression, the fallthrough, and the accepted forms |
| `internal/ast/ladybug_icebug_canonical_test.go` | Modified | `TestMountedCanonicalUnsupportedMultiHopFailsClosed` renamed and re-pointed at the rule that actually fires |
| `internal/ast/rule.go` | Modified | Agent recovery protocol for `canonical catalog:` refusals |
| `docs/specs/ast_module.md` | Created section | What the planner answers, the rule table, and the scoping |

## Trade-offs & Decisions

- **A refusal is scoped by relationship type, not by shape.** The first attempt refused any
  traversal the parser could not read, which is wrong: the planner's rules are about
  preserving the semantics of a LOGICAL type, and a physical member table has nothing to do
  with them. Scoping on the manifest is one condition and it makes every rule safe to add.
- **The refusal is returned unwrapped.** It loses the `ladybug query:` prefix that every other
  error from this function carries. That prefix names a layer the reader cannot act on, and
  the whole point of the change is that the first thing they read is the rule.
- **`label()` was not made projectable.** It could be — the planner knows which member table
  each reached uid came from — but that is a different query from the one written, and
  synthesising the column would make the answer depend on traversal order when a uid is
  reachable through more than one member. Pinning the label in the pattern is exact.

## Technical Debt
- [x] The `count()` variable check (`cm[2] != reachedVar`) was dead code — see System
  Knowledge. Removed, with the reason recorded in place of the branch.
- [ ] Ten rules, ten hand-written messages, and nothing forces a new rule to carry one: a
  future `return refuse(...)`-less rejection would silently reintroduce the anonymous refusal.
  A vet-style test that asserts `parseCanonicalTraversal` has no bare
  `return canonicalPlan{}, nil, false` after the regex match would close it.

## System Knowledge

- **The count-variable check was unreachable.** `canonicalCountPattern` is anchored to the
  WHOLE return clause, so the counted variable is the only one the clause references — and
  `reachedVar` is derived from that same reference. They are equal by construction. A count
  over anything else (a relationship variable, a literal) references neither endpoint and is
  caught earlier by the "projects neither end" rule. Writing a test for the branch is what
  proved it: the query intended to hit it hit the anchor rule instead.
- **Which end is "reached" follows the RETURN, not the arrow.** `plan.anchor`/`plan.reached`
  are assigned from which endpoint variable the RETURN references, and `plan.reverse` flips the
  traversal accordingly. So `RETURN DISTINCT f.name` on `(f)-[:CALLS]->(e)` makes `e` the
  anchor and demands the filter be on `e` — which reads backwards until you know this rule.
- **`TestMountedCanonicalUnsupportedMultiHopFailsClosed` was mislabelled the same way the
  error message was.** Its query has no `DISTINCT`, so it is refused before its `collect()` is
  ever considered, and it would be refused identically at one hop. The test name repeated the
  message's own wrong story.

## Progress Log

### 2026-08-31
- Diagnosis first, on the live graph, before touching anything: the three-row table above is
  the isolation. Also found that the original query returned zero rows even in its accepted
  form, for an unrelated reason — `.astignore` excludes `internal/ast/antlr/` except four
  whitelisted files, so `plpgsql_splice.go` is deliberately not in the graph at all. That is
  the one documented case where reading the file directly is correct, which is what I had
  done at the time.
- Read the prior art before designing: the knowledge page on the bounded planner's hop
  bound, and the backlog item on `tree-sitter-cypher`.
- T1 landed: `canonicalRefusal`, ten named rules, three explicit outcomes.
- T2 landed: refusal propagated unwrapped; the fallthrough message rewritten.
- T3 found two things the reading had not. First, the `count()` variable check is dead code —
  the query I wrote to test it hit the anchor rule instead, and working out why showed the
  branch is unreachable by construction. Removed. Second, **a regression I introduced**:
  making unreadable endpoints a refusal broke
  `TestChunkedExportMountsAndAnswersBoundPatterns`, whose
  `MATCH ()-[r:calls__function_function]->() RETURN count(r)` had always worked — anonymous
  ends are unreadable to the planner and irrelevant to the engine. Fixed at the cause by
  scoping every refusal to logical relationship types, which is the correct rule for all ten,
  not just that one. Regression pinned by
  `TestMountedCanonicalDoesNotRefusePhysicalMemberTables`.
- T4 landed: a section in the spec with the planned forms and the rule table, plus the agent
  recovery protocol in `internal/ast/rule.go`.
- Full suite green: `go test ./...` clean.
