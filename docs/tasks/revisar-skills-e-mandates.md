# Tarefa: completar a revisão das skills e mandates

**Status: concluída** em 2026-07-28. As três etapas estão nos commits `f0432fef`, `70fa594c` e
`39184383`, cada uma com seu changelog em `docs/changelogs/`. O levantamento abaixo fica como
referência (comandos, catálogo de ferramentas, convenções); a seção **O que sobrou** no fim
registra o que não pertencia a esta tarefa.

---

## O que foi feito

### Etapa 1 — `f0432fef` — cada skill ensina as ferramentas do próprio módulo

`hub_search` era a lacuna grave e virou a primeira chamada em toda a skill do Hub, com a semântica
documentada (substring de id/nome/descrição, sem stemming) porque isso muda como se busca. As
outras ausentes entraram todas. Decisões que o levantamento deixou abertas:

- **`ast_install`/`ast_remove` entram** — não são só ciclo de vida, são a origem dos contexts que
  o resto da skill já usava sem explicar de onde vinham.
- **`memory_sync` e `memory_remove` entram** juntos, numa subseção de contextos importados.
- **`knowledge_remove` entra por par** com install, com aviso: sem `context`, limpa o wiki local.
- **`knowledge_sync` entrou fora do levantamento** — não estava na tabela e é a ferramenta estreita
  para o caso em que o watcher não pode ter visto a mudança.

Também: `hub_type-path` não estava no mandate do próprio Hub apesar de ser usado pela skill de
improvements; e o ecossistema de projetos (`cluster_*`) ganhou ensino de verdade em vez de uma
frase de passagem, a pedido do Engenheiro.

### Etapa 2 — `70fa594c` — `config`, `daemon` e `dream` sem sexto mandate

**Desenho escolhido: seções nas skills existentes.** Não por economia — o caro é o mandate, que
fica em contexto em toda sessão, enquanto o corpo da skill é sob demanda. O que decidiu foi que
cada domínio já tinha uma skill que levava o agente até a porta e o abandonava sem ferramenta:

| domínio | skill | o gatilho que já existia sem mecanismo |
|---|---|---|
| `dream` | improvements | "você notou algo fora da mudança atual" — e não havia saída |
| `daemon` | knowledge | a tabela de exceções abre com "o daemon não está rodando" — sem como checar |
| `config` | hub | o hub já é dono de `cluster_*`; configuração é a mesma vaga |

A pergunta certa não é de que domínio a ferramenta é, é **o que o agente está fazendo quando
precisa dela**. Precedente: as skills daqui já são agrupadas por gatilho, não por prefixo.

### Etapa 3 — `39184383` — revisão do conteúdo

A pior: **`memory_gc` estava documentado ao contrário e apaga** — `DryRun` default `false`, então
a chamada sem parâmetro remove todo candidato. A skill apresentava essa como dry-run.

Também: o passo `3. Sync the wiki` sobrevivia no workflow do knowledge contradizendo a seção que o
commit `9e179bc9` acrescentou; o bloco obsoleto de sync ainda existia inteiro no ast; `hub_list`
recebia `project_dir` e um filtro por nome que não tem, na única etapa obrigatória da skill;
quatro lugares aceitavam ferramenta nativa sem o harness ter sido tentado; e `memory_search` era
descrito como leitura de `.md` crus quando é FTS5 sobre o wiki compilado.

---

## O que sobrou (não pertence a esta tarefa)

**Aresta `REFERENCES` de comentário nunca é persistida.** O commit `6ab88223` diz que cada
comentário carrega uma aresta `REFERENCES` para a declaração que precede. Não carrega:
`MATCH (c:Comment)-[:REFERENCES]->(t)` falha com *Table REFERENCES does not exist*. Causa raiz em
`internal/ast/rebuild_index.go:149` — o tipo de relação só é registrado quando `ref.SourceUID != ""`,
e o adaptador de comentários não preenche `SourceUID`, então a tabela nunca é criada e a escrita é
descartada. Os nós `Comment` existem e são alcançáveis por `CONTAINS`; só a aresta falta. Tarefa
separada aberta.

**Índice AST deste projeto tem 16 nós de sonda.** Durante a verificação desta tarefa um projeto de
sonda foi indexado e, por um bug de caminho relativo do `ast_index` via MCP, os nós foram para o
grafo deste projeto: um `File` `main.go` que não existe aqui, mais dez entidades e três comentários.
Nada foi destruído — o grafo estava vazio e `DeleteRepository` é um stub, então a chamada só
adicionou. Sai com `ast_index(reset: true)`, que é reindexação completa.

---

## Onde as coisas ficam

| o quê | onde |
|---|---|
| Template do mandate | `internal/hub/adapters/ide/mandate.go` → `ModuleMandateTrigger` |
| Mandate + skill de cada módulo | `internal/{ast,hub,knowledge,memory,improvements}/rule.go` (improvements tem também `rules.go`) |
| Conteúdo da skill | função `XxxRuleContent()` no topo de cada `rule.go` |
| Frontmatter da skill | dentro de `InstallSkill()`, string `"---\nname: …\ndescription: …\n---"` |
| Ferramentas MCP | `internal/mcpstdio/tools_*.go`, registradas via `brand.MCPToolName("dominio", "acao")` |
| Referência a ferramenta no texto | `brand.MCPToolRef("dominio", "acao")` → renderiza com crases |

Tamanhos: `knowledge` 1000 linhas, `ast` 660, `improvements/rules.go` 566, `memory` 365,
`mandate.go` 364, `hub` 289.

## Comandos (não óbvios — CGO, tags e a lib do Ladybug)

```bash
export LBUG=/home/lainosantos/go/pkg/mod/github.com/\!ladybug\!d\!b/go-ladybug@v0.17.0/lib
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go build -tags fts5 ./...
LD_LIBRARY_PATH="$LBUG:$LD_LIBRARY_PATH" go test -race -tags fts5 -p 4 -timeout 2400s \
  $(go list ./... | grep -v "/antlr/" | grep -v "/treesitter/")
golangci-lint run --timeout=5m     # RODE ANTES DE COMMITAR — a CI reprova por isto
make ci                            # vet, lint, vulncheck, test, ui, ui-lint
```

Os ~26 avisos de lint da UI são pré-existentes e não bloqueiam.

---

## Catálogo real de ferramentas MCP (62, verificado)

```
ast          ast_search ast_query ast_schema ast_source ast_list ast_index ast_export
             ast_embed ast_install ast_remove
hub          hub_search hub_show hub_list hub_install hub_link hub_unlink hub_update
             hub_submit hub_projects hub_uninstall hub_type-path
knowledge    knowledge_search knowledge_list knowledge_schema knowledge_lint
             knowledge_export knowledge_index knowledge_sync knowledge_install knowledge_remove
memory       memory_search memory_insert memory_update memory_list memory_important
             memory_promote memory_demote memory_delete memory_index memory_gc memory_schema
             memory_export memory_sync memory_remove
wiki         wiki_search wiki_browse wiki_xrefs wiki_log wiki_embed
cluster      cluster_get cluster_set cluster_unset cluster_projects
config       config_get config_set config_unset config_list
daemon       daemon_status daemon_stop
dream        dream_status dream_reports dream_subject_add dream_subject_list
             dream_subject_remove
improvements improvements_rules
```

O levantamento contava 62; são 64 com `hub_type-path` (que faltava na lista) e não incluindo as
cinco de ciclo de vida (`init`, `sync`, `update`, `remove`, `version`).

## O levantamento original — lacunas, todas fechadas

### 1. Ferramentas do próprio módulo ausentes do conteúdo da skill

| módulo | ausentes |
|---|---|
| ast | `ast_list`, `ast_index`, `ast_export`, `ast_embed` |
| hub | **`hub_search`**, `hub_submit`, `hub_projects`, `hub_uninstall` |
| knowledge | `knowledge_list`, `knowledge_schema`, `knowledge_lint`, `knowledge_export` |
| memory | `memory_export`, `memory_remove`, `memory_schema` |
| improvements | `improvements_rules` |

**`hub_search` é a mais grave:** o mandate manda "checar o Hub via MCP antes de confiar no
próprio conhecimento" e a skill nunca ensina a ferramenta de busca. O agente recebe a ordem
sem o meio de cumpri-la.

`ast_install`/`ast_remove`, `knowledge_install`/`knowledge_remove` e `memory_sync` são de
ciclo de vida/exceção — decida se entram, não assuma que sim.

### 2. Domínios sem skill nenhuma

`config` (4 ferramentas), `daemon` (2), `dream` (5). Decisão de arquitetura antes de escrever:
skill própria para cada, uma skill "operações" cobrindo os três, ou seções dentro das
existentes.

### 3. Revisão do conteúdo (a maior parte, não iniciada)

Ler cada `XxxRuleContent()` inteira procurando: instrução obsoleta, exemplo que não roda,
ferramenta citada com parâmetro errado, procedimento que o harness já automatiza, e lugar
onde ferramenta nativa é aceita sem o harness ter sido tentado antes.

---

## Já feito (não refaça)

Commit `9e179bc9`:

- `ModuleMandateTrigger` recebe `triggers []string` e `tools []string`. Seções vazias não
  renderizam.
- Os cinco mandates reescritos com gatilhos concretos e inventário de ferramentas.
- Bloco `⚡ MANDATORY: Sync After Every File Modification` do knowledge **removido**.
- Testes: `TestModuleMandateTriggerCarriesTriggersAndTools`,
  `TestModuleMandateTriggerOmitsEmptySections`.

---

## Coisas aprendidas que mudam decisões

**Mandate é gatilho, skill é ensino.** O mandate diz *quando abrir*; o procedimento mora na
skill. Não mova procedimento para o mandate.

**Mandate abstrato não dispara.** "Para qualquer tarefa de análise estrutural, use MCP" é
política, não gatilho: quem recebe "acha quem chama o saveUser" não classifica aquilo como
análise estrutural e vai de grep. Escreva o gatilho na forma em que o pedido chega.

**O watcher também reindexa o AST, não só o wiki.** Confirmado em
`internal/daemon/syncmodule.go`: um watch, dois consumidores (`reindexAST` e `reindexKnowledge`),
cada um com seu arquivo de ignore. E as memórias recompilam por conta do `MemorySyncModule`. Ou
seja, `sync`, `ast_index`, `knowledge_sync` e `memory_index` são **todos** ferramentas de exceção.

**A ferramenta estreita ganha da grande.** Quando só um subsistema está errado, `ast_index`,
`ast_embed` ou `knowledge_sync` fazem o mesmo que `sync` por uma fração do trabalho. `sync`
reindexa AST, os dois wikis, a memória e o Hub.

**O daemon segura o write lock e a leitura falha com mensagem enganosa.** Uma consulta ao grafo
que cai na janela de reindexação falha com `ladybug open: failed to open database with status 1` —
que nomeia o banco e lê como "não existe grafo aqui". É lock: tentar de novo funciona. Índice de
verdade ausente diz outra coisa (`no AST database found at ...`). Documentado nas skills de ast e
knowledge porque cair para grep aqui é o erro mais caro disponível.

**O watcher torna sync desnecessário.** O daemon observa a árvore de docs e reconstrói o wiki
sozinho. Qualquer instrução mandando sincronizar após editar está obsoleta — `sync` é
ferramenta de exceção: daemon parado, mudança vinda de fora da máquina, ou índice comprovadamente
velho. O que continua obrigatório é **escrever** o registro, não reindexá-lo. Procure o mesmo
padrão em `memory_sync` e `ast_index`.

**Nomear a ferramenta no mandate importa.** O agente decide entre MCP e nativa *antes* de abrir
a skill; até lá, só sabe o que o mandate disse.

**As skills são geradas de Go, não de markdown.** São slices de string concatenadas —
`gofmt` depois de mexer, e cuidado com aspas escapadas.

## Convenções deste repositório

- Código, comentários e nomes em **inglês**; commits, changelogs e docs em **português**.
- Changelog obrigatório em `docs/changelogs/{YYYYMMDD}_{nome}.md` ao concluir cada etapa —
  atômico, não um changelog gigante no fim.
- Nunca commitar automaticamente fora do fluxo pedido; nunca remover hooks do git.
- Nomes de sonda em teste devem ser **inventados**, nunca copiados do corpus real.
- Nada de mock ou stub em funcionalidade real sem autorização explícita.

---

## Invariante que ficou no lugar do levantamento

Um teste por módulo afirma que **toda ferramenta que o módulo possui é alcançável a partir da
própria skill** — porque o mandate anuncia o inventário, e ferramenta anunciada que a skill não
ensina é ordem sem meio de cumpri-la. Foi exatamente o caso do `hub_search`.

Ferramenta MCP nova, portanto, tem duas obrigações além do registro em `tools_*.go`: entrar no
`tools []string` do mandate do módulo e ser ensinada no `XxxRuleContent()`. O teste do pacote
reprova se faltar a segunda.

Os testes que verificam **aviso**, e não só menção, existem porque menção sozinha não evita o
erro: citar `dream_subject_add` sem dizer que o agente do dream não herda a conversa produz
subjects inúteis, e documentar `memory_gc` sem dizer que a chamada nua apaga produz perda de dados.
