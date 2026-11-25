---
title: Policy Rust de ignores AST
references:
  - target: {type: task, id: tsk-84a0}
    relation: implements
  - target: {type: task, id: tsk-ab16}
    relation: uses
  - target: {type: task, id: tsk-8b30}
    relation: supports
  - target: {type: task, id: tsk-6d5a}
    relation: supports
---

# Policy Rust de ignores AST

Para consumidores de indexação e watch que precisam dos defaults AST derivados
da configuração capturada. `AstIgnorePolicy` produz dados para o matcher Core
existente. A implementação está em
[ignore_policy.rs](../../crates/graphit-ast/src/ignore_policy.rs); os testes externos
estão em [ignore_policy.rs](../../crates/graphit-ast/tests/ignore_policy.rs).
O produtor não percorre arquivos nem executa o callback AST do daemon.

## API e configuração

```text
AstIgnorePolicy::from_config(
    snapshot: &ConfigSnapshot,
    original: &SnapshotContext,
) -> Result<AstIgnorePolicy, AstIgnorePolicyError>;
```

`filename()` devolve `.astignore`; `defaults()` empresta a lista owned de patterns.
O caller fornece seus roots reais e usa `IgnoreScope::load` com esses dados,
ou os copia para o `IgnoreConfig` do [watcher](rust_daemon_watchers.md).
O [Core matcher](../../crates/graphit-core/src/ignore_rules.rs) aplica defaults
protegidos, custom do scope mais profundo à raiz e então Git. A policy não concede
Auth, captura filesystem, cria cache de configurações ou escolhe linguagens.

Os getters tipados de `graphit_core::config::ConfigSnapshot` leem o winner real;
um erro não procura outro layer nem vira default. `ast.index_docs` ausente é false.
Quando true, a policy não consulta o `knowledge.docs_dir` sem uso: um tipo inválido
nesse campo não impede a configuração AST que já admitiu indexar docs.
Quando false, `knowledge.docs_dir` ausente é `docs`; o winner deve ser string.
Somente Environment decodifica uma string JSON no getter bool. O getter `&str`
conserva sua string literal: Environment docs dir `null` descreve `/null/`.
JSON null literal é rejeitado pelo Core ao construir o documento, antes da policy.

| Default | Derivação e efeito |
|---|---|
| `.git/` | Protege diretórios internos Git, na raiz ou em checkouts aninhados, contra descoberta automática e watch AST. |
| `<Brand.directory()>/` | Usa a marca validada do snapshot e protege seu diretório, inclusive conteúdo gerado. |
| `/<Brand.key()>.lock.json` | Naming confirmado do lockfile mantido; somente a raiz deste projeto é excluída. Um lockfile dentro de fixture não recebe esse default. |
| `/<docs-dir>/` | Somente a docs tree relativa à raiz é excluída, quando `index_docs=false` e o path resolve dentro do projeto. |

O lockfile da raiz é diferente de `<brand-directory>/.project.json.lock`.
A recipe Rust deriva o nome da marca capturada; não pressupõe um getter Core
de rootlock.

## Paths e nomes literais

A rotina usa os componentes de paths da biblioteca padrão, com os separadores
nativos. Ela aceita os aliases admitidos pela configuração: `./docs` e
`  ./docs  ` resolvem para `docs`; `docs//x` para `docs/x`; `docs/../x` para `x`.
Isso interpreta o path de configuração, sem transformar bytes de código ou
resolver symlinks. Diretórios ainda inexistentes podem receber um pattern.

`.` e um resultado vazio como `docs/..` não geram exclusão docs: excluir a raiz
esvaziaria o grafo. Paths absolutos, prefixes nativos e `..` que escape a raiz
também não geram exclusão docs. Os guards de marca e lockfile continuam presentes.
String configurada vazia, somente espaços, NUL ou CR/LF é erro explícito de path;
não vira valor ausente ou `docs` default.

`globset=0.4.20::escape` protege os metacaracteres do glob. No Unix, uma quote
curta conserva o backslash literal antes do SDK, porque o matcher Core o interpreta
como escape; no Windows os componentes nativos já interpretam separadores.
Assim, `docs[part]`, `docs*literal`, braces, Unicode e backslash nativo não podem
ampliar a docs tree para um nome parecido. O pattern tem slash inicial e final;
`internal/x/docs/` continua fora da exclusão `/docs/`.

Negação custom não reabre um default protegido. Para admitir a docs tree, use
`ast.index_docs=true`. Isso remove apenas o default docs; regras custom/Git podem
continuar excluindo paths. Markdown continua dependendo do catálogo de extensões,
conforme o [guia de ignores](../guides/ignore_files.md#astignore).

O default Git evita capturar refs, locks e objetos mutáveis como código do
projeto, mesmo quando uma negação custom tenta reabrir `.git/`. Não altera a
leitura Source explícita de FILE sob PROJECT autorizado, que mantém suas próprias
admissões, limites e verificações. `.github/` e fontes fora dos metadados Git
continuam sujeitos às regras normais de seleção.

## Seleção das fontes deste repositório

O `.astignore` versionado exclui os parsers Tree-sitter `parser.c` e
`parser.c.inc`, as cópias dos headers de runtime em `tree_sitter/*.h` e o
diretório C++ gerado do ANTLR. As gramáticas `.g4`, scanners `.c`/`.c.inc`,
headers de scanners e wrappers authored permanecem sujeitos à seleção normal.
Não há exclusão de toda a pasta de gramáticas nem de todas as extensões
`.h`, `.c.inc`, `.js` ou `.json`. Isso preserva fontes mantidas sem indexar
os grandes parsers gerados. O catálogo de extensões ainda decide quais formatos
têm estrutura AST; a leitura Source explícita conserva seu fallback para disco.

Os assets Vite copiados pelo build têm seletores próprios. Fontes React/TypeScript
continuam disponíveis; remover um seletor não concede autorização nem ignora
os limites concretos de arquivo.

## Erros, controles e consumidores

`AstIgnorePolicyError` conserva `Operation(io::Error)`,
`Setting { key, cause: io::Error }` e `DocsDirectory(io::Error)`, inclusive
`Error::source`. O mesmo Original é checado antes/depois de resolver settings,
durante componentes e antes de propagar outro erro. Cancelamento e deadline
preservam Interrupted e TimedOut; service Original sem deadline é válido.

Uma policy antiga conserva seus dados. Outra captura de Config produz outra
policy; esta API não promete reload automático de configurações. O consumidor
mantém roots/settings/Auth e lifecycle atuais e revalida suas capturas ao executar
IO ou entregar resultados. Política derivada e marca não restauram um grant.

## Verificação focal

O target externo combina ConfigSnapshot/ConfigDocument e o Core matcher reais:
defaults/custom Brand, lockfile root versus nested, docs root versus nested,
index_docs/dot/escape/aliases, nomes glob literais, backslash Unix, negações e
reinclusão, tipos winners/Environment/null, snapshots novos e controles Original.
Não usa walker ou mocks de autorização para provar a policy. Em Windows,
os casos `*`/`?` mantêm os asserts lexicais do SDK, mas não tentam criar nomes
que o filesystem nativo proíbe.

Os comandos são `cargo test --locked -p graphit-ast --test ignore_policy` e
`cargo clippy --locked -p graphit-ast --lib --test ignore_policy -- -D warnings`.
Tentativas e qualificação pertencem a `.build/ast-ignore-policy/`.
Em Linux, `test04` passou os 11 testes em 0,00 s; o comando levou 394,08 s,
incluindo a fila compartilhada e a recompilação de dependências. O Clippy focal
também passou (`clippy01`, exit 0, 128,81 s). O histórico conserva `test01`
interrompido sem exit observado,
`test02` falho por assertions que não propagavam `Result<bool>` (corrigidas) e
`test03` falho durante a transição do Core (corrigida pelo dono).
Windows/macOS permanecem sem execução. Essa verificação de dados e matcher
não substitui a integração Main/watch/AST, Auth, ModelService,
bootstrap NativeNN ou publicação semântica de todas as linguagens.
