# Passo 1 da migração: índice de busca no LadybugDB (FTS + vetor), e o bloqueio que impede remover o SQLite

**Data:** 2026-07-26
**Escopo:** `internal/ast/ladybug_search.go` (novo, produção) + 7 arquivos de teste
**Origem:** instrução do Engenheiro — escrever o passo 1 com FTS e vetor juntos e remover o SQLite

---

## O QUE foi entregue

`internal/ast/ladybug_search.go` — índice de busca completo sobre LadybugDB, ao lado do
SQLite. Nada foi removido.

- **Schema:** `SearchEntity(uid, name, name_split, name_tri, docstring, etype, path, line,
  emb FLOAT[768])`, `SearchFile(path, name, name_split, name_tri, source)`, `SearchMeta`.
  `name` é o que o resultado exibe, `name_split` é o que o índice casa — separados de
  propósito, porque o índice SQLite guardava o split na mesma coluna e devolvia
  `"config.go config go"` como nome.
- **Sete índices FTS por campo**, que é como o peso por campo é reconstruído (o
  `bm25(0,10,3,2,1)` do FTS5 não tem equivalente).
- **Índice vetorial** sobre a mesma tabela — sem tabela auxiliar. Linhas com vetor NULL são
  aceitas e ignoradas pela consulta, e o índice pode ser criado em tabela vazia
  (`TestLadybugVectorSchemaConstraints`), o que dispensa o `entity_vec_map` do SQLite.
- **`RebuildFromCache`** com `StreamEntries` (memória O(lote), não O(corpus)), insert em
  lote por `UNWIND`, e swap atômico via caminho irmão + rename.
- **`UpdateIncremental`** in-place.
- **`Search` / `SemanticSearch` / `HybridSearch`** com fusão RRF, mais o fallback
  `CONTAINS` para consultas menores que um trigrama.

### Paridade medida (mesmo `ShardCache` alimentando os dois motores)

| | SQLite | Ladybug |
|---|---|---|
| top-1 esperado, 16 sondas | 12/16 | **14/16** |
| resultados vazios | 0 | 0 |
| semântico: `CFG_LOAD` para `config` | — | **1º/5** |

Cinco testes diferenciais: paridade, idempotência do rebuild, incremental (rows obsoletas
saem, novas entram, repetição não duplica), semântico ponta a ponta com o embedder real, e
fusão que não perde o que cada metade achou sozinha.

## POR QUE o SQLite NÃO foi removido

O plano registrava como fato estabelecido: *"FTS e VECTOR atualizam em insert/delete sem
rebuild"*. Para VECTOR é verdade. **Para FTS é falso.**

```
inserção linha a linha em índice FTS vivo:
  22 de 25 linhas invisíveis — em 12 de 12 iterações
```

Sempre 3 visíveis: padrão fixo, não aleatório, o que indica janela de visibilidade. Isso
explica por que todos os probes anteriores — os do plano e os cinco meus desta sessão —
"provaram" atualização in-place: inseriam 1 ou 2 linhas, dentro dessa janela. Era verde por
tamanho de amostra.

Consequência: qualquer escrita exige recriar os índices FTS, trabalho O(corpus) para
mudança O(1).

| | SQLite | Ladybug |
|---|---|---|
| rebuild total, 200k entidades | 3,2s | 15,8s |
| latência de consulta | 50–146ms | 92–158ms |
| **incremental de 1 arquivo** | **~330ms** (pipeline atual) | **5,3s** |

Remover o SQLite hoje entregaria regressão de ~16× no caminho quente do daemon. A
autorização para remover foi dada antes destes números existirem, então a decisão volta ao
Engenheiro em vez de ser executada sobre premissa derrubada.

## Caminho até o diagnóstico (cinco hipóteses refutadas)

Registrado porque o custo de cada erro foi real e o padrão é reaproveitável:

1. *"Insert em lote não mantém o índice"* — refutado, mantém (com 2 linhas).
2. *"Índice criado antes dos inserts não os vê"* — refutado (com 2 linhas).
3. *"Consulta com parâmetro ligado não casa"* — refutado, funciona igual a literal.
4. *"O índice não sobrevive a CHECKPOINT/close/rename"* — refutado, sobrevive.
5. *"É o corpus, a ordem de build ou o tamanho do lote"* — refutado por bisecção; todas as
   seis combinações falhavam igual.

O que localizou o defeito foi um teste em camadas (`TestLadybugSearchIndexDiagnostic`):
linhas gravadas → consulta FTS crua → passe do wrapper → `Search()`. A camada 2 vinha
vazia com os dados presentes, o que eliminou de uma vez toda a metade de cima. E o que
matou as hipóteses 1 e 2 foi **repetir** o probe com 25 linhas em vez de 2.

Erro meu que vale registrar: um probe intermediário usou `alphaToken` e consultou `alpha`,
concluindo que o FTS estava quebrado. `alpha` não é token de `alphaToken` — eu mesmo já
havia medido isso três changelogs antes. O probe estava errado, não o Ladybug.

## Correção aplicada dentro do desenho

`createIndexes()` passou a rodar **depois** da carga em massa, e não no schema. Isso tornou
o rebuild determinístico (antes: correto numa execução, vazio na seguinte) e também mais
correto: com o índice construído durante a carga, `checksum` devolvia 5 documentos com
score idêntico (0,0202); depois, devolve só o que casa.

`UpdateIncremental` chama `rebuildFTSIndexes()` (DROP + CREATE) após escrever, que é a
recuperação documentada pela própria mensagem de erro do liblbug. É correto e é a origem
dos 5,3s.

## Testes que vigiam, em vez de só passar

- `TestLadybugFTSPerRowInsertIsReliable` está **invertido de propósito**: passa enquanto o
  bug do liblbug existe e **falha quando for consertado**, avisando que
  `rebuildFTSIndexes` pode sair e que o custo O(corpus) por edição desaparece.
- `TestLadybugFTSBulkInsertMaintainsIndex` e `TestLadybugFTSUpdateSemantics` ganharam
  ressalva explícita de que seus verdes descrevem a janela de visibilidade com 2–3 linhas,
  e não manutenção de índice — para que ninguém os leia como prova outra vez.

## Report upstream a fazer (4º da lista)

**LadybugDB/liblbug 0.18.2** — índice FTS não é mantido em `CREATE`, nem por linha nem por
`UNWIND` em lote. Repro mínimo: semear 10 linhas, `CREATE_FTS_INDEX`, inserir 25 linhas,
consultar cada uma → 22 não são encontradas, reprodutível em 12/12 execuções. Só
`DROP_FTS_INDEX` + `CREATE_FTS_INDEX` recupera. O índice VECTOR, no mesmo banco, atualiza
in-place corretamente.

## Estado

Suíte completa verde com `-count=1`: `internal/ai` 102,8s, `internal/ast` 52,6s,
`internal/fswatch`, `internal/daemon`, `internal/sysutil`. `go build -tags fts5 ./...` e
`go vet` limpos.

Removida a scaffolding de bisecção junto com o knob `fileInsertBatchOverride` que ela
exigia em código de produção — investigação encerrada, o gancho não deve sobreviver a ela.
