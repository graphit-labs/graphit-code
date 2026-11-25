---
title: Rust Kimi REST/WS Ask and Conversation
doc_type: spec
description: Contratos Kimi para Knowledge Ask e Incoming Conversation, com custódia Process e sessão/prompt/turno nativos.
references:
  - target: {type: task, id: tsk-e6ff}
    relation: implements
  - target: {type: session, id: ses-c98a}
    relation: relates_to
---

# Rust Kimi REST/WS Ask and Conversation

Contrato para integradores de AI e Knowledge Ask. O consumidor REST/WS usa ProcessHost e é exercitado por peers locais controlados, sem exigir uma conta Kimi comercial.

## Superfície escolhida e versão

A inspeção usa Kimi Code main no commit
[ef727e517923cb312cb48d42825c49dfacd528e7](https://github.com/MoonshotAI/kimi-code/tree/ef727e517923cb312cb48d42825c49dfacd528e7),
package 2.2.0. Isso fixa o contrato de origem; não certifica o executável instalado,
autenticação, modelo ou runtime do fornecedor.

A [Server API oficial](https://www.kimi.com/code/docs/en/kimi-code-cli/reference/server-api.html)
é experimental. REST e WebSocket têm schemas da versão servida em OpenAPI/AsyncAPI;
a integração usa o contrato inspecionado, sem prometer compatibilidade com versões futuras.
Os envelopes possuem código de negócio próprio: HTTP bem-sucedido sozinho não prova sucesso.

ACP foi rejeitado para esta resposta one-shot: no contrato inspecionado, falha não
relacionada a Auth também pode produzir EndTurn e chunks locais de compaction não
identificam resposta principal. Print também emite texto de hook como assistant sem
origem discriminada. Nenhum desses canais é fallback para o resultado REST/WS.

## Inicialização e credenciais

O Service usa o executável, ambiente e cwd capturados. Os argumentos são separados,
sem avaliação de shell:

~~~text
web --no-open --host 127.0.0.1 --port 0 --log-level error
~~~

O nível error seleciona a única linha oficial de readiness. O default silent não
é o contrato escolhido. O token é separado da base antes de chamar url 2.5.8;
a credencial nunca entra no armazenamento de Url. A base deve ser canônica e
literal: HTTP com IP 127.0.0.1, porta não zero observada e caminho raiz, sem
userinfo, query ou fragmento residual. O texto retornado pelo parser deve ser
idêntico à base original; aliases numéricos ou caminhos normalizados não bastam.
Exige-se um único fragmento de token válido.

O token permanece em memória Zeroizing e header marcado Sensitive. Linha de
readiness, token, header e corpo privado de erro não pertencem a logs, receipts ou
documentação. A API pública expõe causas redigidas, preservando erros Process e de
transporte; o erro bruto do provider fica privado.

No Windows, o encerramento pode confirmar o processo principal sem confirmar a
árvore de descendentes. Kimi mantém essa distinção no recibo. Uma conclusão
bem-sucedida exige status de saída zero, EOF nos dois pipes, ausência de falhas de
controle e nenhum owner de limpeza pendente.

Erros nativos e terminais main failed/cancelled/blocked, com error ou interrupt,
são classificados antes do callback de progresso bruto. O payload da falha fica
privado e não vira texto principal. ACK não zero conserva o frame privado e o
código Native real. HTTP não 2xx conserva o status Http original e, quando
válido, o envelope JSON auditado em armazenamento privado. Falha ao inspecionar
esse corpo não troca o status HTTP por um erro JSON.

O owner Arc concreto fica registrado no Service antes de await ready, inclusive
na falha de spawn com Remaining retido. A execução usa ProcessHost,
spawn_service/run_service e o mesmo Original finito. Cancelamento/deadline para o
job; stop, observe, reap e shutdown conservam os resultados reais. Drop não prova
join, saída normal, EOF ou cleanup. Uma observação incompleta continua incompleta.

## Sessão, prompt e texto elegível

O consumidor reutiliza BoundedClient e WebSocket existentes.

`run_kimi` preserva a admissão tipada Knowledge: confere o módulo, revalida a
autoridade e exige o mesmo `Arc<SnapshotContext>` da requisição.
`run_kimi_guarded` compartilha o executor REST/WS e recebe um recheck concreto
do caller, além do mesmo Original finito. O caller conserva sua própria admissão;
Roles não fabricam `ProjectReadAdmission` Knowledge. O guard roda nas barreiras
existentes antes/depois de I/O e entrega; sua falha conserva a causa real e impede
resposta elegível. A separação não muda o owner de processo nem prova sua terminação.

`KimiRequest.model: Option<String>` segue o
[schema oficial do prompt](https://github.com/MoonshotAI/kimi-code/blob/ef727e517923cb312cb48d42825c49dfacd528e7/packages/kap-server/src/protocol/rest-prompt.ts#L16-L31):
o override é opcional e, quando presente, não pode ser vazio. `None` omite
a chave `model` do JSON, preservando a seleção/default nativo; não envia `null`,
não exige um nome inventado nem registra um modelo efetivo estimado.
A [rota oficial](https://github.com/MoonshotAI/kimi-code/blob/ef727e517923cb312cb48d42825c49dfacd528e7/packages/kap-server/src/routes/prompts.ts#L258-L268)
troca o modelo somente quando recebe esse campo. Ask e Conversation mantêm
`Some(model)` para sua seleção real explícita. Falhas reais do modelo/configuração
nativa continuam erros; Graphit não escolhe outro modelo silenciosamente.

O fluxo exige:

1. GET meta e POST session com metadata.cwd observado.
2. server_hello com protocolo 2 e ACK de subscribe antes de enviar o prompt.
3. POST prompt com texto JSON documental, modelo explícito opcional e permission_mode manual,
   sem produzir arquivo por execução.
4. prompt_id nativo retornado pelo POST; turn.started de main com promptId igual.
   Deltas e terminal devem conservar agentId e turnId exatos.

Hook result, compaction, ferramentas e subagentes são apenas progresso. O offset
externo é contado em unidades UTF-16 e validado antes do append. Cada
turn.step.started reinicia offset e texto; somente o texto do último step pode
ser candidato à resposta. Não se concatenam steps para fabricar uma conclusão.

A entrega exige completed sem error/interrupt, shutdown pela API oficial e,
no App, status 0, ambos EOFs, join/cleanup reais e recheck da admissão Knowledge.
Texto parcial, terminal isolado ou shutdown solicitado não substituem essas provas.
Close, resync ou gap de WS falham causalmente: sem polling, reconnect ou fallback.

O auditor JSON lexical existente e serde RawValue preservam o envelope nativo.
accepted_notifications no App conta somente o prefixo real de callbacks cujo
emit.await terminou Ok, exigido pelo gate MCP existente. Não é contador do vendor,
quota, tokens ou custo. Nenhum valor de uso ou custo é inventado.

## Source e limites de entrega

- [AI REST/WS](../../crates/graphit-ai/src/cli_kimi.rs): endpoint, limites, codec,
  correlação, steps e resultado elegível.
- [Owner Process](../../crates/graphit-ai/src/cli_kimi_process.rs): readiness,
  parada, observação e Remaining.
- [Ask Service](../../crates/graphit-app/src/knowledge_ask_service.rs): argv e registro
  do owner antes de ready, reap/shutdown.
- [Ask Kimi](../../crates/graphit-app/src/source_server/knowledge_ask/kimi.rs): preparação
  documental, progresso, receipt, answer e rechecks.

O Source atual mantém admissão, resultado elegível e entrega separados. A observação
concorrente do owner respeita o prazo também ao aguardar seu mutex: sem nova
observação, retorna pending e conserva a custódia real. As matrizes legadas não
são ampliadas por esta unidade.

## Qualificação Linux por peer

| Camada | Prova aceita | Medição e limite |
| --- | --- | --- |
| SDK unitário | 4 PASS, emissão 24x; Source intacto | Reutilizado, sem atribuir essa prova à posterior correção do observer. |
| AI integração | 5 PASS e 1 entrypoint peer ignored, emissão 25a após correção do mutex | 2,141 s wall; cancelamento do primeiro observer e prazo do observer concorrente. |
| App MCP | 2 PASS, emissão 24z; Source intacto | 123 ms wall, prova própria de MCP. |
| App | 6 casos ativos comprovados | Quatro casos de 25f reutilizados; HTTP exato com 17 cenários PASS em 64,005 s; invalidação exata PASS em 4,407 s na 25k. Não é tempo de suíte completa. |

O runner inicial cortou o HTTP aos 60 s; isso não foi veredito de falha do produto.
A primeira invalidação falhou no helper com ConnectionRefused porque a admissão já
havia parado o child. Somente o helper de teste mudou: antes de aceitar o controle
já fechado, exige Denied, ausência de answer/native_result, join, cleanupComplete e
ambos EOFs. A execução 25k comprovou essas condições. Os peers nativos controlados
observados vivos após os terminais foram zero.

AI Clippy 25e passou em 4,022 s wall; App Clippy 25h passou em 31,001 s wall antes
da alteração exclusiva do helper, sob o escopo Root; xtask Clippy 25i também passou.
O guard Root cargo xtask test graphit-ai/cli_kimi passou 5 casos com 1 entrypoint
peer ignored: teste 2,02 s, wall 17,623 s. A confirmação da cobertura fast-lib4 pelo
guard ainda pertence ao Root e não é declarada PASS aqui. Esses tempos não medem
CPU, desempenho de inferência ou runtime vendor. A Task mantém a autoridade sobre
checks e lifecycle; esta prova local não conclui parent, all11 ou sessão.

## Incoming Conversation continuation

Kimi Conversation reuses the existing private USER record and Server API runner for create, continue and cold reopen.

Kimi session IDs remain opaque native strings; no UUID grammar, Init event or vendor-specific echo is invented. The existing driver correlates WebSocket session_id, the accepted subscription, main agent, fresh prompt_id, turn and UTF-16 offsets. The same native completed/error/approval/question policy applies as Ask: manual permission mode is sent, approval and question requests do not trigger autoapproval, trust or personal-settings changes. This is not an OS sandbox or a claim that the vendor process only accesses the documentary corpus.

Each Incoming operation captures fresh USER read/write, PROJECT Knowledge and Original, checks selected physical/compiled source bindings and uses the captured program, model, environment and admitted cwd. The existing custody entry associates Arc<KimiServer> before waiting for readiness. Cancellation, deadline, shutdown and abandoned callers keep the USER writer until actual native Process/Remaining observation and join. A matching observed identity, completed main turn without native error, successful process exit/EOFs/cleanup, USER commit and current authority are required before releasing the answer.

The registered affected validation is three SDK store/configuration focals and three actual Incoming focals using the existing controlled REST/WS peer. These cover exact private create/resume/cold reopen, source changes and user isolation, missing/different session/cwd, native protocol/terminal failures without replacement creation, progress before termination, write conflicts and cancellation/deadline/shutdown joins. Existing Ask proofs are reused for unchanged parsing and lifecycle paths. Supplier installation, login, account, paid service and external catalogue are not acceptance requirements.
