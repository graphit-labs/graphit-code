---
title: Rust Copilot one-shot Knowledge Ask
doc_type: spec
description: Contrato Copilot CLI JSONL com custódia Process, correlação nativa, limites e fixtures locais.
references:
  - target: {type: task, id: tsk-625d}
    relation: implements
  - target: {type: task, id: tsk-4c72}
    relation: implements
  - target: {type: task, id: tsk-7f7c}
    relation: implements
  - target: {type: task, id: tsk-cab0}
    relation: relates_to
  - target: {type: session, id: ses-c98a}
    relation: relates_to
---

# Rust Copilot one-shot Knowledge Ask

A integração Rust executa o Copilot em modo prompt, com stdin e saída JSONL.
`tsk-625d` substitui o transporte SDK/TCP de `tsk-7f7c` e `tsk-4c72`.
Os recibos dessas entregas continuam históricos; não qualificam o runner CLI.
O runner passou nove testes focais em Linux; a suíte de armazenamento
Conversation passou 42, incluindo os três casos Copilot. A fixture local protege
o contrato sem exigir executável Copilot, conta, autenticação ou plano.
O Clippy focal AI passou com `-D warnings`. A qualificação dos consumidores
App permanece separada.

## Seleção e fronteira do processo

A seleção capturada de `ai.cli`, programa, ambiente, cwd e modelo pertence ao
consumidor. O runner recebe `CopilotRequest { command, session, model, prompt }`
e usa o `ProcessHost` existente. Programa ausente mantém o erro real. Não há
shell, download, ou descoberta de outra família.

O runner acrescenta argumentos separados:

~~~text
--output-format=json --no-ask-user --available-tools= --model=<modelo literal>
~~~

O prompt vai exclusivamente por stdin, limitado antes do spawn. Resume acrescenta
`--resume=<UUID completo canônico>`; prefixos e nomes são rejeitados. O runner
não usa `--session-id`, que pode criar uma sessão ausente. Create recebe a identidade
que o resultado nativo observou. Resume exige igualdade literal dessa identidade;
sessão inexistente, resultado ausente ou identidade diferente falham sem Create.

A allowlist vazia de ferramentas preserva a política documental. Não habilita
`--allow-all` nem promete isolamento do sistema operacional ou controle de hooks
do perfil nativo. Eventos de ferramentas que o CLI emitir continuam disponíveis
ao consumidor interno, sujeitos aos limites e à política da superfície pública.

## Proveniência do contrato CLI

A [referência programática oficial](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference)
documenta stdin e `--output-format=json`. A
[referência de comandos](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)
distingue `--resume` de `--session-id`.

O formato terminal foi conferido estaticamente no `package/app.js` publicado
no [pacote oficial Copilot CLI 1.0.51](https://registry.npmjs.org/@github/copilot/-/copilot-1.0.51.tgz).
O emissor `XKo` encaminha eventos nativos e suprime `session.idle` e
`session.shutdown`; o emissor `eJo` escreve:

~~~json
{"type":"result","timestamp":"2026-10-18T00:00:00Z","sessionId":"12345678-1234-4234-8234-123456789abc","exitCode":0,"usage":{"premiumRequests":0,"totalApiDurationMs":0,"sessionDurationMs":0,"codeChanges":{"linesAdded":0,"linesRemoved":0,"filesModified":[]}}}
~~~

Esse terminal não contém texto final nem `success`. Ele antecede o save nativo;
um save pode falhar depois de `exitCode:0`. A inspeção do predicado `r0r` e do
parser confirma que `--available-tools=` fornece uma allowlist presente sem
nomes de ferramentas habilitados. Essa evidência de versão 1.0.51 não certifica
um CLI 1.0.98 instalado ou uma conta do fornecedor. As fixtures modelam o contrato
inspecionado; não iniciam um servidor RPC fictício.

## Correlação e entrega

`user.message.id` identifica a origem do prompt. A resposta candidata deve ser
`assistant.message` da origem e do `turnId` principais atuais, com `messageId`
válido. Somente o último chunk de uma mensagem fracionada pode contribuir.
Deltas, trechos de raciocínio, ferramentas, outro prompt e agentes filhos não
constituem a resposta. O encerramento `assistant.turn_end` deve corresponder ao
turno atual; texto vazio é erro.

O terminal `result` confirma o UUID e requer `exitCode:0`. Um `session.error`
principal fatal rejeita a resposta; `model_call` transitório e erros de filhos
ficam privados. Não há retry Graphit. Nem `session.idle` nem `session.shutdown`
são requisitos de conclusão do CLI.

O callback privado `session_bound` pode ocorrer ao observar `result`: ele não
é um ACK anterior ao prompt nem autoriza commit. A resposta só fica elegível após
status físico zero, stdout/stderr em EOF, cleanup completo, ausência de Remaining,
join e revalidação da autoridade e do Original. Um resultado válido seguido de
falha no save/processo não entrega sucesso.

## Cancelamento, autorização, limites e custódia

O runner requer o mesmo `Arc<SnapshotContext>` da admissão atual PROJECT
Knowledge e um deadline finito. Original e admissão são revalidados durante
os frames, nos callbacks e antes da entrega. Um módulo diferente ou Original
estrangeiro falha antes do spawn. O supervisor Process observa cancelamento e
deadline; o runner o aguarda até devolver sua custódia, sem descartá-lo por timeout.

`CopilotLimits` limita entrada, prompt, saída total e resposta. `CodexWireLimits`
limita frame, profundidade, nós, campos, arrays, strings e IDs. O auditor JSON
verifica os limites e chaves duplicadas antes de construir o evento. Fragments
UTF-8 podem atravessar reads; newline delimita frames e EOF pode completar o
último frame. JSON inválido ou truncado falha sem interpretar stdout como texto.
O Process aplica também seus limites independentes de stdin/stdout/stderr.

`CopilotReceipt::process()` e `process_mut()` preservam o `Outcome` ou `Failure`
real, inclusive Remaining. Falhas expõem o mesmo recibo por `receipt_mut()`.
O consumidor transfere essa custódia ao owner existente antes de operações
posteriores de persistência; não cria outra árvore de processos ou shutdown host.

## Dados privados e projeções

Debug de seleção, recibo e falha não inclui prompt, seletor ou evento nativo.
Stderr não é publicado como resposta ou log do runner. O sink interno não recebe
`result`, `session.start`, `session.resume` ou `session.error`. Terminal, erros e
seletor confirmados permanecem acessíveis no recibo privado aos consumidores que
precisam deles. Eventos de progresso internos ainda podem conter IDs, conteúdo e
argumentos; cada consumidor deve construir sua projeção pública apropriada.

## Incoming Conversation Copilot

O armazenamento e a política de Conversation ficam nos contratos
[AI Conversation](rust_knowledge_conversation.md) e
[Incoming App](rust_knowledge_conversation_app.md). A seleção é capturada uma vez.
Cada turno requer autoridade USER/PROJECT atual e a mesma seleção literal.
Create, continue e cold reopen usam o UUID observado; falha de Resume preserva a
identidade comprometida sem escolher outro programa ou criar sessão substituta.
`conversation_id` é a identidade Graphit pública; o seletor nativo fica privado.
List/delete gerenciam metadata Graphit; native clear permanece específico de Kiro.
Os consumidores App têm validação própria e não herdam um PASS de runner.

## Verificação focal e fontes

Em `tsk-625d`, os nove casos CLI passaram em 2,143 s e os 42 casos de
`knowledge_conversation` em 2,178 s de execução, excluindo compilação. A primeira
execução falhou no setup da fixture Auth: faltava o owner_root entre os
allowed_roots. A correção ficou somente na fixture. Os recibos privados da
folha preservam essa falha e as reexecuções; não houve mudança no produtor Auth.
O nono caso protege cancelamento e deadline de um callback bloqueado durante
um frame normal e no fragmento final de EOF. Clippy AI com `-D warnings` passou
para biblioteca, fixture e os dois alvos de integração; seus 840,088 s incluem
espera pelo lock e checagem, sem separação dessas fases.

O alvo `cli_copilot` executa a fixture como filho nativo real e cobre Create e
cold Resume, correlação, eventos de ferramentas, resultado ausente ou diferente,
falha de save após result0, erros, UTF-8 fragmentado, EOF final, limites, falha do
sink, cancelamento, deadline e rejeição de autoridade antes do spawn. As suítes
App exercitam suas superfícies e persistência separadamente. Não existe mais
um seletor de teste do router SDK nem um filtro unit vazio.

~~~text
cargo xtask test graphit-ai/cli_copilot graphit-app/knowledge_copilot_ask
~~~

- [Runner CLI](../../crates/graphit-ai/src/cli_copilot.rs)
- [Testes do runner](../../crates/graphit-ai/tests/cli_copilot.rs)
- [Fixture CLI compartilhada](../../crates/graphit-ai/tests/support/copilot_peer.rs)
- [Seleção e correlação Conversation](../../crates/graphit-ai/src/knowledge_conversation.rs)

Compatibility tests use local fixtures and do not require an authenticated vendor account. They do not by themselves establish native Windows/macOS execution or compatibility with every future supplier release.
