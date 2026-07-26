# Vetor nativo do LadybugDB medido: o ganho real da consolidação

**Data:** 2026-07-26
**Escopo:** `internal/ast/ladybug_vector_test.go` (novo), `internal/ast/abbrev_semantic_test.go` (correção)
**Origem:** questionamento do Engenheiro — se o semântico cobre os casos difíceis, a consolidação
no LadybugDB vale a pena, e ele suporta vetor nativo

---

## O QUE foi medido

`TestLadybugVectorIndex` prova a metade que nenhuma medição anterior tocou:

| probe | resultado |
|---|---|
| extensão `vector` carrega | ✅ |
| coluna `FLOAT[768]` (largura de `ai.EmbeddingDimensions`) | ✅ |
| `[]float32` liga como parâmetro de query | ✅ (sem literal de 768 elementos) |
| ranking de vizinho mais próximo | ✅ `loadUserConfig` em 1º |
| **DELETE refletido sem rebuild** | ✅ sai do índice |
| **INSERT refletido sem rebuild** | ✅ vetor novo passa a ser o 1º |

## POR QUE isso muda a decisão

O `sqlite-vec` (vec0) aloca blocos fixos de 1024 linhas e **nunca recupera espaço em
delete** — apenas marca um bit de validade. É exatamente por isso que `RebuildFromCache`
não pode atualizar no lugar e precisa escrever arquivo novo e renomear (comentário em
`fts_sqlite.go:140-147`). O índice vetorial do Ladybug atualiza in-place nos dois
sentidos, então esse workaround inteiro desaparece.

Segundo efeito, não previsto: **a mudança de trigrama desta mesma data tornou a migração
mais barata.** O único recurso do FTS5 sem equivalente no Ladybug era o tokenizer
`trigram` nativo. Com o saco de trigramas pré-computado, a dependência passou a ser
tokenizer de palavras sobre um campo comum — que o Ladybug tem
(`TestLadybugIndexedSubstring`). A paridade de FTS caiu para tokenizer de palavras +
BM25 + peso por campo, tudo já provado.

## Revisão de posição registrada

O changelog `20260726_busca_saco_de_trigramas.md` concluiu que "o argumento de qualidade
de busca para migrar deixou de existir". Isso continua correto **para a metade FTS** e
está incompleto como conclusão geral: a metade vetorial não havia sido medida, e é onde
está o ganho. A recomendação de parar a migração fica revista.

## Lacuna que sobra

O índice de prefixo do FTS5 (`prefix='2 3 4'`, que alimenta o passe `token*` em
`buildPrefixQuery`) **não tem equivalente no Ladybug** — o probe `conf*` retorna vazio.
A hipótese é que o saco de trigramas o torne redundante (`config*` alcançando
`configuration` também casa por sobreposição de trigramas), mas isso é **hipótese, não
medição**. Precisa ser medido antes de qualquer remoção do SQLite.

## Correção de asserção própria

`TestSemanticReachOfAbbreviations` exigia `CFG_LOAD` na "metade de cima" do ranking. Isso
estava errado com os dados já à vista: `CFG_LOAD` é 1º/7 para a consulta `config` mas
4º/7 para `configuration`, ou seja, a asserção codificava a sorte de uma redação como
requisito. Além disso era redundante — `worstRelated > bestUnrelated` já cobria o
essencial. Substituída pela propriedade que importa: `CFG_LOAD` precisa superar todo
identificador irrelevante (margem medida ~4×), sem exigir posição fixa.

## Estado de verificação

Fecha a pendência do changelog anterior. Verde: `internal/ai`, `internal/ast`,
`internal/fswatch`, `internal/daemon`, `internal/sysutil`. `go build -tags fts5 ./...` e
`go vet` limpos, com ORT 1.26.0.
