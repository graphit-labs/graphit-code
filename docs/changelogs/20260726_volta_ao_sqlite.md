# Volta ao SQLite: migração do índice de busca revertida

**Data:** 2026-07-26
**Escopo:** `internal/ast/` (motor de busca e testes)
**Origem:** decisão do Engenheiro depois das medições no corpus real

---

## O QUE mudou

O índice de busca volta ao SQLite (FTS5 + sqlite-vec). `internal/ast/search_index.go` e a
implementação sobre LadybugDB foram removidos; `internal/ast/fts_sqlite.go` voltou.

**O que NÃO voltou junto — e é o ponto desta reversão:** as três melhorias feitas ao
`fts_sqlite.go` antes da migração nunca chegaram a existir num commit com o arquivo vivo
(foram escritas e o arquivo foi apagado na mesma leva). Um `git revert` as teria descartado
em silêncio. Foram reaplicadas à mão:

1. **Saco de trigramas** — `entity_trigram` deixa de usar `tokenize='trigram'` e passa a ter
   `name_tri` com trigramas pré-computados sobre tokenizer de palavras. É o que faz `config`
   encontrar `coreConf` (2/4 → 4/4), que foi o pedido original.
2. **`ORDER BY rank`** no passe de trigrama, que não existia — o RRF pontua por posição, e sem
   ordenação o passe injetava posições arbitrárias na fusão.
3. **`sortResultsDeterministic`** nos quatro pontos (saída de `queryFTS`, de `queryTrigram` e
   os sorts finais de `Search` e `HybridSearch`), sem o qual o top-1 mudava entre builds do
   mesmo corpus.

Também sobrevive `searchIndexSuffix` como constante única, em vez do literal
`".search.sqlite"` repetido em cinco lugares — a divergência entre esses lugares e a lista de
exclusão do `CleanupInterruptedSwap` apagaria o índice.

## POR QUE a migração foi revertida

Três defeitos do LadybugDB 0.18.2, todos medidos:

1. **O índice FTS não é mantido em insert.** 22 de 25 linhas ficam invisíveis, em 12/12
   iterações. Obriga `DROP` + `CREATE` dos sete índices após cada escrita: trabalho O(corpus)
   para mudança O(1).
2. **Falha em cascata no incremental.** A partir do quarto update consecutivo, o delete
   abortava com *"FTS index 'sf_source' is inconsistent: document for node offset 3002 is
   missing during delete"*. Corrigido no desenho (derrubar os índices antes de mutar), mas o
   custo permanece.
3. **Corrupção intermitente de string.** Num corpus de 35.358 arquivos — todos UTF-8 válido
   em disco, verificado byte a byte — 4 linhas armazenadas voltaram com UTF-8 inválido e a
   indexação falhou; a reexecução idêntica não reproduziu. Não foi reduzida a probe mínimo
   (`TestLadybugBulkInsertStringIntegrity` não a captura em 5 execuções).

O terceiro é o decisivo: perda silenciosa de integridade de dado, não determinística.

Comparação no corpus completo, mesma máquina, mesma amostragem:

| 35.358 arquivos | SQLite | Ladybug |
|---|---|---|
| FULL total | 4m13,6s | 4m39,6s |
| FULL write | 13,2s | 38,9s |
| incremental por edição | 288–331ms | 5,0s |

O ganho que justificava a migração era o índice vetorial atualizar in-place, eliminando o
rebuild de arquivo inteiro forçado pelo vazamento de espaço do vec0. Continua real, mas não
paga os três defeitos acima.

## O que ficou no repositório como evidência

Os probes do LadybugDB permanecem, porque documentam por que não migrar e servem de repro se
o upstream corrigir:

- `ladybug_fts_perrow_test.go` — **invertido de propósito**: passa enquanto o bug existe e
  falha quando for corrigido.
- `ladybug_fts_update_test.go`, `ladybug_fts_bulk_test.go`, `ladybug_fts_persist_test.go`,
  `ladybug_fts_param_test.go` — semântica de atualização do FTS, com a ressalva de que seus
  verdes descrevem a janela de visibilidade de 2–3 linhas.
- `ladybug_vector_test.go`, `ladybug_vector_schema_test.go` — o índice vetorial funciona e
  atualiza in-place; é o que valeria a pena reaproveitar.
- `ladybug_llm_test.go` — a extensão `llm` é cliente de provedor externo, não modelo embutido.
- `ladybug_fts_utf8_test.go`, `ladybug_bulk_string_integrity_test.go` — as tentativas de
  reduzir a corrupção a probe mínimo. Nenhuma reproduz; ficam registradas como o que já foi
  descartado (caracteres de controle, tamanho, UTF-8 inválido na origem).

Testes de requisito que sobrevivem à troca de motor, agora medidos no SQLite:
`TestSearchIndexQualityFloor` (piso 12/16), `TestTruncatedQueryCoverage` (piso 9/11),
`TestAbbreviationRecallByNameAlone`, `TestSearchOrderIsDeterministic`,
`TestSearchIndexIncremental` e `TestSearchIndexIncrementalRepeated`.

`TestExpansionFieldCeiling` mudou de conclusão com o motor e foi reescrito para registrar as
duas: no FTS5 as duas redações chegam a 9/9, porque o índice de prefixo casa `config` com
`configuration`; sem índice de prefixo, só a redação com o token exato chega lá. A asserção
agora falha se a paridade se perder, o que sinalizaria a perda do casamento por prefixo.

## Estado

Suíte completa verde com `-count=1`: `internal/ai` 86,5s, `internal/ast` 16,4s,
`internal/wiki`, `internal/fswatch`, `internal/daemon`, `internal/sysutil`.
`go build -tags fts5 ./...` e `go vet` limpos.

E2E no corpus real (3000 arquivos amostrados entre os 13 tipos de objeto):

```
FULL  parsed=2999 empty=287 errors=0 | parse=15,7s write=1,47s total=17,2s
INCR  6 rounds escopados, 28-40ms cada | INCR total=99ms
```

Contra o mesmo e2e sobre LadybugDB: write 4,7s e ~1,0s por round. Os requisitos seguem
verdes — `config` alcança `coreConf` e `CONF_MGR` pelo nome, piso de qualidade em 12/16,
truncamento em 9/11.
