---
title: AST Explorer — hiding a node type did not filter the graph, and "view source" returned 404
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [ui, ast, uiserver, bugfix]
---

# AST Explorer — hiding a node type did not filter the graph, and "view source" returned 404

## Objective

Two visible defects in the web UI's AST explorer (`graphit ui`), reported together:

1. **Hiding a node type in the sidebar did not remove anything from the plotted graph.** The eye in
   the "Node Labels" column toggled the row's visual state (struck through, `EyeOff`), but the
   canvas still had all the nodes.
2. **"Open Source Code" answered 404.**
   `GET /api/file?path=internal%2Fast%2Fladybug_gc_pressure_test.go&project_dir=~/…/graphit-code`
   → `{"error":"File source not found"}`, for a file that is indexed.

The two have independent causes, both on the server (`internal/ast/server.go`) — not one
line of React had to change.

## Implementation Details

### Defect 1 — the node's `label` carried the NAME, not the graph label

`buildGraphNode` (`internal/ast/server.go`) assembled the node like this:

```go
"id": id, "name": displayName, "label": displayName,   // ← nome, duas vezes
"type": label,                                          // ← o rótulo do grafo
```

And `extractUserQueryGraph` — the builder for the path of a Cypher query typed into the bar —
had exactly the same inversion (`"label": name`).

The front-end expects the opposite. `GraphCanvas` filters with
`hiddenLabels.has(n.label)`, and `hiddenLabels` is filled by `SchemaPanel`, which takes
the labels from `/api/schema` — that is, `label(n)`: `File`, `Function`, `Struct`. Comparing
`Function` with `handleFile` never matches, so **nothing was hidden**. The same `n.label`
feeds three other things that were also silently broken:

| Use in `internal/ui/src/components/ast/GraphCanvas.tsx` | What it did before |
|---|---|
| `visibleNodes` → `hiddenLabels.has(n.label)` (line 124) | never hid anything |
| `getNodeColor(n.label)` (lines 309, 595) | colour by entity name — the panel's colour picker had no effect, and the legend did not match the canvas |
| `baseRadius(n)` (lines 57-58) | `File`/`Module`/`Package` never matched: every node with radius 6 |
| `ExplorerPage.tsx:529,544` | the node card showed `LABEL: cli_reference.md` and a redundant "Kind" row |

Fix: `label` and `type` now both carry the graph label; `name` remains the display name.
`type` was kept because `NodeTree.tsx` already depended on it
(`n.type === 'File'`, `codeEntityTypes.has(n.type)`) — the front-end was internally
inconsistent, one half reading `type` and the other `label`.

### Defect 2 — `/api/file` ignored `project_dir`

`handleFile` resolved the search index with
`s.storePathFor(r.URL.Query().Get("context"))`, which only knows the project the server
was started in. Everything else in the explorer (`/api/graph`, `/api/schema`,
`/api/query`, `/api/search`) goes through `dbForContext`, which **honours** `project_dir`.

The result is the exact symptom in the report: with the UI's project selector pointed at
another project, the graph on screen comes from project B and clicking "view source" asks
project A's index for project B's file — which it does not have. 404 on an indexed file.

Fixes:

- `requestedRoot(r)` (new) centralizes the "is it another project?" decision that
  `dbForContext` made inline; `dbForContext` now uses it, with no behaviour change.
- `storePathForRequest(r)` (new) gives the same answer as `dbForContext`, only as a
  path — the file handler needs the path and not the handle, because the file's text
  lives in the search index next to the database (`file_fts`), not in the graph.
- Additional hardening: `DefaultLadybugConfig` builds `.graphit/ast/project/ladybugdb`
  **relative to the process's CWD**. That is only the repo root when the server was
  started from inside it — `graphit ui --repo <path>` from another directory is not.
  When the path comes in relative, it is now resolved against the server's root.

No fallback to reading from disk was added: `handleFile` still serves only what is in
the index, which is what keeps a path traversal
(`path=../../../etc/passwd`) from turning into an arbitrary read.

## Use Cases

### UC-01: Hide/show a node type in the graph
- **Actor**: user of the AST explorer in the web UI.
- **Preconditions**: a context open at `/ast/explorer/<contextId>`, graph loaded.
- **Main Flow**:
  1. The "Schema" tab of the left column lists the labels coming from `GET /api/schema`
     (`SchemaPanel`, `nodes[].label`).
  2. User clicks the eye icon on a label's row → `onToggleLabel(label)`.
  3. `ExplorerPage` adds/removes the label in `hiddenLabels` (new `Set`).
  4. `GraphCanvas` recomputes `visibleNodes`/`visibleLinks` and calls `fg.graphData(...)`,
     preserving the positions of the nodes that stay visible.
- **Alternative Flows**:
  - The same path holds for "Clusters" and "Languages", which filter by
    `n.properties.cluster` / `n.properties.lang` — those were never broken.
  - `hiddenLabels` is persisted in `localStorage` (`graphit_hidden_labels`) and reapplied
    when the explorer is reopened.
- **Error Scenarios**:
  - Hiding every label leaves the canvas empty; no exception — `graphData` accepts
    empty lists.
  - Edges whose two ends are not both visible disappear along with them (filter in `visibleLinks`).
- **Postconditions**: the canvas plots only nodes whose label is not in `hiddenLabels`.
- **Affected Files**: `internal/ast/server.go` (`buildGraphNode`,
  `extractUserQueryGraph`), `internal/ui/src/components/ast/GraphCanvas.tsx`,
  `internal/ui/src/components/ast/SchemaPanel.tsx`,
  `internal/ui/src/components/ast/ExplorerPage.tsx`.

### UC-02: Open a node's source code
- **Actor**: user of the AST explorer.
- **Preconditions**: a selected node that has `file`, or a file chosen in the "Tree" tab.
- **Main Flow**:
  1. Click on "Open Source Code" (or on a file row in the tree) →
     `handleFileClick(path)`.
  2. `astApi.getFile(path, contextId, activeProjectDir)` →
     `GET /api/file?path=…&context=…&project_dir=…`.
  3. `handleFile` resolves the store with `storePathForRequest(r)` and reads the text from
     the search index with `FileSourceAt(store + SearchIndexSuffix, path)`.
  4. The right panel opens with the content (`CodePanel`).
- **Alternative Flows**:
  - Without `project_dir`, or with `project_dir` equal to the server's root: reads the
    store of its own project.
  - With the `context` of an imported context: reads that context's store — global when it
    is the project itself, `<project_dir>/.graphit/ast/<ctx>/ladybugdb` (with the symlink
    resolved) when it is another project.
- **Error Scenarios**:
  - `path` missing → 400 `path param required`.
  - File not indexed in that project → 404 `File source not found`; the UI shows the
    toast "Failed to load file" and `// Could not load file content.`.
- **Postconditions**: the code panel shows the file of the project the request named.
- **Affected Files**: `internal/ast/server.go` (`handleFile`, `storePathForRequest`,
  `requestedRoot`), `internal/ui/src/api/ast.ts`,
  `internal/ui/src/components/ast/ExplorerPage.tsx`.

## Test Cases & Acceptance Criteria

### Feature: the node's label in the graph payload
Ref: UC-01

#### Scenario: a named node carries the graph label in `label`
```gherkin
Given a graph row whose label is "Function" and whose name is "handleFile"
When the server assembles the node for the viewer
Then the "label" field is "Function"
  And the "type" field is "Function"
  And the "name" field is "handleFile"
```

#### Scenario: a file node with no path falls back to its own name as the path
```gherkin
Given a graph row with label "File", name "server.go" and an empty path
When the server assembles the node for the viewer
Then the "label" field is "File"
  And the "file" field is "server.go"
```

#### Scenario: a node with no name displays its own path
```gherkin
Given a graph row with label "Directory", an empty name and path "internal/ast"
When the server assembles the node for the viewer
Then the "name" field is "internal/ast"
  And the "label" field is "Directory"
```

#### Scenario: nodes coming from a user's Cypher query follow the same contract
```gherkin
Given a query record with Label "Function" and the name property "handleFile"
When the user query graph extractor assembles the node
Then the "label" field is "Function"
  And the "name" field is "handleFile"
```

#### Scenario: hiding a label removes those nodes from the canvas
```gherkin
Given the explorer open with 308 plotted nodes, of which 115 have label "File"
When the user clicks the eye on the "File" row in the schema panel
Then the plotted graph now has 193 nodes
  And no plotted node has label "File"
When the user clicks the eye on "File" again
Then the plotted graph goes back to 308 nodes
```

### Feature: /api/file honours the project the request names
Ref: UC-02

#### Scenario: serving a file from another project
```gherkin
Given a server started in project A
  And project B has "internal/ast/ladybug_gc_pressure_test.go" in its search index
When a GET /api/file arrives with the path of that file and the project_dir of project B
Then the response is 200
  And the body carries the content indexed in project B
```

#### Scenario Outline: without project_dir, it serves its own project
```gherkin
Given a server started in project A, which has "cmd/main.go" indexed
When a GET /api/file arrives with path "cmd/main.go" and "<extra query>"
Then the response is 200
  And the body carries the content indexed in project A

Examples:
  | extra query              |
  | (none)                   |
  | project_dir=<root of A>  |
  | context=__project__      |
  | project_dir= (empty)     |
```

#### Scenario: a file that is not in the requested project is still a 404
```gherkin
Given a server started in project A, which has "cmd/main.go" indexed
  And project B does not have "cmd/main.go" in its index
When a GET /api/file arrives with path "cmd/main.go" and the project_dir of project B
Then the response is 404
```

#### Scenario: request without path
```gherkin
Given a server started in any project
When a GET /api/file arrives without the path parameter
Then the response is 400
```

#### Scenario: a relative store is resolved against the server's root
```gherkin
Given a server whose repoPath is "/tmp/proj" and whose backend points at the relative
      path ".graphit/ast/project/ladybugdb"
When the server resolves the store for a request without project_dir
Then the resolved path is "/tmp/proj/.graphit/ast/project/ladybugdb"
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/server.go` | Modified | `buildGraphNode` and `extractUserQueryGraph` now emit the graph label in `label`; `handleFile` now resolves the store through `storePathForRequest`; new `requestedRoot` and `storePathForRequest`; `dbForContext` reuses `requestedRoot` |
| `internal/ast/server_file_handler_test.go` | Created | Regression for the cross-project 404, the default path, the legitimate 404 and relative store resolution |
| `internal/ast/server_graph_node_test.go` | Created | Pins the contract `name` = display / `label` = `type` = graph label, in both builders |
| `docs/tasks/ast-explorer-hide-labels-and-file-source-404.md` | Created | This record |

## Trade-offs & Decisions

- **Fix on the server, not in the front-end.** The alternative was to swap `n.label` for
  `n.type` at the four points in `GraphCanvas`/`ExplorerPage`. It was discarded: `label`
  duplicated `name` exactly, there was no consumer for "label = name", and `NodeTree`
  already read `type` as the label. Fixing it on the server leaves both fields coherent
  with the TypeScript interface (`GraphNode`) and with `/api/schema`, and fixes all four
  uses at once without a UI rebuild.
- **`type` kept duplicating `label`.** It could have been removed, but `NodeTree.tsx`
  depends on it and `writeGraphResponse` uses `n["type"] == "file"` to build the `files`
  list. Keeping both is the compatible step; unifying them is debt recorded below.
- **`storePathForRequest` mirrors `dbForContext` instead of refactoring it.** `dbForContext`
  has five callers and zero test coverage, and the "same project, no context" branch
  returns the already-open handle (`s.db`), which has no equivalent path. Only the shared
  decision — "is it another project?" — was extracted into `requestedRoot`, which is
  exactly the point where the two had diverged.
- **No fallback to reading from disk in `handleFile`.** It would also solve "file created
  after the last indexing", but it would open path traversal on an endpoint that today is
  safe by construction (it only serves what the index has). It stays as debt with an
  explicit scope.

## Technical Debt

- [ ] `GraphNode.type` and `GraphNode.label` now carry the same value. Unifying them means
  touching `NodeTree.tsx` (`n.type === 'File'`, `codeEntityTypes`), `writeGraphResponse`
  (`n["type"] == "file"`) and the interface in `internal/ui/src/api/ast.ts` — and
  rebuilding the bundle. As long as it lasts, `ExplorerPage.tsx:546` never renders the
  "Kind" row, because the condition is `type !== label`.
- [ ] `handleExportBundle` (`internal/ast/server.go`) still uses `storePathFor("")` +
  `s.db`: it is always the server's project, and it would ignore `project_dir` if it ever
  received one. Today the request body has no such field, so it is not a bug — it is an
  undocumented limit in the UI.
- [ ] `/api/file` has no fallback to disk: a file saved after the last indexing answers
  404 until the daemon reindexes. If it is going to be solved, it has to come with path
  containment against the project root.
- [ ] The selected-node card (`ExplorerPage.tsx:529`) uses `labelColor(label)` directly,
  ignoring the custom colour in `nodeColors` that the canvas respects. It only became
  visible now that `label` is the real label.

## System Knowledge

- **The files' text is not in the graph.** It lives in `file_fts.source`, in the sqlite
  next to the database (`<dbPath>.search.sqlite`), and `FileSourceAt` reads it read-only,
  deliberately outside `OpenSearchIndex` — which runs `migrateSearchSchema` and DROPs
  `file_fts` when the schema version differs. Reading a source must never destroy the
  index.
- **The system sqlite does not open that index**: `sqlite3 … "SELECT … FROM file_fts"` fails
  with `no such module: fts5`. To inspect it, use the MCP tools (`ast_source`) or a Go
  binary with `-tags fts5`.
- **Every explorer request carries `project_dir`**, even when it is its own project —
  the `appStore` (zustand, persisted) keeps `activeProjectDir` and every `astApi.*` passes
  it along. Any new handler has to decide what to do with it; ignoring it is the bug of
  this task.
- **`DefaultLadybugConfig().DBPath` is relative** (`.graphit/ast/project/ladybugdb`).
  Handlers that derive a path from it need to anchor at the server's root.
- **`pkill -f "<binário> ui"` kills the agent's own shell**, because the pattern matches the
  command line of the `bash -c` that runs it. Killing by PID (via `pgrep`/`ss -lntp`) is
  the way.

## Progress Log

### 2026-08-11
- The 404 reproduced: two `graphit ui` servers, one in each project. From the neighbouring
  project's server, `/api/graph?project_dir=…/graphit-code` returns 318 nodes of
  graphit-code (it honours `project_dir`) and `/api/file` with the same `project_dir`
  returns 404.
- The `label` defect reproduced in the real payload: `{"label": "copy_test.go", "type":
  "File"}`.
- `internal/ast/server.go` fixed at all three points; new tests written before validating,
  and confirmed red against the old code (with `server.go` temporarily restored) and green
  against the new one.
- `go vet` clean in `internal/ast` and `internal/uiserver`; the `internal/ast` suite ran.
- Verified end to end in the browser with a recompiled binary: hiding "File" takes the
  plotted graph from 308 → 193 nodes, with zero `File` nodes left; showing it again goes
  back to 308. Clicking a file in the tree fires `GET /api/file` → 200 and the right panel
  renders the content. The node card now shows `LABEL: File`.
