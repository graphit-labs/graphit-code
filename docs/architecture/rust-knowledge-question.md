---
references:
  - target: {type: task, id: tsk-625d}
    relation: implements
  - target: {type: task, id: tsk-28ee}
    relation: implements
  - target: {type: task, id: tsk-be6e}
    relation: uses
  - target: {type: task, id: tsk-9953}
    relation: implements
  - target: {type: task, id: tsk-a9c4}
    relation: supports
  - target: {type: task, id: tsk-ec15}
    relation: implements
  - target: {type: task, id: tsk-b74a}
    relation: uses
  - target: {type: task, id: tsk-cd41}
    relation: supports
  - target: {type: task, id: tsk-29bb}
    relation: corrected_by
  - target: {type: task, id: tsk-bdd8}
    relation: implements
  - target: {type: task, id: tsk-05a1}
    relation: implements
  - target: {type: task, id: tsk-e256}
    relation: implements
  - target: {type: task, id: tsk-1f4a}
    relation: uses
  - target: {type: task, id: tsk-322f}
    relation: uses
  - target: {type: task, id: tsk-ca7a}
    relation: implements
  - target: {type: task, id: tsk-a0f9}
    relation: specified_by
  - target: {type: task, id: tsk-69f2}
    relation: derived_from
  - target: {type: task, id: tsk-a6e8}
    relation: uses
  - target: {type: task, id: tsk-1597}
    relation: uses
  - target: {type: memory, id: 01M4NQ5CHEAHZQ46M9FJBVBJQT}
    relation: uses
  - target: {type: task, id: tsk-9db8}
    relation: implements
  - target: {type: task, id: tsk-e6ff}
    relation: specified_by
  - target: {type: task, id: tsk-7f7c}
    relation: specified_by
---

# Consulta documental Knowledge com Codex

O título e as qualificações Codex abaixo preservam seu histórico. As extensões App estão descritas no [contrato Ask one-shot com Cursor](../specs/rust_cursor_completion.md), em [Ask one-shot com Claude](#ask-one-shot-com-claude), [Ask one-shot com Qwen](#ask-one-shot-com-qwen) e [App Kiro: Ask e continuidade documental](#app-kiro-ask-e-continuidade-documental), com seus escopos e evidências próprios.

A biblioteca App compõe uma pergunta a um único PROJECT: lê a seleção Knowledge FULL18 autorizada no mesmo pin, envia a pergunta literal e as páginas selecionadas a uma execução Codex e devolve a geração observada, as páginas fornecidas e o receipt real. A API está em [knowledge_question.rs](../../crates/graphit-app/src/knowledge_question.rs).

A composição de `tsk-ca7a` foi qualificada em Linux: três testes nativos de integração, duas regressões privadas Serde e Clippy conjunto de biblioteca e integração passaram. Não houve testes ignorados nessas suítes focais. Essa prova histórica não qualifica o novo adapter autenticado de `tsk-e256`; seu estado está separado na seção de qualificação. Estas provas qualificam a biblioteca. O ingresso CLI/MCP HTTP de `tsk-05a1` possui implementação e qualificação separadas abaixo; autenticação do fornecedor, Conversation, LiveSearch e ranking têm escopos próprios.

## OpenCode Ask em Rust

O contrato ativo é [OpenCode 2 via ACP protocolo 1](../specs/rust_opencode_completion.md); o suporte à versão 1 foi removido.

O binding OpenCode capturado agora tem dispatch e custódia próprios em Source,
consumindo a mesma preparação documental e `ProjectReadAdmission` real. O driver
ACP exige o modelo oferecido e confirmado, chunks da sessão principal e
`end_turn`; a entrega também exige exit0, EOF dos dois streams e rechecks de
autoridade. O owner concreto fica retido no Service antes do próximo await;
spawn e registro são serializados com o snapshot de shutdown. O receipt usa
`accepted_notifications` observado e remove resposta e resultado nativo quando
a entrega é negada.

Quatro casos App de processo e um de receipt passaram, além dos sete testes SDK
AI. Os cinco casos de ingresso falharam em readiness antes do peer ACP: o binding
tinha custódia própria, mas readiness exigia um ProcessHost ausente. A correção
agora usa os mesmos checks reais de Original e parada do Service para a custódia
OpenCode. A nova emissão App16 executou cinco casos: passaram negação Read antes
do spawn e invalidação PROJECT após chunk; falharam as conclusões CLI, stdio
global e HTTP. Houve end_turn/SDK concluído, mas o processo não teve exit normal,
e a resposta foi suprimida. A revisão local identificou perda do finish metadata
do ByteStreams no wrapper ConnectTo, que permite Drop solicitar parada antes da
saída nativa normal. O wrapper agora preserva o driver real via map_future e
possui regressão de EOF seguido de exit0. A emissão App20 passou esse novo caso
e os três ingressos antes falhos: CLI, stdio global com progress antes da
conclusão/exit e HTTP com receipts/erros nativos. Os dois PASS de negação App16
foram reutilizados, sem nova execução. As provas posteriores SourceShutdown22a
(3,03 s) e owner-tree22c (2,53 s), em conjunto, qualificam o recorte Linux de
shutdown observado/abandonado/retomado e retenção da árvore nativa até cleanup.
O caso Source sozinho não prova EOF/permit sem o caso do mesmo owner. A qualidade
focal passou em AI24r, biblioteca App produtiva atual25h e runner Xtask25i. Os
seletores OpenCode usam o dispatcher e a propagação de falha existentes; as provas
funcionais inalteradas foram reutilizadas, sem nova execução do gate. Isso não qualifica o
fornecedor instalado ou outros sistemas operacionais. O [contrato OpenCode](../specs/rust_opencode_completion.md)
registra o alvo estável e esses limites. A extensão Conversation tem estado
próprio abaixo. Estes recibos históricos não qualificam OpenCode 2; a qualificação atual está no contrato citado.

<a id="ask-one-shot-com-kimi-restws-em-implementação"></a>

## Ask one-shot com Kimi REST/WS

O consumidor Kimi compõe preparação documental e admissão Knowledge conforme [Rust Kimi REST/WS](../specs/rust_kimi_completion.md).

O Service registra o owner Process antes de aguardar readiness; o mesmo Original
finito governa cancelamento/deadline. A API cria sessão com metadata.cwd, confirma
server_hello2/subscribe ACK e envia o JSON documental/modelo capturado com permissão
manual, sem arquivo por execução. prompt_id nativo, main e turnId fixam a resposta;
o offset UTF-16 externo é checado antes do append e reinicia em cada step. Só o
texto do último step pode ser elegível, com turn.ended reason completed sem
error/interrupt, depois do shutdown oficial, status 0/EOFs/join/cleanup e recheck
de admissão no App. Readiness separa o token antes do parser Url e exige
igualdade da base canônica literal com o resultado do parser.

Erro nativo e terminal main failed/cancelled/blocked ou com error/interrupt são
classificados antes do callback bruto; seus payloads permanecem privados. ACK
não zero conserva frame privado e código Native real. HTTP não 2xx mantém
status Http e envelope JSON auditado privado; erro de inspeção não substitui
a causa HTTP.

Hook, compaction, ferramenta e subagente são progresso; fechamento/gap/resync de WS
não aciona polling/reconnect/fallback. `accepted_notifications` registra somente o
prefixo emit.await Ok exigido pelo gate MCP, sem inventar usage, quota ou custo.
Erros brutos do provider permanecem privados.

AI/App/xtask Clippy e o guard de integração registrado passaram nos escopos do
artefato; App Clippy antecede a correção test-only do helper. A cobertura fast-lib4
pelo guard ainda aguarda confirmação Root. Peers controlados vivos após terminais:
zero. Isso não qualifica autenticação/runtime vendor, Windows/macOS, Conversation,
Live/Dream Kimi, all11, parent ou sessão; lifecycle permanece na Task.

## Ask one-shot com Copilot CLI

O [contrato Copilot Rust](../specs/rust_copilot_completion.md) de `tsk-625d`
usa o programa/cwd/ambiente capturados pelo serviço e a admissão PROJECT
Knowledge original. O consumidor passa o prompt documental UTF-8 por stdin,
com modelo literal em `--model=<literal>`, `--output-format=json`,
`--no-ask-user` e `--available-tools=`. O programa recebe argv diretamente,
sem shell produtivo ou transporte SDK/TCP. A seleção CliCompletionProvider
continua separada dos hosts NativeBinding.

Os eventos JSONL passam pelo produtor AI existente e pelo progress MCP serial.
Ask preserva eventos e resultado nativo segundo seu contrato operacional;
`receipt.native_result_json` contém o terminal original somente em entrega
admitida. A resposta vem do texto principal correlacionado ao prompt/turno,
sem promover delta, ferramenta, raciocínio ou agente filho a resposta.
O terminal é `type=result`, sem resposta embutida, e não `session.idle`.
Ele só permite entrega junto de processo realmente bem-sucedido, EOF das duas
saídas, cleanup completo, ausência de Remaining e autoridade atual. Uma falha
de gravação nativa após `result.exitCode=0` ainda falha a operação.

O serviço recebe a custódia real antes de projetar ou aguardar novamente. Negação tardia suprime answer e terminal; cancelamento, abandono e shutdown retêm o mesmo owner até encerramento observado.

## Preparar a chamada

O caller fornece o `Arc<SnapshotContext>` original, uma `ProjectReadAdmission` real de Knowledge, a pergunta UTF-8, a seleção explícita, `KnowledgeQuestionModel`, `KnowledgeQuestionLimits` e uma closure de transporte. A admissão precisa pertencer a `ProjectReadModule::Knowledge`; seu `request().snapshot()` deve ser o mesmo Arc do original. Reproduzir parâmetros ou deadline em outro contexto não atende à identidade exigida. A operação recheca contexto e admissão antes da leitura, depois da leitura e depois da execução.

`KnowledgeQuestionModel` contém `requested_model`, `wire_limits` e `completion_limits`. `max_graphit_attempts` precisa ser exatamente `1`; AI valida os demais limites na construção de `CodexCompletionRun`. A closure é chamada uma vez depois de preparar o contexto, sem retry ou fallback. O caller mantém os valores emprestados pelo receipt e assume o transporte e seu supervisor.

`KnowledgeQuestionLimits` contém `read: KnowledgeIndexReadLimits` e três `NonZeroUsize`: `max_query_bytes`, `max_selected_pages` e `max_prompt_bytes`. Pergunta vazia ou composta apenas de whitespace é inválida. Perguntas admitidas preservam todos os bytes. Slugs vazios ou duplicados são inválidos; cada slug solicitado deve existir no corpus não vazio, sem omissão silenciosa. As páginas fornecidas seguem a ordem por slug da leitura Data, independentemente da ordem de seleção.

## Chamada App autenticada com transporte real

O adapter [knowledge_ask.rs](../../crates/graphit-app/src/knowledge_ask.rs) oferece
`ask_knowledge_codex_authenticated(auth, question, execution)`. Ele admite
`ReadKnowledge` e executa o transporte Codex existente; o consumidor não precisa
fornecer uma closure. A API recebe três grupos:

- `AuthenticatedKnowledgeQuestion`: referências ao `ProjectRecordAuthority`, ao
  `AuthenticatedCaller` e ao `RequestContext` original, além do `ScopeInput` real.
  O owner autentica Runtime ou Local no issuer existente antes da chamada. O
  adapter não cria credencial, issuer, daemon ou contexto substituto.
- `KnowledgeCodexQuestion`: pergunta, slugs explícitos, `KnowledgeQuestionModel`
  e `KnowledgeQuestionLimits` da composição acima.
- `CodexQuestionExecution`: host de serviço e sink emprestados, `Request` capturado
  pelo caller e limites de streaming. Programa, argv, cwd, ambiente e limites
  Process seguem literalmente para `run_codex`.

`Request.stdin` deve ser `None`. `Some`, inclusive bytes vazios, retorna
`KnowledgeAskFailure::InputAlreadyPresent` depois da admissão e antes de ler o
corpus ou executar um child. O adapter preenche o stdin somente com o prompt
autorizado. Ele conserva os estados sem execução e o resultado da composição,
sem retry, fila de eventos ou fallback.

Falha de admissão conserva `AuthError` em `KnowledgeAskFailure::Auth`, acessível
por `auth()`. Falha posterior conserva `KnowledgeQuestionFailure` em `Question`;
`question()` e `question_mut()` dão acesso aos getters de execução abaixo e ao
mesmo receipt/remaining. `Error::source()` preserva causas disponíveis; `Debug`
e `Display` deste wrapper imprimem apenas categorias. O owner continua responsável
por repair e shutdown reais, inclusive quando houve negação após execução.

`requested_model` registra a escolha solicitada pelo caller; não confirma qual
modelo o child executou. A CLI nativa possui provider/model. Este adapter não
cria chave global de chat, não usa `daemon.models` (embeddings locais) e não
monta ou descobre argv. O ingresso posterior é responsável por selecionar a CLI
explicitamente e gerar opções nativas suportadas. A jornada documental AI é
`knowledge ask`; `knowledge query` é consulta estruturada ao índice. O ingresso CLI/MCP HTTP abaixo compõe esta API; a biblioteca permanece independente do transporte HTTP.

## Serviço HTTP e comando Ask

O ingresso de `tsk-05a1` está implementado em [knowledge_ask_service.rs](../../crates/graphit-app/src/knowledge_ask_service.rs), [source_server/knowledge_ask.rs](../../crates/graphit-app/src/source_server/knowledge_ask.rs) e [knowledge_ask_entry.rs](../../crates/graphit-app/src/knowledge_ask_entry.rs). A qualificação deste ingresso em Linux está descrita abaixo e separa seus consumidores das provas históricas da biblioteca.

Com o daemon foreground ativo, o consumidor pode executar:

```sh
graphit knowledge ask --query 'Qual é a capital?' --slug Paris --model MODELO
```

`--slug` é repetível. A alternativa automática usa o mesmo comando:

```sh
graphit knowledge ask --query 'Qual é a capital?' --retrieval hybrid --top-k 5 --model MODELO
```

`--retrieval` exige `--top-k` positivo e exclui `--slug`; `--metric` é opcional
somente para semantic/hybrid. Pergunta, slugs e modelo seguem literalmente, sem trim ou shell. `--project-dir` seleciona outro PROJECT retido, e o argumento global `--project-version` seleciona sua versão. A CLI reutiliza discovery, runtime key, headers e conexão privada MCP do Source existente. Ela não inicia outro daemon nem faz login. Escreve apenas a resposta final observada; estados sem resposta são JSON final. Falha do serviço retorna erro e exit não zero.

O tool MCP HTTP `graphit_knowledge_ask` recebe `{query, selected_slugs, model, project_dir?, project_version?}` ou a alternativa exclusiva `{query, model, retrieval:{mode:fts|semantic|hybrid,top_k,metric?}, project_dir?, project_version?}`. Slugs vazios continuam `empty_selection`; nunca acionam recuperação automática. Campos desconhecidos e optional presente como null são inválidos. Não recebe programa, argv, ambiente, cwd, limites ou imported context. Auth Runtime/Local e seleção PROJECT são as mesmas do Source; ReadKnowledge é admitido antes de Data/Process no mesmo Original. Deadline finito é capturado por chamada e não é renovado durante fases. Cancelamento do transporte ou do serviço cancela o Original; o owner Source conserva e aguarda o trabalho nativo. Se Auth ou Original negar entrega depois da execução, o receipt interno conserva a resposta observada para custódia; a projeção pública omite o texto de Answer e conserva usage, status e cleanup reais. Uma negação de entrega acrescenta `delivery_error` sem substituir a primeira causa nativa já observada. O sink recheca Original depois de serializar cada evento e antes de enviá-lo.

O startup do daemon captura uma única vez o ambiente do processo. Com binding Codex, resolve `codex` por `which = 8.0.6` no PATH capturado e mantém o executável absoluto; o cwd por chamada é o PROJECT admitido. O argv nativo é `exec --json --model MODELO -`, e stdin só recebe o prompt autorizado. O modelo de resposta é explícito por chamada, sem chave global de chat. Apenas a recuperação semântica/híbrida consulta o M local compartilhado; ela não troca o provider de resposta nem executa inferência do corpus.

No ingresso Codex descrito nesta seção, Codex é opcional para o startup; sua indisponibilidade preserva Source, Broker inventory e Task. As extensões atuais abaixo ampliam os bindings aceitos pelo serviço Ask. Uma tentativa explícita indisponível retorna erro finito acionável após autenticação/admissão, com a causa Which capturada no startup. Configurar o binding ou instalar a CLI correspondente no PATH e reiniciar o serviço é a recuperação. Não há lookup por request, downloader, fallback ou substituição automática de provider.

Os defaults desta composição são Host com dois slots/dois workers e stacks dos SDKs; prompt/stdin 1 MiB, resposta/stdout 8 MiB, stderr 64 KiB, grace 2 s e cleanup 10 s. A leitura FULL18 conserva identificadores de até 4096 bytes e batches de 128 linhas, sem quota agregada de páginas, edges ou texto do corpus; limites de arquivo reutilizam o NativeDocumentFileLimits capturado do Source. Pergunta tem cap de 64 KiB e seleção 4096 páginas. Wire possui frame/string/decoded string de 1 MiB, depth 64 e nodes/fields/items 65536; Completion mantém uma tentativa e model id 4096 bytes. São limites efetivos dessas operações; não representam RSS nem tokens do fornecedor.

### Progress e resultado

Quando `_meta.progressToken` está presente, o sink aguarda cada `notifications/progress` do SDK antes do próximo evento. `progress` é o ordinal do evento conhecido observado; não há total, porcentagem ou ACK do consumidor. `message` contém JSON do evento nativo projetado. Unknown valid frames continuam ignorados pelo produtor, sem evento inventado.

A projeção conserva thread started, turn started/completed/failed/error e item started/updated/completed. Itens conservam outer id e campos públicos de agent message, reasoning, command/output/exit/status, file changes, MCP tool arguments/result/error, colaboração, web search, todos e erro. Web search mantém inner id distinto. Usage do turn contém as cinco métricas nativas. Campos JSON brutos em argumentos/resultados MCP e web results passam por `serde_json::RawValue`: inteiros grandes, `1e400` e `-0` mantêm a representação. Ausência/null são conservados apenas quando o decoder os distingue; nenhum turn id ou modelo executado é inventado.

O receipt possui `usage` independente de `completion`: o getter do produtor reusa a evidência de completion quando presente e conserva a snapshot de tentativa incompleta quando ausente. Um exit não zero pode preservar `completion` ausente e ainda expor usage observado. Esse campo usa a projeção existente de provenance e métricas; não inventa completion, Answer, EOF ou causa para transportar uso. A correção `tsk-29bb` passou 38 testes afetados Linux sem ignored: 20 de estado, 17 Process/Codex e um caso Auth/FULL16/HTTP com dois terminais. A projeção real mantém usage8/2/3/1/9, NonzeroExit, known3 e cleanupComplete com completion ausente; no sucesso, usa a mesma evidência já guardada na completion. Clippy selecionado de AI e App passou; comandos, wall/harness e limites estão em `.build/rust-codex-receipt-usage/qualification.json` e na [especificação Codex](../specs/rust_codex_completion.md). Essa prova não fecha a entrega STDIO.

O JSON final distingue `no_corpus`, `published_empty`, `empty_selection`, `answer`, `empty_answer`, `evidence_unavailable`, `failed` e `no_completion_observation`. Quando observados, inclui geração (corpus/version/physical branch/inherited target), provided slugs, completion e receipt Process com status/exit, finalização de pipes, cleanup, bytes aceitos/observados/entregues, frames e known events. `requested_model` e provided slugs são fatos da entrada, sem confirmação do modelo ou citações. Usage mantém provenance observed/estimated/unavailable; RPCs nativos permanecem unobserved.

RemainingProcess é retirado sincronamente do receipt e retido no serviço antes de awaits posteriores. Source shutdown para admission, cancela execução, aguarda jobs e somente então consome o Host, antes de remover discovery/credenciais. JoinHandle e PendingHost ficam em campos duráveis: cancelar o waiter não aborta join nem descarta repair. Repair Observed com cleanup incompleto continua sendo reportado como incompleto; pendência ou falha real não vira sucesso. O forwarding stdio usa o consumidor separado `tsk-bdd8`, descrito abaixo.

## App Kiro: Ask e continuidade documental

A extensão reutiliza o serviço Ask e o [store privado de conversas](../specs/rust_knowledge_conversation_app.md). O startup captura o completion provider independente do host nativo. Captura executável, ambiente e cwd; não introduz factory global, chave de modelo ou outro Host/RPC. O modelo literal é obrigatório por request, limitado a 4096 bytes, e permanece `requested_model`, sem atestar o modelo executado.

O contrato executável suporta somente Kiro V3 atual, conforme a [composição documentada](../specs/rust_knowledge_conversation_app.md). Seletores V2 e preview e probes de help antigos são história, sem gates de instalação ou conta. New/Load passam o modelo literal na metadata nativa e o consumidor valida a oferta de configuração, envia SDK Select e exige confirmação antes de Prompt; não substitui modelo nem faz fallback. O caller não fornece MCP servers ou additional roots nesse caminho. Isso não afirma isolamento das configurações nativas, não altera trust/perfil e não executa scans ou clamps de permissões pessoais.

Ask conserva Auth Runtime/Local e PROJECT ReadKnowledge. A conversa Incoming usa USER Knowledge read/write e uma nova PROJECT ReadKnowledge por turno que lê fontes, sem PROJECT write. A preparação faz uma leitura do corpus e reutiliza o renderer e a INSTRUCTION existentes. Corpus ou seleção vazios não iniciam child. History entregue em Loading não é uma nova resposta; o ID solicitado em Load só confirma continuidade quando há resposta Load efetivamente observada.

O adapter [knowledge_kiro_process.rs](../../crates/graphit-app/src/knowledge_kiro_process.rs) usa Tokio Command com grupo próprio (`process_group(0)`), cwd e ambiente capturados, e ByteStreams do SDK. `AskService::kiro_process` aguarda assincronamente um dos dois slots sob cancelamento e deadline do Original e stop do serviço; depois da espera, recheca a mesma admissão USER/PROJECT por `allowed` emprestado. O transport `ConnectTo`, exigido como estático pelo SDK, recebe o permit owned já admitido, sem esperar outro semáforo, e recheca Original/stop antes de spawn. O permit permanece retido até o Child wait real; conclusão do driver não libera slot nem writer. Os caps são stdin de 1 MiB, stdout de 8 MiB, frame de 1 MiB, stderr de 64 KiB, grace de 2 s e cleanup de 10 s. Prompt tem cap de 1 MiB, query de 64 KiB, resposta de 8 MiB, ID/modelo de 4096 bytes e notifications de 65536. Esses caps não são limites de RSS nem de filas internas do SDK.

Progress informa provider, phase e o update nativo; omite o `session_id` do envelope, preservando IDs e dados dos tools no update. Atualizações de tools não demonstram efeitos Records. A observação do kernel Child wait é separada de driver OK ou EOF. O serviço transfere o writer USER antes do RPC. Depois de reter o turno e antes da espera por slot, o `ConversationDecisionGuard` lexical começa como unsubmitted: Drop sem transport retorna por `release_unsubmitted`. Ao receber o transport owned, o caller chama `submitted()` antes de executar; daí Drop abandona somente a decisão de persistência, conservando o writer até Child wait real, inclusive no intervalo após receipt e antes de join/finish. O guard é desarmado por `decided()` somente após finish. O SDK pode descartar normalmente o futuro de transport em `connect_with`; isso não é Drop do caller App. O adapter não possui flag COMPLETED: seu Drop apenas pede stop e libera o turno comprovadamente não iniciado. A decisão de persistência pertence ao guard App e a finish, não ao futuro SDK. Pendência ou falha conserva child e causa; conclusão do driver não libera o writer. Cancelar o waiter HTTP conserva jobs: texto Cancelled ou receipt ausente não prova ausência de efeitos nem join. Negação de Auth após execução remove Answer e a projeção USER da entrega, mantendo observações reais. Logs padrão não expõem IDs de sessão nativos ou segredos.

### Histórico de qualificação Kiro App em Linux

App check02 e o build da fixture bin02 passaram. Antes da alteração do argv V3, native04 passou quatro casos, native06 passou um e httpcancel02 passou um, totalizando seis conversas únicas em Linux com Broker real, USER/Lance e processo SDK Agent. As fixtures exercitam V2/V3 New, Continue e cold reopen, mudanças de binding selecionado e não relacionado, persistência parcial, erro Load sem fallback New e recuperação, e limites de frame/stderr/deadline sem resposta parcial.

O recorte anterior à mudança de argv passou 29 casos distintos: seis conversas, seis casos ec15 de libkiro02, oito commonprep, oito regressões Ask e um Incoming Codex. Libkiro02 passou sete testes, incluindo um hook estrangeiro excluído dessa contagem (harness 2,15 s, wall 2,69 s); commonprep01 passou oito (8,06 s/11,82 s); Askregression01 passou dez, incluindo duas repetições excluídas (10,88 s/11,34 s); CodexIncoming01 passou um (1,96 s/23,62 s). Loops, retries, repetições e hooks incidentais não criam testes únicos adicionais. Clippy01 de biblioteca, integrações Question/Conversation e bin, sem dependências e com `-D warnings`, passou em 23,78 s. O readback Source Tracker não encontrou divergência nas três correções de guard. São provas focais Linux e custos de comando, sem ganho de desempenho inferido.

Native01 permanece histórico com um PASS e duas falhas de fixture: a publicação Auth precisava aguardar o join do owner, e a asserção de deadline precisava observar a causa textual real. Bin01 também conserva as falhas de schema Value enum e ID emprestado corrigidas antes de bin02. Native02 passou três casos (harness 16,42 s, wall 264,67 s, incluindo build de aproximadamente 4 min 08 s). Custody01 parou antes de executar por três erros de compilação externos em `role_tool_server/git.rs`; build06 passou em 16,16 s. Libkiro01 passou seis novos casos ec15 e um hook incidental (2,24 s de harness, 29,19 s de wall). Custody02 passou três casos, incluindo Drop em fila com writer USER (2,13 s de harness, 31,05 s de wall), antes da última revisão de adapter Drop. Native03 encontrou duas falhas reais de liberação prematura do writer, que motivaram a correção descrita acima. Esses resultados e falhas continuam históricos, sem somar revisões à contagem atual.

Httpcancel02 passou o caso HTTP real: o writer USER permanece retido depois de SDK DriverClosed e cancelamento público MCP, e só é liberado depois de Source shutdown observar Child wait real. O child controlado ignora TERM e retarda exit; ele não simula autenticação comercial. O harness levou 2,64 s e o comando 100,19 s, incluindo build de 1 min 37 s e rebuild C++ compartilhado.

A qualificação deste recorte App, testes e documentação focal permanece registrada em `tsk-ec15`. O recibo histórico efêmero `.build/rust-knowledge-kiro-app/qualification.json` foi removido pontualmente e não está disponível neste checkout. Antes da mudança de argv, Clippy02 de `knowledge_conversation` e bin SDK, sem dependências e com `-D warnings`, passou: dev de 1 min 50 s e wall de 110,69 s, incluindo rebuild C++ compartilhado, sem inferir ganho de desempenho. Os 29 casos únicos, os dois recortes de Clippy, fmt10, diff e links locais passaram; o readback Source Tracker das correções de guard foi NOFAULT.

No contrato histórico de flags V3 anterior ao atual, v3flag-conversation01 passou quatro conversas (harness 17,21 s, wall 42,72 s). Naquela revisão, a fixture SDK Agent aceitava apenas `["--v3", "acp"]` e rejeitava `--agent-engine=v3`; isso não define os argumentos executáveis atuais. V3flag-ask01 passou os dois casos Ask stateless, zero falhas ou ignored e 206 filtrados (harness 2,34 s, build de 1 min 21 s, wall 84,83 s). São seis casos V3 requalificados dentro do total de 29 casos distintos, não 35. Clippy03 de biblioteca, `knowledge_conversation` e bin SDK, sem dependências e com `-D warnings`, passou (dev de 1 min 03 s, wall 63,91 s, incluindo rebuild C++ compartilhado). Fmt de serviço/fixture e diff após a mudança de flags passaram, e o readback Tracker do argv exato foi NOFAULT. V2, caps, metadata/Select, Auth, custódia, IO, HTTP cancel e guard Codex não mudaram; suas suítes não foram repetidas. O caller ec15 está qualificado neste recorte Linux, sem qualificar todo App, autenticação comercial Kiro (`tsk-5640`), Main, todas as CLIs ou Windows/macOS.

## Knowledge Ask por STDIO

Com o serviço foreground ativo, `graphit mcp --stdio` usa o
[SourceProxy](../../crates/graphit-app/src/mcp_source.rs) existente. O catálogo
mantém `graphit_ast_source` e anuncia `graphit_knowledge_ask` somente quando o
HTTP autenticado oferece Ask. Uma chamada explícita a Ask ausente ainda chega
ao serviço para receber seu erro real; não escolhe outra CLI ou modo Role.
A pergunta, slugs e modelo seguem o mesmo contrato HTTP. A seleção atual
preenche apenas root/versão ausentes; null explícito, root estrangeiro e
versão explícita continuam sujeitos à validação do serviço.

O [handler privado](../../crates/graphit-app/src/mcp_source/knowledge_ask.rs)
mantém uma conexão por chamada, com o mesmo Original e deadline absoluto.
A admissão exige prazo finito antes de ler as credenciais Runtime ou abrir
a conexão. O gate conserva esse Instant junto do mesmo Original, e o timeout
do SDK usa apenas o tempo restante desse prazo; não há prazo substituto para
contextos de serviço sem deadline.
Depois do handshake, recheca o Original no primeiro poll do pedido antes
de permitir enqueue SDK; a causa IO real de cancelamento/deadline fica privada.
Encaminha os campos SDK da request por move, inclusive metadata,
input responses e request state. Liga o token downstream real de
`RequestHandle` ao token recebido no STDIO; callback anterior a esse bind
aguarda. O SDK pode despachar notifications fora de ordem, então o gate
específico da chamada aguarda ordinal 1, 2, … e encaminha cada mensagem
literal sem interpretar seu JSON. Isso conserva números RawValue de eventos
MCP/WebSearch, inclusive `1e400`, `-0` e inteiros grandes.

No rmcp 3.5.1, metadata de request permanece em `Incoming.meta`. Metadata de
notification desserializada fica nas extensions do contexto: o handler move
`NotificationMetaObject` com `remove`, pois `context.meta` pode estar vazio.
O observer tipado também consulta extensions; `params.meta == None` depois
da desserialização não prova ausência. Metadata JSON externa segue os tipos
do SDK e não tem garantia de fidelidade numérica lexical arbitrária.

O terminal aguarda o prefixo aceito real: `receipt.known_events` para provider
ausente (Codex existente) ou literal `codex`, e
`receipt.accepted_notifications` para provider literal `kiro`. Provider unknown,
null ou não string é inválido, sem fallback Codex. `observed_notifications` inclui notifications ignoradas, estrangeiras
ou anteriores à fase aceita e não é esse prefixo; não há alias ou contador
Codex fabricado para Kiro. Campo aceito ausente, null, negativo ou fracionário
é contrato inválido, mesmo sob erro nativo, sem conversão para zero. Um envio
pode preceder falha na rechecagem pós-push do Original e não ser aceito pelo
produtor: Tail observado além do prefixo é legítimo nesse caso. Tail, gap,
token ou ordinal inválido e falha de entrega impedem sucesso com resposta
truncada. Sob negação, a projeção conserva receipt, causa, usage, status,
cleanup e metadata nativos e omite Answer. Os sucessos `no_corpus`, `published_empty` e
`empty_selection` têm prefixo zero documentado. Erro sem receipt tem alvo
não observado; não comprova pré-execução nem permite inventar count zero.
Erros SDK mantêm sua causa privada e expõem classe e código MCP observados,
sem serializar mensagem/data brutos nos diagnósticos próprios.

O fechamento impede nova admissão antes do cancelamento SDK e aguarda os
callbacks realmente iniciados. A notification de cancelamento recebe apenas
um primeiro poll sem yield: Pending não prova enqueue, envio ou ACK e seu
waiter não atrasa o shutdown owned. A conexão continua sob custódia do job
até o cancel/join SDK real. Erros SDK de forward/cancel e JoinError de
cleanup ficam privados com a primeira categoria de entrega; a projeção
não imprime suas mensagens brutas. Callbacks SDK ainda não polled podem existir
após o join do serviço; quando começam, o gate fechado impede IO. Não há
promessa de join físico de todos eles nem ACK de consumo. EOF/cancelamento
mantêm os jobs no JoinSet do proxy; o serviço HTTP continua dono dos children
e de seu reap. Copias próprias de token/seleção e mensagem usam os limites
Content existentes. Objetos já alocados pelo SDK não recebem hard frame cap
ou limite de RSS por esse gate.

Debug/erros próprios expõem classes e contagens. O App STDIO padrão não
instala subscriber. Actors rmcp com INFO/DEBUG/TRACE opt-in podem registrar
payload por política do caller; um subscriber arbitrário não é isolado
por este handler. Ativar esses logs no produto exige filtro do caller/SDK
em uma unidade própria antes de qualificar esse modo.

A qualificação atual de `tsk-bdd8` executou 24 testes Linux, todos PASS e sem
ignored: 12 Gate, cinco integrações Ask STDIO, três regressões MCP Source,
duas AST Source focais e duas do ingresso CLI. O executável compilado atravessa
Auth, FULL16, HTTP e Process reais. A fixture observa mensagens MCP/WebSearch
literais, token e ordem antes do EOF do child; cobre root atual e versão main
explícita, catálogo opcional, rejeições sem child, erro nonzero/malformed com
usage e receipt, cancelamento, EOF, deadline e waiter de shutdown abandonado.
Os 12 casos Gate verificam também binding antecipado, ordem 2 antes de 1,
prefixo aceito, gap/tail, erro SDK real privado e callback tardio sem IO.

| Execução atual | PASS / ignored | Harness (s) | Wall (s) |
| --- | --- | --- | --- |
| Gate `units12` | 12 / 0 | 0.08 | 11.047949 |
| Ask STDIO `stdio09` | 5 / 0 | 17.22 | 60.049560 |
| MCP Source `mcp02` | 3 / 0 | 0.19 | 174.119754 |
| AST texto `ast-text01` | 1 / 0 | 0.29 | 14.657700 |
| AST erros `ast-denial01` | 1 / 0 | 0.10 | 3.851918 |
| Ask CLI `ask-cli01` | 2 / 0 | 1.21 | 6.233112 |

Clippy selecionado passou para App lib/bin e os quatro targets de integração,
com `--no-deps -D warnings`, em 111.357487 s wall. Os comandos e recibos estão
em `.build/rust-knowledge-ask-stdio/qualification.json`; usam `--locked
--offline`, jobs1, debug0 e incremental0 no target compartilhado. Wall inclui
fila e build, separado do harness; não mede latência de inferência nem P95.
Nenhuma prova de autenticação comercial, modelo executado, RSS, outros sistemas
operacionais ou isolamento de subscriber opt-in é inferida desses casos.

O strict de cfg unit tests foi consumido do owner Role `tsk-cd41` COM30:
App/Daemon `--lib --tests --no-deps -D warnings` passou após dois guards dos
testes Gate receberem escopos lexicais antes do await de shutdown. As assertions
e a produção permaneceram inalteradas; os 24 testes não foram repetidos.
O Cargo informou 32.21 s para esse strict compartilhado; wall externo não está
disponível. Seu comando, ambiente e log `strict-qualified.log`, bem como a
origem `strict-final.log` dos dois lints, ficam no recibo de qualificação. Essa
prova compartilhada cobre cfg strict e não incorpora outras jornadas Role.

As tentativas anteriores permanecem no mesmo diretório. `units01/03` e
`stdio01/03` falharam em tipagem própria; `units04` teve 9 PASS/1 FAIL e
revelou a leitura incorreta de metadata de notification. `stdio04` teve
3 PASS/2 FAIL: corrigiu-se a fixture MCP/Web e o produtor de usage em
`tsk-29bb`, sem fabricar completion no proxy. `stdio02`, `units06/07` e
`units09/10/11` não executaram os testes da folha por seams de seus owners;
estes últimos registram respectivamente 3179.537651, 470.062988 e 396.450766 s
wall. `units08` parou por ENOSPC antes dos testes e a medição também falhou:
wall e exit Cargo são indisponíveis, não reconstruídos. `mcp01` recusou
`--locked` durante a reconciliação do lock compartilhado (0.140015 s wall);
o coordenador confirmou os pins preservados e apenas a remoção do contador de
alocação. Os passes parciais de `units02/05` e `stdio05` são históricos e não
entram nos 24 atuais. As provas históricas de HTTP abaixo têm escopo próprio.

## Leitura e prompt

`KnowledgeSearchCapture::read_selected_documents` lê a seleção explícita com predicado de slugs no mesmo pin capturado, validando FULL18 até EOF do stream selecionado e conservando somente esses documentos. A validação global da captura permanece separada e não é repetida durante a leitura selecionada. Slugs inválidos, duplicados, desconhecidos ou removidos são erros; corpus publicado vazio é diferente de seleção vazia. A ordem explícita permanece lexical. Aquisição, leitura e integridade pertencem a [Data Knowledge](rust-knowledge-semantic-data.md); suas falhas permanecem erros. A composição usa os bodies publicados, sem reler arquivos Markdown. Os providers ativos recebem o mesmo preparo, sem segunda captura ou Vec do corpus inteiro.

Ao consumir o preparo, Os providers ativos conservam somente o prompt e os `KnowledgeHead` observados (slug, título e updated_at), além da geração. Bodies, summaries e demais campos dos documentos são descartados antes de aguardar o subprocesso; os getters do preparo continuam completos enquanto ele existe.

O prompt é JSON de **entrada** com `instruction`, `question` e `pages[{slug,title,body}]`. A instrução pede resposta apoiada nas páginas, indicação de insuficiência e tratamento dos bodies como dados. Essa instrução textual não concede autoridade nem garante confinamento de ferramentas. A resposta do modelo permanece o texto observado pelo protocolo Codex.

Pergunta, títulos e bodies são serializados sem trim ou truncamento. O cap conta os bytes efetivamente emitidos, incluindo escaping e envelope JSON: o limite exato passa; excedê-lo retorna `Limit("prompt bytes")` antes da closure. Nenhum prefixo parcial é entregue.

O writer integral privado verifica o Original em `write`, `write_all` e `flush`, inclusive slices vazios. Seu `write_all` faz uma única escrita, evitando retry de `Interrupted` sob cancelamento persistente, conforme Memory `01M4NQ5CHEAHZQ46M9FJBVBJQT`. O primeiro erro Request, Limit ou Allocation é conservado; o sinal terminal para Serde não substitui sua causa. Crescimento do buffer é fallível e somas de bytes são verificadas. Esses limites não medem RSS nem garantem preempção nativa ou controle de todas as alocações internas dos SDKs.

## Compor o transporte

Este exemplo recebe admissão, host e `Request` já configurados pelos owners. Programa, argumentos, diretório, ambiente e limites Process continuam responsabilidade do caller. O exemplo instala o prompt no stdin e executa a tentativa `Request`:

```rust
use std::sync::Arc;
use graphit_ai::{
    cli_codex::{CodexSink, CodexStreamLimits, run_codex},
    codex_completion::AttemptPurpose,
};
use graphit_app::knowledge_question::{
    KnowledgeQuestionFailure, KnowledgeQuestionLimits, KnowledgeQuestionModel,
    KnowledgeQuestionResult, ask_knowledge_codex,
};
use graphit_auth::ProjectReadAdmission;
use graphit_core::file_io::SnapshotContext;
use graphit_process::{ProcessHost, Request};

async fn perguntar<'n, S: CodexSink>(
    original: &'n Arc<SnapshotContext>,
    read: &ProjectReadAdmission<'_>,
    question: &str,
    selected_slugs: &[&str],
    model: KnowledgeQuestionModel<'n>,
    limits: KnowledgeQuestionLimits,
    host: &ProcessHost,
    mut request: Request,
    stream_limits: CodexStreamLimits,
    sink: &mut S,
) -> Result<KnowledgeQuestionResult<'n>, KnowledgeQuestionFailure<'n>> {
    ask_knowledge_codex(
        original, read, question, selected_slugs, model, limits,
        |mut run, prompt| async move {
            request.stdin = Some(prompt.into_bytes().into());
            run_codex(&mut run, host, request, AttemptPurpose::Request,
                None, stream_limits, sink).await
        },
    ).await
}
```

O exemplo não constrói credenciais nem substitui o Original. O protocolo, usage e receipts são os contratos de [Completion Codex](../specs/rust_codex_completion.md). A composição não fabrica receipt ou infere uma resposta de exit zero, stderr ou texto parcial.

## Interpretar resultados

| Resultado | Observação e ação |
| --- | --- |
| `NoCorpus` | Data observou ausência; a closure não é chamada. |
| `PublishedEmpty { generation }` | Existe geração publicada sem documentos; a closure não é chamada. |
| `EmptySelection { generation }` | Há documentos, mas a seleção está vazia; a closure não é chamada. |
| `Executed` | Conserva `generation`, metadados `KnowledgeHead` das páginas fornecidas, `Box<CodexReceipt>` e disposição. |

`KnowledgeQuestionDisposition` distingue `Answer`, `EmptyAnswer`, `EvidenceUnavailable`, `Failed` e `NoCompletionObservation` conforme o receipt real. `trim()` decide apenas a classificação de whitespace; o texto não é alterado. Usage, ordinal e purpose continuam as observações nativas.

`provided_pages` identifica os documentos realmente fornecidos, **não citações verificadas**. Um `[[slug]]` na resposta não demonstra uso correto nem autoriza outra leitura. A geração é a observação da leitura realizada: não garante que o head permaneceu igual durante a execução e não resolve retirement ou lifecycle de branches.

## Erros e recuperação

`KnowledgeQuestionFailure::error()` expõe Request, Auth, Data, construção Completion, Encoding, Allocation e Limit, além de categorias para pergunta, seleção, admissão, Original e limite de tentativas inválidos. Data e Completion são boxed preservando tipos e causas. `Error::source()` mantém as causas disponíveis; `Debug` e `Display` do wrapper não incluem pergunta, bodies, slugs, modelo, caminhos ou frames.

Antes da closure, falhas têm `execution() == None`. Após o await, falha de rechecagem conserva o resultado real da execução. Falha Codex conserva o `CodexFailure` inteiro. `execution()` e `execution_mut()` expõem referências ao `Result<CodexReceipt,CodexFailure>` guardado; seu armazenamento privado boxed não muda a propriedade do processo.

Para recuperar recursos:

1. Acessar o receipt de `Executed` ou da execução retida; em falha Codex, usar `receipt_mut()`.
2. Inspecionar `process_mut()`: sucesso e erro Process podem conservar `remaining`.
3. Quando houver `remaining`, aguardar seu `repair` bounded e interpretar a observação real.
4. Aguardar `ProcessHost::shutdown`; um resultado pendente conserva um owner que também exige tratamento.

Uma falha após execução não autoriza descartar owners, repetir a pergunta automaticamente ou substituir o contexto de autorização. Cancelamento e rechecagem são cooperativos nas fronteiras existentes.

## Qualificação

O ingresso `tsk-05a1` executou sua suíte privada em `units08`: nove testes passaram, sem falhas ou ignored, harness 10,77 s e wall 40,52 s. Auth, FULL16, Process, SDK Codex e cliente MCP HTTP são reais; o executável Python controlado produz frames determinísticos. A prova inclui progress recebido antes do EOF, JSON numérico bruto MCP/WebSearch intacto, captura literal de launch/prompt, erros nativos, cancelamento/deadline, custódia de join e reap, disponibilidade opcional e Answer omitido na entrega negada. A CLI compilada passou seus dois casos em `cli02`, sem ignored, harness 0,97 s e wall 29,91 s: pergunta/modelo/prompt/launch literais, PATH divergente do caller, saída final exclusiva e Source CLI operante com Codex ausente. O filtro foreground de ausência passou um teste com três modos de perfil e restart/cleanup (`foreground01`, harness 1,01 s, wall 20,11 s). A regressão Source passou seus dois testes reais em `source02`, harness 0,06 s e wall 0,46 s: Original/deadline/service cancel e HTTP cancel com sibling FILE/custódia nativa. Clippy selecionado de App lib/bin, integração Ask e foreground inventory passou com `--no-deps -- -D warnings` em `strict01`, wall 19,30 s. São 14 testes afetados, todos com contagem positiva e nenhum ignored; não é uma qualificação ampla da migração.

Comandos deste ingresso, no mesmo target compartilhado e com os três ajustes de perfil acima:

```sh
cargo xtask cargo test --locked --offline -p graphit-app --lib knowledge_ask -j1 --target-dir .build/rust-records-data/knowledge-index-source/target
cargo xtask cargo test --locked --offline -p graphit-app --test integration -j1 --target-dir .build/rust-records-data/knowledge-index-source/target -- knowledge_ask_ingress::
cargo xtask cargo test --locked --offline -p graphit-app --lib source_server::tests -j1 --target-dir .build/rust-records-data/knowledge-index-source/target
cargo xtask cargo test --locked --offline -p graphit-app --test integration -j1 --target-dir .build/rust-records-data/knowledge-index-source/target -- --exact foreground_broker_inventory::exact_absence_no_active_and_local_without_broker_use_real_local_startup
cargo xtask cargo clippy --locked --offline -p graphit-app --lib --bin graphit --test integration --no-deps -j1 --target-dir .build/rust-records-data/knowledge-index-source/target -- -D warnings
```

A continuidade Task/Broker consome as provas do coordenador `tsk-0a1a` (COM17, quatro casos compiled foreground com NativeBroker signed TCP/PROJECT real) e `tsk-8d58` (COM20, NativeSource/Auth/install lifecycle), além da regressão foreground acima com ambiente capturado sem Codex. Os testes remotos de ForegroundInventory que exigem fixture pública/container permanecem escopo não qualificado de `tsk-e6e4`; ignored não é prova. O ingresso não confirma autenticação de fornecedor, modelo executado, stdio forwarding, RSS, Windows ou macOS.

As tentativas anteriores ficam em `.build/rust-knowledge-ask-ingress/`: `compile01` falhou em lifetimes próprios, sem wall medido; `compile02`, `units01` e `units02` falharam antes dos testes por mudanças Auth/NativeInstall de seus owners; `units03` falhou na compilação da fixture própria. `units04` executou 3 PASS/5 FAIL, `units05` 5 PASS/3 FAIL e `units06` 6 PASS/3 FAIL. As correções fizeram as fixtures seguir escaping, campos e sequência do protocolo real, e preservaram os erros observados. `units07` falhou antes do App por falta de espaço no target compartilhado; seu wall ficou indisponível porque a gravação da medição também falhou. A coordenação liberou apenas ELFs antigos e inativos antes de `units08`; nenhum teste não executado conta como PASS. `cli01` executou 1 PASS/1 FAIL; a fixture Source usava `--path` em vez do caminho posicional da API existente. `source01` falhou antes dos testes porque a fixture NativeInstall do seu owner mudou a key para Zeroizing<String> e ainda a formatava sem borrow explícito; o erro foi encaminhado ao owner. Tempos de fila/build não são medidas de latência do produto.

[Testes de integração](../../crates/graphit-app/tests/knowledge_question.rs) combinam Core, Auth, H, Data FULL16 e Process/Codex reais. Python3 resolvido no PATH lê o JSON por stdin e emite stdout Codex determinístico. A execução histórica de `tsk-ca7a` prova a composição com closure e seu transporte, sem autenticação de fornecedor ou CLI comercial. A thread de fixture com stack de 8 MiB é joined e atende ao encoding Arrow/Lance em build debug; não é uma quota de produto.

Os casos cobrem ausência e corpus vazio, seleção, pergunta Unicode e bodies literais, caps exatos e excedidos, erro Data, identidade Original, módulo e grant, respostas ausente/vazia, JSONL inválido, exit não zero, provider failure, cancelamento/deadline durante child e execução conservada após rechecagem. Duas regressões privadas usam Serde real para cancelamento, escrita vazia, escaping e primeiro erro sticky.

Comandos focais, com `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` e `CARGO_INCREMENTAL=0` na fila compartilhada normal:

```sh
cargo xtask cargo test --offline --locked -p graphit-app --test integration -j1 --target-dir .build/rust-records-data/knowledge-index-source/target -- knowledge_question::
cargo xtask cargo test --offline --locked -p graphit-app --lib knowledge_question::tests -j1 --target-dir .build/rust-records-data/knowledge-index-source/target
cargo xtask cargo clippy --offline --locked -p graphit-app --lib --test integration -j1 --no-deps --target-dir .build/rust-records-data/knowledge-index-source/target -- -D warnings
```

Evidências históricas de `tsk-ca7a` ficam em `.build/knowledge-question/`. A integração final passou em três testes sem ignored (build 5,06 s, harness 7,25 s); duas regressões privadas passaram (build 14,05 s); Clippy conjunto passou com exit 0 em 7,78 s. Esses tempos observados não são garantia de desempenho. O primeiro attempt de integração falhou na compilação do helper de lifetimes da fixture e foi corrigido; seu log foi preservado. A revisão dessa composição não encontrou divergência material. A execução disponível é Linux; Windows e macOS não foram qualificados.

O novo adapter autenticado `tsk-e256` acrescenta quatro casos no
[módulo child](../../crates/graphit-app/tests/support/knowledge_ask.rs), reutilizando
as mesmas fixtures. Sua suite01 falhou na compilação antes dos testes: faltava o
edge regular Process no App e havia um match Task desatualizado. O edge existente
foi cedido pelo coordenador, sem novo pin; o owner Task corrigiu seu match.
Suite02 também falhou antes dos testes, durante uma mudança Core em execução:
o plano de payload ainda referenciava o tipo e campo retention removidos.
Suite03 compilou e executou sete casos: seis passaram; o novo caso de estados
vazios falhou porque sua fixture tentou publicar duas vezes no mesmo destino.
O caso foi corrigido para usar um corpus independente em cada estado.
Suite04 falhou antes dos testes por imports desatualizados do consumidor
`native_install` durante mudanças HTTP/Hub; seus owners corrigiram os imports.
Suite05 passou os sete testes de integração, sem ignored: build 22,43 s,
harness 6,77 s e wall total 29,37 s. Os quatro novos casos qualificam o adapter
com Auth, FULL16 e Process reais; os três existentes qualificam os limites,
deadline e rechecagem da composição no mesmo target. A revisão estática final
não encontrou defeito material; os seis targets passaram fmt, whitespace e links.
Clippy selecionado passou em strict02, exit 0 e wall 8,47 s. Strict01 falhou antes
dos diagnósticos App porque o módulo Records `lexical_search` ainda estava sendo
criado pelo seu owner. Evidências e tentativas reais ficam em `.build/rust-app-knowledge-ask-e256/`;
fixtures não confirmam autenticação de fornecedor, modelo executado, ingresso
CLI/MCP/HTTP ou qualificação Windows/macOS.

As jornadas futuras seguem [Consulta documental e LiveSearch](rust-knowledge-ai-live.md), com produtores próprios de Conversation, managed runs, eventlog, run-owner e workspace. A folha single-PROJECT não depende de um gate universal dessas jornadas.

## Recuperação automática na mesma operação Ask

`retrieval` e `selected_slugs` são exclusivos. `mode` aceita `fts`, `semantic`
e `hybrid`; `top_k` é positivo e respeita o limite existente de páginas do
prompt. `metric` pertence somente à consulta semântica/híbrida; fornecê-la em FTS é inválido. O modo FTS não depende de M.
A recuperação usa a própria pergunta, sem previews, e guarda apenas os slugs
ranqueados entre páginas. Semântico/híbrido reusa o único vetor Query observado
na paginação, depois de readiness e EOF dos inputs atuais. O kernel Knowledge
mantém ranking, filtros e combinação; o App não cria outro RRF.

Os documentos escolhidos são lidos por `read_selected_documents` da MESMA
capture Source/NN da busca. O `SearchPage` permanece sem bodies. ReadKnowledge,
Original, seleção viva, configuração efetiva e a lease local do caller Native
ou Web são preservados. Não há conversão Web para Runtime. Híbrido sem M pode
usar hits lexicais reais do mesmo pin com estado `partial`; semântico sem M
retorna `unavailable`. NoHits, NoCorpus e indisponibilidade sem documentos não
iniciam provider. Erros de autorização, integridade ou transporte continuam
falhas.

O resultado automático acrescenta `retrieval` com estado, gerações e slugs
selecionados observados. `provided_slugs`/`provided_pages` descrevem páginas
realmente entregues ao prompt, sem afirmar citações ou modelo efetivamente
executado. O preparo e o writer aplicam os limites reais do prompt antes do
lançamento e não entregam prefixo parcial. Progresso, SSE, custódia de receipts,
cancelamento e join reutilizam os owners existentes.

## Ask one-shot com Claude

O binding Claude agora usa o [consumidor JSONL próprio](../specs/rust_claude_completion.md)
e o [adapter App](../../crates/graphit-app/src/source_server/knowledge_ask/claude.rs).
A preparação documental comum permanece a mesma: seleção/corpus/autorização reais,
pergunta literal e `RequestContext` original. O launcher/model/cwd/ambiente são
capturados pelo serviço; o prompt entra no stdin zeroizante de Process. Os callbacks
emprestam a admissão Knowledge e executam `check_read` diretamente, sem callback
booleano de autoridade.

`stream_event`, conteúdo de ferramentas e IDs nativos chegam como progresso auditado.
`parent_tool_use_id` e `origin` preservam a distinção entre subagent, turno injetado
e resposta ao caller. Apenas o `result` principal pode fornecer answer, depois de
sucesso/EOF/exit/rechecks. Usage ausente não vira zero. O receipt tem provider
`claude`, `known_events` aceitos, contagens de bytes e observações Process reais.
`native_result_json` conserva o frame final para delivery autorizado; guards de
negação removem esse campo e `completion.answer`, retendo contagem/prefixo e custody.
Negação após efeitos não transforma a execução em uma chamada sem receipt.

## Ask one-shot com Qwen

O binding Qwen usa o [contrato JSONL próprio](../specs/rust_qwen_completion.md)
e o [adapter App](../../crates/graphit-app/src/source_server/knowledge_ask/qwen.rs).
O serviço captura launcher/model/cwd/ambiente e envia o documento no stdin
zeroizante text de Process, com pipe não-TTY. A preparação e a admissão Knowledge
são os produtores reais existentes. O callback empresta a admissão e executa
check_read diretamente em volta do progresso, preservando o Original.

O resultado nativo success só permite answer após exit0, EOF de ambos os pipes
e rechecks de entrega. Texto parcial/subagent não compõe a resposta. O emissor
Qwen atual pode omitir a origem em result/error de subagente; qualquer erro
nativo falha conservadoramente, sem heurística de origem. O contrato registra
essa limitação e não atribui ao Graphit a proveniência interna do texto result.
Missing result text/usage é erro de schema; texto vazio permanece empty_answer.

Remaining é transferido ao dono Process antes da projeção/erro. Denial após
efeitos conserva receipt/prefixo/causa e remove answer e native_result_json
tanto no SourceServer quanto no SourceProxy. A qualificação focal e seus limites
estão no contrato; fixtures não qualificam vendor instalado, Windows/macOS,
Conversation Qwen ou o catálogo completo de e6ff.

## Conversation com Claude

O [consumidor Incoming MCP](../specs/rust_knowledge_conversation_app.md) compõe
create/continue com o produtor Claude existente e o store USER. A continuação usa
o ID nativo efetivamente observado e as fontes PROJECT selecionadas sob admissão
atual; list/delete operam sobre metadados Graphit, e clear nativa permanece Kiro-only.
A prova própria de tsk-9953 cobre cold reopen, isolamento, binding e recuperação;
não amplia a qualificação do Ask stateless nem comprova login comercial ou outros SO.

## Ask one-shot com Antigravity

O [consumidor Antigravity](../specs/rust_antigravity_completion.md) seleciona
`agy` pelo CliCompletionProvider de completion, independente do host Agent/Skill/MCP.
A captura startup mantém programa, cwd e ambiente; o modelo da requisição
é passado literalmente. O documento
preparado comum vai em uma mensagem nativa user JSON no stdin zeroizante, com
`--input-format stream-json --output-format stream-json --model MODEL`; o pipe
fecha após a mensagem. Não há documento em argv, shell ou arquivo temporário.

O decoder retorna `Option<AntigravityEvent>` emprestado: eventos desconhecidos
são auditados sem alocação adicional por wrapper. Init fixa a sessão; steps
conservam índices e RawValue nativos de tools/subagents. Somente o response
do result SUCCESS compatível pode virar answer após exit0, EOFs e cleanup/join.
Progresso, inclusive texto de subagente, nunca compõe esse candidato. O consumer
empresta a admissão PROJECT Knowledge real, rechecando a mesma Original antes e
depois de efeitos/progresso/entrega. Remaining passa ao AskService antes de
projeção ou await; denial suprime answer e native_result_json.

AI7 e seis jornadas App CLI/HTTP/STDIO estão funcionalmente qualificadas, com
recibos e limites no contrato; a qualidade estrita afetada também passou.
O runner portátil original falhou por expectativa STDIO herdada e permanece
registrado como FAIL; somente o exact corrigido foi repetido e passou. Fixtures
controladas não qualificam vendor autenticado, outros sistemas operacionais,
Conversation/resume ou Live. As obrigações amplas continuam em e6ff.

## Conversation com Qwen

Cada turno revalida Incoming USER e PROJECT, e a continuação envia somente o ID privado observado em Init por `--resume`. Não usa sessão mais recente nem fallback New. Mismatch, ausência de identidade, erro, cancelamento ou falha de persistência impedem entregar uma continuação como sucesso; o writer permanece até o join real. A política de ferramentas é a nativa capturada, sem yolo/trust ou configurações pessoais alteradas; a admissão Graphit não representa isolamento do processo. Isso não requalifica Ask, não habilita clear Qwen e não conclui Conversation dos demais CLIs.

## Conversation com Copilot CLI

A extensão `tsk-625d` reutiliza o produtor CLI/Process de Ask para
create/continue/cold reopen via Incoming MCP. O store USER mantém binding e ID
privado; a App registra o writer antes de lançar o processo e o retém até
cleanup terminal ou Host join real. Resume usa apenas
`--resume=<UUID-privado-exato>`, com igualdade do ID observado no `result` e
sem seleção latest ou sessão substituta. O ID pode chegar depois do prompt;
não há ACK anterior à inferência. Entrega exige resposta principal, sucesso
real do processo e USER commit com rechecks USER/PROJECT atuais.

A projeção pública Conversation remove recursivamente `sessionId` e `arguments`; `receipt.native_result` é objeto sanitizado, enquanto o terminal completo fica privado no receipt AI. Falha/negação remove resposta, terminal público e projeção USER. Os [contratos e alvos App](../specs/rust_knowledge_conversation_app.md#copilot-incoming-integration) detalham as jornadas. As fixtures não exigem fornecedor ou conta.

## Ask documental com Grok

O [consumidor Grok](../specs/rust_grok_completion.md) usa `CliCompletionProvider::Grok` e o transporte oficial por arquivo privado, sem colocar o documento em argv ou assumir stdin. `--deny` e `*` são argumentos literais da política nativa de tools; a autorização PROJECT governa os acessos Graphit e não vira sandbox do processo. O mesmo registro Remaining conserva o arquivo até observação real do join. Somente text.data compõe o candidato, liberado após end_turn, exit0, ambos EOFs, cleanup e rechecks. O terminal e os tools mantêm JSON nativo/RawValue, sem inventar init, sessão, usage ou texto a partir dos tools. Instalação ou conta do fornecedor não é requisito desses testes.

## Conversation documental com Cursor

A [extensão Cursor](../specs/rust_cursor_completion.md#incoming-conversation-continuation) reutiliza o driver Ask, o store USER e a custódia existentes. O Init nativo observa a identidade; continuidade e reabertura usam `--resume ID` explícito antes do prompt literal. Cada turno mantém modo ask, programa/modelo/environment capturados, seleção física/compilada e admissão USER/PROJECT atual. Nenhum latest, sessão substituta ou novo controle global é introduzido.

O writer acompanha o Process até encerramento real; entregar resposta exige sucesso, exit0/EOFs, persistência e rechecks. Eles usam peer controlado e consumidores Graphit reais, sem exigir binário ou conta do fornecedor. A prova Ask anterior não é atribuída automaticamente a Conversation; clear nativo continua Kiro-only.

## Conversation documental com Grok (qualificada)

A [extensão Grok](../specs/rust_grok_completion.md#incoming-conversation-continuation) usa `end.sessionId` nativo, sem Init fictício, para persistir o UUID privado no store USER e retomar por `--resume ID`. Cada turno exige os mesmos bindings selecionados e USER/PROJECT/Original atuais. Progresso textual antes de End não comprova continuidade nem libera a resposta.

O documento continua em arquivo privado, sem cópia em argv/stdin. Sua TempPath é adotada pela custódia existente antes do primeiro poll e permanece até terminação real, inclusive abandono interno e retorno Remaining. Driver/wire/serializadores Ask são reutilizados; só End/end_turn válido, exit/EOFs, cleanup e commit permitem entrega. A política `--deny *` é a nativa já utilizada, sem promessa de sandbox ou alteração pessoal.

## Conversation documental com OpenCode

A implementação atual aceita somente OpenCode 2. Progresso child-session não compõe a resposta principal.

A extensão tsk-28ee reutiliza o produtor ACP Ask, o preparo documental central,
o store USER e a custódia reais. Create observa NewSessionResponse; continue e
cold reopen usam LoadSessionRequest com o ID privado exato, sem latest, importador
ou criação substituta. O ACK correlacionado confirma Load, cujo schema não tem
campo de ID; cada update precisa corresponder ao ID selecionado. O modelo literal
carregado deve coincidir antes de Set/prompt. O replay pré-ACK é validado e
limitado, mas não integra resposta, uso, progresso ou prefixo do turno novo.
Progresso atual conserva o payload nativo sob a chamada USER verificada, inclusive
IDs de protocolo quando presentes. O seletor armazenado segue privado, fora dos
metadados/receipt final e dos inputs públicos; payload nativo bruto não pertence
a logs ordinários ou evidência pública.

Hooks tipados revalidam USER/PROJECT e o mesmo Original nas fronteiras nativas. O owner ACP registrado acompanha o writer até encerramento real da árvore e retorno de transporte/EOF, inclusive cancelamento, waiter descartado, deadline e shutdown Source. Entrega exige EndTurn/texto novo, driver concluído, cleanup limpo, commit USER e autoridade atual; conflito/negação suprimem resposta. List/delete seguem metadados e clear nativo continua Kiro-only. O [contrato OpenCode](../specs/rust_opencode_completion.md#incoming-conversation-opencode) registra interfaces, erros e focais.

## Conversation documental com Kimi (qualificada)

A [extensão Kimi](../specs/rust_kimi_completion.md#incoming-conversation-continuation) usa a Server API experimental já integrada: Create recebe o ID opaco nativo; continuidade/cold reopen consultam somente esse ID privado e conferem metadata.cwd antes do prompt na mesma sessão. Sem latest, título, fallback, UUID ou Init inventado. USER/PROJECT/Original e bindings selecionados continuam atuais por turno. A mesma custódia retém Arc<KimiServer> e writer até join real, inclusive abandono/cancel/deadline/shutdown. A correlação sessão/prompt/main/turno/UTF-16 e a política manual nativa são reutilizadas; aprovação automática, trust e sandbox não são adicionados.

## Conversation documental com Antigravity (qualificada)

A [extensão Antigravity](../specs/rust_antigravity_completion.md#incoming-conversation-continuation) reutiliza stdin stream-json, store USER e custódia Process/Remaining. O ID privado observado em Init é retomado somente com --conversation ID; o resultado deve confirmar essa identidade. USER/PROJECT/Original e bindings atuais, sucesso nativo, exit/EOFs/cleanup/join reais e commit governam a entrega. Apenas response pertence ao turno atual; usage, num_turns e duração continuam metadados cumulativos do fornecedor, sem métricas artificiais. O getter de engine é exaustivo para os providers ativos, sem None morto; serviço não configurado ainda é erro real antes de efeitos. A rejeição de configuração ausente usa um executável AGY realmente indisponível; Gemini CLI foi retirado do catálogo.

## Escolha inicial de evidência na UI

Uma nova pergunta no browser usa recuperação FTS de evidência por padrão, sem
exigir selecionar a biblioteca inteira antes de escrever. Busca localiza e
mostra páginas; a evidência escolhida limita o que a resposta pode consumir.
Seleção manual continua uma escolha explícita, e `selected_slugs: []` continua
sem documentos. Essa escolha da UI não muda defaults API/CLI/MCP nem ativa
embedding, busca semântica ou reranking.
