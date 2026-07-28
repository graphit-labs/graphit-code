# Revisão do conteúdo das skills: obsoleto, contraditório, errado e concessivo

**Data:** 2026-07-28
**Escopo:** `internal/{ast,knowledge,memory,improvements}/rule.go`, `internal/improvements/rules.go`
e testes novos
**Origem:** etapa 3 de `docs/tasks/revisar-skills-e-mandates.md`

---

## A pior: `memory_gc` estava documentado ao contrário, e apaga

```go
// internal/mcpstdio/tools_memory.go
if !input.DryRun && len(report.Candidates) > 0 {
    for _, c := range report.Candidates {
        _ = svc.RemoveMemory(c.ID)
    }
}
```

`DryRun` é `bool`, default `false`. **A chamada sem parâmetro apaga.** A skill dizia o oposto:

```
memory_gc(project_dir: "...")                   # find stale/empty memories (dry-run)
memory_gc(project_dir: "...", dry_run: false)   # delete GC candidates
```

Um agente seguindo aquilo destruía memórias acreditando que estava escaneando. Sem confirmação e
sem undo.

Corrigido, com aviso próprio e a ordem certa — **escanear, ler, só então apagar**. E com o motivo
para não confiar no critério: candidato é memória intocada por `stale_days` (default 30) ou vazia,
e trinta dias sem leitura é evidência fraca para apagar uma `convention` ou `correction` — são
exatamente as memórias que ficam paradas até a única sessão em que impedem você de repetir um
erro. Ler o scan, `memory_promote` no que deve sobreviver, aí coletar.

## Contradição dentro da mesma skill: o passo de sync que sobrou

A skill de knowledge tinha as duas instruções ao mesmo tempo:

```
1. Do the work
2. Write the documentation
3. Sync the wiki — call graphit_sync ...      ← aqui
4. Only then report the task as complete
```

e, mais abaixo, a seção *"Reindexing is automatic — do not call sync in the normal flow"*
introduzida no commit `9e179bc9`. Um agente lendo de cima para baixo obedece a primeira.

O passo 3 saiu. O que ficou no lugar diz que **não há passo de reindexação** e aponta para a
seção que explica as exceções. A exigência de conclusão é o **registro**, não o índice dele.

## O mesmo bloco obsoleto ainda existia inteiro no ast

`### ⚡ MANDATORY: Sync After Every File Modification`, com *"esquecer de chamar sync é violação
de integridade do framework"*. O commit anterior removeu o do knowledge e deixou este.

E é obsoleto pela mesma razão, verificada em `internal/daemon/syncmodule.go`: o watcher observa a
árvore de fontes e chama `reindexAST` com os caminhos exatos que mudaram — o comentário no código
diz que nomear os caminhos deixa o reindex pular a descoberta inteira, ~350ms de um incremental de
~1,07s num repositório de 35k arquivos.

Virou tabela de exceções com **a ferramenta certa por linha**, não `sync` para tudo:

| situação | o que chamar |
|---|---|
| daemon parado | `ast_index` |
| código veio de fora — pull, checkout, rebase, restore | `ast_index` |
| consulta devolve coisa velha um minuto depois da edição | `ast_index` com `path` |
| gramática errada para a extensão | `ast_index` com `grammar` |
| busca semântica devolve nada | `ast_embed` |

Em toda linha a ferramenta estreita ganha: mesmo efeito, fração do trabalho.

## Ferramenta errada para vetores faltando

Phase 2.3 mandava: *"se os resultados semânticos estiverem vazios, chame `graphit_sync`"*. Isso
reindexa o grafo AST, os dois wikis e o Hub para consertar um conjunto de vetores. É `ast_embed`.

Acrescentado o que faltava para o agente não tirar a conclusão errada: no modo hybrid a busca cai
para FTS quando não há vetores, então **semântico vazio não é hybrid vazio** — tentar em hybrid
antes de concluir que o código não existe.

## Parâmetro que a ferramenta não tem, na única etapa obrigatória da skill

O protocolo Hub-first da skill de knowledge — *"antes de implementar QUALQUER integração"* —
mandava:

```
hub_list(project_dir: "/path/to/project", type: "knowledge")
```

`hub_list` não tem `project_dir`. E o passo 0 do Workflow dizia *"chame `hub_list` filtrando por
name/type"*: `hub_list` **não filtra por nome**, só por tipo. A única etapa obrigatória da skill
era uma chamada que não consegue fazer o que foi pedido dela.

Agora é `hub_search(query: "<nome do sistema>", type: "knowledge")` — buscar pelo nome, que é o
que você tem —, com `hub_list` no papel de fallback quando a busca volta vazia. Mesmo erro
corrigido no passo 6 do fluxo de investigação do ast.

## Ferramenta nativa aceita sem o harness ter sido tentado

Quatro lugares. Em cada um a ferramenta do harness passou à frente, com a razão colada e a
exceção nomeada — regra sem porquê é ignorada na primeira vez que atrapalha.

### "Já sei o caminho, vou só ler o arquivo"

Duas ocorrências no ast diziam *"use suas ferramentas nativas de leitura — são mais rápidas e
simples"*. `ast_source` lê a cópia indexada: uma chamada dá faixa de linhas, uma função pelo nome,
ou um padrão com contexto — onde a leitura direta dá o arquivo inteiro e você paga cada linha em
tokens. E é a única das duas que funciona em contexto importado, cujos arquivos não estão neste
checkout.

A exceção real, nomeada: o arquivo **não está no grafo** — acabou de ser criado, `.astignore`
exclui, ou `ast.index_source` é `false`. `ast_source` diz isso quando acontece; essa resposta é o
sinal para ler do disco, não motivo para pular a ferramenta.

### "Preciso buscar dentro de comentários"

*"Searching inside string literals or comments → grep/ripgrep on source files"* — obsoleto desde
os commits `6ab88223` e `d5b1b66b`. Comentário é entidade `Comment` com o texto em `name`.
Verificado nesta sessão:

```
MATCH (f:File {path: 'x.go'})-[:CONTAINS]->(e) RETURN label(e) AS type, e.name, e.line_number ORDER BY e.line_number
→ 1|Package main is a probe fixture.|Comment
  7|Quartzo|Function
  ...
```

O grafo ganha justamente no que grep supostamente faz bem: sem regex para escapar, resultado já
com arquivo e linha, e **comentário de bloco vem como um nó só** em vez de cinco linhas casadas
sem relação. Literal de string dentro de corpo de função continua fora do grafo — e mesmo lá,
`ast_source` com `pattern` antes de grep.

`Comment` entrou na tabela de propriedades e ganhou seção no cookbook, com seis consultas: marcador
em qualquer lugar, comentários de um arquivo em ordem de leitura, esqueleto com comentários
interpolados, código comentado esquecido, cabeçalho de licença, e comentário adjacente a uma
declaração. As duas estruturalmente novas foram executadas contra o grafo antes de entrar.

### Sair do wiki não é ir para o grep

A tabela "quando NÃO usar o wiki" mandava para *"Normal file tools"* e *"grep/ripgrep on source
code"*. Sair do wiki entrega o agente à **skill de AST**, não à busca textual — os dois índices
cobrem coisas diferentes e grep está abaixo dos dois. Dito explicitamente, porque a leitura
natural da tabela antiga era a oposta.

### Busca na web sem o Hub ter sido tentado

A skill de improvements tem uma seção inteira — *"When to Search the Internet"* — que manda buscar
na web em erro desconhecido, quirk de biblioteca, incerteza sobre abordagem e escolha de
dependência. **Sem nenhuma menção ao Hub**, contra o mandate do Hub, que é categórico.

Entrou um gate acima da seção inteira: `hub_search` primeiro para qualquer coisa externa; web só
depois de vazio e **dizendo ao usuário**. Com a razão que importa: o artefato do Hub é curado,
versionado e casado com a versão que este projeto usa — que é exatamente o que um resultado de
busca não é.

E o equivalente interno: antes de buscar na web sobre o comportamento **deste** projeto, o grafo e
o wiki já sabem. `ast_search` e `knowledge_search` respondem do que está aqui; um buscador
responde do que costuma ser verdade.

## Duas afirmações que a implementação não sustenta

**`memory_search` não lê arquivos `.md` crus.** A skill dizia isso numa linha e o contrário cem
linhas antes, na mesma tabela. `wiki.BM25Search` abre o wiki compilado via SQLite FTS5 e só cai
para um índice BM25 em memória se o banco FTS não existir. Corrigido nas duas — e acrescentada a
consequência que o agente precisa para explicar o sintoma: memória escrita segundos atrás pode não
aparecer porque o wiki ainda não recompilou. `memory_list` lê o store e vê o que o wiki ainda não
compilou; `memory_index` força o rebuild.

**`hub_list` não mostra o que está instalado** — já corrigido na etapa 1, citado aqui porque é a
mesma classe: afirmação sobre uma ferramenta que a ferramenta não sustenta.

## Nome de ferramenta escrito à mão

`internal/memory/rule.go` tinha `` `graphit_wiki_browse` `` como literal em vez de
`brand.MCPToolRef("wiki", "browse")`. Num build rebrandado isso renderiza uma ferramenta que o
agente não tem. Faltava também `wiki: "memory"` — o default é o wiki do projeto, ou seja, a skill
de memória mandava navegar o wiki errado.

## Decision Validation Gate passava por fora do harness

O gate manda verificar se alguma decisão anterior justifica a implementação atual. Dois dos quatro
passos ignoravam as ferramentas:

- *"Check `docs/decisions/` for an ADR"* → `knowledge_search`, que ranqueia e traz as
  cross-references, então acha o ADR que menciona seu módulo mesmo quando o nome do arquivo não
  menciona.
- *"Look for comments like `// DECISION:`"* → consulta no grafo, que cobre o codebase inteiro numa
  chamada em vez do arquivo à frente do agente, e devolve a justificativa completa em vez da linha
  que casou.

O passo de memórias ganhou os projetos irmãos: a decisão pode ter sido tomada ao lado. O de
relatórios anteriores ganhou `dream_reports` com `all: true` — uma sessão noturna pode ter olhado
exatamente isso e concluído deixar como está.

E o passo 4 da codificação de artefatos (*"olhe nos diretórios de artefatos do IDE"*) agora resolve
o caminho com `hub_type-path`: cada IDE organiza diferente, e olhar no lugar errado responde "não
existe esse artefato" quando existe.

## Testes

Cada um afirma a correção **e** o que a substituiu, porque só proibir o texto antigo deixa passar
a remoção sem substituto:

| teste | o que trava |
|---|---|
| `TestMemoryRuleContentDoesNotInvertTheGCDryRun` | a forma destrutiva não volta a ser documentada como dry-run |
| `TestMemoryRuleContentDescribesSearchAccurately` | `memory_search` não volta a "lê `.md` crus" |
| `TestMemoryRuleContentBuildsTheBrowseToolFromTheBrand` | nome de ferramenta construído da marca, escopo `memory` |
| `TestASTRuleContentDoesNotDemandSyncAfterEveryEdit` | o bloco obsoleto não volta |
| `TestASTRuleContentSendsMissingEmbeddingsToEmbed` | vetores faltando vão para `ast_embed`, não `sync` |
| `TestASTRuleContentTreatsCommentsAsQueryable` | comentário não volta para o grep |
| `TestASTRuleContentPrefersSourceToolOverNativeRead` | e a exceção legítima continua nomeada |
| `TestKnowledgeRuleContentWorkflowHasNoSyncStep` | a contradição interna não volta |
| `TestKnowledgeRuleContentUsesHubSearchForIntegrations` | `hub_list` não volta a receber `project_dir` nem filtro por nome |
| `TestKnowledgeRuleContentRoutesCodeQuestionsToTheGraph` | sair do wiki continua levando ao grafo |
| `TestImprovementsRuleContentGatesWebSearchBehindTheHub` | o gate do Hub não sai |
| `TestImprovementsRuleContentValidatesDecisionsThroughTheHarness` | o gate de decisão não volta a listar diretório |

Um teste meu foi corrigido no caminho: `TestMemoryRuleContentHasNoHardcodedToolNames` mutava
`brand.Brand` global com `t.Parallel()` e o detector de race pegou — outros testes paralelos do
pacote leem a variável. Trocado por uma afirmação direta sobre a correção, que não precisa mexer
em estado global.

`golangci-lint` limpo, suíte com `-race` verde.
