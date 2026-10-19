---
title: Consulta documental e LiveSearch na migração Rust
doc_type: architecture
references:
  - target: {type: task, id: tsk-2436}
    relation: uses
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-794d}
    relation: uses
  - target: {type: task, id: tsk-01eb}
    relation: relates_to
  - target: {type: task, id: tsk-a0f9}
    relation: specified_by
  - target: {type: task, id: tsk-08c0}
    relation: derived_from
  - target: {type: task, id: tsk-cd05}
    relation: derived_from
  - target: {type: task, id: tsk-c847}
    relation: derived_from
  - target: {type: task, id: tsk-542d}
    relation: supports
  - target: {type: task, id: tsk-84d5}
    relation: relates_to
  - target: {type: task, id: tsk-b210}
    relation: relates_to
  - target: {type: task, id: tsk-a6e8}
    relation: uses
  - target: {type: task, id: tsk-bb2a}
    relation: uses
  - target: {type: task, id: tsk-1597}
    relation: uses
  - target: {type: task, id: tsk-ca7a}
    relation: relates_to
  - target: {type: knowledge, id: Wiki_Module_Specification}
    relation: supports
---

# Consulta documental e LiveSearch na migração Rust

Esta página orienta os donos de Knowledge, AI e Runtime em `graphit-code`.
O responder documental e a sessão Live têm composições Rust específicas no App.
Esta página distingue contratos implementados, provas focais e integrações ainda
pendentes; a presença de uma interface não qualifica a jornada completa. `tsk-a0f9` preserva fontes, limites,
fixtures e dependências; [fronteiras Data/Records](rust-knowledge-data-boundaries.md)
detalha os snapshots e a publicação ainda necessários.

## Jornadas ativas que precisam permanecer distintas

| Jornada | Responsabilidade |
| --- | --- |
| Compilação por domínio | Knowledge compila documentos deterministicamente; inferência é uma capacidade separada. |
| Consulta documental AI | Knowledge organiza fontes/contexto/citações; AI possui inferência e conversa nativa. |
| Consulta AI HTTP | App vincula a operação ao request/sink e ao catálogo selecionado. |
| Busca MCP | Recuperação paginada FTS/semantic/hybrid; Data possui a consulta, sem síntese conversacional implícita. |
| LiveSearch | App possui sessão no servidor, preparação, turnos, eventos e cleanup. |

Não foi demonstrado um consumidor ativo de geração AI de documentos por domínio.
Agrupar comunidades não comprova essa capacidade. Uma antiga referência a
generator ou um campo no schema não justificam portar helpers sem consumidor.

No MCP atual, `SessionID` participa do binding do cursor; o handler executável não
inicia nem continua uma conversa AI. Na busca nativa, semantic indisponível falha;
hybrid pode retornar lexical com estado `degraded` explícito quando não consegue
query embedding. Essa capacidade deve ter política aceita no novo contrato, sem
alegar contribuição semântica que não ocorreu. Agent off conserva a recuperação
local conforme `tsk-542d`; enablement não concede read authority.

## K-A: responder documental limitado

O consumidor precisa de query, fontes/versões selecionadas, request original,
admissão real, Data read view e Completion/Conversation do produtor AI. Knowledge
organiza catálogo, recuperação inicial, pedidos de páginas e síntese com fontes.
Protocolo de ações, provider parsing, retries, tokens e sessão nativa pertencem a
AI; um marcador textual isolado não substitui o protocolo do produtor.

Identidade de página inclui source/context, slug e geração. Repetir a mesma página não duplica conteúdo; duas fontes com o mesmo slug continuam distintas. O controle de uma consulta precisa considerar os pedidos efetivamente executados, inclusive páginas já solicitadas.

Os limites concretos da consulta cobrem chamadas de síntese e retries, prompt/contexto, páginas e input/output conforme o contrato do produtor. `len/4` é estimativa, não uso observado pelo tokenizer. Limites positivos e erros finais precisam de acordo entre AI, Data e Knowledge.

Resultados distinguem resposta concluída, ausência de corpus, ausência de match,
fonte/página indisponível, denial, quota, cancelamento e erro do provider. Citações
precisam corresponder ao conteúdo autorizado observado; texto de modelo não
concede autorização nem comprova que uma página foi lida. Política de falha parcial
de fontes precisa ser explícita.

Continuação, listagem e exclusão da conversa documental precisam de owner/escopo
reais, persistência honesta e invalidação do binding nativo quando pertinente.
Chat documental, Live session, run, sessão nativa CLI e TaskSession são identidades
distintas. Native CLI ID permanece privado no produtor AI.

## K-L: sessão Live gerenciada

Live prepara um workspace efêmero com artefatos selecionados e ferramentas, depois
permite turnos. Os estados observados são preparing, ready, running, failed e
closed. Preparing rejeita turnos; o contrato exige que uma falha de preparação
termine em failed, com custódia honesta dos recursos eventualmente criados.
A composição agora retém falha e workspace após receber o File de birth; essa
alteração foi exercitada no App21d; suas provas e limites atuais estão registrados abaixo.
Erro de turno e cancelamento explícito permitem ready quando o cleanup é válido.
Closed não pode voltar a ready.

Não depende de um manager ou eventlog universal em nova crate Runtime. O shutdown exige observar o owner real e EOF limpo antes de concluir.

O runner deve reutilizar `graphit_ai::cli_kiro::run_kiro` e a custódia
`KiroProcessOwner` já consumida por `knowledge_conversation`. Esse caminho aceita
um agente Hub selecionado, validado e publicado para o binding real. Não deve
forçar Scout, ManagedRole ou bootstrap de Memory para executar uma investigação.
A seleção explícita indisponível falha antes do workspace, sem troca de agente.
O binding Kiro atual é V3, com `kiro.modeId` e seleção explícita de modelo.
O catálogo/DTO representa essa seleção diretamente, sem campo `preview`.
As evidências anteriores de V2 permanecem históricas; a composição e a
qualificação do runtime V3 pertencem aos produtores Kiro. O browser real das
fixtures usa um peer Kiro controlado pelo SDK; essa prova não equivale ao vendor
instalado. O Main reportou AccessDenied no catálogo de modelos do Kiro instalado
antes dos hooks; essa qualificação externa continua pendente.

Cada run conserva seu Original legitimamente admitido e as condições efetivas de
autoridade, cancelamento e deadline. O contexto de uma assinatura é distinto do
Original do run: sua desconexão não renova nem cancela implicitamente esse run.
Knowledge não fabrica grants, Background ou prazo novo para manter trabalho
expirado. Uma nova tentativa exige admissão legítima.

O owner de serviço é a instalação capturada e verificada após autenticação
Runtime atual. Sessões dessa instalação compartilham deliberadamente esse owner;
não há identidade por conexão CLI. Sessões Web/Native UI exigem USER comprovado:
issuer e subject do `VerifiedUserSubject`, revalidados, permitem construir a
`UserIdentity` Core estável. Persistir essa identidade não concede autorização.
Cada operação exige autenticação e admissão atuais; serviço não vira USER nem
concede acesso automático às sessões locais. `Actor` privado Runtime/Local é
vinculado ao issuer, origem e geração; não deve ser persistido como identidade
USER ou identidade reconstituível após restart. Um campo de request não define
owner.

Envio exige serialização de um turno ativo e fence atual. Dois envios simultâneos
não iniciam dois turnos. A pergunta inicial pode executar uma vez após preparação,
sem depender de subscriber conectado. O App responde pelo lifecycle e pelos
resultados observados usando APIs reais Core/Auth e os produtores existentes.
O histórico usa o journal condicionado Core NativeDocumentNamespace sob
knowledge-live, com owner real e Original por operação. O workspace nasce privado
e conserva identidade física Core; reabrir exige captura e autorização atuais.
Preparação recebe CapturedSourceReader e RenderedContent reais, com a tupla
inteira e o nome efetivo do renderer. A integração Hub selecionada e o MCP
com USER/PROJECT estão qualificados pelos ingressos App24e/24g e pelo Browser
App24f descritos abaixo. Isso não concede integração Hub arbitrária nem transforma
ToolGrants de outro startup em autoridade atual.
Metadados do agente não concedem ferramentas, shell nem acesso a contextos.

## K-E: sink do engine, assinaturas e replay

Na consulta AI direta, o sink pertence à operação; sua desconexão pode cancelar
o processo conforme `tsk-08c0`. Para Live, o serviço App compõe o `KiroSink`
tipado existente e gerencia esse sink independentemente dos subscribers.
Desconectar um subscriber encerra sua assinatura; o run continua sob seu Original
até terminar, ser explicitamente cancelado ou atingir seu deadline efetivo.
O canal HTTP não é o sink do engine. `KiroDeliveryPhase` é fase operacional,
não sequência ou marcador de replay.

Um subscriber lento não deve travar o agente. Falha real de IO ou de limite
concreto do sink conserva a causa e conduz cancelamento, join e cleanup.
A composição precisa entregar eventos incrementais reais; reconstruir transcript
somente após exit não prova streaming. Credenciais e referências privadas não
podem aparecer no progresso, inclusive quando um texto é dividido em chunks.

Eventos preservam estado, preparação, prompt, texto, ferramentas, erros e término,
com correlação observada e separação entre payload público e privado. A sequência
é monotônica. Replay seguido de tail não perde nem duplica eventos, inclusive
cursor à frente da observação e assinatura durante o turno. A assinatura entrega
somente eventos após o cursor admitido.

Os limites concretos de IO, payload, replay e queue, além de cancelamento e
deadline, devem corresponder às APIs efetivamente compostas. Seus contratos
ainda precisam ser fechados, sem quotas agregadas artificiais, novos números ou
política implícita. Cursor e replay conservam scope, generation e autoridade
reais. Corrupção, falha de provider ou perda de acesso não viram histórico vazio.
Native CLI ID permanece privado; output ou tool attempt não provam delivery
nem commit de registro.

O App separa payload público persistível de payload privado transitório.
Se uma falha impedir registrar o terminal, o outcome e o resultado real de
cleanup precisam continuar reobserváveis por um caminho autorizado; não se
fabrica evento gravado ou sucesso. O App retém KnownTerminal em custódia e o inclui em GET/replay e frames
SSE opened/terminal. O valor informa persisted, joined e clean sem inventar
sequência de journal. Live15 comprova terminal não persistido após cancelamento/deadline do Original e reconciliação por nova admissão; falha física de storage com child continua sem prova. Replay distingue cursor expirado, generation incompatível,
corrupção e denial de vazio legítimo. Ack perdido não duplica eventos.

## K-H: fontes, artefatos, restart e cleanup

Hub fornece a seleção real de tipos/versões, configuração e bindings. Conflitos de
versões são validados antes de instalação. A política de partial/coverage/warnings precisa de contrato explícito.
Knowledge lê somente contextos selecionados e autorizados, local ou mounted,
com snapshots Data. Sem source identity/index existentes, preparação não inicializa
nem reindexa o projeto de origem. Viewer não substitui pelo projeto do dashboard.

User Memory não é WikiSource. Seu acesso exige identidade privada confiável e
provider/autorização próprios. Preparação de contexto não cria autoridade. Runtime `tsk-cd05` refina aquisição de contexto,
mas não fornece um LiveSessionManager.

Cancel explícito interrompe o turno e descendentes, mantendo sessão utilizável
quando o produtor confirma cleanup. Close encerra preparação, trabalho e
subscribers, preservando histórico e workspace. Enquanto há execução owned,
Close solicita cancelamento e informa busy até observar join/cleanup. Remove
fecha a sessão e reclama somente recursos efetivamente owned após quiescence
comprovada. Se cleanup atingir seu limite com trabalho ainda vivo, o outcome deve
informar incompletude e aplicar a política do produtor; não apagar o workspace e
declarar sucesso. Remove não exige ACP session/delete nem promete apagar cache
do vendor; Clear nativa de Conversation é outra jornada. Uma espera de duração fixa não comprova essa integridade.

O contrato de restart permite reabrir metadata/transcript com nova admissão legítima e exige representar preparing/running interrompidos sem fingir resume. A recuperação App foi exercitada no App21d com preparação sem Prompt, admissão fresh e proteção do workspace estrangeiro. A revisão do consumidor confirma create/request validation antes de recover e retorno de Failed antes do resolver. A recuperação se limita ao caso Preparing sem Prompt nem native_session, ou ao último turno cujo término já confirmou joined&&clean. O runner só chama kiro_process após a escrita condicionada de Running+Prompt ter sucesso; qualquer erro dessa escrita retorna antes da chamada. Portanto, sem Prompt durável não nasceu child por esse protocolo.

A exclusividade por instalação já é fornecida pelo produtor tsk-2436:
DaemonAuth::start mantém o FileLock de daemon.pid antes de rotacionar a key.
O ForegroundSource produtivo conserva o mesmo Arc até server.shutdown concluir.
Um novo startup bem-sucedido sob esse owner, com o inode do lock preservado,
exclui outro daemon cooperante. SourceServer::new isolado, outro serviço no mesmo
daemon, discovery, PID, Preparing ou SlotNone não demonstram esse startup.

LiveSessionService::begin_preparation reserva o mesmo SessionSlot antes de
create/observe e do primeiro await do resolver Root. PreparationGuard retém o
Original real, owner, existência e geração anteriores ao create. A igualdade da
request é conferida antes de guard.recover_interrupted: Some(session) encerra o
job quando recupera Failed; None segue preparação explícita. Reconciliar um
terminal Ready quiescente permite que o novo turno continue. O guard conserva
o efeito real em recovered_effect(); Root o acrescenta ao erro subsequente da
preparação sem substituir sua causa. Closed não revive; Prompt inconclusivo é Busy.
Não se exige prova universal de join de processos antigos para o caso sem Prompt.

A reserva permanece durante resolver, MCP e run. Outro preparador recebe Busy. Create idêntico pode devolver a Session existente durante a reserva, inclusive sem Prompt; request diferente gera Conflict. Close/Remove sinalizam parada e permanecem Busy até a liberação do guard; shutdown também aguarda essa liberação. O Drop somente libera a reserva correspondente, sem IO. Último Prompt sem TurnDone correspondente joined&&clean continua bloqueando reprepare, Close e Remove, inclusive em Failed. Uma nova admissão pode reclamar somente o workspace físico owned conferido. O reclaim de stores criados por versões antigas não é um importer/cleanup legado Rust a reconstruir.

### Fronteira de preparação refinada

A fronteira salva em PreparedSelection distingue NativeText, InlineRule e
ReadContext. NativeText recebe CapturedArtifact, source_entry relativo à raiz
capturada e RenderedContent obrigatório para agent/skill. O principal também
recebe agent_source_entry real e participa da reserva global, cópia e comparação
de assets; Root preenche esse campo por CapturedHubText.source_entry(). InlineRule conserva
a captura da rule incorporada no agente pelo produtor Root. ReadContext recebe
tupla inteira, fingerprint e referência ao Original real. Esse stamp interno
não é grant nem entra no DTO. Seleções são obrigatórias em ordem e tupla;
variante/tipo, fingerprint ou Original incompatível falha antes do workspace.
Não há cópia de Lance, NN ou corpus documental.

Skill usa a seleção Directory real com SKILL.md root validado. Assets relativos
a source.directory são projetados sob o parent do entry nativo SDK. A cópia
exclui exatamente o entry autoral; os bytes renderizados são escritos com
criação exclusiva. O preflight reserva todos os entries, assets e ancestrais:
arquivos conflitantes ou arquivo/diretório são recusados antes do birth,
enquanto diretórios compartilhados são permitidos. A revalidação compara entries
e assets por streaming, inclusive EOF. Não há rawcopy seguido de overwrite.
Rules não recebem arquivo instalado próprio nem render_rule inventado: Root
compõe o agente via MiniJinja/canonical Markdown wrapper. Tupla, ReaderFP,
source/native entry, proveniência e bytes renderizados vinculam a preparação.

Uma captura Core de raiz File usa source_entry vazio: a chave é relativa ao
próprio arquivo. O basename não identifica esse nó. A projeção valida a chave
com open_file real do mesmo Original; vazio em uma captura Directory não é
arquivo válido. Destinos nativos continuam caminhos relativos não vazios.
A correção conserva File e não amplia a seleção para seu diretório pai.

Essa implementação e sua fixture SDK com assets >128 KiB, colisões de caminhos,
byte extra no EOF e truncamento passaram no focal App16. A fixture compõe bytes
de agente e rule explicitamente; a composição completa do resolver/template Root
continua exigindo prova própria. O caso de assets do agente principal, colisão com projeção Skill declarada e
reprepare alterado também passou no App20. A fronteira de stamps de leitura
passou nas duas fixtures de snapshot do App21d; essa fronteira não prova a captura SDK produtiva Root.

### Falha de preparação e efeito observado

Após o SDK FS retornar o File recém-nascido, o serviço retém esse mesmo handle
em seu slot antes de outra captura cancelável. Workspace::record usa Core fresh
e compara o objeto capturado com o handle owned; o journal recebe o record antes
da projeção. Falha não troca identidade por pathname nem apaga recurso estrangeiro.
O SDK create_private_directory_relative documenta que mkdir seguido de erro pode
deixar uma entrada antes de retornar File. Esse limite é aceito: conservar causa
Interrupted/IO e não adotar o caminho. Não há garantia universal zero-resíduo,
sweeper ou novo produtor FS nesta unidade.

prepare registra a falha do seu estágio. Root pode chamar
fail_preparation(access,session,cause) para falha anterior do resolver/renderer.
LiveError conserva causa primária, NativeDocumentJournalEffect observado e
preparation_persistence_error() secundário, sem expor paths privados no Display.
Se o Original expirou, o slot já owned retém o outcome; o erro atual informa a
persistência recusada e uma próxima admissão write reconcilia record/Failed.
GET permanece somente leitura e não inventa evento ou sequência: antes dessa
reconciliação o journal pode ainda mostrar Preparing. Repetir a reconciliação
não duplica o erro/Failed já observado.

A proteção de geração está salva: o estágio conserva a geração observada e o
helper a compara com Slot e journal antes de registrar pendência/Failed.
fail_preparation recebe a Session observada, cuja geração é DATA, sem conceder
autoridade. A regressão cria uma falha real, remove e recria o mesmo id, depois
aplica o resultado tardio e exige replay/cache/estado novos intactos. O handler
HTTP classifica pela causa primária tipada através de Preparation/History e
conserva journal_effect externo; context_changed, not_ready, busy e closed
continuam códigos distintos. Essa classificação HTTP passou no App21d. HTTP e
MCP agora incluem diagnostic com kind, journal_effect, secondary_kind opcional e
persistence opcional, todos tipados; texto nativo e caminhos privados não são
serializados. O diagnóstico do domínio e sua resposta HTTP passaram no focal
App22c. A resposta Runtime MCP serializada passou no App24c, incluindo
context_changed, not_ready, busy, closed, efeito externo Unknown, causa
secundária e falha de persistência. Essa fixture usa CallToolResult real e
preserva a privacidade; não prova o ingresso autenticado de rede. A regressão
de falha após projeção e geração recriada passou no App21d
com a chave vazia real da captura File, preservando a negativa basenameNotFound.

Retenção pós-File, cancelamento antes de capturar o record, erro após projeção,
reconciliação fresh, cold sem Prompt, substituto estrangeiro e child real com
cleanup incompleto passaram no App21d. As fixtures do guard novo, a recuperação
da pendência seguida de Cancel e a adaptação das observações de terminal passaram no focal App22c.
KnownTerminal continua no cache e na observação GET/SSE, inclusive quando a
persistência falha. O retorno do runner conserva o erro de persistência; os
testes consultam o terminal com admissão fresh real. Essa unidade não qualifica
recovery automático de toda a aplicação.

O [contrato das ferramentas Live](../specs/rust_live_agent_tools.md) detalha o
MCP readonly e sua fronteira de autoridade. O producer Main/Records conserva capturas Knowledge/AST/Imported e admissões
atuais no allowed e nas chamadas MCP, com o RequestContext real do run. A rota
MCP recebe pedidos numa fila limitada owned pelo run e é removida ao término.
ArcSnapshot serializado ou um novo RequestContext não substitui essas provas.
Os testes HTTP/histórico desta folha não qualificam essa integração. O Main
reportou no App20 duas provas HTTP conjuntas com create/run, assets derivados,
quatro MCP stdio e logout com join; essa evidência pertence ao produtor Main.
O runner recebe Option<&LiveMcpLaunch> do produtor Records. A prova compara o
Original e aplica somente GRAPHIT_LIVE_MCP_URL, GRAPHIT_LIVE_MCP_BEARER e
GRAPHIT_LIVE_WEB_SESSION depois de registrar o owner e antes do primeiro poll.
Esses valores e o cwd privado alimentam a redação do sink, sem serem gravados no
perfil nativo. Falha anterior ao poll libera o transporte/permit e registra
resultado NotSpawned; não presume child executado. A prova conjunta reportada pelo Main no App20 exercita essa composição.
Os filtros antigos desta folha continuam provas de histórico/HTTP, com escopo separado.

O contrato Root DerivedPayload exige original_selection do descriptor de origem,
separado da selection da saída renderizada. SourceBinding e seleção original
conservam a prova Core; não se reconstrói Markdown, hash ou grant a partir do
rendered. Derivedpin sem entrada autoral e PROJECTSource externo pertencem ao
resolver Root, cuja integração ainda está em andamento.

### Identidades documentais para viewer e MCP

A fronteira privada acordada com Root armazena ReadContexts na mesma escrita
que publica Ready. default_project é KnowledgeContextStamp::NoCorpus ou
Published { sdk_stamp }; selected contém seleções Knowledge e AST em ordem,
com a tupla completa e o respectivo sdk_stamp. O default deriva de
Session.project e não ocupa uma posição extra em Session.artifacts. Uma
publicação documental vazia conserva Published, distinta de NoCorpus.

Root produz o SHA64 Knowledge a partir da tupla e da KnowledgeGeneration
observada (uri, version, physical_branch, inherited_target); NN não participa
dessa identidade Knowledge. AST usa exclusivamente o SHA64 opaco retornado por
CapturedAstContext::fingerprint(), com selection/backend e observações SDK reais.
Option::None de Graph/Raw representa ausência real no produtor; Some conserva
gerações e pins observados. O mesmo helper captura e recaptura; não se cria
versão zero, string vazia, receita AST paralela ou enum novo no journal. O serviço
valida SHA64 canônico e o Original real da preparação e inclui default e
selecionados no fingerprint. O journal exige o campo read_contexts, inclusive
null antes da preparação; arquivo sem o campo é recusado. Ready/Running
preparados conservam contexts e fingerprint juntos. Nada disso persiste grants,
Original ou pins, nem substitui uma captura SDK. A ordem relativa entre os
contextos Knowledge/AST e cada tupla integral é preservada; missing, extra,
reordenação, troca de tipo/versão ou SHA malformado recusam o journal. Mudar
o stamp AST em reprepare gera Conflict antes de reprojetar os bytes. O default
e o viewer continuam Knowledge; AST selecionado pertence às ferramentas AST.

read_contexts(access,id,generation) exige a admissão Knowledge Read atual e
retorna ReadContextsSnapshot { session, contexts, last_prompt } de uma única
leitura do journal. last_prompt é o turn_id do último Prompt. Geração diferente
falha com Conflict; preparação ausente falha com NotReady. A leitura funciona
fora de Running, após Failed ou Close, sem RunContext, child, recovery ou write.
O consumidor MCP pode conferir Running e o último turno com esse mesmo snapshot.
Root reabre os contextos com SDKs e admissões fresh e compara a geração
observada depois da consulta, sem adotar latest. Fingerprint agregado não
substitui esses stamps individuais.

A implementação de viewer/metadados/stamps está salva no código21d. O Main
mantém a prova HTTP anterior do viewer válida; a ampliação AST tem qualificação
produtiva separada. As fixtures desta fronteira usam Core/Auth/Hub reais para
journal e projeção; NoCorpus e os stamps
fornecidos ao preparador são dados explícitos do caller trusted. Elas não
qualificam leitura PROJECT/Imported ou ausência real de corpus, cujo produtor
é Root. Fixtures dedicadas ao histórico declaram esses dados somente em cfg(test).
Os novos casos cobrem geração, ordem/fulltuple, Original incompatível, SHA64,
campo obrigatório, limites e corrupção sem reparo.

## Ingressos Live em composição

O TOOL Runtime `graphit_knowledge_live_history` e o HTTP Web usam o mesmo
`HistoryOperation`/dispatch e a mesma `Arc<LiveSessionService>`. Runtime exige
credencial atual e permissões Knowledge da instalação. Histórico ausente retorna
vazio após rechecks, sem criar seu diretório. O primeiro write USER exige o
bootstrap Core do namespace com a admissão Knowledge write atual e os limites
Artifact/Payload configurados; não inicializa Memory nem outro corpus. HTTP usa cookie OAuth
real, verifica o bearer com o Broker e admite USER Knowledge por operação;
mutação exige a admissão write correspondente. Histórico USER não exige
PROJECT para listar/reabrir; preparar e executar contextos exige suas admissões
PROJECT/imported separadas. ReadMemory ou cookie sozinho não concede Knowledge.

| Rota HTTP | Resposta |
| --- | --- |
| `GET /api/live/sessions` | `{sessions:[Session]}` |
| `GET /api/live/sessions/{id}` | `{session,events,terminal}` |
| `POST /api/live/sessions/{id}/cancel` | `{requested:boolean}`; pedido não comprova término |
| `POST /api/live/sessions/{id}/close` | `{session}`; Closed irreversível, workspace preservado |
| `DELETE /api/live/sessions/{id}` | `{deleted:true}` após cleanup owned |
| `GET /api/live/sessions/{id}/events` | SSE autorizado, replay seguido de tail |

GET read-only sem Origin exige `Sec-Fetch-Site: same-origin` e
`Sec-Fetch-Mode: cors` ou `same-origin`. Origin presente sempre passa pela
verificação estrita existente; POST/DELETE exigem Origin. Cookie e refresh
continuam no produtor BrowserAuth existente. O transporte revalida live/expiração,
stop e cancelamento também ao retirar cada frame da fila; logout descarta payload
pendente. Um erro posterior a Close/Remove conserva o efeito Core observado,
inclusive Close aplicado antes de uma falha parcial de Remove.

O código Root agora contém a operação em
[operation.rs](../../crates/graphit-app/src/browser_auth/live_execute/operation.rs)
e o preparador em
[prepare.rs](../../crates/graphit-app/src/browser_auth/live_execute/operation/prepare.rs).
A operação cria um RequestContext único e admite USER Knowledge read/write;
o preparador passa agent_source_entry pela captura real, renderiza o agente
e conserva admissões/readers/leases/pins durante a execução. O contrato Root
usa o template central live/agent.md, ferramentas MCP readonly selecionadas,
executable capturado pela composição e endereço local efetivamente bound.
SessionWeb usa seu token único de cancelamento; o job e o mesmo serve MCP são
aguardados após a parada. Esses consumidores estão salvos e compilaram no App20. O Main reportou provas HTTP conjuntas de create/run/MCP;
o Browser App24f tem a prova independente e os limites descritos abaixo. SourceRef.Resources.root identifica o branddir
do projeto; o resolver exige prova Hub pareada e Directory no escopo físico.

O lifetime SSE é distinto do budget RPC de history/Create/Send. A assinatura
nasce com um único Original próprio limitado pela Session real. O mesmo job
captura WebCredentials::expires_at() do guard atualizado e aplica o menor bound
Auth/Session antes do primeiro frame; o getter já considera os tokens verificados.
A conversão UTC→Instant é conservadora. Não se renova Original, credencial ou
TTL, e os rechecks USER/PROJECT/caller permanecem atuais. O limite Auth observado
é dado privado transportado por oneshot até o gate do body, sem entrar no DTO.

Terminal/Ready não encerra a assinatura entre turnos. Drop, logout e shutdown
sinalizam o trabalho owned e aguardam sua saída; o job observa logout mesmo
sem novos eventos. O body confere deadline/expiração e logout antes do sinal
cancel usado para drenagem, preservando as causas deadline/permission_denied;
cancelamento real continua cancelled. Payload já enfileirado não ultrapassa
esses limites. History/Create/Send conservam seu orçamento RPC original.

SSE usa frames `opened`, `event`, `terminal`, `closed` e `error`. `opened`
contém Session, cursor pedido `after` e terminal conhecido. Cada `event` contém
`{sequence,turn_id,data}`, com `data.type` fechado nos onze tipos Live.
Seu SSE id é `generation:sequence`. `terminal` é observação separada sem id
de replay; `closed` encerra a assinatura com Session fechada. IO/auth/limite
não vira EOF saudável.

`ReplayCursor={generation,sequence}` e `Event.sequence` usam string decimal
canônica no JSON, preservando todo u64 em JavaScript. Número JSON, sinal,
zeros à esquerda e overflow são recusados. Cursor HTTP vem de
`?generation=...&sequence=...` ou `Last-Event-ID`; fornecer ambos é erro.
Generation diferente é erro. Cursor futuro pode entregar zero eventos e
continuar observando. O watch precede cada leitura e o filtro de sequência
deduplica o replay. A fila limitada pode esperar apenas seu leitor, sob seu
Original; o sink do engine persiste independentemente dessa fila.

Fonte técnica: [serviço e custódia](../../crates/graphit-app/src/knowledge_live.rs),
[admissão USER](../../crates/graphit-app/src/task_server/browser/live.rs) e
[HTTP/SSE](../../crates/graphit-app/src/browser_auth/live.rs). Fixtures reais
Core/Auth/replay/queue/u64 e OAuth HTTP ficam em `knowledge_live/tests.rs` e
`knowledge_live/browser_tests.rs`. Notificações tipadas não qualificam child,
login vendor, outra plataforma ou viewer.

Close tem ação explícita; GET tardio/abortado e Activity por turn_id conservam seus fences. Os marcadores confirmaram callback inicial completo, batch recebido e nova execução sem publicação Knowledge; AST e supervisor continuaram normalmente. A causa foi a comparação de extensões na relevância incremental: Settings normaliza seletores como `.md`, mas Path.extension() retorna `md`. A captura inicial aceitava o sufixo canônico pelo SDK; o predicado do evento o recusava. A correção usa o mesmo sufixo sem o ponto na comparação, preservando o seletor canônico e a comparação ASCII. Esse focal também reproduziu o problema no App24j sem Broker/S3 e sem modelos. As observações temporárias foram verificadas no ELF Main24l e executadas na reprodução causal; depois foram retiradas. O consumidor existente de Knowledge foi estendido com alterações reais de documento e README.MD sem rescan, verificando nova geração e corpo, além de noop, exclusão de código e cancelamento. Esse helper retorna unit e não fornece assert numérico de geração. O helper próprio abortou por stack overflow no App24n; seus mesmos futuros SDK foram colocados no heap, preservando os asserts e a stack padrão.

A rejeição de um turno antigo já registrado é validada no journal antes de
armar a reserva em memória. Quando o efeito é Unchanged, mantém terminal e
Original válido, sem deixar a sessão busy ou iniciar outro child. Erros com
efeito Applied/Unknown conservam custódia. Cancelar o Original enquanto o run
espera um permit nativo produz outcome Cancelled, assim como após o spawn;
NotSpawned não é execução nativa observada. Essas regressões passaram no Live15.

No próximo ingresso write legítimo, Cancel, Close e preparação tentam registrar
o terminal nativo já observado. Isso permite Ready após join/clean sem renovar
o Original cancelado do run; o workspace permanece para a nova admissão. Se a
reconciliação ou entrega falhar, o efeito Core conhecido continua no erro.

A recuperação de terminal Ready retorna None e permite novo Send. Cancel posterior preserva Applied no erro.

## Integração produtiva e validação

A composição Live Web usa OAuth, permissões USER/PROJECT independentes,
artefatos instalados, preparação de contextos, journal, viewer e o Main MCP
reais. Os testes HTTP e Browser com processo de agente controlado verificam
criação, envio, histórico, referências, cancelamento e encerramento. A prova
de leitura MCP está documentada no [contrato de ferramentas Live](../specs/rust_live_agent_tools.md).
O teste Browser atual também confirmou dois turnos na mesma conexão SSE,
separados por mais de trinta segundos, sem recarregar a página. Esses resultados
não qualificam a autenticação do serviço externo de um agente.

A sessão conserva a geração observada na preparação. Publicar conteúdo depois de uma preparação NoCorpus não autoriza adotar essa nova geração silenciosamente: a leitura deve indicar mudança de contexto e exigir nova preparação. O MCP já tem prova dessa transição.

As composições [single-PROJECT](rust-knowledge-question.md) e
[conversa documental USER](../specs/rust_knowledge_conversation_app.md)
reutilizam os leitores e as admissões existentes. Seleção de versão, geração,
binding e permissões continuam verificadas no ingresso de cada operação.
Oferecer páginas a um agente não transforma sua resposta em citação verificada.

Permanecem necessárias a qualificação nativa do Kiro V3 quando o serviço permitir
acesso ao catálogo, a ponte Web/Broker dos contextos remotos e as plataformas
ainda sem execução nativa. Preservar essas limitações nos registros correspondentes;
sucesso de SDK ou de processo controlado não substitui esses consumidores.
