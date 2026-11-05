---
status: implemented-library
task: tsk-8b30
session: ses-c98a
corrections: [tsk-4b82, tsk-0c26]
references:
  - target: {type: task, id: tsk-cab0}
    relation: implements
  - target: {type: task, id: tsk-1528}
    relation: implements
  - target: {type: task, id: tsk-9c4f}
    relation: implements
  - target: {type: task, id: tsk-4a63}
    relation: implements
  - target: {type: task, id: tsk-8b30}
    relation: derived_from
  - target: {type: task, id: tsk-820f}
    relation: relates_to
  - target: {type: task, id: tsk-0c26}
    relation: implements
  - target: {type: task, id: tsk-d751}
    relation: implements
  - target: {type: task, id: tsk-3b96}
    relation: relates_to
---

# Supervisão de checkouts em Rust

O crate `graphit-daemon` contém observação nativa e lifecycle de supervisores. O App compõe esses componentes com os produtores AST/wiki, jobs, caches, modelos, UI/MCP/tray e serviços do sistema, conforme o [lifecycle de memória](rust_daemon_memory_lifecycle.md). Testes isolados destes componentes não qualificam toda essa composição.

## Identidade e consumidores

`ProjectContext` recebe `ProjectId` e `VersionContext` do Core. `SupervisionKey` usa ID, checkout absoluto e versão capturada. Dois worktrees do mesmo projeto conservam supervisores distintos; mudar versão substitui o predecessor após seu término.

`ProjectSupervisor::start(context, watch_config, producer)` cria o watcher fora do executor assíncrono e executa uma leitura inicial. O produtor recebe contexto, `ChangeBatch` e `Cancellation` por chamada. Existe uma chamada de produtor por supervisor em cada instante. As alterações recebidas enquanto ele trabalha alimentam o próximo lote. Eventos de filesystem, inclusive remoção de arquivo, alimentam o produtor; não atualizam atividade humana de Dream.

O produtor deve observar cancellation e terminar sua limpeza antes de retornar. O supervisor sinaliza cancellation e aguarda a future; não aborta a future que ainda possui recursos do SDK. Falhas de produtor/worker chegam ao resultado de `stop`. `park` aguarda a mesma limpeza e devolve somente o contexto, sem watcher, fila ou callback de produtor.

`ProjectSupervisors::reconcile` recebe projetos desejados com um booleano `active` já decidido pelo resolvedor de configuração/atividade do daemon. Remove projetos ausentes, estaciona inativos, retoma ativos e substitui versões. Os owners existentes selecionados para parar recebem cancellation no registro antes dos joins. O registro conserva cada owner e seu handle durante o await; uma interrupção permite continuar o mesmo join. Só o término bem-sucedido remove ou estaciona o predecessor e permite sua substituição. Uma falha conserva sua entrada e causa, sem iniciar substituto nessa chamada; a reconciliação das outras seleções continua. A factory recebe o contexto explícito e devolve um supervisor pronto. O registro não calcula política de inatividade nem cria runtime de modelos ou process hosts.

`shutdown` e `stop_checkout` também conservam os owners registrados durante os joins, cancelando todo o grupo selecionado antes de aguardar qualquer cleanup. Sucesso remove somente os encerrados; falha conserva a entrada correspondente e o primeiro erro observado, enquanto os outros joins continuam. Uma tentativa explícita posterior termina os joins pendentes ou remove uma entrada já concluída, sem repollar o handle consumido ou reexecutar o produtor. A seleção de checkout preserva os outros checkouts; shutdown seleciona todos. Contextos estacionados são leves e não comprovam ausência de dados ou metadados.

Essa retenção cobre owners já inseridos no registro. A future da factory/start antes de retornar um owner e a limpeza de um resultado com chave incompatível são outros boundaries; esta correção não os qualifica. O caller produtivo ainda deve conservar a coleção real e obter Auth/Original atuais para cada operação. O SDK isolado não demonstra o ingresso multi-projeto do daemon nem um importador de arquivos para Memory.

`ProjectSupervisors::stop_selection(&SupervisionKey)` cancela e aguarda somente o owner da chave exata capturada por `ProjectContext::key`. Conserva o owner e seu `JoinHandle` no registro durante o await; interromper esse await permite aguardar o mesmo trabalho posteriormente. Após término com sucesso, remove somente as entradas ativa e estacionada dessa chave. B no mesmo checkout, outros checkouts, projetos e versões continuam supervisionados. Uma chave ausente é um no-op do registro de owners; não comprova ausência de dados ou de metadados.

Falhas de produtor ou worker preservam a causa no resultado e a entrada ativa correspondente. O handle concluído é retirado para evitar aguardar novamente uma task já consumida. Uma chamada explícita posterior pode remover essa entrada concluída, sem reexecutar o produtor. Uma entrada estacionada contém apenas o contexto leve; removê-la não executa limpeza de Data. `stop_checkout` tem outro alcance: encerra todas as versões do checkout selecionado.

`SupervisionKey` identifica ownership e não concede autoridade. O caller App deve obter a seleção admitida com o `Original` e o scope reais antes de chamar o SDK, conservar a instância real de `ProjectSupervisors` e revalidar autoridade após o await. Retirada de dados, de versão Core e de checkout são efeitos independentes. Esta API não cria ingresso App, leitores globais, quiescência, rollback ou replay; a composição de retirada de Knowledge pertence à sua folha própria.

## Rescan explícito por seleção

`ProjectSupervisor::rescan_handle()` devolve um `ProjectRescanHandle` clonável, vinculado à mesma `SupervisionKey`. `ProjectSupervisors::rescan_handle(&key)` encontra somente a seleção ativa exata; uma seleção ausente, estacionada, cancelada ou concluída não fornece um handle pelo registro.

`request_rescan()` envia um hint coalescido ao mesmo callback serial, com `ChangeBatch.rescan = true` e sem paths. Notificações recebidas antes ou durante um job deixam somente um próximo rescan pendente. O sinal usa `tokio::sync::Notify`, sem outra fila, polling ou armazenamento de textos. Nem o hint administrativo nem eventos de filesystem constituem atividade humana.

Durante um job, o hint administrativo cancela apenas o trabalho de background e aguarda o produtor Source terminar. Ele não cancela `source_preemption`: pedidos repetidos de Stop/SessionEnd não devem fazer um full ceder antes de publicar FILEs. A origem do sinal é preservada no supervisor; um rescan nativo, inclusive overflow sem paths, continua preemptivo, assim como deltas concretos, controles, erros e cancelamento do owner. A fila e os limites existentes permanecem os mesmos.

O retorno `true` indica que o sinal foi aceito por um owner vivo. Não comprova execução, publicação ou autorização: cancellation pode descartar um hint já aceito. Cada job produtivo precisa obter seu próprio `Original` e admissões atuais. O handle conserva a identidade e cancellation, mas usa `Weak` para o sinal da task e não retém o watcher, callback, corpus ou modelo. Cancelamento, parking, término ou substituição invalidam o handle antigo; ele não acorda o sucessor.

O ingresso Role/Source após CRUD Memory está qualificado em Linux em `tsk-d751`, conforme o contrato abaixo. A anexação da projeção real no Main já está implementada por `tsk-6d5a`, após `registry.reconcile` e antes de clone/serving, somente quando há recursos Memory. A prova CRUD→modelo/persistência e covered-no-RPC de `tsk-3b96` passou em duas funções Linux; seu Clippy scoped passou, qualificando a integração desta folha em Linux; receipts Dream permanecem na integração própria de `tsk-9d92`. O caller conserva a seleção antes dos efeitos e não sinaliza a publicação dos próprios embeddings. Observar indiscriminadamente o corpus Memory não distingue esses efeitos e pode repetir o job por sua própria escrita.

## Ingresso Role/Source de wake Memory

Em Linux, `SourceServer::attach_memory_rescans(&self, Arc<[ProjectRescanHandle]>) -> io::Result<()>` é `pub(crate)` e publica os handles atuais do owner no Role já anexado, inclusive após clone/serving. `RoleToolServer` atualiza essa projeção por RwLock; o write admitido clona o Arc em escopo curto antes do await e escolhe a identidade/root/version exatos. Nenhum lock atravessa o store await, nem surge registry ou autoridade nova. Source sem Role retorna erro; projeção vazia é válida. O [Foreground Source](../../crates/graphit-app/src/foreground_source.rs) coleta os handles reais após reconciliação e publica essa projeção; sem recursos Memory, publica a vazia. O delta be6e está sob qualificação; os recibos anteriores mantêm seu escopo histórico.

Após validar o grant Role, cada chamada Memory seleciona um handle emprestado pela igualdade de ID, root do checkout e versão capturada. Após o await da escrita admitida, `request_published_rescan` envia o hint para `Ok(Memory)` ou `MemoryStoreError::Confirmed`, antes do mapeamento JSON e dos checks externos de entrega tardia. Uma criação idempotente com `Ok` pode sinalizar. Leituras, `NoChange`, negação, payload malformado e erros anteriores ao efeito não sinalizam. A classificação privada de `Confirmed` não é uma injeção de falha de commit SDK.

O booleano de `request_rescan` não altera o resultado CRUD nem sua causa ou versão. Handle ausente, stale ou projeção vazia deixam esse resultado intacto. Aceitação do hint não prova computação, autorização ou sucesso do job: o callback Memory obtém seu próprio `Original` finito e admissões atuais. O grant da chamada Role não é transferido ao modelo. A publicação dos embeddings pelo próprio callback não passa por este ingresso.

A implementação está em [SourceServer](../../crates/graphit-app/src/source_server.rs), [RoleToolServer](../../crates/graphit-app/src/role_tool_server.rs) e [execução Memory Role](../../crates/graphit-app/src/role_tool_server/memory.rs); as fixtures estão em [testes Role](../../crates/graphit-app/src/role_tool_server/tests.rs). A qualificação Linux passou em seis funções distintas: três novas de ingress e três existentes afetadas, incluindo entrega tardia, catálogo e referências de scope. Os compiles App e o Clippy scoped `graphit-app --lib --no-deps -- -D warnings` passaram no Source03 atual. A fixture de classificação tipada de `Confirmed` não simula erro de commit SDK. A falha anterior do catálogo usou o oracle antigo de 16 colunas Knowledge, corrigido somente na fixture para as 18 atuais, nomes de estado/projeção de referências e nullability; a produção de ingress permaneceu inalterada. A primeira tentativa strict foi bloqueada pela transição externa de `IgnorePolicy`, preservada no histórico. As verificações finais de estilo constam da evidência da folha. Esses resultados não qualificam a composição Main, os dois projetos/modelo/covered-no-RPC de `tsk-3b96`, Dream ou outras plataformas.


<a id="integração-role-httpmemory-em-validação"></a>

### Integração Role HTTP→Memory qualificada em Linux

A fixture de `tsk-3b96` está implementada em [role_rescan_tests.rs](../../crates/graphit-app/src/foreground_memory_jobs/role_rescan_tests.rs), com HTTP Role real servido pelo Source/Foreground e os mesmos recursos `Arc` dos callbacks produtivos. Uma seam `Weak` de [ForegroundSource](../../crates/graphit-app/src/foreground_source.rs), restrita a `cfg(all(test, target_os = "linux"))`, permite observar esses recursos sem criar outro owner produtivo ou API. O teste pausa no mutex compartilhado antes da criação do `Original` e da autenticação; não pausa depois de scan ou RPC. Assim, mutações feitas enquanto o callback aguarda precisam ser lidas pelo job com seu contexto e admissões novos, conforme [MemoryJobResources](../../crates/graphit-app/src/foreground_memory_jobs.rs).

O percurso usa `model-server` real com a fixture ONNX CPU `embedding_rank2`, dimensões 4 e ORT 1.27.1. A narrativa cobre dois projetos, mutações HTTP, persistência e estacionamento independente; não qualifica GGUF ou EmbeddingGemma2. Para o corpus já coberto, a fixture chama o index real com um endpoint TCP sem responder, exige `Complete { last_attachment: None, .. }` e verifica que nenhuma conexão foi aceita. Não usa `Model.query` como oracle antes de completar seu produtor corretivo.

A execução Linux do Source03 passou nas duas funções próprias `role_http_mutations_reach_real_main_model_on_two_projects_and_park_independently` e `role_http_no_model_keeps_actual_crud_without_a_memory_owner`; o primeiro caso também verifica `get` Role retornando `absent` após delete. Compile03 e native-tests02 terminaram com exit0; o log registra 2 PASS, com model-server real, ORT 1.27.1 e a fixture de quatro dimensões. Fmt01, diff01 e strict01 passaram. O comando `cargo clippy --locked --offline -j1 -p graphit-app --lib --no-deps -- -D warnings` terminou com exit0, qualificando esta integração em Linux dentro dos limites descritos. A primeira compilação falhou em três erros próprios de fixture, corrigidos no Source02. Native01 preserva a falha do caso positivo antes da inferência por ausência de `GRAPHIT_TEST_ORT_LIBRARY` e o PASS do caso sem modelo. Os seis casos de `tsk-d751` e os oito de `tsk-6d5a` conservam seus escopos próprios, sem somar contagens nem inferir qualificação GGUF, EmbeddingGemma2, `Model.query`, Dream ou outros sistemas operacionais.

## Observação nativa e ignore

O SDK [notify-debouncer-full 0.8.0-rc.2](https://docs.rs/notify-debouncer-full/0.8.0-rc.2/notify_debouncer_full/) realiza debounce, eventos contínuos e associação de rename. O lock fixa `notify 9.0.0-rc.7`. Essas versões são release candidates adotadas para consumir a correção upstream que preserva eventos FSEvents pendentes ao reiniciar o stream após mudanças de registros. O watcher usa o backend recomendado pelo SDK com `NoCache` oficial explícito. A API entrega paths/rescan e dispensa inventário de file IDs para identificar objetos de rename. A qualificação nativa de Windows/macOS permanece na integração de plataforma. [Release upstream](https://github.com/notify-rs/notify/releases/tag/notify-9.0.0-rc.7).

O adaptador `CheckoutWatcher` compartilha um `RecommendedWatcher` nativo por configuração dentro do processo. Cada checkout conserva seu próprio debouncer SDK, assinaturas de paths, scopes, controles e mailbox. Observadores external-only participam do mesmo backend quando usam a mesma configuração. O cache global guarda apenas `Weak`; o último owner libera o backend. Isto reduz instâncias nativas sem alterar limites do sistema ou substituir eventos por polling.

Cada assinatura recebe os eventos dos seus diretórios cadastrados; rename conserva os paths originais do evento. Overflow/rescan e erros nativos sem paths são entregues a todas as assinaturas. Categorias de erro, paths e códigos IO do sistema permanecem observáveis. O agregado de assinaturas conserva o watch enquanto outro owner usa o mesmo path; `unwatch`, rebuild ou Drop de um checkout não retiram a observação do sibling. O callback nativo não adquire o mutex usado para comandos ao SDK e libera o registro de assinaturas antes da entrega, evitando esperar o próprio event loop. As configurações de criação distintas conservam backends distintos; os backends nativos da versão fixada do SDK devolvem `false` para `configure` em runtime. A reconfiguração de controles PROJECT continua individual no estado do checkout.

No FSEvents, o SDK conserva o stream quando o registro permanece equivalente e preserva os eventos pendentes quando precisa reiniciá-lo. Mudanças de registro passam pelo produtor real. O teste de remoção de diretório também observa seu pai sobrevivente para receber o evento nativo de remoção.

Eventos de leitura, abertura e fechamento de leitura são descartados no primeiro
callback nativo, antes de copiar assinaturas, comparar paths ou entrar nos
debouncers. Isso evita multiplicar o trabalho provocado pelas próprias leituras
e reaberturas de diretórios. O callback preserva eventos com `need_rescan`,
erros e eventos de mutação. O SDK fixado emite rescan/overflow como `Other`;
a verificação sintética de `Access` com essa flag cobre apenas o filtro de entrada,
sem alterar o tratamento posterior do lote.
`Access(Close(Write))` é preservado como evento real de escrita no fanout e no
consumidor após o debounce. No backend Linux, esse fechamento permite entregar
alterações durante uma sequência contínua de `Modify(Data)`, sem esperar que o
writer pare. Não há timer, fila adicional ou polling. Esta garantia depende dos
eventos que o backend realmente emite; não qualifica Windows/macOS ou backends
sem fechamento de escrita. O filtro não inclui metadados de atime.
A API `notify::Config` fixada não permite configurar uma máscara
nativa de eventos; não há fork ou backend alternativo para fazer esse filtro.

Um watcher observa a união AST/wiki por checkout. Cada diretório é cadastrado em modo não recursivo quando pelo menos um `IgnoreScope::should_descend` permite. Regras de ignore são as do Core: paths relativos, scopes imutáveis, regras de subdiretório, defaults e filename próprio por consumidor. Quando AST/wiki têm exatamente os mesmos defaults e filename, seus scopes usam clones do snapshot imutável Core, compartilhando policy/layers no mesmo checkout. Perfis diferentes conservam scopes próprios. Pastas excluídas pelos dois consumidores não ganham watches. Não existe parser de ignore paralelo.

Novos diretórios ganham seus watches e scopes. Remoção/rename retiram os registros de toda a subárvore. Mudanças de arquivos de ignore recarregam scopes, ajustam os watches e pedem rescan; essa leitura recupera mudanças ocorridas enquanto a topologia foi atualizada.

A remoção nativa pode preceder `unwatch`. Um adapter privado delega ao backend recomendado e interpreta exclusivamente `WatchNotFound` como remoção já concluída. Isso permite ao debouncer executar sua limpeza de roots/cache. Os demais erros permanecem erros observáveis; o adapter não simula watches nem altera o backend.

Uma subárvore com `PermissionDenied` é ignorada e o primeiro erro é reportado em `ChangeBatch.error` com rescan, mantendo os watches das pastas acessíveis. Seu diretório não recebe watch/scope quando a leitura ou admissão é negada. A raiz ilegível continua impedindo a inicialização. A categoria IO original do SDK é conservada com sua causa; limite de watches/ENOSPC e outros erros continuam erros. Restauração de permissão e recarga dos ignores permitem observar novamente a subárvore.

Durante a descoberta de diretórios, `NotFound` e o `PathNotFound` nativo só representam remoção concorrente quando o child está de fato ausente e a raiz continua sendo um diretório legível sem symlink. O watcher não instala o scope/watch desse child e entrega o sinal `rescan` existente, conservando o watch do parent. Uma raiz ausente ou ilegível continua falhando; um child ainda presente não oculta erro de leitura de seus controles. Outros erros de SDK, limites de watches e escopo permanecem observáveis. Essa observação de topologia não altera os fences estritos da captura Source: mutação depois do inventário continua invalidando a captura.

O matcher Core resolve cada prefixo com a semântica de travessia Git, também nas regras custom. Uma negação de arquivo não reabre um diretório pai excluído; uma negação não relacionada, como `!.env.example`, não cadastra watches em `target/` ou `.build/` excluídos. Para reincluir um arquivo, seus pais precisam estar permitidos, por exemplo com `generated/*` e `!generated/driver.rs`. O watcher conserva a precedência defaults/custom/Git e a união AST/wiki; não cria exclusões universais para nomes de diretórios de build nem contadores globais de negações.

## Lotes e encerramento

`ChangeBatch` contém paths absolutos do checkout (incluindo paths apagados), um sinal de rescan e o primeiro erro observado. Produtores consultam o estado atual de seus arquivos. Alterações de topologia/ignore, erro ou excesso de caminhos exigem rescan.

A fila retém no máximo 16 lotes; cada lote incremental retém até 256 paths. Quando o produtor fica atrás dos eventos, o watcher substitui os detalhes por um sinal compacto de rescan e acorda o consumidor. O callback nativo não espera a fila liberar espaço. Não há corpus/snippets pendentes nesta camada.

`ProjectWatcher::start/stop` fazem trabalho bloqueante do SDK/filesystem. O supervisor os executa por `spawn_blocking`. `stop` tira o handle do mutex de callback antes de aguardar `Debouncer::stop`; isso evita join segurando o mutex que o callback precisa. O callback usa `Weak` para o estado do watcher, sem ciclo de owners.

O Drop do watcher encerra e aguarda seu SDK. O Drop do supervisor sinaliza cancellation; sua task conserva os recursos até terminar a limpeza. Consumidores que precisam confirmação de término/erro devem chamar `stop`, `park` ou `shutdown` e aguardar.

Para encerrar uma versão capturada preservando as demais, o registro oferece `stop_selection` com o contrato de ownership retido descrito acima.

## Validação e limites da entrega

Testes focados usam diretórios temporários e eventos reais do backend: criação/removal/rename de diretório, união AST/wiki, ignore editado, deleção, escrita contínua, fila cheia/rescan, stop e liberação dos scopes. Testes de supervisor exercitam contexto por checkout/versão, cancellation sem abandono da future, estacionamento sem interromper outro checkout, retomada com rescan, atividade após deleção, retirada de projeto obsoleto e troca de versão.

O compartilhamento de backend foi qualificado em Linux com 29 testes de watcher e 24 de supervisor, todos aprovados. Os quatro casos adicionados usam eventos reais para 40 owners no mesmo backend, retirada de um owner de path compartilhado e liberação/recriação após o último Drop; reconfiguração de ignores e stop de um checkout com scopes e eventos do sibling preservados; e retirada de um watch recursivo com descendant ainda observado. O caso de duplicação de erros verifica categorias, paths e código IO sem simular uma falha nativa de recursos. A compilação levou 9min43s; execução de watcher 1,05s e supervisor 0,43s no mesmo executável. Esta evidência cobre o SDK nativo Linux, não a ativação do Source instalado nem Windows/macOS ou jornadas dos hosts de agentes.

A entrega inicial executou 23 testes Main com sucesso. A correção de subárvores ilegíveis tem 16 testes de watcher (incluindo três fixtures Unix com chmod real e um negativo de política/SDK) e 11 de supervisor consumidor; fmt e Clippy dos targets afetados acompanham cada entrega. Elas não demonstram o produto daemon completo nem todos os sistemas operacionais. A integração com os produtores reais permanece nas folhas H/J/C/M e a cobertura integrada na folha I.

A seleção exata foi qualificada com os 17 testes atuais de supervisor em Linux, incluindo quatro casos novos: await interrompido com recurso e owner retidos, continuidade de eventos em B após encerrar A, chave estacionada/ausente e falhas reais de IO ou panic da task com causa preservada e cleanup posterior sem repoll. Os testes usam o backend nativo de filesystem e o SDK de supervisores; não demonstram o caller App autorizado, retirada de dados/versão Core ou os hosts de agentes instalados. Os contratos atuais de agentes são Kiro V3, OpenCode 2 e AGY; testes de compatibilidade não exigem binário do fornecedor, conta ou plano ativo.

A correção dos joins em grupo executou 20 testes de supervisor em Linux. As novas jornadas interrompem checkout, shutdown e reconciliação por remoção ou parking com dois produtores mantendo arquivos abertos durante cleanup; o registro retém os mesmos handles, aguarda a limpeza posterior e os eventos nativos continuam no checkout sibling quando ele não pertence à seleção. Reativar uma seleção já cancelada aguarda o predecessor antes de iniciar um novo owner. Um predecessor concluído com erro permanece registrado e não é estacionado ou substituído nessa chamada, inclusive quando a versão desejada muda no mesmo checkout. O teste de erro em checkout também conserva a entrada até cleanup explícito. Esta prova cobre o SDK de owners registrados, não a factory antes de inserção, o ingresso produtivo multi-projeto, AST/Memory/modelos ou outras plataformas.

A suíte `graphit-app --test project_retirement` executou 13 testes em Linux. Seu caso de falha de cleanup conserva a causa e o inventário, mantém o owner falho junto ao checkout sibling e encerra ambos em cleanup explícito posterior, sem reiniciar o produtor. A produção App já propaga essa falha antes de alterar metadados; seu teste foi ajustado para observar o owner retido e confirmar o cleanup posterior. Essa regressão qualifica o consumidor de retirada testado com os SDKs reais, sem comprovar credenciais de produção ou hosts de agentes instalados.

O sinal explícito `tsk-0c26` foi qualificado com os 24 testes de supervisor em Linux, incluindo quatro casos novos: coalescing de notificações durante dois jobs pausados sem execução paralela; lookup exato, parking e retomada com handles antigos inativos e checkout sibling preservado; falha real do produtor invalidando o sinal antes de `stop`, sem perder sua causa; e hints contínuos preservando a entrega de eventos nativos. Cancelamento tem prioridade, enquanto rescan e filesystem disputam sem preferência fixa. A prova cobre o produtor Supervisor; notificação por MCP/Dream, execução Memory pelo Foreground e outros sistemas operacionais continuam fora dessa qualificação.

A remoção do callback de atividade filesystem foi integrada ao Foreground e aos consumidores existentes em `tsk-9a23`. Os quatro casos afetados de supervisor passaram em 0,07 s: coalescing sem execução paralela, entrega de filesystem sob hints contínuos, parking com sibling e remoção de arquivo com a versão capturada. Os consumidores Role Memory rescan, Knowledge retirement e Project retirement passaram um caso cada (9,76 s, 6,19 s e 0,11 s). São evidências funcionais dos mesmos produtores, sem um sinal substituto de atividade humana; os logs ficam em `fs-activity-removal-daemon01.log` e `fs-activity-removal-consumers01.log`. Compilação e espera são separadas desses tempos.
O Clippy scoped das libs Daemon/App e dos targets Project/Knowledge retirement passou com warnings como erro em `fs-activity-removal-strict01.log`; o ajuste de `unused_mut` da fixture não alterou sua lógica nem exigiu repetir os casos funcionais.


## Composição Foreground de Memory

O Foreground conserva um único `ProjectSupervisors` para suas seleções físicas de projeto e versão. Source, Task, Now e os callbacks usam a mesma coleção `Arc` capturada no bootstrap; cada callback conserva o índice de sua seleção. O owner de modelos não cria outro registry ou watcher.

A configuração de ignore é capturada individualmente por checkout. O caller substitui somente a camada Project pelo documento lido por `ConfigStore::project`, mantendo as camadas Command, Environment, Global e Distribution já capturadas. Resolve `AstIgnorePolicy` e `KnowledgeIgnorePolicy`, copia seus dados para `WatchConfig` e libera o snapshot temporário. Erros de configuração e cancellation continuam erros. O matcher nativo Core processa `.astignore`, `.wikiignore` e as regras Git; essa captura não relê o ambiente ou a configuração global.

O callback Memory executa a leitura inicial e os rescans explícitos pelo processo compartilhado do modelo. Cada execução cria um contexto finito, autentica novamente a chave runtime e obtém admissões Memory PROJECT de leitura e escrita, além da lease local de embedding. Nenhuma admissão ou texto do corpus permanece no registry. Um mutex Tokio compartilhado serializa somente o pipeline Memory do worker nativo único; sua aquisição é cancelável e precede o contexto finito, a autenticação e a abertura do corpus. Projetos aguardando execução conservam sua seleção, sem batches de textos ou grants. Eventos incrementais que não pedem rescan não provocam reindexação Memory.

Parking ou retirada cancelam e aguardam somente os owners selecionados. O shutdown do Foreground aguarda o registry antes de drenar e encerrar o processo do modelo e antes de retirar a chave runtime. A publicação dos próprios embeddings não acorda novamente Memory. O ingresso Role/Source após CRUD está qualificado em Linux, conforme [wake Memory](#ingresso-rolesource-de-wake-memory). O Main projeta os handles dos owners Memory após a reconciliação e os anexa ao Source antes de compartilhar o handler; seleções somente Dream ficam fora. A prova completa CRUD→modelo→persistência e os efeitos Dream permanecem nas integrações respectivas.

A qualificação desta composição está na folha `tsk-6d5a`, com fixtures de dois projetos, configurações distintas de ignore, processo real de modelo, indexação persistida e retirada independente. A execução Linux passou nos oito casos focados: sete na suíte nativa e o caso de versão inválida corrigido no focal separado. Os vetores são calculados pelo modelo ONNX CPU real da fixture; isso prova a composição, sem substituir a qualificação do modelo padrão EmbeddingGemma2. Atividade humana de Dream vem dos ingressos humanos existentes; o supervisor não emite esse sinal a partir de filesystem ou rescan. A jornada completa Dream sem modelo pertence ao seu consumer próprio.

## Configuração de parsers Docker

As associações `ast.scip.parsers` pertencem à configuração normal. Cada supervisor
PROJECT observa os controles globais e do checkout pelo mesmo watcher, mesmo sem
o módulo Task. Uma alteração de configuração informa `controls_changed` e passa
pela resolução do pipeline AST. Não existem roots de bibliotecas externas,
`native_grammars_changed`, caches de aquisição de `.so`/`.dll` ou pedidos de restart
para trocar gramática. O fallback usa somente os parsers builtin.
