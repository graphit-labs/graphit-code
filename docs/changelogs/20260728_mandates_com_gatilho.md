# Mandates ganham gatilho concreto e inventário de ferramentas; sync deixa de ser obrigatório

**Data:** 2026-07-28
**Escopo:** `internal/hub/adapters/ide/mandate.go`, `internal/{ast,hub,knowledge,memory,improvements}/rule.go`
**Origem:** pedido do Engenheiro para melhorar os mandates e revisar o conteúdo das skills

---

## O problema do mandate abstrato

Cada mandate dizia, em essência, "para qualquer tarefa de *análise estrutural*, use MCP".
Isso é política, não gatilho. Um agente a quem se pede "acha quem chama o saveUser" **não
necessariamente classifica aquilo como análise estrutural** — e vai de grep. O mandate estava
correto e mesmo assim não disparava.

`ModuleMandateTrigger` passou a receber duas listas:

- **`triggers`** — as situações concretas em que a skill tem de ser aberta, escritas na forma
  em que o pedido chega, não no vocabulário do módulo. "Você está prestes a rodar grep para
  localizar código" dispara; "análise estrutural" não. Fecha com *"se você não tem certeza se
  um destes se aplica, ele se aplica"*, porque a dúvida é onde o gatilho falha.
- **`tools`** — as ferramentas MCP que o módulo possui, nomeadas no próprio mandate. O agente
  precisa saber que a ferramenta existe **antes** de abrir a skill: é nesse instante que ele
  decide entre MCP e ferramenta nativa, e até então a skill ainda não foi lida.

Seções vazias não são renderizadas, então um módulo sem ferramentas não ganha cabeçalho solto.

## Os cinco gatilhos

Escritos a partir do que faz cada módulo ser ignorado na prática:

- **AST** — dispara em "vou rodar grep/glob/find", em pedidos que nomeiam um símbolo, em
  perguntas de relacionamento (quem chama, o que quebra), e em *"você está prestes a responder
  sobre código que não leu, de memória de bases parecidas"*.
- **Hub** — dispara em qualquer biblioteca ou API externa **incluindo as que você acha que
  conhece bem**, e antes de recorrer a busca na web.
- **Knowledge** — dispara em "por que isto é assim", em pedidos sobre feature/arquitetura em
  vez de símbolo, e em *"você está prestes a afirmar como este projeto funciona por inferência,
  não por ter lido aqui"*.
- **Memory** — dispara no início da sessão **antes da primeira resposta**, quando o usuário
  corrige ou expressa preferência, quando a segunda tentativa falha como a primeira, e quando
  você iria escrever em memória nativa do IDE.
- **Improvements** — dispara ao terminar uma tarefa, porque a reflexão **faz parte de
  terminar**, não é extra opcional.

## Sync deixa de ser obrigatório: o watcher já faz

O mandate do knowledge exigia: *"After ANY code change you MUST update the task log and run
sync via MCP — a task without docs + sync is NOT complete"*. E a skill dedicava um bloco
`### ⚡ MANDATORY: Sync After Every File Modification`, com *"esquecer de chamar sync é
violação de integridade do framework"*.

**Isso está obsoleto.** O daemon observa a árvore de docs e reconstrói o wiki sozinho. A
instrução mandava o agente duplicar trabalho que já acontece e, numa árvore grande, esperar por
um rebuild que não precisava.

Não é lacuna na skill — é ordem errada no mandate, que é a direção oposta da que eu tinha
suposto ao mapear as ferramentas.

O bloco virou uma tabela de **quando o watcher não pode ter visto**: daemon parado, mudança
vinda de fora da máquina (pull, checkout, restore), ou busca devolvendo coisa velha minutos
depois da edição — aí sim `sync`, e o erro dele é o sinal.

**O que continua obrigatório é o registro.** Mudança sem seu log de tarefa é incompleta; essa
obrigação é sobre escrever a documentação, não sobre reindexá-la.

## Divisão de papéis, agora explícita

- **Mandate = gatilho.** Quando abrir a skill. Curto.
- **Skill = ensino.** Como fazer. Pode cobrir `sync` como ferramenta de exceção sem que o
  mandate a exija.

## O que NÃO foi feito

O levantamento de cobertura de ferramentas está pronto e verificado, mas as lacunas seguem
abertas — as skills não citam parte das ferramentas dos próprios módulos:

| módulo | ferramentas do módulo ausentes do conteúdo da skill |
|---|---|
| ast | `ast_list`, `ast_index`, `ast_export`, `ast_embed` |
| hub | `hub_search`, `hub_submit`, `hub_projects`, `hub_uninstall` |
| knowledge | `knowledge_list`, `knowledge_schema`, `knowledge_lint`, `knowledge_export` |
| memory | `memory_export`, `memory_remove`, `memory_schema` |
| improvements | `improvements_rules` |

`hub_search` é a mais grave: o mandate manda "checar o Hub via MCP" e a skill nunca ensina a
ferramenta de busca.

E três domínios **não têm skill nenhuma**: `config` (4 ferramentas), `daemon` (2), `dream` (5).
Os mandates agora nomeiam as ferramentas de cada módulo, o que reduz o dano, mas não substitui
o conteúdo.

Suíte completa com `-race` limpa, `golangci-lint` limpo.
