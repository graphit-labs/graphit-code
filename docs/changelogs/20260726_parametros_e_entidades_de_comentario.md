# Parâmetros deixam de sumir; comentários viram entidades buscáveis

**Data:** 2026-07-26
**Escopo:** `internal/ast/antlr/common/matcher.go`, `internal/ast/antlr_adapter.go`,
`internal/ast/queries/plsql.yaml`
**Origem:** pedido do Engenheiro após a explicação da perda de parâmetros

---

## 1. Parâmetros PL/SQL não chegavam ao cache — 35,4% das entidades

`ConvertToCache` descarta parâmetros e campos sem contexto, para não criar órfão no grafo. O
contexto vinha de `resolveParentContextAntlr`, que olhava **só o pai imediato** do match — e
para o padrão `//parameter/parameter_name` o pai é `parameter`, enquanto o corpo da função
está vários níveis acima. Todo parâmetro saía sem dono e caía no filtro.

**Correção:** o matcher passou a **carregar o contexto na descida**
(`Pattern.MatchWithContext`), com estado O(1) e sem alocação — `TreeNode` não tem ponteiro
para o pai, e um mapa de ancestrais custaria uma entrada por nó em arquivos de 700 KB. `Match`
continua existindo e delega com predicado nulo, então nenhum outro chamador muda.

| | antes | depois |
|---|---|---|
| `ACAO_PERMITIDA.sql` | 10 → 5 | **10 → 10** |
| amostra de 367 arquivos | 967 perdidas (35,4%) | **0** |
| parâmetros no cache | 0 | **967** |

Guarda: `TestOracleParametersReachTheCache`.

## 2. O nó fantasma `CREATE`

A correção acima expôs um defeito que já existia e estava escondido: o contexto saía como
`CREATE`, não `ACAO_PERMITIDA`. `FirstTerminalText()` devolve o primeiro token do nó de
contexto, e uma declaração começa por palavras-chave — `create_function_body` tem
`'CREATE' 'EDITIONABLE' 'FUNCTION'` antes de `function_name`.

Efeito: entidades atribuídas a um contexto chamado `CREATE`, que nó nenhum tem, deixando
`HAS_PARAMETER` apontando para um fantasma. A própria função já sofria disso
(`uid=…::CREATE.ACAO_PERMITIDA`).

**Correção:** `declarationName` procura o filho direto cuja regra termina em `_name`
(`function_name`, `procedure_name`, `tableview_name`, `package_name`) — a convenção desta
família de grammars — e cai de volta no primeiro terminal para grammars que não a sigam.
Também desempacota identificadores delimitados (`"GC"` → `GC`).

Agora todas as entidades do arquivo saem com `ctx=ACAO_PERMITIDA`.

## 3. Comentários viram entidades, com o texto como nome

`COMMENT ON ... IS '<texto>'` era extraído apenas como relação `REFERENCES` para a coluna
comentada, e **o texto nunca era capturado**. No export Oracle de referência isso é o
dicionário de dados inteiro jogado fora: **2209 arquivos** que contêm só `COMMENT ON`, e que
rendiam zero entidades.

Três queries em `plsql.yaml` passaram a `type: entity`, `graph_label: Comment`, com padrão
`//comment_on_column/quoted_string` (e as variantes `table` e `materialized`), de modo que o
**nome da entidade é o próprio texto** — que é o que alguém busca. A relação para o objeto
comentado foi mantida, então "o que está documentado" e "o que a documentação diz" continuam
ambos respondíveis.

```
Comment: "Indicador se Caixa e para Almoxarifado"
search almoxarifado -> hit
search indicador    -> hit
```

Efeito no censo por tipo de objeto:

| | antes | depois |
|---|---|---|
| tipo `comments` | 0 entidades, 12/12 vazios | **47 entidades, 0 vazios** |
| amostra total (151 arquivos) | 1282 entidades, 8% vazios | **1329 entidades, 0% vazios** |

Guarda: `TestOracleCommentsAreEntitiesAndSearchable`, que verifica a entidade, a ausência dos
delimitadores no nome e a busca por uma palavra que só existe dentro do comentário.

## Armadilha encontrada no caminho

**Os arquivos de query não são embutidos no binário.** São lidos de
`~/.graphit/runtime/<versão>/ast/queries/`, então editar `internal/ast/queries/plsql.yaml` no
repositório **não tem efeito nenhum** até ser copiado para lá. O `Makefile:268` copia para
`cmd/launcher/runtime/ast/queries/`, de onde o launcher extrai na instalação; para
desenvolvimento é preciso sincronizar `~/.graphit/runtime/dev/ast/queries/` à mão.

Isso custou uma rodada de depuração: a mudança estava correta e o resultado continuava zero.
Está registrado no comentário do teste para não custar de novo.

## Estado

Suíte completa verde com `-count=1`: `internal/ai` 119,6s, `internal/ast` 17,5s,
`internal/ast/antlr/...`, `internal/fswatch`, `internal/daemon`, `internal/wiki`,
`internal/sysutil`. `go build -tags fts5 ./...` limpo.

Nota de reindexação: os UIDs mudaram (o contexto deixou de ser `CREATE`), então o índice
existente precisa ser reconstruído.
