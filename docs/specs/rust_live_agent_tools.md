---
title: MCP USER de leitura para runs Live Rust
doc_type: specification
references:
  - target: {type: task, id: tsk-794d}
    relation: implements
  - target: {type: task, id: tsk-69f2}
    relation: relates_to
  - target: {type: task, id: tsk-01eb}
    relation: derived_from
---

# MCP USER de leitura para runs Live Rust

O produtor interno da folha `tsk-794d` permite que um processo Live consulte
Knowledge e AST selecionado sob a identidade Web e as seleções do próprio run. A arquitetura e os
demais contratos estão em [Consulta documental e LiveSearch](../architecture/rust-knowledge-ai-live.md).

**Estado:** a composição Kiro V3 compilou na App24h. O novo caso real
NoCorpus→Published passou (4,42 s), e a regressão de seleção principal Web/MCP
passou (17,38 s), ambos exatos, com stack padrão, sem falhas ou sinal.
AST/CXX passou na App24f (27,67 s); seu Source não mudou neste delta.
Clippy atual App normal e o teste externo Role passaram na execução central
App24h (36,40 s). Isso não lintou os módulos cfg(test) gerais nem T7. O delta
T7 foi validado pela compilação do libtest (138 s), formatação/diff e pelos
dois focais executados acima, sem campanha ampla de testes ou lint.

A App24c já havia aprovado os nove testes de catálogo/proxy, incluindo enums
AST, e imprimido três casos MCP aprovados (CPU, expiração Web e
Original/deadline). Esses resultados são reutilizados por escopo; não houve
reexecução integral de toda a suíte na App24f. A falha de setup AST por provedor
duplicado foi corrigida compondo Task e Source da mesma autoridade pelo helper
Graph. O stackguard anterior foi observado no libharness, sem prova de falha
do binário Main. A App24f inclui os cortes locais `Box::pin` de `execute`
e `call`, o payload interno `Operation::Call` em Box tipado e a limpeza
coesa do leitor compartilhado. Os dois focais aprovados não isolam o efeito
individual de cada ajuste. Original, borrows, admissions e a mesma future
aguardada foram preservados; nenhum limite de stack foi alterado.

## Catálogo fechado

| Ferramenta | Entrada | Fonte autorizada |
| --- | --- | --- |
| `graphit_live_context` | Objeto vazio | PROJECT efetivo e contextos selecionados |
| `graphit_knowledge_heads` | `context` opcional | Heads do PROJECT ou contexto selecionado |
| `graphit_knowledge_page` | `slug`, `context` opcional | Página, geração e referências reais |
| `graphit_knowledge_search` | `search`, `context` opcional | Leitor FTS, semantic ou hybrid existente |

Omitir `context` seleciona o PROJECT e a versão registrados na Session. Um contexto
explícito PROJECT ou importado exige a tupla inteira `source/project_id/instance_id/project_kind/type/id/version`
presente na seleção do run; ID ou slug isolado não autoriza uma fonte.
Para PROJECT, `source=project`, `type=knowledge`, `project_id=id`, versão real
e campos `instance_id`/`project_kind` ausentes identificam o membro. O leitor
resolve esse membro e versão em Resources, com membership atual e nova admissão
Broker ReadKnowledge; nunca substitui uma seleção explícita pelo PROJECT da Session.

`search` conserva o input Knowledge: `query` e `mode` obrigatórios, `metric`,
`top_k`, `page_size`, `preview` e `cursor`. Argumentos malformados são recusados como `invalid_arguments`. A validação e os resultados de
indisponibilidade/degradação pertencem ao leitor existente, sem fallback novo.
O catálogo sem AST conserva essas quatro ferramentas. A composição Source agora
acrescenta AST somente sob seleção persistida e produtor ReadAst próprios; sua
qualificação do produtor usa os focais App24f; a jornada HTTP consumidora
continua com seu próprio aceite. Não expõe Shell,
escrita, filesystem genérico, Auth, Task, Memory, AI ou forwarding genérico.

Os nomes das ferramentas deste produtor são constantes do host. O catálogo e
o proxy não recebem `Brand`; nomes MCP derivados de uma marca de redistribuição
não estão qualificados por esta folha.

## AST selecionado: produtor exercitado na App24f

A fachada da mesma folha acrescenta quatro leitores quando houver AST
explicitamente preparado e persistido, com composição real de Graph/Source:

| Ferramenta | Entrada além de `context` obrigatório |
| --- | --- |
| `graphit_ast_source` | SourceParameters flat: `path`, entity e seletores existentes |
| `graphit_ast_schema` | Nenhuma |
| `graphit_ast_query` | QueryData flat: `query`, `parameters` opcional |
| `graphit_ast_search` | Input flat: `query`, `mode`, `k`, `metric` |

As quatro exigem `context` fulltuple AST selecionado e persistido. O agente descobre as tuplas em `inputSchema.properties.context.enum` das ferramentas AST retornadas por `tools/list`. Todas as quatro têm a mesma enum, formada somente pelas seleções AST preparadas do run. O proxy preserva essa enum tipada e canonicaliza o restante dos schemas; enum ausente, divergente ou duplicada é recusada. Descoberta de metadata não concede leitura de Source ou grafo. Não usam PROJECT default nem um grant Knowledge/Hub como ReadAst. Cada chamada obtém sua admissão AST própria e lê os dados disponíveis nessa operação. Atualizações derivadas não invalidam o run preparado. PROJECT sem grafo preserva a ausência SDK real; Source pode ler o arquivo do path e versão selecionados, sem índice ou Git, inclusive extensão não suportada. Importado/remoto preserva a seleção da publicação autorizada e sua integridade, sem recorrer ao disco do PROJECT corrente.

LIST usa metadata de Session/USER journal/turn/entrada Web verificados: quatro
ferramentas sem AST preparado, oito com AST persistido e reader real. Não invoca
CONTEXT nem exige PROJECT Knowledge para autorizar AST ou metadata. Knowledge e
AST fazem admissões independentes por chamada. O proxy valida o conjunto exato
de quatro ou oito nomes, sem
duplicatas, subconjuntos mistos ou nomes desconhecidos. O viewer Knowledge não
interpretará AST como página.

O catálogo HTTP do preparador recebe SourceServer real e usa a admissão AST
própria para oferecer seleções PROJECT e instaladas. Observa metadata do grafo;
a ausência de grafo preserva Source físico autorizado. Não carrega corpo Source
ou NN nessa descoberta. Esse catálogo prepara a seleção; tools/list do run
consulta apenas o journal e metadados já preparados.

Root compõe SourceServer/GraphServer e NativeMcpPlan; o produtor AST isolado
fornece AstSelection e CapturedAstContext. O template MD consome as listas
`live_knowledge_tools` e `live_ast_tools` fornecidas pelo renderer; os grants
continuam readonly/noShell e têm os mesmos quatro ou oito seletores. Search
reutiliza os modos fts, semantic (default), vector e hybrid do DTO existente.
As fontes compilaram e o focal AST do produtor passou na App24f com stack padrão.
Não há alegação de AST Live entregue por existir o módulo isolado ou pelas
provas Graph/Gemma/CodeQuery já qualificadas por seus próprios donos.

Query e Source por entity usam blocking jobs CXX. O caminho AST da fila conserva
e aguarda a mesma chamada após cancelamento da chamada, EOF, cancelamento do
Original ou deadline, retendo admissions/leases/capturas e suprimindo a resposta
cancelada. A causa observada antes da espera não é substituída pelo resultado
tardio. Rechecks antes de iniciar o reader impedem novo trabalho após cancel
já observado. EOF não cancela o Original do run. Essa
custódia é distinta do owner ModelService descrito abaixo; descartar a future
não comprova o join CXX. O focal AST passou na App24f, incluindo as cinco causas de custódia
descritas em Fronteiras e qualificação.

## Autoridade e seleção

O job conserva o mesmo `&RequestContext` original do run, sem reconstrução de
Snapshot, Role, Runtime ou administrador. Cada list/call verifica a entrada
BrowserAuth atual, viva, não expirada e fisicamente igual à registrada;
verifica novamente o JWT contra o `RegisteredWebClient` real; e compara issuer,
subject e revisão do provider com o dono do run.

Somente então admite USER Knowledge read no escopo capturado, seguido de nova
admissão do módulo realmente lido: PROJECT/imported ReadKnowledge ou ReadAst.
LIST usa apenas o gate de metadata autorizado. Antes da entrega revalida
Original, autoridade, seleção e estado. O dispatcher exige Session `Running`,
mesma geração, PROJECT, binding, agente, artefatos, modelo e último Prompt
associado ao `turn_id`.

`SelectedContext` é a associação entre seleção completa e `SourceKey` canônico
fornecida pelo resolver produtivo. É proveniência, não admissão. O consumer
retém as capturas e fornece o recheck `allowed` real. O viewer projeta apenas o
PROJECT admitido e os contextos selecionados; não enumera projetos ou registry
globais. O viewer admite o PROJECT da Session e cada PROJECT selecionado;
Membership PROJECT é revalidado após a admissão assíncrona também no viewer.

## Leitura independente de publicações derivadas

A preparação persiste as seleções autorizadas de PROJECT/versão e dos artefatos,
sem fingerprints agregados de Source, grafo, busca ou embeddings. O journal não
congela a cabeça dos corpora derivados. Cada chamada do MCP ou viewer admite
novamente seu módulo e captura o snapshot nativo necessário àquela operação.
Uma publicação incremental concorrente não causa `context_changed` por si só.
Um corpus ausente pode aparecer entre duas chamadas e uma página pode retornar
seu conteúdo atualizado, sem refazer a preparação.

`NoCorpus`, corpus publicado vazio e registro ausente continuam estados distintos
da leitura atual. Se uma página, entidade ou Source não existir, a operação
informa essa ausência pontual; isso não invalida as demais seleções do run.
O mesmo snapshot nativo pode servir o corpo e as referências de uma página em
uma operação, sem exigir equivalência com outro corpus.

O kernel compartilhado com o viewer conserva os controles Running/turn do MCP.
O viewer usa sua própria identidade, cancelamento, prazo e admissões, sem criar
um run fictício. Identidade de projeto, branch/versão, seleção de artefatos,
autorização, revisões de escrita e integridade dos artefatos permanecem
obrigatórios. Os fingerprints do conteúdo das instruções instaladas continuam
protegendo a instalação e a preparação; não são gerações dos corpora derivados.

## Rota, concorrência e recuperação

`LiveMcpBroker` mantém fila Tokio de capacidade 1 por run. O handler HTTP envia
operação, bearer privado, cancelamento da chamada e canal de resposta. Auth e IO
acontecem no job que empresta o Original do run.

A chave `(WebSessionID, LiveID, turn_id)` está vinculada à Arc específica da rota
e ao contexto completo da Session capturada. `begin_run` constrói o owner sem
publicá-lo; `LiveMcpLaunch::apply_to` publica na fronteira do lançamento nativo
real, antes do primeiro poll do processo. Retry idempotente sem spawn não deve
ativar rota nova. Drop fecha a fila e remove somente a mesma Arc.

O consumer mantém `OwnedRunRoute::serve` polled como future independente em
paralelo com o runner. A espera de uma ferramenta não pode impedir o polling do
motor. Cancelamento da chamada limita sua espera/leitura/resposta e não cancela
o run. Original, deadline e stop continuam governando o trabalho. O owner Live
mantém a custódia e observa o join do processo; o proxy não declara esse término.

Após o join real do filho nativo, o consumer encerra a rota naturalmente:

```rust,ignore
let finish = owned.stop_handle(); // antes de emprestar owned em serve
// manter a mesma future serve polled junto de run_turn
// após run_turn juntar o filho:
finish.stop();
// aguardar a mesma serve; então descartar owned
// access.read e histórico final usam o mesmo Original ainda válido
```

`stop()` fecha novas chamadas, publicação e esperas de resposta. `serve` aguarda
a leitura já ativa terminar antes de retornar e descarta seus bytes. O Original
conserva seu cancelamento e deadline; não há prazo novo, contexto substituto ou
shutdown do ModelPool compartilhado. Drop protege a remoção da mesma Arc, mas o
caminho natural deve sinalizar e aguardar a future para reter IO e capturas.

Nas chamadas Knowledge, se cancelamento da chamada ou EOF chegar antes desse
sinal, o job descarta a future `execute`, libera a fila e suprime a resposta.
Nas chamadas AST aguarda a mesma future até o join; a fila só libera depois da
quiescência, com a resposta cancelada descartada. O ModelService conserva
owner nativo, guard, inputs, permit e join independentes até o término do kernel,
sinalizado pelo cancelamento RPC. Isso preserva a custódia do worker; não prova
que todos os kernels anteriores já terminaram quando `serve` retorna. O consumer
aguarda seus joins de processo/proxy e a leitura ainda ativa no stop natural;
não desliga o pool compartilhado para concluir uma turn.

## APIs de composição

| API interna | Responsabilidade |
| --- | --- |
| `catalog()` / `potential_catalog()` / `list(CallContext)` | Baseline, nomes potenciais fechados e metadata por run |
| `catalog_from_names(names)` / `catalog_from_tools(tools)` | Validar conjuntos exatos e conservar somente enum AST preparada |
| `call(CallContext, params)` | Executar leitores admitidos |
| `LiveMcpBroker::new()` / `router(limits, stop)` | Roteamento e serviço HTTP rmcp |
| `begin_run(Registration)` | Vincular entrada Web, Session, ReadContexts esperado, turno e Original reais |
| `OwnedRunRoute::launch(identity, original, address)` | Prova de lançamento e valores privados |
| `OwnedRunRoute::serve(RunContext)` | Atender fila no job original |
| `OwnedRunRoute::stop_handle()` / `RunRouteStop::stop()` | Sinalizar fecho natural e aguardar a mesma serve |
| `LiveMcpLaunch::apply_to(original, transport)` | Conferir prova/Original, publicar e aplicar ambiente |
| `LiveMcpLaunch::private_output()` | Valores privados para redação do sink |
| `LiveMcpProxy::from_environment(limits, timeout)` | Configurar o proxy do processo |
| `LiveMcpProxy::serve_stream(read, write)` | Servir stdio e drenar owners próprios |
| `LiveMcpProxy::request_stop()` | Sinalizar o token existente, mantendo o mesmo serve_stream aguardado |

O preparador obtém `ReadContextsSnapshot` pelo getter69 e passa Session e
ReadContexts do mesmo snapshot à Registration. O getter lê Session, contextos e
último Prompt do mesmo journal. Cada etapa do gate MCP usa esse único snapshot:
antes da leitura, compara-o ao expected retido pela rota e o fornece ao kernel;
depois da leitura, confere novamente esse vínculo antes de entregar a resposta.
Não substitui o expected pela geração mais recente. O viewer HTTP conserva seu
próprio recheck, sem exigir um estado Running fictício.

`CallContext`/`RunContext` emprestam backend `BrokerProjectAccess` real, recursos
ReferenceExplorer, Original, Access USER, serviço Live, Session, turno, seleções,
model slot e `allowed`. Também emprestam `source: Option<&SourceServer>` e
`ast: &[AstSelection]`; somente CallContext recebe o cancelamento incoming.
Fixtures Knowledge usam None e slice vazio. Resources sozinho não contém o
backend PROJECT. O job
retém o Arc retornado por `TaskServer::record_snapshot()` e usa
`record_resources(&snapshot)` até o join.

`launch` recebe apenas o `SocketAddr` loopback do listener MCP já bindado e
retido pelo serviço Root. Não recebe URL cliente nem descobre listener. A URL é
gerada internamente: `/api/live/sessions/{id}/turns/{turn}/mcp`. O SDK rmcp cuida
de framing stdio/Streamable HTTP; o serviço desabilita sessões MCP legadas e
usa respostas JSON. Um ID de sessão MCP não é autoridade Web.

## Ambiente privado e lifecycle

O plano nativo conserva executable capturado e argv constantes
`mcp --stdio --live-user`. URL, bearer, referência Web e turno não entram nos
bytes persistidos do agente. Launch substitui somente:

- `GRAPHIT_LIVE_MCP_URL`
- `GRAPHIT_LIVE_MCP_BEARER`
- `GRAPHIT_LIVE_WEB_SESSION`

Preserva os demais valores do ambiente. Os valores privados não podem aparecer
em Task, receipts, configuração global, agente renderizado ou logs; o sink usa
`private_output()` para sua redação.

O proxy valida HTTP para IP loopback literal, porta não zero (incluindo a porta HTTP padrão) e caminho esperado, sem
credenciais na URL, query ou fragmento. Desabilita proxy e redirects, marca
headers privados como sensíveis e não lê Runtime key, Role, profiles ou login
vendor. Cada list/call usa o SDK e a admissão do broker.

A CLI pode ligar SIGINT/SIGTERM ao token existente por `request_stop()` e deve
aguardar o mesmo `serve_stream`, incluindo os joins. Nenhum owner substituto é
criado para sinalizar a parada.

A inicialização HTTP conserva a custódia do transporte SDK: se discover falhar
ou for cancelado antes de existir uma conexão, o transporte volta ao mesmo
owner da chamada, que aguarda `Transport::close()`. Não há novo executor ou
parser. Depois da conexão, o owner também aguarda `connected.cancel()` e o
fecho do transporte devolvido.

EOF cancela transportes do proxy e drena todos os owners que criou, mesmo após
um erro de join. Falhas locais/transporte têm mensagens saneadas; erros
estruturados do broker seguem no `CallToolResult`. Isso não comprova término do
processo/run Live.

## Fronteiras e qualificação

Resolver Hub, captura de artefatos, renderer SDK e materialização nativa de
Agent/Skill/Rule pertencem ao preparador. Metadados de grants ou USER Knowledge
write não concedem ferramentas/Shell. O preparador valida grants reais e falha
causalmente para descriptor incompatível, sem reescrita permissiva.

Os focais da folha devem provar OAuth/Broker/USER reais, seleção inteira,
PROJECT/imported Knowledge e referências, identidade/provider/entry/turn,
Original, backpressure/RAII, stdio↔HTTP, Launch, cancelamento, EOF/drain e
polling independente. O focal PROJECT usa Session0 e membro selecionado1 de
mesmo slug, retirando cada membro em uma execução própria e comprovando o outro
e o importado. O focal CPU encaminha semantic/hybrid por default, PROJECT
explicitamente selecionado e importado, com ModelSlot e gerações reais. Também
retém uma leitura no Broker durante stop natural, verifica serve pendente e
confere Original/Access/histórico após o join. Isso não simula join do filho nativo,
que precisa ser provado pelo consumidor Root/69. Suites afetadas, strict, formatação e revisão inversa
Source↔doc permanecem requisitos; inspeção de Source não é PASS.

O focal AST usa a instalação real do helper Foreground existente, com Task e
Source da mesma autoridade, Raw e NN publicados pelo pipeline já usado pelos
testes maduros. Compartilha a preparação Core genuína com o focal Knowledge.
Confere MCP8 e suas enums, schema, query, Source por entity e FTS de PROJECT
e pacote Native exportado/importado pelo SDK existente, com capturas nativas e admissões
reais. O arquivo de extensão não indexada existe só no PROJECT: sua ausência
na cópia importada deve falhar, distinguindo fallback autorizado de troca de
fonte. Também confere tuplas inválidas e grants independentes. Usa a
barreira cfg(test) do reader após a execução CXX real, enquanto a mesma closure
ainda está ativa, para cinco causas: cancelamento SDK da chamada, EOF, cancel
do Original, deadline e stop natural. Exige serve pendente antes da liberação
e finished mais await da mesma serve após a liberação. O hold acontece depois
do retorno bem-sucedido do engine CXX, enquanto a closure ainda está ativa:
não testa interrupção dentro do engine. A prova da retenção por chamada soma
essa observação à revisão dos borrows de capture/admission/lease na mesma
execute; o teste não observa Drop individual desses objetos. Esses casos passaram
no focal AST exato da App24f. A jornada HTTP real
Create→prepare/Ready→Send/Main/MCP8 pertence ao consumidor Root/NativePublication
e passou no recibo App24g acima; a preparação Core deste focal não a substitui.

Fontes: [dispatcher](../../crates/graphit-app/src/live_agent_tools.rs),
[dispatcher AST](../../crates/graphit-app/src/live_agent_tools/ast.rs),
[proxy stdio](../../crates/graphit-app/src/live_agent_tools/stdio.rs),
[broker e Launch](../../crates/graphit-app/src/browser_auth/live_mcp.rs),
[focal Web real](../../crates/graphit-app/src/browser_auth/tests/live_mcp.rs),
[focal AST/CXX](../../crates/graphit-app/src/browser_auth/tests/live_mcp/ast.rs),
[expiração real](../../crates/graphit-app/src/browser_auth/tests/live_mcp/expiry.rs) e
[forwarding CPU](../../crates/graphit-app/src/browser_auth/tests/live_mcp/cpu.rs).
