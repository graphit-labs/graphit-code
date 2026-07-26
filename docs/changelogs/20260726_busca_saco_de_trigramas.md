# Busca: saco de trigramas no lugar de frase de trigramas

**Data:** 2026-07-26
**Escopo:** `internal/ast/fts_sqlite.go` (+ testes)
**Origem:** exigência do Engenheiro — "deve ser possível pesquisar por `config` e encontrar por exemplo `coreConf`"

---

## O QUE mudou

1. **`entity_trigram` deixou de usar `tokenize='trigram'`.** A tabela passou a ter uma
   coluna `name_tri` com os trigramas **pré-computados em tempo de escrita**, indexada
   com tokenizer de palavras (`unicode61 remove_diacritics 2`). O nome original passou
   a ser coluna `UNINDEXED` (só para devolver no resultado).
2. **`queryTrigram` reduz a consulta ao mesmo saco de trigramas** e faz `OR` entre os
   termos, em vez de casar a consulta como frase.
3. **`ORDER BY rank` foi adicionado** ao passe de trigrama (não existia).
4. **`ftsSchemaVersion` 3 → 4** — a migração recria as tabelas FTS, portanto **exige
   reindexação** para o novo campo ser populado.
5. Novos helpers `normalizeForTrigrams` e `identifierTrigrams` em código de produção.

## POR QUE

`tokenize='trigram'` do FTS5 casa os trigramas da consulta como **frase ordenada**, o
que é contenção de substring: a consulta tem de ocorrer dentro do documento. Logo
`conf` alcança `coreConf`, mas `config` **não** — `coreConf` não contém os trigramas
`nfi`/`fig`.

Medido em `TestAbbreviatedIdentifierSearchSQLite`, buscando **só por nome** (sem ajuda
da prosa):

| probe | antes | depois |
|---|---|---|
| `config` → coreConf, CONF_MGR, configLoader, initConfiguration | 2/4 | **4/4** |
| `conf` → idem | 4/4 | 4/4 |
| `config` → CFG_LOAD | 0/1 | 0/1 |
| **total** | **6/9** | **8/9** |

Pontuar um **saco** de trigramas com BM25 mantém a sobreposição parcial ranqueável, que
é exactamente o que o caso abreviado exige. `CFG_LOAD` continua inalcançável por não
compartilhar trigrama com `config` — isso é caso de busca semântica ou alias, não de FTS.

O `ORDER BY rank` é correção independente: o RRF pontua por **posição** no ranking, então
sem ordenação o passe de trigrama injetava posições arbitrárias na fusão.

## COMO foi verificado

- `TestAbbreviationRecallByNameAlone` — a exigência, escrita antes da implementação
  (vermelho: `config` não alcançava `coreConf`/`CONF_MGR`; verde depois).
- `TestAbbreviatedIdentifierSearchSQLite` — mede as duas direções de casamento parcial
  em dois corpora. A variante "só nomes" existe para remover um **confundimento
  encontrado durante o trabalho**: as docstrings do corpus continham "configuration", e
  o índice de prefixo do FTS5 casava por ali, não pelo identificador. Com prosa o
  número era 8/9 tanto antes quanto depois, escondendo a regra em teste.
- `TestTrigramNoiseDoesNotDisplaceExactMatches` — invariante de precisão: ruído abaixo
  de um acerto verdadeiro é custo aceitável; ruído que o desloca é regressão. 6/6 probes
  mantêm o casamento exato em top-1.
- `TestTrigramBagSearchLatency` — 200k entidades sintéticas com vocabulário pequeno (caso
  adversário para `OR`). A/B na mesma máquina:

  | consulta | antes (frase) | depois (saco) |
  |---|---|---|
  | config | 32ms | 90ms |
  | conf | 50ms | 100ms |
  | checksum | 34ms | 75ms |
  | ENTRG | 32ms | 50ms |
  | EXTRAIR_DOC | 80ms | 116ms |
  | validaSchema | 70ms | 146ms |

  Pior caso confirmado em 3 execuções: 153ms / 159ms / 162ms (variância 1,06×). O lado
  "antes" é execução única — tratar o fator ~2× com margem, embora a direção seja
  consistente nas 6 consultas.
- `TestAbbreviatedIdentifierSearch` (novo, Ladybug) — mede o mesmo corpus em FTS cru,
  split e trigrama no Ladybug: 1/9, 3/9 e 8/9. É a evidência de que o ganho vem da
  **representação** (saco vs frase), não do motor.
- Suítes `internal/ast`, `internal/fswatch`, `internal/daemon`, `internal/sysutil`
  verdes; `go build ./...` e `go vet` limpos.

## Custo aceito, explicitamente

- **Precisão:** nomes sem relação que compartilham um trigrama comum entram no conjunto
  (`checksum` e `validateSchema` compartilham `che`). Fica **abaixo** dos acertos reais
  porque o passe tem peso 0,7 no RRF contra 1,5–3,0 dos passes exatos — invariante
  guardada por teste.
- **Latência:** ~2× no pior caso, 155ms em 200k entidades. Aceitável para chamada de
  ferramenta MCP; o teto do teste é 1s, para pegar blowup, não para servir de meta.
- **Reindexação obrigatória:** o bump de schema esvazia as tabelas FTS existentes. O
  índice em `~/.graphit/ast/project/ladybugdb.search.sqlite` precisa ser reconstruído.

## Consequência para a migração SQLite → LadybugDB

O ganho medido vem da representação dos trigramas, reproduzível em qualquer um dos dois
motores — **o argumento de qualidade de busca para migrar deixou de existir**. Resta o
argumento de redução de dependências (`go-sqlite3`, `sqlite-vec`, tag de build `fts5`),
que é real mas não tem urgência medida: nenhuma medição desta sessão apontou a busca
como lenta ou incorreta. Decisão de seguir ou parar a migração fica com o Engenheiro.

Correção documental relacionada: o comentário de `TestOracleIdentifierSearch` afirmava
"consolidar em FTS do Ladybug sozinho" com base em 6/6 = 6/6. Aquela medição só cobria
consultas de **token inteiro**; o comentário foi corrigido para dizer isso e apontar
para a nova medição, em vez de deixar a conclusão errada no repositório.
