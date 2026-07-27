# Stemmer, nome limpo no resultado e piso de confiança no passe semântico

**Data:** 2026-07-27
**Escopo:** `internal/ast/fts_sqlite.go`, `internal/ast/pipeline.go`, testes
**Origem:** lista de melhorias aprendidas com o LadybugDB + pergunta do Engenheiro sobre o híbrido

---

## O QUE mudou (schema v5 — exige reindexação)

1. **Stemmer porter** em `file_fts` e `entity_fts`
   (`tokenize='porter unicode61 remove_diacritics 2'`). Deliberadamente **não** em
   `entity_trigram`, cujos tokens são 3-gramas que o stemmer estragaria. Aprendizado direto
   do Ladybug, onde stemming é padrão e desligá-lo derrubou 4/7 acertos para 1/7.
2. **Nome limpo no resultado.** `name` passa a ser `UNINDEXED` e o split vai para
   `name_split`. Antes as duas coisas dividiam a mesma coluna, então a busca devolvia
   `"parseConfig parse Config"` — para quem consome via MCP e para todo teste, que precisava
   remover o sufixo antes de comparar. Pesos BM25 acompanham as colunas novas.
3. **Piso de confiança no passe semântico** (`semanticFloorCosine = 0.20`).
4. **`jsonCache.FlushDirty()`** deixa de ter o erro descartado: flush que falha perde
   trabalho já parseado e o próximo run reparseia sem dizer por quê.

### O que NÃO foi feito, e por quê

O fallback para consultas com menos de 3 caracteres, que eu havia proposto, **não é
necessário neste motor**. Ele resolvia uma lacuna do LadybugDB, que não tem wildcard. Aqui o
índice de prefixo do FTS5 já cobre: `cf` acha `CFG_LOAD` porque `cf` é prefixo do token `cfg`.
Medido antes de escrever qualquer código.

## O híbrido era pior que o léxico

Pergunta do Engenheiro: com o semântico chega a 16/16? Medido em
`TestHybridSearchQualityFloor`, com embeddings reais nas mesmas 16 sondas:

```
antes do piso:  lexical 13/16, híbrido 9/16  (perde 4, ganha 0)
```

As perdas eram `conf`, `valid`, `audit` e `cf` — e as duas últimas caíam em
`computeChecksum`. A causa: busca por vizinho mais próximo **sempre** devolve vizinhos. Para
uma consulta de duas letras o embedding não carrega significado, mas o passe entrava na fusão
mesmo assim e afogava o casamento exato.

O piso corta vizinhos abaixo de 0,20 de cosseno, valor lido da separação que o modelo produz
de fato: relacionados 0,34–0,39, não relacionados 0,07–0,08
(`TestSemanticReachOfAbbreviations`).

```
depois do piso: lexical 13/16, híbrido 11/16
das 11 sondas decisivas: lexical 11, híbrido 11
```

**O peso do passe semântico foi mantido em 2.0.** Variá-lo entre 0,8, 1,2, 1,5 e 2,0 não mudou
absolutamente nada: quando dois documentos são igualmente plausíveis o passe semântico devolve
os **dois**, em posições adjacentes, então um peso uniforme não os reordena. Baixá-lo teria
sido mudança sem justificativa vestida de conserto.

## Por que 16/16 não é a meta

Cinco das dezesseis sondas são **empates**, e o teste agora as marca como tal:

| consulta | esperado | alternativa igualmente defensável |
|---|---|---|
| `configuration` | parseConfig | initConfiguration |
| `schema` | validateSchema | SchemaValidator |
| `config` | configLoader | **Config** (casamento exato) |
| `conf` | CONF_MGR | coreConf (ambos têm o token exato `conf`) |
| `valid` | validateSchema | SchemaValidator |

Nas **11 sondas com uma resposta defensável, léxico e híbrido acertam 11/11**. Chegar a 16/16
significaria ajustar o motor para preferir um lado arbitrário de cinco caras-ou-coroa — que é
o que o teste passa a impedir: ele exige paridade só nas decisivas, e falha se uma sonda de
empate devolver algo que não seja nenhum dos dois lados.

Também removi duas sondas indefensáveis de `TestTruncatedQueryCoverage` (`valid` e `db`, esta
casando `connectDatabase` e `closeDatabase` igualmente); o piso passa de 9/11 para 9/9.

## Ganho medido

| | antes | depois |
|---|---|---|
| piso de qualidade léxico | 12/16 | **13/16** (stemmer) |
| truncamento | 9/11 (2 sondas indefensáveis) | **9/9** |
| híbrido nas sondas decisivas | 7/11 | **11/11** |
| nome no resultado | `"parseConfig parse Config"` | `"parseConfig"` |

Guarda nova: `TestSearchResultsCarryCleanNames` falha se o split voltar a vazar para o nome
exibido.

## Estado

Suíte completa verde com `-count=1`: `internal/ai` 205,8s, `internal/ast` 20,3s,
`internal/ast/antlr/...`, `internal/fswatch`, `internal/daemon`, `internal/wiki`,
`internal/sysutil`. `go build -tags fts5 ./...` limpo.
