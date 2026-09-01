---
title: The Explorer returned with 500 in a large graph - the LIMIT followed after expansion, and projection read the side without labels.
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [ast, server, cypher, ladybug, explorer, performance, buffer-pool]
---

Explorer returned a 500 on a large graph - the LIMIT followed after expansion.

## Objective

Open the Explorer pointing at a large corpus responded with 500:

```
GET /api/graph?context=__project__&project_dir=<corpus>
{"error":"ladybug query: Buffer manager exception: Unable to allocate memory!
          The buffer pool is full and no memory could be freed!"}
```

## Implementation Details

The default query of the Explorer was:

```cypher
MATCH (n)
OPTIONAL MATCH (n)-[r]->(m)
RETURN ...
LIMIT 300
```

The `LIMIT` connects to the lines, after expansion. The motor crosses all nodes with all output edges and only then discards everything except 300. In a small graph, this does not appear; the corpus in question has **2,524,089 nodes**, and the intermediate result exhausts the buffer pool.

Why now: The commit INLINE_1 made INLINE_2 honor INLINE_3. Before,
with the UI selector pointing to another project, the handler responded about the server project—small and thus within the limit. The ceiling always existed; the correction of INLINE_4 exposed it.

The correction consists of two samples, both limited in scope to the scan, joined by the handler:

Sample of us — `defaultGraphQuery`:

```cypher
MATCH (n)
WITH n LIMIT 300
WITH collect(n) AS sample, collect(id(n)) AS sample_ids
UNWIND sample AS n
OPTIONAL MATCH (n)-[r]->(m) WHERE id(m) IN sample_ids
RETURN ...
```

The `LIMIT` connects the nodes before any expansion, so the work is limited by 300 independently of the size of the graph. Restricting the other end to the same sample limits the edges as well. The `OPTIONAL MATCH` preserves the nodes without edges within the sample — without it, a graph with no edges would be drawn empty.

This translation aims to maintain the technical nature and structure of the original Portuguese text while rendering it in idiomatic English.

**Sample Edges** — `defaultGraphEdgeQuery`:

```cypher
MATCH (n)-[r]->(m)
WITH n, r, m LIMIT 300
RETURN ...
```

This second exists because only we draw a field of points: the first 300 nodes of a graph with repository shape, all `File` — is the order in which tables are read — and `File` points to entities that fall outside the sample. Measured: 300 nodes and **zero** edges. Together they give what exists and how it is connected.

Inline codes (`File`, `File`) should be replaced with actual values or placeholders as needed for clarity in translation.

The two remained separate and not fused into one, because each maintains her own simple and limited scan, and any of them coming empty is harmless. The failure of the second does not undermine the answer: the nodes already collected still hold value for the design.

### Segunda metade do mesmo bug: ler PROPRIEDADE do lado oposto

Limiting the scan wasn't enough either. The same 500 returned, with the query already limited to the same corpus (now 1,979,175 instances). The trigger was no longer just about the size of the scan — it was the **projection**.

---


Measured column by column, with the corrected query running against the corpus:

| projection | result |
|---|---|
| `count(*)` | 200 – 0.35 s |
| `label(n)`, `n.name`, `n.path` | 200 – 0.37 s |
| `CAST(id(m) AS STRING)` | 200 – 0.34 s |
| `label(m)` | 200 – 0.33 s |
| `label(r)` | 200 – 0.35 s |
| **`m.name`** | **500 – buffer pool** |
| **`m.name, m.path`** | **500 – buffer pool** |

`INLINE_20` is a node with no label; therefore, reading one of its properties forces the engine to reach the corresponding column in every table of nodes, and the filter `INLINE_21` is applied afterward, not before. `INLINE_22` and `INLINE_23` are structural and do not cost this access—therefore they pass.

Confirmation on the other side: The same expansion **without** the filter by ID projects `OPTIONAL MATCH (n)-[r]->(m)` without bursting — returns 31 MB in 2.2 seconds. This is the combination of filter-by-ID + reading property that changes the plan.

The correction is to stop asking for what is not necessary. __INLINE_26__ passes only ___INLINE_27__, ___INLINE_28__, and ___INLINE_29__:

```cypher
RETURN
  CAST(id(n) AS STRING) AS src_id, label(n) AS src_label,
  n.name AS src_name, n.path AS src_path,
  n.cluster AS src_cluster, n.lang AS src_lang,
  CAST(id(m) AS STRING) AS dst_id, label(m) AS dst_label,
  label(r) AS rel_type
```

Nothing is lost because `m` is in the sample: the same `UNWIND` emits all `m` also, emitting it with name and path. What was missing for this to work was for the origin to pass overwriting on `extractBuiltinQueryGraph` — before it only created the node if it didn't already exist; so a node seen as a destination would freeze like an ID label. The two queries project the same six columns of origin, so never losing data is achieved by simply reusing the original values.

The query on edges does not change: it projects INLINE_35 without filtering by ID, which is the case measured as safe.

Third Passage: Not an index, but the entire expansion

With the 500 resolved, the correct question remains: "It does not justify 0.45 seconds for 300 knots." The natural hypothesis is missing an index. Not true, and measurement proves:

The query stage of the network is as follows:

| Stage of Query on Nodes | 197,917,5 nodes | 57,673 nodes |
|---|---|---|
| A — Pure Scan (___INLINE_36__) | 0.2–0.5 s | 0.2 s |
| B — A + ___INLINE_37__ / ___INLINE_38__ | 1.4–2.1 s | 0.8 s |
| C — B + ___INLINE_39__ free | 16–20 s | 13–15 s |
| D — C + ___INLINE_40__ | 41–66 s | 30–38 s |
| E — identical to D, all with explicit labels | 1.7–2.2 s | 1.6–2.0 s |

Note: The inline codes (___INLINE_36__, ___INLINE_37__, ___INLINE_38__, ___INLINE_39__, and ___INLINE_40__) are placeholders for actual code or identifiers that should be replaced with the specific values relevant to your query setup.

The graph is 34 times larger and the query is 1.2-1.4x slower. The cost remains almost constant relative to volume — so it's not a scan, and an index doesn't reach: an index reduces how many rows you visit; this query already visits 300 rows. What she pays is:

Translation:
The graph is 34 times larger and the query is 1.2-1.4x slower. The cost remains almost constant relative to volume — so it's not a scan, and an index doesn't reach: an index reduces how many rows you visit; this query already visits 300 rows. What she pays is:


1. **Fan-out of tables.** The graph has 24 node labels, 9 types of edges, and 33 physical tables.
   `MATCH (n)-[r]->(m)` without a label becomes an operator by combination (origin × relation × destination) that exists in the schema. The cost per operator is paid once for each, regardless of whether there are 300 or 3 million nodes. It's the jump B→C (0,014 → 0,17 seconds), and it's what line E confirms: fixing labels returns the time to C.
2. **Filter on list `IN`:** `id(m) IN sample_ids` is a linear comparison against 300 elements, evaluated by candidate lines in each of those operators. It's the jump C→D (0,17 → 0,45 seconds).

Rejected by measurement, the alternative "free expansion capped in 2000 lines" became faster (0.56–0.79 seconds) and preserved 5 of the 300 roots — a directory with 1464 files consumed the entire line budget.

What the expansion delivered, measured: **293 directories**, all __inline__ 44__. - The directory tree that Explorer's file pane already shows.

Correction: the query **stops expanding**. It becomes a scan and projection, nothing more.
The connectivity now passes entirely through ___INLINE_45__, whose budget has increased from 300 to 1000 — the cost of it is the same fan-out paid once, so 1000 costs what 300 would have cost (~0.19 seconds measured in both). The two budgets now became constants `graphSampleNodes` and `graphSampleEdges`, closing the debt of the literal `300`.


With this, INLINE\_49 returned to its original conditional form: the override existed solely to repair the marker of ID-E-Label that expansion produced, and without expansion there is no marker at all.

## Use Cases

### UC-01: Open Explorer on any graph size

- **Actor**: UI web (`ExplorerPage`), via `GET /api/graph`.
- **Preconditions**: an indexed project; `project_dir` can name another project.
- **Main Flow**:
  1. `dbForContext` resolves the requested project's database.
  2. The sample of nodes runs: 300 nodes and edges between them.
  3. The sample of edges runs: up to 300 edges with nodes at both ends.
  4. Both feed into the same `extractBuiltinQueryGraph`, which duplicates by ID.
  5. The response comes with `nodes`, `links`, `files`, and `fileContents`.

- **Alternative Flows**:
  - `cypher_query` in the querystring: user's query runs instead of both samples, and edges are not added.
- **Error Scenarios**:
  - Edge sample fails: ignored; nodes still drawn.
  - Node sample fails: 500 with message from database, as before.

- **Postconditions**: The work is limited to 300 nodes and 300 edges, regardless of the graph's size.
- **Affected Files**: `internal/ast/server.go`

## Test Cases & Acceptance Criteria

Feature: The Explorer samples are limited during scanning

Scenario: The limit of nodes comes before expansion
```gherkin
Given the standard Explorer query
When it is inspected in its form.
Then the LIMIT appears before the OPTIONAL MATCH.
The other end of the edge is restricted to the same sample.
```

Scenario: The sample limits before projecting
```gherkin
Given a query de arestas do explorer
When it is inspected.
Then o LIMIT aparece antes do RETURN
```

Scenario: A graph still draws its nodes even without edges
```gherkin
Given an indexed single-file project
When a standard Explorer query is executed against it
Then ela roda sem erro
  And devolve pelo menos uma linha
```

Scenario: No sample does not grow
```gherkin
Given the standard Explorer query
When it is inspected.
Then it does not contain an OPTIONAL MATCH, an EDGE PATTERN, or SAMPLE_IDS.
And does not project any property of m
```

Scenario: The budgets reach the queries
```gherkin
Given as constantes graphSampleNodes e graphSampleEdges
When two default queries are inspected
Then cada uma carrega o LIMIT da sua constante
Both constants are within what the canvas draws and the buffer pool absorbs.
```

### Feature: as duas amostras chegam ao desenho
Ref: UC-01

Scenario: The node survives without an edge and the distant endpoint after the merge
```gherkin
Given an edge of the sample graph with node "0:1" and no edges
  And uma linha da amostra de arestas ligando "0:2" a "0:3" por CONTAINS
When two are processed simultaneously by the same extractor
The drawing has three knots.
And the node "0:1" retains its name, despite not having an edge.
And the "0:3" node retains name and path, rather than becoming a generic point.
  And existe exatamente um link
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/server.go` | Modified | The INLINE_62__ passes to the scan; new sample of edges; queries moved to package scope; sample of nodes no longer projects opposite property; destination marker is replaced by real line |

| `internal/ast/server_graph_query_test.go` | Created | Fixes the shape of two queries that survive nodes without edges, the sample of nodes does not read opposite properties, and the destination marker is replaced by the actual line |

## Trade-offs & Decisions

- **Two queries instead of one.** Merging the samples into a single Cypher would require an inline correlated subquery — `collect(...)[0..3]` hit nested aggregation in the binder. Two simple scans are more predictable and easier to limit.
- **300 nodes and 300 edges.** Maintains the ceiling that the original query was aiming for; what changed was where it connects.
- **The failure of the edge sample is swallowed.** An explorer with nodes but no edges is degradation; a 500 is blank screen.
- **Cutting the opposite side columns instead of dropping the filter by id.** The other measure — free expansion, without `WHERE id(m) IN sample_ids` — also doesn’t overflow, but returns 31 MB in 2.2 seconds against 103 KB in 0.37 seconds. Decision made on the third pass, which removed the full expansion: it became irrelevant what column from the opposite side to request.
- **The sample of nodes lost the directory tree for gaining six times speed.** The 293 edges she drew that she designed left the drawing in the corpus large. Accepted because (a) the explorer’s file panel already shows this hierarchy, (b) the edge budget increased to 1000, more than compensating in number, and (c) in a normal code project, the own sample of edges already brings `Directory-CONTAINS->File` (measured: 421 out of 1000 in this project).
- **Edge budget larger than node budget (1000 vs 300).** They are what make up the figure, and the cost of the edge sample is fan-out tables paid once — 300 and 1000 cost the same. Increasing nodes, on the other hand, would not bring any connectivity at all.

Note: The inline codes (`collect(...)[0..3]`, `WHERE id(m) IN sample_ids`, `Directory-CONTAINS->File`, `Directory-CONTAINS->File`) are placeholders for actual code snippets that should be replaced with the corresponding lines of code.

## Technical Debt

- [x] The limits are embedded as literal `300` in both queries. Resolved on the third pass: they became `graphSampleNodes` (300) and `graphSampleEdges` (1000), with a test that fails if the constant does not reach the query. If at some point they become request parameters, the ceiling on the server remains mandatory — it is the number that protects the buffer pool.
- [ ] The sample of nodes returns the first in the scan order, which in a repository graph means "only files". Stratified by label would give a better view of opening.
- [ ] The same trap continues open for Cypher as the user enters into the explorer: `validateReadOnlyQuery` written bar, not cost. A query with `MATCH` without label and reading properties on one side filtered by ID drowns the request with the same message, and the handler returns a 500 error.
- [ ] The 500 from the buffer pool reaches the UI as an error text of the database. It should be translated into something actionable — the user cannot know that the cause is the form of the query.

## System Knowledge

- **In a graph with millions of nodes, `LIMIT` after an expansion does not protect anything.**
  The materializer materializes the intermediate before cutting. All query visualizations here must be limited by scanning.
- **Limiting the scan is not enough: reading a node property without label on a filtered side alone exhausts the buffer pool.** `id()` and `label()` are structural and come for free; `m.name` requires reaching column `name` of every table of nodes, with the id filter applied after. In a query visualization, project only the property on the side that the scan has already fixed.
- **The buffer pool message does not distinguish between the two causes.** It is the same phrase for "the scan is too large" and for "the projection is too expensive". Before it, measure column by column: swapping `RETURN` with `count(*)` separates one from another in a call.
- **`graphit ui` chooses the open port.** With an old server still running on 8080, the new goes to 8081 — and a binary session previously serving the old code continues to serve the old code on the original port. To investigate "the bug came back", check which process handles the port (`ss -ltnp`) and when is the binary before concluding anything about the source code.
- **The response of `/api/graph` uses `links`.** The keys are `nodes`, `links`, `files`, `fileContents`. Confusing the key to validate leads to concluding that there are no edges.
- **`graphit ui` does not accept `--port` — only ___INLINE_92__. The port is chosen alone and exits as `localhost:NNNN`.
- A 35k Oracle corpus generates ~2.5M nodes in the graph.

Note: Inline numbers are placeholders for specific lines or code snippets that should be replaced with actual values when translating into English.

## Progress Log

### August 11, 2026
- 500 instances processed; isolated cause in `LIMIT` after ___INLINE_95__.
- Candidate query forms tested directly against a graph of 2.5 million nodes before any code is written.
- Corrected node sample; measurement revealed no edges; added edge sample.
- Verified endpoint to endpoint with the real server: corpus large at 200 in 1.09 seconds, 834 nodes and 332 links; small project at 200 in 0.44 seconds, 433 nodes and 593 links.

Note: The inline references (`LIMIT` and ___INLINE_95__) are placeholders for specific code segments or identifiers that should be replaced with actual values when translating to English.

### 2026-08-11 (segunda passagem — o 500 voltou)

- Reported again by the user, at the same endpoint and with the same message.
- The old binary hypothesis was discarded: `/usr/local/bin/graphit` already contained both corrected queries, and **reproduced the 500 anyway**. It stood, however, a previous session server on port 8080 with an intermediate build — noted in System Knowledge because it makes "bug regression" seem like code regression.
- The isolated cause by column bisecting against the real corpus: the limited query passes through `count(*)`, with properties of `n`, `id(m)`, `label(m)`, and `label(r)`; it fails as soon as `m.name` enters. The complete table is in Implementation Details.
- Applied correction: the opposite side reduced to `id` and `label`, and the origin started overwriting in `extractBuiltinQueryGraph` so that the marker was replaced by the actual line of its own node.
- Verified end-to-end with the newly compiled binary: 1.979.175 nodes respond **200 in 0.75 seconds with 515 nodes and 593 links**; this project, 200 in 0.34 seconds with 378 nodes and 300 links. Audited the response of the large corpus: **zero** nodes with the same name as the label, **zero** nodes without property `name`, **zero** pointing links to absent nodes — that is, the suppression of columns left no marker behind.
- `go test -tags fts5 ./internal/ast/` complete: ok in 67.9 seconds.
- The MCPs never passed through this path: `graphit_ast_query` and `graphit_ast_search` responded normally about the same corpus throughout the investigation. The bug was exclusive to the standard explorer query.

Note: The inline codes (`/usr/local/bin/graphit`, `count(*)`, etc.) are placeholders for actual code snippets that would be provided in a real translation context.

### 2026-08-11 (terceira passagem — performance)

User's Question:
"Is there an index? It doesn't make sense for a query to take so long." Not missing,
and the measurement is in Implementation Details: same query, graph 34x smaller, same time.
Constant cost relative to volume ⇒ not a scan ⇒ index does not reach.

Measurement A → E three times per stage on both graphs to separate
the cost of fan-out tables from the cost of filtering `IN` over list.
Alternative "cap-sized expansion" measured and discarded: slower, preserves 5 out of 300 nodes.
Query no longer expands; budget for edges 300 → 1000; literals became
`graphSampleNodes`/`graphSampleEdges`; `extractBuiltinQueryGraph` returned to conditional form (no marker needed).
Re-tested together: `TestDefaultGraphQueryBoundsTheScan`, `TestDefaultGraphQueryDoesNotExpand`, `TestGraphSampleBudgetsStayBounded`,
`TestBothSamplesReachTheDrawing`. `go test -tags fts5 ./internal/ast/` passed in 52.6 seconds.

Measured end-to-end, three calls per project:

Translation:
User's Question:
"Is there an index? It doesn't make sense for a query to take so long." Not missing,
and the measurement is in Implementation Details: same query, graph 34x smaller, same time.
Constant cost relative to volume ⇒ not a scan ⇒ index does not reach.

Measurement A → E three times per stage on both graphs to separate
the cost of fan-out tables from the cost of filtering `IN` over list.
Alternative "cap-sized expansion" measured and discarded: slower, preserves 5 out of 300 nodes.
Query no longer expands; budget for edges 300 → 1000; literals became
`graphSampleNodes`/`graphSampleEdges`; `extractBuiltinQueryGraph` returned to conditional form (no marker needed).
Re-tested together: `TestDefaultGraphQueryBoundsTheScan`, `TestDefaultGraphQueryDoesNotExpand`, `TestGraphSampleBudgetsStayBounded`,
`TestBothSamplesReachTheDrawing`. `go test -tags fts5 ./internal/ast/` passed in 52.6 seconds.

Measured end-to-end, three calls per project:

| | Before | After |
|---|---|---|
| corpus of 1,98M nodes | 0.75s · 515 nodes · 593 links | **0.33 s cold, 0.12 s warm** · 1,215 nodes · 1,000 links |
| this project (57k nodes) | ~0.34 s · 378 nodes · 300 links | **0.15 s cold, 0.024 s warm** · 749 nodes · 1,000 links |

Zero placeholders and orphaned links in both. Faster drawing more graph.
