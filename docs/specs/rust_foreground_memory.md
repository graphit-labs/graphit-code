---

title: Memory PROJECT no browser Foreground Rust
doc_type: spec
description: Leitura, escrita e busca Memory PROJECT e própria USER por sessão browser e admissões nativas.
references:
  - target: {type: task, id: tsk-6286}
    relation: implements
  - target: {type: task, id: tsk-377c}
    relation: uses_api
  - target: {type: task, id: tsk-5995}
    relation: uses_api
  - target: {type: task, id: tsk-c623}
    relation: uses_api
  - target: {type: task, id: tsk-4e50}
    relation: implements
  - target: {type: task, id: tsk-5a58}
    relation: implements
  - target: {type: task, id: tsk-a684}
    relation: uses_api
  - target: {type: task, id: tsk-8e5d}
    relation: uses_api
  - target: {type: task, id: tsk-6062}
    relation: uses_api
  - target: {type: task, id: tsk-db07}
    relation: implements
  - target: {type: task, id: tsk-ef75}
    relation: relates_to
  - target: {type: task, id: tsk-9d46}
    relation: uses_api
  - target: {type: task, id: tsk-51f2}
    relation: implements
  - target: {type: task, id: tsk-67e7}
    relation: uses_api
  - target: {type: task, id: tsk-c135}
    relation: uses_api
---

# Memory PROJECT no browser Foreground Rust

Current path contract: PROJECT roots and relative paths cross JSON/API/CLI as literal Unicode strings. Serde preserves spaces, accents, escapes and separators without lossy conversion. Non-Unicode input fails explicitly; `{os,units}` is rejected. Historical evidence below describes the code that was exercised then and does not qualify the removed path codec.

Integradores podem consultar Memory PROJECT no listener Foreground que habilita o componente [browser autenticado](rust_foreground_tasks.md). O componente reutiliza seus providers, cookies, origem, projetos e versões capturados. Esta API entrega dados nativos; o consumidor React e a navegação entre referências têm suas próprias entregas. Para manutenção explícita dos índices pela CLI e pelo MCP, consulte [Memory index](rust_memory_index.md).

## Requisições e resultados

As três operações usam `POST` com `Content-Type: application/json` ou um tipo `application/*+json`, cookie da sessão autenticada e `Origin` igual à origem do serviço. O corpo contém `selection` e `input`, ambos obrigatórios. A seleção é a tupla completa `project_dir: NativePath`, `project_id: ProjectId` e `project_version` literal não vazia. Use a seleção retornada pelos contextos do browser; o caminho já é uma string Unicode literal; não normalize o texto nem infira a versão por Git.

| Rota | `input` | `native` no resultado |
| --- | --- | --- |
| `/api/memory/list` | Objeto de filtros, ou `{}` | `{state:"no_corpus"}` ou `{state:"published", generation, memories}` |
| `/api/memory/get` | `{id: MemoryId}` | `null` para ausente ou excluída; caso contrário `{memory, creation, generation}` |
| `/api/memory/history` | `{id: MemoryId}` | `null` para ausente; caso contrário `{creation, current, deleted, revisions, generation}`, incluindo o estado excluído |

Cada resposta bem-sucedida contém `{selection, native}` e `Cache-Control: no-store`. A seleção vem do projeto admitido. Um corpus publicado sem registros elegíveis continua `state:"published"` com `memories:[]`; isso difere da ausência de corpus. Não há truncagem de histórico ou prefixos bem-sucedidos diante de erro.

Os filtros opcionais da listagem são `classification`, `tag`, `mandatory` e `important`; `exclude_mandatory` é booleano e seu default é `false`. Classificação aceita `fact`, `skill`, `tension`, `decision`, `convention` e `correction`. Os predicados combinam com AND, a tag é literal e não pode ser branca. O SDK ordena mandatory, important, demais; dentro dessas categorias usa atualização decrescente e ID estável. A operação lê os heads atuais até EOF antes de filtrar e devolver o resultado.

Memory mantém seus campos canônicos, inclusive revisão, atributos, referências, geração de referências, autores e timestamps inteiros. Creation inclui projeto, versão da publicação, autor, entrada de criação e Memory inicial. Cada revisão inclui o antes, depois, autor, versão e operação completa; as operações de consolidação também incluem os membros e as observações persistidas. `generation` preserva `sdk_version`, projeto, versão selecionada, branch física e destino herdado. Inteiros `u64` devem ser tratados sem perda de precisão pelo consumidor; não são datas formatadas ou aliases da API antiga.

Campos desconhecidos ou repetidos, tipos incorretos, ID não canônico, versão vazia/NUL e seleção incompleta são rejeitados. O estado USER requer sua própria identidade e autorização e não é acessível por estas rotas.

## Autorização, limites e erros

Cada requisição valida o access JWT com discovery/JWKS em cache e obtém uma autorização PROJECT atual para **Memory/read**. Uma autorização Task/read e um JWT válido não substituem esse acesso. A seleção e os IDs são dados, não permissões. UserInfo pode obter perfil no login quando necessário; sua indisponibilidade não invalida um JWT válido. A janela para observar revogação ou mudanças de claims continua sendo a expiração do access token, conforme [autenticação browser](rust_browser_auth.md).

Leitura, admissão e entrega usam o mesmo Original, cancelamento e prazo absoluto. O SDK captura um pin para cada operação, valida o histórico completo no get/history e revalida sua captura/admissão durante a leitura. O serviço mantém a operação aceita sob seu JoinSet até terminar; encerramento impede novas leituras e aguarda as operações. Não há cache de corpus entre requisições ou renovação implícita da autoridade.

Requests Memory e Knowledge usam uma única decodificação Serde nos tipos fechados, sem árvore JSON intermediária ou contadores de nós/profundidade. `max_input_bytes` limita o corpo antes do decode. Os campos de texto decodificados (versão, tag e slug) respeitam `max_scalar_bytes`; IDs usam a validação canônica nativa de ULID, e o caminho Unicode literal respeita `max_scalar_bytes`, além dos controles de formato, NUL e tamanho. `max_collection_entries` continua reservado às coleções reais. Campos desconhecidos, duplicados e tipos inválidos continuam rejeitados.

O leitor nativo usa `batch_rows` e os limites físicos de cada arquivo já capturados. A consulta lexical tem um limite individual de bytes. Não há quota de linhas ou texto acumulado do corpus; schema, EOF, histórico e captura/admissão continuam validados. A saída é serializada uma única vez, diretamente dos tipos nativos, num VecWriter limitado por `max_output_bytes`. Esse buffer é movido para o corpo HTTP depois dos rechecks; não há conversão intermediária do corpus para JSON Value, cópia dos bodies ou nova serialização na entrega. Não são quotas ou reservas de recursos entre operações.

| Condição | Resultado |
| --- | --- |
| Corpo inválido | `400 invalid_arguments` ou `invalid_selection` |
| Corpo maior que o limite HTTP | `413 invalid_arguments` |
| Seleção válida, mas não capturada | `404 selection_unavailable` |
| Método GET | `405` |
| Sem componente browser habilitado | `404` |
| Sessão/JWT inválido | `401` |
| Origem ou PROJECT negado | `403` |
| Resposta PROJECT com módulo/vínculo diferente | `502 invalid_project_response` |
| Original cancelado/expirado durante leitura | `408 request_cancelled_or_expired`, ou falha do boundary browser sem payload nativo |
| Componente parado | Falha do boundary, sem payload nativo |
| Índice lexical ausente ou incompatível na busca PROJECT/USER | `503 lexical_index_unavailable`, com a mensagem causal nativa em `error.message`; a leitura não cria índice nem retorna hits vazios |
| Limite nativo/saída ou corpus inválido | `500`, com código causal como `memory_read_limit`, `response_limit`, `incomplete_corpus` ou falha de storage |

## Consumidor React nativo

`MemoryExplorerPage` usa `api/nativeMemory.ts` e `api/nativeRecordRead.ts` para
os POSTs singulares `/api/memory/list`, `/get` e `/history`. PROJECT envia a
seleção completa capturada e compara a seleção do envelope retornado. USER usa
`/api/user-memory/list|get|history` com apenas `input`; observa `scope_id` e
`currentOwner`, e rejeita um namespace diferente do solicitado na URL. Esses
pedidos também rejeitam `scope_id` duplicado na URL antes de qualquer IO.
Os dados retornados
não concedem autoridade. A seleção PROJECT conserva NativePath, ID e
versão literais; a navegação USER não fabrica uma seleção PROJECT.

A página mostra `Memory.attributes`, body, tags, metadata, geração e histórico
SDK completo: creation e cada revision com operation, before e after. Conserva
a ordem entregue pelo SDK, sem ordenação frontend por Date nem reconstrução de
campos Go como hash, previous, next ou revision_id. História excluída difere de
registro ausente; NoCorpus, corpus publicado vazio e falha causal têm estados
próprios. Retry mantém o escopo; mudança de seleção limpa o payload anterior,
e AbortSignal e a chave do escopo impedem publicação de respostas antigas.
O catálogo vincula payload e erro à chave completa do filtro — classificação,
tag, important e mandatory —, retirando o resultado anterior enquanto o novo
POST está pendente. Isso não cria filtro de dataset local ou API de busca.

Metadata u64 é exibida literalmente. O codec comum `lossless-json 4.3.1`
conserva inteiros seguros como number, os demais como bigint e valores f32/f64
como number; detalhes JSON usam o mesmo encoder sem converter inteiros para
strings. As referências exibidas são observações de `Memory.references`, sem
href ou consulta de título. Não substituem edges autoritativas: o compositor
de referências e Incoming têm provas próprias: ef75 COMPLETED 65 e 51f2
COMPLETED 79 estão qualificados em seus escopos.

Uma seção adicional Record links usa o compositor nativo de `tsk-51f2` com
binding físico/lógico observado e fontes adicionais opt-in, sem alterar as
referências raw ou o histórico SDK. Reader é locator, não grant; a navegação
readmite o destino. O [contrato central de referências](rust_foreground_record_references.md)
delimita páginas, estados e cursores. db07 não declara FullNoLoss, compositor
ou fechamento de tsk-17a7; isso é limite de prova, não pendência desses owners.

No chrome USER, `RouteWorkspace` não chama `loadProjects`. O header conserva
`AuthIdentity` e Refresh, sem seletores PROJECT. `WorkspaceRefresh` usa o escopo
e guard USER, independentes de diretório PROJECT e agente; a chave de Routes
USER independe de ProjectVersion, preservando o draft ao mudar essa versão.
O chrome também conserva Header/Auth quando o gate de conteúdo recebe
401, 403 ou 503; oito testes App passaram (harness 2,39 s). Isso não concede
permissão nem transforma negação em conteúdo saudável.
Uma prova focal passou 18 casos (harness 2,21 s): dois novos App USER com readers
mockados, oito TaskWorkspace — um Mode novo e sete retidos — e oito Refresh
existentes. São provas de componentes Linux/Node 22/jsdom, sem prova HTTP,
browser ou grant Auth. A matriz runtime própria db07 continua aberta, sem
dependência de replay ou conclusão de ef75 já qualificada.

O hosting finito inclui seis rotas: `/memory/explorer`, as raízes PROJECT/USER e seus caminhos com ID, além de `/knowledge/explorer`. A tabela de teste foi atualizada.

O store agora captura contextos Memory por `POST /api/memory/contexts` com
`{input:{}}`, sob o módulo `memory`, sem inventário Task ou projeto global.
`current:null` mantém a seleção vazia; não há escolha automática do primeiro
contexto. Trocar módulo limpa/aborta leituras e bloqueia respostas ou negações
tardias; Retry conserva a seleção da URI. App, header e Refresh usam esse
escopo próprio; USER permanece independente. O isolamento de componentes
não prova zero IO PROJECT no browser inteiro.

As [rodadas focais atuais](rust_foreground_tasks.md#cliente-react-e-inteiros-nativos)
têm união de 49 casos, com sobreposição entre suites, sem somar o baseline
histórico. A integração HTTP/CUA própria db07 mantém os limites descritos
na matriz focal R15, sem declarar pendência de ef75.
Auth `tsk-438e` e HTTP CRUD `tsk-9d46` são boundaries de escrita distintos: os
resultados backend USER CRUD não qualificam por si a escrita React descrita
abaixo. A correção de setup de busca também não qualifica sozinha o consumidor
UI. A busca Knowledge/Ask opcional com M real ainda requer os produtores
Browser de seus owners; `tsk-247e` SourceServer não qualifica esse caller.
Referências 51f2/ef75 e Gantt já estão qualificados por seus owners; db07 não
repete nem assume fechamento desses escopos.

### Escrita e busca React

O cliente `nativeMemoryMutation` usa os DTOs fechados de create/update/delete
de `tsk-9d46`: PROJECT envia `{selection,input}`, USER apenas `{input}`.
Update/delete conservam CAS `expected_revision` como NativeInteger. O UUID de
idempotência permanece estável em retry explícito do mesmo intento, escopo,
operação e payload, incluindo reason. No mesmo escopo, completed/confirmed
encerra essa chave antes do guard de foco; uma nova Create igual recebe outro
UUID. Mudança de escopo é ignorada pelo guard anterior, e um novo intento usa
o fingerprint desse escopo. O formulário
conserva o body literal e edita tags como array, com textareas individuais
Tag N e ações Add/Remove. Não há CSV: vírgulas e newlines permanecem literais.
Update omite references, preservando as referências SDK existentes.

Receipts reais distinguem completed, confirmed, uncertain e not_applied.
JSON e headers de outcome, inclusive em HTTP500, são reconhecidos sem
inventar receipt, geração ou recovery ausentes. index_publication é opcional:
sua versão SDK inteira e recovery_key real são observações separadas do
CRUD. No fallback de headers há somente index_sdk_version quando conhecido;
não se fabrica generation/recovery do índice. Recovery.physical_branch pode
ser null. A UI mostra Index SDK version separada e o receipt completo.

Somente completed|confirmed encerram intento/chave e permitem fechar/navegar.
not_applied conserva draft, confirmação, rota e chave; uncertain mantém sua
recuperação. Nenhum dos dois inicia retry automático. Memory write not applied
tem notice próprio, também dentro dos modais Create/Edit para não ficar sob
root inert; Remove já possuía notice interno. USER write-only não provoca leitura automática ou discovery
PROJECT. Quando há navegação após receipt USER, ela usa o namespace observado;
resposta tardia após mudança de foco não navega, fecha o formulário ou relê
automaticamente, mesmo quando encerra a chave do intento.
Após update confirmado do mesmo ID, o histórico é relido quando já havia
leitura. O dossier é recriado pela revisão atual, e a edição seguinte usa o
CAS fresco. USER write-only continua sem releitura automática.

#### Remoção por diálogo R11

A tentativa IAB de remover encontrou window.prompt não suportado e não
enviou POST. R11 substitui esse fluxo por MemoryRemovalForm, reutilizando
ModalPortal e work-dialog. O diálogo apresenta o título e a revisão
capturados, exige uma razão não vazia e envia a string literal, sem trim
do payload. Cancel e Escape antes de confirmar não fazem I/O.

O intento captura o registro, revisão e MemoryReadScope. Mudança de escopo,
foco, revisão ou estado deleted invalida o diálogo; erro tardio de RemoveA
não aparece como erro de RemoveB. Idempotência e os receipts completed,
confirmed e uncertain preservam o contrato anterior, sem retry automático.
acceptMutation valida o escopo capturado e encerra a chave
completed/confirmed antes do guard de foco. Quando RemoveA termina após o
foco mudar para B, o receipt não é publicado em B, mas o intento de A é
settled. O guard de erro de remoção continua validando foco e escopo.
A regressão late confirmed A→B mantém o diálogo B e o intento correto;
MemoryPage03 passou 22 casos em 7,18 s e lint03 MemoryPage/test saiu 0.
A rodada 21 anterior e os 17 casos inalterados continuam históricos, sem soma.

A correção está implementada em Source. Unit01 teve 37 PASS/1 FAIL em 38 casos
de quatro arquivos, 29,25 s: a fixture nova tinha current revision sem cadeia
correspondente. Corrigidos before/after/operação, o rerun MemoryPage passou
21 casos em 6,93 s. Os 17 casos dos outros três arquivos passaram sem alteração
na rodada inicial; reruns sobrepostos não se somam. Type noEmit01 saiu 0,
lint01 de MemoryPage/test e lint02 do teste corrigido saíram 0. Naquela rodada o Browser
real da remoção ainda não havia passado; a rodada08 posterior o qualificou
no fluxo observado, conforme a prova independente abaixo.

Compile06 passou em 16,12 s. Na fixture histórica dessa rodada, FTS SDK
PROJECT/USER era publicado uma vez após seed; isso não qualificava o
initialize-writer produtivo. Esse publisher manual foi removido em R12.
Host04 passou um teste em 351,41 s, incluindo o intervalo em que aguardava
controle. Browser04 falhou no timeout de 20 s de create; Memory search
devolveu 200. O IAB no mesmo host observou create, update e history nas
revisões 1/2; delete continuou impedido pelo prompt até a correção R11.
Esses positivos parciais não qualificam o novo diálogo.

Build03 passou tsc-b/Vite/cópia: 3.793 módulos, Vite 4,47 s.
O index da rodada histórica Build03 tem 2.612.285 bytes,
SHA-256 f1eb9de1a8de514385c512398f4fd41c282ba7efc0c885cfc430e0b564c46efc,
igual no dist e shipped. Build02/67f e Compile07/1min38s são históricos:
bin07 não embute o hash f1eb atual nem qualifica esta UI em runtime.
Host05 executou depois com f1eb; os resultados e o limite do novo R13 estão
registrados abaixo.

R12 agora está implementada na fixture e no runner. hosting.rs remove
MemorySeed::CreateWithLexicalIndex/create_lexical_index e o bool USER;
USER db07 não recebe as duas escritas seed, com Option<MemoryId>None.
incoming_user permanece configurado; o preflight USER list observa
no_corpus e o scope real. References e os outros cenários são preservados.

O runner começa pelo PROJECT extra e USER sem corpus, cria pela UI e exige
FTS200 com hit literal após create e update, além de histórico e remoção
pelo diálogo React. bodies[mark:] compara a seleção exata. Essas são
asserções implementadas, ainda não uma execução runtime aprovada.
py_compile, rustfmt e diff check passaram; a rodada Compile08 usou
offline/locked/j4 no target/profile existentes. Host05 executou depois;
isso não qualifica o novo build04/R13 nem a matriz global.

Root confirmou Source produtora e fixtures 67e7 coesas, Data3args, e liberou
compilar agora. A relação é uses_api, sem esperar COM/status 67e7 ou um
registro futuro AppMemoryIndex: o Browser contribui à qualificação do produtor,
e o antigo gate de COM criava ciclo. Nenhum Source UI mudou em R12; bundle f1eb
permanece atual. O disco deixou de ser bloqueio: 7,2 GiB livres é apenas a
observação mais recente; saturação/SIGBUS anterior fica no histórico.
Host04 PASS e Browser04 FAIL permanecem distintos; a matriz runtime R12 e os
checks Browser globais continuam abertos.

#### Outcomes e índice no consumer R13

O packet R13 foi salvo antes de Source. Unit01 teve 33 PASS/2 FAIL em 35
casos/9,13 s pelo matcher toHaveValue inexistente, corrigido. Unit02 passou 36
casos em 8,19 s: 11 API e 25 MemoryPage. Type01 encontrou a opção exact indevida
de TestingLibrary e tuple readonly no FactList; os tipos foram corrigidos sem
alterar comportamento. Lint01 e lint02 scoped de quatro arquivos saíram 0.
Build04 saiu 0, comprovando tsc-b final após as correções somente de tipos.
A falha Type01 permanece no histórico; Unit02 36 PASS continua aplicável. Os quatro testes DTO do Root passaram em 0,05 s
como prova do produtor, não do UI runtime.

Host05 passou em 273,77 s com shutdown/reopen. Seu bundle f1eb passou a observação
HTTP em 303 ms. Browser05 falhou no observer de Task latest com StopIteration
apesar de GET 200; o callback reentrante foi corrigido enfileirando a leitura.
O IAB05 real observou USER NoCorpus→primeira UI create completed→FTS com um
hit literal→remoção React cancel sem excluir→confirmação com razão→reload
deleted revisão 2 e histórico. É uma prova parcial: não qualifica toda matriz,
zero I/O PROJECT automático ou os novos outcomes R13.
Build04 passou Vite 3.793 módulos/19,43 s e cópia: index 2.612.899 bytes,
SHA-256 b4e594e1d03a0690638901cfaf7200e539f51613dde223279f38b5c83037f39a,
igual no dist e shipped. Compile09 passou em 3 min 28 s com embedding desses
assets R13 no target/profile existentes. Host06 depois falhou antes do probe
em 21,21 s: hosting.rs:1469, ForegroundFailureInvalidData/ProfileDecodeFailure
do stock/plsql.yaml, Decode span 35365..35365, schema_path None,
retained_owner true. Browser06 falhou no pré-requisito e executou zero Browser;
não houve IAB atual. Root encaminhou ao owner AST/C8 e determinou não repetir
Host até o perfil Source coeso. Não há alteração AST/Runtime nem contorno
de bootstrap pelo consumer. Unidades 36/lint/build04/compile09 continuam
válidos nos seus escopos; nenhum deles passa runtime. f1eb fica histórico.

O owner fortalecerá checks do runner para FTS por termo exclusivo após revisão
e exclusão corrente versus retained history. Ainda não há resultado de
execução desses checks; não mudou Source UI ou asset.

#### IAB07 real e matriz focal R15

Compile10 passou em 32,04 s com C8 coeso. Host07 passou em 335,27 s e serviu
bundle b4. Browser07 foi interrompido por SIGINT no coletor Task, com
Response.json pendente e no running event loop durante cleanup. Isso é
limitação do coletor, não falha CRUD; não se inventa HTTP a partir dela.

Foi observado receipt de create anterior ao abrir novo Edit. O ajuste limpa
somente setMutation ao abrir novo Create/Edit, sem mexer na key/receipt de
intento em andamento. Unit03 teve 37 PASS/1 FAIL em 38 casos/8,83 s, pois o
callback Edit ainda não limpava o notice; corrigido o handler, Unit04 passou
38 casos em 9,07 s, dois arquivos (11 API/27 page). Lint03 e lint04 focal
final saíram 0. Build05 saiu 0 com tsc-b/Vite 3.793 módulos/6,77 s e
dist/shipped iguais: 2.612.921 bytes,
SHA-256 9678a314f4b42c5c8d363cd0a56f6e7389bbf1e35768e771ab96dec3b6fa48f1.
Compile11 saiu 0 em 14,98 s, observado no log. Browser08 posteriormente
qualificou a Memory de forma independente conforme os resultados abaixo;
a matriz Knowledge ainda estava pendente nessa rodada; o teardown Host08
foi concluído. A reconciliação final está registrada abaixo.

R14/A4 foi supersedido e a matriz R15 foi salva antes de Source. O runner
ficou focal: coleta apenas JSON POST Memory/USER/Knowledge e snapshot da fila,
gravando qualification.json a cada passo passed antes do teardown, para
preservar Memory se Knowledge falhar depois. Task/Session, CRUDmain duplicado,
Ask e FTS paging já provados não são repetidos. Reutilizam-se Browser03/04 para Task/ownmodule,
IAB03 para Knowledge FTS/paging/Ask/importbrowse e IAB07 para USER CRUD.
Faltam PROJECT primeira Memory, USER zeroIO/narrow, seleção Mirror,
Knowledge hybrid/import semantic+hybrid/corpus/page states/import heads→page
zeroIO, query aceita cancelada sem late reply e narrow/denial. Esses checks
continuam abertos; a prova parcial não implica matriz global aprovada.

#### Memory qualificada no Browser08

O bundle 9678a314f4b42c5c8d363cd0a56f6e7389bbf1e35768e771ab96dec3b6fa48f1
serviu a prova real. PROJECT first create→FTS→update com termo exclusivo→FTS
→delete/history, excluindo o registro na busca corrente e incluindo no
histórico retido, passou em 7.540 ms. USER passou a mesma jornada em 7.696 ms,
com zero I/O PROJECT e Task. Mirror NativePath/reload literal passou 2.763 ms.
Esses são tempos dos estágios completos, não latência de uma requisição.

Memory está qualificada independentemente. O runner global falhou depois na
espera de HTTP 20 s de Knowledge hybrid/dot, não no seletor nem em Memory.
IAB08 observou a busca já partial 22/model_unavailable até 93,434 s após click;
instante de conclusão e latência não foram medidos. O coordenador rastreou
default produtivo 120 s; depois confirmou fixture 120000 ms e ajustou
somente wait do runner para 130000 ms.
Knowledge continua sem qualificação dos checks faltantes. A coleta Host08
terminou com exit 0, 1 PASS/0 FAIL, 8 filtered, finished 693,57 s; .done foi emitido
antes de 900 s e teardown está concluído. Esse tempo inclui espera do host, não
latência de requisição. Runner08 saiu 1 pelo timeout HTTP 20 s de Knowledge,
preservado no artefato; lint04, py_compile e focus-help saíram 0.
Não se reconstrói Memory/Task/Gantt/Refs nem se amplia esta prova para
FullNoLoss.

#### Knowledge09 focal sem replay de Memory

A rodada09 usou o mesmo bundle 9678/binário Compile11. PROJECT hybrid/dot
passou HTTP 200 partial/model_unavailable em 49.494 ms do click até JSON completo;
o estágio completo levou 58.274 ms. Shipping 131 ms e signin 1.051 ms são medições
separadas. Essa é uma medição efetiva de query_http, diferente da observação
IAB08 até 93,434 s, que não media instante de conclusão nem latência.

O runner saiu 1 depois na linha 310: esperava no_corpus, mas o contexto exibia
publication_version 1 vazio; a primeira asserção published_empty passou.
Root e owner Knowledge confirmaram o
[contrato do watcher](../architecture/rust-knowledge-foreground.md#knowledge-no-foreground-rust):
Knowledge-only/NoM publica geração 1 vazia com docs ausente ou vazio;
r9-empty-foreground-09 passou uma função/dois cenários em 11,11 s.
PublishedEmpty09 é correto, não hipótese de defeito frontend. Knowledge NoCorpus não foi observado nessa coleta Browser; R17 o reconcilia
por camadas abaixo, sem gap funcional. Host09 saiu 0, 1 PASS/0 FAIL em 123,03 s, com
done/cancel/reopen concluídos.

A espera do runner foi ajustada para 130000 ms, fixture 120000 ms mais 10 s de
folga, sem mudar limite produtivo. Host10 está ativo somente para
imported/cancel/narrow/zeroIO com --records-focus knowledge-imported, reutiliza
Memory08 e hybrid PROJECT09 e ainda não tem resultado. Memory permanece
qualificada independentemente; Knowledge completo segue pendente.

#### R16: entrada importada e asserts sem replay

Host10 saiu 0, 1 PASS em 66,33 s. Runner10 falhou no signin, linha 245, antes de
HTTP das APIs de domínio: /knowledge/contexts devolvia 404. A rota React
existia, mas não era servida por Foreground. Root cedeu o hunk
foreground_ui.rs e seu teste: Source agora adiciona a rota fixa no mesmo
index e casos GET/HEAD/query/404 de subrota no teste existente.
Cargo route01 terminou exit 101 após 102,085 s por No space left on device/os28
ao escrever target/debug/deps/liblbug-6a3a53fb057dfaeb.rlib. Libtest não
executou e o assert de rota não foi alcançado: é bloqueio de ambiente,
não falha do assert. Compile12 não iniciou porque o orquestrador parou no
erro; runtime11 também não iniciou nessa tentativa. A rota R16 estava
implementada, ainda sem qualificação nessa coleta. df mostrou 109M livres/100%; Root foi avisado para recuperar
espaço compartilhado. Não houve retry ou clean do target pelo consumer.
py_compile, diff check e rustfmt check saíram 0 sem qualificar runtime.
O bundle 9678 não mudou.

R16 está salva na Task antes de Source. O runner valida ausência de discovery
global projects/project-catalog/project contexts e user-memory em import-only,
acrescentando somente leitura USER zeroDiscovery global que faltava.
Também observa evento terminal de abort após release e tenta entregar tarde
o HTTP 200 real retido pelo padrão AST existente. São asserts implementados,
não resultados de execução. Não se repetem CRUD Memory08, hybrid09, Task,
Gantt ou refs.

#### Retry autorizado e harness da rodada11

Root liberou 18 GB removendo somente executáveis antigos e autorizou retry.
Route02 passou 1/0 em 0,10 s de runtime, 78,788 s no comando com compile/test,
cobrindo GET/HEAD/query/404 subrota. Compile12 saiu 0: 9,368 s no comando e
9,17 s reportados pelo compilador. O executável real é
foreground_tasks-01664d47aa20fd05. A rota tem prova unitária; runtime ainda
não recebe PASS por isso.

Host12 está ativo usando o executável extraído do log Compile12, sem
recompilar. SHA será gravado ao encerrar. Import/cancel/late/zeroIO e o
resultado Runtime12 continuam pendentes; nenhum PASS é antecipado.

#### Browser13: leituras Knowledge e cancel comprovado, estágio aberto

Browser13 usou bundle 9678/binário Compile12 SHA-256
1330be07d996d72607c29038dc2ee9e0d69809c926a5a24188071c3c9ee84e31.
A entrada /knowledge/contexts/reload e inventory→heads→page/reload importados
passaram em 32.892 ms, com SourceKey exato em URI/HTTP e zero PROJECT/Task/global
discovery no intervalo observado. Semantic/l2 retornou HTTP 200
unavailable/model_unavailable em 6.569 ms e hybrid/dot HTTP 200
partial/model_unavailable em 14.789 ms, click→JSON completo. Slug literal
PROJECT ausente passou em 18.520 ms. PublishedEmpty inicial/HTTP 200/seleção exata
passou em 1.191 ms, sem congelar startup; Knowledge NoCorpus continua explicitamente não observado
no Browser, com cobertura R17 por camadas abaixo.

Cancel13 comprovou issuer_entered/released e terminal net::ERR_ABORTED.
O estágio falhou depois no get_by_text exact=True, linha 396, pois o container
inclui Retry search; a captura mostra Search request stopped. Retry search.
Isso não comprova defeito UI nem transforma automaticamente o estágio em PASS.
Host13 saiu 0, 1 PASS/138,54 s; runner saiu 1. Os stages anteriores permanecem
qualificados independentemente.

O próximo knowledge-controls ajustará matcher ao padrão AST exact=False e
exerceria só cancel terminal/late, USER read, narrow e logout; os resultados
14/15 e a reconciliação final estão abaixo. Não recompila nem repete imports, absent, MemoryCRUD, Task,
Gantt, refs ou hybrid já qualificados.

#### Qualificação final db07 por camadas R17

Browser14 passou SEARCH cancel aceito/issuer entered/release/terminal
ERR_ABORTED em 9.202 ms. O reply importado HTTP 200 real com um hit foi retido;
Cancel terminal→fulfill_after_cancel→sem resultados passou em 7.540 ms. Isso
qualifica a tentativa de entrega tardia real, não afirma que JS a recebeu.
USER read HTTP 200 em 801 ms fez somente /api/user-memory/list, sem I/O
PROJECT/Task/global discovery no intervalo. Narrow14 chegou a logout 204,
mas page.goto competiu com o redirect de AuthIdentity.tsx:66; a falha do
runner é preservada. Host14 passou 1/0 em 58,76 s.

R17 aceita reconciliação por camadas. Knowledge NoCorpus não foi observado
no Browser, mas não é gap funcional: wire nativeKnowledge.test.ts103 e
React WikiExplorerNative.test.tsx47 distinguem os estados. Backend HTTP real
tsk-c135 COMPLETED 20, checks K2/KT1 PASS, preserva HTTP08 (13 Knowledge/7,99s)
na Task durável; Source atual knowledge.rs251–259 verifica mirror no_corpus
para heads/page e 342–357 distingue publicação vazia. Logs históricos .build
estão ausentes; não se afirma releitura nem nova execução desses logs.

HEADS PROJECT cancel/context change tem prova UNIT em
WikiExplorerNative.test.tsx63/oldHeadsSignal abort73; late74–75 resolve PAGE.
Source WikiExplorerPage47–55/cleanup73 tem guards. R18 acrescenta prova UNIT
para HEADS importados: ImportedKnowledge.test.tsx66–91 adia importedHeads,
verifica o sinal antigo abortado após trocar a tupla, aguarda os heads Mirror
e só então resolve HEADS e PAGE antigos. A lista atual permanece; título e
slug antigos ficam ausentes. Browser14 cancel/late é SEARCH; nenhuma dessas
provas declara cancelamento ou resposta tardia HEADS por HTTP.

A busca usa FTS, semantic ou hybrid, `top_k:20`, filtros aninhados e opção de
histórico. Query não vazia tem debounce de 250 ms; cancelamento e chave de query/
escopo descartam respostas tardias. Scores, distances, hits, modelo e geração
são dados nativos, disponíveis no JSON completo com inteiros exatos. Modelo
indisponível, cobertura ausente e identidade incompatível são estados próprios,
distintos de erro HTTP ou resultado vazio saudável. Não há filtro local
substituindo a busca. O cliente Go anterior foi retirado; a página conserva
ordem SDK e body completo, sem sort de prioridade ou remoção de headings Go.

Mutation passou seis testes e Search três. A rodada de clientes, incluindo
readers afetados, passou 25 casos (harness 3,63 s); não se somam subconjuntos
sobrepostos. A rodada anterior da página passou treze testes portados para SDK
(harness 7,04 s). A rodada atual passou 34 casos (harness 6,77 s): dezesseis de
produto, sete de reader e onze de workspace, com três regressões novas.
O primeiro caso de histórico falhou por manter a revisão antiga selecionada;
essa falha e a correção de reset por revisão permanecem registradas. O lint
scoped dos três arquivos terminou com zero erros e warnings.
A primeira rodada de 29 teve seis falhas de fixture em labels, StyledSelect e
fila one-shot; as correções e tentativas ficam preservadas. Typecheck e lint
dos quatro arquivos NativeMemory passaram, com zero erros e warnings.
São provas Linux/Node 22/jsdom, sem qualificação HTTP/CUA real. Os nove testes
HTTP backend de `tsk-9d46` — PROJECT CRUD dois, USER CRUD dois, receipts três e
Search dois — pertencem ao produtor: não são testes React. O build atual passou
em 5,63 s; bytecompare atual e os dois testes Rust de assets passaram no escopo
acima. Esses passes locais não qualificavam Browser sozinhos; a prova real
e a reconciliação R17 acima qualificam apenas o escopo db07, sem ampliar A6/T5.

## Escrita e busca HTTP nativas

As rotas `POST /api/memory/create|update|delete|search` usam o envelope
`{selection,input}`. As rotas equivalentes `/api/user-memory/...` recebem apenas
`{input}`. A identidade USER e o ator vêm do principal Web verificado, nunca do
corpo. Campos de ator, issuer/subject livres ou seleção PROJECT no corpo USER
são rejeitados. Cada escrita PROJECT obtém autorização Broker **Memory/write**;
Read não a substitui. USER exige habilitação explícita do provider e a dupla
`MutateUser`/`WriteMemory` na política da instalação.

| Operação | Campos de `input` |
| --- | --- |
| create | `idempotency_key`, `title`, `body`, `attributes:{classification,mandatory,important}`, `tags` e `references` opcional |
| update | `id`, `expected_revision`, `idempotency_key`, `reason` e campos opcionais `title`, `body`, `classification`, `mandatory`, `important`, `tags`, `references` |
| delete | `id`, `expected_revision`, `idempotency_key`, `reason` |
| search | `text`, `top_k`, `mode:fts|semantic|hybrid` (default fts), `filter` com os predicados da listagem e `include_deleted_history` (default false) |

Escrita retorna um recibo em `native`: `state`, `id` e `revision` quando
conhecidos, `idempotency_key`, `sdk_version`, `generation` e `recovery_key`
quando o SDK os fornece, além de `completion_error` e `delivery_error` causais.
`completed` conserva o ID/revisão retornados pelo store; `confirmed` conserva
publicação já aplicada mesmo quando há falha posterior; `uncertain` conserva a
chave de recuperação de um commit com resultado incerto. `not_applied` indica
que o índice foi publicado, mas o CRUD não foi confirmado nem chegou a um
commit incerto. O campo opcional `index_publication` conserva separadamente
`sdk_version`, `generation` USER (ou null em PROJECT) e `recovery_key` desse
índice; ele é omitido quando ausente. A publicação lexical não é uma criação
ou revisão do registro. Se houver Commit CRUD incerto na causa, `uncertain`
e sua chave continuam sendo do CRUD; se o CRUD já confirmou, sua versão e
chave não são substituídas pelas do índice. Nenhuma dessas observações concede
autoridade ou inicia retry automático. O recibo não inclui
body, title ou histórico: Write não concede Read. O consumidor reabre get/history
com sua autorização de leitura independente.

Se a serialização do recibo falhar por limite concreto ou erro do encoder,
o serviço responde **500 sem corpo** e conserva metadados mínimos em headers:
`x-graphit-memory-outcome` (`completed`, `confirmed`, `uncertain` ou `not_applied`),
`x-graphit-memory-id`, `x-graphit-memory-revision` e
`x-graphit-memory-sdk-version`, quando conhecidos. A publicação lexical também
conserva `x-graphit-memory-index-sdk-version` quando conhecida, sem fabricar
geração ou chave completas no fallback. Inteiros permanecem decimais
exatos, inclusive acima de 2^53. `x-graphit-memory-response-error` distingue
`response_limit` e `response_encoding_failed`; os headers opcionais
`x-graphit-memory-completion-error` e `x-graphit-memory-delivery-error` conservam
as causas já conhecidas. Não há corpo de registro nem outro limite ou operação
SDK. O consumidor deve observar esses headers antes de classificar o 500:
confirmação não autoriza retry, e `uncertain` exige reobservação com admissão
nova. Os dados completos do recibo continuam sujeitos ao limite JSON normal.

Conflitos de idempotência e revisão retornam 409. O conflito CAS conserva
`expected` e `actual` como literais u64, sem arredondamento acima de 2^53.
Memory ausente em uma mutação retorna 404; get/history mantêm seu contrato de
`null`. Cada chave pertence à operação e ao ator persistido pelo SDK, e replay
não cria uma nova revisão. A confirmação dos efeitos e os rechecks de entrega
usam o mesmo Original; uma falha posterior não apaga o resultado conhecido.

Busca conserva os estados nativos `no_corpus`, `published_empty`, `unavailable`
e `published`, geração, total, ordem de hits e scores/distâncias. `mode` observa
o modo solicitado; a identidade de modelo acompanha os resultados semantic e
hybrid. Missing coverage e modelo incompatível não viram lista vazia saudável.
O App não ordena, filtra um catálogo local nem implementa RRF; o SDK usa seu pin
para a consulta. Ausência do modelo do daemon é explícita, sem fallback FTS.
O browser compartilha o Arc de recursos e o único child/client existentes;
o empréstimo é atualizável. A identidade Runtime autoriza somente o serviço de
inferência no mesmo Original, enquanto a ReadAdmission Web autoriza o corpus.
USER continua sem seleção, grant ou IO PROJECT/Task.

A qualificação funcional de `tsk-9d46` passou nove casos novos: dois HTTP
PROJECT CRUD, dois HTTP USER CRUD, três de decoder/recibos e dois de busca.
Eles verificam dados SDK persistidos, CAS e idempotência, revisões u64 exatas,
ator verificado, Write separado de Read, USER sem chamadas PROJECT e
confirmação preservada em saída limitada. A busca PROJECT/USER executou FTS,
semantic e hybrid com os índices SDK e o processo de modelo CPU real,
preservando filtros, geração, modelo e score/distância. A indisponibilidade do
modelo retorna estado explícito, sem fallback ou catálogo vazio. Após uma nova
escrita USER sem embeddings, semantic e hybrid retornam `unavailable` com
`reason:missing_coverage`, geração atual e sem hits parciais. Esse delta foi
qualificado no mesmo caso USER, sem somá-lo novamente aos nove casos. A fixture usa
um único componente produtivo de modelo, sem watcher: não ignora cancelamento
nem altera os contratos de shutdown dos supervisores.

A qualidade de produção App/lib passou no strict compartilhado de
`tsk-6ded`, junto dos targets `knowledge_export` e `knowledge_conversation`:
`cargo clippy --offline --locked -p graphit-app --lib --test knowledge_export
--test knowledge_conversation --no-deps -j4 -- -D warnings`. Essa evidência,
reutilizada sem replay, não afirma lint de `cfg(test)` da biblioteca, CLI ou
todos os targets. Os consumidores Memory têm suas execuções HTTP reais, formato
causal e contrato de transporte conferidos no receipt próprio.

## Implementação e validação

`foreground_memory.rs` faz a adaptação finita para `NativeMemoryStore::list/get/history`. `task_server/browser.rs` seleciona ReadMemory antes da admissão PROJECT; `browser_auth/task.rs` mantém a sessão e o trabalho aceito. Os tipos canônicos de Records têm apenas a serialização necessária ao consumidor, sem tornar os campos privados de consolidação construíveis pela API browser.

A suíte afetada usa o issuer OAuth assinado e PROJECT HTTP dos testes existentes, grava Memory com Auth nativo e Lance e reabre o leitor. Verifica resultados, filtros, criação/histórico/exclusão, seleção exata, negação de módulo, ausência de PROJECT, perfil indisponível, corpo estrito, prazo, parada, remoção do provider, limites e manifest inválido. A equivalência da serialização de Creation e das sete operações com os codecs persistidos é verificada separadamente. A qualificação Linux passou 12 cenários HTTP Memory (10 na suíte focal, um caso de duas versões admitidas e um manifesto ASCII curto), um caso de saída limitada e dois casos de equivalência canônica. As regressões afetadas passaram quatro cenários Task e 11 de referências (oito HTTP e três de parser/navegação), totalizando 30 funcionais distintos; reruns não são somados. Um teste opcional preexistente de Broker público externo permaneceu ignored e não compõe essa qualificação. Strict App/lib+tests, Records/lib, formatação e links também passaram. A corrupção curta exigiu a correção de bounds do fornecedor Lance13 em `tsk-6062`, sem troca de pin; o consumidor HTTP passou com erro causal 500 e shutdown sem panic. Os logs iniciais falhos são preservados no receipt `.build/rust-native-memory-browser/qualification.json`. A composição não constitui qualificação da UI React nem de outras plataformas.

## Decoder fechado em uma passagem

A limpeza 4e50 retirou a árvore JSON intermediária e o segundo decode do mesmo
corpo. Cinco testes focais dos consumidores verificaram campos desconhecidos,
duplicatas escapadas, null/tipos inválidos, limites reais de bytes/texto/units,
IDs canônicos e NativePath literal sem perda de bytes. As doze regressões HTTP
Memory e treze Knowledge também passaram, incluindo permissões, corrupção,
cancelamento e limites de saída: 30 testes distintos desta unidade, sem ignored.
O pequeno QueryValue exigido pelo dispatcher existente permanece; o decoder não
cria um parser ou protocolo paralelo. Clippy App lib/testes passou com
`--no-deps -- -D warnings`; formato e documentação foram conferidos. Evidências ficam em
`.build/rust-fixed-browser-decoder/qualification.json`.

## Memory USER no browser

As rotas `/api/user-memory/list`, `/api/user-memory/get` e
`/api/user-memory/history` usam o mesmo `POST`, cookie, Origin e Content-Type das
rotas PROJECT. O corpo USER contém apenas `{input: ...}`. List aceita os mesmos
filtros; get/history recebem `{id: MemoryId}`. Não recebe seleção de projeto,
versão, instalação, issuer, subject, ator ou `scope_id` como autoridade.

A identidade vem do access JWT da sessão Web, com seu issuer e subject literais.
A instalação vem da captura confiável do processo. A leitura exige tanto
`ReadUser` quanto `ReadMemory`, revalida o provider Broker configurado e usa o
mesmo Original em captura, admissão, SDK e entrega. Ela ocorre antes da seleção
PROJECT e não consulta a autorização de projetos. Um JWT válido continua sem
conceder permissão PROJECT automaticamente.

Na fábrica real do Foreground, os providers explicitamente configurados em
`task.browser.providers` nas camadas Distribution, Global, Environment ou
Command habilitam leitura e escrita da própria Memory USER. A camada PROJECT e
o corpo da requisição não podem conceder essa permissão. Sem essa configuração,
ou para outro provider, a política USER permanece negada. Essa configuração habilita `MutateUser`, `WriteMemory` e `WriteFencedMemory` para
os providers browser selecionados. Não concede acesso a projetos.

Cada resposta contém `{scope_id, currentOwner, native}`. `currentOwner` observa a
instalação e a identidade admitidas; `scope_id` é a representação canônica do
array JSON `[installation_id, issuer, subject]`, entregue como string. Ambos são
dados para o consumidor, sem função de grant ou credencial. Não há normalização
de issuer/subject ou herança de projeto, versão, branch física ou pin PROJECT.

O resultado `native` de list é `no_corpus` ou `published` com generation e
memories, inclusive quando a lista publicada está vazia. Get retorna `null` para
ausente/excluída; history retorna `null` para ausente e preserva o histórico
completo de registros excluídos. Os objetos canônicos USER incluem owner,
autores, entrada de criação, revisões, geração, referências e timestamps.
Leituras ausentes não criam corpus ou diretórios USER. O SDK reabre os dados
persistidos e valida a parte selecionada até EOF; erros não entregam prefixos
bem-sucedidos. A saída nativa usa o encoder limitado já existente, com uma única
serialização emprestada e movimento do buffer para HTTP.

O corpo fechado usa um decode Serde e os limites concretos de bytes, tags e IDs.
As respostas mantêm `Cache-Control: no-store`. Corpo inválido recebe 400,
cookie/JWT inválido ou expirado recebe 401, e origem ou permissões negadas recebe
403. Limite de saída/corpus inválido recebe erro causal 500, sem dados parciais;
cancelamento e parada impedem a entrega. UserInfo continua opcional para perfil,
sem consulta obrigatória por leitura USER. A janela de revogação e mudanças de
claims permanece a expiração do access token conforme
[autenticação browser](rust_browser_auth.md), sem introduzir outro TTL.

A qualificação Linux desta entrega passou dez testes focais: sete cenários HTTP
USER e um decoder na biblioteca, mais dois cenários que iniciam a fábrica real
Foreground. Eles verificam dados persistidos, catálogo publicado vazio,
criação/histórico/exclusão, filtros, isolamento por subject literal, negações de
permissão, cookie/Origin/método/corpo estrito, expiração, provider removido,
parada, limites de bytes, manifesto USER corrompido e symlink de runtime. A
fábrica real lê USER sem grant PROJECT e com UserInfo indisponível; configuração
ausente ou originada em PROJECT não habilita o browser USER e não cria storage.
As guardas de Auth e Records já qualificadas para issuer/instalação distintos,
perfil não relacionado, Original estrangeiro, cancelamento e prazo são evidência
dos fornecedores, separadas desses dez testes e sem repetição de suas suítes.
O receipt da unidade fica em
`.build/rust-native-user-memory-browser/qualification.json`. Esta qualificação
não abrange a interface React ou outras plataformas.

## Contextos autorizados do módulo

`POST /api/memory/contexts` recebe `{input:{}}`, cookie Web e Origin exato. O módulo Memory é fixado pela rota. A resposta `{contexts,current}` inclui somente as tuplas admitidas por Memory/read; current é null quando a seleção configurada não foi admitida. Task negado não impede Memory permitido. Versão e NativePath são observações exatas de cada seleção, não grants. A qualificação funcional e a composição de [referências nativas](rust_foreground_record_references.md) pertencem a tsk-ef75. USER continua independente de PROJECT; suas rotas não chamam este discovery.

## Armazenamento selecionado

O issuer compartilhado resolve o backend a partir da admissão viva de cada operação.
Memory/Task, seus índices e jobs usam a tabela Lance local ou S3 selecionada pelo provider,
com branches por versão PROJECT; USER Memory mantém seu proprietário verificado e a instalação,
sem branch Git. O Broker autoriza e emite STSv3; os dados trafegam diretamente com S3.
Falhas remotas não provocam escrita local. Novas operações capturam o head atual do SDK,
preservando CAS, histórico e snapshots já abertos. Contrato, configuração e layout em
[S3 STS storage](../guides/s3-and-ui-network.md#memory-and-task-tables-and-versions).
