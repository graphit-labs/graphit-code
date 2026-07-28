# FTS do wiki ganha cobertura, e o daemon passa a vigiar o wiki de verdade

**Data:** 2026-07-27
**Escopo:** `internal/wiki/fts_db_test.go`, `internal/daemon/syncmodule.go`,
`internal/fswatch/fswatch.go`, `internal/ignorer/ignorer.go`, testes do daemon
**Origem:** dois pedidos do Engenheiro — resolver o FTS do wiki sem cobertura, e depois
"o daemon também precisa vigiar e fazer sync do wiki assim como ast"

---

## 1. FTS do wiki — de zero a 33 de 40 funções

`internal/wiki/fts.go` são 1494 linhas com toda a camada de armazenamento e recuperação do
wiki, e **nada abria um `WikiDB`**. Os testes com "search" no nome cobrem o laço de busca com
IA e helpers puros — trigramas, snippets, montagem de string de consulta — nenhum encosta no
SQLite.

A consequência mais afiada era a tag de build. O índice de chunks é uma tabela virtual FTS5,
então `go build` sem `-tags fts5` produz um binário cujo wiki falha ao abrir o banco. A suíte
passaria verde por cima disso.

Por isso os testes novos **falham em vez de pular** quando FTS5 não está disponível, com a
mensagem dizendo que o binário foi construído sem a tag. Pular restauraria exatamente o ponto
cego que eles existem para fechar.

Cobertos: abertura e criação do índice, reabertura preservando conteúdo, ida e volta de busca
(corpo, título, resumo, múltiplas palavras), acentuação — o corpus é português, e um
tokenizador que estrague diacríticos seria dano silencioso —, rebuild que substitui em vez de
acumular, `CheckAllHashesMatch` nos três casos que decidem se um rebuild acontece,
referências cruzadas em profundidade 1 e 2, os quatro filtros de `Browse`, ida e volta do log
de sync, e consultas hostis (`"`, `AND`, `NEAR(`, `"unbalanced`) que precisam devolver nada em
vez de erro.

Restam 7 funções sem cobertura, todas do caminho de embeddings — `SemanticSearch`,
`HybridSearch`, `PendingEmbeddings`, `InsertChunkVector`, `EmbeddingStats`,
`semanticSearchLocked` — mais `optimizeTables`. Precisam de vetores reais.

## 2. O daemon vigiava o wiki através do `.astignore`

Pergunta do Engenheiro que expôs isto: no código, `.wikiignore` e `.astignore` são cada um
para o seu?

**Para o que cada um indexa, sim.** `.astignore` é aplicado por `ast.NewAstIgnoreChecker`;
`.wikiignore` por `knowledge.NewKnowledgeIgnoreChecker`, dentro do pipeline do wiki
(`internal/knowledge/wiki.go:40`).

**Para o que dispara, não.** Não existe watcher do wiki. `fswatch.New` aparece em três lugares
— `syncmodule` (projeto), `memorysyncmodule` (memória) e `ast/watcher` (o `ast watch` da CLI) —
e o do projeto era montado com o checker do AST, sendo a única fonte de evento para os dois
consumidores.

Caso concreto: `docs/` no `.astignore` é configuração plausível, já que o AST parseia markdown
e você pode não querer isso. Com ela, o watcher nem registrava vigilância no diretório. Editar
um documento não gerava evento, o wiki nunca era avisado, e o `.wikiignore` seguia
perfeitamente correto sobre um pipeline que não rodava.

### Por que não um segundo watcher

Seria o desenho óbvio, e está errado aqui: `knowledge.docs_dir` é `"."` por padrão, então um
watcher próprio do wiki duplicaria a árvore inteira do projeto — dobrando o consumo de watches
de inotify, que o `fswatch` já trata como recurso escasso o bastante para ter mensagem de erro
dedicada.

### O que foi feito

Um watcher só, permissivo o bastante para os dois, com roteamento por checker:

- `fswatch.Config.Ignore` passou de `*ignorer.IgnoreChecker` para a interface `fswatch.Ignorer`
  (`IsIgnored` + `ShouldDescend`). O tipo concreto continua satisfazendo.
- `ignoreUnion` pula um caminho **só quando todos os membros pulam**, então a vigilância cobre
  a união do que os dois querem. Os dois excluem o diretório da marca por padrão, então a união
  continua excluindo e o daemon não acorda com as próprias escritas.
- `classifyBatch` recebe os dois checkers e cada consumidor aplica o seu ao que chega.

### A regressão que a interface causou, e o conserto

Trocar um ponteiro concreto por interface reabre a armadilha do **nil tipado**: um
`*ignorer.IgnoreChecker` nil guardado numa interface faz `Ignore != nil` passar, e o método é
chamado com receptor nil. `TestDetectsCreateAndModify` no `fswatch` estourou com SIGSEGV.

O conserto ficou em `ignorer`, não em `fswatch`: `IsIgnored` e `ShouldDescend` passaram a
tratar receptor nil como "não ignora nada". O chamador não tem como pegar isso — a checagem de
nil dele não enxerga nil tipado — então a guarda tem que morar no receptor.

## Testes

- `internal/wiki/fts_db_test.go` — dez testes, todos abrindo banco de verdade.
- `internal/daemon/syncmodule_wikiwatch_test.go` — docs excluído do AST ainda alcança o wiki
  (o caso quebrado), o espelho disso, caminho excluído pelos dois não alcança ninguém, e a
  tabela de decisão do `ignoreUnion`.

Os testes de roteamento são **herméticos**, usando a feature de queries de projeto registrarem
extensões que entrou no commit anterior: `stageProjectParsers` escreve arquivos de query no
`.graphit/ast/queries` do projeto temporário, então `.sql`, `.go` e `.md` são reconhecidos sem
runtime instalado. Antes eles pulariam.

Suíte completa com `-race` limpa, sem `~/.graphit` presente.
