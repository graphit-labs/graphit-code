---
title: AST Explorer - did not filter a type of node, and "view source" returned 404
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [ui, ast, uiserver, bugfix]
---

# AST Explorer - hiding a node type did not filter the graph, and "View Source" returned 404

## Objective

Two visible defects in the AST Explorer of the UI Web (`graphit ui`), reported together:

1. "Hiding a type of node on the side did not remove anything from the plotted graph."
   The eye in the column "Node Labels" alternated between the visual state of the line (dashed, `EyeOff`), but the canvas remained with all the nodes.
2. "The 'Open Source Code' responded 404."
   `GET /api/file?path=internal%2Fast%2Fladybug_gc_pressure_test.go&project_dir=/home/…/graphit-code`
→ Inline 3, for an indexed file.

The two have independent causes, both on the server (__INLINE_4__). No line of React needed to be changed.

## Implementation Details

Defect 1 — The name was carried by the node, not the graph label.


___

6 INLINE_6___ (7 INLINE_7___) was setting up the node like this:

```go
"id": id, "name": displayName, "label": displayName,   // ← nome, duas vezes
type: label,  // ← The label of the graph
```

And `extractUserQueryGraph` — the constructor for the path query Cypher line entered in the bar—
had exactly the same inversion (`"label": name`).

The front-end expects the opposite. `GraphCanvas` filters with
`hiddenLabels.has(n.label)`, and `hiddenLabels` is filled by `SchemaPanel`, which strips
the labels from `/api/schema` — or, in other words, `label(n)`: `File`, `Function`, `Struct`. Comparing
`Function` with `handleFile` never works, so **nothing was hidden**. The same `n.label`
feeds three other things that were also broken in silence:

---

Note: Inline codes and markdown are preserved as is.

Use in `internal/ui/src/components/ast/GraphCanvas.tsx` | What did he/she do before |
--- | ---
`visibleNodes` → `hiddenLabels.has(n.label)` (line 124) | never hid |
`getNodeColor(n.label)` (lines 309, 595) | color by name of entity — the color picker panel had no effect and the legend didn't match the canvas |
`baseRadius(n)` (lines 57-58) | `File`/`Module`/`Package` never matched: all nodes with radius 6 |
`ExplorerPage.tsx:529,544` | card of node showed `LABEL: cli_reference.md` and a redundant "Kind" line |

Note: The code blocks, markdown, file paths, and technical terms are unchanged.

Correction:
___INLINE_32__ and ___INLINE_33__ now load the two labels of the graph; ___INLINE_34__ remains the display name. ___INLINE_35__ has been kept because ___INLINE_36__ already depended on it (___INLINE_37__, ___INLINE_38__). — The frontend was internally inconsistent, one half reading ___INLINE_39__ and the other ___INLINE_40__.

### Defeito 2 — `/api/file` ignorava `project_dir`

The inline 43 resolved the search index with
inline 44, which only knows about the project in which the server was started. The rest of the explorer (inline 45, inline 46, inline 47, inline 48) passes through inline 49, which **honors** inline 50.

The result is the exact symptom of the report: when the project selector in the UI points to another project, the graph on the screen comes from project B and clicking "view source" asks for the file from project A — which doesn't exist. 404 on an indexed file.

Corrections:

- The new _INLINE_51__ centralizes the decision "is it another project?" that ___INLINE_52__ used inline; ___INLINE_53__ now uses it without changing behavior.
- The new _INLINE_54__ gives the same response as ___INLINE_55__, just in a different way — the file handler needs the path, not the handle, because the text of the file is stored alongside the index (___INLINE_56__) on the server's root directory, not within the graph.
- Additional hardening: _INLINE_57__ constructs ___INLINE_58__, relative to the process’s CWD. This is only the repo root when the server was started from inside it — ___INLINE_59__ in another directory isn't. When the path is relative, it now resolves against the server's root.

Note: I've assumed that "___INLINE_51__" through "___INLINE_59__" are placeholders for specific inline code or variables that should be replaced with actual values when translating into English.

No fallback for reading from disk was added: `handleFile` continues to serve only what is in the index, which is what prevents a path traversal (`path=../../../etc/passwd`) from becoming arbitrary reading.

## Use Cases

### UC-01: Hide/Show a Node Type in the Graph

- **Actor**: User of the AST Explorer UI Web.
- **Preconditions**: An open context in `/ast/explorer/<contextId>`, graph loaded.
- **Main Flow**:
  1. The "Schema" column header lists labels coming from `GET /api/schema`
     (`SchemaPanel`, `nodes[].label`).
  2. User clicks on the eye icon of a line with a label → `onToggleLabel(label)`.
  3. `ExplorerPage` adds/removes the label in `hiddenLabels` (new `Set`).
  4. `GraphCanvas` recalculates `visibleNodes`/`visibleLinks` and calls `fg.graphData(...)`,
     preserving the positions of nodes that remain visible.
- **Alternative Flows**:
  - The same path applies to "Clusters" and "Languages", which filter by
    `n.properties.cluster` / `n.properties.lang` — these never were broken.
  - `hiddenLabels` is persisted in `localStorage` (`graphit_hidden_labels`) and reapplied
    when reopening the explorer.
- **Error Scenarios**:
  - Hiding all labels leaves the canvas empty; no exception — `graphData` accepts
    empty lists.
  - Edges whose both ends are not visible sum together (filter in `visibleLinks`).
- **Postconditions**: The canvas plots only nodes whose label is not in `hiddenLabels`.
- **Affected Files**: `internal/ast/server.go` (`buildGraphNode`, `extractUserQueryGraph`), `internal/ui/src/components/ast/GraphCanvas.tsx`,
  `internal/ui/src/components/ast/SchemaPanel.tsx`, `internal/ui/src/components/ast/ExplorerPage.tsx`.

### UC-02: Open Source Code of a Node

**Actor**: User of the AST Explorer.
**Preconditions**: Selected node that has **INLINE_88**, or file chosen in the "Tree" tab.
**Main Flow**:
1. Click on "Open Source Code" (or an entry in the tree) → `handleFileClick(path)`.
2. `astApi.getFile(path, contextId, activeProjectDir)` → `GET /api/file?path=…&context=…&project_dir=…`.
3. `handleFile` resolves the store with `storePathForRequest(r)` and reads the text from the index of
   search with `FileSourceAt(store + SearchIndexSuffix, path)`.
4. Right panel opens with content (`CodePanel`).
- **Alternative Flows**:
  - Without `project_dir`, or with `project_dir` equal to the root of the server: reads the store from the own project.
  - With `context` from an imported context: reads the store from that context — global when it's the own project, `<project_dir>/.graphit/ast/<ctx>/ladybugdb` (with resolved symlink) when it's another project.
- **Error Scenarios**:
  - `path` absent → 400 `path param required`.
  - File not indexed in that project → 404 `File source not found`; the UI shows the toast "Failed to load file" and `// Could not load file content.`.
- **Postconditions**: The right panel displays the source code of the project that the request named.
- **Affected Files**:
  - `internal/ast/server.go` (`handleFile`, `storePathForRequest`, `requestedRoot`), `internal/ui/src/api/ast.ts`,
    `internal/ui/src/components/ast/ExplorerPage.tsx`.

Note: The inline codes and placeholders are kept as is.

## Test Cases & Acceptance Criteria

Feature: Label of Node in Graph Payload

Scenario: A named node loads the graph label into `label`
```gherkin
Given an edge of graph with label "Function" and name "handleFile"
When the server sets up the node for the viewer
Then the field "label" is "Function".
And the field "type" is "Function."
And the field "name" is "handleFile".
```

Scenario: A file node without a path falls back to its own name as the path.
```gherkin
Given uma linha de grafo com label "File", nome "server.go" e path vazio
When the server sets up the node for the viewer
Then the field "label" is "File".
And the field "file" is "server.go".
```

Scenario: A node without a name displays its own path
```gherkin
Given uma linha de grafo com label "Directory", nome vazio e path "internal/ast"
When the server sets up the node for the viewer
Then the field "name" is "internal/ast".
And the field "label" is "Directory".
```

Scenario: Users coming from a user's Cypher query follow the same contract
```gherkin
Given um registro de query com Label "Function" e propriedade name "handleFile"
When the user's query graph extractor builds the node
Then the field "label" is "Function."
And the field "name" is "handleFile".
```

Scenario: Hide a label removes those nodes from the canvas
```gherkin
Given the open explorer with 308 plotted points, of which 115 have a label "File"
When the user clicks on the "File" line in the schema panel
The graph then has 193 nodes.
And no plotted node has a label "File"
When the user clicks on the "File" eye again
The graph returns to having 308 nodes.
```

Feature: /api/file honors the project that the request names

Scenario: Serving a File from Another Project
```gherkin
Given um servidor iniciado no projeto A
And project B has "internal/ast/ladybug_gc_pressure_test.go" in its search index.
When chega GET /api/file com path desse arquivo e project_dir do projeto B
The answer is 200.
And the body carries the indexed content in project B.
```

Scenario Outline: Without project_dir, serves the own project
```gherkin
Given um servidor iniciado no projeto A, que tem "cmd/main.go" indexado
When chega GET /api/file com path "cmd/main.go" e "<query extra>"
The answer is 200.
And the body carries the indexed content in project A.

Examples:
  | query extra              |
  | (nenhum)                 |
  | project_dir=<root de A>  |
  | context=__project__      |
  | project_dir= (vazio)     |
```

Scenario: An artifact that is not in the requested project continues to be 404.
```gherkin
Given um servidor iniciado no projeto A, que tem "cmd/main.go" indexado
And project B doesn't have "cmd/main.go" in its index.
When chega GET /api/file com path "cmd/main.go" e project_dir do projeto B
The answer is 404.
```

Scenario: Request without Path
```gherkin
Given um servidor iniciado em qualquer projeto
When it reaches GET /api/file without the path parameter
The answer is 400.
```

Scenario: The relative store is resolved against the server's root.
```gherkin
Given a server with a repoPath of "/tmp/proj" and an backend pointing to the path
      relativo ".graphit/ast/project/ladybugdb"
When the server resolves the store of a request without project_dir
The resolved path is "/tmp/proj/.graphit/ast/project/ladybugdb"
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/server.go` | modified | `buildGraphNode` and `extractUserQueryGraph` now issue the graph label on `label`; `handleFile` now solves the store by `storePathForRequest`; new `requestedRoot` and `storePathForRequest`; `dbForContext` reuses `requestedRoot` |
| `internal/ast/server_file_handler_test.go` | Created | Regression of cross-project 404, default path, legitimate 404 and relative store resolution |
| `internal/ast/server_graph_node_test.go` | Created | Fixes the contract `name` = display / `label` = `type` = graph label, in both constructors |
| `docs/tasks/ast-explorer-hide-labels-and-file-source-404.md` | Created | This record |

The file has been modified. The 404 cross-project, standard path, legitimate 404, and relative store resolution regressions have been fixed. A new label is emitted by the graph's label in the inline section, while the store now uses a new inline section. The inline section reuses an existing inline section.

## Trade-offs & Decisions

- **Correct on the server, not in the frontend.** The alternative was to replace `n.label` with `n.type` at the four points of `GraphCanvas`/`ExplorerPage`. It was discarded: `label` duplicated `name` exactly, there was no consumer for "label = name", and `NodeTree` already read `type` as a label. Correcting on the server makes the two fields consistent with the TypeScript interface (`GraphNode`) and with `/api/schema`, and fixes all four uses in one go without rebuilding the UI.
- **`type` is kept duplicating `label`.** It could have been removed, but `NodeTree.tsx` depends on it, and `writeGraphResponse` uses `n["type"] == "file"` to build the list `files`. Keeping both is the compatible step; unifying is a registered debt below.
- **`storePathForRequest` mirrors `dbForContext` instead of refactoring it.** `dbForContext` has five callers and zero test coverage, and the branch "same project, no context" returns an already opened handle (`s.db`), which does not have a corresponding equivalent path. The shared decision — "is this another project?" — was extracted to `requestedRoot`, exactly where both had diverged.
- **No fallback for reading from disk in `handleFile`.** It would also resolve "file created after the last indexation", but it would open a path traversal on an endpoint that is now safe by construction (only serves what the index has). It remains as an explicit scope debt.

## Technical Debt

- [ ] `GraphNode.type` and `GraphNode.label` now carry the same value. Unifying requires
  touching `NodeTree.tsx` (`n.type === 'File'`, `codeEntityTypes`), `writeGraphResponse`
  (`n["type"] == "file"`) and the interface in `internal/ui/src/api/ast.ts` — rebuild the bundle while it lasts. It never renders the line "Kind" because the condition is `type !== label`.
- [ ] `handleExportBundle` continues to use `internal/ast/server.go` + `storePathFor("")`: always the server project, and would ignore `project_dir` if ever received. The current request body does not have this field, so it is not a bug — it is an undocumented UI limit.
- [ ] The card of the selected node (`ExplorerPage.tsx:529`) uses `labelColor(label)` directly, ignoring the custom color in `nodeColors` that the canvas respects. It became visible only now that `label` is the true label.

Note: Inline codes and Markdown are preserved as they are not translated.

## System Knowledge

- **The text in the files is not on the graph.** It resides at `file_fts.source`, next to the database (`<dbPath>.search.sqlite`), and `FileSourceAt` reads it while being read-only deliberately outside of `OpenSearchIndex` — which runs `migrateSearchSchema` and drops `file_fts` when the schema version differs. Reading a source file can never destroy an index.
- **The SQLite system's SQLite does not open this index:** `sqlite3 … "SELECT … FROM file_fts"` fails with `no such module: fts5`. To inspect, use the tools MCP (`ast_source`) or a Go binary with `-tags fts5`.
- **Each request from the explorer loads `project_dir`**, even when it is itself a project — the `appStore` (zustand, persisted) maintains `activeProjectDir` and all of `astApi.*` passes. Any new handler must decide what to do with it; ignoring it is this task's bug.
- **`DefaultLadybugConfig().DBPath` is relative** (`.graphit/ast/project/ladybugdb`).
  Handlers that derive from it need to anchor at the root of the server.
- **`pkill -f "<binary> ui"` kills its own shell agent**, because the `bash -c` command line used by the agent's standard behavior. Killing by PID (via `pgrep`/`ss -lntp`) is the way.

Note: The inline references are placeholders for actual file paths, database names, and other technical details that should be replaced with specific values when translating to English.

## Progress Log

### 2026-08-11
- Reproduced the 404: two servers, INLINE_189, one in each project. The server from the neighboring project returns 318 graphit-code nodes (honors INLINE_191). The same INLINE_193 returns 404.
- Reproduced the defect in the payload real: `{"label": "copy_test.go", "type": "File"}`.
- Corrected three points; new tests written before validation, and confirmed red against old code (with INLINE_196 restored temporarily) and green against new code.
- Cleaned up INLINE_200 in INLINE_201 and INLINE_202; suite INLINE_203 executed.
- Verified end-to-end in the browser with binary compiled: clicking on "File" hides the plotted graph of 308 nodes → 193 nodes, then reappears back at 308. Clicking an item in the tree renders INLINE_204 → 200 and the right panel displays content. The node card now shows INLINE_205.

Note: Inline codes have been replaced with INLINE_ placeholders for brevity.
