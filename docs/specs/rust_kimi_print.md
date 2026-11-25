---
title: Rust Kimi Print for managed Role execution
doc_type: spec
description: Kimi Print JSONL consumido sobre ProcessHost, com recheck concreto e sem retenção paralela de saída.
references:
  - target: {type: task, id: tsk-0257}
    relation: implements
  - target: {type: session, id: ses-c98a}
    relation: relates_to
---

# Rust Kimi Print for managed Role execution

Contrato técnico do seam [cli_kimi_print](../../crates/graphit-ai/src/cli_kimi_print.rs)
para o driver [managed RoleRun](managed_role_context.md#run-owned-native-transports).
O seam está integrado à composição Rust; a qualificação do consumidor multihost
permanece separada da prova do adapter. Não altera o caminho
[Knowledge Ask/Conversation REST/WS](rust_kimi_completion.md), nem usa Print como
fallback de resposta documental. A seleção do perfil não concede autoridade.

## Composição nativa e proveniência

O contrato de origem é Kimi Code 2.2.0 no commit
`ef727e517923cb312cb48d42825c49dfacd528e7`. A
[definição oficial de argumentos](https://github.com/MoonshotAI/kimi-code/blob/ef727e517923cb312cb48d42825c49dfacd528e7/apps/kimi-code/src/cli/commands.ts)
define `--agent-file <path> --output-format stream-json [-m <model>] -p <prompt>`
na raiz do comando. Não existe subcomando `print`. O driver passa o Markdown
efetivamente publicado e selecionado pelo caller; `None` para modelo omite `-m`,
preservando a configuração nativa. Um modelo explícito vazio é inválido no composer.
Não combinar seleção de perfil com resume/continue, nem `-p` com yolo/auto/plan.

O [loader Print oficial](https://github.com/MoonshotAI/kimi-code/blob/ef727e517923cb312cb48d42825c49dfacd528e7/apps/kimi-code/src/cli/v2/run-v2-print.ts)
lê o arquivo e seleciona seu perfil na criação da sessão. Print usa modo auto
internamente; instruções, nome de perfil e essa seleção não constituem sandbox,
admissão ou grant Graphit. O driver e os owners existentes mantêm o guard MCP,
NativeLaunch, binding, grants atuais, credenciais em ENV privado e fechamento do
mesmo run. O seam não descobre executável, cwd, ENV, perfil ou modelo.

## API e memória

`run_kimi_print(original, host, request, recheck, limits, sink)` recebe o mesmo
`Arc<SnapshotContext>`, `ProcessHost` e `graphit_process::Request` capturados pelo
driver. `recheck` é `&(dyn Fn() -> io::Result<()> + Sync)`, concreto do consumidor;
não fabrica `ProjectReadAdmission` Knowledge. `KimiPrintLimits` é alias de
`CodexWireLimits`, reutilizando sua validação e auditoria lexical. Os limites de
stdin/stdout/stderr, grace e cleanup pertencem ao Request Process existente;
não há quota de número de frames, contador duplicado ou deadline renovado.

O sink recebe `KimiPrintEvent { native, data }` emprestado de uma única mensagem
auditada, com backpressure serial e o mesmo Original. `data` discrimina Assistant
com content e tool_calls opcionais, Tool com tool_call_id e content, Meta com type,
ou Unknown. Os argumentos de tool calls permanecem strings nativas, inclusive
fragmentos; não são executados nem reinterpretados pelo adapter. Campos novos
permanecem no RawValue auditado. JSON inválido, campos conhecidos mal tipados,
identificadores inválidos e violações dos limites concretos recusam o stream.

O framing aceita LF, CRLF, UTF-8 fragmentado entre chunks e última mensagem sem LF
somente após EOF real. A mensagem é consumida antes do await do callback, sem replay
quando ele é abandonado. Seu buffer é reutilizado e limitado por frame_bytes;
CR do CRLF conta no limite concreto. O recibo retém somente o resultado Process,
sem corpos, transcript, lista de eventos, resposta montada ou contagens paralelas.
As contagens reais continuam no Outcome/Failure Process.

## Conclusão, causas e custódia

O [writer JSONL oficial](https://github.com/MoonshotAI/kimi-code/blob/ef727e517923cb312cb48d42825c49dfacd528e7/apps/kimi-code/src/cli/prompt-render.ts)
emite mensagens de assistant, tool e meta, incluindo versão, retry e resume hint.
Não há evento JSON terminal de sucesso, uso de tokens ou modelo efetivo inferível.
Texto de hook pode aparecer como assistant sem origem discriminada. São mensagens
observadas do RoleRun, não prova de resposta principal documental.

Sucesso do adapter exige exit zero, EOF de stdout e stderr, Cleanup Complete,
ausência de Remaining e recheck final válido. Original e recheck são conferidos
antes da execução, durante os chunks, antes/depois dos callbacks e na entrega.
Cancelamento e deadline também interrompem callback pendente após EOF; cancelamento
do host continua sob o supervisor Process existente. O sink deve ser cooperativo,
seguro ao drop e não deixar trabalho destacado.

`KimiPrintFailure` preserva a primeira razão e a causa real privada de IO, recheck,
JSON/auditoria ou Process. `receipt().process()` e `receipt_mut().process_mut()`
conservam o terminal e o mesmo Remaining real para reparo pelo owner. Recusa antes
de spawn não fabrica terminal. Um exit ou EOF isolado não conclui RoleRun; o driver
retém admissão e sessão administrativa até cleanup/join reais. Debug/Display
expõem categorias e observações Process seguras, nunca argumentos, ENV ou corpos.

## Prova delimitada

[As focais](../../crates/graphit-ai/tests/cli_kimi_print.rs) reutilizam o executável
de fixture CLI existente, seus modos raw/sequence e ACK por socket. Cobrem mensagens
oficiais e futuras auditadas, callback antes do exit, CRLF/UTF-8 e frame final,
recheck antes/depois de efeito, schema e limites, exit não zero, sink fechado,
cancelamento Original/host e deadline com callback abandonado e custódia preservada.
Não exigem CLI vendor instalado, conta ou plano.

As seis focais do adapter passaram na composição Linux 47b, em 2026-10-17.
Cada seletor executou exatamente um teste, sem ignorados ou stderr; a conferência
reutiliza os logs da execução coordenada, sem replay. Esse resultado não encerra
a qualificação do consumidor managed RoleRun.

Essas focais de adapter não comprovam composição do argv, leitura do perfil ou
ferramenta Role real. Essa prova pertence ao consumer RoleRun com seu peer multihost,
perfil publicado, ambiente privado, admissão e ciclo administrativo reais. A
qualificação REST/WS anterior não é reexecutada nem transferida para Print.
