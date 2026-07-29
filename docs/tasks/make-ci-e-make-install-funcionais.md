# Tarefa: deixar `make ci` e `make install` funcionais

**Data:** 2026-07-28
**Status:** concluída

## Objetivo

Os dois alvos precisavam voltar a passar de ponta a ponta. `make ci` falhava; `make install`
nunca tinha sido verificado nesta máquina. Este log registra o que estava quebrado, o que foi
corrigido e o que ficou de fora de propósito.

## O diagnóstico

`make ci` é `ui vet lint vulncheck test ui-lint`. Rodando um alvo por vez:

| alvo | resultado antes |
|---|---|
| `ui` | ✓ (npm ci + vite, 7 s) |
| `vet` | ✓ |
| `lint` | ✓ — golangci-lint 2.12.2, 0 issues |
| `vulncheck` | ✓ — 0 vulnerabilidades chamadas pelo código |
| `test` | ✗ — `internal/ast` falhava |
| `ui-lint` | ✓ — 26 warnings, 0 errors (eslint sai 0) |

Ou seja: um único ponto de falha, dentro de `test`.

## Causa raiz do `make test`

Os três testes de `internal/ast/writer_delete_repository_test.go` morriam no seed, não na
asserção:

```
--- FAIL: TestDeleteRepositoryEmptiesTheGraph (0.04s)
    seed "MATCH (n:Function {uid: 'fnA'}), (p:Parameter {uid: 'paA'}) CREATE (n)-[:HAS_PARAMETER]->(p)":
    ladybug query: Binder exception: Table HAS_PARAMETER does not exist.
```

O commit anterior, `763fe938` ("fix: AST indexing schema error"), reescreveu
`initSchemaForLabels` (`internal/ast/ladybug.go:275-285`). Antes, o grupo HAS_PARAMETER era
derivado de `CallerLabels` e, quando essa lista vinha vazia, caía num literal
`FROM Function TO Parameter`. Esse fallback foi **removido de propósito** — o comentário em
`rebuild_index.go:146-150` explica por quê: ele inventava uma ponta de rel table apontando
para uma node table que aquele corpus nunca criava. O grupo agora sai de
`info.ParamOwnerLabels` e de nada mais.

`sondaSchema`, o `SchemaInfo` montado à mão pelo teste, declarava `HasParams: true` sem
`ParamOwnerLabels`. Sob a semântica nova isso significa "nenhuma aresta HAS_PARAMETER", e o
seed cria exatamente uma. O teste ficou desatualizado; a produção está correta —
`rebuild_index.go:151-163` popula `paramOwnerSet` a partir do dono real de cada parâmetro e
filtra por `labelSet` antes de passar adiante.

Correção: declarar o dono, como `FieldOwnerLabels` já fazia para HAS_FIELD.

## O segundo problema: node_modules dentro do `go list`

Não derrubava o CI, mas fazia o CI local e o remoto cobrirem conjuntos de pacotes diferentes.

`make ui` roda `npm ci`, e uma das dependências transitivas da UI carrega fontes Go. Depois
de um build de UI:

```
$ go list ./... | grep node_modules
github.com/graphit-labs/graphit-code/internal/ui/node_modules/flatted/golang/pkg/flatted
```

Como `ci` roda `ui` **antes** de `vet`/`lint`/`test`, os três passavam a cobrir código de
terceiro. Nos jobs do GitHub isso nunca acontece: eles só criam um placeholder em
`internal/ui/dist` e o job de teste não roda `npm ci`. O `.golangci.yml` já excluía
`node_modules` — em `exclusions.paths` e numa regra por linter — então o lint estava
consistente e as ferramentas do go não.

Os filtros de pacote viraram duas variáveis no topo do Makefile, usadas nos três lugares que
antes repetiam a cadeia de greps.

## Verificação do `make install`

`install` depende de `build` → `build-linux` (`ui setup-lbug fetch-ort-linux fetch-model`).
Passa, com os caches de `/tmp` quentes. O binário sai em `.build/graphit-linux-amd64` com
~517 MB, porque embute modelo, ONNX Runtime e ICU.

O `PREFIX` default é `/usr/local/bin`, que não é gravável pelo usuário aqui — o alvo cai no
branch `sudo cp` e pede senha, o que não roda de forma não interativa. A verificação foi feita
pelo branch gravável, e o binário instalado foi executado com `HOME` falso para não reescrever
`~/.graphit/runtime/v0.1.27` nem o `launcher.stamp` com o daemon rodando de dentro dele:

```
make install PREFIX=/tmp/graphit-install-test
HOME=/tmp/gt-fakehome /tmp/graphit-install-test/graphit --version   # graphit version v0.1.27
```

O runtime extraiu completo (`graphit-core`, `graphit-mcp`, `liblbug.so`, `libonnxruntime.so`,
`models/`, `ast/`) e o core respondeu. O caminho build → cópia → extração → exec está
funcional; o branch com `sudo` é o mesmo `cp` com privilégio, e o `⚠ não está no PATH` só
dispara quando o `PREFIX` de fato não está no PATH — foi o caso do diretório temporário.

## Resultado

```
make ci
  ✅ All CI checks passed.
```

Sem nenhuma linha `FAIL` no log. `coverage.out` é gerado (é o arquivo que o job do GitHub
converte para Cobertura).

## Arquivos alterados

| Arquivo | Mudança | Motivo |
|---|---|---|
| `internal/ast/writer_delete_repository_test.go` | Modificado | `sondaSchema` ganhou `ParamOwnerLabels: []string{"Function"}` e o comentário de por que o fallback não existe mais |
| `Makefile` | Modificado | `GO_PKGS_SKIP` / `GO_PKGS_PARSERS` no topo; `vet` e `test` passam a usá-las, excluindo `node_modules` |

## Decisões e trade-offs

- **Corrigir o teste, não a produção.** O fallback removido em `763fe938` era o bug, não a
  remoção dele. Reintroduzi-lo faria o schema voltar a declarar uma ponta para uma node table
  inexistente, que é exatamente o que abortava o rebuild no corpus Oracle.
- **Filtrar `node_modules` no Makefile em vez de deixar como está.** É pequeno e alinha o go
  tool com o `.golangci.yml`, que já tinha tomado essa decisão. Sem isso, `make ci` continua
  passando hoje mas depende da saúde de um pacote npm para continuar passando amanhã — e a
  falha apareceria só na máquina do dev.
- **Variáveis em vez de repetir o grep.** A cadeia aparecia em três lugares; era assim que ela
  ia divergir no próximo filtro.

## Débito técnico

- [ ] `BUILD_ID ?= $(shell …)` é variável recursiva, então o `$(shell)` roda a cada expansão:
      os três `go build` de `build-linux` recebem UUIDs diferentes (visível no log do build).
      Hoje é inofensivo — `version.BuildID` só é lido pelo próprio launcher, que compara o
      stamp com o valor compilado dentro dele (`cmd/launcher/main.go:205-231`), e nada
      cruza o BuildID do core com o do launcher. Corrigir seria `BUILD_ID := $(BUILD_ID)`
      depois do `?=`, preservando override por env.
- [ ] `.github/workflows/ci.yml` instala Zig 0.16.0 no job `build-check`, mas nada no
      `build-linux` usa Zig — nenhum `CC`/`CXX` aponta para ele. Provável resíduo; remover
      encurta o job.
- [ ] `build-linux` empacota todo `libicu*.so.[0-9]*` que encontra em `/usr/lib` e `/lib`:
      nesta máquina entram os sonames 74 **e** 78, mais `libicutest` e `libicutu`, que o
      binário não usa. Peso morto dentro dos 517 MB.
- [ ] `install` roda `mkdir -p $(PREFIX)` sem `sudo` antes de decidir se precisa de `sudo` no
      `cp`. Se o `PREFIX` não existir e o diretório pai não for gravável, o make aborta ali,
      antes de chegar ao branch com privilégio. Não afeta o default (`/usr/local/bin` já
      existe), afeta um `PREFIX` novo sob caminho de root.

## Conhecimento do sistema

- **Um `SchemaInfo` montado à mão precisa declarar os donos.** `HasParams: true` sozinho não
  cria HAS_PARAMETER, do mesmo jeito que `HasFields: true` sozinho não cria HAS_FIELD. Os dois
  grupos saem só das listas de donos (`ParamOwnerLabels`, `FieldOwnerLabels`), filtradas por
  `nodeTables`. Qualquer teste futuro que construa schema direto cai na mesma armadilha.
- **`make ui` muda o resultado de `go list ./...`.** É a única razão pela qual a ordem dos
  alvos dentro de `ci` importa para o conjunto de pacotes testados.
- **O launcher resolve seu appDir por `os.UserHomeDir()`**, isto é, `$HOME`. Isso é o que
  permite testar um binário recém-instalado sem tocar em `~/.graphit` — e é também o que faz
  rodá-lo com o `HOME` real ser perigoso: `cleanupOldRuntimes()` apaga os diretórios de
  runtime de outras versões.
- **`ui-lint` passa com warnings.** São 26 (imports não usados, um `any`, um
  `react-hooks/exhaustive-deps`) e o eslint sai 0, então não bloqueiam. Não são novos.
