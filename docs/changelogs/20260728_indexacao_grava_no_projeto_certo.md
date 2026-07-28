# Indexação grava no projeto certo, e `reindex` volta a apagar

**Data:** 2026-07-28
**Escopo:** `internal/mcpstdio/{context.go,tools_ast.go,tools_lifecycle.go}`,
`internal/ast/{writer.go,ladybug.go}`, mais dois arquivos de teste novos
**Origem:** reprodução do Engenheiro — indexar `/tmp/probe` por MCP colocou 16 nós de sonda no
grafo deste repositório

---

## O problema

`brand.DotDir()` é literalmente `".graphit"`. Todo construtor de caminho dos módulos devolve, por
consequência, um caminho **relativo à raiz do projeto**:

| Construtor | Devolve |
|---|---|
| `ast.DefaultLadybugConfig()` | `.graphit/ast/project/ladybugdb` |
| `knowledge.WikiDir()` | `.graphit/knowledge/project` |
| `memory.ProjectLinkDir(scope)` | `.graphit/memory/<scope>` |

Os handlers MCP resolviam isso com `os.Chdir(projectDir)` + `defer os.Chdir(origWd)` — e deixavam o
caminho relativo **escapar** do bloco. Como `LadybugBackend` abre o banco de forma preguiçosa
(`sync.Once` em `connect()`, só na primeira query), a resolução acontecia depois, contra o cwd do
processo do servidor MCP. Ou seja: contra outro projeto.

O sintoma é silencioso. A indexação reporta `totalfiles:1|parsedfiles:1|...` e sucesso; os nós vão
para o grafo errado.

O detalhe que escondeu o bug por tanto tempo: em `openASTDB` (leitura) o `os.Stat` roda **dentro**
do `chdir`, com o cwd certo. A verificação passava no lugar certo e a abertura acontecia no lugar
errado, o que fazia o problema aparecer só na escrita.

## A raiz, e por que o `chdir` nunca foi necessário

Os construtores são **puros em relação ao cwd** — só concatenam strings constantes, nunca leem
`os.Getwd`. O `chdir` não servia para construí-los; servia só para resolver o resultado relativo.

Então a correção não é "fazer `filepath.Abs` dentro do `chdir`": é **ancorar explicitamente em
`projectDir`** e não depender do cwd em momento nenhum.

```go
func anchorToProject(projectDir, path string) string {
	if path == "" || filepath.IsAbs(path) {
		return path
	}
	return filepath.Join(projectDir, path)
}
```

Caminho já absoluto passa direto — é o caso dos contextos importados (`~/.graphit/ast/<nome>`, via
`brand.GlobalDir()`) e do override `LADYBUGDB_PATH`. `LadybugConfigForContext` foi verificada: é
absoluta no caminho normal, e relativa apenas no *fallback* de `brand.GlobalDir() == ""`, que a
âncora agora cobre.

Como efeito colateral, `openASTDB`/`openASTDBReadWrite` deixam de mexer no cwd do processo. Sobrava
ali uma janela de corrida — cwd é estado global mutável e o servidor atende requisições
concorrentes — que simplesmente deixa de existir.

## Os outros pontos do mesmo defeito

A busca pelo padrão achou mais quatro, dois deles piores que o relatado:

- **`ast_index(reset: true)` apagava o projeto errado.** `os.RemoveAll(filepath.Dir(cfg.DBPath))`
  rodava **sem `chdir` nenhum**, com o caminho relativo — destruía o banco AST de quem estivesse no
  cwd do servidor.
- **`CacheDir` do pipeline** (em `ast_index` e no `sync`) apontava para o diretório de outro
  projeto, espalhando o cache de parse.
- **`resolveWikiDir`** devolvia `.graphit/knowledge/project` para fora do `chdir`: toda leitura do
  wiki de conhecimento por MCP resolvia contra o cwd do servidor. Aqui o `chdir` **permaneceu**,
  porque `memory.WikiDir` faz `os.Stat` para decidir se o diretório existe e essa sondagem precisa
  rodar em `projectDir`; só o resultado é ancorado antes de sair.

`ast_embed` faz todo o trabalho **dentro** do `withProjectDir`, então já estava correto e não foi
mexido.

## `DeleteRepository` deixa de ser um stub

`GraphWriter.DeleteRepository` retornava `nil` sem apagar nada. Era por isso que
`ast_index(reindex: true)` não removia nó obsoleto e entidade de arquivo deletado sobrevivia até
alguém rodar um `reset: true` inteiro.

O escopo é por `File.path`, que o pipeline grava relativo à raiz indexada: `repoPath` igual à raiz
cobre o grafo todo, um subdiretório cobre o próprio prefixo, e um caminho fora da raiz não apaga
nada.

O caso não óbvio é `Parameter` e `Field`. Quando a gramática não os declara como labels, eles
recebem um DDL mínimo — `uid, name, lang, is_stub`, **sem coluna `path`** — e penduram no dono, não
no `File`. Não dá para casá-los por arquivo. A varredura final pega o que ficou sem dono, depois
que os donos já foram apagados:

```cypher
MATCH (n:`Parameter`) OPTIONAL MATCH ()-[r]->(n) WITH n, count(r) AS owners WHERE owners = 0 DELETE n
```

O escopo continua valendo num delete parcial: parâmetro cujo dono sobreviveu mantém a aresta de
entrada. Há teste para exatamente isso.

Nós `Directory` também são removidos por prefixo — não estão na lista de arquivos, mas uma árvore
que foi embora não deveria deixar as pastas de pé.

E o erro agora **sobe**: em `tools_ast.go` o retorno era descartado com `_ =`. Uma limpeza que
falha em silêncio faz o reindex empilhar nós novos sobre os velhos, que é o defeito original.

## Testes

Ambos os arquivos foram verificados contra o código antigo — falham lá, passam aqui.

`internal/mcpstdio/context_projectdir_test.go` roda com o processo em um diretório e o
`project_dir` em outro. Contra o código antigo:

```
DBPath = ".graphit/ast/project/ladybugdb"; want "/tmp/.../001/.graphit/ast/project/ladybugdb"
openASTDB() succeeded; want a missing-database error for the target project
resolveWikiDir() = ".graphit/knowledge/project"; want "/tmp/.../001/.graphit/knowledge/project"
```

A segunda linha é o mascaramento em estado puro: `openASTDB` **teve sucesso** porque encontrou o
banco do projeto vizinho.

O teste de escrita não para no caminho: força a abertura preguiçosa com um `CREATE NODE TABLE` e
depois afirma que o banco nasceu no projeto pedido **e** que o projeto do cwd continua sem
`.graphit`.

`internal/ast/writer_delete_repository_test.go` semeia o grafo à mão em vez de indexar — o pipeline
só recolhe arquivo cuja gramática esteja instalada, e um diretório temporário não tem nenhuma
(`TreeSitterSupportedExtensions()` devolve vazio ali). Semear tem a vantagem de permitir montar o
esquema exatamente no caso difícil, com `Parameter`/`Field` sem `path`. Sem a varredura de órfãos:

```
DeleteRepository() left map[Field:1 Parameter:1] behind
```

## Verificação

```
go test -race -tags fts5 -p 4 ./internal/ast/... ./internal/mcpstdio/...   ok
golangci-lint run ./internal/ast/... ./internal/mcpstdio/...               0 issues
make ui && make build                                                      exit 0
```

## O `make ci` estava verde e mentindo

Ao rodar `make ci` a pedido do Engenheiro, apareceram **30 testes falhando** — e mesmo assim:

```
  ✅ All CI checks passed.
MAKE_CI_EXIT=0
```

Nenhuma das falhas é desta mudança: são todas de arquivos não tocados
(`search_index_test.go`, `abbrev_recall_test.go`, `fts_db_test.go`, …), todas com a mesma causa:

```
open search index: search schema migrate: ... no such module: fts5
```

Dois defeitos pré-existentes no `Makefile`, independentes um do outro:

1. **O alvo `test` não passava `-tags fts5`.** A variável `BUILD_TAGS := fts5` existe na linha 39 e
   é usada pelos alvos de build, mas o `test` nunca a referenciava. Sem a tag, o SQLite é compilado
   sem o módulo FTS5 e todo teste que abre o índice de busca falha. Comprovado no mesmo teste, mesmo
   código: falha sem a tag, `ok` com ela.
2. **O alvo `test` engolia o código de saída.** As chamadas de `go test` eram seguidas de `;` e mais
   comandos, então o status da receita era o do **último** comando (o `if` do coverage), nunca o do
   `go test`. É a razão de o `ci` imprimir sucesso com 30 falhas na tela.

Corrigidos: `-tags $(BUILD_TAGS)` nas duas invocações e propagação explícita via
`status=1` + `exit $$status`. Com a tag, a suíte inteira passa de verdade — pacotes de projeto e
pacotes de parser, os dois conjuntos que o `make test` roda, ambos exit 0.

Um terceiro ponto, menor: `ci: vet lint vulncheck test ui ui-lint` colocava `vet` **antes** de `ui`,
e o `vet` precisa de `internal/ui/dist` (não versionado, `.gitignore:29`). Numa worktree limpa o
`make ci` morria no primeiro alvo com `pattern dist/*: no matching files found`. Reordenado para
`ci: ui vet lint vulncheck test ui-lint`.

`make install` não foi executado: depende de `make build` (exit 0, verificado) e de um `cp` para
`/usr/local/bin`, que não é gravável pelo usuário e exigiria `sudo`.

## Débito que fica

Um `LADYBUGDB_PATH` **relativo** passa a ser resolvido contra `projectDir` em vez do cwd. É a
semântica coerente com o resto, e a que faz o override funcionar por projeto — mas é uma mudança de
comportamento para quem dependesse do cwd. Caminho absoluto, que é o uso esperado, não muda.
