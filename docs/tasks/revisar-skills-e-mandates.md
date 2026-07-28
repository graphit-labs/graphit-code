# Tarefa: completar a revisão das skills e mandates

Arquivo de tarefa auto-contido. O prompt para abrir a próxima sessão está no fim, mas leia o
levantamento antes — ele já foi feito e verificado, e refazê-lo é desperdício.

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
             hub_submit hub_projects hub_uninstall
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

## O que FALTA — lacunas verificadas

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

## Prompt para a próxima sessão

> Leia `docs/tasks/revisar-skills-e-mandates.md` — é auto-contido e traz o levantamento já
> verificado, os comandos de build/test/lint com as variáveis certas, o catálogo real das 62
> ferramentas MCP e o que já foi feito no commit `9e179bc9`. Não refaça o levantamento.
>
> Complete o trabalho, nesta ordem:
>
> 1. **`hub_search` primeiro** — é a lacuna mais grave: o mandate manda checar o Hub via MCP e
>    a skill nunca ensina a ferramenta de busca. Depois as outras ferramentas ausentes de cada
>    skill, conforme a tabela.
> 2. **Decida e implemente a cobertura de `config`, `daemon` e `dream`**, que hoje não têm
>    skill nenhuma. Me diga qual desenho você escolheu e por quê antes de escrever tudo.
> 3. **Revise o conteúdo de cada `XxxRuleContent()` por inteiro**, procurando: instrução
>    obsoleta (o watcher já automatiza várias coisas), exemplo que não roda, parâmetro errado,
>    e todo lugar onde ferramenta nativa é aceita sem o harness ter sido tentado antes. Priorize
>    sempre as ferramentas deste harness sobre as nativas do agente.
> 4. Aplique boas práticas de escrita de skill: gatilho concreto em vez de domínio abstrato,
>    exemplo executável em vez de descrição, e a razão junto da regra — uma regra sem o porquê
>    é ignorada na primeira vez que atrapalha.
>
> Rode `golangci-lint run` **antes de cada commit** — a CI reprova por isso e é fácil esquecer.
> Suíte completa com `-race` verde antes de fechar. Changelog atômico por etapa, em português.
> Vá commitando por etapa em vez de tudo no fim, e me diga o que decidiu em cada uma.
