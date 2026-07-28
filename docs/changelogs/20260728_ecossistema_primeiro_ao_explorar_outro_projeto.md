# Explorar outro projeto: ecossistema primeiro, e o irmão se explora por MCP

**Data:** 2026-07-28
**Escopo:** `internal/{hub,ast,knowledge,memory}/rule.go` e testes novos
**Origem:** pedido do Engenheiro durante a revisão das skills

---

## A regra

Quando a pergunta é sobre código, documentação ou comportamento que **não está neste
repositório**, a ordem é obrigatória:

1. **Resolver no ecossistema — `cluster_projects` — antes de qualquer outra coisa.** Antes de
   perguntar ao usuário onde o projeto fica, antes de chutar um caminho, antes de `ls` num
   diretório pai, e antes de responder do que um serviço desses normalmente faz.
2. **Se está no ecossistema, explorar igual a este projeto** — mesmos tools MCP, o `dir` dele como
   `project_dir`.
3. **Só se não está** a pergunta muda de forma: checkout que o usuário aponta vira context
   importado (`ast_install`); dependência que você não tem vira busca no Hub.
4. **Ferramenta nativa na árvore do irmão é a última** — depois do grafo e do wiki, não em vez
   deles.

## A distinção que as skills não faziam

A skill de ast falava de **context importado** como se fosse o único jeito de consultar código de
fora. Não é, e a linha que faltava é a mais barata das quatro:

| o código está em | como consultar |
|---|---|
| este repositório | default, sem `context` |
| **projeto irmão do ecossistema** | **o grafo dele: passe o `dir` como `project_dir`.** Sem import, sem context |
| checkout na máquina que não é projeto registrado | `ast_install` uma vez, depois `context` |
| dependência sem checkout | `hub_search` com `type: "ast"` → `hub_install`, depois `context` |

Um projeto registrado **já tem grafo indexado, wiki compilado e memórias próprias**. Importá-lo
como context reindexa um grafo que já existe. E não saber disso é o que manda o agente ler
arquivo por arquivo: nada tinha dito que havia grafo para consultar.

## Nada precisa ser instalado, linkado ou importado

`project_dir` é **parâmetro**. Apontar qualquer tool para outro projeto é passar um valor
diferente — é isso, e é tudo. Verificado no código: `wiki_search`, `wiki_browse`, `wiki_log`,
`knowledge_search`, `memory_search`, `ast_*` todos recebem `project_dir`, e
`resolveWikiDBDir(projectDir, scope)` resolve o wiki **daquele** projeto.

`hub_link` entrou como anti-padrão explícito, porque é a confusão natural: link existe para
desenvolver artefato localmente, traz **um** artefato para **este** projeto, e não concede acesso
nenhum que passar `project_dir` já não dê. É verboso para uma coisa que ele nem faz.

## O conjunto que o irmão merece

```
# 1. resolver — nunca chutar o caminho
cluster_projects(project_dir: "/path/to/project")

# 2. o que o código faz
ast_search(project_dir: "<sibling dir>", query: "token validation")
ast_query(project_dir: "<sibling dir>", query: "MATCH (f) WHERE ... RETURN ...")

# 3. ler — por entidade ou faixa de linhas, nunca o arquivo inteiro
ast_source(project_dir: "<sibling dir>", path: "<do query>", entity: "<do query>")

# 4. para que serve, e o que mudou lá
knowledge_search(project_dir: "<sibling dir>", query: "authentication")
wiki_search(project_dir: "<sibling dir>", query: "...", wikis: ["project", "memory"])
wiki_browse(project_dir: "<sibling dir>")
wiki_log(project_dir: "<sibling dir>")

# 5. por que é assim
memory_search(project_dir: "<sibling dir>", query: "token")
```

`wiki_log` é o que vale lembrar: lista o que o wiki daquele projeto adicionou, atualizou e apagou
por sync, então *"o que mudou lá recentemente"* é uma chamada em vez de um diff reconstruído à mão.

`memory_search` no irmão é o que nenhuma leitura de código substitui: **o código mostra o quê, as
memórias são o único registro do porquê.** Decisão tomada de propósito por quem trabalhou lá não
se reconstrói lendo o resultado dela.

## Por que essa ordem e não a óbvia

Cada razão colada na regra, porque regra sem porquê é ignorada na primeira vez que atrapalha:

- **O irmão registrado já tem grafo e wiki.** Nada a importar, nada a indexar, nenhum artefato a
  instalar — você tem lá os mesmos tools que tem aqui, no instante em que sabe o caminho. Pular o
  passo 1 é como isso passa batido.
- **Grep na árvore de outro projeto é a pior opção disponível.** Layout desconhecido, sem ranking,
  cada match pago em tokens, e sem acesso às relações — quem chama, o que importa, quem implementa
  — que são o motivo pelo qual você estava olhando.
- **Chutar o caminho falha em silêncio.** `project_dir` errado não dá erro: responde com confiança
  sobre outro codebase, ou volta vazio e lê exatamente como *"esse código não existe"*.
- **A palavra do usuário raramente casa com o nome do diretório.** Casar contra `name` e
  `description` da saída, não contra o basename.

## Mandates

A decisão entre MCP e nativa acontece **antes** de abrir qualquer skill, então a ordem entrou
também nos gatilhos:

- **hub** — *"o usuário nomeia outro projeto, serviço ou repositório — resolva no ecossistema
  PRIMEIRO, depois explore com os tools MCP de AST e wiki usando o `project_dir` dele; nunca chute
  o caminho, nunca leia nem faça grep nos arquivos dele"*
- **ast** — o gatilho de outro repositório ficou explícito, e a cláusula incondicional passou a
  dizer que grafo-primeiro **vale para outros projetos também**, senão o agente lê como se
  valesse só para o repositório em que está
- **knowledge** — *"a documentação que você precisa é de outro projeto — resolva no ecossistema e
  busque no wiki DELE, nunca ande nem faça grep na árvore de docs"*
- **memory** — *"a pergunta é sobre outro projeto do ecossistema: as memórias dele guardam por que
  ele é como é"*

`cluster_projects` entrou no inventário de ferramentas dos mandates de ast e knowledge — o agente
precisa saber que a ferramenta de resolução existe no momento em que decide, e até ali só sabe o
que o mandate disse.

## Testes

| teste | o que trava |
|---|---|
| `TestHubRuleContentMandatesTheCrossProjectProtocol` | a ordem obrigatória e as seis chamadas no irmão, incluindo `project_dir` ser só um parâmetro |
| `TestHubRuleContentForbidsNativeExplorationOfSiblings` | os três anti-padrões: reimportar, `ls`/grep para se orientar, e `hub_link` para "ter acesso" |
| `TestMandateTriggerCarriesTheEcosystemFirstOrder` | a ordem não sai do mandate |
| `TestASTRuleContentDistinguishesSiblingsFromImportedContexts` | as quatro linhas da tabela, e a proibição de importar irmão registrado |
| `TestASTRuleContentWarnsThatAWrongProjectDirFailsSilently` | o aviso de falha silenciosa |
| `TestMandateTriggerExtendsGraphFirstToOtherProjects` | grafo-primeiro continua valendo para outro projeto |
| `TestKnowledgeRuleContentCoversSiblingWikis` | wiki do irmão com `wiki_log` e `ast_source` inclusos |
| `TestKnowledgeRuleContentPutsTheLookupBeforeTheReading` | a ordem: busca antes da leitura, wiki antes dos arquivos |
| `TestMemoryRuleContentCoversSiblingMemories` | memórias do irmão são legíveis |

Um teste meu da etapa 1 quebrou e foi corrigido: afirmava o título literal `"Imported Contexts"`,
que virou seção de nível menor com outra grafia. Agora compara sem depender de caixa — mesma lição
do `TestHubRuleContent` na etapa 1: **afirmar conteúdo, não redação.**

`golangci-lint` limpo, suíte completa com `-race` verde.
