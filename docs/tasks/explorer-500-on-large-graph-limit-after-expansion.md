---
title: Explorer returned 500 on a large graph — the LIMIT came after the expansion, and the projection read the side without a label
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [ast, server, cypher, ladybug, explorer, performance, buffer-pool]
---

# Explorer returned 500 on a large graph — the LIMIT came after the expansion

## Objective

Opening the AST explorer pointed at a large corpus responded 500:

```
GET /api/graph?context=__project__&project_dir=<corpus>
{"error":"ladybug query: Buffer manager exception: Unable to allocate memory!
          The buffer pool is full and no memory could be freed!"}
```

## Implementation Details

The explorer's default query was:

```cypher
MATCH (n)
OPTIONAL MATCH (n)-[r]->(m)
RETURN ...
LIMIT 300
```

**The `LIMIT` binds to the ROWS, after the expansion.** The engine crosses every node
with every outgoing edge and only then discards everything but 300. On a small graph
this does not show up; the corpus in question has **2,524,089 nodes**, and the
intermediate result exhausts the buffer pool.

Why only now: commit `b5139d5` made `/api/graph` honour `project_dir`. Before that,
with the UI selector pointed at another project, the handler answered about the
SERVER's project — small, and therefore within the limit. The ceiling always existed;
the `project_dir` fix was what exposed it.

The fix is two samples, both bounded at the scan, joined by the handler:

**1. Node sample** — `defaultGraphQuery`:

```cypher
MATCH (n)
WITH n LIMIT 300
WITH collect(n) AS sample, collect(id(n)) AS sample_ids
UNWIND sample AS n
OPTIONAL MATCH (n)-[r]->(m) WHERE id(m) IN sample_ids
RETURN ...
```

The `LIMIT` binds to the nodes before any expansion, so the work is bounded by 300
regardless of the size of the graph. Restricting the far end to the same sample bounds
the edges too. The `OPTIONAL MATCH` preserves the nodes without edges inside the
sample — without it, a graph with no edges would be drawn empty.

**2. Edge sample** — `defaultGraphEdgeQuery`:

```cypher
MATCH (n)-[r]->(m)
WITH n, r, m LIMIT 300
RETURN ...
```

This second one exists because sampling nodes alone draws a field of dots: the first
300 nodes of a repository-shaped graph are all `File` — that is the order in which the
tables are scanned — and `File` points to entities, which fall outside the sample.
Measured: 300 nodes and **zero** edges. The two together give what exists and how it
is connected.

The two were kept separate, rather than merged into one, because each stays a simple
and bounded scan, and either of them coming back empty is harmless. The failure of the
second does not bring the response down: the nodes already collected are still worth
drawing.

### Second half of the same bug: reading a PROPERTY from the far side

Bounding the scan was not enough. The same 500 came back, with the query already
bounded, over the same corpus (now 1,979,175 nodes). The trigger was no longer the size
of the scan — it was the **projection**.

Measured column by column, with the fixed node query running against the corpus:

| projection | result |
|---|---|
| `count(*)` | 200 — 0.35 s |
| `label(n)`, `n.name`, `n.path` | 200 — 0.37 s |
| `CAST(id(m) AS STRING)` | 200 — 0.34 s |
| `label(m)` | 200 — 0.33 s |
| `label(r)` | 200 — 0.35 s |
| **`m.name`** | **500 — buffer pool** |
| **`m.name, m.path`** | **500 — buffer pool** |

`m` is a node **without a label**, so reading a property off it forces the engine to
reach the corresponding column of *every* node table, and the filter
`id(m) IN sample_ids` is applied after that, not before. `id()` and `label()` are
structural and do not cost that access — which is why they pass.

Confirmation from the other direction: the same expansion **without** the filter by id
(a free `OPTIONAL MATCH (n)-[r]->(m)`) projects `m.name` without blowing up — it
returns 31 MB in 2.2 s. It is the combination of filter-by-id + property read that
changes the plan.

The fix is to stop asking for what is not needed. `defaultGraphQuery` now projects
only `id(m)`, `label(m)` and `label(r)` from the far side:

```cypher
RETURN
  CAST(id(n) AS STRING) AS src_id, label(n) AS src_label,
  n.name AS src_name, n.path AS src_path,
  n.cluster AS src_cluster, n.lang AS src_lang,
  CAST(id(m) AS STRING) AS dst_id, label(m) AS dst_label,
  label(r) AS rel_type
```

Nothing is lost, because **`m` is in the sample**: the same `UNWIND` emits every `m`
as an `n` too, with name and path. What was missing for this to work was for the source
side to start overwriting in `extractBuiltinQueryGraph` — before, it only created the
node if it did not already exist, so a node first seen as a destination stayed frozen
as an id-and-label marker. The two queries project the same six source columns, so
overwriting never loses data.

The edge query does not change: it projects `m.name` without a filter by id, which is
the case measured as safe.

### Third pass: it was not an index, it was the whole expansion

With the 500 resolved, the right question was left over — **0.45 s for 300 nodes is not
justified**. The natural hypothesis is a missing index. It is not, and the measurement
proves it:

| stage of the node query | 1,979,175 nodes | 57,673 nodes |
|---|---|---|
| A — pure scan (`WITH n LIMIT 300`) | 0.002–0.005 s | 0.002 s |
| B — A + `collect`/`UNWIND` | 0.014–0.021 s | 0.008 s |
| C — B + free `OPTIONAL MATCH` | 0.16–0.20 s | 0.13–0.15 s |
| D — C + `WHERE id(m) IN sample_ids` | 0.41–0.66 s | 0.30–0.38 s |
| E — same as D, everything with an explicit label | 0.17–0.22 s | 0.16–0.20 s |

**The graph is 34x bigger and the query is 1.2–1.4x slower.** Cost practically constant
with respect to volume — so it is not the scan, and an index does not reach it: an index
reduces how many rows you visit, and this query already visits 300. What it pays for is:

1. **Table fan-out.** The graph has 24 node labels, 9 edge types and 33 physical tables.
   `MATCH (n)-[r]->(m)` without a label becomes one operator per combination
   (source × relation × destination) that exists in the schema. Cost per operator, paid
   once each, independent of whether there are 300 or 3 million nodes. It is the B→C
   jump (0.014 → 0.17 s), and it is what row E confirms: pinning the labels gives back
   C's time.
2. **The `IN` filter over a list.** `id(m) IN sample_ids` is a linear comparison against
   300 elements, evaluated per candidate row, in each one of those operators. It is the
   C→D jump (0.17 → 0.45 s).

Discarded by measurement, the alternative "free expansion capped at 2000 rows": it came
out **slower** (0.56–0.79 s) and preserved **5 of the 300** source nodes — a directory
with 1464 files consumed the entire row budget.

What the expansion delivered, measured: **293 edges, all `Directory-CONTAINS->File`** —
the directory tree, which the explorer's file panel already shows.

Fix: the node query **stops expanding**. It becomes scan and projection, nothing more.
Connectivity now comes entirely from `defaultGraphEdgeQuery`, whose budget went up from
300 to 1000 — its cost is the same fan-out paid once, so 1000 costs what 300 cost
(~0.19 s measured on both). The two budgets became the constants `graphSampleNodes` and
`graphSampleEdges`, which closes the debt of the literal `300`.

With that, `extractBuiltinQueryGraph` went back to its original conditional form: the
overwrite by the source side existed to repair the id-and-label marker the expansion
produced, and without the expansion there is no marker at all.

## Use Cases

### UC-01: Open the explorer over a graph of any size
- **Actor**: web UI (`ExplorerPage`), via `GET /api/graph`.
- **Preconditions**: an indexed project; `project_dir` may name another project.
- **Main Flow**:
  1. `dbForContext` resolves the database of the requested project.
  2. The node sample runs: 300 nodes and the edges between them.
  3. The edge sample runs: up to 300 edges with the nodes at both ends.
  4. Both feed the same `extractBuiltinQueryGraph`, which deduplicates by id.
  5. The response goes out with `nodes`, `links`, `files` and `fileContents`.
- **Alternative Flows**:
  - `cypher_query` in the querystring: the user's query runs in place of the two, and the edge sample is not appended.
- **Error Scenarios**:
  - Edge sample fails: ignored; the nodes are still drawn.
  - Node sample fails: 500 with the database's message, as before.
- **Postconditions**: the work stays bounded by 300 nodes and 300 edges, whatever the size of the graph.
- **Affected Files**: `internal/ast/server.go`.

## Test Cases & Acceptance Criteria

### Feature: the explorer's samples are bounded at the scan
Ref: UC-01

#### Scenario: the node limit comes before the expansion
```gherkin
Given the explorer's default query
When its shape is inspected
Then the LIMIT over the nodes appears before the OPTIONAL MATCH
  And the far end of the edge is restricted to the same sample
```

#### Scenario: the edge sample limits before projecting
```gherkin
Given the explorer's edge query
When its shape is inspected
Then the LIMIT appears before the RETURN
```

#### Scenario: a graph with no edges still draws its nodes
```gherkin
Given a single-file project, indexed
When the explorer's default query is executed against it
Then it runs without error
  And returns at least one row
```

#### Scenario: the node sample does not expand
```gherkin
Given the explorer's default query
When its shape is inspected
Then it contains no OPTIONAL MATCH, no edge pattern, and no sample_ids
  And it projects no property of m
```

#### Scenario: the budgets reach the queries
```gherkin
Given the constants graphSampleNodes and graphSampleEdges
When the two default queries are inspected
Then each one carries the LIMIT of its own constant
  And both constants stay within what the canvas draws and the buffer pool absorbs
```

### Feature: both samples reach the drawing
Ref: UC-01

#### Scenario: a node with no edge and a far endpoint survive the merge
```gherkin
Given a node-sample row with the node "0:1" and no edge
  And an edge-sample row linking "0:2" to "0:3" through CONTAINS
When both are processed by the same extractor
Then the drawing has three nodes
  And the node "0:1" keeps its name, despite having no edge
  And the node "0:3" keeps name and path, instead of becoming an anonymous dot
  And there is exactly one link
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/server.go` | Modified | `LIMIT` now binds at the scan; new edge sample; queries moved to package scope; the node sample stops projecting properties of the far side; the source side now overwrites in `extractBuiltinQueryGraph` |
| `internal/ast/server_graph_query_test.go` | Created | Pins the shape of both queries, that nodes with no edge survive, that the node sample does not read a property of the far side, and that the destination marker is replaced by the real row |

## Trade-offs & Decisions

- **Two queries instead of one.** Merging the samples into a single Cypher would require
  a per-node correlated subquery — `collect(...)[0..3]` ran into nested aggregation in
  the binder. Two simple scans are more predictable and easier to bound.
- **300 nodes and 300 edges.** Keeps the ceiling the original query already intended;
  what changed was where it binds.
- **The edge sample's failure is swallowed.** An explorer with nodes and no edges is
  degradation; a 500 is a blank screen.
- **Cutting the far side's columns, rather than dropping the filter by id.** The other
  measured way out — free expansion, without `WHERE id(m) IN sample_ids` — also does not
  blow up, but returns 31 MB in 2.2 s against 103 KB in 0.37 s. Decision superseded in
  the third pass, which removed the whole expansion: it became irrelevant which column
  of the far side to ask for.
- **The node sample lost the directory tree to gain 6x in speed.** The 293
  `Directory-CONTAINS->File` edges it used to draw left the drawing on the large corpus.
  Accepted because (a) the explorer's file panel already shows that hierarchy, (b) the
  edge budget went up to 1000, more than compensating in count, and (c) on a normal code
  project the edge sample itself already brings `Directory-CONTAINS->File`
  (measured: 421 of the 1000 in this project).
- **Edge budget larger than the node budget (1000 vs 300).** It is the edges that make
  the picture, and the edge sample's cost is table fan-out paid once — 300 and 1000 cost
  the same. Raising nodes, by contrast, would bring no connectivity at all.

## Technical Debt

- [x] ~~The limits are still hardcoded as the literal `300` in both queries.~~ Resolved in
  the third pass: they became `graphSampleNodes` (300) and `graphSampleEdges` (1000), with
  a test that fails if the constant does not reach the query. If they ever become a request
  parameter, the ceiling on the server stays mandatory — it is the number that protects the
  buffer pool.
- [ ] The node sample returns the first ones in scan order, which on a repository graph
  means "files only". A sample stratified by label would give a better opening view.
- [ ] The same trap stays open for the Cypher the user types into the explorer:
  `validateReadOnlyQuery` blocks writes, not cost. A query with a label-less `MATCH` and
  a property read on a side filtered by id brings the request down with the same message,
  and the handler returns a raw 500.
- [ ] The buffer pool 500 reaches the UI as the database's error text. It is worth
  translating into something actionable — the user has no way of knowing the cause is the
  shape of the query.

## System Knowledge

- **On a graph with millions of nodes, `LIMIT` after an expansion protects nothing.**
  The engine materializes the intermediate before cutting. Every visualization query
  here has to bound at the scan.
- **Bounding the scan is not enough: reading a property of a node WITHOUT A LABEL on a
  side filtered by id blows the buffer pool on its own.** `id()` and `label()` are
  structural and come for free; `m.name` forces reaching the `name` column of every node
  table, with the filter by id applied afterwards. In a visualization query, project
  properties only from the side the scan has already pinned.
- **The buffer pool message does not distinguish the two causes.** It is the same
  sentence for "the scan is too big" and for "the projection is too expensive". Faced
  with it, measure column by column: swapping `RETURN` for `count(*)` separates one from
  the other in a single call.
- **`graphit ui` picks a free port.** With an old server still up on 8080, the new one
  comes up on 8081 — and a binary from an earlier session keeps serving the old code on
  the old port. When investigating "the bug came back", check which process is answering
  on the port (`ss -ltnp`) and how old the binary is before concluding anything about the
  source code.
- **`/api/graph`'s response uses `links`, not `edges`.** The keys are `nodes`, `links`,
  `files`, `fileContents`. Checking the wrong key when validating leads to the conclusion
  that there are no edges.
- **`graphit ui` does not accept `--port`** — only `--repo`. The port is chosen on its own
  and comes out on stdout as `localhost:NNNN`.
- An Oracle corpus of 35k files yields ~2.5M nodes in the graph.

## Progress Log

### 2026-08-11
- 500 reproduced; cause isolated in the `LIMIT` after the `OPTIONAL MATCH`.
- Candidate query shapes tested directly against the 2.5M-node graph before writing any
  code.
- Node sample fixed; measured, it revealed zero edges; edge sample added.
- Verified end to end with the real server: large corpus 200 in 1.09 s with 834 nodes and
  332 links; small project 200 in 0.44 s with 433 nodes and 593 links.

### 2026-08-11 (second pass — the 500 came back)

- Reported again by the user, at the same endpoint and with the same message.
- The stale-binary hypothesis was discarded first: `/usr/local/bin/graphit` already
  contained both fixed queries, and **reproduced the 500 anyway**. Still standing, though,
  was a server from an earlier session on 8080 with an intermediate build — noted in
  System Knowledge, because it is what makes "the bug came back" look like a code
  regression.
- Cause isolated by column bisection against the real corpus: the bounded query passes
  with `count(*)`, with `n`'s properties, with `id(m)`, with `label(m)` and with
  `label(r)`; it blows up as soon as `m.name` enters. Full table in Implementation
  Details.
- Fix applied: the far side reduced to `id` and `label`, and the source side now
  overwrites in `extractBuiltinQueryGraph` so the marker is replaced by the node's own
  real row.
- Verified end to end with the freshly compiled binary: a corpus of 1,979,175 nodes
  answers **200 in 0.75 s with 515 nodes and 593 links**; this project, 200 in 0.34 s with
  378 nodes and 300 links. The large corpus's response was audited: **zero** nodes with a
  name equal to the label, **zero** nodes without a `name` property, **zero** links
  pointing at a missing node — that is, suppressing the columns left no marker behind.
- `go test -tags fts5 ./internal/ast/` in full: ok in 67.9 s.
- The MCPs never went through this path: `graphit_ast_query` and `graphit_ast_search`
  answered normally over the same corpus throughout the investigation. The bug was
  exclusive to the explorer's default query.

### 2026-08-11 (third pass — performance)

- The user's question: "is an index missing? it makes no sense for a query to be slow".
  None was missing, and the measurement is in Implementation Details: same query, graph
  34x smaller, same time. Cost constant with respect to volume ⇒ it is not the scan ⇒ an
  index does not reach it.
- Measurement ladder A→E run three times per stage, on both graphs, to separate the cost
  of table fan-out from the cost of the `IN` filter over a list.
- The "expansion capped at 2000 rows" alternative measured and discarded: slower and it
  preserves 5 of 300 nodes.
- The node query stopped expanding; edge budget 300 → 1000; literals became
  `graphSampleNodes`/`graphSampleEdges`; `extractBuiltinQueryGraph` went back to its
  conditional form (without the expansion there is no marker to repair).
- Tests rewritten alongside: `TestDefaultGraphQueryBoundsTheScan`,
  `TestDefaultGraphQueryDoesNotExpand`, `TestGraphSampleBudgetsStayBounded`,
  `TestBothSamplesReachTheDrawing`. `go test -tags fts5 ./internal/ast/` ok in 52.6 s.
- Measured end to end, three calls per project:

  | | before | after |
  |---|---|---|
  | corpus of 1.98M nodes | 0.75 s · 515 nodes · 593 links | **0.33 s cold, 0.12 s warm** · 1215 nodes · 1000 links |
  | this project (57k nodes) | ~0.34 s · 378 nodes · 300 links | **0.15 s cold, 0.024 s warm** · 749 nodes · 1000 links |

  Zero placeholders and zero orphan links in either. Faster AND drawing more graph.
