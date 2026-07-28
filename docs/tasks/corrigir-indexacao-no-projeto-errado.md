---
title: Indexação por MCP gravava no grafo do projeto errado
status: done
created: 2026-07-28
updated: 2026-07-28
tags: [ast, mcpstdio, paths, bug, indexacao]
---

# Indexação por MCP gravava no grafo do projeto errado

## Objective

Corrigir o bug reproduzido pelo Engenheiro em 2026-07-28: `graphit_ast_index(project_dir:
"/tmp/probe")` reportava sucesso, não criava banco algum em `/tmp/probe`, e colocava 16 nós de
sonda no grafo de `/home/lainosantos/projects/graphit-code`. Dois projetos abertos no mesmo servidor
MCP podiam contaminar o grafo um do outro, em silêncio.

Junto, e por ser parte da mesma correção: `GraphWriter.DeleteRepository` era um stub que retornava
`nil` sem apagar nada, o que impedia `ast_index(reindex: true)` de remover nós obsoletos.

Registrado antes em `docs/tasks/revisar-skills-e-mandates.md`, seção **O que sobrou**.

## Implementation Details

### A causa raiz é mais ampla que o site relatado

`brand.DotDir()` (`internal/brand/brand.go:22`) devolve `"." + Brand` — a string literal
`".graphit"`. Todo construtor de caminho dos módulos é, por consequência, **relativo à raiz do
projeto**: `ast.DefaultLadybugConfig()`, `knowledge.WikiDir()`, `memory.ProjectLinkDir(scope)`.

Os handlers MCP resolviam isso com `os.Chdir(projectDir)` + `defer os.Chdir(origWd)`, deixando o
caminho relativo escapar do bloco. `LadybugBackend` abre o banco de forma preguiçosa (`sync.Once`
em `connect()`, `internal/ast/ladybug.go:104`), então a resolução acontecia depois do `defer`,
contra o cwd do processo servidor.

**Descoberta que mudou a forma da correção:** os construtores são *puros em relação ao cwd* — só
concatenam constantes, nunca leem `os.Getwd`. O `chdir` nunca foi necessário para construí-los;
servia só para resolver o resultado relativo. Por isso a correção não foi `filepath.Abs` dentro do
`chdir` (como sugerido na reprodução), e sim ancorar explicitamente em `projectDir`, eliminando a
dependência de cwd — e com ela a janela de corrida entre requisições concorrentes.

### Sites corrigidos

Todos em `internal/mcpstdio`, via os dois helpers novos `anchorToProject` e `astConfigForProject`:

| Site | Defeito |
|---|---|
| `openASTDBReadWrite` (`context.go`) | o bug relatado |
| `openASTDB` (`context.go`) | idem; o `os.Stat` dentro do `chdir` mascarava na leitura |
| `ast_index(reset: true)` (`tools_ast.go`) | `os.RemoveAll` de caminho relativo **sem `chdir` nenhum** — apagava o banco AST do projeto do cwd |
| `CacheDir` do pipeline (`tools_ast.go`, `tools_lifecycle.go`) | cache de parse gravado noutro projeto |
| `resolveWikiDir` (`context.go`) | devolvia `.graphit/knowledge/project` para fora do `chdir` |

Em `resolveWikiDir` o `chdir` **permaneceu**: `memory.WikiDir` → `GlobalScopeDir` faz `os.Stat` para
decidir se o link do projeto existe, e essa sondagem precisa rodar em `projectDir`. Só o resultado
é ancorado antes de retornar.

`ast_embed` (`tools_ast.go:431`) faz todo o trabalho **dentro** do `withProjectDir` — já estava
correto, não foi tocado.

`LadybugConfigForContext` foi verificada como pedido: absoluta no caminho normal (via
`brand.GlobalDir()`), relativa apenas no fallback `GlobalDir() == ""`, que a âncora cobre.

### `DeleteRepository`

Escopo por `File.path`, que o pipeline grava relativo à raiz indexada. `w.rel(repoPath)` dá `"."`
para a raiz, o prefixo para um subdiretório, e `""` para fora da raiz (caso em que não apaga nada).

Três fases:
1. Para cada label ativo com coluna `path`: `MATCH (n:Label) WHERE n.path IN $paths DETACH DELETE n`.
2. Nós `File` e `Directory` sob o prefixo.
3. Varredura de órfãos para os labels **sem** coluna `path`.

A fase 3 existe por causa de um detalhe do DDL (`internal/ast/ladybug.go:208-214`): quando a
gramática não declara `Parameter`/`Field` como labels, eles recebem um esquema mínimo
(`uid, name, lang, is_stub`) **sem `path`**, e penduram no dono via `HAS_PARAMETER`/`HAS_FIELD`, não
no `File`. Não há como casá-los por arquivo.

O erro passou a subir em `tools_ast.go` — era descartado com `_ =`.

## Use Cases

### UC-01: Indexar um projeto a partir de um servidor MCP que está noutro diretório
- **Actor**: agente, via `graphit_ast_index`
- **Preconditions**: servidor MCP rodando com cwd diferente de `project_dir`
- **Main Flow**:
  1. `resolveProjectDir` transforma `project_dir` em absoluto
  2. `astConfigForProject(projectDir, "")` monta o config e ancora `DBPath` em `projectDir`
  3. `openASTDBReadWrite` devolve backend com caminho absoluto
  4. `ast.RunPipeline` grava; a abertura preguiçosa resolve no lugar certo
- **Alternative Flows**:
  - `context` preenchido → `LadybugConfigForContext`, normalmente já absoluto, passa direto
  - `LADYBUGDB_PATH` absoluto → respeitado sem alteração
- **Error Scenarios**:
  - `project_dir` inexistente → `resolveProjectDir` erra antes de qualquer abertura
- **Postconditions**: banco em `<project_dir>/.graphit/ast/project/ladybugdb`; nenhum outro projeto tocado
- **Affected Files**: `internal/mcpstdio/context.go`, `internal/mcpstdio/tools_ast.go`

### UC-02: Reindexar removendo nós obsoletos
- **Actor**: agente, via `graphit_ast_index(reindex: true)`
- **Preconditions**: grafo já povoado; `reset` falso
- **Main Flow**:
  1. `NewGraphWriter(db, absPath, true)`
  2. `DeleteRepository(ctx, absPath)` apaga o subgrafo sob `absPath`
  3. `RunPipeline` com `ForceRebuild: true` reinsere
- **Alternative Flows**:
  - `path` aponta subdiretório → só o prefixo é apagado
  - caminho fora da raiz → nada é apagado
- **Error Scenarios**:
  - falha ao apagar → erro **retorna** ao chamador; antes era engolido
- **Postconditions**: grafo sem entidade de arquivo deletado
- **Affected Files**: `internal/ast/writer.go`, `internal/mcpstdio/tools_ast.go`

### UC-03: Resetar o índice AST de um projeto
- **Actor**: agente, via `graphit_ast_index(reset: true)`
- **Preconditions**: nenhuma
- **Main Flow**: `os.RemoveAll` sobre o diretório do banco **do projeto pedido**
- **Error Scenarios**: antes desta correção, apagava o banco do projeto no cwd do servidor
- **Postconditions**: só o banco do projeto pedido desaparece
- **Affected Files**: `internal/mcpstdio/tools_ast.go`

## Test Cases & Acceptance Criteria

### Feature: resolução de caminho por projeto
Ref: UC-01, UC-03

#### Scenario: banco nasce no projeto pedido, não no cwd do servidor
```gherkin
Given o processo está no diretório "bystander"
  And "target" é um projeto vazio distinto
When openASTDBReadWrite("target") é chamado
  And uma escrita força a abertura preguiçosa do banco
Then o banco existe em "target/.graphit/ast/project/ladybugdb"
  And "bystander" continua sem diretório ".graphit"
```

#### Scenario: banco ausente é reportado para o projeto pedido
```gherkin
Given "bystander" já tem um banco AST
  And o processo está em "bystander"
When openASTDB("target") é chamado
Then a chamada falha
  And a mensagem nomeia o banco de "target"
```

#### Scenario: caminho absoluto passa direto
```gherkin
Given LADYBUGDB_PATH aponta para um caminho absoluto fora do projeto
When astConfigForProject é chamado
Then DBPath é exatamente esse caminho absoluto
```

#### Scenario: wiki de conhecimento é ancorado no projeto
```gherkin
Given o processo está em "bystander"
When resolveWikiDir("knowledge", "target", "") é chamado
Then o resultado é "target/.graphit/knowledge/project"
```

### Feature: DeleteRepository
Ref: UC-02

#### Scenario: apagar a raiz esvazia o grafo
```gherkin
Given um grafo com File, Function, Struct, Method, Parameter e Field
  And Parameter e Field não têm coluna "path"
When DeleteRepository é chamado com a raiz indexada
Then nenhum nó permanece em label algum
```

#### Scenario: apagar um subdiretório preserva o resto
```gherkin
Given um grafo com "sonda.go", "pedreira/veio.go" e "pedreira/xisto.go"
When DeleteRepository é chamado com o subdiretório "pedreira"
Then resta exatamente um nó de cada label
  And o Parameter de "sonda.go" sobrevive, porque seu dono sobreviveu
```

#### Scenario: caminho fora da raiz não apaga nada
```gherkin
Given um grafo povoado com raiz "root"
When DeleteRepository é chamado com um diretório fora de "root"
Then a contagem de cada label permanece inalterada
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/mcpstdio/context.go` | Modificado | `anchorToProject` e `astConfigForProject`; `openASTDB`/`openASTDBReadWrite` deixam de usar `chdir`; `resolveWikiDir` ancora o retorno |
| `internal/mcpstdio/tools_ast.go` | Modificado | `reset` e `CacheDir` usam o config ancorado; erro de `DeleteRepository` sobe |
| `internal/mcpstdio/tools_lifecycle.go` | Modificado | `CacheDir` do `sync` ancorado |
| `internal/ast/writer.go` | Modificado | `DeleteRepository` implementado; `pathsUnder` e `labelHasPath` novos |
| `internal/ast/ladybug.go` | Modificado | acessor `DBPath()` — o backend é preguiçoso, não havia como inspecionar o destino |
| `internal/mcpstdio/context_projectdir_test.go` | Criado | regressão dos caminhos por projeto |
| `internal/ast/writer_delete_repository_test.go` | Criado | regressão do stub, incluindo o caso sem `path` |
| `Makefile` | Modificado | `test` sem `-tags fts5` e engolindo o exit code; `ci` rodava `vet` antes de `ui` |

## Trade-offs & Decisions

**Ancorar em `projectDir` em vez de `filepath.Abs` dentro do `chdir`.** A sugestão da reprodução
funcionaria, mas mantém o cwd como intermediário. Como os construtores não leem o cwd, ancorar é
equivalente e elimina a corrida entre requisições concorrentes. Custo: `anchorToProject` precisa que
`projectDir` já seja absoluto — é, sempre vem de `resolveProjectDir`, e está documentado no helper.

**Não serializar `withProjectDir` com mutex.** Restam ~45 handlers que fazem `chdir` — inclusive
`ast_embed`, que segura o cwd durante todo o ciclo de embeddings. Um mutex global serializaria
handlers longos. A correção certa é remover o `chdir` desses caminhos como foi feito aqui, um
módulo por vez; ficou como débito.

**Semear o grafo à mão no teste de `DeleteRepository`.** A primeira versão rodava `RunPipeline` num
diretório temporário e indexava zero arquivos — `collectFiles` filtra por
`HasParserForExtensionIn`, e num diretório sem gramática instalada
`TreeSitterSupportedExtensions()` devolve vazio. Semear é hermético e permite montar exatamente o
caso difícil (`Parameter`/`Field` sem `path`), que um `.go` real talvez não produzisse.

**Nós `Directory` apagados por prefixo.** Poderiam ser deixados para o pipeline remesclar, mas uma
árvore removida deixaria as pastas de pé — o mesmo tipo de resíduo que a tarefa corrige.

## Technical Debt

- [ ] **`withProjectDir` continua mutando o cwd do processo** em ~45 handlers (`tools_memory.go`,
      `tools_knowledge.go`, `tools_hub.go`, `tools_cluster.go`, `tools_dream.go`). Duas requisições
      concorrentes podem interferir uma na outra. Correção: propagar `projectDir` explicitamente
      pelos construtores de caminho, como foi feito no módulo AST.
- [ ] **`ast_embed` segura o cwd durante todo o ciclo de embeddings**, que é longo. Enquanto isso,
      qualquer handler que dependa do cwd vê o diretório errado.
- [ ] **`LADYBUGDB_PATH` relativo mudou de semântica**: resolve contra `projectDir`, não contra o
      cwd. É a semântica coerente e a que faz o override funcionar por projeto, mas é mudança de
      comportamento. Absoluto — o uso esperado — não muda.
- [x] **`make ci` estava verde e mentindo** — resolvido nesta tarefa. Ver `## System Knowledge`.
- [ ] **Os 16 nós de sonda** do relato original continuam no grafo deste repositório até alguém
      rodar `ast_index(reset: true)` ou `reindex: true` — este último agora funciona.

## System Knowledge

- **`brand.DotDir()` é uma string literal relativa**, não um caminho resolvido. Todo caminho
  derivado dela é relativo à raiz do projeto e precisa ser ancorado por quem consome fora do
  projeto. É a origem de toda esta classe de bug.
- **`LadybugBackend` conecta preguiçosamente** (`sync.Once` em `connect()`). Um config errado não
  falha na construção — falha, silenciosamente e no lugar errado, na primeira query.
- **Nem todo nó carrega `path`.** `Parameter` e `Field` recebem DDL mínimo quando a gramática não os
  declara. Query que assume `n.path` em todo label falha com *Cannot find property path*.
- **O pipeline só indexa extensão com gramática instalada.** Num diretório temporário
  `TreeSitterSupportedExtensions()` devolve lista vazia e `RunPipeline` retorna `TotalFiles: 0` sem
  erro. Teste que dependa de indexação real precisa de gramática, ou de semear o grafo.
- **`File.path` é relativo à raiz passada ao pipeline**, não à raiz do projeto. Indexar um
  subdiretório grava caminhos relativos àquele subdiretório.
- **`internal/ui/dist` não é versionado** (`.gitignore:29`). Um `go build ./...` numa worktree nova
  falha em `internal/ui/embed.go` com *pattern dist/\*: no matching files found* até rodar
  `make ui`. Não é regressão.
- **O alvo `test` do Makefile rodava sem `-tags fts5`**, apesar de `BUILD_TAGS := fts5` existir na
  linha 39 e ser usado pelos alvos de build. Sem a tag, o SQLite não tem o módulo FTS5 e 30 testes
  de índice de busca falham com *no such module: fts5*. A tag é convenção documentada do projeto —
  o alvo simplesmente não a referenciava.
- **O alvo `test` engolia o código de saída do `go test`.** As invocações eram seguidas de `;` e de
  um bloco `if` de coverage, então o status da receita era o do último comando. Resultado:
  `make ci` imprimia *"✅ All CI checks passed"* com 30 testes falhando na tela e saía 0. Numa
  receita `make` encadeada por `\`, todo `go test` precisa de `|| status=1` e um `exit $$status`
  no fim — ou a falha desaparece.
- **A suíte inteira passa com `-tags fts5`**, nos dois conjuntos que o `make test` roda: pacotes de
  projeto (com `-race`) e pacotes de parser gerado. Ambos exit 0.

## Progress Log

### 2026-07-28
- Confirmada a causa raiz por leitura de `context.go`, `ladybug.go` e `config.go`.
- Levantados quatro sites adicionais do mesmo defeito, dois deles piores que o relatado
  (`reset` destrutivo, wiki de conhecimento).
- Corrigidos com `anchorToProject`/`astConfigForProject`; `chdir` removido do caminho de abertura.
- `DeleteRepository` implementado, com a varredura de órfãos para o caso sem `path`.
- Testes escritos e **verificados contra o código antigo**: falham lá, passam aqui. Também
  verificado que a varredura de órfãos é necessária — sem ela sobram `map[Field:1 Parameter:1]`.
- `go test -race -tags fts5 -p 4 ./internal/ast/... ./internal/mcpstdio/...` passa;
  `golangci-lint` sem issues nesses pacotes.
- `make ui` + `make ci` executados a pedido do Engenheiro.
