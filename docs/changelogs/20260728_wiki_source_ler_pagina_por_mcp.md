# `wiki_source`: ler página do wiki por MCP, com o mesmo fatiamento do source

**Data:** 2026-07-28
**Escopo:** `internal/textslice/` (novo), `internal/wiki/source.go` (novo),
`internal/ast/source_service.go`, `internal/mcpstdio/tools_wiki.go`,
`cmd/graphit/commands/{wiki.go,runners.go}`, `internal/{knowledge,memory}/rule.go`
**Origem:** pedido do Engenheiro — o agente costuma estar limitado ao próprio workspace

---

## O problema

As skills mandam, em vários lugares: *"busque no wiki, **leia a página da entidade**, siga os
[[wikilinks]]"*. Todas as ferramentas de wiki recebem `project_dir` e leem em nome do agente —
`wiki_search`, `wiki_browse`, `wiki_log`, `wiki_xrefs`, `knowledge_search`, `memory_search`.

**Ler a página era o único passo sem ferramenta.** Só sobrava a leitura direta de arquivo. E é
justamente o passo que falha quando o agente está confinado ao próprio workspace: a página que ele
precisa pertence a outro projeto do ecossistema, num path que ele não pode abrir.

Ou seja: a etapa 4 mandou explorar o irmão por MCP, e no meio do caminho a instrução voltava a
depender de uma ferramenta nativa que não alcança lá.

## `wiki_source`

MCP e CLI, com **as mesmas opções de fatiamento do `ast_source`**: `head`, `tail`,
`start_line`/`end_line`, `line_numbers`, e `pattern` com `regex`, `before` e `after`.

```
wiki_source(project_dir: "/path/to/project", path: "auth-flow")
wiki_source(project_dir: "/path/to/project", path: "auth-flow", head: 40)
wiki_source(project_dir: "/path/to/project", path: "auth-flow", pattern: "refresh token", before: 2, after: 4)
wiki_source(project_dir: "/path/to/project", path: "<slug>", wiki: "memory")
wiki_source(project_dir: "<sibling dir>", path: "<slug>")          # o caso que motivou tudo
```

```bash
graphit wiki source auth-flow --head 40
graphit wiki source auth-flow --pattern token --before 2 --after 4
graphit wiki source correction-x --wiki memory
graphit wiki source auth-flow --project /path/to/other-project
```

### `path` aceita o que as outras ferramentas devolvem

`wiki_search`, `wiki_browse` e `wiki_xrefs` devolvem `Slug`; `knowledge_search` devolve `Path`,
que é `Slug + ".md"`. Então `path` aceita as duas formas, mais um path relativo ao diretório do
wiki — e casa **sem diferenciar caixa**, porque nome de arquivo de wiki é gerado do título
(`1._Clean_Code.md`) e o slug que o agente tem em mão raramente bate exatamente. Exigir casamento
exato tornaria a ferramenta inútil justamente com os resultados da própria busca.

Errou o slug? O erro **lista as páginas que existem**. Isso é a resposta, não motivo para voltar a
ler arquivo.

### Duas classes de erro, não uma

`ErrPageNotFound` distingue slug errado de referência recusada. Slug errado se resolve listando as
alternativas; referência que **escapa do diretório do wiki** precisa manter o próprio motivo — se
as duas caíssem no mesmo lugar, a listagem enterraria a razão da recusa.

Verificado: `../../../etc/passwd`, `..`, `../outside.md` e path absoluto são todos recusados, e
nenhum deles é reportado como "não encontrado".

## DRY: o fatiamento saiu para `internal/textslice`

Tudo em `ast.SourceService.GetSource` depois de buscar o texto é manipulação de texto pura. Fazer
uma segunda cópia para o wiki seria duplicar ~100 linhas que iriam divergir na primeira correção —
e a própria skill de improvements chama isso de código WET.

O pacote novo carrega o que é idêntico nas duas: `Apply` (faixa → head → tail → pattern, nessa
ordem fixa), `Search` (casamento, janelas de contexto mescladas), `FormatMatches` e
`FormatWithLineNumbers`. **Só a busca do texto difere, e só ela ficou nos chamadores** — grafo de
código de um lado, diretório de wiki do outro.

`ast.SourceService` delega os três helpers-folha. A aritmética de janela de entidade, que é a parte
sutil e específica do AST, ficou onde estava — extrair também aquilo seria risco sem ganho.

Efeito colateral: o insertion sort escrito à mão que ordenava índices saiu, e com ele
`TestSortInts`. `sort.Ints` faz o mesmo e não precisa de teste aqui.

## As skills

O passo **1b** entrou na sequência de busca do knowledge, entre "busque" e "leia o frontmatter",
com as duas razões coladas:

- **Recebe o projeto como parâmetro.** Leitura de arquivo não sai do seu sandbox; esta ferramenta
  lê em seu nome, então a página de um irmão é a mesma chamada com outro `project_dir`.
- **Fatia.** Página longa custa a parte que você pediu, não toda ela.

E a tabela "quando NÃO usar o wiki" ganhou uma nota explícita: **ler página de wiki não está nessa
lista.** As ferramentas nativas de arquivo entram para *escrever* documentação, nunca para
recuperá-la.

Na skill de memória, o passo 3 da recuperação deixou de dizer "leia o conteúdo" e passou a mostrar
`wiki_source` com `wiki: "memory"` — incluindo a forma com `pattern` para memória longa e a forma
com `project_dir` de irmão. O passo 4 (*"nunca leia .md de memória direto"*) agora diz **o que
fazer em vez disso**, que é o que faltava para a proibição ser acionável.

`wiki_source` entrou no inventário dos mandates de knowledge e memory.

## Testes

`internal/textslice`: ordem de composição (head aplica à janela selecionada, não ao arquivo),
numeração absoluta preservada no tail, faixa fora do fim é limitada em vez de estourar, janelas de
contexto sobrepostas mesclam sem repetir linha, separador `---` só onde linha foi omitida, padrão
literal casa sem caixa e regex não é dobrado em silêncio, regex inválida é reportada.

`internal/wiki`: slug resolvido nas quatro grafias, fatiamento igual ao do source, recusa de fuga
do diretório em quatro formas, `ErrPageNotFound` distinto da recusa, entrada vazia rejeitada,
`ListPages` sem extensão, `firstHeading` pulando frontmatter.

Um caso do teste de mesclagem estava com a expectativa errada — eu contei 6 linhas onde a linha 5
é legitimamente omitida. O código estava certo; virou dois testes, um para "há lacuna, tem
separador" e outro para "janelas se juntam, não tem".

Verificado contra o wiki real deste projeto: `--head 12 --line-numbers` devolve o frontmatter
numerado, `--pattern DRY --before 1 --after 2` devolve dois grupos separados por `---` com os hits
marcados, e `1._clean_code.md` casa `1._Clean_Code.md`.

`golangci-lint` limpo.

> **Nota sobre o estado da árvore:** havia outra sessão trabalhando em ANTLR/Oracle nesta mesma
> árvore durante esta etapa. Nada dela foi commitado nem revertido — este commit stageia apenas os
> arquivos desta etapa, por caminho explícito.
