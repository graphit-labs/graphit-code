---
type: specification
title: Source foreground em Rust
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrects
  - target: {type: task, id: tsk-ba70}
    relation: corrects
  - target: {type: task, id: tsk-1528}
    relation: implements
  - target: {type: task, id: tsk-678a}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: implements
  - target: {type: task, id: tsk-0529}
    relation: implements
  - target: {type: task, id: tsk-6d5a}
    relation: relates_to
  - target: {type: task, id: tsk-f37c}
    relation: implements
  - target: {type: task, id: tsk-9d34}
    relation: corrects
  - target: {type: task, id: tsk-bcf2}
    relation: uses_api
  - target: {type: task, id: tsk-0e74}
    relation: implements
  - target: {type: task, id: tsk-b64c}
    relation: implements
  - target: {type: task, id: tsk-f9c1}
    relation: uses_api
  - target: {type: task, id: tsk-4ca0}
    relation: uses_api
  - target: {type: task, id: tsk-e07a}
    relation: uses_api
  - target: {type: task, id: tsk-e6e4}
    relation: implements
  - target: {type: task, id: tsk-5020}
    relation: uses_api
  - target: {type: task, id: tsk-6442}
    relation: uses_api
  - target: {type: task, id: tsk-ef75}
    relation: uses_api
  - target: {type: task, id: tsk-b9b4}
    relation: derived_from
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
---

# Source foreground em Rust

`graphit_app::foreground_source::ForegroundSource::start` recebe instalação
Core, `ProjectBootstrapInput` por seleção de checkout/versão, índice corrente,
ConfigSnapshot, NativeBinding escolhido pelo caller e contexto original finito.
A seleção do provider e o startup exigem Core `required_deadline` antes dos
efeitos. Contexto de serviço sem deadline falha com InvalidInput antes de
preparar a instalação; identity ensure e reserva do listener derivam o tempo
restante do mesmo Instant original, após novo check causal.
`start_and_publish`, usado pelo CLI de `daemon foreground` e `daemon --managed`,
compõe esse mesmo constructor com a publicação do inventário configurado antes
de `run`. O modo gerenciado carrega configuração e inventário globais, sem
selecionar o diretório corrente como PROJECT.
O módulo não interpreta argv. Configuração inline `--config/-c KEY=VALUE`, ambiente e caminhos
são selecionados pela composição central antes dessa chamada.

Os exports App, o roteamento CLI e o owner Source usam a composição comum de
PROJECT, modelos, Memory, AST, Knowledge, Now e tarefas nos três SOs suportados.
Cada integração continua consumindo seus produtores concretos; tray e o
lançamento detached conservam seus caminhos específicos de Linux. A composição
de fonte não comprova execução nativa em outro SO: compilação por target e
jornadas no host permanecem evidências separadas, com pendências explícitas.

O startup exige a identidade existente de cada projeto selecionado, carrega a
política Hub atual e usa o bootstrap Runtime uma vez por seleção para reter suas
versões reais. O inventário pode estar vazio: a seleção corrente é `None`, sem
PROJECT artificial. Autenticação, controle do daemon e leitores de contextos
instalados continuam compostos antes de compartilhar o servidor. Operações
PROJECT exigem uma seleção viva e sua própria admissão.

Role, Task, Now e Ask recebem a mesma seleção compartilhada antes de clonar o
Source. Ela admite o ciclo vazio → projeto real → vazio; snapshots anteriores
continuam imutáveis e índices inválidos em seleções não vazias falham sem mudar
membership. Ask resolve o executável pelo PATH capturado relativo à instalação;
a execução mantém o diretório do PROJECT efetivamente admitido por requisição.
Dream inicia no primeiro projeto real e é encerrado na retirada correspondente.
Não é necessário reconstruir o servidor ou obter `Arc::get_mut` após servir.
O child de modelos resolve primeiro `graphit-model-server` (com o sufixo
executável do SO) ao lado do executável Graphit em execução, conforme o layout
do pacote instalado. Se esse arquivo não existir, usa o PATH capturado da
composição relativo à instalação. A resolução não altera o ambiente global;
um sibling existente inválido reporta erro. O teste do resolvedor usa um
hardlink do sidecar real nos dois layouts; a execução gerenciada do serviço
continua exigindo sua própria prova nativa.
O mesmo checkout pode ter versões distintas selecionadas; os roots físicos são
deduplicados no realm do único DaemonAuth, sem alterar a ordem das seleções ou
o índice corrente. Esse índice define a seleção usada quando o pedido não
especifica projeto/versão. A versão vazia é resolvida pelo bootstrap atual;
não é uma versão adicional. Seleções duplicadas do mesmo root e versão
resolvida falham com `InvalidInput` antes de publicar descoberta, inclusive
quando uma usa versão vazia e outra seu nome resolvido. Esse erro retém o
proprietário já iniciado para `shutdown` explícito.
Source continua lendo
o caminho físico/versão já resolvidos e relative path solicitado; não consulta
Git, branch, refs ou inventário a cada leitura. Não cria identidade de projeto
ausente nem importa configurações/dados antigos.

As leituras ordinary do Source incluem Read(Ast, Memory, Knowledge) para Runtime
e Local. A composição Runtime acrescenta explicitamente Read(Task), WriteTask,
WriteOwnTask e CoordinateTask para as operações Task produtivas; cada operação
continua exigindo sua admissão real e seleção PROJECT. Os exports nativos de
Knowledge e AST também recebem explicitamente `External(PublishedFilesystem)`
na composição Runtime. `ReadKnowledge` ou `ReadAst` isolados não concedem esse
efeito; Local readonly continua sem permissão para publicar arquivos.
Membership ou um selector
não concede esses poderes. Com Broker configurado,
Runtime recebe a regra real InventoryPublish e PresencePublish para o provider
capturado, com LocalBrokerKey no provider Local ou NativeAccess no Native.
RemoteRequest também pertence à composição Runtime configurada; a presença não
transforma essa regra em grant PROJECT. A composição administrativa Core/Auth
permanece separada.
A política Source não representa o restante do catálogo ou login web de
projeto. Os limites de conteúdo/documento/snapshot/paths reutilizam os
valores Hub existentes e constructors Core/Auth. Não há catálogo, pool ou
sistema de reservas adicional. A política de credenciais escolhe TTL30min,
máximo24h e refresh1min; sessões administrativas15min/máximo1h são parâmetros
válidos do SDK para a composição administrativa própria.
Essa validade de credenciais do issuer é distinta do TTL de access tokens JWT
do Broker; o módulo não emite nem assina tokens Broker. A renovação Native é composta pelo owner
com `NativeProfileManager::refresh_broker`, conforme a seção de renovação abaixo;
a validade usa a expiração exata do token e não muda o TTL emitido pelo Broker.

Listener default é 127.0.0.1 com porta do SO. `mcp.host` aceita endereço IP
loopback; `mcp.port` aceita inteiro u16, zero seleciona porta livre. Seleção
inválida falha antes de criar chave. Com `task.browser` global explícito, o
Foreground acrescenta login browser e leituras Task com decisão PROJECT fresca
no mesmo listener; veja [Task browser Foreground](rust_foreground_tasks.md).
Sem esse opt-in, as rotas cookie/browser não são compostas; Task nativo continua
capturado no Source. Host externo, CORS, hospedagem
UI e demais módulos permanecem entregas da composição ampla do daemon. O listener
Core é o mesmo socket entregue ao SourceServer, sem bind probe ou rebind.
A descoberta só é publicada após inicializar o handler e reservar esse socket.

O controle nativo usa `POST /admin/daemon` com `{"operation":"status"}` ou
`{"operation":"stop"}`. O Runtime autenticado precisa de `Configure`
incondicional; Origin, headers ambíguos e credenciais inválidas são recusados.
A resposta contém o PID autenticado e `running` ou `stopping`. Ela não concede
acesso PROJECT. O cliente usa o endereço loopback exato, sem proxy nem redirects,
e a Original finita cobre a leitura da chave e o HTTP.

Stop prepara e valida o ACK antes de cancelar o token existente. O encerramento
normal aguarda handlers e jobs e remove discovery, chave e singleton; o handler
não espera por si mesmo e nenhum PID de discovery autoriza sinalizar processos.
O teste pelo executável real em `crates/graphit-app/tests/daemon_control.rs`
qualifica recusas sem efeito, cancelamento/expiração e essa limpeza completa.
A composição dos comandos/gerenciadores está em
[serviços do daemon](rust_daemon_services.md).

`run(self, &Cancellation)` aguarda a execução do servidor ou cancelamento do
proprietário. Depois aguarda servidor e jobs nativos antes do cleanup. EOF de
um cliente fecha aquela conexão; o proprietário segue atendendo outros clientes.
O lifetime do serviço não usa o deadline do startup.

O CLI retorna exit0 em managed/detached somente depois de `run` retornar
`Ok(())`, incluindo seu cleanup externo. O caller foreground interativo conserva
exit130 quando cancelado. Falhas de startup e cleanup preservam seus caminhos
de erro; a configuração de parser não exige substituição do processo.

Ao terminar o loop de `run`, o Source escreve uma única linha de diagnóstico
`Graphit Source terminal` com `selected_branch` e `error_kind`. Os labels fixos
são `service_stop`, `served`, `external_watcher`, `now`, `now_refresh`, `dream`
e `user_dream`; o kind é o `io::ErrorKind` do resultado ou `None` em sucesso.
O label identifica a branch selecionada, sem atribuir a ela a causa do erro:
o resultado pode incluir shutdown/join do servidor executado nessa branch.
A linha ocorre depois desses joins internos: sua ausência não comprova que
nenhuma branch foi selecionada. Ela antecede o cleanup externo de
`ForegroundSource` e não imprime erro
bruto, request, caminho ou credencial. Ela preserva o erro original, sua cadeia
de causas, owners retidos e a ordem de encerramento; não comprova a causa de
uma falha nem o sucesso do drain nativo.

Falhas de configuração ou de admissão de um checkout no início de um lote
interrompem aquele passe e permanecem diagnósticos explícitos do módulo.
AST, Memory e Task conservam o trabalho pendente para o próximo evento real;
não iniciam polling nem abrem stores por um fallback. Uma raiz movida ou um
acesso revogado não transforma esse erro de job em falha do watcher já retido.
Shutdown continua cancelando e aguardando os owners: erros de join, panic,
estado envenenado e cleanup continuam sendo propagados.

A reconciliação Memory em background usa uma única
`RequestContext::service` cancelável para o corpus inteiro, como AST e Knowledge.
A política `hub.limits.request.deadline_ms` é capturada como duração para cada
RPC de embedding Document/Query, sem deadline agregado de corpus. O mesmo
Original, admissions e lease permanecem válidos durante todos os lotes;
cancelamento, autorização, EOF, CAS e joins não são substituídos. Consumers
HTTP/CLI conservam seus próprios prazos finitos.

Quando o M compartilhado fica pronto, a composição solicita um rescan aos
supervisores existentes das seleções com Embedding habilitado e consumidores
AST, Memory ou Knowledge. Isso inclui Knowledge-only quando AST e Memory estão
desligados. O foco `model_ready_rescans_existing_knowledge_only_supervisors`
exercita o owner real passando de M ausente a child CPU pronto e a entrega do
rescan a dois supervisores Knowledge-only, sem alterar arquivos ou configuração
durante a partida do modelo. Essa prova cobre a notificação; a inferência e a
publicação Knowledge têm sua qualificação própria no pipeline.

A alteração de `daemon.models` é GLOBAL. Os controles de configuração já
observados pelos supervisores notificam o owner Source de forma coalescida.
Quando existe um owner Browser, o watcher externo do próprio Source também
observa o arquivo GLOBAL, inclusive sem PROJECT; ele compartilha a observação
de gramáticas nativas quando aplicável. Os controles dos watchers PROJECT são
preservados. Não há polling. Esse owner resolve a configuração completa,
captura os `ConfigId` de todos os slots e solicita o refresh autenticado do
child com sua posse Runtime e uma Original finita. Somente depois do ACK
publica a nova Arc nos consumidores Source/Task e no slot semântico compartilhado
por Role, Browser, AST e Knowledge, e solicita rescan pelos handles existentes.
Nenhum seletor de perfil por PROJECT é criado.

Uma requisição já admitida no child conserva a geração antiga até terminar.
`ExpectedModel` no parent é metadata: uma captura antiga ainda não admitida
pode receber `ConfigChanged`; esse erro não comprova reload. Configuração
inválida continua sendo erro observável e conserva a geração válida, sem
publicação parcial. Configuração ausente ou slots explicitamente vazios são
aplicados como mapa vazio: o ACK antecede a retirada do modelo no parent, e
borrowers já admitidos continuam possuídos até terminar. O controle leve do
child permite reativação; o encerramento Source aguarda seus recursos reais.

Se o cliente perde o ACK por transporte, deadline ou cancelamento, o resultado
permanece incerto e o erro é exposto. Source retira o anúncio semântico para novos
requests, preservando os borrowers existentes. Uma reconciliação efetiva de
checkout reutiliza o child e refaz o refresh com uma captura Original fresca;
não há retry automático ou polling. Um ACK válido já recebido é publicado sem
uma segunda checagem App que possa descartar o efeito confirmado. A retirada do
último consumidor fecha o child real, inclusive quando os slots estão vazios.
São consumidores os PROJECT com Embedding e Memory, AST ou Knowledge, e Memory
USER com Memory e Embedding globais habilitados. Source oferece USER também sem
Browser e sem PROJECT; Task não concede esse acesso. Captura, refresh e retirada
consultam separadamente os módulos efetivos PROJECT e a configuração Global
atual, preservando os overrides Command e Environment capturados. Todos os
PROJECT com Embedding desabilitado não retiram uma seleção Global USER válida;
`modules.embedding=false` em Command ou Environment continua prevalecendo.
Habilitar recursos não substitui autenticação Native, subject verificado,
admissões USER ou a lease do serviço no consumo. A configuração Global Knowledge
sozinha não cria outro consumidor sem PROJECT.
O campo `TaskServer.memory_model` alimenta o consumo Browser; Task CRUD e FTS
continuam lexicais. Source mantém a seleção enquanto há um consumidor
habilitado. Desabilitar Embedding retira os recursos semânticos também do Browser e
encerra o child se não houver outra demanda adquirida.
Rerank é independente: sua configuração no provider não cria demanda. Source
retém somente a identidade do provider e ConfigId após uma aquisição local real
admitida por Runtime. Cada refresh recaptura a lease Rerank e a configuração
atual; essa identidade não concede autoridade. O child permanece possuído para
essa demanda, sem anunciar Embedding, e a identidade é retirada com seu lifecycle
ou com a mudança do provider. A alocação do engine segue o comportamento lazy do
ModelService existente. USER Dream tem seus próprios recursos.

Falhas de jobs por projeto são registradas com suas causas, recovery keys e efeitos
observados, tanto durante a operação quanto no cancelamento. Elas não repetem a
publicação nem impedem a drenagem dos outros jobs. O callback deixa de iniciar
trabalho ao observar o cancelamento; todos os workers iniciados são aguardados.
Uma publicação aplicada ou incerta continua representada pelo erro/outcome do
SDK e pelo diagnóstico; o encerramento do daemon não a transforma em sucesso.
Não há classificador paralelo de marcadores de cancelamento.

O job Knowledge em background usa uma única Original de serviço cancelável para
documentos e semântica, sem prazo global do corpus. A política Hub de duração
permanece capturada como limite de cada RPC Models; callers com Original finita
continuam limitados pelo menor prazo. As admissões PROJECT, a mesma Arc Original
e o cancelamento são preservados até os efeitos e joins.

O lote de cada supervisor é uma future async retida pelo owner. Capturas
síncronas de controles AST e configuração Memory usam closures delimitadas no
pool blocking existente do Tokio. Os produtores mantêm suas próprias fronteiras
Native; o produtor AST conserva a fronteira do parser e de seus valores Rc.
Compõe o produtor AST, incluindo Raw e semântica, e Knowledge com `tokio::join!`;
aguarda ambos
antes da reconciliação Memory ou do retorno do lote. O supervisor reúne esse mesmo
trabalho antes de parar o watcher, inclusive após cancelamento. O lote não retém
um worker blocking externo enquanto aguarda os produtores ou RPC Models. A
regressão `knowledge_native_batch_current_thread_serves_authenticated_http_and_joins`
verifica atendimento HTTP autenticado, publicação Knowledge e join no runtime
`current_thread`. Essa regressão cobre o reactor e os joins do Source no Linux.

AST semântico executa somente após Raw válido, quando há publicação nova ou refresh
semântico. Knowledge conserva os paths e flags do mesmo lote e pode publicar seus
documentos enquanto AST aguarda Models; uma falha Raw continua permitindo a sua
tentativa independente. Cada branch mantém suas próprias admissões, Original,
resultados e erros, com o cancelamento compartilhado do supervisor. As duas futures
ficam em `Box::pin`, para não somar seus estados SDK grandes no frame de stack do
lote. Erro em uma branch não descarta a outra. Isso remove a dependência de espera no consumidor;
não promete inferência paralela no child compartilhado. O produtor AST conserva a
publicação Graph e seus pins Raw/NN atuais, sem antecipar um pacote incompleto.
A regressão `supervisor_knowledge_publishes_while_ast_models_response_is_pending`
exige uma geração documental Knowledge nova com a geração AST NN ainda na base,
segurando a entrega de todas as respostas reais após uma alteração AST fresca.
Após a liberação, exige a publicação NN do novo arquivo. Cache válido não cria uma
obrigação de quantidade de RPCs, conexões ou fragmentos TCP.

Cada supervisor conserva a obrigação de reconciliação Raw completa até uma
execução bem-sucedida, inclusive quando já existe um Main publicado. Uma falha
Raw mantém essa obrigação. A coalescência conserva paths concretos junto ao
pedido de rescan; overflow pode perder esse detalhe, mantendo a
reconciliação completa obrigatória. No ramo AST, o próximo lote real com paths
explícitos publica primeiro esses FILEs e tombstones, sem promover o delta a
uma enumeração completa. Sua seleção de compiler fica restrita aos mesmos
paths, preservando a cauda semântica anterior. Erro sem pending tipado mantém
a seleção de recuperação aberta. O sucesso do delta não limpa a
obrigação de full, inclusive quando ela nasce de um novo rescan nesse lote.
Após publicar o delta, o produtor serial retoma o full se o token de background
continuar vivo; um evento novo pode fazê-lo ceder entre FILEs. O lifetime e a
Original continuam protegendo o FILE iniciado. Apenas o full concluído limpa
sua obrigação; mudanças de configuração ou gramática mantêm a reconciliação
completa exigida. Não há replay de commit preparado nem alteração das flags e
paths entregues a Knowledge e Memory. Erros posteriores à publicação preservam
seus efeitos conhecidos e também exigem reconciliação na próxima admissão
válida, sem presumir rollback.

Knowledge captura Source sem adquirir e liberar antecipadamente um writer. Antes da captura, prepara somente os diretórios internos PROJECT pela API idempotente de Core, com o mesmo scope e `Original`, sem arquivo ou lock de escrita. A publicação mantém o writer e o CAS reais do SDK. Deadline, negação, corrupção e `KnowledgeAfterPublication` mantêm suas causas e os efeitos conhecidos. Falhas de join, descoberta e cleanup dos owners continuam sendo falhas do encerramento.

Source e Now conservam a primeira `JoinError` no próprio owner quando drenam
jobs concluídos. Uma nova requisição mantém seu próprio resultado e `Original`;
a falha anterior é devolvida no encerramento, depois de aguardar todos os jobs.
Abandonar o waiter de shutdown não descarta handles nem a causa. A qualificação
Now de `tsk-9a23` passou nos três casos afetados de pânico, novo caller,
waiter cancelado e fechamento normal da subscrição; o consumidor Source tem
seu caso próprio em `tsk-9d92`.

A leitura nativa de Knowledge importado usa
`RoleIssuer::admit_authenticated_imported_knowledge_read` com o caller Runtime
ou Local autenticado, seu próprio `Original`, instalação e grant `ReadKnowledge`.
A admissão conserva o emissor e a instalação; não adquire autoridade PROJECT
nem converte o caller em Web. A factory e os leitores lexicais importados HTTP/MCP foram qualificados.
O Main compilado também exporta Knowledge PROJECT, importa o pacote físico e
consulta o novo contexto por MCP após criar seu LocalLink. Essa jornada passou
em 13,38 s. A busca FTS pelo Main também passou, em 15,81 s, com paginação
PROJECT/importada, cursor, preview, ausência do registro e erros de seleção;
o Clippy dos targets afetados passou em 28,94 s. A busca NN importada continua
em integração; esses resultados lexicais não a qualificam.

AST importado usa `admit_authenticated_imported_ast_read` com `ReadAst`
incondicional para Runtime/Local. Web conserva o verifier e a política do
provider configurado. `capture_imported_ast_embedding_service` retém essa
mesma prova, instalação e `Original`, com origem Runtime, Local ou Web real;
permite somente o embedding local, sem autoridade PROJECT ou requisição remota.
Os cinco testes Auth afetados passaram. As consultas importadas NativeGraphAPI/MCP
com quatro modos e Web foram qualificadas separadamente. O Main compilado também
passou em imports, Source e FTS de dois contextos reais, fora do checkout e com AST
PROJECT desabilitado. Exportar esse contexto pelo Main, importar o pacote e
consultar Source/FTS do novo vínculo passou em 10,23 s, incluindo os casos
negativos e encerramento por sinal. Os modos NN reutilizam a rota GraphEntry
qualificada nos consumidores Native/MCP/HTTP; não foram todos executados pelo
binário Main. A jornada do navegador conserva sua qualificação própria.

Uma falha Auth durante o I/O Raw conserva o `AuthError` como causa. O mapper
consulta o mesmo `Original` após a falha: cancelamento ou prazo efetivamente
observados definem `Interrupted` ou `TimedOut`; uma requisição viva continua
reportando sua negação. As keys e a versão de publicação aplicada ou incerta
não são descartadas por essa classificação.

O ingresso humano `UserPromptSubmit` do host configurado verifica o caller, o mesmo `Original` e a tupla física de projeto e versão viva antes de sinalizar Dream. O pulse ocorre antes do bootstrap opcional de Task e independe de uma sessão Task; leva somente um timestamp. Eventos de filesystem não são atividade humana.

### Renovação Native dos canais Now

No delta de `tsk-cab0`, o mesmo owner Now trata o evento real
`ServiceUseError::NeedsRefresh`, inclusive se ele ocorrer durante o primeiro
publish/welcome. Antes de `run`, a admissão do inventário também pode solicitar
renovação quando devolver exatamente `InventoryAdmissionFailure::Auth(NeedsRefresh)`
com efeito `NotSent`. Só após uma rotação bem-sucedida essa publicação é novamente
tentada. Efeito possivelmente aplicado ou erro remoto não permite essa repetição.

O owner cancela e aguarda os publishers atuais sem cancelar seu próprio lifetime,
retém uma única operação administrativa real de `NativeProfileManager::refresh_broker`
e usa o caller Runtime/AdminParent, verifier e limites OAuth existentes. A operação
preserva CAS, efeito remoto, resultado da publicação Core e prova de cleanup pendente.
Cancelamento sinaliza a operação e aguarda seu resultado. Uma espera abandonada não
abandona o job retido; shutdown faz join antes de remover Auth/descoberta/chave.
Falha de rotação é terminal nesse owner, sem novo POST automático.
Em falha de startup, `start_and_publish` devolve o Source ainda possuído em
`ForegroundFailure.owner`; o caller deve consumir `owner.shutdown()` e preservar
a causa original. Esse cleanup não autoriza outra rotação.

Depois do sucesso, projetos correntes recebem novas capturas, credenciais e snapshots.
Se a renovação ocorrer antes do startup, os publishers continuam começando somente
em `run`, com publish/welcome reais. Rotação e readmissão usam os prazos finitos já
configurados; o ciclo publisher conserva a Original de serviço cancelável. Não há
nova margem de expiração, polling ou cache de tokens. `Superseded` continua fatal.
Perfil sem refresh token exige recuperação administrativa.

A focal de JWT com dois projetos desativa AST, Memory e Knowledge pelos controles
produtivos para isolar renovação, dois publishers e consumidores reais sem corpus.
O bootstrap expirado tem execução independente. A regressão antiga com corpora e
indexadores ativos permanece aberta: o Broker observou heartbeat expirado com JWT
válido, e Runtime investiga as fases de leitura/composição/envio. Essa jornada não
é qualificada pelos PASS isolados. A classificação de expiração entre adquirir a
lease e receber o primeiro welcome foi revisada na fonte; esse intervalo estreito
não recebeu uma fixture controlada própria nesta rodada.

A folha `tsk-f37c` concluiu a supervisão Now em Linux e sua integração Source.
A atualização dinâmica do inventário segue qualificação própria em `tsk-9a23`. Os gates de produtores e
hosts Windows/macOS permanecem separados. O owner retém a mesma Arc das seleções Core entregue ao Source/tasks,
e um job por seleção nasce em `run`. Cada future cria seu próprio
`RequestContext::service` cancelável sem deadline, autentica Runtime e captura
PresencePublish, PROJECT e versão reais dentro da operação. Nenhuma lease ou
Original é campo autorreferente do owner. `run` serve HTTP em uma tarefa Tokio
possuída enquanto inicia os jobs Now, sem aguardar Welcome.
O sinal interno do publisher só muda depois do Welcome genuíno; ele continua
distinguindo a falha inicial terminal da recuperação após prontidão.
O mesmo loop atende comandos PROJECT e observa os resultados terminais dos jobs.
HTTP autenticado não depende do primeiro snapshot
ou welcome: só o Now sinaliza sua prontidão após publish/welcome reais de todos
os canais. Uma falha terminal tipada de publisher Now encerra essa publicação,
inclusive antes do primeiro welcome. `next` aguarda e retira o job; o Source
registra a causa e continua atendendo Task, HTTP e administração local, mantendo
os publishers irmãos. Não há retry de Forbidden, concessão de ACL ou readiness
fabricada. `NeedsRefresh` conserva o caminho próprio de renovação Native; uma
falha global dessa renovação, falha de join do owner ou erro de cleanup não
classificado conserva o encerramento do Source. O cancelamento explícito do
serviço ainda encerra e aguarda HTTP e todos os owners antes do cleanup. A tarefa continua sendo
pollada durante os awaits de refresh; não há segunda publicação de readiness.

Os entrypoints CLI do daemon foreground e gerenciado usam o mesmo runtime Tokio
com scheduler multi-thread e seus defaults. Os demais comandos conservam seu
runtime atual. Os publishers continuam como tarefas async no `JoinSet`: o pool
blocking não retém uma thread por canal enquanto o WebSocket aguarda mensagens.
As capturas Native síncronas compartilham os workers do scheduler; não ocupam
um único reactor responsável por todo o serviço. Essa composição preserva os
empréstimos de Original, caller e lease. Ela não garante a cadência de um
publisher durante um poll síncrono demorado; a jornada do serviço instalado
continua necessária para qualificar sua liveness.

Updates seguintes publicam na metade do heartbeat timeout negociado; sucesso de send não é ACK Broker. Uma única recomposição real pode permanecer pendente entre esses envios. Nesse intervalo, o publisher usa a última observação genuína do mesmo canal/PROJECT/versão, preserva seu `generated_at` e cria novas admissões e máscaras por módulo antes de cada envio. Uma leitura concluída substitui a observação; erro terminal, cancelamento ou abandono encerram a renovação. Task/session denied seguem failed/forbidden honestos.

Na fixture Native com índices ativos, o Runtime produtivo tem `Read(Task)` e o
seed Local sem S3 cria e assume uma tarefa e sua sessão no mesmo PROJECT. O
publisher e o observador continuam Native com OIDC/JWKS e ACL reais; o Source
resolve seu storage pelo produtor atual e Discovery Disabled válido. Os asserts iniciais
verificam dados disponíveis, IDs reais, vínculo e contadores da sessão 0/1.
Esse caminho não instala negação Task; as provas existentes de masking com
negação e revogação efetivas permanecem separadas e inalteradas.

A qualificação focal atual em Linux aprovou quatro casos, cada um com um teste
executado, zero falhas e zero ignorados: dois projetos com shutdown possuído
(`native_foreground_now_two_projects_and_owned_shutdown`, 32,26 s), recuperação
de perda Broker (`foreground_now_recovers_broker_loss_with_fresh_state_and_cancelable_wait`,
10,85 s), cancelamento/falha remota de refresh sem retry
(`native_foreground_refresh_cancellation_and_remote_failure_do_not_retry`, 2,62 s)
e Memory GGUF ativa até stop explícito
(`native_foreground_now_with_active_memory_isolates_publishers_until_stop`, 11,29 s).
O primeiro caso verifica Task com ID real, administração pela CLI compilada,
head novo do publisher irmão, Superseded, Forbidden, retirada e cleanup. Sua
negativa de expiração ainda encerra o Source pela falha global `now_refresh`.
Os dois casos de recuperação/refresh foram preservados após mudanças exclusivas
nos consumidores dos outros dois casos. As falhas anteriores de seed Native sem
storage admitido, write sem rescan e expectativa de Knowledge ausente foram
preservadas como falhas dos consumidores anteriores. A compilação Release foi
medida separadamente; essa execução não qualifica instalação ou hosts vendors,
Windows/macOS, CRUD Memory MCP Root ou S3 Enabled.

O Native15 histórico passou integralmente em Linux em 92,97 s, incluindo observação
genuína repetida seguida de head novo, versões/reinícios, takeover, ACL e retirada
reais, expiração assinada, NoCorpus, cancelamento e joins. O recibo preserva os
FAIL anteriores e a correção exclusiva dos asserts da fixture. Esse PASS não
ativa o release nem qualifica hosts nativos Windows/macOS ou inferência de modelo.
Na sub-jornada atual de expiração sem registros semeados, Task, sessions e Memory
continuam `no_corpus` com []/0/false/null. O job Knowledge pode já ter confirmado
uma geração documental vazia: nesse caso seu estado é `available`, mantendo
esses campos vazios e nenhum erro. Publicado vazio e ausente permanecem distintos.

Os dois casos Local afetados, owned-shutdown e Memory ativa com joins terminais,
também usam o `Read(Task)` produtivo atual e não semeiam Task/session. Essas duas
seções devem ser `no_corpus`, []/0/false/null. O teto Runtime histórico sem leitura
Task foi supersedido por essa política; as negativas reais e a publicação Memory
dessas jornadas permanecem preservadas. Seus resultados focais ficam separados
do Native15 no recibo.
O owned-shutdown passou em 24,53 s. O caso com Memory ativa falhou no upgrade
Local com HTTP 504, antes dos asserts Task/geração Memory, sob a antiga janela de
operação de 2 s da fixture. O Source identifica deadline antes do upgrade, sem
isolar verificação, SQL ou agendamento. Esse caso ordinário não exige latência de
2 s: seu caller passa agora `RealtimeHttpConfig` explícito de 30 s pelos helpers
Local existentes. Default, outros callers e registry/read/list continuam em 2 s;
Original, token, heartbeat e negativos reais permanecem iguais. A execução focal
com 30 s passou pelos dois snapshots iniciais, mas falhou em 93,66 s durante a
espera da publicação Memory: Source terminou com `Remote(Deadline)` e concluiu os
joins. O log não identifica o comando Broker original nem comprova gerações
Memory completas; a falha fica preservada. A reconciliação passa a recuperar
somente os notices remotos `closed`/`deadline` após readiness, com nova admissão.
O focal do classificador passou, cobrindo os doze notices e os erros locais
terminais. A primeira tentativa após o ajuste falhou com HTTP 504
`Deadline`/`MayHaveApplied` durante a espera de snapshot e fica histórica: precedeu
a reemissão da dependência Broker final pelo Cargo. Com Broker auth-first e
consumidor reemitidos, Memory ativa falhou em 150,58 s no loop inicial de
snapshots: o subscriber direto recebeu `Remote(Deadline)`. O helper cancelou o
Source, que retornou `Stopped` e concluiu os joins. O subscriber emprestado termina
com qualquer notice de erro e não possui reconexão. Essa falha não comprova saída
do publisher, duas visões iniciais bem-sucedidas ou gerações Memory completas. A
jornada Local não foi qualificada por esse resultado. Native15 e owned-shutdown são reutilizados.

Após as mudanças finais dos produtores de autorização Local readonly e lookup
Core, o mesmo seletor Memory ativa foi executado uma vez, sem alterar a fixture,
no App25. M05 falhou em 79,67 s: o subscriber direto recebeu `Remote(Deadline)` no
loop inicial de snapshots. Após o helper cancelar o serviço, o join do Source
retornou `Remote(OutcomeUncertain)`. O helper retirou o owner retido da falha e
concluiu seu shutdown adicional com sucesso; `retained_owner: false` foi impresso
depois dessa retirada. O log não ordena a origem desse erro em relação ao
cancelamento nem comprova duas visões esperadas ou as gerações Memory. O baseline
do verifier Local passou em 3 ms; commits Knowledge não comprovam Memory nem a
causa da falha anterior. M01–M05 ficam preservados, sem ampliar retry ou prazo.
O `Debug` usado em M05 omitia `ForegroundFailure.service`: quando shutdown falha,
o Source preserva ali o erro operacional anterior, e `error` descreve a falha do
shutdown. Portanto o `OutcomeUncertain` impresso não identifica necessariamente
a primeira falha e pode ser do cleanup; o recibo histórico M05 não expõe a causa
preservada. O Debug atual inclui service, sem reclassificar OutcomeUncertain ou
afirmar a causa específica daquele erro histórico.

Com os produtores finais Core (FILE local e EOF global), App (startup concorrente),
Knowledge (contexto de erro) e Broker (read-commit), App26 reemitiu o target de
integração. O seletor Local Memory ativa inalterado passou uma vez em M06
(86,36 s): duas views esperadas, campos Task/session sem corpus, duas gerações
Memory reais e revogação terminal de presença com shutdown e cleanup do owner.
O container privado exato estava ausente ao fim, sem exclusão ou sinal externo.
Esse PASS conjunto não identifica qual delta resolveu M01–M05, cujos históricos
permanecem. Native15, owned-shutdown e classifier são reutilizados; a jornada
Linux não comprova integração do Source instalado nem runtime nativo Windows/macOS.

Depois do primeiro publish/welcome, uma queda de conexão recuperável encerra
somente a operação Now. O job conserva sua seleção e o Source continua atendendo
operações locais. O socket, a lease e o payload antigos são descartados; a espera
cancelável começa em 1 s e dobra até 30 s, reiniciando após welcome válido. A nova
operação de serviço autentica Runtime novamente, admite a seleção e a credencial
atuais, publica o inventário completo corrente com o mesmo id da instalação e
compõe um snapshot novo. A recuperação de transporte não renova token nem
reaproveita sequência Now; a seção de renovação Native descreve seu caminho próprio. Essa
publicação idempotente de estado também é permitida após perda de rede com efeito
possivelmente aplicado; OutcomeUncertain remoto e incerteza de refresh/rotação
continuam terminais. Não há polling de saúde nem grant adicional InventoryRead. O transporte HTTP e
a admissão Broker preservam uma categoria Connection apenas para timeout nativo
ou IO de conexão conhecidos; Transport genérico não autoriza retry.

Reset TCP sem Close e falhas de conexão tipadas, além dos códigos Close 1000,
1001, 1012 e 1013, permitem essa recuperação. Os notices JSON `closed` e `deadline`
também permitem recuperação depois do primeiro publish/welcome, descartando a
operação anterior antes da nova admissão. O Broker usa `closed` para expiração
somente de heartbeat e verifica autenticação antes do budget de operação.
Expiração Auth, inclusive junto de budget ou heartbeat, permanece terminal;
takeover real continua `superseded`. Deadline local SDK/Original, cancelamento,
frame/opcode/UTF-8 inválidos, falhas TLS, transporte desconhecido, Close 1008,
outros notices JSON, incluindo `outcome_uncertain`, e retirada de registro
continuam terminais. O erro tipado Native `NeedsRefresh` segue o caminho de
renovação descrito acima; outros erros Auth/JWT e cancelamento permanecem terminais. O startup inicial exige publish/welcome
reais; sua falha não é convertida em inicialização offline. Cancel/drain Now e shutdown dos demais
owners precedem descoberta/chave/Auth cleanup; falha conserva owner e causa para
recuperação explícita. Pedidos Task usam o snapshot live de seleções capturado
por request, com tuple exato e rechecagem de retirada; não restauram a seleção
inicial como fallback.
[O publisher emprestado](rust_runtime_now_publisher.md) e
[o Source Now](rust_source_now.md) têm provas próprias; seus PASS não comprovam
esta integração Foreground. O consumidor Native real em Linux está qualificado
no caso sem modelo/configuração global e na jornada combinada R5 com M ativo.

As jornadas consumidoras estão em
[foreground_now.rs](../../crates/graphit-app/tests/foreground_now.rs), nos testes
`native_foreground_now_two_projects_and_owned_shutdown` e
`native_foreground_now_local_owned_shutdown`. Eles usam Foreground, Auth/Core,
corpora Lance e Broker TCP/WebSocket/PostgreSQL reais. A rodada final passou
com dois testes em 256,35 segundos; Clippy com warnings como erros passou em
34,60 segundos após a jornada combinada, e as três regressões de configuração do Source passaram. A
jornada Local foi isolada após HTTP 504 no upgrade do observador em uma
tentativa anterior; o isolamento não estabelece a causa daquele deadline.
Prepare a [fixture PostgreSQL do Now](../development/rust-fast-guard.md#native-qualification) antes de executar os testes.

~~~text
cargo xtask cargo test --locked --offline -p graphit-app --features postgres-qualification --test foreground_now native_foreground_now_two_projects_and_owned_shutdown -- --ignored --exact
cargo xtask cargo test --locked --offline -p graphit-app --features postgres-qualification --test foreground_now native_foreground_now_local_owned_shutdown -- --ignored --exact
~~~

O caso sem modelo/configuração global exercita o encerramento sem jobs M
ativos. A prova complementar real em `tsk-6d5a` cobre M e seus supervisors,
sem Now ativo. O caso histórico
`native_foreground_now_with_active_memory_terminal_joins` passou em 104,39
segundos: model server compilado, ORT CPU qualificado, configuração global
somente na instalação temporária, identidade resolvida pelos produtores reais
e gerações semânticas Memory publicadas via SDK em dois projetos. O terminal
Now por ACL encerrava ambos os canais e o Source; esse comportamento global
foi substituído pelo isolamento por publisher descrito acima. A prova histórica
usava ORT/ONNX e não qualifica o consumidor GGUF atual. O caso mantido agora é
`native_foreground_now_with_active_memory_isolates_publishers_until_stop`: usa
o produtor de fixture GGUF real, `GRAPHIT_EMBEDDINGGEMMA2_MODEL` e
`GRAPHIT_TEST_MODEL_SERVER`, preserva Memory ativa depois da retirada dos canais
Now. Depois de um write Native autorizado, o hook Kiro Stop autenticado solicita
o rescan do owner Memory, e o teste exige outra publicação semântica antes do stop
explícito do serviço. O write direto do Store não é um evento de rescan do Source.
Essa jornada não afirma CRUD Memory no catálogo MCP Root nem execução do vendor Kiro.
O helper consulta
a identidade atual pelos produtores de metadata/Memory e conserva a fixture
física até o join. A execução Linux atual com GGUF passou em 11,29 segundos,
com um caso aprovado e nenhum ignorado. A tentativa anterior que aguardava rescan
após somente o write Native falhou; o consumidor passou a usar o boundary Stop real.

A variante histórica `native_foreground_now_with_active_memory_admin_stop_joins`
passou em 17,42 segundos com a CLI compilada real na fixture ORT/ONNX isolada.
Esse resultado não qualifica sua adaptação GGUF, não reexecutada nesta unidade. Depois
das gerações Memory dos dois projetos, recebe HTTP200 e um frame Now nativo
não vazio e mantém esse stream aberto durante o Stop Runtime autenticado,
com o PID esperado. Verifica o ACK `stopping`, exit0 do processo daemon, reap
do único child de modelos observado, fechamento da conexão HTTP parcial e
remoção do listener, da chave e da descoberta. As gerações Memory já estão
prontas no momento do Stop; o trabalho ativo comprovado é o stream HTTP Now,
junto aos owners de supervisão e modelos. O cenário de terminal por ACL e
as negativas qualificadas do produtor Models permanecem preservados.

Uma tentativa anterior respondeu 504/Deadline no inventário, antes de iniciar
M. A rodada qualificada executa primeiro o verifier Local real, que passou em
117 ms e aquece esse caminho. Isso não identifica a causa do deadline anterior
nem comprova sua reconexão fria. O Source conserva a causa e o owner quando o
startup falha; a recuperação continua explícita, sem aumento de timeout.
O produtor está disponível em
Linux; essas provas não qualificam Windows/macOS ou compatibilidade de agentes.
Kiro usa V3 atual. Os testes de compatibilidade seguem a documentação do agente,
sem exigir binário instalado, conta ou plano ativo.

`shutdown(self)` também permite encerrar uma instância ainda não servida ou
repetir cleanup de um proprietário retido. Usa uma nova operação finita para
remover somente sua descoberta e a chave Runtime, liberando o singleton por
último. Falha retorna `ForegroundFailure`: `error` preserva a causa, `owner`
retém recursos quando ainda precisam de cleanup, `runtime` preserva o outcome
SDK e `service` retém um erro do servidor simultâneo ao erro de cleanup.
AuthStartupFailure permanece dentro da causa com seus recursos SDK próprios.
O caller deve inspecionar esse outcome antes de retry; Drop libera handles,
mas não prova remoção de arquivos. Descoberta alterada não é removida e a chave
fica possuída até a recuperação explícita do caller.

Falhas de join conservam descoberta, chave e owner para recuperação, depois de
aguardar os demais componentes. Falha ao remover a descoberta também interrompe
a retirada da chave. Se a retirada Runtime já teve efeito `Applied`, um erro
posterior continua reportado com o outcome real, sem devolver um owner apenas
para repetir a remoção aplicada. `NotApplied` e `Unknown` conservam o owner.
O shutdown do Source aguarda Live mesmo quando Ask falha e preserva os resultados
anteriores na seleção do erro retornado.

Memory, Dream, Ask e ManagedRoles distinguem diagnósticos terminais de trabalho
pendente. A falha observada continua sendo reportada; depois das esperas da
chamada, seu diagnóstico pode ser consumido sem perder o estado físico já
drenado. Uma nova tentativa aguarda somente os owners ainda pendentes.
Dream conserva cwd, run e application enquanto o observer ou o host ainda
possui cleanup. Um relatório terminal `Unknown` não comprova reaping: sua causa
continua erro, e os recursos conservados só são liberados após o drain real.
A regressão `ask_join_failure_still_drains_the_real_retained_live_child` compõe
o erro de join de Ask com um child Live retido; a leitura do recibo de join
não realiza o cleanup que cabe ao Source.

Após uma falha recuperável de retirada PROJECT na fase `dream_join`, o Dream
parado continua possuído para retry de cleanup. O loop monitora somente Dream
ativo pelo token de cancelamento real; o término desse owner parado não encerra
o listener nem os serviços dos outros projetos. Erros de um Dream ativo
continuam sendo observados pelo caminho de falha global.

O transporte encerra o listener e junta as conexões que possui. Cabeçalhos incompletos são fechados ao cancelar, sem esperar o cliente. O ACK autenticado de Stop pode terminar sua escrita somente até o deadline absoluto da própria requisição; esse prazo não é renovado pelo encerramento. Os jobs aceitos continuam sendo drenados antes da retirada de descoberta, chave e singleton.

Os testes desse módulo exercitam recursos e transportes reais do componente.
A folha `tsk-0e74` acrescenta jornadas de publicação no SDK e no CLI compilado,
com SIGINT/SIGTERM reais. Quatro casos locais e seis regressões Foreground
passaram. A jornada CLI corrigida exige inventário ausente antes de cada
lançamento e novo PUT observado antes do sinal; a jornada SDK passou com
restart unchanged, nova versão, histórico ausente, 401/restore, cancelamento,
deadline e limites reais de encoding/resposta. Receipts e falhas anteriores
ficam em `.build/rust-runtime-foreground-inventory-20261011/`.
Registro de serviço por SO, integração completa de módulos, updates contínuos,
consumidor NativeAccess, modelos e hosts instalados têm entregas próprias.
Essas jornadas de startup não comprovam Windows/macOS nem substituem os testes
contratuais dos agentes atuais, que não exigem binário, conta ou plano ativo.

Contratos relacionados: [entrada Source](rust_source_entrypoint.md),
[clientes Source](rust_source_callers.md) e
[descoberta do serviço](rust_source_discovery.md).

## Diagnóstico opt-in das leituras Source

O executável inicializa uma única composição padrão `tracing` no bootstrap do
CLI. O filtro lê `RUST_LOG` com a sintaxe de
[`EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).
Ausência, valor vazio ou filtro inválido deixam o diagnóstico desligado (`OFF`).
A saída formatada e os eventos de criação/fechamento dos spans vão para stderr;
stdout continua reservado à saída do comando ou protocolo.

Para discriminar a fila/worker e captura Knowledge, a abertura Lance e as seções
locais de Now, defina no ambiente do processo Source somente os targets necessários:

```text
RUST_LOG=graphit_app::source_server::knowledge_search=debug,graphit_data::lance_branch_store=debug,graphit_daemon::project_now=debug
```

Para diagnosticar a publicação Raw AST, acrescente
`graphit_app::foreground_ast_sources=debug`. O span `index` registra as
fronteiras estáticas `capture`, `walk`, `prepare` e `publish`; a falha registra
somente `error_kind`, sem payload, path, consulta ou erro bruto do SDK.
As fases delimitam operações, não a causa final; `publish` indica início do
commit, não sucesso.

Quando Raw falha durante `walk`, o target
`graphit_core::artifact_store::native_source::selection=debug` distingue as
fronteiras de binding, inventário, controles e arquivo selecionado que retornam
`InvalidData`. O evento contém somente um `boundary` estático, sem caminho ou
payload; preserva o erro e as verificações de integridade da captura.

A variável deve ser herdada pelo daemon que atende a requisição. Defini-la apenas
no cliente `graphit mcp` não muda o filtro de um daemon já iniciado. Em um serviço
gerenciado, os registros seguem o destino de stderr da unidade, incluindo o
journal quando esse é seu destino configurado.

Os spans dessas três fronteiras omitem todos os argumentos; seus eventos usam
rótulos estáticos. Nenhum caminho, URI, credencial, consulta, body ou erro SDK bruto
é acrescentado por essa instrumentação. `queue_enter`/`worker_enter` e os marcadores
de captura delimitam Source Knowledge; `commit_handler_start`/`commit_handler_end`
e `load_start`/`load_end` delimitam as duas esperas SDK. Now marca as etapas Task,
Memory e Knowledge e seus checks de admissão. Um marcador inicial sem o final
correspondente localiza a etapa ainda sem conclusão; não prova sua causa.
O filtro conserva a semântica padrão de `RUST_LOG`, sem allowlist adicional ou
subscriber próprio por módulo/operação. Este diagnóstico não altera retornos,
cancelamento, admissões ou as verificações de integridade.

## Publicação configurada no startup

A seleção faz pré-leitura do documento Auth privado pelos produtores Core/Auth
reais, sob o mesmo Original. Apenas a preparação e a identidade da instalação
repetem operações Core idempotentes já usadas no startup. Auth, perfil e
credenciais são lidos; não há escrita de configuração pessoal. Documento
exatamente ausente, NoActive e provider Local sem Broker escolhem o modo local.
Missing, Stale, decode inválido, falhas físicas, cancelamento e deadline mantêm
suas causas; não se tornam ausência.

Com provider Local e Broker, a regra usa o nome e endpoint capturados do provider.
Depois do commit Core, listener e descoberta, o owner autentica o caller Runtime
com sua chave real emprestada. O empréstimo cai antes do await. O PUT usa os fatos
Core completos e o Original original, incluindo seu deadline absoluto e sinal de
cancelamento. Falha retorna ForegroundFailure com o owner retido e a causa tipada
de publicação; o efeito de rede e o commit local continuam observáveis. O caller
faz shutdown explícito, sem rollback do Core, replay ou retry do PUT.

Provider Broker Native usa regras Runtime InventoryRead/InventoryPublish com
NativeAccess e RemoteRequest ordinary, nome e endpoint exatos do provider capturado.
O owner retém um BrokerVerifierCache privado junto ao mesmo DaemonAuth, sem lease
ou token persistido no cache. O PUT recebe modo Native explícito; seleção e lease
revalidam o Original, perfil e binding, e a igualdade do Arc Original é checada
antes de discovery/JWKS. Falha de verificação conserva causa e owner para cleanup,
sem fallback Local, login, refresh ou grant PROJECT. O produtor Auth `tsk-e07a`,
cache `tsk-5020` e getter `tsk-6442` estão entregues. A integração `tsk-e6e4` está
qualificada nos consumidores Daemon/App e CLI Linux; sua própria jornada Code SDK → Broker Native
com o launcher `tsk-84fd` passou em `native-tcp-01.log` (1 teste, 25.08s).
Essa prova não qualifica hosts instalados. O teste positivo específico
`native_foreground_publishes_and_retains_owner_after_acl_denial` exercita `f.start`
real com o launcher Native público: publicação whole, retirada Publish403 com
commit e owner retidos, shutdown, restauração ACL e publicação atualizada. Seu
resultado passou em `app-native-01.log` (1 teste, 8.99s), com metadata/JWKS reais,
Token/UserInfo0 e cleanup confirmado. Os testes comuns do App passaram em
`app-focal-01.log` (4 PASS, 3 ignored). Local Foreground e CLI compilada passaram
em `app-local-01.log` (1 PASS, 20.76s) e `app-cli-01.log` (1 PASS, 4.86s),
incluindo cleanup SIGINT/SIGTERM. Clippy focal Daemon/App com `-D warnings`,
fmt edition2024, diff check e links locais passaram; revisão Source↔docs
confirmou as regras Native, assinaturas, Original e retenção/cleanup do owner. O GET usado como oracle é HTTP independente, enquanto
a prova do consumidor GET Native do Daemon é a jornada indicada acima.

Com o binário público fornecido pelo input call-local
`GRAPHIT_BROKER_NATIVE_INVENTORY_FIXTURE_BIN`, executar após compilar o App:

```text
cargo xtask cargo test --locked --offline -j1 -p graphit-app --test integration -- foreground_broker_inventory::native_foreground_publishes_and_retains_owner_after_acl_denial --ignored --exact --test-threads=1
```

Os testes Local anteriores e assertions armazenadas não comprovam JWT Native válido.

O cache reutiliza os limites Network do client e a função pura verifier_limits
da factory administrativa: token=max_scalar_bytes, JSON ContentLimits,
max_keys=max_collection_entries, TTL metadata/JWKS300s e cooldown unknown-key1s.
Esses TTLs não renovam access JWT, deadline Original ou ACL. Sem Native na
composição, o campo cache é None. Não se herda o HTTP loopback da factory admin.

O client usa HTTPS/TLS normal e o endpoint real como origin. O bool readonly
`broker.allow_loopback_http` aceita HTTP loopback somente com true explícito e
default false, capturado por ConfigSnapshot de arquivo ou `-c`. Não há novo
framework ENV, proxy ambiente, TLS desabilitado ou roots pessoais. Request e
response usam TransportBytes, in-flight usa TransportInflight e o request
mantém RequestDeadlineMs do Original. Os demais parâmetros App são URL8192,
headers65536/count32, idle por host1/30s, certificates8/bytes65536. O constructor
SDK valida os limites, sem clamp; limites incoerentes preservam erro.

Inputs SDK contendo apenas checkouts válidos podem publicar o histórico Core de
um checkout que desapareceu. Isso não muda o selector CLI: ele inclui todos os
checkouts registrados e o constructor ainda exige a identidade física de cada
input. O CLI mantém erro visível para checkout histórico ausente. Mudar essa
seleção e publicar updates contínuos precisa dos donos e producers App próprios;
startup ou restart não substituem essas jornadas.

O contrato das ferramentas Now/instâncias e do stream autenticado do Source está em [Now e instâncias no Source Rust](rust_source_now.md).

## Registry existente para leitores browser

Source captura sempre o TaskPolicy nativo e injeta diretamente o handle do Registry global da instalação com os limites nativos já selecionados, inclusive quando `task.browser` está ausente. `SourceServer::attach_tasks` cria o TaskServer sob o mesmo DaemonAuth e o mesmo provider de seleções live; o browser configurado recebe um handle desse mesmo TaskServer. Não há outro Registry, issuer ou store. A permissão Web de leitura importada continua habilitada apenas para os providers browser capturados da configuração trusted, sem aceitar provider/grant do corpo e sem camada PROJECT nessa decisão. Os leitores readonly e a qualificação backend própria de `tsk-ef75` estão no [contrato de referências browser](rust_foreground_record_references.md); a qualificação G1 de `tsk-b9b4` tem evidência própria abaixo.

## Autostart dos consumidores CLI no Linux

Os comandos de trabalho que usam Source preparam e validam seus argumentos,
configuração e seleção antes de iniciar o daemon. Os consumidores Graph, Task,
Knowledge e instalação de agentes/hooks entregam ao transporte o mesmo pedido
preparado; não reinterpretam argv depois do startup. Um ambiente de transporte
Role é recusado antes desses efeitos e conserva seu ingresso próprio.

A composição CLI usa o launcher de [serviços do daemon](rust_daemon_services.md).
Um processo Running autenticado é reutilizado. Com gerenciador disponível, Start
registra o serviço somente quando ausente, preserva o registro e a opção de login
existentes e aguarda readiness autenticado. Uma falha desse gerenciador é causal.
Somente sua indisponibilidade no Linux admite o processo detached. O CLI não
incorpora um Foreground oculto nem mantém o daemon preso ao processo do comando.

Quando a operação usa um checkout real, a composição captura seu ProjectId,
root físico e chama `source_entry::maintain_checkout` antes do RPC,
com `select=false` e a admissão do Runtime. Captura e preparação não concedem
acesso PROJECT. A sessão stdio reutiliza essa mesma implementação de manutenção
uma única vez; entradas globais e contextos instalados não inventam um checkout.
O comando automático deixa a versão vazia para o serviço observar o checkout
atual. `--project-version` permanece no pedido de leitura ou nos defaults do
proxy; não é encaminhado como versão a manter. Uma leitura de versão ausente
conserva seu erro e não acrescenta essa versão ao inventário. A manutenção
operacional explícita conserva seu comando próprio de seleção e registro.
`source_entry::address` e os clientes HTTP/MCP continuam leitores da descoberta
existente; o autostart pertence à composição do executável.

Managed/foreground, consultas de status, stop, retire, Auth/configuração, operações locais de pacotes/modelos e transportes Role conservam seus fluxos próprios. Now usa uma Original finita para startup e preserva a Original do stream para acompanhamento contínuo. As provas de serviço Linux disponíveis incluem comandos separados Start/Status/Stop com e sem gerenciador e preservação do login. Entradas inválidas e comandos excluídos não iniciam o serviço.

## Task no Root Source e CLI

A composição introduzida em `tsk-b9b4` anexa Task ao State Source antes de servir. O catálogo MCP atual usa as 27 operações nativas ativas em Linux, ou as seis leituras nos demais sistemas. O dispatcher entrega o contexto RMCP real a TaskServer; não cria um wrapper de contexto vazio. A rota `POST /api/tasks/native/{operation}` compartilha o listener com o único `/mcp` Source, usando nomes de operação `snake_case` e envelopes nativos completos. Browser `/api/tasks` conserva autenticação cookie e PROJECT próprios; consulte [Task nativo](rust_task_mcp.md).

O CLI `graphit task` usa `task_entry` para discovery, conexão e chamada MCP autenticada ao Source. Os 28 subcomandos Linux usam nomes como `create-session`, `revise-session`, `get-task` e `session-heads`; outros sistemas expõem as seis leituras. Cada campo do input nativo é uma flag específica; listas aceitam flags repetidas, sem envelope JSON no CLI. `--project-dir` e `--project-id` selecionam o tuple; `--project-version` é a opção global. A seleção não concede autorização. SourceProxy só completa `selection` aninhada quando o schema da ferramenta declara esses campos; valores explícitos inválidos continuam disponíveis ao servidor para rejeição. `task schema` consulta as tabelas e colunas permitidas; `task query --table tasks --column id --page-size 10 --top-k 10` usa a consulta finita/projetada nativa de Records. O input obrigatório inclui seleção de tabela/filtros, colunas, `page_size` e `top_k`; o cursor de continuação é opaco e não concede autoridade. [O contrato Task MCP](rust_task_mcp.md) descreve o envelope e distingue corpus ausente de resultado publicado vazio.

Quando `--project-dir` identifica um checkout e não há versão explícita, o consumidor Task captura `GitCheckout` e `VersionContext` pelos produtores Core, sob a Original e o deadline do cliente. O wire envia a versão efetiva desse checkout junto ao NativePath. A ambiguidade de seletores parciais continua sendo erro no protocolo; a captura não autoriza nem mantém um projeto. No proxy, essa captura serve somente a seleção Task aninhada, preservando os contratos das ferramentas com seletores planos.

Exemplos de sintaxe, com quoting Unix e um ID ilustrativo a substituir pelo Task real:

```sh
graphit task create-session --idempotency-key example-session --title Exemplo --description 'Demanda de exemplo' --strategy 'Executar a demanda'
graphit task get-task --id 01KSH1CRFFG8Z74B5ZS78WW809
graphit task session-heads
```

O Source possui e aguarda os jobs Task no shutdown antes do cleanup de descoberta/chave/Auth. `ForegroundTasks::shutdown` encerra somente BrowserAuth, e não repete o join desse TaskServer compartilhado. A troca da versão da mesma raiz substitui a tupla anterior, que passa a ser recusada.

No Linux, o mesmo startup anexa o serviço Now existente antes de compartilhar o Source. `graphit_now` em modo `local` exige `project_id` e `version` mantidos, sem provider, perfil Broker ou cache de verifier. O owner opcional de cache é reutilizado para os modos Broker e instâncias; suas permissões e serviços próprios continuam obrigatórios. O [contrato Now](rust_source_now.md) descreve os estados por seção e a seleção atual.

## Configuração dos indexadores

Os indexadores customizados são imagens Docker selecionadas pela configuração
`ast.scip.parsers`, no formato `.ext=imagem,.outra=imagem`. A lista efetiva usa as
camadas normais de configuração. O watcher de configuração existente informa
`controls_changed`; não há loader de bibliotecas externas nem watcher ou
subprocesso pai dedicado à sua substituição. Variáveis de ambiente são capturadas
no startup e exigem nova invocação quando mudam. O contrato dos containers e da
publicação SCIP está no [guia de extensibilidade](../guides/ast_extensibility.md).

## Banco AST entre publicações FILE

O contexto de uma publicação progressiva conserva o cohort PROJECT/version, a
captura de diretório e os pins Lance entre FILEs. O owner forte de `LiveGraph`
pertence à operação que lê ou publica uma FILE. A publicação composta mantém o
owner e o fence retornados pela mesma captura até concluir grafo, invalidação NN,
FTS, pending e manutenção de índices. A recaptura usa o registry existente e
recusa mudança da identidade física do diretório; não cria outro grafo para
substituir silenciosamente o contexto em andamento.

Observações de FILE são valores owned antes dos awaits de parsing e Source
histórico. A publicação de externals SCIP adquire o banco depois da importação
do resultado do compiler. O consumidor NN mantém sua captura durante a inferência
e os checkpoints de uma FILE, e a libera antes da próxima FILE mesmo com backlog
pronto. A fila, os metadados do modelo e o estado de inicialização continuam no
loop externo; a nova captura não repete descoberta ou reconciliação do modelo.

Na execução produtiva, o `ChangeBatch` de um delta explícito delimita a
restauração de compiler pendente às suas FILEs e subárvores, com comparação de
componentes de caminho. Esse escopo é independente da seleção NN. Full,
retomada sem novos paths e reparo da receita lexical mantêm a restauração
completa. Pending de outras FILEs permanece durável para essa retomada; sua
existência não faz um delta de outra linguagem iniciar o compiler alheio.

Leitores ativos continuam conservando seus próprios handles. Sem outro owner,
o último drop pode executar checkpoint nativo síncrono; a FILE seguinte paga a
abertura do mesmo banco e a validação de catálogo, namespace e estado salvo.
Isso reduz a retenção do produtor durante um full, mas não comprova limite de
RSS nem elimina memória de uma FILE ativa ou dos stores Lance. O
[contrato do grafo local](rust_ast_live_graph.md) descreve o lifecycle e o fence.

O publicador estrutural conserva o locator, o cohort e os diretórios capturados,
mas não um snapshot Lance durante a passagem inteira. Cada operação recaptura
o corpus sob o fence existente e conserva a proteção de leitura até terminar
seu preparo e commit. O snapshot é liberado antes do próximo FILE ou gate,
permitindo que a limpeza por NN FILE encontre o corpus sem um reader estrutural
ocioso. Leitores ativos continuam protegidos e podem adiar essa limpeza. Essa
manutenção cobre `corpus.lance` e, após commits Source por FILE e no EOF,
`source.lance`. O mesmo helper consulta a cadência de 32 versões antes de limpar;
não executa GC pesado a cada commit. Source usa capturas admitidas e SHA por FILE,
sem depender de consultas a versões históricas. Leitores Source retidos e branches
continuam protegidos pelo SDK e pelo lease existente.

## Testes offline de Source e publicação AST

Após `cargo fetch --locked`, as suítes `source_server`, `foreground_source` e
`foreground_ast_semantic` executam pelos alvos Cargo abaixo. Cada teste usa
instalação, PROJECT, credenciais de fixture e porta loopback próprios. Os clients
HTTP locais desabilitam proxy ambiente e limitam o tempo de requisição. A espera
pela publicação consulta tabelas e FILEs esperados com prazo finito.

```text
cargo xtask cargo test --locked --offline -p graphit-app --test integration -- source_server:: foreground_source:: foreground_ast_semantic::
```

Os cenários de branches usam Git 2.54+ do toolchain, repositório temporário sem
remotos e configuração global/sistema isolada na criação da fixture. O cenário
`ast_only_publishes_current_graph_without_native_nn` publica Rust, JSON e Java
pelo Foreground real com embeddings desabilitados e PATH privado. O mesmo teste
é compilado para Linux, macOS e Windows. Ele também publica uma FILE posterior
e verifica que o pin histórico da FILE inalterada continua válido.

Os oracles verificam o Source e seu hash no pin retido de cada FILE. A espera
por conteúdo atualizado compara com o arquivo esperado, sem exigir que linhas
NN ou observações do grafo compartilhem a versão global mais recente do Raw.
A falha de gramática e sua recuperação são verificadas pelo teste unitário
`grammar_failure_acknowledgement_preserves_source_and_recovers_without_models`
em [scip_pipeline_tests.rs](../../crates/graphit-app/src/foreground_ast_sources/scip_pipeline_tests.rs).
O mesmo produtor processa dois PROJECTs, retorna o erro efetivo antes do reparo
e preserva os pins de Source e NN. O segundo PROJECT progride com configuração
válida. Esse recorte Linux usa o peer RPC controlado existente, sem pesos de
modelo. As qualificações com inferência real verificam atualização de perfil,
conteúdo e consumo nos dois projetos.

Os dez cenários de inferência nesse último alvo são qualificações explícitas
com `--ignored`, `GRAPHIT_TEST_MODEL_SERVER` e
`GRAPHIT_EMBEDDINGGEMMA2_MODEL`. Eles exigem o executável compilado e o GGUF
fixado. A suíte padrão preserva as asserções de publicação e Source sem carregar
esses pesos. A execução nativa em cada SO é verificada pelo CI correspondente.

Os testes de consumidores em `graph_server`, `ast_source` e `foreground_ast_*`
aplicam o mesmo isolamento de proxy. Requests JSON têm timeout de transporte.
Clients MCP limitam conexão e leitura; handshake e RPC obedecem ao Original ou
a um timeout por operação, preservando a sessão SSE. As provas de shutdown
pollam o waiter retido antes de descartá-lo. A fixture SCIP conserva seu handle
do produtor em erros e panics, cancela os tokens e faz join com prazo antes de
aguardar o shutdown do serviço de compiler.

O cenário de scheduling espera a preempção real do job retido antes de liberar
o FILE. Publicação, tail, tombstones e identidades históricas são observados nos
consumidores, aceitando eventos admitidos em um ou mais batches. O coalescing
da fila é verificado separadamente pelos testes do supervisor no Daemon.
O delta deve aparecer enquanto o tail permanece pendente; a exclusão Graph e
FTS de um evento independente é confirmada após o `full_completion` do rescan,
preservando o sibling que não pertence à subárvore removida.

O teste `full_and_nn_backlog_release_native_owner_between_files` prepara
histórico real com commits SDK de rows inalteradas no corpus temporário. No
gate após o primeiro NN FILE, consulta `Dataset::versions()` e exige pruning
enquanto a segunda FILE ainda tem projeções pendentes, preservando seus oracles
de Source, hash e identidade. Esse teste cobre a limpeza por NN FILE; o EOF do
produtor estrutural tem sua própria fronteira de execução.
