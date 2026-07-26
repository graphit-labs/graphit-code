# Remoção do SQLite do índice de busca do AST

**Data:** 2026-07-26
**Escopo:** `internal/ast/` (produção e testes), `cmd/graphit/commands/ast.go`
**Origem:** decisão do Engenheiro, reafirmada depois de apresentados os números de regressão

---

## O QUE mudou

`internal/ast/fts_sqlite.go` (938 linhas) foi **removido**. O índice de busca do AST passa a
ser o `SearchIndex` sobre LadybugDB, com FTS e vetor na mesma base.

| antes | depois |
|---|---|
| `internal/ast/fts_sqlite.go` | `internal/ast/search_index.go` |
| helpers de query/identificador dentro do arquivo do SQLite | `internal/ast/search_common.go` |
| `<dbPath>.search.sqlite` | `<dbPath>.search` (constante `searchIndexSuffix`) |

- Consumidores atualizados: `query.go`, `pipeline.go`, `embedder.go`,
  `cmd/graphit/commands/ast.go`. A superfície pública não mudou de forma
  (`OpenSearchIndex(path)`, `RebuildFromCache`, `UpdateIncremental`, `Search`,
  `SemanticSearch`, `HybridSearch`, `Close`), então o diff nos consumidores é o caminho.
- `CleanupInterruptedSwap` passou a proteger `<dbPath>.search` — ele apaga irmãos
  `<dbPath>.*` como resíduo de swap, e o índice de busca não é resíduo.
- `internal/ast/search_common.go` reúne o que é independente de motor: `tokenizeQuery`,
  `splitCodeIdentifier`, `identifierTrigrams`, `normalizeForTrigrams`,
  `sortResultsDeterministic`, `deduplicationKey`, `dedupTokens`, `rrfK`. São decisões
  medidas, não escolhidas, e sobrevivem à troca de armazenamento.

### Verificação de que o desacoplamento é real

`go test ./internal/ast/` **sem a tag de build `fts5`** passa (58s). Antes era impossível:
o pacote não compilava sem o driver SQLite com FTS5.

## O que NÃO foi removido, e por quê

`go-sqlite3`, `sqlite-vec` e `BUILD_TAGS := fts5` **permanecem**, porque
`internal/wiki/fts.go` (1494 linhas) tem o próprio índice SQLite FTS5 + vec0, consumido por
`chat`, `memory`, `knowledge`, `uiserver` e `daemon`. Migrá-lo é outra migração de porte
comparável, fora do escopo desta.

Gap de cobertura encontrado no caminho: o wiki tem 97 funções de teste que rodam em 0,016s
e **nenhuma** abre um banco com as três tabelas `fts5`/`vec0` que ele cria. Remover a tag de
build não faria a suíte falhar — quebraria em runtime, em silêncio. Não é escopo desta
mudança, mas está registrado.

## Custo aceito, medido antes da decisão

| | SQLite | Ladybug |
|---|---|---|
| top-1 esperado (16 sondas) | 12/16 | **14/16** |
| rebuild total, 200k entidades | 3,2s | 15,8s |
| latência de consulta | 50–146ms | 92–158ms |
| incremental de 1 arquivo | ~330ms | **5,3s** |

O incremental regride ~16× porque o índice FTS do liblbug 0.18.2 não é mantido em insert
(22 de 25 linhas invisíveis, 12/12 iterações), obrigando `DROP` + `CREATE` dos sete índices
após cada escrita. Isso foi apresentado ao Engenheiro com os números; a decisão de remover
foi reafirmada.

## Testes: o oráculo diferencial deixou de existir

Vários testes existiam para comparar Ladybug contra SQLite. Sem o segundo lado, cada um foi
convertido preservando a expectativa, em vez de apagado:

| antes | depois |
|---|---|
| `TestConsolidationQualityGate` (Ladybug × SQLite) | `TestSearchIndexQualityFloor` — piso absoluto, com o 12/16 do SQLite registrado como referência |
| `TestPrefixIndexGap` (dois motores) | `TestTruncatedQueryCoverage` — piso de 11/11, motor único |
| `TestAbbreviatedIdentifierSearchSQLite` | `TestAbbreviatedIdentifierRecall` |
| `TestTrigramBagSearchLatency` | absorvido por `TestSearchIndexScaleCost`, que mede o mesmo em escala e ainda cobre o incremental |
| testes de `quoteToken`, `buildPhraseQuery`, `buildANDQuery`, `buildORQuery`, `buildPrefixQuery`, `buildSearchPasses` | removidos com as funções — LadybugDB não tem frase, booleano explícito nem wildcard, então não havia o que construir |

Arquivos renomeados para deixarem de mentir: `abbrev_sqlite_test.go` →
`abbrev_recall_test.go`, `prefix_gap_test.go` → `truncated_query_test.go`,
`ladybug_search*.go` → `search_index*.go`.

## Um resultado que se inverteu com a remoção

`TestExpansionFieldCeiling` media 9/9 para um campo de expansão perfeito. No Ladybug o mesmo
teste dá **8/9** — igual a trigrama sozinho, ou seja, a expansão passa a não comprar nada.

A razão importa: no FTS5 o acerto de `CFG_LOAD` vinha do **índice de prefixo** (`config`
casando `configuration` na docstring). LadybugDB não tem wildcard e o stemmer porter não
reduz `configuration` a `config`. Então **aquele teto de 9/9 era artefato do FTS5.**

O teste foi reescrito para medir as duas redações e registrar a condição real: a expansão só
ajuda se contiver o **token exato** da consulta (`"config load"` → 9/9;
`"configuration load"` → 8/9). Isso é bem mais fraco do que o 9/9 sugeria, e nenhum gerador
garante repetir a palavra do buscador — o que reforça, por um motivo novo, a decisão de não
construir o campo.

## Estado

Verde com `-count=1`: `internal/ai` 98,5s, `internal/ast` 60,0s, `internal/wiki`,
`internal/fswatch`, `internal/daemon`, `internal/sysutil`. `go build ./...` (com e sem a tag
`fts5`) e `go vet` limpos.
