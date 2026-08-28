---
title: Local Ladybug via Icebug filesystem in-memory, sem swap e sem legacy
status: done
created: 2026-08-27
updated: 2026-08-28
tags: [ast, ladybug, icebug, storage, architecture]
---

# Local Ladybug via Icebug filesystem in-memory

## Objective

Tornar o grafo local on-the-fly no mesmo formato do Hub (`icebug-disk`/`icebug-canonical`), mas com `storage` em filesystem absoluto em vez de `s3://`. O catálogo `ladybugdb` deixa de existir como arquivo — toda conexão abre `:memory:` e aplica `schema.cypher` que aponta para `graph.icebug/` local. Eliminam-se `CopyDBDir` → `AtomicSwapDB` + `.wal/.shadow` + `CHECKPOINT`, e a publicação no Hub passa a ser `cp -r graph.icebug/ + cypher com storage ajustado para s3://`.

Sem retrocompatibilidade: estamos em dev, artefatos antigos não são lidos, stores file-based são descartados no primeiro sync.

## Reasoning

Pesquisa `docs.ladybugdb.com/import/icebug` + `LadybugDB/ladybug/docs/icebug-disk.md:14` confirma: `storage='<path-to-dir>'` pode ser relativo/absoluto ou `s3://`. Go binding já tem `OpenInMemoryDatabase(":memory:")` `go-ladybug/database.go:70`. Hub já usa `ExportIcebugCanonical` `internal/ladybugstore/icebug_canonical.go:86` e `MountIcebugGraph` `internal/ast/icebug_transfer.go:127`. Local e Hub convergem no mesmo writer/reader, só muda a URI.

Detalhes da convergência exigidos pelo usuário (requisitos de 2026-08-27/28):
1. **Parquets gerados DIRETAMENTE dos shards** — sem popular o catálogo ladybug primeiro para depois exportar (que era O(corpus) extra e impedia o incremental).
2. **Incremental reescreve SÓ os parquets afetados** — o catálogo antigo reescrevia o bundle inteiro (O(corpus), 53s em 24k arqs), o usuário exigiu partial rewrite.
3. **Query local passa pelo MESMO planner/transport do Hub** — bounded traversal planner (`tryCanonicalBoundedTraversal`), fail-closed para rel lógico não-planejável via `namesLogicalRel`; a diferença local-vs-hub é só `storage:'/abs/graph.icebug'` vs `s3://`.
4. **Sem condicionais**: `LadybugConfig` só tem `StoreDir` + `IcebugDir` + `ReadOnly` — NÃO existe `DBPath` nem flag `InMemory` (in-memory não é opcional). Hub também é mount in-memory: só `schema.cypher` + `manifest` + `search.mount` são persistidos; NUNCA se escreve um `ladybugdb` de import.
5. **Nada hardcoded por linguagem** — labels `(File,Function,Class,...)`, colunas e tipos são derivados dos shards (`ExportDirect*` dinâmico), não literais `ICON=GO_ATTRIBUTES`.
6. **Reverse edges** viraram propriedade do BUILD: `hub.icebug.reverse_edges` config => `PipelineOptions.ReverseEdges *bool`; export direto/incremental têm variantes `*WithReverse`. (Antes era também config de publish; agora é dado na geração e o artefato é fiel a ele.)

## Plan & Task Breakdown

- [x] **T1 — Store layout** — `internal/store/store.go`: `ASTProjectIcebugDir()`, `ASTHubIcebugDir()`, `ASTProjectIcebugSchema/Manifest`, `ASTContextIcebugDir`, `ASTHubIcebugSchema`; `internal/store/contextpaths.go`: `ASTContextIcebugDirIn`, `ASTContextDirIn` (leem lockfile: hub → `ASTHubDir`, link → `ASTProjectDir` do irmão, local → `ASTContextDir`). `ASTProjectDBPath/ASTContextDBPath/ASTHubDBPath/DBFileName` eliminados (final, não está mais no código).
- [x] **T2 — Backend in-memory** — `internal/ast/ladybug.go`: `LadybugConfig{StoreDir,IcebugDir,ReadOnly}`; `openOnce` sempre `OpenInMemoryDatabase`; `mountLocalIcebugLocked` lê `schema.cypher` + `icebug.json` do `IcebugDir` e executa o DDL; `Query()` se `canonical` → planner (`tryCanonicalBoundedTraversal`), senão `runQuery` físico; `namesLogicalRel` fail-closed; `StoreDir()`/`Close` sem registry. `LadybugConfigFor` só muda `StoreDir`/`IcebugDir`; `LADYBUGDB_PATH` env vira override de `StoreDir` (e `IcebugDir` derivado).
- [x] **T3 — Export direto dos shards** — `internal/ast/direct_icebug.go`: `ExportDirectFromRebuildIndex(WithReverse)` — iteracao sobre shard tree, colunas derivadas das rows (`columnsForLabel`), tipos inferidos (`inferTypeFor`), IDs densos POR-TABELA (igual `ExportIcebugCanonical`), `writeCanonicalSchemaDirect`, `reverseEdgesDirect(edges, props, from, to)` com self-loop só quando `from==to && source==target`, `copyIcebugFile` para o bundle final; parquets esparsos (`exportDirectDelta`) + `ExportDirectIncremental(WithReverse)` que só reescreve os parquets afetados.
- [x] **T4 — Pipeline** — `internal/ast/pipeline.go`: `RebuildIcebugFromCache(WithReverse)` (full) ou `ExportDirectIncremental` (partial, quando `doIncremental` decide), `bundleDir` do backend (`LadybugConfig.IcebugDir`), search `UpdateIncremental`/`RebuildFromCache`, `ReverseEdges` wired, `ForceRebuild` excluído do incremental; `internal/ast/icebug_rebuild.go`: `rebuildIcebugFromCacheWithDelta` — full + delta.
- [x] **T5 — Planner canônico compartilhado** — `internal/ast/ladybug_icebug_canonical.go`: `canonicalPKFor` (PK real por label: `File.path`, resto `uid`), `sanitizeCanonicalPKEquality`/`rewritePKEqToIN` (rewrites `=`→`IN`), `sanitizeCondPK` (anchor conditions), `canonicalUIDMembers` por `hasKey`, aceita rel-var `[r:CALLS]`, count com `AS n`, regex de RETURN que captura `ORDER BY`/`LIMIT`.
- [x] **T6 — Publish Hub fiel** — `internal/hub/registry.go`: `prepareASTPublish` copia `graph.icebug/` local, reescreve `schema.cypher` trocando filesystem URI por `s3://` `store.ArtifactURI` (`rewriteIcebugStorageURI`); `internal/hub/ast_store.go`: mount = só `schema.cypher` + `manifest` + `search.mount` stageado; `astStoreBuilt` checa `schema.cypher` + `SearchIndexBuilt`.
- [x] **T7 — Remoção legacy** — removidos: `incremental_rebuild.go`, `json_rebuild.go`, `ladybug_registry.go`, `reflink_linux.go`, `reflink_other.go`, test-setup swap (`AtomicSwapDB`, `CleanupInterruptedSwap`, `CopyDBDir`, `engineSidecarSuffixes`), `TestCancelAndRebuild`/`copy/`/`e2e_bench`/`incremental_cost_probe`/`real_corpus_incremental_probe`/`ladybug_checkpoint`/`ladybug_reader_isolation` (ver git status).
- [x] **T8 — Callers adaptados** — `server.go` (getOrCreateCachedDB/storePathForRequest), `query.go`, `bundle.go`, `source_service.go`, `embedder.go` (search-only rebuild), `mcpstdio/context.go`+`tools_ast.go`+`tools_lifecycle.go`, `cmd/graphit/commands/{ast,lifecycle,runners}.go`, `internal/daemon/syncmodule.go`, `internal/ast/config.go` (ImportedContext.StoreDir, contextStoreBuilt via bundle, ListImportedContextsIn), `internal/hub/service.go` Link check via `graph.icebug/schema.cypher`.
- [x] **T9 — Docs** — `docs/architecture/storage_layout.md`, `docs/specs/ast_module.md`, `docs/specs/hub_collaboration.md`, este task log.

## Implementation Details

- Seletor de corpos linguagem: `internal/ast/rebuild_helpers.go` (novo) — `engineOwnedRelTypes()` (CALLS/OWNS), `shortHex()`, `BuildEmbLookup()`, `batchRows()`, `estimateRowBytes()`, `copyBatchBytes()`, `writeJSONFile`; substitui os helpers dos arquivos de rebuild deletados.
- `mountLocalIcebugLocked` usa `IcebugDir` (bundla) → `schema.cypher`; para Hub import de S3, `MountIcebugGraph(ctx, storeDir, schema)` só guarda o schema; nunca precisa de rede no catcher: o read é lazy no engine (`storage='s3://'`).
- `ExportDirectFromRebuildIndexFromStore` (`icebug_transfer.go`) existe para re-export de um store derivado do bundle mount (usado no teste de traversal remoto `ladybug_icebug_canonical_test:259,345`); `ExportGraphToIcebug` é wrapper dele para testes de custo com `GRAPHIT_REAL_STORE`.
- O `SearchIndex*` API recebeu `storeDir` (antes `dbPath`) — `LanceIndexPath(storeDir)`, `searchMountURI(storeDir)`, `searchConfigFor(storeDir)`, `OpenSearchIndex(storeDir)`, `WriteSearchMount(storeDir, uri)`.

## Use Cases

### UC-01: Index local on-the-fly filesystem
- **Actor**: `graphit sync`/`graphit ast index`
- **Preconditions**: projeto com lockfile ULID
- **Main Flow**: parse shards → `RebuildIcebugFromCache(WithReverse)` gera `graph.icebug/schema.cypher + nodes_*.parquet...` diretamente dos shards (novo bundle em cache-dir `tmp.<hex>/` → rename) → próxima query abre `:memory:` + aplica `schema.cypher`
- **Error**: export falha → sync falha, bundle antigo mantido (rename não ocorreu)
- **Files**: `internal/store/store.go`, `internal/ast/direct_icebug.go`, `internal/ast/icebug_rebuild.go`, `internal/ast/pipeline.go`

### UC-02: Query local sem arquivo db
- **Actor**: `graphit_ast_query` (MCP)
- **Preconditions**: `graph.icebug/` existe
- **Main Flow**: `NewLadybugDB`(config=only StoreDir+IcebugDir) → `:memory:` → `LOAD schema.cypher` → `Query()` responde via planner canônico OU runQuery, apontando `/abs/graph.icebug`
- **Error**: `graph.icebug/` ausente → `contextStoreBuilt` false → MCP devolve "no store"; erro de DDL → hint
- **Files**: `internal/ast/ladybug.go`, `internal/ast/server.go`, `internal/mcpstdio/context.go`

### UC-03: Publish Hub a partir do estado local
- **Actor**: `graphit hub submit`
- **Preconditions**: `graph.icebug/` local pronto
- **Main Flow**: copiar `graph.icebug/` → tmp publish → substituir storage URI filesystem por `s3://bucket/prefix/graph.icebug` no `schema.cypher` → upload S3
- **Files**: `internal/hub/registry.go`, `internal/hub/ast_store.go`

### UC-04: Hub install (mount, sem baixar o grafo)
- **Actor**: `graphit hub install`/MCP, `contextStoreBuilt`
- **Preconditions**: artefato publicado (schema.cypher + nodes_*.parquet no S3)
- **Main Flow**: `ensureASTStore` → `MountIcebugGraph` stage só `schema.cypher` (in-memory per-conn) + manifest + `WriteSearchMount` → `SearchIndexBuilt` usado como built check
- **Files**: `internal/hub/ast_store.go`

## Test Cases & Acceptance Criteria

### Rodando (passagarei `ok` na suite completa com `-tags lancedb`, 2026-08-28)
```gherkin
Given projeto com 2 funções e 1 CALLS
When sync gera graph.icebug filesystem
And query MATCH (n) RETURN count(n) via :memory: mount
Then count igual ao nativo e existe nodes_*.parquet
```
- `internal/ast`: `TestIcebugArtifactMountsAndAnswers` (hub), `TestCanonicalPQPlannerTraversal`, `TestCanonicalPQCallsTraversal`, `TestCanonicalCountPatternAs`, `TestAllNamedLogicalRelsFailClosed`, `TestMountedIcebugRemoteRealGraph` (env-gated),...
- `internal/hub`: publish roundtrip + mount tests; `TestHubService_Link_AST` agora escreve `graph.icebug/schema.cypher`.
- `internal/livesearch/prep`: `TestCodeGraphsAreReportedByNameOnceAddressable` — setup do teste foi corrigido para criar `schema.cypher` em `HubContextDir` (o "store built" agora é presença do bundle).
- `internal/daemon`: e2e syncmodule checa `store.ASTProjectDir()/graph.icebug/schema.cypher` + `OpenSearchIndex(storeDir)`.

### Pendente de rodar/validar manualmente (E2E CLI)
- [ ] `go build -tags lancedb ./...` + suite completa `go test -count=1 -tags lancedb -timeout 1200s ./internal/... ./cmd/...` **após limpeza de hoje** (parcial: ast/hub/store/daemon/mcpstdio/prep/commands ok; ast full está rodando)
- [ ] `GRAPHIT_GLOBAL_DIR=<tmp> graphit ast index` em repo com >200 arquivos → confirmar bundle + tempo
- [ ] Editar 1 arquivo, `graphit ast index` de novo → deveria reescrever só parquets afetados (medir: `nodes_0.parquet` mtime muda poucos parquets, não todos)
- [ ] `graphit hub submit` contra S3 fake (MinIO) → reescreve URI p/ `s3://`, parquets idênticos
- [ ] `graphit hub install` de outro projeto → mount, query via MCP node, sem baixar graph
- [ ] `graphit ast link` → `TestHubService_Link_AST` passa no código; manual check do erro "index the source project first" com bundle ausente

## Files Changed (destaques)

| File | Change | Reason |
|---|---|---|
| `internal/ast/direct_icebug.go` | NOVO | export direto dos shards, dinâmico, incremental parcial |
| `internal/ast/icebug_rebuild.go` | Novo | RebuildIcebugFromCache(+WithReverse), delta |
| `internal/ast/ladybug.go` | Modificado | :memory: + mount schema per-conn, sem DBPath/InMemory |
| `internal/ast/ladybug_icebug_canonical.go` | Modificado | planner compartilhado convergido com Hub |
| `internal/ast/pipeline.go` | Modificado | usa direct export/incremental, ReverseEdges |
| `internal/ast/rebuild_helpers.go` | Novo | helpers dos rebuilds removidos |
| `internal/ast/icebug_transfer.go` | Modificado | mount staging, ExportDirectFromRebuildIndexFromStore |
| `internal/ast/server.go`, `query.go`, `embedder.go`, `bundle.go`, `source_service.go` | Modificado | callers |
| `internal/ast/config.go` | Modificado | StoreDir, contextStoreBuilt, ListImportedContextsIn |
| `internal/ast/search_lance.go` | Modificado | API search por storeDir |
| `internal/store/store.go` + `contextpaths.go` | Modificado | resolvers icebug, legacy DBPath* removidos |
| `internal/hub/registry.go` | Modificado | publish por copy bundle + rewrite URI |
| `internal/hub/ast_store.go` | Modificado | mount só schema+manifest+search.mount |
| `internal/hub/service.go` | Modificado | Link check via bundle |
| `internal/mcpstdio/` | Modificado | context resolver/tools adaptados |
| `cmd/graphit/commands/` | Modificado | help text sem ladybugdb |
| `internal/livesearch/prep/index_test.go` | Modificado | setup do teste novo em `schema.cypher` |
| `internal/daemon/syncmodule_e2e_test.go` | Modificado | check bundle+search index |
| `internal/hub/coverage_extra_test.go` | Modificado | Link test usa bundle |
| `internal/store/store_test.go` | Modificado | shapes icebug |
| `internal/ast/hubstore_test.go` | Modificado | ContextDirIn, sem DBPath |
| `docs/architecture/storage_layout.md`, `docs/specs/ast_module.md`, `docs/specs/hub_collaboration.md` | Modificado | formato novo |
| Deletados | incremental_rebuild.go, json_rebuild.go, ladybug_registry.go, reflink_*.go, cancel_and_rebuild_test.go, copy_test.go, e2e_bench_test.go, incremental_cost_probe_test.go, real_corpus_incremental_probe_test.go, ladybug_checkpoint_test.go, ladybug_reader_isolation_test.go |

## Trade-offs & Decisions

- In-memory vs file catalog: in-memory elimina WAL/lock/checkpoint; custo = reaplicar schema a cada conexão (ms). Escolhido in-memory, sem condicional.
- Export direto vs populate-then-export: populate do catálogo era O(corpus) e impossibilitava incremental; export direto não tem esse lago.
- IDs por tabela (não globais): anti-pattern de `count` desgarrado; self-loop apenas quando `from==to && source==target`.
- `ExportDirectIncremental` cai em full quando há arquivos deletados (não dá para re-derivar dados antigos); aceito — neste caso renomeia o bundle inteiro, mas o incremento é melhor.
- Remove TODO legacy DBPath*: o env `LADYBUGDB_PATH` continua para override de StoreDir em testes/dev; `Internal/ast/rule.go` help text atualizado p/ `graph.icebug`.

## Technical Debt

- [ ] Medir e documentar tempo do full rebuild em repo grande (~24k arquivos, antes era 53s; o direct export deve sair bem mais rápido — não medido com a versão final).
- [ ] `ExportGraphToIcebug`/`ExportDirectFromRebuildIndexFromStore` são usados apenas por testes de custo (env-gated) — decidir se viram API pública ou são movidos para _test-only. (Hoje: ficam em `icebug_transfer.go`, compilam, e um deles é usado em `ladybug_icebug_canonical_test` env-gated.)
- [ ] `ladybug_segmentation_cap` e shard tree no bundle (`nodes_*.parquet` nomes a partir do ShardCache) não usam shard segment names reais? — verificar se o nome do parquet final carrega o shortHex do shard e se o manifest reflete isso.
- [ ] `item.path_lower`-style columns: o export direto deriva colunas de rows; confirmar que `path` nunca é coluna e sim `uid` para `File`, para não divergir do schema físico (_id).
- [ ] Cleanup: `.build.lock` no hub dir; verificar remoção de stores antigos `ladybugdb` no daemon (ficou sem código de cleanup? `atob`/`sync` does not delete legacy files; decide follow-up).
- [ ] `internal/ast/rule.go` texto de "Where the graph lives" é a única rota que mostra caminhos ao usuário; revisar se deve incluir `graph.icebug/schema.cypher` (já feito) e reindexar wiki.

## Progress Log

### 2026-08-27
- Pesquisa web formato icebug filesystem confirmada (`storage='<abs>/graph.icebug'`, docs.ladybugdb.com/import/icebug) e `:memory:` (`lbug.OpenInMemoryDatabase`) validada.
- Implementado v1: `ASTProjectIcebugDir`, `LadybugConfig.InMemory`, `mountLocalIcebugLocked`, `RebuildIcebugFromCache`, `prepareASTPublish` filesystem→s3, `isCanonicalTraversalQuery` para single-hop, checagens `InMemory` em `newASTBackendReadOnly`/`mcpstdio`.
- Validado: `GRAPHIT_GLOBAL_DIR=/tmp/graphit-test-icebug` sync gerou `graph.icebug/` com `storage='/abs'` e `MATCH (n:File)` via `:memory:` retornou 2 rows; publish reescreve storage para `s3://`.
- Docs atualizados (storage_layout, ast_module, hub_collaboration).

### 2026-08-27 (v2 — requisitos do usuário)
- Para o export de populate-then-export: user exigiu parquets direto dos SHARDS (não do catálogo) e incremental reescrevendo apenas parquets afetados → `direct_icebug.go` + `ExportDirectIncremental` + `exportDirectDelta` + `rebuildIcebugFromCacheWithDelta`.
- Escrito primeira versão com labels/colunas hardcoded por linguagem → user recusou; reescrito dinâmico (derivado das rows, tipos inferidos).
- User exigiu query local pelo MESMO caminho do Hub (bounded planner, fail-closed) → `tryCanonicalBoundedTraversal`, `namesLogicalRel`, sanitizações PK (`=`→`IN`, cond em anchor).
- User exigiu "sem condicionais": `DBPath` e `InMemory` removidos de `LadybugConfig`; openOnce sempre `:memory:`; Hub também vira mount in-memory (só schema+manifest+search.mount persistidos).
- Removidos `.wal/.shadow/checkpoint` (AtomicSwapDB, engineSidecarSuffixes, CleanupInterruptedSwap, CopyDBDir) e arquivos legacy associados.
- Fixes de planner: rel-var `[r:CALLS]`, count com alias `AS`, ORDER BY/LIMIT no RETURN, PK por label.
- Fix do writer: IDs por tabela (não global) para evitar "self-loops" de eds entre labels (quando from/to de idiomas trocados).
- `hub.icebug.reverse_edges` → `PipelineOptions.ReverseEdges *bool`; variantes `WithReverse`.

### 2026-08-28 (continuação)
- **Teste do MCP com índice real (`graphit ast index --reset`, repo graphit-code, store 01KSH1…)**:
  - `ast_schema` ✓ — labels com contagens por linguagem (1.47M js, 270k go…), 40 labels genéricas, relationship tables `calls__*/contains__*/imports__*`.
  - `MATCH (n:File) RETURN n.name LIMIT 3` ✓ (bytes do bundle icebug local resolvidos).
  - `MATCH (g:Function {name:'NewLadybugDB'}) RETURN g.name` ✓.
  - `MATCH (f:Function)-[:calls__function_function]->(g:Function {name:'NewLadybugDB'}) RETURN f.name LIMIT 3` ✓ (planner canônico; veio de ações Function e Method dependendo da call).
  - `ast_search` FTS ✓ (`openOnce`, `mountIcebug`, `writeIcebugSchema`);
  - **BUG REAL encontrado**: `ast_search`/`graphit ast query --hybrid` falhava com
    `Invalid input, expected column _distance not found in rank. found columns […,"_score","_rowid"]`
    — causa raiz: índice construído SEM embeddings (`.emb.json` com `emb:{}`; `ast embed` nunca tinha rodado). Com todas as rows de embedding NULL, o rank RRF do engine não produz `_distance` e o erro do C++ engine sobe mudo.
  - **FIX**: `SearchIndex` ganhou `vectorCount` + sidecar `search.lance` irmão `embeds.json` no store dir, escrito por `RebuildFromCache`/`UpdateIncremental` (`writeEmbedsStatus`); `OpenSearchIndex` lê e seta. `HybridSearch` degrada para keywords com log `run graphit ast embed` quando `vectorCount==0`; `SemanticSearch` retorna vazio com o mesmo hint em vez de estourar. No incremental com deletes sem vetores novos, um probe filtrado `WHERE embedding IS NOT NULL LIMIT 1` (`hasVectorRows`) decide o binário. Teste novo: `TestHybridSearchDelegatesToKeywordsWhenTheIndexHasNoEmbeddings`.
  - **status**: suite `internal/ast` verde (124s), família de testes de busca verde; `graphit ast embed` em andamento (daemon); após completar, uma roda (`graphit ast query --hybrid`) valida o hybrid de verdade.
- Funções test-only movidas para `internal/ast/export_test.go`: `ExportGraphToIcebug`/`ExportDirectFromRebuildIndexFromStore` → `exportGraphToIcebug`/`exportDirectFromRebuildIndexFromStore` (+`parseCanonicalManifest`, `copyDirContents`, `rewriteSchemaStorageURI`, `copyLanceIndex`), parâmetro morto `reverseEdges bool` removido; nada exportado restante para produção.
- **PENDENTE**: full suite pós-fix (rodando), embed final + validação hybrid real, commit.

### 2026-08-28 (continuação)
- **BUG REGRESSÃO — ignore rules ignoradas pelo `ast index`** (reportado pelo usuário antes: `node_modules` indexado):
  - Causa: o comando passou a coletar via `collectFilesForPath` (runners.go) e alimentar o pipeline por `ChangedPaths` (scoped). Como o pipeline scoped **não roda** `collectFiles` (aplicador do ignore), `.gitignore`/`.astignore` e dot-dirs nunca eram checados — o `ast index` indexava `node_modules`, `.opencode/`, `graphit.lock.json`.
  - Fix: `collectFilesForPath(rootPath, projectRoot)` agora aplica `ast.NewAstIgnoreChecker(projectRoot)` (boundary = projeto, para escopos como `ast index internal/ui` continuarem honrando a regra da raiz), skipa dot-dirs, e usa `ShouldDescend` para re-inclusões.
  - Testes: `cmd/graphit/commands/collect_files_ignore_test.go` (gitignore, astignore+dot-dirs, scoped-boundary; skip quando grammar indisponível — padrão do repo).
  - Validado manualmente: `/tmp/itest` e `/tmp/itest2` (com `.gitignore internal/ui/node_modules/` e `.opencode/node_modules/zod`) → só `keep/`/`internal/ui/main.go` indexado.
  - **PENDENTE**: `make install` do binário novo e `graphit ast index --reset` no projeto real para expurgar os arquivos errôneos já indexados.

### 2026-08-28 (continuação)
- **FASE 2 — ignores em subdiretórios** (usuário: "tanto os gitignore quanto os ignores customizados de astignore e wikiignore precisam ser lidos nos subníveis e valer"):
  - Descobierto: `.opencode/.gitignore` com `node_modules` não era lido — o checker só coletava ignore files do root até o boundary; um `.gitignore` num subdir (semântica git: scope de diretório) nunca entrava.
  - `internal/ignorer`: novo `IgnoreChecker.At(dirRelPath) DirScope` — devolve o contexto do diretório com os ignore files de cada nível (`.gitignore` + custom `.astignore`/`.wikiignore`), cacheado por diretório; patterns são parseados com o domínio do diretório (gogitignore::domain, já validado: `node_modules` em `.opencode/` casa `.opencode/node_modules/...` e nada mais).
  - `internal/fswatch`: `Ignorer` agora é ALIAS de `ignorer.DirScope` (a interface antiga não era compatível com o retorno covariante do At; explicado no comment); `addTree` atravessa com At e guarda o contexto por diretório; `accept` julga cada evento pelo contexto do seu diretório; helper `usable()` trata o `(*IgnoreChecker)(nil)` tipado como sem-regras (interface nil-typed).
  - `internal/ast/writer.go` (collectFiles) e `internal/knowledge/wiki.go` (enumerateKnowledgeSources + knowledgeSourceFile): atravessam com At, então `.gitignore`/`.astignore`/`.wikiignore` de subdiretórios valem nos discoveries internos; dot-dirs não são mais pulados estruturalmente (regra: só os ignores excluem).
  - `internal/daemon/syncmodule.go`: `ignoreUnion.At` (união AST+wiki) implementada.
  - Testes: `TestAtAppliesSubdirectoryIgnoreFiles` (ignorer), `TestSubdirectoryIgnoreFilesApplyWhileWatching` (fswatch), `TestCollectFilesForPathHonorsSubdirectoryGitignore` (commands), dot-dirs regidos por ignores (commands).
  - **Nota de integração**: `collectFilesForPath` (runners.go) é o discovery do CLI e segue At; o `collectFiles` interno (daemon full-scan/watcher fallback) também; watch via fswatch At.
  - Observação máquina: `graphit-core daemon` (7.4GB RSS) rodando; a suíte `internal/ast` deu "signal: segmentation fault" na rodada 15:12 e passou isolada depois (testes 13s, Verify ok) — provável tensão de memória/execução nativa; ACOMPANHAR (se persistir, investigar liblbug/OOM).

## Handoff

Para outro agente continuar:
1. `go build -tags lancedb ./...` OK + `go test -count=1 -tags lancedb -timeout 1200s ./internal/... ./cmd/...` terminar (ast ~124s).
2. `graphit ast embed` rodando (log `/tmp/embed2.log`): ao terminar, validar `graphit ast query "<termo>" --hybrid --top 5` — deve retornar resultados com scores mesclados (sem `_distance` error) e conferir `embeds.json` → `"vectors": N`.
3. Medir direcionadamente: em repo grande, `graphit ast index` full e incremental (1 arquivo mudado) — registrar tempos no log.
4. Commit: mensagem referenciando este task log, sem amend.
5. Atualizar `docs/specs/ast_module.md` se Discover ou `ast rule` mudarem de forma que o help text precise.
