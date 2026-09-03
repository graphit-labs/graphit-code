---
title: Adapter hook enforcement
type: architecture
status: active
updated: 2026-09-03
tags: [adapters, hooks, mandates, skills, enforcement]
---

# Adapter hook enforcement

## Objetivo

O Graphit usa hooks para executar garantias observáveis e mantém instruções somente para decisões que exigem interpretação. A fronteira é deliberada:

- **hook**: evento e entrada são objetivos; a ação pode ser executada ou bloqueada sem julgamento;
- **mandate**: roteador residente, composto dinamicamente pelo hook apenas para módulos habilitados;
- **skill**: ensina o fluxo decisório apenas depois que o domínio se torna relevante;
- **schema da tool**: continua sendo a referência de argumentos; não é copiado para a skill.

Mais prosa não transforma uma obrigação em garantia. Da mesma forma, um hook não deve bloquear trabalho legítimo com uma classificação que ele não consegue provar.

## Garantias executadas pelos hooks

### Bootstrap de memória

O comando oculto `_session-hook` lê diretamente as tabelas autoritativas de memória dos escopos `project` e `user`. O conteúdo obrigatório é injetado no primeiro contexto do agente; portanto, a chamada inicial de `graphit_memory_mandatory` não depende mais do modelo. Se a tabela não puder ser aberta, o payload volta ao protocolo MCP e declara a chamada necessária.

A busca contextual permanece semântica: a skill manda pesquisar o pedido atual, escolher pelos títulos e ler somente as páginas relevantes. O hook não escolhe memórias por score como se relevância fosse uma certeza mecânica.

### Contexto residente dinâmico

No mesmo evento, `_session-hook` resolve a raiz em runtime a partir do campo nativo do host: `cwd` em Claude, Codex, Gemini e Kiro; `workspace_roots` no Cursor; e `workspacePaths` no Antigravity. A partir de cada candidato, sobe até o lockfile Graphit mais próximo; o cwd do processo fornece o último candidato. OpenCode inicia o subprocesso com seu `directory` runtime como cwd. `.git` nunca define uma raiz Graphit: projetos sem Git funcionam pelo lockfile, e a ausência do lockfile deixa a raiz não resolvida e ativa apenas o fallback compacto. Nenhum checkout absoluto da máquina que executou o sync é serializado no hook. O flag `--project-dir` permanece somente como ponto inicial explícito para diagnóstico e também precisa alcançar um lockfile.

Depois da resolução, o comando lê a configuração e o lockfile desse projeto. Ele compõe, em ordem estável, somente os mandates dos módulos habilitados e os corpos dos artifacts Hub de tipo `rule` instalados. Rules são lidas no artifact autoritativo (`RULE.md`), inclusive links locais; não são copiadas para diretórios de rules da IDE.

O Graphit não cria nem atualiza `AGENTS.md`, `CLAUDE.md` ou equivalentes para entregar essas instruções. Esses arquivos, quando existem, pertencem ao usuário. Skills continuam físicas nos diretórios nativos porque os hosts precisam descobri-las e carregá-las sob demanda.

Agentes externos podem recuperar somente os mandates globais com `graphit_mandates`, sem parâmetros. A tool não resolve projeto nem lê lockfile; em cada chamada, o schema canônico de config resolve ambiente, configuração global e defaults, e o mesmo builder do hook aplica os overrides globais de rules. Memórias obrigatórias, instruções de bootstrap, rules instaladas do Hub e configuração de projeto não fazem parte desse retorno.

### Reinjeção de invariantes

O contexto residente completo é reservado para início real de sessão ou subagente e para a reconstrução excepcional após compactação. Limites recorrentes de prompt ou invocação recebem somente `CoreInvariant`, o lembrete curto de precedência Graphit-first; eles não reconstroem nem repetem memória obrigatória, mandates, rules ou o bootstrap inicial. Limites pós-ação recebem somente `UnitCompletionReminder`. Se a MCP tool exigida não estiver disponível no agente atual, ele continua com suas tools nativas padrão. A única substituição proibida é chamar o CLI do Graphit como se fosse MCP.

Retomar, reentrar ou continuar trabalho interrompido reaplica essa precedência antes da próxima ação. O hook só reinsere o roteador: a classificação do domínio continua a cargo do agente, que carrega a skill correspondente apenas quando o próximo passo encontra um trigger.

### Checkpoint de tarefa e finalização

O Graphit trata a menor unidade semanticamente reportável como um checkpoint, não como sinônimo mecânico de qualquer tool call. Nos eventos pós-ação disponíveis, o hook pede ao agente que decida se a unidade terminou e, em caso positivo, atualize imediatamente o gerenciador ativo e o task log com o que foi entregue e o próximo passo. Kiro também possui `PostTaskExec`, que fornece um boundary objetivo de task de spec.

Depois da última atualização de tarefa, o evento final dispara `graphit sync` em segundo plano. `_session-hook --sync` inicia o executável Graphit ativo com o argumento `sync`, libera o processo filho e devolve imediatamente o payload nativo de conclusão; não espera indexação, lock ou processo terminar. Falha ao iniciar o dispatcher é erro do hook, mas a sincronização já iniciada não controla nem atrasa a resposta final do agente.

### Subagentes: três garantias diferentes

Um subagente só está corretamente coberto quando três camadas independentes estão satisfeitas:

1. **Entrega da instrução** — o contexto isolado recebe o protocolo Graphit. Herança da conversa ou do arquivo de regras nunca é presumida quando o host oferece um boundary próprio.
2. **Visibilidade da tool** — o runtime inclui os servidores MCP Graphit no registro de tools do filho. Essa camada pertence ao host e pode ser reduzida por `tools`, `disallowedTools`, permissões, `includeMcpJson` ou configuração cloud.
3. **Roteamento de uso** — hooks entregam a preferência Graphit-first. Uma tool não exposta não pode ser criada por um prompt; por isso a ausência libera as ferramentas padrão do host em vez de bloquear o trabalho.

`SubagentProtocol` é autocontido e marcado por `GRAPHIT_SUBAGENT_PROTOCOL_V1`. Claude e Codex o recebem em `SubagentStart`. Cursor não permite contexto adicional nesse evento, então o adapter tenta injetar o protocolo no input de `Task` por `preToolUse`. Esse hook é deliberadamente fail-open: se a versão do host não aplicar `updated_input`, o filho ainda nasce e usa suas tools nativas.

O Graphit preserva allowlists de subagentes pertencentes ao usuário. Alterá-las silenciosamente poderia conceder acesso que foi removido de propósito. Quando uma allowlist exclui Graphit, o subagente mantém o trabalho com as tools permitidas pelo host.

Há um limite externo incontornável: quando o host exige confiança ou permite desabilitar hooks, um arquivo do próprio projeto não pode aprovar a si mesmo. Cursor Cloud também executa turnos exploratórios somente leitura antes de carregar hooks do repositório. Nenhum arquivo do projeto consegue aplicar Graphit-first antes de o próprio host carregá-lo; nesse intervalo, o agente opera normalmente com as capacidades padrão. Para usar Graphit também no cloud, o MCP precisa estar configurado na camada de time/enterprise do host. As ações concretas de confiança, ativação, reload e verificação são documentadas separadamente para cada adapter em [Activate Graphit Hooks in Each Agent](../guides/agent_hook_activation.md); uma ressalva arquitetural genérica não substitui instruções ao usuário.

### Fallback nativo

Os adapters não bloqueiam tools nativas. O payload de um hook de tool use não prova que `graphit_ast_*` está realmente exposto naquele agente ou subagente; negar `Grep`, `Glob`, `rg` ou equivalentes poderia impedir todo o trabalho. O mandate e a skill dão precedência ao Graphit quando disponível e autorizam as tools padrão quando ele não está.

Para código local suportado, a skill ainda orienta AST-first. Conteúdo não indexado, formato não suportado ou indisponibilidade da tool usam descoberta nativa diretamente. Contextos importados continuam sem fallback nativo porque seu source não está no workspace do agente.

## Matriz por adapter

### Ciclo de trabalho

| Adapter | Retomada/reinjeção | Menor unidade disponível | Finalização assíncrona |
|---|---|---|---|
| Claude Code | `SessionStart`/`SubagentStart` carregam o bootstrap; `UserPromptSubmit` reinjeta só o invariant compacto | `PostToolUse` pede avaliação e atualização imediata | `SubagentStop`, `Stop` e `SessionEnd` disparam sync |
| Codex | `SessionStart`/`SubagentStart` carregam o bootstrap; `UserPromptSubmit` reinjeta só o invariant compacto | `PostToolUse` pede avaliação e atualização imediata | `SubagentStop`, `Stop` e `SessionEnd` disparam sync |
| Cursor | `sessionStart`; `preToolUse(Task)` inicializa o filho | `postToolUse` pede avaliação e atualização imediata | `subagentStop`, `stop` e `sessionEnd` local disparam sync |
| Gemini CLI | `SessionStart` carrega o bootstrap; `BeforeAgent` reinjeta só o invariant compacto | `AfterTool` pede avaliação e atualização imediata | `AfterAgent` e `SessionEnd` disparam sync |
| Kiro | `SessionStart`, `UserPromptSubmit` e `AgentSpawn` | `PostToolUse` avalia a unidade; `PostTaskExec` cobre task de spec | `Stop` dispara sync |
| Antigravity | `PreInvocation` carrega o bootstrap na invocação zero e só o invariant nas seguintes | `PostInvocation` pede avaliação e atualização imediata | `Stop` dispara sync |
| OpenCode | transform de system por sessão e hook de compactação | `tool.execute.after` pede avaliação e atualização imediata | `session.idle` e `session.deleted` usam `Bun.spawn(...).unref()` |

Todos os disparos finais são fire-and-forget. O dispatcher usa APIs de processo do runtime; OpenCode usa array de argumentos, e os hosts cujo schema exige uma command string recebem o executável escapado conforme o sistema operacional. Não há script auxiliar dependente de shell, e a configuração gerada não contém paths do checkout; esse é o contrato comum para Linux, Windows e macOS.

Os eventos presentes na API não são tratados como cobertura automática. Antigravity oferece `PostToolUse`, mas esse evento aceita somente `{}` como saída e não consegue reinjetar a orientação de gerenciamento da tarefa; por isso o adapter usa `PostInvocation`, que suporta `injectSteps[].ephemeralMessage`, e não instala um `PostToolUse` vazio. Pelo mesmo critério, `beforeSubmitPrompt` do Cursor não substitui um boundary de reinjeção porque sua saída não oferece contexto adicional. Limitações nativas ficam explícitas, em vez de serem mascaradas por hooks sem efeito.

### Subagentes e visibilidade

| Adapter | Instrução no subagente | Visibilidade das MCP tools | Fallback e limite |
|---|---|---|---|
| Claude Code | `SubagentStart` injeta `SubagentProtocol`; não depende de `CLAUDE.md`, que alguns built-ins não carregam. | O filho herda as tools do pai, salvo filtros/background e `tools`/`disallowedTools` do agente customizado. | Uma allowlist que remova MCP deixa o filho usar suas tools nativas permitidas. |
| Codex | `SubagentStart` injeta `SubagentProtocol` como contexto de developer. | O adapter instala MCP no projeto; a disponibilidade no filho ainda depende da superfície Codex que realizou o spawn. | Sem Graphit no registro do filho, permanecem as tools padrão dessa superfície. |
| Cursor | `preToolUse(Task)` tenta injetar o protocolo sem bloquear o spawn. | Filho local herda todas as tools do pai. Filho cloud usa MCPs configurados para o time, não os MCPs locais. | Ausência de Graphit ou falha de reescrita mantém o subagente nativo; não há gate em `subagentStart`. |
| Gemini CLI | `BeforeAgent` reaplica o contexto residente em cada turno; o bootstrap principal vem de `SessionStart`. | Agente customizado sem `tools` herda o pai; uma lista explícita pode excluir `mcp_*`/`mcp_server_*`. | Configuração restrita continua com os built-ins configurados. |
| Kiro | Steering é compartilhado; `SessionStart` cobre IDE e `AgentSpawn` cobre CLI. | Subagentes compartilham MCP e permissões do projeto; perfil customizado pode desligar `includeMcpJson` ou declarar MCPs próprios. | Sem MCP do projeto, o perfil continua com suas tools nativas. |
| Antigravity | `PreInvocation` injeta bootstrap na invocação zero e contexto residente nas seguintes; a documentação não expõe um evento próprio de subagente. | Clones dinâmicos podem herdar toolset; agente estático controla `tools` e `mcpServers`, vazios por padrão. | Execuções fora dos hooks do projeto ou sem MCP continuam com o toolset definido pelo host. |
| OpenCode | `experimental.chat.system.transform` inicializa cada `sessionID`, incluindo sessões filhas, e `experimental.session.compacting` preserva o contexto residente na compactação. | A configuração MCP é do projeto, mas permissões específicas do agente podem negar tools MCP. | Permissão que negue MCP mantém as tools permitidas para esse agente; o plugin não bloqueia alternativas. |

Cada adapter concreto possui sync, remoção, formato e path do seu host. `FolderBasedAdapter` não conhece eventos nem formatos de hooks.

## Contrato de sincronização

`graphit sync` reconcilia cada adapter como uma única unidade de lifecycle: skills/commands/agents físicos, configuração MCP local ao projeto e hooks nativos da IDE. Isso ocorre em toda sincronização, não apenas no `init`. O writer substitui entradas Graphit anteriores pelo estado atual, preserva entradas pertencentes ao usuário e precisa ser idempotente. Artifacts `rule` ficam no cache/lockfile autoritativo e são consumidos no próximo hook; sync não os materializa na IDE.

Há dois contratos de espera distintos: uma invocação explícita de CLI ou MCP reporta seu próprio resultado ao chamador; a invocação automática de finalização apenas a dispara e não espera. Assim, o agente encerra sem ficar preso à indexação, mas todo boundary final suportado inicia uma tentativa completa de reconciliação.

Uma atualização parcial não pode ser anunciada como sucesso. Falhas de resolução ou escrita do MCP, assim como falhas de parsing ou escrita dos hooks, sobem por `SyncIDEAdapter`; tanto o CLI quanto a tool `graphit_sync` devolvem erro. O teste integrado troca o executável Graphit entre duas sincronizações e verifica, nos sete adapters, que MCP e hooks recebem o valor novo e descartam o antigo.

## O que permanece semântico

Não é seguro automatizar sem um modelo:

- construir uma consulta Cypher adequada, escolher fonte e avaliar impacto de edição;
- decidir qual artifact do Hub é relevante ou quando documentação externa é necessária;
- selecionar quais resultados de memória/wiki devem ser lidos;
- decidir se uma descoberta é durável, duplicada, contraditória ou merece promoção;
- produzir e manter um task log que explique objetivo, decisões, progresso e dívida;
- reconhecer se uma ação concluiu a menor unidade independentemente reportável antes de atualizar esse log;
- decidir quando freshness precisa ser provada antes de uma conclusão.

Esses itens permanecem nos mandates/skills, mas sem manuais de schemas, justificativas genéricas ou exemplos repetitivos.

## Modelo de orçamento

- O preâmbulo residente tem limite testado de 1.600 bytes e só aparece uma vez no contexto composto.
- Cada mandate de módulo tem limite próprio e contém apenas request-shapes + tools de entrada.
- Cada skill compacta tem um teto absoluto testado entre 5 e 6,5 KB; a geração real também é comparada ao baseline abaixo.
- Toda tool do módulo continua aparecendo uma vez no `Tool index`; detalhes de argumentos vêm do schema publicado pela própria tool.
- O bootstrap de memória e o contexto dinâmico completo ocorrem apenas no início real de sessão/subagente. Boundaries recorrentes de prompt/invocação recebem só `CoreInvariant`; checkpoints pós-ação recebem só `UnitCompletionReminder`. A compactação pode reconstruir contexto dinâmico porque representa perda efetiva de contexto, não um evento normal de cada turno.

### Resultado medido

Medição dos artifacts Codex antes/depois da sincronização desta mudança:

| Artifact | Antes | Depois | Redução |
|---|---:|---:|---:|
| `graphit-ast/SKILL.md` | 89.269 B | 3.013 B | 96,6% |
| `graphit-hub/SKILL.md` | 29.611 B | 2.190 B | 92,6% |
| `graphit-knowledge/SKILL.md` | 70.539 B | 2.941 B | 95,8% |
| `graphit-memory/SKILL.md` | 38.380 B | 3.010 B | 92,2% |

As quatro skills juntas caíram de 227.799 para 11.154 bytes (95,1%). O antigo artifact residente `AGENTS.md` deixou de existir: o custo agora é o contexto composto no evento nativo, com módulos desabilitados omitidos e sem uma segunda cópia do preâmbulo. Bytes são usados como métrica determinística de regressão; tokens variam conforme o tokenizer do host.

## Fontes oficiais verificadas

- [Claude Code hooks](https://code.claude.com/docs/en/hooks)
- [Claude Code subagents](https://code.claude.com/docs/en/sub-agents)
- [OpenAI Codex hooks](https://learn.chatgpt.com/docs/hooks)
- [Cursor hooks](https://prod.cursor.com/docs/hooks) e [Cursor subagents](https://prod.cursor.com/docs/subagents)
- [Gemini CLI hooks reference](https://geminicli.com/docs/hooks/reference/) e [Gemini CLI subagents](https://geminicli.com/docs/core/subagents/)
- [Kiro hooks](https://kiro.dev/docs/hooks/), [triggers](https://kiro.dev/docs/hooks/types/) e [Kiro subagents](https://kiro.dev/docs/chat/subagents/)
- [Google Antigravity hooks](https://antigravity.google/docs/ide/hooks/) e [Antigravity subagents](https://www.antigravity.google/docs/subagents/)
- [OpenCode plugins/hooks](https://opencode.ai/docs/plugins/) e [OpenCode agents](https://opencode.ai/docs/agents/)

As capacidades foram verificadas em 2026-09-03. Mudanças de fornecedor devem alterar primeiro esta matriz e seus testes de adapter; nunca devem ser simuladas por uma abstração comum que o host não suporta.
