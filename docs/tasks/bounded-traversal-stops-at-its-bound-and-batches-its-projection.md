---
title: The canonical traversal planner stops at its hop bound, batches its projection, and specifies its row order
status: done
created: 2026-08-30
updated: 2026-08-30
tags: [ast, ladybug, icebug, query-planner, performance, consistency]
---

# The canonical traversal planner stops at its hop bound, batches its projection, and specifies its row order

## Objective

Found while verifying that the icebug export rewrite
([[icebug-export-streams-instead-of-materializing]]) had not changed query behaviour: a
one-hop `MATCH (a)-[:CALLS]->(b)` against a mounted bundle took 1.8 s at 300 files, 28.9 s
at 1500 and over 180 s at 6000 — with the correct answer every time.

The engineer's instruction was explicit: this code was written by another agent, it is not
their design, so evaluate its reasoning on evidence and decide. What they need is
**performance and consistency**.

### Reasoning

Three defects, all in `internal/ast/ladybug_icebug_canonical.go`, all independent of the
export work (the bundle this reads is byte-identical before and after it — see
`icebug_bundle_dump_probe_test.go`).

**1. A bounded plan did not stop at its bound.** `parseCanonicalTraversal` sets
`minHops, maxHops = 1, 1` for a bare `-[:REL]->` and `1, N` for `*1..N`. The frontier loop
was `for hop := 1; len(frontier) > 0; hop++` with no exit, applying the bound only when
deciding what to admit into `reached`. Everything past `maxHops` was computed and discarded,
and the discarded work is proportional to the DEPTH of the component behind the anchor —
unbounded, and unrelated to the size of the answer.

The file's own header says the unbounded frontier is deliberate: *"runs UNBOUNDED
breadth-first frontiers — termination comes from visited saturation and the caller's
deadline, never from a hop ceiling"*. That is correct and necessary for `*` with no ceiling,
which is untouched here. For a plan that HAS a ceiling it is not a trade-off: the filter
already refuses everything deeper, so the extra hops cannot change any answer. The
differential run below is what settled it — result set identical on all 79 queries.

**2. The projection ran one engine query per reached uid.** `finishCanonicalTraversal`
built `IN [singleUID]` inside `for _, uid := range uids { for _, label := range ... }`,
while the frontier loop directly above it already batched at `icebugTraversalBatchSize`
(512). A result of N rows cost N round trips per candidate label. This is the dominant cost
and it only shows up when the query projects PROPERTIES — `RETURN a.name, a.path`, which is
the shape the `graphit-ast` skill instructs every agent to write. A `RETURN b.uid` query
short-circuits in `uidProjection()` and never reaches this loop, which is why the first
measurements missed it.

**3. The row order was incidental.** It fell out of the iteration — reached uid, then
candidate label, then engine row order — so it was keyed on a uid the caller cannot see and
that disappears from the projection entirely. Batching moves it. Rather than trade one
incidental order for another, the order is now SPECIFIED: sorted on the record's own
canonical key (`icebugRecordKey`), which is reproducible whatever the planner does
underneath and matches the uid-projection path, which already answered in sorted order.

### Justification for the approach

Everything here was decided by a differential harness against a real bundle rather than by
reading. Alternatives dropped:

- **Preserving the old row order while batching.** Not possible in general: reproducing
  "grouped by reached uid" requires knowing which uid produced each record, and the
  projection frequently does not include it. Specifying the order is strictly better than
  either incidental one.
- **Leaving the hop ceiling alone because the header comment says unbounded is the design.**
  The comment describes the unbounded case correctly. Applying it to bounded plans is
  rationalisation, and the differential run disproves any behavioural dependency.
- **Batching only, without the hop ceiling.** They are independent and both measured
  positive; there is no reason to take one.

## Plan & Task Breakdown

- [x] **T1 — Differential harness against a real bundle** — Spec:
  `ladybug_icebug_traversal_diff_probe_test.go`. 79 queries over a copy of this project's own
  bundle (70,033 nodes / 227,573 edges / CALLS across 7 member tables), recording per query
  the row count, a hash of the row ORDER, a hash of the row SET, the error and the timing.
  Done when a before/after pair can be diffed mechanically.
- [x] **T2 — Stop the frontier loop at `maxHops`** — Spec: `tryCanonicalBoundedTraversal`.
  Constraint: `maxHops == 0` means unbounded and must keep saturating.
- [x] **T3 — Batch the projection** — Spec: `finishCanonicalTraversal` batches at
  `icebugTraversalBatchSize`, the same width as the frontier loop.
- [x] **T4 — Specify the row order** — Spec: sort the deduplicated records on
  `icebugRecordKey`. Done when two consecutive executions of the same query return
  byte-identical order.
- [x] **T5 — Regression tests that do not need a real bundle** — Spec:
  `ladybug_icebug_traversal_bounds_test.go`, over a synthetic corpus whose CALLS graph is a
  chain spanning every file, so the depth behind any anchor is the whole corpus.

## Implementation Details

`internal/ast/ladybug_icebug_canonical.go`:

```go
		frontier = next
		if plan.maxHops != 0 && hop >= plan.maxHops {
			break
		}
```

and `finishCanonicalTraversal` inverted from `for uid { for label { IN [uid] } }` to
`for label { for batch of 512 uids { IN [...] } }`, collecting `{key, record}` pairs and
sorting on the key before returning.

## Use Cases

### UC-01: An agent asks who calls a function
- **Actor**: any agent using `graphit_ast_query`, or the Pre-Edit Impact Check in the
  `graphit-ast` skill.
- **Preconditions**: a mounted canonical (icebug) catalog; the query names a LOGICAL
  relationship type, so it cannot run natively and must go through this planner.
- **Main Flow**:
  1. `Query` fails to run the pattern natively and calls `tryCanonicalBoundedTraversal`.
  2. The anchor set is resolved to uids, one query per candidate anchor table.
  3. The frontier expands one hop at a time, batched at 512 uids per member table, and
     STOPS once `maxHops` hops have run.
  4. `finishCanonicalTraversal` answers: a count directly, uids directly, or — for a
     property projection — one batched query per candidate label, deduplicated and sorted.
- **Alternative Flows**:
  - `*` with no ceiling (`maxHops == 0`) keeps expanding to visited saturation.
  - `RETURN b.uid` short-circuits in `uidProjection()` and issues no projection queries.
- **Error Scenarios**:
  - A var-length form the planner cannot preserve fails CLOSED with the member names,
    rather than being forwarded to a recursive plan that enumerates the whole graph.
- **Postconditions**: rows are deduplicated and in a specified, reproducible order.
- **Affected Files**: `internal/ast/ladybug_icebug_canonical.go`.

## Test Cases & Acceptance Criteria

### Feature: A bounded plan stops at its bound
Ref: UC-01

#### Scenario: A one-hop query on a corpus with a deep chain behind the anchor
```gherkin
Given a corpus of 1500 files whose CALLS graph is a chain spanning every file
When a one-hop query is run from an anchor in the middle of the chain
Then it returns exactly 1 row
  And it completes in under 5 seconds
```

#### Scenario Outline: The bound is honoured at every depth
```gherkin
Given the same 1500-file chain corpus
When a "<pattern>" query is run from the same anchor
Then it returns exactly "<rows>" rows

Examples:
  | pattern       | rows |
  | -[:CALLS]->   | 1    |
  | *1..2         | 2    |
  | *1..3         | 3    |
```

### Feature: An unbounded plan still saturates
Ref: UC-01

#### Scenario: `*` reaches the whole component
```gherkin
Given a corpus of 60 files whose CALLS graph is a cycle over all of them
When an unbounded "*" query is run from one anchor
Then it reaches 59 nodes
  And the anchor itself is not re-admitted, because it is seeded into the visited set at depth 0
```

### Feature: The row order is specified
Ref: UC-01

#### Scenario: A property projection is sorted, deduplicated and stable
```gherkin
Given a corpus of 400 files
When a "*1..3" query projecting b.name is executed
Then no row appears twice
  And the rows are in ascending canonical-key order
When the same query is executed a second time
Then the rows come back in exactly the same order
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/ladybug_icebug_canonical.go` | Modified | hop ceiling, batched projection, specified order |
| `internal/ast/ladybug_icebug_traversal_bounds_test.go` | Created | regression tests over a synthetic deep-chain corpus |
| `internal/ast/ladybug_icebug_traversal_diff_probe_test.go` | Created | differential harness against a real bundle |

## Trade-offs & Decisions

- **The row order changed, and that was unavoidable.** Batching regroups the rows. The
  choice was between a second incidental order and a specified one; a specified one is what
  "consistency" means. Measured: 16 of 79 queries return their rows in a different order
  than before, all of them property projections; the SET is identical in all 79.
- **The sort costs O(n log n) over the result**, which is nothing beside the round trips it
  sits behind.
- **`*` with no ceiling gains nothing here.** If unbounded reachability is slow, this change
  does not address it.

## Technical Debt

- [ ] `*3` is parsed as `minHops=3, maxHops=0` — i.e. "3 or more" — where Cypher means
  "exactly 3". Pre-existing, unaffected by this change, and it means such a query still runs
  unbounded. `parseCanonicalTraversal`, `internal/ast/ladybug_icebug_canonical.go:313`.
- [ ] The floor for one hop is still `len(members)` engine queries — 7 for CALLS on this
  corpus. Fusing the member tables would need reader support.
- [ ] The differential probe needs the bundle's `storage` path rewritten by hand after
  copying it. Worth a helper if it gets used often.

## System Knowledge

- **A logical relationship type can never run natively.** `CALLS` and `CONTAINS` map to N
  physical member tables (`calls__function_function`, …) and the engine has no way to fuse
  them, so every such query goes through this planner — including one-hop ones. Querying
  the PHYSICAL member table directly bypasses the planner entirely and is 6–35 ms where the
  logical form was hundreds.
- **`uidProjection()` is a short-circuit that hides projection cost.** `RETURN b.uid` (and
  `RETURN b.uid AS x`) returns straight from the reached set without issuing a single
  projection query. Any benchmark written with `RETURN b.uid` measures the frontier loop
  only and will miss `finishCanonicalTraversal` completely — which is exactly what happened
  during the first pass of this investigation.
- **The anchor is seeded into `visitedDepth` at depth 0**, so a cycle that comes back round
  to it does not re-admit it. An unbounded walk over an N-cycle returns N-1 rows, not N.

## Progress Log

### 2026-08-30
- Reproduced from the export verification: one-hop logical traversal at 1.8 s / 28.9 s /
  >180 s for 300 / 1500 / 6000 files, correct answers throughout.
- Read the planner rather than assuming: found the frontier loop ignoring `maxHops`, and
  then — looking for why property queries were still slow — the per-uid projection loop.
- Built the differential harness FIRST, against a copy of this project's own bundle, so the
  decision would rest on measurement rather than on reading. Baseline: 79 queries,
  125,466 ms, zero errors.
- Applied the hop ceiling and the batched projection. Result:

  | | before | after |
  |---|---|---|
  | total, 79 queries | 125,466 ms | **12,961 ms** (9.7x) |
  | result SET differs | — | **0 of 79** |
  | row COUNT differs | — | **0 of 79** |
  | errors | 0 | **0** |
  | queries made slower | — | **0** |

  Biggest single wins, all of them property projections:
  `contains.props` on `ladybug.go` 26,926 → 651 ms (333 rows), on `direct_icebug.go`
  21,721 → 601 ms (243 rows), on `pipeline.go` 20,549 → 583 ms (273 rows).
- Row ORDER differed in 19 of 79. Confirmed both orders were deterministic, so the choice
  was which one to keep — specified the order by sorting on the canonical record key. After
  that: 16 of 79 differ from the original, and **0 differ between two consecutive runs**.
- Wrote the regression tests over a synthetic deep-chain corpus so the bound is guarded
  without needing a real bundle. First version of the unbounded test expected N rows and got
  N-1; the code was right and the expectation was wrong — the anchor is seeded at depth 0.
  Corrected the test, not the code.
- `go test ./...` green.
