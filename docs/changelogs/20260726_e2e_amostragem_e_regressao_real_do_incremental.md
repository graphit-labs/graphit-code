# E2E: amostragem do corpus corrigida e a regressão real do incremental

**Data:** 2026-07-26
**Escopo:** `internal/ast/e2e_bench_test.go`, `internal/ast/oracle_extraction_census_test.go`,
`internal/ast/oracle_corpus_extraction_test.go`,
`internal/ast/oracle_pipeline_extraction_test.go`
**Origem:** validar no corpus real de 35k arquivos a busca que acabou de ser substituída

---

## O problema: o e2e limitado media extração vazia

Rodar `TestE2EIndex` com `GRAPHIT_E2E_MAX_FILES=800` reportava:

```
FULL  files=799 parsed=799 empty=799 errors=0 | parse=452ms
```

**Todos os 799 arquivos sem uma única entidade.** O índice de busca ficava vazio, então
qualquer tempo medido por ali — rebuild, incremental, latência de consulta — descrevia um
índice vazio, não o corpus.

E `parse=452ms` para 799 arquivos (0,57ms cada) já denunciava: PL/SQL via ANTLR custa ~70ms
por arquivo. O parse real não estava acontecendo.

### Causa: prefixo de walk, não bug

Quatro hipóteses foram testadas e descartadas antes de achar a certa:

1. *"a grammar PL/SQL não está resolvendo"* — `TestOracleCorpusExtraction` mostra
   **73 entidades em 6 arquivos** via `antlr-plsql` (tree-sitter-sql extrai 0, esperado: é
   SQL genérico contra DDL Oracle).
2. *"o pipeline não chama o ANTLR"* — `TestOraclePipelineExtraction` mostra o pipeline
   armazenando entidades normalmente.
3. *"é a estrutura de diretórios ou o `.astignore` copiado"* — bisseccionado: nenhum dos dois
   importa.
4. *"é o número de arquivos"* — também não.

O que importa é **quais** arquivos. O corpus tem um diretório por tipo de objeto,
`filepath.WalkDir` é lexical, e o primeiro tipo é `comments/` — arquivos com `COMMENT ON`,
que não têm entidade nomeada para extrair. Copiar um **prefixo** de 800 arquivos pegava só
`comments/`.

`TestOracleExtractionCensus` mede por tipo (12 arquivos de cada):

| tipo | entidades | vazios | mediana/arquivo |
|---|---|---|---|
| comments | **0** | 12/12 | 0 |
| constraints | 42 | 0/12 | 3 |
| functions | 122 | 0/12 | 9 |
| indexes | 12 | 0/12 | 1 |
| mviews | 12 | 0/12 | 1 |
| packages | **776** | 0/12 | 58 |
| procedures | 134 | 0/12 | 10 |
| sequences | 12 | 0/12 | 1 |
| synonyms | 12 | 0/12 | 1 |
| tables | 102 | 0/12 | 7 |
| triggers | 39 | 0/12 | 4 |
| types | 7 | 0/7 | 1 |
| views | 12 | 0/12 | 1 |

**1282 entidades em 151 arquivos, 8% vazios.** Extração funciona em 12 de 13 tipos;
`comments` render zero é correto, não defeito.

### Correção

`TestE2EIndex` passou a amostrar **round-robin entre os diretórios de topo** em vez de pegar
prefixo. Com isso, 800 arquivos passam a dar `empty=67` (8%, batendo com o censo) e
`parse=6,3s` em vez de 452ms.

Consequência para o histórico: **toda medição anterior feita com `GRAPHIT_E2E_MAX_FILES`
descreve um índice vazio** e não deve ser citada. Isso inclui o baseline de "~330ms" para o
incremental que eu mesmo usei como referência no changelog anterior.

## A regressão real do incremental

Com a amostragem corrigida, comparação apples-to-apples no mesmo subconjunto de 800 arquivos
e na mesma máquina, usando um worktree do commit anterior à migração:

| | SQLite (antes) | Ladybug (depois) |
|---|---|---|
| FULL total | 7,24s | 8,58s |
| FULL write | 706ms | 2,24s |
| incremental escopado, por round | **9–11ms** | **615–875ms** |
| INCR total | **89ms** | **979ms** |

O incremental regride ~11× no total e ~85× no write escopado. A causa é conhecida e já
registrada: o índice FTS do liblbug 0.18.2 não é mantido em insert, obrigando `DROP` +
`CREATE` dos sete índices após cada escrita — trabalho O(corpus) para mudança O(1). Por isso
o custo **cresce com o corpus**: 979ms em 800 arquivos, 5,3s em 200k entidades sintéticas.

Correção de registro: o changelog anterior citou "~330ms → 5,3s (16×)". A direção estava
certa, o baseline não — ele vinha do prefixo vazio. Os números acima são os válidos.

## Estado

Medição no corpus completo (35.358 arquivos) em andamento no momento da escrita.
`go vet` limpo.
