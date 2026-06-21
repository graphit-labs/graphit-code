# Ignore treesitter vendored parsers (like ANTLR)

## Problema

Os arquivos `parser.c.inc` nos subdiretórios de `internal/ast/treesitter/`
(clojure, graphql, objc, r) são parsers C gerados pelo tree-sitter que foram
vendorizados porque os respectivos pacotes não publicam bindings Go oficiais.

O GitHub Linguist os detectava como **Pawn** por causa da extensão `.inc`,
inflando as estatísticas de linguagem do repositório de forma incorreta.

Além disso, esses arquivos não estavam sendo ignorados no AST indexer nem no
golangci-lint, ao contrário dos arquivos gerados do ANTLR que já tinham essas
exclusões.

## Solução

### `.astignore`
Adicionada regra `internal/ast/treesitter/*/parser.c.inc` para excluir os
arquivos C vendorizados do índice AST. Os `binding.go` (código Go de
integração) continuam sendo indexados normalmente.

### `.gitattributes` (novo arquivo)
Criado com `linguist-vendored=true` para todos os `parser.c.inc`:
```
internal/ast/treesitter/*/parser.c.inc linguist-vendored=true
```
Isso faz o GitHub Linguist excluir esses arquivos do cálculo de linguagem do
repositório (o mesmo efeito de `vendor/`).

Aproveitado para marcar arquivos gerados do ANTLR (`.interp`, `.tokens`)
como `linguist-generated=true`.

### `.golangci.yml`
Adicionado `internal/ast/treesitter` ao bloco `paths` de exclusão do
golangci-lint, espelhando o tratamento já existente para `internal/ast/antlr`.

## Arquivos afetados

| Arquivo | parser.c.inc |
|---|---|
| internal/ast/treesitter/clojure/ | 825 KB |
| internal/ast/treesitter/graphql/ | 281 KB |
| internal/ast/treesitter/objc/ | ? |
| internal/ast/treesitter/r/ | ? |

## Arquivos modificados

- `.astignore` — nova regra para `parser.c.inc`
- `.gitattributes` — criado do zero
- `.golangci.yml` — `internal/ast/treesitter` adicionado aos paths de exclusão
