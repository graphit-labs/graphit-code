---
Clicking on a link reveals the code in the entity line, with the line highlighted.
status: done
created: 2026-08-11
updated: 2026-08-11
tags: [ast, server, ui, explorer, cypher, ladybug]
---

Clicking on a node opens the code in the line of the entity, with the line highlighted.

## Objective

In the explorer, clicking on a folder and opening its code would take you to the **top of the file**.
In line 800 of a 1200-line file, the user had to hunt for it.
entidade que acabou de clicar.

The functionality did not exist in any of the three layers— it was not a bug of
Regression

| camada | o que faltava |
|---|---|
| query | nenhuma das duas amostras projetava `line_number` |
The API/Type field "`GraphNode`" does not have a line item field.
Panel received only `content` and `filename`; `highlightedLines` was a syntactic highlight, not a line break.

## Implementation Details

### Servidor

The `graphSideColumns` has been added to exist for the projection of one side of the line.
(`src_`/`dst_`), o que fez a coluna nova ser acrescentada em UM lugar em vez de
Four. The two samples use it, and `graphNodeSideFrom` reads both sides with the same.
code

`buildGraphNode` deixou de receber seis strings posicionais e passou a receber
Direct Motivation: The seventh positional parameter—line—is likely to remain unchanged.
Next to `cluster` and `lang`, changing two of them would not cause a compilation error.

The line exits from two places around the node, as intended:

- Where the explorer reads, to jump;
- Where the detail panel reads raw properties.

Line 0 is not sent. It's the placeholder value of the destination call stub.
(`is_stub = true`), which lacks a declaration to open. Omitting is what leaves "there is no"
Clear distinction between "jump line" and "line 1".

### A armadilha do binder — e por que existe `querySample`

The `INLINE_0` does not have a label, and in LadybugDB, a property "links to an attribute that is labeled with **ALGUNO** label of the graph.
Here's the translation:

There is no table with any information about a graph that only has files and directories.
`Binder exception: Cannot find property line_number for n`, que **derruba a query
inteira** — ou seja, um 500 novo no explorer, exatamente o sintoma que as duas
The previous corrections for this endpoint were removed.

Discovered by the existing test that indexes a single INLINE_0 and whose tables
They are Files, Directories, Fields, Parameters, and CONTAINS - none with `line_number`.

`querySample` roda a variante com a coluna e repete sem ela **apenas** se o erro
mention this property; any other error escalates to the caller. In this graph,
There is an entity that can jump in any direction, so nothing gets lost.

### Front-end

- `GraphNode.line?: number`.
- `handleFileClick(path, line?)` guarda `sourceLine`; `handleNodeClick` passa
The button "Open Source Code" is identical.
- `CodePanel` recebe `highlightLine`, marca a linha com `.target-line` (fundo em
The text is already in English, so it remains unchanged:

  `--primary` the 14% and a 3px left border, but with the highlighted number) and calls
  `scrollIntoView({ block: 'center' })`.

The effect depends on **INLINE 0**, and both dependencies are
necessary: The panel opens **before** the file arrives, so on the first rendering
There is a line, but there is no content; and clicking on another entity of the same file
Changes only the line.

### Custo medido

| query | hoje | com a linha |
|---|---|---|
Sample of us | 0.0052-0.0059 seconds | 0.0051-0.0061 seconds
| amostra de arestas (linha nos dois lados) | 0,131–0,158 s | 0,141–0,153 s |

## Use Cases

UC-01: Click on an entity in the graph to fall into its declaration
Actor: user of the explorer, clicking on a node in the canvas.
Preconditions:

The node represents an entity with a declaration (not a File, a...
The directory is not just an entry point; it's indexed.
- **Main Flow**:
  1. `handleNodeClick` recebe o `GraphNode`, que carrega `line`.
  2. Chama `handleFileClick(node.file, node.line)`.
The panel opens, and INLINE_0 is recorded, and the content is searched in INLINE_1.
When the content arrives, **INLINE_0** scrolls the line to the center and marks it.
- **Alternative Flows**:
  - Clique num File ou Directory: `line` ausente, o arquivo abre no topo, nada marcado.
Button "Open Source Code" on the details panel: same path.
Second entity in the same file: only the line changes; the panel repositions itself.
- **Error Scenarios**:
Line beyond the end of the file (outdated index): nothing marked, nothing rolled, it
    arquivo abre normalmente.
  - Grafo sem nenhuma tabela com `line_number`: `querySample` repete sem a coluna e o
    explorer desenha igual.
  - `/api/file` falha: comportamento anterior, toast de erro.
Postconditions: the panel displays the file positioned in the declaration, with the line
  destacada.
- **Affected Files**: `internal/ast/server.go`, `internal/ui/src/api/ast.ts`,
  `internal/ui/src/components/ast/ExplorerPage.tsx`,
  `internal/ui/src/components/ast/CodePanel.tsx`.

UC-02: Run a Custom Cypher and Click on a Node in the Result.
Actor: user, through query bar.
Preconditions: The query returns nodes (`RETURN n`).
- **Main Flow**: `extractUserQueryGraph` levanta `line_number` das propriedades cruas
For that, starting from there is UC-01.
Error Scenarios: Queries that return only scalar columns do not produce nodes.
  resultado sai como tabela — inalterado.
Postconditions: The jump works as expected with the standard vision.
- **Affected Files**: `internal/ast/server.go`.

## Test Cases & Acceptance Criteria

### Feature: a linha chega ao explorer
Ref: UC-01, UC-02

Scenario: Entity Loads Its Declaration Line
```gherkin
Given an `Function` declared on line 441
When the server sets up the graph node
The knot has a length of 441.
And their properties have line number equal to 441.
```

Scenario Outline: We do not load a line when there is no declaration
```gherkin
Given a type `<type>` without declaration line
When the server sets up the graph node
Then it doesn't have the field line

Examples:
  | tipo                        |
  | stub de destino de chamada  |
  | File                        |
```

#### Scenario: os dois lados de uma aresta preservam sua linha
```gherkin
Given uma linha da amostra de arestas com um File na origem e uma Function na linha 441 no destino
When o extrator processa a linha
The root's origin doesn't have lines
The destination node has a line length of 441.
```

The scenario where data is fetched from user queries also includes loading the line.
```gherkin
Given a query that returns a node with the property line number equal to 441
When the user's query extractor processes the record
The knot has a length of 441.
```

Feature: The samples run even when the property does not exist
Ref: UC-01

Scenario: Graph with only files and directories
```gherkin
Given an indexed single-file project with tables without line numbers
When samples of nodes and edges are executed through the handler's path
Then nenhuma das duas devolve erro
And at least one line is returned by us.
```

### Feature: o painel salta e destaca
Ref: UC-01

Scenario: The requested line is marked and centralized.
```gherkin
Given um arquivo de cinco linhas aberto com a linha 3 pedida
When o painel renderiza
Then a linha 3 tem a classe target-line
And it is the only marked one.
And it rolled into view.
```

#### Scenario: sem linha, o arquivo abre no topo
```gherkin
Given um arquivo aberto sem linha pedida
When o painel renderiza
Then no line is marked.
And nothing is rolled up.
```

The content arrives after the line.
```gherkin
Given the open panel with line 5 requested and an empty content
When the content of the file arrives
The line 5 is marked.
And it's all about vision.
```

#### Scenario: outra entidade no mesmo arquivo
```gherkin
Given o painel posicionado na linha 3 de um arquivo
When apenas a linha pedida muda para 5
The fifth line is rolled up for vision.
  And segue existindo exatamente uma linha marcada
```

Scenario: Beyond the End of the File
```gherkin
Given um arquivo de cinco linhas aberto com a linha 999 pedida
When o painel renderiza
Then no line is marked
And nothing is left undone.
And it is displayed normally.
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| | Modified | `internal/ast/server.go`/`graphSideColumns` builds the projection of one side; `graphNodeSide`/`graphNodeSideFrom` replaces the six positional parameters; `line_number` projected onto both samples; `querySample` falls to the variant without the column when the graph does not have it; `extractUserQueryGraph` raises the line of properties |
Modified | Tests on both sides of the line, from the stub's zero and the user's query path.
Modified | Graph test passes as `querySample`, which is the path used by the handler.
| `internal/ui/src/api/ast.ts` | Modificado | `GraphNode.line` |
Modified | Click on the node, click on the file, and click Open
| `internal/ui/src/components/ast/CodePanel.tsx` | Modificado | `highlightLine`: marca a linha e a centraliza |
Created | Six Scenarios, Including Late Content Arrival

## Trade-offs & Decisions

- **`querySample` em vez de consultar o schema antes.** Perguntar ao banco quais
Tables would cost an inline query every request for a case
Degenerate. The attempt-and-repeat strategy costs nothing along the normal path and self-corrects.
As soon as the graph gains entities. The price is a string comparison in the message.
Despite error, restricted to the exact property for not swallowing other binder errors.
Instead of an additional seventh parameter. Add `line int` at the end.
  de seis strings funcionaria e compilaria; trocar `cluster` com `lang` numa chamada
The future would also compile it. The struct costed about 20 lines and a signature change
  coberta por teste.
Instantaneous jump, not smooth. `scrollIntoView` without `behavior: 'smooth'`: this is
navigation for a declaration, not a push - in a file of 20 thousand lines
The smooth animation would be a journey.
The highlight remains, it does not fade away. The user requested the highlighted line; a distinction.
  que some em 2s deixa de responder "qual era mesmo a linha?" trinta segundos depois.

## Technical Debt

The jump only occurs when the node comes from a sample or a query that returns
We need to carry `line` as well, no matter how it arrives from another future path.
There is nothing in this type that could possibly force it.
- [ ] Marks a line; entities occupy an interval (_`line_number` until
Highlighting the entire block would be more informative, and the graph already has
  `end_line` — falta projetar e desenhar.
- [ ] If the file on disk has changed since indexing, the line points to it.
The place is wrong without any warning. There's no freshness check between the graph and the text.
  servido por `/api/file`.

## System Knowledge

- **No LadybugDB uma propriedade liga se ALGUM label do grafo a tiver.** Em `MATCH (n)`
without labels, INLINE_0 works on a graph with functions and **completely breaks the query**
In an empty graph, it is INLINE_0 — and the message
Lists the existing tables, which is like saying "property does not exist" of
"reconstructed graph"
**`line_number = 0` is a placeholder for stub, not localization. Every consumer that...**
  trate 0 como linha vai saltar para o topo achando que acertou.
The explorer's side tree is an archive tree. Entities are only

Translation:

"The explorer's secondary tree is a file system tree. Entities are merely"
Clickable on the graph's canvas - this is also the reason for verifying this
The change has moved to a component test instead of browser automation.
Measured in this project: of the 1,235 drawings made, 1,140 have lines; the 95 without are 94.
Directory and a stub for a Table. In the large corpus, 286 of 1268 -- the rest are Files (936)
  e Directory (46).

## Progress Log

### 2026-08-11

Confirmed that the functionality did not exist in any of the three layers, instead
Assume regression.
Cost before implementation measured on both sides was negligible.
- Implementado servidor → tipo → painel; `buildGraphNode` refatorado para struct no
  caminho.
The minimum graph test caught the _`Binder exception`_ — a 500 new that would have gone to
production. Therefore, INLINE_0__.
Verified end-to-end in the UI real: The response brings only for those who have
declaration. The graph's canvas is not clickable by automation, so the behavior of
The panel was fixed for component test (6 scenarios).
- `go test -tags fts5 ./internal/ast/` e `npx vitest run` (34 testes) verdes; `tsc` e
  `eslint` limpos nos arquivos alterados.
