---
title: Deterministic module enforcement and instruction optimization
status: completed
created: 2026-09-02
updated: 2026-09-02
tags: [adapters, hooks, mandates, skills, determinism, tokens]
---

# Deterministic module enforcement and instruction optimization

## Objective

Auditar todos os mandates e skills dos módulos Graphit e mover para hooks nativos dos adapters tudo o que puder ser executado deterministicamente. O resultado deve minimizar instruções residentes e conteúdo carregado sob demanda, preservar nos mandates apenas gatilhos indispensáveis e manter nas skills somente decisões/procedimentos que realmente exigem raciocínio do agente.

A implementação parte do mecanismo de hooks já existente, das decisões registradas sobre ownership por adapter e da regra de não preservar compatibilidade prematura. Primeiro será formada uma matriz entre eventos desejados e hooks oficialmente suportados por cada agente; em seguida será definido um contrato por capacidade, não por falsa uniformidade entre IDEs. Alternativas rejeitadas: reforçar as mesmas obrigações com mais prosa (continua probabilístico e custa contexto) e centralizar comportamento específico no adapter base (viola o ownership já decidido).

## Plan & Task Breakdown

- [x] **T1 — Inventariar instruções e superfícies de execução** — Spec: mapear todos os mandates/skills gerados, seus gatilhos, duplicações, contradições e ações mecanizáveis; mapear também os hooks e o ciclo de vida atuais de todos os adapters. Done = nenhuma instrução ou adapter suportado fica fora da matriz.
- [x] **T2 — Verificar hooks oficiais por agente** — Spec: consultar documentação primária atual de cada agente/IDE suportado e registrar eventos, payloads, capacidade de bloquear/injetar contexto, escopo e limitações. Done = cada conclusão possui fonte oficial e data, sem inferir APIs de memória.
- [x] **T3 — Definir a política de enforcement** — Spec: classificar cada obrigação como hook determinístico, validação/teste, mandate residente ou skill sob demanda; priorizar hooks em início/fim/retomada/antes de tool use sem prometer garantias que o host não oferece. Done = matriz de responsabilidade e orçamento de contexto aprováveis pelo código.
- [x] **T4 — Implementar enforcement nos adapters** — Spec: alterar cada adapter concreto e helpers neutros necessários, preservando configuração do usuário, idempotência, escopo local ao projeto e cleanup simétrico. Done = testes cobrem install/sync/remove e os eventos disponíveis em todos os hosts.
- [x] **T5 — Reescrever mandates e skills** — Spec: reduzir mandates a roteamento curto e acionável; organizar skills para leitura just-in-time, remover redundância, conhecimento genérico e procedimentos agora garantidos por hooks; manter proteção MCP-first onde não houver enforcement técnico. Done = conteúdo menor, sem perda de cobertura das ferramentas nem conflitos internos.
- [x] **T6 — Validar comportamento, tamanho e regressões** — Spec: adicionar testes estruturais/de integração para eventos, ownership, conteúdo gerado e orçamento de tokens; executar suites focadas e completas relevantes. Done = hooks e artefatos gerados são idempotentes, válidos e mensuravelmente menores.
- [x] **T7 — Atualizar documentação e memória** — Spec: atualizar arquitetura/decisões, este log, registrar decisões persistentes e executar um `graphit_sync` final. Done = outro agente consegue continuar somente a partir dos registros e todos os índices refletem o estado final.
- [x] **T8 — Garantir enforcement em subagentes** — Spec: verificar por host como subagentes herdam instruções e MCP tools, identificar eventos/limites específicos e implementar bootstrap ou reinjeção adicional onde o host permitir. Done = nenhum adapter afirma uma garantia maior que sua API; testes provam configuração e payload de subagentes.
- [x] **T9 — Documentar o modelo de visibilidade de tools** — Spec: separar claramente três camadas — instruções herdadas, tools disponibilizadas pelo host e enforcement de uso — e registrar as limitações não controláveis pelo Graphit. Done = a arquitetura explica quando uma tool pode ser vista/usada por um subagente e qual fallback cobre cada lacuna.
- [x] **T10 — Corrigir indisponibilidade para fallback nativo** — Spec: substituir o comportamento fail-closed por fallback explícito às tools padrão do agente quando Graphit não estiver disponível, mantendo Graphit-first quando estiver; remover bloqueios que impedem o fallback e preservar somente guards capazes de distinguir disponibilidade. Done = subagentes sem Graphit continuam trabalhando com tools nativas, sem instruções, testes ou documentação contraditórios.
- [x] **T11 — Reconciliar hooks durante todo sync de IDE** — Spec: garantir que o mesmo fluxo que atualiza a configuração MCP também reconcilie os hooks nativos do adapter selecionado, removendo entradas Graphit obsoletas e preservando entradas do usuário. Done = install/sync atualizam MCP e hooks juntos de modo idempotente em todos os adapters, com testes que falham se uma das duas superfícies ficar desatualizada.

## Implementation Details

- O inventário encontrou quatro geradores de mandate/skill (`ast`, `hub`, `knowledge`, `memory`) e sete adapters com lifecycle concreto próprio.
- A pesquisa oficial confirmou eventos de início em todos os adapters e pontos adicionais de retomada, compactação, subagente, pré-tool ou pré-invocação conforme o host.
- O bootstrap agora pode receber conteúdo obrigatório já lido pelo hook. O comando oculto consulta diretamente as tabelas autoritativas dos escopos de projeto e usuário; se essa leitura falhar, mantém o protocolo MCP anterior como fallback explícito.
- `CoreInvariant` separa o roteamento MCP-first do procedimento de bootstrap, permitindo reinjeção barata sem repetir toda a skill.
- Claude e Codex reinjetam o bootstrap em subagentes; Gemini reaplica o invariante em `BeforeAgent`; Kiro cobre `SessionStart` (IDE) e `AgentSpawn` (CLI); Antigravity reaplica o invariante em toda `PreInvocation`; OpenCode carrega a memória via processo local, injeta o invariante em toda chamada de modelo e o inclui na compactação.
- Os quatro conteúdos gerados foram substituídos por skills decisórias compactas. Schemas de tools deixaram de ser duplicados; permanecem fluxos, precedência, fallback, freshness e operações administrativas. Mandates agora contêm apenas roteamento, request-shapes concretos e um pequeno conjunto de tools de entrada.
- Os adapters não bloqueiam shell nem ferramentas nativas: os hooks não conseguem provar que o MCP foi exposto ao agente atual. Graphit é preferido quando disponível; indisponibilidade, formato não suportado ou conteúdo local não indexado liberam as tools padrão do host.
- O contrato reinjetado cobre todo agente/subagente e instrui fallback nativo imediato quando a tool Graphit necessária não existe naquele contexto.
- Cursor usa `preToolUse(Task)` para tentar anexar um protocolo autocontido ao prompt isolado. O hook não é `failClosed` e não há gate em `subagentStart`; falha de reescrita ou ausência do MCP preserva o spawn nativo.

## Use Cases

### UC-01: Instalar enforcement compatível com o agente
- **Actor**: processo de instalação/sync do Graphit.
- **Preconditions**: um adapter suportado está selecionado e a configuração do projeto é gravável.
- **Main Flow**:
  1. O adapter identifica somente as capacidades nativas do agente que representa.
  2. Instala hooks Graphit nos eventos capazes de executar a obrigação correspondente.
  3. Mantém mandates/skills apenas para as lacunas que exigem decisão do modelo.
- **Alternative Flows**:
  - Um host sem determinado evento mantém a obrigação mínima como instrução explícita.
- **Error Scenarios**:
  - Configuração inválida ou não gravável falha sem destruir entradas do usuário.
- **Postconditions**: o máximo possível do comportamento Graphit é acionado pelo runtime do host, não pela lembrança do agente.
- **Affected Files**: a determinar em T1.

### UC-02: Carregar instrução Graphit somente quando necessária
- **Actor**: agente atendendo uma solicitação no projeto.
- **Preconditions**: os artifacts do adapter foram sincronizados.
- **Main Flow**:
  1. Um mandate curto reconhece um gatilho não mecanizável.
  2. O agente abre apenas a skill do domínio correspondente.
  3. A skill ensina o fluxo mínimo e as ferramentas necessárias para a ação atual.
- **Alternative Flows**:
  - Uma ação inteiramente coberta por hook não consome conteúdo duplicado do mandate/skill.
- **Error Scenarios**:
  - Uma capacidade ausente no host permanece declarada como limitação e não como garantia falsa.
- **Postconditions**: contexto residente e carga sob demanda são menores, sem perder o roteamento MCP-first.
- **Affected Files**: a determinar em T1.

## Test Cases & Acceptance Criteria

### Feature: Enforcement determinístico por capacidade
Ref: UC-01

#### Scenario: Instalação idempotente preserva configuração do usuário
```gherkin
Given um projeto com configuração nativa e hooks pertencentes ao usuário
When o mesmo adapter Graphit é sincronizado duas vezes
Then cada hook Graphit compatível existe exatamente uma vez
  And todas as entradas pertencentes ao usuário permanecem inalteradas
```

#### Scenario: Ausência de evento não gera garantia falsa
```gherkin
Given um agente que não expõe um evento necessário
When o adapter Graphit é sincronizado
Then nenhuma configuração nativa inexistente é emitida
  And a obrigação residual aparece no menor artifact instrucional aplicável
```

### Feature: Instruções sob demanda com orçamento reduzido
Ref: UC-02

#### Scenario: Procedimento mecanizado não é duplicado
```gherkin
Given uma obrigação executada deterministicamente por hook
When mandates e skills são gerados
Then o mandate não repete o procedimento completo
  And a skill não ordena ao agente executar manualmente o evento já garantido
```

#### Scenario: Toda ferramenta continua alcançável
```gherkin
Given o inventário de ferramentas MCP de cada módulo
When a skill correspondente é gerada
Then cada ferramenta que exige julgamento continua documentada ou é explicitamente coberta por automação
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `docs/tasks/deterministic-module-enforcement-and-instruction-optimization.md` | Created | Abrir a tarefa antes de qualquer alteração de implementação e registrar seu plano. |
| `internal/sessionhook/sessionhook.go` | Modified | Separar invariante compacto, suportar memória obrigatória já carregada e novos formatos de lifecycle. |
| `cmd/graphit/commands/session_hook.go` | Modified | Ler memórias obrigatórias de projeto e usuário antes de renderizar o contexto do hook. |
| `internal/hub/adapters/ide/{claude,codex,gemini,kiro,opencode}.go` | Modified | Usar os limites de lifecycle oficialmente disponíveis para bootstrap, subagentes, retomadas e compactação. |
| `internal/{ast,hub,knowledge,memory}/rule_compact.go` | Created | Substituir skills e triggers residentes por versões just-in-time menores e orientadas a decisão. |
| `internal/{ast,hub,knowledge,memory}/rule.go` | Modified | Remover os manuais gerados antigos e manter somente lifecycle e frontmatter das skills compactas. |
| `internal/hub/adapters/ide/mandate_compact.go` | Created | Reduzir preâmbulo e renderer de triggers sem remover o contrato MCP-first. |
| `internal/hub/adapters/ide/mandate.go` | Modified | Remover os renderers antigos; parsing, ordenação e persistência continuam separados do conteúdo compacto. |
| `internal/hub/adapters/ide/session_hooks.go` | Modified | Permitir hooks agrupados com matcher nativo sem duplicar reconciliação/cleanup. |
| `internal/sessionhook/sessionhook_test.go` e testes compactos por módulo | Modified/Created | Verificar bootstrap executado, reinjeção, fallback, cobertura de tools e orçamento de conteúdo. |
| `docs/architecture/adapter-hook-enforcement.md` | Created | Registrar matriz oficial por host, fronteira hook/mandate/skill, garantias, limitações e orçamento. |
| `internal/hub/adapters/ide/{cursor,claude,codex,gemini,kiro,antigravity,opencode}.go` | Modified | Entregar o protocolo a subagentes quando o host permite e remover bloqueios que impediam o fallback nativo quando o Graphit não está disponível. |

## Trade-offs & Decisions

- Hooks serão tratados por capacidade/evento de cada agente; não será simulada uma API comum que os fornecedores não oferecem.
- A economia de tokens será medida sobre artifacts gerados e acompanhada de testes de cobertura, não inferida apenas por redução de linhas.
- Compatibilidade com formatos Graphit legados não será adicionada sem requisito explícito; configuração pertencente ao usuário continua sendo preservada.
- “Tool disponível” pertence ao runtime do host, não ao prompt. O Graphit configura os MCPs no escopo do projeto e usa os eventos disponíveis, mas uma allowlist explícita do usuário ou o ambiente cloud ainda pode removê-los. Nessa situação, o agente continua com as tools nativas que o host autorizou; fabricar uma tool ou alterar silenciosamente uma allowlist pertencente ao usuário foi rejeitado.

## Technical Debt

- Nenhum item identificado nesta mudança. Capacidades ausentes dos hosts estão registradas como limitações de adapter, não como dívida silenciosa.

## System Knowledge

- Já existe bootstrap determinístico de memória em todos os sete adapters, com ownership concreto por IDE e paths locais ao projeto.
- O mandate atual pretende ser o gatilho e a skill o procedimento, mas ambos ainda contêm políticas repetidas e algumas contradições históricas.

## Progress Log

### 2026-09-02
- Tarefa aberta antes de mudanças de implementação.
- Memória obrigatória e contextual consultadas; confirmadas as decisões de ownership por adapter, configuração local ao projeto e ausência de requisito de retrocompatibilidade.
- Wiki consultada; identificado o trabalho anterior de bootstrap de memória e a revisão histórica de skills/mandates como base, sem presumir que a auditoria atual já esteja coberta.
- Próximo passo: inventário AST de módulos, geradores, adapters, hooks e testes, seguido da verificação oficial de capacidades por agente.
- Inventário de geradores, adapters, helpers e testes concluído via grafo AST.
- Documentação oficial atual dos sete hosts verificada; a matriz de capacidades será registrada na documentação de arquitetura.
- Bootstrap alterado para executar a leitura obrigatória no hook em vez de apenas pedir ao agente que a faça; fallback continua funcional quando o storage não pode ser aberto.
- Lifecycle ampliado por adapter sem mover ownership para o adapter base; Antigravity passou a reinjetar por invocação por meio do novo comportamento do renderer comum.
- Mandates e skills reescritos como roteamento + decisão; referências extensas e justificativas repetitivas deixaram o contexto do agente.
- A primeira suíte focada compilou os novos geradores, mas revelou que dezenas de testes ainda congelam frases, seções e exemplos do manual anterior. Esses snapshots são agora especificação obsoleta; serão substituídos por testes do contrato compacto (cobertura decisória, tools alcançáveis, orçamento e lifecycle), não satisfeitos reintroduzindo prosa redundante.
- [Histórico, substituído por T10] Testes de snapshots verbosos foram substituídos por contratos de comportamento e tamanho; nesta etapa intermediária, um guard de descoberta nativa foi implementado e depois removido por não conseguir provar disponibilidade do MCP no subagente.
- Matriz de hooks e limitações documentada com fontes primárias atuais para todos os sete adapters.
- As quatro implementações antigas (mais de quatro mil linhas de manuais gerados) foram removidas; arquivos `rule.go` agora contêm somente lifecycle de instalação/remoção e as descrições curtas das skills.
- [Histórico, substituído por T10] A saída dos guards chegou a ser ajustada antes de os próprios guards serem removidos; as descrições de frontmatter continuam exercitando serialização YAML segura sem reintroduzir prosa.
- A suíte completa `go test ./...` passou. Os artifacts reais foram regenerados e validados como JSON/JavaScript; um teste injetável adicional cobre a leitura determinística dos dois escopos autoritativos e o fallback quando o backend não abre.
- Medição final: skills Codex 227.799 → 11.154 bytes (-95,1%); skills + `AGENTS.md` 248.054 → 15.209 bytes (-93,9%). Os artifacts equivalentes dos adapters configurados são byte a byte consistentes.
- Renderers legados restantes removidos de `mandate.go`; não há caminho alternativo capaz de regenerar o manual antigo.
- Otimização de lifecycle: Antigravity só abre as tabelas obrigatórias na invocação zero; OpenCode resolve o bootstrap no primeiro turno de cada `sessionID`, preservando freshness para sessões criadas depois que o plugin já foi carregado.
- Decisão durável incorporada à memória de ownership dos adapters. Documentação de arquitetura e task log concluídos; suíte integral passou após a última alteração de código.
- `graphit_sync` final concluiu e atualizou os índices. Como o processo MCP desta sessão foi iniciado antes da alteração dos geradores, ele materializou uma vez o conteúdo antigo; os artifacts foram imediatamente regenerados com o binário da worktree atual e revalidados. Esse efeito é restrito ao processo de desenvolvimento carregado, não ao código distribuído.
- Tarefa reaberta após a solicitação de garantir o mesmo enforcement para subagentes. O próximo passo é auditar herança de instruções, exposição das MCP tools e lifecycle de subagente separadamente em cada host; `SubagentStart` já cobre Claude/Codex, mas isso não prova por si só que todos os hosts entregam as mesmas tools ao processo filho.
- Auditoria oficial de subagentes concluída. Confirmado que contexto, registro de tools e enforcement são camadas distintas; configurações customizadas podem restringir tools mesmo quando o MCP está instalado no projeto.
- [Histórico, substituído por T10] O primeiro protocolo específico de subagente usava fail-closed e handshake `Task` → `subagentStart`; a suíte focada passou, mas a política foi corrigida pelo usuário para fallback nativo.
- [Histórico, substituído por T10] O mandate chegou a instruir interrupção quando a MCP tool estivesse ausente; agora instrui continuidade com as tools padrão do agente.
- A arquitetura agora separa entrega de instrução, visibilidade de tools e enforcement, incluindo allowlists customizadas, MCP de time no Cursor Cloud e a janela anterior ao carregamento de hooks do projeto.
- [Histórico, substituído por T10] A memória arquitetural registrou temporariamente o handshake fail-closed e será revisada preservando o modelo de três camadas com fallback nativo.
- A suíte integral `go test ./...` e `git diff --check` passaram após as alterações de subagente. JSON dos hooks materializados e o plugin JavaScript do OpenCode também foram validados.
- `graphit_sync` final concluiu com wiki/índices atualizados. Como o processo MCP carregado ainda continha os geradores anteriores, os artifacts instalados foram rematerializados em seguida com o binário da worktree atual. As tentativas CLI de reindexar knowledge avisaram que esse binário de desenvolvimento não tinha a tag `lancedb`; isso não invalidou o sync MCP já concluído nem a materialização dos adapters.
- Tarefa reaberta por correção do usuário: indisponibilidade do framework não deve bloquear o subagente. A política correta é Graphit-first com fallback às ferramentas nativas padrão do próprio host; todos os trechos fail-closed introduzidos em T8/T9 precisam ser removidos ou reformulados.
- Implementação corrigida: `CoreInvariant` e o mandate instruem fallback nativo imediato quando a tool Graphit não está disponível; o Cursor continua injetando o protocolo em `Task`, mas sem `failClosed` e sem gate de `subagentStart`.
- Guards nativos removidos dos sete adapters porque os payloads de hook não provam a disponibilidade real do MCP no contexto filho. O sync remove os hooks bloqueadores previamente materializados; OpenCode deixa de interceptar `bash`/`grep`/`glob`/`list`.
- A skill AST agora permite descoberta nativa quando a tool Graphit estiver ausente ou quando o conteúdo local não for suportado/indexado, sem marcador especial de shell.
- Teste de migração cobre os sete adapters: uma sincronização remove hooks bloqueadores previamente materializados, preserva o bootstrap e mantém no Cursor apenas a injeção não bloqueante de `Task`. Suítes focadas passaram.
- O contrato compacto da skill AST ganhou uma asserção explícita para o fallback quando a tool Graphit necessária estiver indisponível, preservando a exceção de contextos importados. Suítes focadas de hooks, adapters, comandos e AST passaram após a correção.
- Validação final concluída: `go test ./...` e `git diff --check` passaram; JSON dos hooks e JavaScript do OpenCode são válidos; nenhum artifact materializado contém os antigos guards/fail-closed; e as skills dos cinco adapters materializados são byte a byte equivalentes.
- `graphit_sync` concluiu. O processo MCP carregado materializou uma revisão anterior dos geradores, então os cinco adapters presentes na worktree foram imediatamente sincronizados outra vez com o binário atual. Os avisos `built without the lancedb tag` do binário local ficaram restritos à etapa de knowledge index; o sync MCP autoritativo já havia concluído.
- Tarefa reaberta por correção do usuário: hooks devem ser reconciliados em toda sincronização de IDE junto com o MCP, e não apenas na instalação inicial ou em uma etapa independente. T11 auditará o fluxo real e adicionará cobertura acoplada por adapter antes de alterar a implementação.
- Auditoria confirmou que `SyncIDEAdapter` já alcançava o `Sync` concreto — e, portanto, MCP e hooks — mas o writer MCP genérico e o `graphit_sync` via MCP descartavam erros. Esses erros agora são propagados; a descrição e o progresso do sync nomeiam explicitamente as duas superfícies.
- Adicionado teste integrado de `SyncIDEAdapter` para os sete adapters: após trocar o caminho do executável Graphit entre duas sincronizações, tanto o arquivo MCP quanto o arquivo de hooks devem conter o valor novo e remover o antigo.
- O CLI `graphit sync` e o sync executado durante `init` agora retornam erro quando a reconciliação MCP/hooks falha, em vez de apenas imprimir uma etapa com falha e terminar como sucesso. As demais etapas locais ainda concluem antes de o erro ser devolvido.
- Um teste de falha impede regressão no writer genérico: erro ao reconciliar o arquivo MCP precisa subir pelo `Sync`, garantindo que o lifecycle não anuncie atualização conjunta quando apenas parte dela ocorreu.
- T11 concluída. Suítes focadas e `go test ./...` passaram; `git diff --check` também passou. O contrato arquitetural agora declara explicitamente que todo sync reconcilia artifacts, MCP e hooks como uma unidade e que falhas são devolvidas ao chamador.
- `graphit_sync` autoritativo concluiu e os cinco adapters materializados nesta worktree foram sincronizados novamente com o binário atual; a saída confirma “IDE MCP and hooks synced”. JSON/JavaScript dos hooks foram revalidados. O binário local sem a tag `lancedb` avisou apenas sobre seus indexadores AST/knowledge; os índices autoritativos já haviam sido atualizados pelo MCP.
