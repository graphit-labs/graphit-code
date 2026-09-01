---
title: Clicking a node opens the code at the entity's line, with the line highlighted
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [ast, server, ui, explorer, cypher, ladybug]
---

# Clicking a node opens the code at the entity's line, with the line highlighted

## Objective

In the explorer, clicking a node and opening the code took the panel to the **top of the file**.
For a function on line 800 of a 1200-line file, the user had to hunt for the
entity they had just clicked.

The feature did not exist in any of the three layers — it was not a
regression bug:

| layer | what was missing |
|---|---|
| query | neither of the two samples projected `line_number` |
| API/type | `GraphNode` had no line field |
| panel | `CodePanel` received only `content` and `filename`; `highlightedLines` was **syntax** highlighting, not line highlighting |

## Implementation Details

### Server

`graphSideColumns` came into existence to assemble the projection of one side of the row
(`src_`/`dst_`), which made the new column get added in ONE place instead of
four. Both samples use it, and `graphNodeSideFrom` reads both sides with the same
code.

`buildGraphNode` stopped taking six positional strings and now takes
`graphNodeSide`. Direct motivation: the seventh positional parameter — the line — would sit
next to `cluster` and `lang`, and swapping two of them would not be a compile error.

The line goes out in TWO places on the node, on purpose:
- `node["line"]` — where the explorer reads it, to jump;
- `properties["line_number"]` — where the details panel reads the raw properties.

**Line 0 is not sent.** It is the placeholder value of the call target stub
(`is_stub = true`), which has no declaration to open. Omitting it is what keeps "there is no
line to jump to" distinguishable from "line 1".

### The binder trap — and why `querySample` exists

`n` has no label, and in LadybugDB a property **binds if ANY label in the graph
has it**. A graph that has only files and directories has no table with
`line_number`, and asking for the column there does not return an empty column: it returns
`Binder exception: Cannot find property line_number for n`, which **brings down the whole
query** — that is, a new 500 in the explorer, exactly the symptom the two
previous fixes to this endpoint eliminated.

Discovered by the existing test, which indexes a single `package solo` and whose tables
are File, Directory, Field, Parameter and CONTAINS — none with `line_number`.

`querySample` runs the variant with the column and repeats without it **only** if the error
mentions that property; any other error goes up to the caller. In that graph there
is no entity to jump to anyway, so nothing is lost.

### Front-end

- `GraphNode.line?: number`.
- `handleFileClick(path, line?)` stores `sourceLine`; `handleNodeClick` passes
  `node.line`; the "Open Source Code" button likewise.
- `CodePanel` takes `highlightLine`, marks the line with `.target-line` (background in
  `--primary` at 14% and a 3px bar on the left, plus the line number highlighted) and calls
  `scrollIntoView({ block: 'center' })`.

The effect depends on `[highlightLine, content]`, and both dependencies are
necessary: the panel opens **before** the file arrives, so on the first render
there is a line and there is no content; and clicking a second entity in the SAME file
changes only the line.

### Measured cost

| query | today | with the line |
|---|---|---|
| node sample | 0.0052–0.0059 s | 0.0051–0.0061 s |
| edge sample (line on both sides) | 0.131–0.158 s | 0.141–0.153 s |

## Use Cases

### UC-01: Click an entity in the graph and land on its declaration
- **Actor**: explorer user, clicking a node on the canvas.
- **Preconditions**: the node represents an entity with a declaration (not a File, a
  Directory, nor a call stub); the project is indexed.
- **Main Flow**:
  1. `handleNodeClick` receives the `GraphNode`, which carries `line`.
  2. It calls `handleFileClick(node.file, node.line)`.
  3. The panel opens, `sourceLine` is recorded, and the content is fetched from `/api/file`.
  4. When the content arrives, `CodePanel` scrolls the line to the center and marks it.
- **Alternative Flows**:
  - Click on a File or Directory: `line` absent, the file opens at the top, nothing marked.
  - "Open Source Code" button in the details panel: same path.
  - Second entity in the same file: only the line changes; the panel scrolls again.
- **Error Scenarios**:
  - Line past the end of the file (stale index): nothing marked, nothing scrolled, the
    file opens normally.
  - Graph with no table carrying `line_number`: `querySample` repeats without the column and the
    explorer draws the same.
  - `/api/file` fails: previous behavior, error toast.
- **Postconditions**: the panel shows the file positioned at the declaration, with the line
  highlighted.
- **Affected Files**: `internal/ast/server.go`, `internal/ui/src/api/ast.ts`,
  `internal/ui/src/components/ast/ExplorerPage.tsx`,
  `internal/ui/src/components/ast/CodePanel.tsx`.

### UC-02: Run your own Cypher and click a node in the result
- **Actor**: user, via the query bar.
- **Preconditions**: the query returns nodes (`RETURN n`).
- **Main Flow**: `extractUserQueryGraph` lifts `line_number` out of the raw properties
  into `node.line`; from there on it is UC-01.
- **Error Scenarios**: a query returning only scalar columns produces no nodes, and the
  result comes out as a table — unchanged.
- **Postconditions**: the jump works just as in the default view.
- **Affected Files**: `internal/ast/server.go`.

## Test Cases & Acceptance Criteria

### Feature: the line reaches the explorer
Ref: UC-01, UC-02

#### Scenario: entity carries its declaration line
```gherkin
Given a Function node declared on line 441
When the server assembles the graph node
Then the node has line equal to 441
  And its properties have line_number equal to 441
```

#### Scenario Outline: nodes with no declaration carry no line
```gherkin
Given a node of type "<type>" with no declaration line
When the server assembles the graph node
Then the node has no line field

Examples:
  | type                        |
  | call target stub            |
  | File                        |
```

#### Scenario: both sides of an edge preserve their line
```gherkin
Given an edge sample row with a File at the source and a Function on line 441 at the destination
When the extractor processes the row
Then the source node has no line
  And the destination node has line equal to 441
```

#### Scenario: a node coming from a user query carries the line too
```gherkin
Given a typed query that returns a node with the property line_number equal to 441
When the user query extractor processes the record
Then the node has line equal to 441
```

### Feature: the samples run even where the property does not exist
Ref: UC-01

#### Scenario: graph with only files and directories
```gherkin
Given a single-file project, indexed, whose tables have no line_number
When the node and edge samples are executed through the handler's path
Then neither of the two returns an error
  And the node sample returns at least one row
```

### Feature: the panel jumps and highlights
Ref: UC-01

#### Scenario: the requested line is marked and centered
```gherkin
Given a five-line file opened with line 3 requested
When the panel renders
Then line 3 has the target-line class
  And it is the only one marked
  And it was scrolled into view
```

#### Scenario: with no line, the file opens at the top
```gherkin
Given a file opened with no line requested
When the panel renders
Then no line is marked
  And nothing is scrolled
```

#### Scenario: the content arrives after the line
```gherkin
Given the panel opened with line 5 requested and empty content
When the file content arrives
Then line 5 is marked
  And it is scrolled into view
```

#### Scenario: another entity in the same file
```gherkin
Given the panel positioned on line 3 of a file
When only the requested line changes to 5
Then line 5 is scrolled into view
  And there is still exactly one marked line
```

#### Scenario: line past the end of the file
```gherkin
Given a five-line file opened with line 999 requested
When the panel renders
Then no line is marked
  And nothing is scrolled
  And the file is displayed normally
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/server.go` | Modified | `graphSideColumns` assembles one side's projection; `graphNodeSide`/`graphNodeSideFrom` replace the six positional parameters; `line_number` projected in both samples; `querySample` falls back to the variant without the column when the graph does not have it; `extractUserQueryGraph` lifts the line out of the properties |
| `internal/ast/server_graph_node_test.go` | Modified | Tests for the line on both sides, for the stub's 0 and for the user query path |
| `internal/ast/server_graph_query_test.go` | Modified | The minimal-graph test now exercises `querySample`, which is the path the handler uses |
| `internal/ui/src/api/ast.ts` | Modified | `GraphNode.line` |
| `internal/ui/src/components/ast/ExplorerPage.tsx` | Modified | `sourceLine`; the line crosses node click, file click and the open button |
| `internal/ui/src/components/ast/CodePanel.tsx` | Modified | `highlightLine`: marks the line and centers it |
| `internal/ui/src/components/ast/CodePanel.test.tsx` | Created | Six scenarios, including the late arrival of the content |

## Trade-offs & Decisions

- **`querySample` instead of consulting the schema beforehand.** Asking the database which
  tables have `line_number` would cost one query per request, for a
  degenerate case. Try-and-retry costs zero on the normal path and self-corrects
  as soon as the graph gains entities. The price is a string comparison on the error
  message, restricted to the exact property so as not to swallow other binder errors.
- **`graphNodeSide` instead of a seventh parameter.** Appending `line int` at the end
  of six strings would work and would compile; swapping `cluster` with `lang` in a future
  call would also compile. The struct cost ~20 lines and one signature change
  covered by a test.
- **Instant jump, not smooth.** `scrollIntoView` without `behavior: 'smooth'`: this is
  navigation to a declaration, not a nudge — and in a 20-thousand-line file
  the smooth animation would be a journey.
- **The highlight stays, it does not fade.** The user asked for the line highlighted; a highlight
  that disappears in 2s stops answering "which line was it again?" thirty seconds later.

## Technical Debt

- [ ] The jump only happens when the node comes from a sample or from a query that returns
  nodes. A node arriving by some future path has to carry `line` too — there is
  nothing in the type that forces it.
- [ ] `CodePanel` marks one line; entities occupy a range (`line_number` to
  `end_line`). Highlighting the whole block would be more informative, and the graph already has
  `end_line` — projecting and drawing it is what is missing.
- [ ] If the file on disk changed after indexing, the line points to the
  wrong place with no warning at all. There is no freshness check between the graph and the text
  served by `/api/file`.

## System Knowledge

- **In LadybugDB a property binds if ANY label in the graph has it.** In `MATCH (n)`
  with no label, `n.line_number` works in a graph with functions and **breaks the whole query**
  in a graph made only of files. It is not an empty column, it is a `Binder exception` — and the message
  lists the existing tables, which is how "the property does not exist" is told apart from
  "the graph is being rebuilt".
- **`line_number = 0` is a stub placeholder**, not a location. Any consumer that
  treats 0 as a line will jump to the top thinking it got it right.
- **The explorer's side tree is a FILE tree.** Entities are only
  clickable on the graph canvas — which is also why the verification of this
  change went into a component test instead of browser automation.
- Measured in this project: of the 1235 drawn nodes, 1140 have a line; the 95 without are 94
  Directory and one Table stub. In the large corpus, 286 out of 1268 — the rest are File (936)
  and Directory (46).

## Progress Log

### 2026-08-11

- Confirmed that the feature did not exist in any of the three layers, instead of
  assuming a regression.
- The column's cost was measured BEFORE implementing: negligible on both sides.
- Implemented server → type → panel; `buildGraphNode` refactored into a struct along the
  way.
- The minimal-graph test caught the `Binder exception` — a new 500 that would have gone to
  production. Hence `querySample`.
- Verified end to end in the real UI: the response carries `line` only for what has a
  declaration. The graph canvas is not clickable by automation, so the panel's behavior
  was pinned down in a component test (6 scenarios).
- `go test -tags fts5 ./internal/ast/` and `npx vitest run` (34 tests) green; `tsc` and
  `eslint` clean on the changed files.
