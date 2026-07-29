# `make ci` volta a passar, e `make install` finalmente foi executado

**Data:** 2026-07-28
**Escopo:** `internal/ast/writer_delete_repository_test.go`, `Makefile`
**Origem:** pedido do Engenheiro — os dois alvos precisam estar funcionais

---

## O que estava quebrado

Rodando um alvo de `ci: ui vet lint vulncheck test ui-lint` por vez, só um falhava:

| alvo | antes |
|---|---|
| `ui` | ✓ npm ci + vite, 7 s |
| `vet` | ✓ |
| `lint` | ✓ golangci-lint 2.12.2, 0 issues |
| `vulncheck` | ✓ 0 vulnerabilidades alcançáveis pelo código |
| `test` | ✗ `internal/ast` |
| `ui-lint` | ✓ 26 warnings, 0 errors — eslint sai 0 |

E a falha não era de asserção, era de **seed**:

```
--- FAIL: TestDeleteRepositoryEmptiesTheGraph (0.04s)
    seed "MATCH (n:Function {uid: 'fnA'}), (p:Parameter {uid: 'paA'}) CREATE (n)-[:HAS_PARAMETER]->(p)":
    ladybug query: Binder exception: Table HAS_PARAMETER does not exist.
```

Os três testes de `writer_delete_repository_test.go` morriam na mesma linha — a rel table
`HAS_PARAMETER` não existia no banco que o próprio teste acabara de criar.

## A causa: o fallback que `763fe938` removeu de propósito

`763fe938` ("fix: AST indexing schema error") reescreveu `initSchemaForLabels`
(`internal/ast/ladybug.go:275-285`). A versão antiga derivava os donos de parâmetro de
`CallerLabels` e, quando essa lista vinha vazia, chutava um literal:

```go
if len(paramRels) == 0 {
    paramRels = append(paramRels, "FROM `Function` TO `Parameter`")
}
```

Esse chute **era o bug**. O comentário em `rebuild_index.go:146-150` diz por quê: ele
declarava uma ponta apontando para uma node table que aquele corpus nunca criava, e um DDL
rejeitado aborta o rebuild inteiro. Agora o grupo sai de `info.ParamOwnerLabels` e de nada
mais — se a lista está vazia, o grupo não é criado.

`sondaSchema`, o `SchemaInfo` montado à mão pelo teste, declarava `HasParams: true` **sem**
`ParamOwnerLabels`. Sob a semântica nova isso quer dizer "nenhuma aresta HAS_PARAMETER", e o
seed cria exatamente uma.

Quem estava desatualizado era o teste. A produção está correta: `rebuild_index.go:151-163`
popula `paramOwnerSet` a partir do dono real de cada parâmetro — do `FuncUID` do parâmetro e
do `ParentLabel` das arestas CONTAINS — e filtra por `labelSet` antes de repassar. Reintroduzir
o fallback faria o schema voltar a declarar a ponta fantasma, que é o defeito que derrubou o
rebuild no corpus Oracle de 35 358 arquivos.

A correção é declarar o dono, como `FieldOwnerLabels` já fazia para `HAS_FIELD`:

```go
ParamOwnerLabels: []string{"Function"},
```

A regra que sobra para qualquer teste futuro que construa `SchemaInfo` direto: `HasParams: true`
sozinho não cria nada, e `HasFields: true` sozinho também não. Os dois grupos saem só das
listas de donos, filtradas por `nodeTables`.

## O segundo defeito: `node_modules` entrando no `go list`

Não derrubava o CI. Fazia o CI local e o remoto cobrirem conjuntos de pacotes diferentes, que
é pior, porque a divergência só aparece na máquina do dev.

`make ui` roda `npm ci`, e uma das dependências transitivas da UI carrega fontes Go. Depois de
um build de UI:

```
$ go list ./... | grep node_modules
github.com/graphit-labs/graphit-code/internal/ui/node_modules/flatted/golang/pkg/flatted
```

E `ci` roda `ui` **antes** de `vet`/`lint`/`test` — ordem que veio da correção anterior, porque
o `vet` precisa de `internal/ui/dist`. Consequência: os três alvos passavam a cobrir código de
terceiro. Nos jobs do GitHub isso nunca acontece; eles criam um placeholder em
`internal/ui/dist` e o job de teste não roda `npm ci`.

O `.golangci.yml` já tinha tomado essa decisão — `node_modules` aparece em
`exclusions.paths` **e** numa regra por linter. Ou seja, o lint estava consistente e as
ferramentas do go não. Os filtros viraram duas variáveis no topo do `Makefile`, usadas nos três
lugares que antes repetiam a cadeia de greps:

```make
GO_PKGS_SKIP    := /antlr/|/treesitter/|/node_modules/
GO_PKGS_PARSERS := /antlr/|/treesitter/
```

O passe dos parsers gerados filtra `node_modules` também, porque nada garante que um pacote npm
não tenha `/treesitter/` no caminho. Contagem depois da mudança: **38** pacotes no passe com
race, **21** no dos parsers, nenhum de `node_modules`, com ou sem `npm ci` tendo rodado.

## `make install`, que o changelog anterior deixou sem executar

O changelog de `20260728_indexacao_grava_no_projeto_certo.md` termina com: *"`make install` não
foi executado: depende de `make build` (exit 0, verificado) e de um `cp` para `/usr/local/bin`,
que não é gravável pelo usuário e exigiria `sudo`."* Agora foi.

O `PREFIX` default continua exigindo senha, então a verificação foi pelo branch gravável — o
mesmo `cp`, sem o privilégio:

```
make install PREFIX=/tmp/graphit-install-test
  ✓ Installed to /tmp/graphit-install-test/graphit          (517 MB)
```

E o binário instalado foi executado com `HOME` falso, não com o real. Isso não é preciosismo: o
launcher resolve seu `appDir` por `os.UserHomeDir()` (`cmd/launcher/main.go:23-31`), e a versão
buildada é a mesma `v0.1.27` que está instalada — rodá-lo com o `HOME` real reescreveria
`~/.graphit/runtime/v0.1.27` e o `launcher.stamp`, com `cleanupOldRuntimes()` apagando os
diretórios das outras versões, tudo isso com o daemon rodando de dentro desse diretório.

```
HOME=/tmp/gt-fakehome /tmp/graphit-install-test/graphit --version
graphit version v0.1.27
```

O runtime extraiu completo — `graphit-core`, `graphit-mcp`, `liblbug.so`, `libonnxruntime.so`,
`models/`, `ast/` — e o core respondeu. O caminho build → cópia → extração → exec do core está
funcional de ponta a ponta.

## Verificação

```
make ci
  ✅ All CI checks passed.
```

Nenhuma linha `FAIL` no log — o que aqui não é uma formalidade: o alvo `test` só passou a
propagar o exit code na correção anterior, e antes dela imprimia sucesso com 30 falhas na tela.
`coverage.out` é gerado (32 MB, é o arquivo que o job do GitHub converte para Cobertura).

```
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go test -race -tags fts5 -run TestDeleteRepository -count=1 ./internal/ast/
  ok  (1.3 s, 3/3 PASS)
```

## Débito que fica

Nada disso quebra os dois alvos hoje; todos foram encontrados olhando o build e ficam
registrados em `docs/tasks/make-ci-e-make-install-funcionais.md`.

- **`BUILD_ID` é recalculado a cada expansão.** `BUILD_ID ?= $(shell …)` cria variável
  recursiva, então o `$(shell)` roda de novo em cada uso: os três `go build` de `build-linux`
  recebem UUIDs diferentes, visível no log. Hoje é inofensivo — `version.BuildID` só é lido
  pelo próprio launcher, comparando o stamp com o valor compilado dentro dele
  (`cmd/launcher/main.go:205-231`), e nada cruza o BuildID do core com o do launcher. Corrigir
  é `BUILD_ID := $(BUILD_ID)` depois do `?=`, preservando override por env.
- **Zig sobrando no CI.** `.github/workflows/ci.yml` baixa e instala Zig 0.16.0 no job
  `build-check`, mas nenhum `CC`/`CXX` de `build-linux` aponta para ele. Resíduo; remover
  encurta o job.
- **ICU empacotada em excesso.** `build-linux` copia todo `libicu*.so.[0-9]*` que acha em
  `/usr/lib` e `/lib`: nesta máquina entram os sonames **74 e 78**, mais `libicutest` e
  `libicutu`, que o binário não usa. Peso morto dentro dos 517 MB.
- **`install` faz `mkdir -p $(PREFIX)` sem `sudo`** antes de decidir se precisa de `sudo` no
  `cp`. Com `PREFIX` inexistente sob caminho de root, o make aborta ali, antes de chegar ao
  branch com privilégio. Não afeta o default, que já existe.
