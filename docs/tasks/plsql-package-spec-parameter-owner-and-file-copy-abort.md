Task: The parameter of the package specification belongs to the subroutine, and an incorrect copy does not publish any incomplete graph.

**Date:** 2026-08-03 → 2026-08-04
**Status:** ✅ Complete

## Problema

Reportado a partir de uma pergunta concreta sobre a massa de teste do projeto
um corpus Oracle privado de 36.823 arquivos: *por que o
Parameter `P_LOG_TX` is linked to the package `PCK_EXEMPLO` and not to the procedure
`ATLZ_EXEMPLO`?*

The investigation found two independent bugs, as well as two gaps of the same family in

This is already English and idiomatic. No changes were needed.
Other grammars.

### 1. `procedure_spec` / `function_spec` fora de `context_types` (PL/SQL)

Brazilian Portuguese:
The `internal/ast/queries/plsql.yaml` declares Procedures and Functions from three forms each:

| forma | onde aparece | estava em `context_types`? |
|---|---|---|
| `create_procedure_body` | `CREATE PROCEDURE` avulso | sim |
| `procedure_body` | corpo dentro de `CREATE PACKAGE BODY` | sim |
The Portuguese text is already in English, so it remains unchanged:

| `procedure_spec` | **declaration within `CREATE PACKAGE`** (spec) | **not** |

`resolveParentContextAntlr` (`internal/ast/antlr_adapter.go:598`) sobe a cadeia
`match.Context.Outer` e devolve o **primeiro** ancestral cujo rule esteja em `context_types`.
In a package specification, the chain of parameters is set to `parameter → procedure_spec → create_package`.
as if it ignored him
`create_package`.

Medido no grafo do corpus antes do fix — Parameter por `context_type`:

Owner Parameters
|---|---|
| Procedure | 13.542 |
| Function | 10.558 |
| **Package** | **9.052** |

And 100% of the 9, 052 cases are within bounds (none outside), which closes up on it.
Diagnosis: It is precisely the entire set of specifications. Beyond assigning the wrong owner, this **collides
UIDs: Two subprograms within the same package that share a parameter name (_`E_TX_ERRO`)
The typical case in this corpus produced INLINE 0.

Issue: `ConvertToCache` discards parameter and field with empty context - this is data loss.
967 de 2.732 entidades documentada em `internal/ast/oracle_parameter_loss_test.go`. Um
It is not merely an error that a non-callable function does not use its context, but rather it can also leak the entire parameter.

### 2. COPY falho era logado e ignorado, e o rebuild trocava o banco de todo jeito

No mesmo projeto, `MATCH (f:File) RETURN count(f)` devolvia **0**, com `Directory` = 17 e
todas as entidades presentes. Sintomas:

- `graphit ast source` / `graphit_ast_source` falhava com `source not found for path` para
  **qualquer** caminho do projeto (verificado em `schema/packages/PCK_EXEMPLO.sql` e em
The code snippet is not provided in this context, so I cannot translate it directly. However, if you provide the specific code or a brief description of what it does, I can help you understand and then translate it into idiomatic English for you. Please share more details about the code or its purpose.
The code snippet returned no lines in silence;
- as arestas `CONTAINS File→entidade` e `Directory→File` (`internal/ast/json_rebuild.go`)
simply did not exist.

Causa: em `RebuildFromJSON`, o helper `copyNode` apenas logava o erro e incrementava
`copyErrors`; nada checava `copyErrors` antes de `lb.AtomicSwapDB(tempDBPath)`. O rebuild
seguia, rodava enrichment e **publicava** o banco meio-carregado. O log do erro vai para o
Slogan for rebuilding, not for **INLINE_0** — confirmed: no line of
COPY/error nos logs do daemon global nem do projeto.

Why does the COPY of File explode: `ri.fileNodeJSON` builds a **slice** in memory with
"Inline 0" of all files, it writes into a single JSON and loads with a single request.
Here is the translation:

In this corpus there are 36,823 files / 2.4 GB of shards,

This translation maintains the structure and meaning of the original Portuguese sentence while rendering it in idiomatic English.
Individual shards up to 133 MB (a single XML).

The source was not lost during extraction: the shard.
`schema/packages/PCK_EXEMPLO.sql.nodes.json` tem as chaves
The complete _INLINE_0_ is available, but there's only a loss in weight.

Note: The path **incrementally** already addressed this correctly —
`internal/ast/incremental_rebuild.go` cai para rebuild completo quando `insertErrors > 0`, com
comment explaining exactly this class of bug. The full rebuild was what was missing
fechar.

### 3. `html.yaml` declarava contextos e nenhum `context_name_paths`

Continuation directly from [Entities know their father](../changelogs/20260802_entidades_conhecem_seu_pai_e_a_subida_e_memoizada.md)
que adicionou `context_name_paths` a `xml`, `json`, `yaml_lang` e `svelte`. `html.yaml` ficou
de fora: declara `element`, `script_element` e `style_element` como contextos, mas
The field of `name` is not exposed by `tree-sitter-html`; the name goes in `start_tag`. Without the

This translation maintains the meaning and structure of the original Portuguese text while providing an idiomatic English equivalent.
path, `nameNodeOf` devolve nil, o container fica **transparente** e toda entidade HTML volta a
Fall into Place, which is precisely the silent failure that that work was meant to kill.

Other Grammars: Callable Functions That Were Not Contextual

Audit of all 44 grammars crossing "rule that the query transforms into callable" against
Real gaps found beyond PL/SQL:

Grammar rule effect
|---|---|---|
Here is the idiomatic English translation:

"By default, Dart declares the signature as its own type; all method parameters go to the class."
Here is the translation:

"____ | `objc` , `function_definition` , `method_declaration` , `method_definition` | same as parameters went to the class/protocol"
Parameters of constructor were moved to the class.
The first thing you need to do is make sure that your computer meets the minimum system requirements for running Windows 7. This includes having a compatible processor and sufficient RAM. Additionally, ensure that your hard drive has enough space for installation.

Translation:

| Make sure your computer meets the minimum system requirements for running Windows 7 | Ensure compatibility with your processor | Ensure sufficient RAM | Hard drive should have enough space for installation |

Solution

The rules of grammar

`internal/ast/queries/plsql.yaml` — adicionados `function_spec: Function`,
Here is the translation:

"`procedure_spec: Procedure`, `create_materialized_view: MaterializedView` and the family of"

This appears to be a placeholder or template for some kind of code, possibly in C++ or another language that uses inline functions. The placeholders "`procedure_spec: Procedure`" and "`create_materialized_view: MaterializedView`" are likely variables or parameters used within this context. The "family of" suggests it could be related to inheritance or class hierarchies in object-oriented programming.

Without more context, I've provided a general translation that maintains the structure while making minimal changes for clarity.
statements of type level package (`record_type_definition`, `table_type_definition`)
`varray_type_definition`, `ref_cursor_type_definition`, `subtype_declaration` → `Type`), com
comment explaining the invariant in the file: *any rule that transforms a query into
container tem de estar aqui*.

Mais `java.yaml` (`constructor_declaration`), `javascript.yaml` / `typescript.yaml` /
`tsx.yaml` (`generator_function_declaration`), `dart.yaml` (`function_signature`,
`method_signature`), `objc.yaml` (3 regras), `svelte.yaml` (`script_element`, `style_element`,
for parity with `html.yaml`, and `html.yaml` (`context_name_paths` for all three contexts).

Rebuild does not publish an incomplete graph

`internal/ast/json_rebuild.go`:

The database is now **aborted**; it has closed its temporary backend and returned an error.
The previous one stays in place, and the temporary is removed by defer (because `swapped` continues)
Keeping the old bank is strictly better than publishing an incomplete one.
- `copyNode` passou a iterar `batchRows(data, copyBatchBytes)` — 64 MiB por COPY. Novas
functions INLINE_0 and INLINE_1 measure **bytes**, not lines: the sizes here
They vary by six orders of magnitude, so counting lines does not express the limit. One
The larger line exceeds its budget - the goal is to limit the document, not expand it.
Reject content.

The `internal/ast/incremental_rebuild.go` is batched according to the same criteria, ensuring uniformity in processing.
A large file does not necessitate an unnecessary full rebuild.

The text from the files comes out of the graph: the search index is the only owner.

The source was stored **three times** using different code paths:

Copy where written by
|---|---|---|
| shard do parse cache | `<dir do DB>/shards/<path>.nodes.json`, campo `src` | pipeline de parse |
| propriedade do grafo | `File.source` dentro de `ladybugdb` | `COPY File` em `RebuildFromJSON` |
Column of search index | `file_fts.source` in `ladybugdb.search.sqlite` | `fts_sqlite.go`

The ``file_fts`` declares ``source` as an indexed column (``fts_sqlite.go:95``, unlike ``name``)
that is `UNINDEXED` with BM25 weight of 1.0 in `bm25(2.0, 0.0, 8.0, 1.0)`, and the query FTS "file_fts,"
In the table level — so the source is actually searchable, and FTS5 stores it.
Original text recoverable by `SELECT`. Verified in the real corpus: 36,823 lines in
`file_fts`, e `schema/packages/PCK_EXEMPLO.sql` com 12.973 bytes de source — no **mesmo**
Bank whose graph has no nodes.

Of the three, only the search index is accessible - the other two were dead weight that still
She was the most expensive, and that's why she forced the entire repository to be passed through.
For one `COPY`, and it was her who failed. Then **the text falls off the page**:

- INLINE 0 is no longer emitting INLINE 2.
- `internal/ast/json_rebuild.go` — `fileCols` perde a coluna, e com ela desaparece o `COPY` de
  2,4 GB que causou o incidente. O caminho incremental se ajusta sozinho: `insertNodes` deriva
  as chaves de `data[0]`.
- Column `internal/ast/ladybug.go` survives in the DDL of table File, and only because
A reason: the synthetic `'__config__'`, where `RunEnrichment` holds the configuration
  detectada do projeto (`enrichment.go:413`) e que a skill documenta consultar
  (`MATCH (c:File {path: '__config__'}) RETURN c.source AS configs`). Para arquivo real, ela
  fica vazia.
Brazilian Portuguese:
- `INLINE_0` does not consult the graph for text anymore:

Idiomatic English:
- `INLINE_0` no longer queries the graph for text.
Inline 0 and a single Inline 1.
Here is the translation:

- `internal/ast/server.go` - the HTTP content file endpoint identical via
  `storePathFor(context)`.
- `internal/ast/rule.go` — a tabela de propriedades da skill perde `source` de **File**, e a
Section "Quick Source Peek" — which used to say `RETURN file.source` — now says that text is not
Property of the graph: The query provides location information, while Tool INLINE_0 gives text.

New function `FileSourceAt` in `internal/ast/fts_sqlite.go`: a single one
`SELECT source FROM file_fts WHERE path = ?`, abrindo sqlite com `mode=ro`. **Deliberadamente
He does not use `OpenSearchIndex`. This calls `migrateSearchSchema`, which does.
When the schema version differs (`fts_sqlite.go:74`), read one.
Brazilian Portuguese:
"Never can it destroy the index where he reads."

Idiomatic English:
"It's impossible for it to corrupt the place where he reads."

Contexts imported can now be researched.

With the text confined solely to its index, an index-less context is not degraded— it's...
Useless. And that was exactly the case with the Install of the Hub: `internal/hub/service.go`
(`case TypeAST`) chamava `RebuildFromJSON` e parava ali, sem nunca abrir um `SearchIndex`. O
The context was navigable by Cipher and neither searchable nor readable, and the shards he came from.
Built remain at the clone of the Hub, not next to the store — nothing else could serve it.
texto.

New function `ast.BuildSearchIndexFor(dbPath, cache, embCache)` encapsulates
`OpenSearchIndex` + `RebuildFromCache` + `BuildEmbLookup`, e o install do Hub agora a chama,
Failing the installation if it fails. The three contexts remain equivalent:

Origin | Graph | Search Index
|---|---|---|
project own | pipeline | pipeline (INLINE_0)
| `ast_install` de path local | pipeline com `CacheDir: filepath.Dir(ictx.DBPath)` | pipeline |
| artefato do Hub | `RebuildFromJSON` | **`BuildSearchIndexFor`** (novo) |

The resolution by context has been working on both sides without needing to change it: INLINE_0
deriva `lb.cfg.DBPath + searchIndexSuffix` (`query.go:36-38`) e `openASTDB(projectDir, context)`
It returns the backend of the context. Only missing was the file existing.

It stops semantic search from degrading.

Different flag than before, and the problem is with polarity, not surface. With
False — or `--no-source` / `no_source` in the index — the text is not
persistido em lugar nenhum: `antlr_adapter.go:272` e `treesitter_adapter.go:309` nem
They materialize `result.Source`, and `ConvertToCache` writes an empty `src` to the shard. No FTS
About the source being accepted as a consequence: `entity_fts` continues indexing name, `name_split`
And then, with docstrings, it should work by following the name.

What was unacceptable was what the embedding lost. `scanPending` assembled the snippet with
INLINE 0 — the source persisted — that under the flag is empty.
O texto embeddado ficava reduzido a label, path, contexto, nome e docstring: exatamente sem
a parte que descreve o que uma entidade **faz** em vez de como ela se chama. A flag
It degrades semantic search in silence.

The distinction that resolves: the flag says "do not save a copy of the source," not "*do not look at the"
Embedding is a vector, not a recoverable text — it can be calculated from the source.
file and persisted as long as the text never is.

Index tree, indexed only when parsing cache is empty
  texto para aquele arquivo. Vazio desliga a leitura e o embedding cai para nome, docstring
  e contexto, em vez de falhar.
- `Embedder.sourceFromDisk(relPath, shardHash)` — **SAFETY**: exige que
The file aligns with the hash of the shard, and the embedding cache is keyed.
In this hash, embedding text from the new version under the old key would store a vector.
Describing code that the graph does not contain, and it would survive until the file changes again.
Divergence ⇒ It's better than the wrong snippet.
Idle resolution by shard: a cache that **never** touches the disk, so it doesn't need to.
Reading is not a new cost on the standard path; at most one reading.
  por shard, na mesma forma de streaming da varredura.
Brazilian Portuguese:
- The inline 0 won the parameter inline 1, propagated by the embedding module of

Idiomatic English translation:
The inline 0 triumphed in its own parameters, which were passed along from the embedding module.
Daemon, which already had `rootPath`. Also connected to `RepoRoot` on the `ast embed` path.
Sure, here is the translation:

"Sync and in the tool MCP — this last one resolves INLINE_0__ for"

This translation maintains the original meaning while making it more idiomatic English.
Context imported, leaving blank for hub artifact, which does not have a local tree.

### `--no-sources` passa a existir de fato

The flag was declared on two surfaces — INLINE 0 in CLI (INLINE 1) and
The ``no_sources`` feature in the tool MCP was accepted and discarded: `runASTExport` received the parameter.
He never read it, and the success message said "with sources" no matter how they phrased it. Also was
Infeasible as it was written, because it described omitting a property of a node where the text
He no longer lives there.

With the text coming from the index, including is an action of reality, while excluding is a choice of reality:

- `internal/ast/fts_sqlite.go` — `EachFileSource(indexPath, fn)` percorre `file_fts` linha a
Line. **Streaming, not collection:** The entire corpus is on the other side of the cursor (2.4 GB in size)
  caso de 36k arquivos) e quem escreve num zip precisa de um arquivo por vez. Linhas com
The empty INLINE_0 does not enter.
- `internal/ast/bundle.go` — `ExportBundle` ganha `BundleOptions{StorePath, NoSources}`, e
  escreve cada arquivo como um membro `sources/<path>` do zip, em vez de um mapa gigante: o
The extracted bundle turns into an arbor, and reading an archive doesn't require parsing all of them.
It would escape from the root of the file is skipped. The manifest gains `source_count` to distinguish bundle
structural of truncated bundle without guessing.
Connected at all three call sites: CLI, tool MCP, and INLINE 0 (which won)
  `no_sources` no corpo).

Requesting sources and not being able to is an error, not exporting structurally silent— a bundle that
No one can use it, so it seems like failure. Without `StorePath`, the bundle is structural by default.
definition and what it says.

The error messages now distinguish between the two cases, which is diagnostic useful:

| mensagem | significado |
|---|---|
There is no file node — index incomplete.
The file exists without a source configuration - inline 0.

Ligado em `internal/mcpstdio/tools_ast.go` e `cmd/graphit/commands/runners.go` via
`WithShardCache(filepath.Dir(cfg.DBPath))`.

## Use Cases

### UC-01: Indexar um package spec PL/SQL e navegar a assinatura de um subprograma
- **Actor**: agente ou desenvolvedor consultando o grafo.
- **Preconditions**: projeto indexado com `ast.grammar .sql=antlr-plsql`; arquivo contendo
  `CREATE PACKAGE ... AS` com `PROCEDURE`/`FUNCTION` declarados no spec.
- **Main Flow**:
  1. O indexador casa `//procedure_spec/identifier` e cria a entidade Procedure.
2. For each parameter in the signature, use `//parameter/parameter_name`.
  3. `resolveParentContextAntlr` sobe de `parameter` e encontra `procedure_spec`, agora em
     `context_types`, devolvendo `(nome do subprograma, "Procedure")`.
The entity Parameter is stored with `context` = subroutine, `context_type` = `Procedure`,
     e o uid fica `path::PACKAGE.SUBPROGRAMA.PARAM`.
  5. `MATCH (p:Procedure {name:'X'})-[:CONTAINS]->(a:Parameter)` devolve a assinatura.
- **Alternative Flows**:
In **INLINE_0**, the context found is **INLINE_1**/**INLINE_2**.
    comportamento inalterado.
In `CREATE PROCEDURE`, it is `create_procedure_body`.
The declared types in the package (`TYPE ... IS RECORD`) now refer to the own fields' contexts.
Instead of leaving them in the package.
- **Error Scenarios**:
Rule of absent container with `context_types` → parameter assigned to wrong ancestor or
    descartado por `ConvertToCache`. Barrado em teste por
    `TestEveryCallableContainerIsDeclaredAsAContext`.
- **Postconditions**: nenhum Parameter de spec tem `context_type = "Package"`; uids de
Homologated Parameters between Different Subprocedures Do Not Collide.
- **Affected Files**: `internal/ast/queries/plsql.yaml`, `internal/ast/antlr_adapter.go`.

Complete rebuild with non-publication of the graph due to COPY failure
- **Actor**: `graphit ast index`, ou o daemon via sync.
Preconditions: populated parsing cache; existing or non-existent previous database.
- **Main Flow**:
Brazilian Portuguese:
1. INLINE_0 creates the temporary database and initializes the schema.
The second inline divides each table into batches of up to 64 MiB and executes a COPY per batch.
  3. Nenhum erro → enrichment → `AtomicSwapDB` → `swapped = true`.
- **Alternative Flows**:
A small table → a single batch, with zero cost compared to previous behavior.
Greater than the budget → own batch, nothing is discarded.
- **Error Scenarios**:
Brazilian Portuguese to idiomatic English:

- Any COPY fails → **INLINE** 0 → backend temporarily closed, error returned, database
Previous preserved, temporary removed by delete.
  - No caminho incremental, `insertErrors > 0` continua caindo para rebuild completo — que
    agora falha explicitamente em vez de publicar incompleto.
Postconditions: The bank is published or complete, or it has not been published.
- **Affected Files**: `internal/ast/json_rebuild.go`, `internal/ast/incremental_rebuild.go`.

### UC-03: Ler o texto de um arquivo indexado
Actor: agent via INLINE_0, user via INLINE_1, or endpoint
HTTP File Content Transfer
Preconditions: active the `ast.index_source` indexed file; search index in
  `<DBPath>.search.sqlite`.
- **Main Flow**:
  1. O call site resolve o store — do projeto ou do contexto — e chama `WithStore(dbPath)`.
The function INLINE_0 calls INLINE_1, which opens the index in INLINE_2 and executes.
     `SELECT source FROM file_fts WHERE path = ?`.
The text is returned and cut as `head`, `tail`, `entity`, and `pattern`.
- **Alternative Flows**:
- The **location** is sourced from the graph (`line_number`/`end_line`), and the text is provided by `entity`.
index; responsibilities are separated by construction.
  - Contexto importado → mesmo caminho, com o `DBPath` daquele contexto.
- **Error Scenarios**:
Service without storage → saying it's impossible to reach the index, not that the file
There is none.
Absent path in the index → error citing both possible causes: not indexed or
    `ast.index_source` falso.
Deflated index schema version → Reading works, but it does not migrate or drop.
    `file_fts`.
Postconditions: The text originates from a single store; the index remains intact after reading;
The graph is not consulted for text.
- **Affected Files**: `internal/ast/source_service.go`, `internal/ast/fts_sqlite.go`,
  `internal/ast/server.go`, `internal/mcpstdio/tools_ast.go`, `cmd/graphit/commands/runners.go`.

UC-04: Install an Abstract Syntax Tree (AST) Context and Explore It
Actor: user or agent through INLINE_0 of artifact INLINE_1.
- **Preconditions**: artefato no Hub carregando shards do parse cache.
- **Main Flow**:
  1. `internal/hub/service.go` (`case TypeAST`) carrega o shard cache do clone.
  2. `CreateGraphSchema` + `RebuildFromJSON` constroem o grafo em
     `~/.graphit/ast/<project-id>/ladybugdb`.
The third line constructs
     `ladybugdb.search.sqlite` a partir dos mesmos shards.
  4. `ast_search` e `ast_source` com `context: "<nome>"` resolvem aquele store e respondem.
- **Alternative Flows**:
Shard cache empty (_`Count() == 0`) → nothing is built as before.
- **Error Scenarios**:
Error building index → the installation **fails** with an error, instead of leaving a context
Navigable but unsearchable.
Postconditions: all three contexts of origin (project, `ast_install` local, hub) have a graph.
Index of search.
- **Affected Files**: `internal/hub/service.go`, `internal/ast/fts_sqlite.go`.

### UC-05: Exportar um bundle com ou sem o texto dos arquivos
Actor: User via `graphit ast export --format bundle`, Agent via the MCP Export Tool
  ou `POST /api/export/bundle`.
Preconditions: indexed project; search index present when seeking text.
- **Main Flow**:
  1. O call site monta `BundleOptions{StorePath, NoSources}`.
The second part collects vertices and edges of the graph.
With **INLINE_0** set as false and **INLINE_1** filled in, **INLINE_2** traverses the index via
     `EachFileSource` e escreve um membro `sources/<path>` por arquivo, em streaming.
  4. O manifest registra `node_count`, `edge_count` e `source_count`.
- **Alternative Flows**:
- Inline 0 and Inline 1 are skipped in step 3; Inline 2 equals 0.
- Without `StorePath` → by definition, structural bundle.
- **Error Scenarios**:
Error, named after the source step missing or illegible.
    reportado como exportado com sucesso.
Path of the index that would leap free from the root of the zip → jumped, not written.
Postconditions: The content of the bundle matches what the flag requested, and the manifest is correct.
  descreve o que ele carrega.
- **Affected Files**: `internal/ast/bundle.go`, `internal/ast/fts_sqlite.go`,
  `internal/ast/server.go`, `internal/mcpstdio/tools_ast.go`, `cmd/graphit/commands/runners.go`.

UC-06: Index without persisting the source and still have semantic search
Actor: user with `ast.index_source: false` in the configuration or `--no-source` in the index.
Preconditions: an indexed tree present on disk and INLINE_0 configured in the embedder.
- **Main Flow**:
The parse does not materialize `result.Source`, and `ConvertToCache` writes an empty `src` to the shard.
  2. `scanPending` encontra `entry.Source` vazio e chama `sourceFromDisk(relPath, hash)`.
The hash of the file is compared with that of the shard; when they match, the file is read.
The snippet is cut by the line interval of the entity and limited by
The vector is calculated and persisted in the embedding cache.
- **Alternative Flows**:
  - Shard **com** texto → usa o texto do cache, sem tocar o disco.
  - `RepoRoot` vazio (artefato do Hub, por exemplo) → sem snippet; a entidade continua
Embedded by name, docstring, and context.
The FTS about the source does not exist under this flag by definition; `entity_fts` continues
    indexando nome, `name_split` e docstring.
- **Error Scenarios**:
The file's hash does not match the shard's hash → without snippet, to avoid storing a snippet in the cache.
vector indicating that the graph does not contain.
The file is illegible or absent → no snippet, no error: the shard will be reprocessed.
- **Postconditions**: existe vetor para a entidade incluindo o sinal do corpo, e nenhum
Artifact persists contains text.
- **Affected Files**: `internal/ast/embedder.go`, `internal/daemon/adapters.go`,
  `internal/mcpstdio/tools_ast.go`, `cmd/graphit/commands/ast.go`,
  `cmd/graphit/commands/lifecycle.go`.

## Test Cases & Acceptance Criteria

Feature: Owner of the parameter in package specification
Ref: UC-01

The parameter declared in the procedure specification belongs to the procedure.
```gherkin
Given um arquivo com "CREATE PACKAGE PCK_COBRANCA AS" declarando a procedure "ATUALIZAR_INTEGRACAO"
And that procedure declares the parameter "P_LOG_TX"
When the file is parsed with ANTLR-PLSQL grammar
Then a entidade Parameter "P_LOG_TX" tem context "ATUALIZAR_INTEGRACAO"
  And tem context_type "Procedure"
But there's no context "PCK_COBRANCA"
```

The parameter declared in the specification belongs to the function.
```gherkin
Given an invoice package specification that declares the function "BALANCE_DEBITOR" with the parameter "P_ID_CONTRACT"
When the file is parsed
Then o Parameter "P_ID_CONTRATO" tem context "SALDO_DEVEDOR" e context_type "Function"
```

#### Scenario: package body continua atribuindo corretamente
```gherkin
Given a "CREATE PACKAGE BODY" with the procedure "UPDATE_INTEGRATION" and parameter "P_LOG_TX"
When the file is parsed
Then o Parameter "P_LOG_TX" tem context "ATUALIZAR_INTEGRACAO" e context_type "Procedure"
```

Scenario: Parameters survive conversion to cache
```gherkin
Given a package specification declaring the procedure "P" with parameters "P_A" and "P_B"
When o resultado do parse passa por ConvertToCache
Then, since entities "P_A" and "P_B" exist in the cache
```

Feature: Grammar Protection Network
Ref: UC-01

Scenario: A non-context-dependent rule-declared callable fails the build
```gherkin
Given a grammar whose query generates Functions, Methods, Procedures, or Constructors from a rule
And that rule isn't in context_types.
And language is not in flat Languages.
And the rule isn't in callableContainerExemptions
And the pattern does not mention any kind of anon_func_types.
When TestEveryCallableContainerIsDeclaredAsAContext roda
The test fails by naming grammar, labeling, and rules.
```

Scenario: Removing `procedure_spec` from `plsql.yaml` reverts the fix and is detected.
```gherkin
Given plsql.yaml sem "procedure_spec: Procedure" em context_types
When a suite rolls
Then TestEveryCallableContainerIsDeclaredAsAContext falha apontando "procedure_spec"
And the TestPackageSpecParametersBelongToTheirSubprogram fails with the parameter assigned to the Package
```

### Feature: Batching do COPY
Ref: UC-02

Scenario: Large lines are divided into multiple batches
```gherkin
Given 3 linhas de aproximadamente 900 bytes de source cada
When `batchRows` is called with an argument of 2000 bytes
Then more than one batch is produced.
And none of the batches are empty.
And the sum of the lines in each batch equals the total input.
```

The larger line exceeds the budget.
```gherkin
Given uma linha com 10000 bytes de source entre duas linhas pequenas
When `batchRows` is called with an argument of 100 bytes
The big line is present in the output.
And the sum of the lines in each batch equals the total input.
```

#### Scenario Outline: casos de borda do batching
```gherkin
Given a entrada "<entrada>"
When `batchRows` is called with budget `<budget>`
Then the result is "<result>".

Examples:
  | entrada        | orcamento | resultado                   |
  | nil            | 64MiB     | nenhum batch                |
  | slice vazio    | 64MiB     | nenhum batch                |
  | 2 linhas       | 0         | um batch com as 2 linhas    |
  | 2 linhas       | 64MiB     | um batch                    |
```

Feature: The search index owns the text.
Ref: UC-03

Scenario: The source comes from file_fts, and the graph is not consulted.
```gherkin
Given um grafo que devolve zero linhas para toda consulta
And an index containing "schema/packages/PCK_X.sql" with the file's text
When `GetSource` is called for "schema/packages/PCK_X.sql"
The text returned is identical to that stored in file_fts
And the graph was never consulted.
```

Scenario: The line in the table file does not load text
```gherkin
Given um parse cache com "a/b.go" e seu source
When fileNodeJSON monta as linhas da tabela File
Then there is no key "source" in the line "a/b.go".
  And tem path igual a "a/b.go"
```

Scenario: Reading the source does not migrate or destroy the index
```gherkin
Given an index whose search_meta schema version has been lowered to "0"
When FileSourceAt is called for a path present in file_fts
Then it is returned as the source.
And a second call returns the source, proving that file_fts was not dropped.
```

The error names the absent store, not the file.
```gherkin
Given an unconfigured SourceService constructed without a storage path
When `GetSource` is called for `"a/b.sql"`
Then an error is returned.
And the message says that the service is down, not that the file doesn't exist.
```

Scenario: Error citing ast_index_source when the path is not in the index
```gherkin
Given an index containing only "a/b.sql"
When `GetSource` is called for "not/indexed.sql"
Then an error is returned.
And the message cites "ast.index_source" as one of the possible causes.
```

Scenario Outline: Entries that Do Not Resolve
```gherkin
Given an index containing only "a/b.sql"
When `FileSourceAt` is called with index "<index>" and path "<path>"
Then a leitura recusa

Examples:
  | indice          | path              |
Valid | Not/Indexed.sql
  | vazio           | a/b.sql           |
Valid |
  | arquivo ausente | a/b.sql           |
```

Scenario: Empty source does not count as found
```gherkin
Given an index whose `file_fts` field contains `"empty.sql"` with a source equal to an empty string.
When `FileSourceAt` is called with "empty.sql"
Then reading refuses, so that the caller still knows the source is unavailable.
```

### Feature: Embedding sob ast.index_source falso
Ref: UC-06

Scenario: The embedding retains the source signal without retaining the source.
```gherkin
Given an empty shard with the function "ChargeCard" on lines 3 to 6
  And o arquivo real em disco cujo hash bate com o hash do shard
  And RepoRoot apontando para a raiz daquele arquivo
When scanPending monta as linhas pendentes com snippet
Then the snippet of the entity contains the body of the function
And the embedded text contains both "ChargeCard" and the body.
```

Scenario: No data is persisted on the shard
```gherkin
Given the same scenario, after scanPending has produced the snippet
When it is recorded on disk and read back
The source field of the shard is empty.
And the file body does not appear anywhere in the JSON of the shard.
```

Scenario: The modified file is not read to avoid poisoning the cache
```gherkin
Given a shard whose hash was calculated based on the original content
  And o arquivo em disco foi reescrito depois disso
When scanPending tenta montar o snippet
Then no snippet is produced because the hash doesn't match.
```

Scenario: The entity remains embeddable without a RepoRoot
```gherkin
Given um shard sem texto e RepoRoot vazio
When scanPending monta as linhas pendentes
Then no snippet is produced
But the embedded text still contains the entity name
```

Scenario: Cache without touching the disk
```gherkin
Given um shard que carrega texto contendo "CACHED_MARKER"
When scanPending monta o snippet
Then o snippet vem do cache, contendo "CACHED_MARKER"
```

### Feature: Sources no bundle e a flag --no-sources
Ref: UC-05

Scenario: The bundle loads text from the search index.
```gherkin
Given an index containing "svc/handler.go", with the text of the file
When ExportBundle is called with StorePath pointing to that store
Then o zip tem um membro "sources/svc/handler.go" com exatamente aquele texto
  And o manifest declara source_count igual a 1
```

#### Scenario: --no-sources omite o texto
```gherkin
Given the same search index
When ExportBundle is called with Sources set to false
Then there is no member named "sources" in the zip file.
  And o manifest declara source_count igual a 0
```

The scenario where the bundle is structural and declares this fact.
```gherkin
Given BundleOptions sem StorePath
When ExportBundle is called
Then the zipper doesn't have any source members
  And o manifest declara source_count igual a 0
```

Scenario: Requesting sources without an index available is an error.
```gherkin
Given BundleOptions pointing to an inexistent store
  And NoSources falso
When ExportBundle is called
Then an error is returned, naming the source stage.
And no bundle is presented as exported successfully
```

Scenario: Skips over lines without text and reports missing index
```gherkin
Given an index with "has/text.go" containing text and "no/text.go" with a source field empty
When EachFileSource traverses the index
Then "has/text.go" is visited with its text.
But "no/text.go" is not visited.
```

Feature: Imported Context Searchable
Ref: UC-04

The index built from the shard cache serves text and search.
```gherkin
Given a shard cache containing "svc/handler.go" with a function named "HandlePayment"
When the BuildSearchIndexFor method is called for a new store
Then FileSourceAt devolve o texto de "svc/handler.go" daquele store
And a search for "HandlePayment" in this index returns at least one result
```

## Files Changed

| File | Change | Reason |
|---|---|---|
Here is the translation:

"_`internal/ast/queries/plsql.yaml`_ | Modified | _`function_spec`/_`procedure_spec` + matview + types of packages in _`context_types`; invariant comment"
| `internal/ast/queries/java.yaml` | Modified | `constructor_declaration` como contexto |
| `internal/ast/queries/javascript.yaml` | Modified | `generator_function_declaration` como contexto |
| `internal/ast/queries/typescript.yaml` | Modified | idem |
| `internal/ast/queries/tsx.yaml` | Modified | idem |
| `internal/ast/queries/dart.yaml` | Modified | `function_signature`/`method_signature` como contextos |
| `internal/ast/queries/objc.yaml` | Modified | `function_definition`/`method_declaration`/`method_definition` como contextos |
| `internal/ast/queries/html.yaml` | Modified | `context_name_paths` para `element`/`script_element`/`style_element` |
| `internal/ast/queries/svelte.yaml` | Modified | `script_element`/`style_element` como contextos, com name paths |
Here is the translation:

| `internal/ast/queries/sql.yaml` | Modified | Only comment: points to `flatLanguages` and explains why it's a purpose plan |
| `internal/ast/json_rebuild.go` | Modified | Aborta em `copyErrors > 0`; `copyNode` batcheia; `batchRows`/`estimateRowBytes`/`copyBatchBytes` |
"Batches according to the same criterion."
| `internal/ast/rebuild_index.go` | Modified | `fileNodeJSON` deixa de emitir `source` |
Here is the translation:

"_`internal/ast/ladybug.go`_ | Modified | Comment in DDL: column _`source`_ survives only for _`'__config__'`_"
Here is the translation:

"_`internal/ast/source_service.go`_ | Modified | _`WithStore`; text comes only from the search index; error messages that distinguish absent store from non-indexed path"

This translation maintains the structure and meaning of the original Portuguese text while rendering it in idiomatic English.
| `internal/ast/fts_sqlite.go` | Modified | `FileSourceAt`, `EachFileSource` (streaming) e `BuildSearchIndexFor` |
| `internal/ast/bundle.go` | Modified | `BundleOptions`, membros `sources/<path>` em streaming, `source_count` no manifest |
Here is the translation:

"_`internal/ast/server.go`_ | Modified | Content Endpoint Reader from Index; _`storePathFor`_; accepts _`no_sources`_"

This translation maintains the structure and meaning of the original Portuguese text, providing a natural-sounding English equivalent.
The inline 0 is modified; the property table of `File.source` is not present, and the section titled "Quick Source Peek" has been rewritten.
Here is the translation:

"____ | Modified | The installation of artifact AST builds the search index, and fails if it cannot."
| `internal/mcpstdio/tools_ast.go` | Modified | Passa o store ao `SourceService` |
| `cmd/graphit/commands/runners.go` | Modified | Idem na CLI; mensagem do export de bundle deixa de prometer sources |
| `internal/ast/containment_coverage_test.go` | Modified | `TestEveryCallableContainerIsDeclaredAsAContext` + helpers + `callableContainerExemptions` |
**Translation:**

| `internal/ast/oracle_package_spec_owner_test.go` | Created | Regression of reported bug (spec, body, survival in cache) |
| `internal/ast/copy_batch_test.go` | Created | `batchRows`/`estimateRowBytes` |
Here is the Brazilian Portuguese text translated into idiomatic English:

"_______ | Created | Index-only Text, File without `internal/ast/source_search_index_test.go`, no migration, edge cases, searchable context"

This translation maintains the original meaning while making it more natural and conversational in English. The underscores (_) have been removed for clarity.
Inline 0 created | Sources not in bundle, `--no-sources`, structural bundle, error when index is missing, scan
Here is the Portuguese text translated to idiomatic English:

"____ INLINE 0 ____ | Modified | `RepoRoot`, `sourceFromDisk` with hash guard, lazy resolution by shard, `repoRoot` in `RunEmbeddingLoop`"

This translation maintains the structure and meaning of the original Portuguese text while rendering it in a more natural English phrasing.
| `internal/daemon/adapters.go` | Modified | Propaga `rootPath` ao loop de embedding |
| `cmd/graphit/commands/ast.go` | Modified | `RepoRoot` no `ast embed` |
| `cmd/graphit/commands/lifecycle.go` | Modified | `RepoRoot` no caminho de sync |
Here is the Portuguese text translated into idiomatic English:

The inline 0 created by `internal/ast/embedder_no_source_test.go`. Sourced signal from `index_source: false` without text. Hash guard, no degradation with root, preference for cache.

Trade-offs and Decisions

The `sql.yaml` was intentionally reverted. The first attempt added
  `create_function`/`create_table`/`create_view` a `context_types`, e
The inline code has rejected: `sql` is in `flatLanguages`.
Justification written. The grammar Tree-Sitter-SQL only captures TOP-level CREATES and does not have a feature for capturing other types of SQL statements.
Query of column or parameter - nothing is nested for assignment. It's just a comment.
Pointing to the existing decision. A guard test with written reason won the challenge.
Assumption, which is exactly what he exists to do.
Batching by bytes, not lines. An entity line contains dozens of bytes; one of
The file contains the entire archive. A limit by count would be pointless in the single case that matters.
"Abandon in progress instead of publishing partially." Aligning with the decision already taken on the path.
Incremental (`internal/ast/incremental_rebuild.go`). Cost: an outdated index instead of
An incorrect index - and an obvious error instead of silence.
A copy that is viewable, not a chain of fallbacks. The design went through three versions in...
Sequence, and the first two were wrong: (1) reading the shard from the parse cache when the graph
Failure; (2) when the graph fails, read from INLINE_0 with the shard behind. Both dealt with
Redundancy as strength. The third removes the problem instead of circumventing it: the text has one.
Owner only — the index of search, the sole copy that is accessible — and the graph disappears.
Keep it. Without fallback, there is no possible divergence between copies, and the `COPY` that caused the
  incidente deixa de existir.
Choosing which copy to read has never been an expense of space. The three were already paid for in...
  disco. A economia veio de **eliminar** uma: `File.source` sai do `ladybugdb`.
The column `source` is in the DDL of the table File, solely due to the synthetic node.
Here is the translation:

"`'__config__'`," which is not an actual file—`RunEnrichment` stores the detected configuration, and
The skill documents her consultation. Alternative considered and rejected: move this load to another.
Property, what would tear up a documented query for cosmetic cleaning.
Index required for search during installation of the Hub, not best effort. If the text is only there.
Context without an index is not degraded context; it's useless context—so the installation fails instead.
Half-finished delivery. Even principle of rebuilding's half-done.
- **Ler o arquivo em disco para embeddar, em vez de persistir o snippet.** A alternativa era
  gravar o snippet por entidade no shard mesmo sob `index_source: false`. Recusada por duas
Reasons: Contradicts the purpose of the shield for those who link it by confidentiality, and expensive —
The _INLINE_0_ measures body size by entity at 1.31 times its own file size.
Reading the file is akin to sharding the stream, just as scanning already does
There is, and it does not persist anything. The price accepted depends on whether the tree is in disk at the time of
Embedding is true for the project itself and for the context of local paths, not for
artifact of the Hub - where degradation is explicit.
- **Guarda de hash em vez de confiar no arquivo.** Sem ela, embeddar um arquivo mais novo sob a
Old cache key would store a vector describing code that the graph does not contain, and it
Survive until the file changes again. Preferred not to have a snippet with the wrong one.
The path migrates and drops the schema.
In a version divergence. Accepted cost: opening and closing an SQLite connection by
Reading, on a path that only turns when the graph has already failed.
- **Teste-guarda restrito a callables.** A auditoria completa (todo label de container) gera
~30 signals, almost all benign (imports, `package_declaration`, our wrapped components)
for a more immediate context), requiring a large enough list of exceptions to turn into
Noise. Calls are the initial cut: they have Parameters, and one without
Owner is discarded, not just archived incorrectly.
Exceptions to callability are explicit and justified (`callableContainerExemptions`), in
same spirit of `flatLanguages` : "I didn't think about it" and "there's nothing to think about" should be
Distinguishable in the file.

## Technical Debt

The six open debts were closed on August 4, 2026. Two of them, upon verification with
Instead of reasoning, they revealed flaws that the original justification denied.

The index of the corpus was repaired — `make install` + `ast index --reset`. Verified:
7,823 we File (was 0); the parameter under the procedure that declares it, with uid by
The program ended, then the collision happened; Parameter by owner Procedure 33.472 /
      Function 12.432 / **Package 0**; `ast source` e `ast_search` respondem.
The construction of the search index has stopped logging. It is the third case of the pattern.
Log in and follow, and the worst of all three after the text settled there: fail.
      custava o `ast source` do projeto inteiro. `pipeline.go` conta como write error,
      `fullRebuildWithSearch` retorna o erro, e os dois caminhos incrementais capturam o erro
      da goroutine e o devolvem ao chamador.
- [x] **Os logs do rebuild passaram a existir.** A causa era mais simples e pior do que
"are not persisted": `PipelineOptions.Logger` was null and `slogutil.Resolve(nil)`
      devolve um handler **NOP que descarta todo record**. As linhas que havia em
They came from the supervisor, not the pipeline—what made the logs
It is functional. `projectRebuildLogger` writes in the same file, linked to the module of
      sync e no de embedding.
- [x] **A leitura por entidade virou leitura por shard.** O arquivo inteiro continua sendo
Read because the hash covers the entire file and there's no way to verify it by intervals.
Lines - Memory remains in parity with the default path, where the shard carries the same.
The original was indeed wasteful, and it has been corrected: INLINE_0__ does
      `strings.Split` do arquivo inteiro uma vez por ENTIDADE. Um arquivo com 500 entidades
It was divided into 500 parts. Now **INLINE_0** receives the already divided lines,
      uma vez por shard.
The degradation without text has been reported. The investigation closed the debit by another method.
Path: The embedding cache "jumps" through the artifact of the Hub and feeds into the index, then one.
Contextual installation inherits the vectors from the origin and does not need the text. The actual gap is
      estreita — entidade cujo vetor a origem nunca calculou — e nada pode ser feito
      localmente. `scanPending` conta os shards sem texto e avisa, em vez de degradar calado.
- [x] **`clojure`, `julia` e `r` foram verificados com fixture, e a justificativa estava
Wrong. The exception stated that "parameters resolve by a more proximate context."
Declared. They couldn't resolve: **Julia** left the context EMPTY — and INLINE 0
It discards parameters without ownership, so it lost all of them; **r** assigned to a function called
The inline keyword itself because `function_definition` does not have field `name`.
They began to declare containment by virtue of their assignment around.
Here is the translation:

"`parent_capture` in the standard; `function_definition` exited from `context_types` of R, where only"
She could produce the ghost. Clojure does not declare a parameter query, so the issue.
There is none— and that's an assertion now.
The audit of containerized services is now tested.
(`TestEveryNonCallableContainerIsDeclaredAsAContext`, 108 verified declarations, 23)
Exceptions Justified) She found the same bug in `html` as in `toml`: `table` was
The context was declared without `context_name_paths`, so the node was transparent, and all pairs fell.
Instead of belonging to your table.

### Achados novos, corrigidos em seguida

Four of the five grammars without context were corrected.
      (`service: service_name`, `message: message_name`, `enum: enum_name`), `graphql` (os
Four types by ``name``, which is a kind and not a field), ``markdown``
      (`section: atx_heading/heading_content`) e `elixir` (`call: arguments/alias`, mais
In the query parameter for `parent_capture`, because a `def` does not have an alias to name it.
Their parameters would go to the module instead of the function). Verified: `card_id` belongs to
      `Charge`, o `rpc` a `Payments`, campos GraphQL ao seu tipo, code block ao heading,
parameter elixir for its function.
The entity heading in Markdown has been renamed. It was called INLINE_0 — node.
Here is the translation:

"Fully inline, including marker and newline – while the context resolves"
The name that doesn't match its own context is an invisible father. The query has passed.
      capturar `heading_content`.
- [x] **`hcl` continua com contexto inerte, e a tentativa de corrigir foi revertida.**
      **FECHADO mais abaixo, nesta mesma tarefa** — ver "Um name path passou a saber escolher o
N-th child of a kind and the item `hcl` in `### Closed in sequence`. The checkbox is now
Open due to an oversight in accounting and corrected on August 5, 2026, without code changes:
"`hcl.yaml` already brings"
      `block: string_lit[1]/template_literal|string_lit[0]/template_literal`, e
      `TestHCLAttributesBelongToTheirBlock` afirma que `bucket` pertence a `logs` — a
INSTANT, not the type — with `assertNoDanglingContains` at the end. Open debit that has already been closed.
It's worse than nothing: the next agent is supposed to redo the work already done.
The diagnosis report follows below, as it explains *why* the index was necessary.

      Um path
Only goes down, so the only name that can be reached is the first label in the block: TYPE.
`resource "aws_s3_bucket" "logs"`, not an instance. And the type is not an entity —
It asserts that it should never be - then name the context
He synthesized an ID parent and emitted edges for a node that
never is created: measured INLINE 0 with
The attribute bound to the file is incorrect; pointing to a ghost is worse, and
With the build failing in COPY, it starts risking completely destroying the index. It needs
      de um path que saiba dizer "o segundo filho deste kind".
- [x] **Ganhou guarda a classe de erro que eu mesmo cometi duas vezes.**
      `assertNoDanglingContains` converte para cache e verifica que todo `ParentUID` de aresta
CONTAINS exists between entities. INLINE_0 does not validate this: when it doesn't find
      o pai, sintetiza o UID (`entityUID(relPath, e.Context, "")`) e emite a aresta de todo
Approach - same as historical failure "Table does not exist" that would drop the rebuild.
      Aplicado nos testes de r, julia, toml, protobuf, graphql, markdown e elixir.

Closed in sequence

The name path has now learned to choose the nth child of a kind and have alternatives.
The ``contextSpec`` variable stores a list of ``namePath``, with each segment optionally containing an index.
      (`kind[n]`, base zero) e o valor aceita alternativas separadas por `|`, tentadas em
Order. `parsePathSegment` has a unit test because an improperly formed segment fails
OPEN - degrades to "the first child of that breed," or resolves for the wrong one
Instead of for nothing, and the wrong owner is exactly how this area fails.
A void alternative and an index on a field (a field stores only one node) fail in
      validador.
- [x] **`hcl`** — `block: string_lit[1]/template_literal|string_lit[0]/template_literal`. O
The index is what makes it correct: INLINE_0 has two labels and the
entity is the second; the first is the type, which deliberately is not an entity. The second
Alternative covers a block of one label—variable, output, module, provider—which is solely
      label É o nome. Verificado: `ami` pertence a `logs`, `default` a `region`, sem aresta
      pendurada.
- [x] **Tabela toml com chave pontilhada** — `table: bare_key|dotted_key`. `[server.http]`
Now you're in charge of your peers.
- The anonymous function parameter in R has gained a new owner. `binary_operator: Function` with path
**INLINE_0** - which is the same node from where the entity **Function** derives its name, so the owner always
There exists. `function_definition` follows outside because it only produced the ghost `function`.
Verified: Inside an inline lambda within `charge_card`, parameter `item` belongs to
INLINE_0 — not to the lambda, which does not name it, but to the function whose body it is.
The elixir has stopped transforming arguments into functions. Every declaration in Elixir is a...
Here is the translation from Portuguese to idiomatic English:

"`call`, then a common predicate-free pattern in the target house called with the standard format:"
      `def charge(amount, currency)` produzia Function `amount` e Function `currency`, e
      `alias Other.Helper` produzia um Module. Predicados `#match? "^(def|defp|defmacro|defmacrop)$"`
      nas queries de Function e Parameter, `#eq? "defmodule"` no Module e `#eq? "defstruct"` no
      Field.

### Aberto

Nothing on this task. The registration **INLINE_0** is **empty** — five
Grammar rules that were there went out— and inline 0 fails if one
sexta aparecer.

## System Knowledge

- **`resolveParentContextAntlr` devolve o primeiro ancestral que É contexto, e ignora os que
They are not... there is no fallback for "the closest ancestral with a name." A rule outside of this context.
The `INLINE_0` is invisible, not used as a last resort. That's what causes the bug to be
Silent: The result is a plausible owner, just wrong.
- **`resolve`/`owner` pulam o auto-nome.** Uma entidade cujo container tem o mesmo nome e label
It is why she herself cannot be contained by her; the search continues above. Therefore, `create_type` as
Context does not create self-loops in the attributes of an `CREATE TYPE`.
In Tree-Sitter, an arrow function assigned to a variable is resolved by `anonHit`.
  (`internal/ast/treesitter_context.go:246`), via `anon_func_types` + `variable_declarator` —
Not because of `context_types`. Declaring `variable_declarator` as a context is unnecessary and
It would be misleading, because INLINE_0 also serves as a common variable.
Explicit is different from absent. Nil falls into
The ``defaultContextTypes`` and ``{}`` mean "this grammar is purposefully flat." For a
Grammar format data falls into the default category, which is documented as pathological in the documentation.
Inline 0: 74% of the parsing time spent climbing up to the root, testing various types of JavaScript.
The context node without `context_name_paths` can be inert. If the context node does not have
Field `name` and `nameNodeOf` return nil, and the container becomes transparent. Declare context not
  garante que ele seja usado — era o caso de `html.yaml`.
Entities in PL/SQL have `line_number == end_line` (the span comes from the name node, not the definition node)
declaration), then containment here **cannot** be derived from a line range — only from
  cadeia de contexto. Uma consulta que tente inferir dono por intervalo de linhas num package
It doesn't work.
The **INLINE_0** is fixed text, not introspection. He listed **INLINE_1**.
Normally, in a bank with zero nodes, File. Counting of nodes requires `count()`.
The `UNION ALL` in LadybugDB scrambled the correspondence between branch and line in a query of
  contagens (`MATCH (d:Directory) RETURN count(d) UNION ALL MATCH (p:Procedure) ...` devolveu
Numbers that did not match with the same counts rolled separately) for counting of
Diagnosis, run separate queries.
Brazilian Portuguese to idiomatic English:

- "LadybugDB does not exist in INLINE_0" — group by directory requires INLINE_1.
- **O shard cache fica em `<dir do DB>/shards/`**, ou seja `.graphit/ast/project/shards/`, e
Not in `.graphit/cache/` (this skill is not inline). Each shard `.nodes.json` carries `src`.
  completo.
The source lived in three places, written by different paths: inline 0.
  `File.source` no grafo, e `file_fts.source` no `<DBPath>.search.sqlite`. É por isso que o
Disaster was partial - lost a copy, not the text. After this task remains two: the
The shard, which is the parser cache, and the index, which owns it, `file_fts.source` is indexed.
Enter the search with weight BM25 set to 1.0; `name` is also found in the same table as `UNINDEXED`.
Text and localization are now different stores for a purpose. The graph responds *where*
Index responds to what. `ast source` with `entity`.
The two, and that is the only stitch between them.
The ``OpenSearchIndex`` is destructive in version divergence of schema: ``migrateSearchSchema``
  faz `DROP TABLE IF EXISTS file_fts/entity_fts/entity_trigram` quando
Never use this path just for reading.
The context installed by the Hub does not have a search index – corrected here, and still lacks.
  shards ao lado do banco.** `internal/hub/service.go` (`case TypeAST`) carrega o shard cache do
**Clone of the Hub**, not of INLINE_0, so nothing around the store can serve text:
The index is the only way, and that's why building it became mandatory there.
The context installed by `ast_install` from a local path is different; the pipeline runs with
Here is the translation:

"INLINE_0" and produces shards and index normally.
The resolution of context-aware storage has been standardized and did not require any changes:
The index of `lb.cfg.DBPath + searchIndexSuffix` is derived from `query.go:36-38`.
  `openASTDB(projectDir, context)` devolve o backend do contexto. O que faltava era o arquivo
Existence knows it, not the code.
The ``ExportBundle`` is just export: there is no `ImportBundle`. The bundle serves to carry the graph.
for outside (human or another tool); the entry route of context is always shards, way
  `ast_install` ou Hub.

Verification

```
go build ./...                                          # ok
go vet ./internal/ast ./internal/mcpstdio ./cmd/graphit/commands   # ok
go test -tags fts5 ./internal/ast/ ./internal/mcpstdio/ ./cmd/...  # ok
```

The ``fts5`` is mandatory (missing from ``BUILD_TAGS`` in the Makefile); without it, two tests of the search index.
falham com `no such module: fts5` por motivo ambiental.

Testes negativos executados para provar que as redes pegam o bug — removendo
`procedure_spec: Procedure` de `plsql.yaml`:

- `TestEveryCallableContainerIsDeclaredAsAContext` falha nomeando `procedure_spec`;
- `TestPackageSpecParametersBelongToTheirSubprogram` falha com
`P_LOG_TX owned by Package "PCK_COBRANCA"` — an exact reproduction of the symptom reported.

## Progress Log

### 2026-08-03
- Investigado o caso reportado no grafo do `corpus-privado`: `P_LOG_TX` sob
Here is the translation:

"`PCK_EXEMPLO` and the measured standard in 9.052 parameters, all under `schema/packages/`."
Traced back to the cause at INLINE_0 of INLINE_1 × INLINE_2.
Discovered while investigating why **INLINE_0** was not responding, the second bug: zero files in the directory.
Graphed with the source intact across shards, by the failing COPY that did not roll back the swap.
Audited are 44 grammars; found gaps in `dart`, `objc`, `java`, and `javascript`.
  `typescript`, `tsx` e a de `context_name_paths` em `html`.
- Primeira tentativa em `sql.yaml` reprovada por `TestEveryShippedGrammarDeclaresItsContainment`
  e revertida.
- Implementados o abort, o batching e a leitura alternativa de source; escritos os quatro
  conjuntos de teste; confirmado com testes negativos.
Review of Source Reading Drawings: Initial version read from the shard in the parse cache. Measured
  que o `<DBPath>.search.sqlite` do corpus tem os 36.823 arquivos com source completo
The file has been modified for the same database whose graph contains zero nodes, at column indexed by (`file_fts`).
  `file_fts` e o shard removido. Descoberto no caminho que `OpenSearchIndex` dropa tabelas em
Divergence of version, hence dedicated read-only reader, and no index in the context of the Hub
  de busca nem shards ao lado do banco.
Second revision, and the one that closes the design: instead of chaining fallbacks, **the owner only**.
  `File.source` sai do grafo (some o `COPY` de 2,4 GB que causou o incidente), `SourceService` e
The HTTP endpoint starts reading exclusively from the index, and the skill stops announcing.
`File.source`, and the installation of the artifact AST for the Hub starts building the index – without which one wouldn't have it.
  contexto importado ficaria sem texto e sem busca. Skill, DDL e mensagens de erro alinhados.

### 2026-08-04
Commit inline 0 in the main with the three preceding axes.
Implemented INLINE 0. It existed on two surfaces and was accepted or discarded.
Brazilian Portuguese:
(`runASTExport` received and did not read; the message promised "(with sources)" always) -- and after that
Remove INLINE_0 from the graph, and it also has no object. Now the bundle writes members.
The indices read in streaming are declared as `source_count`, and sources are requested.
Without an index is an error, and the three surfaces pass to flag.
The snippet of the embedding no longer degraded semantic search.
The translated sentence from Portuguese to idiomatic English is:

"The vector lost precisely what it was supposed to have."
  descreve o que a entidade faz. Agora vem do arquivo em disco, com guarda de hash, sem
  persistir nada. `RepoRoot` propagado por daemon, CLI, sync e tool MCP.
Reindexed with the new binary. Both reported bugs are closed in.
graph real, with numbers in the section of Technical Debt.
The forecast was wrong, registered because it changed the disk expectation: I said that the reindex.
  encolheria o `ladybugdb` por tirar `File.source`. Ele **cresceu**, de 571 MB para 1078 MB. O
The reason is obvious in retrospect and invalidates the comparison: the 571 MB was a graph that had been mutilated —
zero we File, therefore no edge `CONTAINS File→entidade` nor `Directory→File` — and still
There are 12,752 fewer parameters (45,904 against 33,152). The new bank is larger because it's complete.
How much would it cost above that not be measurable without reintroducing it?
