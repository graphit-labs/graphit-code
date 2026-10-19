---
title: Contrato Rust de UI, MCP/proxy e tray
type: specification
status: partial-implementation-validation
updated: 2026-10-16
project_id: 01KSH1CRFFG8Z74B5ZS78WW808
references:
  - target: {type: task, id: tsk-864c, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-233f, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-4d03, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: specified_by
  - target: {type: task, id: tsk-ea45, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: derived_from
  - target: {type: task, id: tsk-17a7, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses_api
  - target: {type: task, id: tsk-51f2, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
  - target: {type: task, id: tsk-ef75, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
  - target: {type: task, id: tsk-b788, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
  - target: {type: task, id: tsk-d75f, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
  - target: {type: task, id: tsk-3c89, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses_api
  - target: {type: task, id: tsk-fa07, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses_api
  - target: {type: task, id: tsk-ff08, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses_api
  - target: {type: task, id: tsk-368b, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses_api
  - target: {type: task, id: tsk-0529, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: requires
  - target: {type: task, id: tsk-9c58, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
  - target: {type: task, id: tsk-cab0, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
---

# UI, MCP/proxy e tray: cobertura e integração necessária

See the current OpenUI and tray contracts below.

## Matriz de cobertura

| Jornada | Estado e produtor atual | Interface real e limite | Próxima responsabilidade |
| --- | --- | --- | --- |
| Hosting, BrowserAuth e Task/Gantt | Qualificado no recorte Linux de 17a7; 62 casos e onze checks reutilizados | Rotas Task finitas, assets Vite e seleção NativePath/ID/versão | Preservar a entrega; FullRefs A6/T5 segue em 51f2/ef75 |
| Navegação de referências | Parcial; 51f2/ef75 ainda necessários | APIs e readers nativos por domínio | Root/domínios entregam FullNoLoss, sem fallback ou vazio fictício |
| UI Hub | b788 bloqueada pelo transporte real d75f | Consumer depende do produtor concreto | Reconciliar com d75f antes de Source, sem DTO presumido |
| Source CLI e MCP STDIO | 3c89 COM14, fa07 COM21 e ff08 COM25 | Discovery e SDK no endereço atual; seleção capturada | Reutilizar seus escopos, não transformar discovery ausente em autostart |
| Role MCP | Produtor e consumidor existentes | RoleProxy/RoleToolServer e grants por operação | Manter a fronteira Role; não inferir cobertura de CLI geral/Dream |
| Now | Endpoint específico existente | Bearer/Origin de autoridade, NDJSON e subscriber Broker | Consumer React não qualificado por este packet |
| Serviço do usuário | Adaptadores e integração Linux 0529 Native05 qualificados; Windows/macOS pendentes | ServiceDefinition, launcher, controle autenticado e discovery | Reutilizar Linux; não equivale a tray |
| OpenUI | CLI/controle qualificado em Linux por 864c | Capacidade Browser capturada no startup; URL bound ou origem pública explícita validada | Reutilizar controle24j e Main/Foreground24l; tray separado |
| Tray Linux | Integração SNI/Dbusmenu e Main qualificada em 233f/26p | Consome OpenUI, launcher e Auth ativos | Protocolo Linux qualificado; visual e Win/mac separados |

## Browser e hosting já existentes

[`foreground_ui.rs`](../../crates/graphit-app/src/foreground_ui.rs) serve somente
as famílias Task declaradas e incorpora `index.html`, `logo.svg` e `favicon.svg`.
[`scripts/build-rust-ui.mjs`](../../scripts/build-rust-ui.mjs) já gera esses três
assets. `ForegroundTasks` captura `task.browser` no startup e mescla o listener
existente. OpenUI reutiliza essa capacidade explícita; não modifica providers nem
recompõe o router de um daemon headless em execução. Os resultados de
[Gantt](rust_task_gantt.md#verification) e [Foreground](rust_foreground_tasks.md)
são reutilizados, sem repetir testes ou ampliar sua qualificação.

[BrowserAuth](rust_browser_auth.md) expõe providers/login/callback/session/logout,
com cookie opaco HttpOnly, Origin exato e sem JWT em JavaScript. A origem canônica
do Task browser deriva do SocketAddr realmente bound. O JWT local dura até seu
TTL; cada request ainda precisa do grant PROJECT separado. A seleção conserva
NativePath, ID e versão exatos. Não se infere uma política global CORS wildcard.

Os POST tipados de `browser_auth/task.rs` incluem tasks context/contexts,
catalogue/get/session/view/session-heads/references e os readers Memory,
Knowledge e USER Memory existentes. Sua existência não expõe automaticamente
outras rotas da SPA. O inventário de endpoints e autoridade permanece nos
contratos dos produtores.

## Source MCP, Role e eventos

[`source_entry.rs`](../../crates/graphit-app/src/source_entry.rs) oferece
`call_source` e `stdio`, usando discovery e o SDK no endereço real. Discovery
ausente retorna NotFound, sem iniciar serviço. `SourceProxy::new`, `serve_stream`
e `shutdown`, em [`mcp_source.rs`](../../crates/graphit-app/src/mcp_source.rs),
usam root/versão capturados; a versão explícita prevalece. Não consultam Git por
read. EOF/cancelamento e joins pertencem aos owners existentes, com autoridade
ordinary/Runtime real, sem novo issuer. Veja [entrada](rust_source_entrypoint.md),
[callers](rust_source_callers.md) e [discovery](rust_source_discovery.md).

RoleProxy usa loopback `/roles/mcp`, bearer, reference e headers de seleção;
RoleToolServer exige os grants Role por operação. SourceServer mantém `/mcp`.
São escopos separados. A conexão SDK por chamada e seleção estável não exigem
inventar sessão SDK retida, túnel genérico ou replay de escrita ambígua.

No encaminhamento de ferramentas do catálogo, uma falha nativa informa somente
`stage` e uma categoria finita do SDK, distinguindo discovery, connect, call,
cleanup, validade do Original e espera do forwarder. Categorias como
`session_recovery_timeout`, `control_timeout`, `session_expired`, `rpc_error` e
`transport_closed` não formatam a mensagem privada do SDK, URL, header, bearer,
body ou dados de erro RPC. A categoria não inicia retry ou altera o protocolo.
O owner sempre aguarda o cleanup da conexão; quando call e cleanup falham, ambas
as causas permanecem retidas e o diagnóstico distingue a falha de cleanup sem
substituir a causa da chamada. Original inválido continua impedindo a liberação
de um resultado. Os prazos, cancelamento e joins existentes permanecem ativos.

`POST /api/now/subscribe`, em `source_server/now.rs`, autentica Bearer e a
enum Origin de autoridade do request, e transmite `application/x-ndjson` sobre
o Backend/subscriber Broker WebSocket. Não é Browser cookie SSE nem WebSocket
React geral. Polling e subscribe pertencem à tool autorizada específica:
[Now](rust_source_now.md) não cria uma fachada universal de transportes nem
qualifica aqui um consumer React Now.

## Boundary dos demais consumers Observatory

[`App.tsx`](../../ui/src/App.tsx) contém AST/workspace/Hub/context e
outras rotas, mas o hosting Foreground atual é finito e só serve a família Task.
Os consumers abaixo são fatos do frontend, não novas qualificações de 4d03:

| Consumer | Requests existentes | Fronteira ainda necessária |
| --- | --- | --- |
| `api/ast.ts` | GET search, contexts, schema, graph, graph/neighborhood, file, status; DELETE context; POST query | Ligar às APIs Rust reais de AST/GraphServer e seus owners |
| `api/daemon.ts` | GET `/daemon/status` | Handoff produtivo de status/control de 0529 |
| `api/live.ts` | `/api/live/sessions` CRUD/message/cancel e EventSource `/stream` | Produtor real Live, autorização e lifecycle próprios |
| `api/agentStream.ts` e `api/client.ts` | POST com Accept `text/event-stream` | Contrato SSE do consumer, não equivalência com Now NDJSON ou SDK MCP |

Embedding da SPA não entrega esses endpoints. Root e os owners de domínio devem
ligar cada consumer ao produtor real antes do cutover; não há rewrite global do
API client nesta folha. Referências mantêm USER outgoing explícito:
`include_user=false` falha com HTTP400 antes de IO; `true` usa o reader autorizado.
SourceKey/corpus tuple é distinto do namespace literal. O novo client próprio
precisa conservar números com lossless-json 4.3.1, sem `res.json()`; seu pin só
será aplicado por Root ao consumer real. Os shapes completos permanecem no
[contrato de referências Knowledge](rust_knowledge_reference_query.md), 51f2/ef75.

## Serviço produtivo, lazy OpenUI e liveness

[Serviços](rust_daemon_services.md) reutiliza `services.rs`/`control.rs`:
ServiceDefinition, ServiceCommands e UserService oferecem start/stop/restart,
status/install/set_login/uninstall com ProcessHost emprestado e Original Arc por
request. Status conserva Outcome/ServiceFailure reais e o owner de cleanup
pendente. Manager indisponível não vira “ausente” ou detached silencioso.
Windows/macOS nativos continuam pendentes; a prova Linux é do adaptador.

A integração Linux de 0529 foi qualificada por Native05; suas jornadas nativas
Windows/macOS continuam pendentes. PID, singleton, discovery, detached quando o
manager está indisponível, replacement e join são reutilizados por OpenUI.

OpenUI usa `graphit ui` fora de PROJECT. O CLI assegura Running pelo launcher
existente e envia `{"operation":"open_ui"}` ao mesmo `POST /admin/daemon`, com a
chave Runtime somente no header e Configure incondicional. O servidor devolve
`pid`, `state` e `ui_url` somente quando a capacidade Browser está composta e o
owner não está encerrando. O endpoint não aceita Origin de navegador nem
credenciais Web/PROJECT como administração nativa.

No fallback detached Linux, launchers concorrentes serializam a revalidação autenticada, o lançamento e a confirmação de readiness com `daemon-start.lock`. Esse lock usa o prazo e o cancelamento Original e é independente de `daemon.pid`, que permanece com o daemon. O próximo launcher observa o processo confirmado, sem criar outro filho. A prova cobre CLI/daemon/HTTP com `--no-open`; não afirma clique ou janela nativa, nem execução Windows/macOS.

O mesmo recibo preserva as falhas anteriores da corrida, do bundle alterado
durante compilação e do helper DCR obsoleto. `include_bytes!`, dep-info e
`rerun-if-changed` já rastreavam os assets corretamente; emitir após o asset
ficar estável incorporou o bundle final sem workaround de build. O emissor de
teste agora valida o callback HTTP literal-loopback como `native` e HTTPS como
`web`, sem mudar PKCE, state, nonce, claims ou autorização do produto.

ForegroundTasks retém a URL de entrada `/task/gantt` construída da mesma origem
validada por BrowserAuth: `task.browser.public_origin` explícita ou loopback do
listener realmente bound. Listener wildcard usa a mesma família/porta em
loopback; Host/X-Forwarded headers e o config do cliente não selecionam a URL.
A URL não contém chave, bearer, cookie, query ou fragmento. Discovery continua
apenas um endereço observado, nunca um grant ou prova de capacidade Browser.

Sem `task.browser` global configurado no startup, OpenUI retorna
`browser_not_configured`; não fabrica provider/ACL nem recria issuer/router.
`--no-open` imprime somente a URL e não requer desktop. Por padrão,
[open 5.4.4 that_detached](https://docs.rs/open/5.4.4/open/fn.that_detached.html)
seleciona o handler do sistema usando o ambiente desktop do processo. A URL e o
Original são validados antes da transferência. O lançamento ocorre fora do
grupo supervisionado de processos temporários: o navegador pode continuar
aberto após o helper, CLI ou tray terminarem. Sucesso confirma a aceitação do
lançamento pelo sistema; não comprova exibição de janela ou login. Erros de
lançamento retornam a URL para abertura manual e deixam o daemon ativo.

Shutdown continua nos owners existentes: BrowserAuth e SourceServer cancelam e
juntam seus jobs/conexões antes de remover discovery/runtime. OpenUI não cria
servidor por projeto, polling, sessão de autenticação ou TTL novo.

Liveness é requisito produtivo de 0529. A composição SourceServer inspecionada
não declara diretamente `/health`; localização e contrato do produtor ainda não foram
confirmados. Não se inventa endpoint, bypass de Auth por leitura de payload,
rota pública ou assinatura DTO para preencher essa lacuna.

## Tray Linux independente e limites de plataforma

Tray é processo desktop separado: possui ícone, menu, event loop e seus jobs até
join. Login por provider reutiliza Auth existente, sem segredos na UI. Reutiliza
control/discovery de 0529 e o produtor OpenUI de 864c para Start, Restart,
OpenUI, Quit e status/error. Quit remove o ícone e termina só depois do stop
resolvido; falha mantém o menu operável. Ausência de sessão gráfica, watcher ou
desktop não pode impedir daemon headless ou MCP. Não se cria framework de polling
nem se testa reinício do daemon pessoal.

O consumidor `graphit tray`, implementado em 233f, usa `ksni 0.3.6` com apenas
feature Tokio no Linux. Não inclui GTK, AppIndicator ou runtime paralelo. O
menu e a tipografia pertencem ao desktop; o ícone de 32 px deriva do favicon
mantido, com o mesmo bracket branco/cobalt. Não há novo desenho de React.

O registro local de autostart usa `Exec=graphit tray`: o executável é resolvido
no PATH da sessão gráfica e a instalação vem do ambiente dessa sessão. Esses
pré-requisitos precisam existir no desktop, além do shell de instalação. O
registro é um artefato operacional local; não há gerador desktop versionado
qualificado por esta entrega. A resolução pelo PATH segue a
[especificação Desktop Entry](https://specifications.freedesktop.org/desktop-entry/latest/exec-variables).

Status é obtido na abertura do menu, no startup e após ações, sem polling.
A atualização de leitura mantém as ações do menu disponíveis. Uma ação
explícita substitui uma atualização em andamento, preservando o clique.
Start/Restart/Quit usam o launcher e o controle Runtime reais; Open interface
usa o produtor 864c. Log in apresenta perfis Broker existentes agrupados por
provider, sem criar perfil nem escolher nome implícito. Sem perfil, indica o
CLI de configuração; com daemon parado, indica Start para carregar os perfis.
A autorização é a do Auth ativo e conserva seu fluxo OAuth/callback/publicação.
O anúncio de autorização usa a mesma transferência desktop destacada. O
cancelamento do login continua pertencendo ao Auth; a janela já aberta tem
vida independente e não é encerrada pelo cleanup de processos temporários.

Quit cancela e junta um login pendente antes de pedir Stop. Stop negado mantém o ícone/menu com causa e permite nova ação. Sucesso retira o serviço SNI com `Handle::shutdown().await` e junta o ProcessHost. SIGTERM/Ctrl-C encerra somente o consumidor desktop; não obriga o daemon headless a parar. Sessão/bus ausente e falhas duras de registro são erros causais do tray. No mesmo bus ativo, `assume_sni_available(true)` permite iniciar antes do watcher: o SDK conserva o serviço SNI e registra-o quando recebe a chegada do watcher por evento D-Bus. A espera não cria polling e participa do mesmo cancelamento/join; ausência permanente de suporte SNI não comprova ícone ou registro no desktop. Registro tardio em bus privado e ativação manual não comprovam autostart após novo login. O helper de abertura é privado e controlado; não se declara janela ou ícone visual.

[tray-icon 0.26.1](https://docs.rs/tray-icon/0.26.1/tray_icon/) suporta Windows,
macOS e Linux AppIndicator/KSNI. Windows/AppIndicator exigem loop na thread do
ícone; macOS exige main thread com loop rodando. KSNI usa worker D-Bus sem loop
GTK. O default Linux requer GTK/AppIndicator: Root deve escolher features/backend
explicitamente, não ambos. TrayIconBuilder/menu e handlers TrayIconEvent/MenuEvent
encaminham ao proxy do loop existente; não se implementa SDK D-Bus/menu próprio.
[ksni 0.3.6](https://docs.rs/ksni/0.3.6/ksni/) é o backend selecionado para o
consumidor Linux 233f, com default-features=false e somente Tokio. Pins/features
e resolução de lock permanecem com Root; o backend Win/mac não foi implementado.
Essas restrições oficiais, verificadas em 2026-10-13, não são prova de host nativo.
