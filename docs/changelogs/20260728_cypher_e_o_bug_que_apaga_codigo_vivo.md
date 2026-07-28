# Cypher deixa de ser nota de pé e a query de código morto para de mentir

**Data:** 2026-07-28
**Escopo:** `internal/ast/rule.go`, `internal/ast/rule_cypher_test.go`
**Origem:** o Engenheiro observou que o agente usava muito `ast_search` e quase nunca query Cypher

---

## Por que o agente não usava query

Não era falta de exemplos — o cookbook já tinha dezenas. Era o enquadramento, em duas linhas:

```
### Phase 2.3: Hybrid Search (RECOMMENDED — Best Results)

**This is the RECOMMENDED default for text-based discovery.**
```

Um agente lê *"recomendado"*, *"melhores resultados"*, *"default"* — e para ali. A Fase 3, onde a
pergunta de fato é respondida, vinha depois e sem nada dizendo que a saída da busca é **entrada**
dela.

O cabeçalho virou **"the best way to FIND NAMES, never the answer"**, com a regra e a razão:

> Sua saída é entrada da Fase 3, não resultado para reportar. Resultado de busca é chute ranqueado
> por semelhança textual. Ele não sabe o que chama o quê, o que quebraria, nem quão complexo algo
> é — nunca atravessou uma aresta.

E a Fase 3 ganhou título honesto: *"where the question gets answered"*.

## O que entrou antes das fases

Uma seção de abertura dizendo o que isso é: **LadybugDB, banco de grafo de propriedades, com
Cypher** — `MATCH`, caminhos de comprimento variável, agregação, `UNION`, `OPTIONAL MATCH`. Com
uma tabela contrastando as duas ferramentas na **mesma** pergunta:

| pergunta | `ast_search` devolve | uma query devolve |
|---|---|---|
| "Como funciona a autenticação?" | ~15 entidades cujo texto parece "autenticação" | a cadeia de chamadas do entry point até a checagem do token, cada salto nomeado |
| "Quem usa `saveUser`?" | onde a string aparece | todo caller, transitivamente, com arquivo e linha — e nada que só mencione o nome num comentário |
| "É seguro deletar?" | nada sobre segurança | a contagem exata de arestas de entrada, que **é** a resposta |
| "Qual é o código mais arriscado?" | nada — risco não é palavra no fonte | funções ordenadas por `cyclomatic_complexity`, e quais delas sem caller |

E as cinco famílias pedidas, cada uma com queries rodadas contra o grafo real antes de entrar:
relações entre entidades, find usage de verdade, refactoring (raio de impacto), complexidade e
risco, e entendimento de sistema que você nunca leu.

## O bug: cada callable existe DUAS vezes, e a query de código morto mente

```
MATCH (t:Function {name: 'Apply'}) RETURN t.name, t.path, t.line_number, t.is_stub
→ is_stub false | linha 53 | internal/textslice/textslice.go
→ is_stub true  | linha  0 | (path vazio)
```

`CONTAINS` liga o `File` à **declaração**. `CALLS` aponta para o **stub**, chaveado pelo nome nu.
São **nós diferentes**.

Consequência, verificada neste repositório:

```
MATCH (f:Function {name: 'Apply'}) WHERE NOT ()-[:CALLS]->(f) RETURN f.name, f.path
→ Apply | internal/textslice/textslice.go
```

`Apply` tem **13 callers** e a query de "código não usado" da própria skill o classifica como
morto — porque `NOT ()-[:CALLS]->(f)` é verdadeiro para **toda** declaração, sempre. Estava em três
lugares:

- *Finding unused code* (tabela de uso obrigatório)
- *Find orphan functions (dead code candidates)* (cookbook)
- *Safe-to-delete check* — `OPTIONAL MATCH (caller)-[:CALLS]->(f) ... count(caller)`, que conta
  zero numa função com cinquenta callers

**Um agente seguindo qualquer uma delas apaga código em uso.** É o achado mais perigoso de toda
esta revisão.

### A forma que funciona

`WITH collect()` funciona, então cabe numa query só — comparando pelo **nome**, nunca por
identidade de nó:

```
MATCH ()-[:CALLS]->(s:Function) WITH collect(DISTINCT s.name) AS called
MATCH (f:Function) WHERE f.is_stub = false AND NOT f.name IN called AND f.entry_point_score < 10
RETURN f.name, f.path, f.cyclomatic_complexity ORDER BY f.cyclomatic_complexity DESC
```

Verificado: `Apply`, `ReadPage`, `ListPages` e `firstHeading` — todas com callers — saem do
resultado. E `MATCH (caller)-[:CALLS]->(t:Function {name: 'firstHeading'}) RETURN count(caller)`
devolve 3, o número certo.

## O silêncio que se lê como "não há nada"

Da mesma causa vem uma segunda classe de falha: **misturar tipos de aresta em volta do mesmo nó
devolve zero linhas e nenhum erro.**

```
(caller)-[:CALLS]->(e)<-[:CONTAINS]-(f:File)    → 0 linhas
```

Não é limitação do engine — é que nenhum nó tem as duas arestas. Isolado:
`(caller)-[:CALLS]->(Apply)` → 13 linhas; `(Apply)<-[:CONTAINS]-(File)` → 1 linha; juntos → 0.

Duas queries pré-existentes caíam nisso e **sempre** devolviam vazio:

- *Find circular dependencies between files* — `IMPORTS` e `CONTAINS` alternados no mesmo caminho
- *Find parent interface usages* — `IMPLEMENTS` e `CALLS` em cláusulas unidas por `WHERE` cruzado

Corrigidas: a primeira virou uma pergunta que uma aresta só responde; a segunda virou duas queries,
explicitamente, com o motivo escrito. E *Move-file impact*, que usava `OPTIONAL MATCH` com `WHERE`
referenciando a cláusula anterior, virou dois passos com a lista de nomes no `IN`.

Documentado como as três consequências que se projeta query em volta, mais a regra prática:
**acrescente `f.is_stub = false` a qualquer query que devolva `path` ou `line_number`**, senão você
reporta resultado com path vazio e linha `0` como se fosse localização real.

## Uma minha, no mesmo padrão

A query que eu tinha escrito para "superfície pública de um módulo" filtrava por `e.is_exported` com
`e` sem label — e devolveu 51 linhas, **das quais 34 eram comentários**, porque `is_exported` também
vem `true` em nó `Comment`. Corrigida com `label(e) IN [...]`, e a armadilha documentada: quando o
filtro é sobre *declarações*, diga quais labels você quer.

Também trocada uma query de dependências que dependia de `m.is_dependency`, vazio neste grafo, por
uma que ordena por quantos arquivos importam cada módulo — e devolve resposta útil.

## Testes

`TestASTRuleContentHasNoAlwaysTrueDeadCodeQuery` distingue **prosa de exemplo executável**: as
formas quebradas continuam citadas no texto, como o que não se deve escrever, e o helper
`runnableQueries` só olha linhas que começam com `MATCH ` — as que um agente levanta direto para a
ferramenta. Meu primeiro teste era cego a essa diferença e reprovava o próprio aviso.

Mais `TestASTRuleContentExplainsTheStubDuality`,
`TestASTRuleContentFramesSearchAsGroundingNotAnswer` (que proíbe o retorno de *"RECOMMENDED default
for text-based discovery"*) e `TestASTRuleContentCoversTheQueryOnlyQuestions`, sobre as cinco
famílias.

`golangci-lint` limpo.

> **Nota:** outra sessão trabalha em ANTLR/Oracle nesta mesma árvore. Nada dela foi commitado nem
> revertido; este commit stageia apenas `internal/ast/rule.go`, o teste novo e este changelog.
