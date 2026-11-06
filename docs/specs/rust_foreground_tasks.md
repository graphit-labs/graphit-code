---

title: Task browser no serviço Foreground Rust
doc_type: spec
description: Configuração, autenticação, leitura PROJECT e encerramento Task no listener Foreground existente.
references:
  - target: {type: task, id: tsk-0a1a}
    relation: implements
  - target: {type: task, id: tsk-9694}
    relation: derived_from
  - target: {type: task, id: tsk-744f}
    relation: uses_api
  - target: {type: task, id: tsk-ca81}
    relation: implements
  - target: {type: task, id: tsk-2447}
    relation: implements
  - target: {type: task, id: tsk-17a7}
    relation: specified_by
  - target: {type: task, id: tsk-8e5d}
    relation: implements
  - target: {type: task, id: tsk-6cf9}
    relation: uses_api
  - target: {type: task, id: tsk-db07}
    relation: implements
  - target: {type: task, id: tsk-ef75}
    relation: relates_to
  - target: {type: task, id: tsk-ef75}
    relation: uses_api
  - target: {type: task, id: tsk-b9b4}
    relation: derived_from
  - target: {type: task, id: tsk-51f2}
    relation: uses_api
---

# Task browser no serviço Foreground Rust

Current path contract: PROJECT roots and relative paths cross JSON/API/CLI as literal Unicode strings. Serde preserves spaces, accents, escapes and separators without lossy conversion. Non-Unicode input fails explicitly; `{os,units}` is rejected. Historical evidence below describes the code that was exercised then and does not qualify the removed path codec.

O operador pode habilitar leituras Task por cookie no mesmo listener de `daemon foreground`. A composição reutiliza DaemonAuth, projetos e versões capturados, discovery e sinal de parada do Foreground. A composição foi qualificada no Linux com o CLI compilado, OAuth/PROJECT TCP e Task persistido; resultados dos componentes anteriores são registrados separadamente.

## Habilitar o componente

Configure providers Broker pelo produtor [Auth existente](rust_browser_auth.md). Os nomes selecionados devem existir no mesmo issuer e todos devem ter exatamente o mesmo endpoint Broker. Forneça também o endpoint PROJECT explícito: mesma origem Broker aprovada, caminho `/v1/project/access/authorize`, sem query ou fragmento.

`task.browser` é um objeto estrito e opt-in. Ausente, não compõe rotas Task/browser por cookie; o TaskServer nativo, [Source](rust_source_foreground.md), Admin e NativeInstall continuam ativos. Source captura TaskPolicy e Registry nativos independentemente desse opt-in. A captura browser usa somente Distribution, Global, Environment e Command. Uma camada PROJECT não define essa confiança.

| Campo | Regra |
| --- | --- |
| `providers` | Array obrigatório, não vazio, nomes exatos, únicos e não brancos de providers Broker existentes. |
| `project_endpoint` | String obrigatória; endpoint PROJECT validado pelo SDK. |
| `allow_loopback_http` | Booleano, default `false`. |
| `login_timeout_ms` | Inteiro positivo representável, default `300000`. |
| `session_lifetime_ms` | Inteiro positivo representável, default `3600000`. |
| `refresh_margin_ms` | Inteiro representável, incluindo zero, default `60000`. |

Campos desconhecidos, tipos incorretos, null e parâmetros inválidos causam erro. Os limites são validados antes de servir. Margem zero permite refresh quando o SDK/expiração o exigir. Esses tempos governam fluxo e sessão browser; não substituem o `exp` do JWT nem prometem revogação imediata. O timeout de request vem de `HubResourcePolicy::RequestDeadlineMs`.

O `config.json` global da instalação, lido por `ConfigStore::global`, usa o envelope `settings`. Exemplo para um provider já existente `workspace`, cuja origem Broker é `https://broker.example`:

```json
{
  "settings": {
    "task.browser": {
      "providers": ["workspace"],
      "project_endpoint": "https://broker.example/v1/project/access/authorize",
      "allow_loopback_http": true
    }
  }
}
```

Preserve os demais settings ao alterar o documento. Ele seleciona providers, sem criar credenciais ou conceder PROJECT. O CLI aceita `--config/-c KEY=VALUE`; exemplo de quoting Unix:

```sh
graphit --config 'task.browser={"providers":["workspace"],"project_endpoint":"https://broker.example/v1/project/access/authorize","allow_loopback_http":true}' daemon foreground
```

O Environment usa a variável derivada de `ConfigKey::environment_variable` para a marca atual, com esse mesmo objeto JSON como valor. Não há novas flags. O listener atual é HTTP loopback. BrowserAuth aceita os literais `127.0.0.1` e `::1`: habilitar browser exige `allow_loopback_http: true` explícito, mesmo com Broker remoto HTTPS. O default `false` rejeita startup antes de publicar discovery.

## Startup, login e leitura

O owner reserva o `BoundListener` real. Sua origem canônica é `http://` mais o `SocketAddr` literal, inclusive IPv6. Não faz bind probe/rebind nem usa `Host`/`Forwarded` como confiança.

O mesmo issuer autentica o empréstimo Runtime no Original, cria o parent administrativo real e abre `NativeProfileManager::begin(ProfileManagerAuthority::Admin(...))`. O caller copia somente nomes, revisões e configuração Broker necessários aos verificadores/backend. Empréstimos de chave, caller e snapshot terminam antes de servir. Esse caminho não publica AuthDocument, não cria issuer e não usa perfil ativo como provider browser.

A descoberta só é publicada depois de listener e componentes válidos. Falha de startup conserva `ForegroundFailure` e os recursos necessários a cleanup. Providers fixos exigem drain/restart para mudanças de configuração; a admissão continua rejeitando provider removido ou revisão alterada.

| Endpoint | Jornada |
| --- | --- |
| `GET /auth/providers` | Listar providers configurados. |
| `POST /auth/login` | Iniciar OAuth com provider permitido e Origin exato; HTTP200 JSON `authorization_url` e cookie de fluxo opaco. |
| `GET /auth/callback` | Concluir o fluxo correspondente. |
| `GET /auth/session` | Ler identidade autenticada da sessão. |
| `POST /auth/logout` | Invalidar sessão com Origin exato. |
| `POST /api/tasks/context` | Obter contexto nativo selecionado. |
| `POST /api/tasks/contexts` | Listar somente os tuples projeto/versão retidos, com PROJECT fresco por item. |
| `POST /api/tasks/catalogue`, `/get`, `/session`, `/view`, `/session-heads` | Ler Task/Session persistidos. |

O login entrega cookie opaco. Tokens e credenciais daemon permanecem no servidor. Cada leitura Task exige Origin exato, sessão válida, JWT assinado e decisão PROJECT fresca antes da admissão Records. O contexto — path nativo, projeto e versão — é dado de seleção para os pedidos seguintes; não é grant. Perfil CLI, headers Runtime ou grant serializado não substituem sessão e PROJECT.

O caller browser navega para `authorization_url` retornada pelo login JSON; o callback preserva o redirect para `/`. A identidade é fornecida por `GET /auth/session`, separadamente da lista de providers: provider, subject assinado, username opcional e expires_at. Sessão ausente retorna401 e permite iniciar login com a lista configurada.

`POST /api/tasks/contexts` recebe seleção vazia e nenhum payload de input. A resposta é `{"contexts":[…],"current":…}`, com cada item contendo o caminho Unicode literal em string JSON, ProjectId e versão resolvida que o owner já capturou. Cada item recebe sua própria admissão PROJECT no mesmo Original; negação ou falha em qualquer item interrompe a resposta inteira, sem linhas parciais. Os limites de coleção e bytes vêm da política Task e o deadline cobre a operação inteira. Não é inventário global, descoberta de branches ou importação Hub. O browser conserva o path nativo junto ao ID e à versão para selecionar diretórios distintos com identidade/versão iguais e para invalidar respostas antigas. A operação seguinte autoriza novamente.

Datas, dependências, históricos e cursores vêm do [Task nativo](rust_task_mcp.md), sem reconstrução de registros antigos. As rotas browser são somente leitura. A hospedagem dos artefatos está descrita abaixo. A integração funcional React/Task/referências e o dispatcher Task MCP compartilhado pertencem às suas entregas próprias.

## Cliente React e inteiros nativos

O cliente Task/Session reutiliza um único codec, `api/nativeJson.ts`, com
`lossless-json` pinado em `4.3.1`. Inteiros seguros são `number`; inteiros fora
do intervalo seguro são `bigint`, sem arredondamento. IDs, strings, null e
false conservam seus valores. Caminhos são strings Unicode literais;
não há array de unidades de sistema operacional. O ajuste de dependências Node desta unidade é somente esse pin.

`api/task.ts` lê o texto da resposta pelo codec e serializa requests pelo mesmo
helper; `api/taskSession.ts` reutiliza esse transporte. Cursores opacos são
devolvidos sem alteração, para a mesma seleção e filtro. Requests, cursores,
detalhes JSON e exports conservam inteiros como tokens numéricos JSON, inclusive
`18446744073709551615`, sem substituí-los por strings nem modificar globalmente
BigInt. TaskNativeDTO e SourceBackend não mudam por esta adaptação do cliente.

TaskExplorer e SessionExplorer usam o mesmo helper para detalhes e exports.
Contadores e revisões aparecem literalmente no DOM. `SessionTaskProgress`, em
`components/shared/EngineeringUI.tsx`, calcula apenas o percentual visual
limitado entre 0 e 100; os contadores continuam exatos no label e em
`aria-valuetext`. Um timestamp fora do intervalo de Date conserva seu valor
literal, em vez de mostrar uma data inválida ou arredondada.

`TaskModeToggle` conserva a query `selection` ao alternar entre Task, Gantt e
Session. Ao escolher um contexto, `TaskWorkspaceSelectors` grava na URI a
tupla capturada real e preserva pathname e ID do registro, além dos demais
parâmetros da query. Esse ajuste não altera o algoritmo Gantt ou o CSS.

O store `taskWorkspace` identifica o módulo `task|memory|knowledge`. Task usa
seu inventário; Memory e Knowledge usam seus próprios POSTs
`/api/memory/contexts` e `/api/knowledge/contexts`, com `{input:{}}`.
`current:null` é válido e não seleciona automaticamente o primeiro contexto.
Trocar módulo limpa/aborta o estado anterior, e geração/módulo bloqueiam
respostas e negações Auth tardias. Retry conserva a seleção explícita da URI.
App, header e Refresh dispensam o inventário Task e o projeto global para
Memory/Knowledge; USER e Knowledge importado têm escopos independentes.

Uma prova focal de componentes passou 18 casos (harness 2,21 s): dois novos
App USER com readers mockados, oito TaskWorkspace — um Mode novo e sete
retidos — e oito WorkspaceRefresh existentes. Ela verifica navegação e chrome
local; não prova HTTP, browser ou grant Auth. A falta de prova real nessa
rodada é histórica; os aceites posteriores de ef75/51f2/db07 estão separados
na reconciliação integrada abaixo.

## Referências nativas de Task e Session

`POST /api/tasks/references` compõe a API Task/browser opt-in. A API HTTP de
8e5d foi qualificada no Linux; os resultados do SDK Records 6cf9 são antecedentes
separados. Esta API não qualifica IAB, a jornada completa de 17a7 ou FullNoLoss
entre todos os módulos.

Envie JSON com `selection` e `input` obrigatórios. O Content-Type deve ser
`application/json` ou `application/...+json`. Campos desconhecidos, duplicados,
tipos incorretos, IDs inválidos e coerções são rejeitados antes de PROJECT.

| Campo | Contrato |
| --- | --- |
| `selection.project_dir` | String Unicode literal, sem normalização de espaços, acentos ou separadores. |
| `selection.project_id` | ProjectId Core real. |
| `selection.project_version` | Versão explícita, não vazia e sem NUL. |
| `input.kind` | Exatamente `task` ou `session`. |
| `input.id` | ID nativo válido do domínio escolhido. |
| `input.page_size` | Omitido significa 20; inteiro de 1 a 100. Null é rejeitado. |
| `input.outgoing_cursor`, `input.incoming_cursor` | Cursores SDK opacos; omissão ou null significa None. |

Use a seleção nativa fornecida pelo serviço. Ela não é grant: cada leitura
reutiliza cookie, Origin exato, JWT, PROJECT fresco e admissão ReadTask atuais.
Capture físico, admissão e consulta compartilham o mesmo Original. Uma única
`NativeTaskStore::references` produz as duas páginas no mesmo pin.

O resultado HTTP é `{selection, native, navigation}`. A seleção vem da tupla
realmente admitida e de seu NativePath owner_root, que pode conservar um
separador final; não é eco do request. Ela pode selecionar a próxima leitura.
`native` preserva integralmente o DTO `TaskReferencesRead`: `"no_corpus"` ou
`{"published": ...}` com generation, focus_revision, outgoing, incoming,
coverage e warnings. Cada página mantém results, total, complete e next_cursor;
as páginas boxed do SDK são transparentes no JSON. Namespaces, revisões,
proveniência, ordinais, observações e cursores não são reescritos.

`navigation.outgoing` e `navigation.incoming` são arrays alinhados com os results
correspondentes. Cada item contém `source_href` e `target_href`, serializados
como null quando não resolvidos. NoCorpus mantém ambos os arrays vazios.
Somente Task/Session autorizados e observados nesse pin geram rotas locais com
segmento de ID codificado: `/task/explorer/<id>` ou `/task/sessions/<id>`.
Memory, Knowledge, USER, imports e projetos estrangeiros unresolved preservam
sua identidade, sem href ou consulta adicional de body. Navegação não concede
autoridade para a próxima leitura.

Coverage qualifica fontes Task/Session PROJECT do corpus selecionado.
`inbound_other_sources_not_qualified` explicita as demais fontes pendentes.
Completude da projeção, cobertura das fontes e resolução rica dos targets são
dimensões separadas: outgoing pode ser completo com targets unresolved;
incoming/global não é declarado completo pela ausência de outros produtores.
Warnings e incompletude do SDK não viram listas vazias saudáveis. Persistência,
markers/current heads e recuperação pertencem ao
[contrato Records de relações](record_relations.md).

O corpo respeita `ContentLimits.max_input_bytes` e a árvore JSON respeita os
limites atuais de nós, profundidade, scalar bytes e entradas de coleção antes
de PROJECT/SDK. Os limites de consulta vêm da política atual:

| Limite SDK | Origem |
| --- | --- |
| `read` | TaskPolicy.read. |
| `max_rows` | ContentLimits.max_nodes. |
| `max_edges` | ContentLimits.max_collection_entries. |
| `max_text_bytes` | ContentLimits.max_input_bytes. |

São limites do scan real, incluindo dados consultados além das páginas.
`max_output_bytes` limita separadamente o JSON serializado antes da entrega.
Não há configuração nova, quota cumulativa, ledger ou garantia de RSS.

| Falha | Resposta atual |
| --- | --- |
| Media type, shape, campos, ID ou árvore JSON inválidos | 400, invalid_arguments. |
| Tupla tipada sem correspondência nos contextos retidos | 404, selection_unavailable, antes de PROJECT. |
| Corpo acima do limite HTTP | 413, antes de PROJECT/SDK. |
| Cursor rejeitado pelo SDK | 400, invalid_cursor. |
| Request SDK cancelado/expirado, inclusive Reference::Request | 408, request_cancelled_or_expired. |
| Outra falha de Reference | 500, reference_projection_failure. |
| Foco ausente em corpus publicado | 500, task_not_found/session_not_found causal; não NoCorpus. |
| Limite/invalidez tardia do SDK | 500, invalid_native_input. |
| JSON acima do limite de saída | 500, response_limit, sem prefixo de sucesso. |
| Cookie/JWT inválido, Origin ou PROJECT negado, provider removido | Categorias atuais 401/403. |
| GET na rota ou ausência de opt-in | 405 ou 404, respectivamente. |

Deadline, cancelamento e parada conservam a categoria do ponto em que foram
observados; não são convertidos indiscriminadamente em Request SDK. Se o Original
for cancelado ou expirar durante a captura física PROJECT ou de seu binding, a
falha conserva `408 request_cancelled_or_expired`. Com o Original ainda válido,
a falha de captura continua na categoria de negação existente. Original,
admissão, stop e sessão são rechecados nas fronteiras atuais. Um handler aceito
pode sobreviver ao EOF até seu deadline finito, conservando ownership até join.
A consulta não publica, reconcilia, repara dados ou realiza retry automático.

Onze casos novos passaram, sem casos ignorados: três verificam o decoder e a
navegação; oito usam cookie/JWT assinados, PROJECT TCP, writes nativos e reopen.
Cobrem incoming/outgoing, página default 20 e continuação até 100, duplicados,
unresolved, seleção admitida e cursor de outro diretório com ID/versão iguais,
limites reais, lista vazia após clear, geração obsoleta, EOF com join,
deadline, stop, negação de PROJECT durante a leitura e provider removido.
As quatro regressões Task/browser afetadas passaram; a fixture pública opcional
Broker/PostgreSQL/TLS/Docker existente permaneceu ignorada e não é prova desta
folha. O Clippy atual de App lib passou com `--no-deps -- -D warnings`, assim
como a formatação e os links documentais. O receipt em
`.build/rust-records-references/browser-api/validation.json` preserva as rodadas
falhas anteriores e separa esses resultados dos testes SDK e de UI.

```text
cargo xtask cargo test --offline --locked -p graphit-app --lib references -j1 -- --test-threads=1
cargo xtask cargo test --offline --locked -p graphit-app --lib browser_auth::tests::task -j1 -- --test-threads=1
cargo xtask cargo clippy --offline --locked -p graphit-app --lib -j1 --no-deps -- -D warnings
```

## Hospedagem de artefatos Vite

Com `task.browser` habilitado, a origem literal do Foreground serve o bundle React embutido. Abra `/task/gantt`; `/`, `/task` e `/task/` respondem 307 para esse caminho. O callback OAuth mantém 303 para `/`, seguido desse redirecionamento da entrada.

GET e HEAD servem HTML somente em `/task/gantt`, `/task/explorer`, `/task/explorer/{task_id}`, `/task/sessions` e `/task/sessions/{session_id}`. Os dois últimos caminhos com ID permitem recarregar um dossier diretamente. `/logo.svg` e `/favicon.svg` servem SVG. As respostas usam os bytes gerados pelo Vite, `Cache-Control: no-cache`, `X-Content-Type-Options: nosniff`, `text/html; charset=utf-8` ou `image/svg+xml`.

Um caminho desconhecido, como `/task/missing.js`, `/task/rota-inexistente`, `/task/gantt/extra` ou `/assets/missing.css`, retorna 404 sem HTML da SPA. Métodos diferentes de GET/HEAD retornam 405 nas rotas de assets. Os prefixos `/api`, `/auth` e `/mcp` mantêm seus próprios contratos, incluindo erros, sem fallback da SPA. Sem opt-in, essas rotas UI não são servidas.

Para atualizar o bundle distribuído, execute na raiz do projeto:

```text
node scripts/build-rust-ui.mjs
```

O comando executa o typecheck/build Vite existente e grava `crates/graphit-app/assets/ui/index.html`, `logo.svg` e `favicon.svg`. Esses três artefatos acompanham o source. Cargo os incorpora com `include_bytes!`; o build Rust não executa Node/npm, não consulta a rede e não lê `ui/dist`. O `build.rs` preserva os metadados da marca/versão e acompanha mudanças nos assets. Atualizar os artefatos exige o ambiente Node do frontend; compilar os artefatos já distribuídos exige somente o ambiente Rust e suas dependências.

A hospedagem é o produtor ca81. A funcionalidade Task/Gantt, os leitores/dossiers e as referências nativas permanecem na integração 17a7. Os testes de assets não qualificam essa jornada ou o login pelo navegador real; referências consomem o SDK Task/Session de persistência/query entregue em 6cf9 e a API/caller autorizados de 8e5d.

No frontend Task, o único Refresh genérico fica no header de `WorkspaceRefreshProvider`. Gantt registra seu loader e não duplica esse botão na toolbar; Retry local aparece somente diante de erro causal, enquanto filtros e Load more permanecem. O header aguarda o inventário de contextos e todos os leitores registrados do escopo atual, mantendo Busy até settle e preservando drafts. A seleção usa NativePath, projeto e versão exatos; um objeto equivalente devolvido pelo inventário conserva a seleção vigente. O refresh explícito cancela a carga automática de catálogo ainda agendada, evitando que ela aborte uma leitura aguardada. Trocar o escopo cancela o trabalho anterior. O dossier agenda seu carregamento inicial com um guard do lifetime da página e ignora respostas ou negações tardias após abort/unmount.

O componente `WorkspaceRefreshProvider.tsx` mantém a composição; `WorkspaceRefresh.ts` contém contexto, hooks e a espera agregada. Os botões, seletores, foco, estados e tokens do design system existente permanecem os mesmos. Essa coordenação de leitores é a cleanup 2447 e não qualifica as referências nativas ou a jornada completa de 17a7.

## APIs e limites do owner

O módulo privado `crates/graphit-app/src/foreground_tasks.rs` oferece `ForegroundTaskConfig::capture`, `ForegroundTasks::new`, `router` e `shutdown`. `ForegroundTasks::new` recebe um handle do mesmo TaskServer já anexado ao Source com `attach_tasks` e seu provider live, em vez de criar outro servidor. `router` usa `BrowserAuth::task_router` e mescla o router privado de assets estáticos `foreground_ui::router`. `SourceServer::serve_composed` acrescenta esse router depois de Admin/NativeInstall e mescla `TaskServer::native_router` para `POST /api/tasks/native/{operation}`; não incorpora o standalone `TaskServer::router`, que possui outro `/mcp`. O `/mcp` Source permanece único e despacha as operações do catálogo com o contexto RMCP real. Browser `/api/tasks` mantém seu contrato separado.

O helper interno `task_policy` constrói TaskPolicy com conteúdo, timeout, scope, limites nativos, artifacts, payloads, projeção e binding existentes; a captura browser reutiliza seu conteúdo e timeout. Seu batch explícito é `8192`, correspondente ao default do scanner Lance13. Não deriva de fixture nem de variável Lance incidental. OAuth/JSON/rede usam limites Hub existentes; metadata/key cache TTL é 300s, cooldown de chave desconhecida 1s e conexão idle 30s, conforme a receita SDK de Auth.

O Foreground compartilha stop, provider live dos tuples projeto/versão e DaemonAuth. Browser e Task mantêm seus JoinSets reais. Source possui e aguarda os jobs do TaskServer compartilhado, além de seus drains Admin/NativeInstall, antes do cleanup de discovery/chave Runtime e singleton. `ForegroundTasks::shutdown` encerra somente BrowserAuth; não repete o join Task.

## Falhas e recuperação

| Condição | Resultado e ação |
| --- | --- |
| Objeto inválido, provider ausente/Local, endpoints distintos | Startup causal; corrigir configuração/produtor, sem modo permissivo. |
| Origin incorreto | HTTP403 antes de PROJECT; usar a origem literal publicada. |
| Cookie ou JWT inválido | HTTP401; refazer login. |
| PROJECT negado ou provider removido | HTTP403; verificar autorização/configuração atuais pelo produtor responsável. |
| Envelope, corpo ou limite inválido | Resposta do SDK; corrigir input/limite, sem repetir como grant. |
| Disconnect, deadline ou stop | Original finito e cancelável; trabalho iniciado continua possuído até join local. Hyper pode reter um handler aceito após EOF até seu deadline. |
| Falha de cleanup/join | `ForegroundFailure` conserva causa/owner/outcome; caller deve executar cleanup explícito no owner retido. |

Não há rollback de efeitos Core ou retry automático. Cancelar um waiter de shutdown de componente conserva seus trabalhos possuídos; essa garantia não qualifica cancelamento arbitrário do future consumidor de todo o host nem promete terminar trabalho remoto no Broker.

## Qualificação

Os resultados de 0a1a abaixo são antecedentes da composição. A extensão 744f qualificou a resposta JSON200 de login e a lista `/api/tasks/contexts`: cinco testes novos, quatorze regressões Browser locais afetadas e um consumidor Foreground compilado passaram. Os novos casos usam três raízes nativas realmente confiáveis desde o startup Auth, inclusive dois diretórios com o mesmo ID/versão, PROJECT fresco por item, erro integral no segundo projeto negado, limites, deadline/parada e Task persistido/reaberto. Os vinte casos e as falhas anteriores estão em `.build/rust-foreground-react/`. O receipt público Broker anterior foi preservado separadamente; a integração funcional React permanece em 17a7 e a hospedagem dos artefatos é qualificada separadamente em ca81. Após a rechecagem final de Original/Task.stop no agregado, os cinco testes específicos passaram novamente e o Clippy do App lib/teste Foreground terminou sem avisos Rust com `--no-deps -- -D warnings` (`api-strict02.log`).

A hospedagem ca81 executou o build Vite atual e distribuiu os três outputs com bytes iguais (`hosting-vite02.log`, `hosting-assets.json`). Os dois testes de assets passaram em 0,05s, cobrindo GET/HEAD, bytes/MIME/cache/nosniff, rotas finitas e erros, e compilando somente as declarações reais de `include_bytes!` em árvore isolada sem `dist` e com PATH vazio. Essa fixture não executa `build.rs` nem demonstra um checkout Cargo completo sem Node; a compilação App real e a inspeção do build Rust são evidências distintas. Três regressões Foreground afetadas passaram em 4,83s: configuração ausente/inválida e cleanup, HTML no CLI compilado com leituras nativas/Source e SIGINT/SIGTERM, e deadline preservando dados nativos. O filtro `foreground_` não executou o probe interativo. Receipts e falhas de compilação anteriores estão em `.build/rust-foreground-react/hosting-receipts.json`. O Clippy de App lib/teste Foreground passou com `--no-deps -- -D warnings` (`hosting-strict03.log`), após a correção do Core e a liberação coordenada de espaço em disco. A jornada IAB, Task UI e referências obrigatórias de 17a7 não são qualificadas por esses cinco testes.

A cleanup React 2447 aprovou 84 testes distintos de componentes em onze suites: 44 casos atuais de Task/Auth/header e 40 de consumidores globais do Provider preservados. Os 27 casos anteriores de Task/Auth permanecem cobertos. As seis regressões novas verificam NativePath igual/diferente, negação após unmount, refresh nativo sem Hub, mudança de escopo durante o inventário e catálogo pendente por mais de 200ms com seleção clonada, antes e depois da carga inicial. Os dois casos de clone/timer falharam antes da correção. TypeScript passou após os ajustes, e o lint estrito dos nove arquivos inteiramente owned terminou com zero problemas. O lint global normal terminou com zero erros e os cem avisos existentes de outros arquivos; os cinco consumidores com mudança somente de import mantêm seus avisos prévios. Receipts e falhas estão em `.build/rust-foreground-react/ui-cleanup-receipts.json`. São provas de componentes, com APIs mockadas, e não de SDK de referências, novo bundle distribuído ou navegador real. Essa exigência histórica de regeneração dos três assets tem o resultado atual de 17a7 descrito a seguir.

O histórico de `tsk-17a7` revisão 39 registra a jornada hosted observada por Root CUA: OAuth, calendário civil com leap-day de 2028, dossier Task, Session, seleção de contexto, reload, badge a 720px e cleanup. Os antigos recibos locais `17a7-root-browser-observations.json` e `17a7-badge-candidate.json` não estão disponíveis neste checkout; as observações continuam históricas na Task. Na revisão 42, antes do delta de header único, Source/Tracker foi NOFAULT, 58 testes React em seis arquivos passaram (harness 7,78s, wall 8,133s), o helper real de build passou em 13,145s com três assets distribuídos byte-equal, e lint de seis arquivos passou em 10,686s. Host01 locked/offline falhou antes do harness em 0,062s, por `graphrs =0.12.0` do manifest Knowledge ausente no Lock/cache. Root resolveu esse pin, mas host02 parou em seis erros externos de compilação Records antes do harness (wall 15,908s); nenhuma dessas tentativas prova uma jornada browser atual. O `.build/rust-task-gantt-owner/qualification.json` será atualizado com escopo parcial. A [verificação Gantt](rust_task_gantt.md#verification) separa esses antecedentes do aceite posterior de referências ef75/51f2. O aceite integrado WEB-A6/T5 pertence à Task17a7; esses antecedentes não qualificam Auth comercial ou Windows/macOS.

O delta44 de Refresh único recebeu readiness Source/Tracker NOFAULT. Os 15 casos focais Gantt/Explorer passaram (harness 7,13s, wall 7,757s). O novo caso Gantt monta os providers reais do header e usa teclado, NativePath clonado e catálogo deferred; verifica que o timer de 260ms não aborta o read, Busy permanece até settle e o calendário atualiza de três dias inclusivos para um. Sua API Task é mockada, portanto não é prova browser. O inventário anterior de 58 casos ganha somente esse caso: 59 distintos, sem somar reruns. Buildrefresh01 falhou por TS2769 no próprio teste; a query foi corrigida para `name: /^Refresh$/`. Buildrefresh02 passou em 10,561s, os cinco Gantt final02 passaram (harness 4,47s, wall 4,772s), e lint dos dois arquivos afetados passou em 0,754s.

Host03 passou uma jornada atual do host compilado com IAB, zero ignored: build 44,53s, harness 376,71s e wall 421,400s incluindo hold interativo. O browser iniciou anônimo, autenticou via Broker local signed OAuth e leu o NativePath/main exato com Task persistido/reaberto: dois Tasks, intervalo inclusivo 28/02→01/03 com leap-day 29/02, due 02/03, undated e dependências reais. Teclado abriu dossier/checks/lifecycle e request/strategy da Session com dois Tasks ligados; trocar PROJECT removeu o conteúdo anterior, mostrou vazio e permitiu restaurar o contexto e fazer deep reload do selector Session exato. O Refresh único ficou busy/disabled com Loading tasks e voltou habilitado com as datas. Em 720×900, o overflow do calendário permaneceu local e header/filtros ficaram usáveis; Escape restaurou foco em Project. O browser restaurou viewport, fez logout anônimo sem linhas Task anteriores e fechou a tab. Screenshots default/narrow foram observados em CUA, sem artefato de arquivo salvo.

As observações históricas `.build/rust-task-gantt-owner/browser-observations.json` e o resultado host03 `.build/rust-task-gantt-owner/browser03.result.json` registraram finished/host_joined/discovery_clean verdadeiros, UserInfo0 e 48 chamadas PROJECT. Esses arquivos efêmeros foram removidos na limpeza pontual de `.build`; a evidência qualificada permanece registrada em `tsk-17a7`, sem replay ou recriação dos recibos. O inventário Gantt histórico tem 62 casos distintos: 59 React, essa única jornada hosted e dois unitstatic, com somente um teste adicionado ao inventário React anterior e sem somar reruns. Staticunit02 passou dois casos, zero ignored (harness 0,05s, wall 56,299s). Static01 falhou antes do harness com sete E0308 externos em tuples Role (wall 369,968s incluindo fila); a correção externa precedeu staticunit02. Os links de referências retornaram HTTP404 visível naquele snapshot; esse fato histórico não é apagado pelo aceite posterior de ef75/51f2. A prova independente UI não qualificava FullRefs e não conclui 17a7, Auth comercial ou Windows/macOS. Host01/host02 e buildrefresh01 permanecem históricos, sem converter as falhas pré-harness em provas browser.

Strict03 passou a verificação atual App lib/integração `foreground_tasks`, com `--locked --offline --no-deps -j4 -- -D warnings`, em 17,803s wall, sem harness de testes. O mesmo inventário histórico tem 62 casos distintos, zero ignored e um teste novo; ele é reutilizado, sem nova execução ou soma de reruns. Strict01 permanece histórico: chamada externa de constructor AST com oito argumentos (wall 14,503s); strict02 falhou em Records TaskRefs externo por budget indefinido e argumentos extras em `references.rs:597/1188` (wall 77,724s). As correções AST/Role/Records precederam strict03 e foram feitas pelos respectivos owners, sem alterações desses módulos ou allowances por esta unidade. Os 16 logs de tentativas, incluindo seis falhas históricas, permanecem no recibo; falhas e reruns não acrescentam testes. A mudança UI owned continua restrita a um hunk Gantt, um teste e três assets distribuídos. A entrega de referências posterior é aceita separadamente; WEB-A6/T5 pertence ao aceite integrado registrado em Task17a7.

## Reconciliação integrada atual de 17a7

`tsk-17a7` reúne o aceite integrado e a correção de Session back; os
resultados e limites qualificados estão descritos abaixo. As referências
próprias já estão qualificadas:

| Entrega | Aceite e limite da prova |
| --- | --- |
| Backend ef75 | COMPLETED65, 15 checks PASS; 32 testes distintos em 33 execuções. Não se somam reruns ou suites dos produtores. |
| React referências 51f2 | COMPLETED79, 15 checks PASS; 60 unidades com transporte mockado e cobertura Browser combinada aceita: main10 com 19 etapas PASS, focal12 com 4 PASS narrow/logout e expiry02 com 3 PASS de sessão real de 30 s. main10 terminou failed; as falhas 10/11 permanecem, sem alegação de uma execução integral PASS. |
| Readers db07 | COMPLETED187, 28 checks PASS; qualificação por camadas, com limites explícitos para NoCorpus e HEADS. Não conclui outras entregas. |

See the [reference contract](rust_foreground_record_references.md).

A lacuna concreta encontrada foi o botão Sessions remover a seleção física.
SessionExplorerPage.tsx:71–75/118 agora usa catalogueHref derivado da seleção
validada tanto ao voltar quanto na troca de escopo. O caso em
SessionExplorerPage.test.tsx:86–120 monta o TaskWorkspaceBootstrap real com
transporte mockado: dois NativePaths de mesmo ID/versão, um com byte255,
Session→Sessions→URI literal→workspace novo/remount→heads/detail Mirror e
troca física explícita. O RED anterior observou selection null; a rodada final
passou os sete casos Session em 3,84 s (testes 1,244 s), com ESLint scoped 0.
Essa é prova unitária; não é um novo Browser. A correção não altera CSS,
design, tokens ou backend.

O build final autorizado `node scripts/build-rust-ui.mjs` passou `tsc -b`,
Vite com 3.793 módulos em 4,89 s e cópia dos três assets. O index atual tem
2.612.971 bytes, SHA256
`ea32c572488c42a79f9e4d7b87433ddaa4f8ac26aa6d748ac061c2e53c3b319a`.
Logo e favicon têm 249 bytes cada, SHA256
`537271954e96afaff8307be5c272579e88f45ecc0ac11c7199b87d30e9cd1233`.
Os três assets são byte-equal a dist. Isso qualifica TypeScript, build e bytes
da correção Session; não declara uma nova execução HTTP, um binário Rust
recompilado com esses assets ou um novo Browser. A mecânica de hospedagem
já qualificada é reutilizada. O index db07 de 2.612.921 bytes/SHA9678 permanece
antecedente histórico. A Task17a7 mantém a autoridade sobre seus 13 checks
e lifecycle; esta qualificação por camadas não conclui a validação ponta a ponta,
ses-c98a ou outras agrupadoras.

Os novos testes focados estão em `src/foreground_tasks/tests.rs` e `tests/foreground_tasks.rs`. Exercitam config global/Environment, startup ausente/inválido, signed OAuth/PROJECT TCP, Records persistido e reaberto, Source sibling e CLI compilado com SIGINT/SIGTERM. Receipts e falhas ficam em `.build/rust-foreground-tasks/`. A folha 0a1a registrou 4 testes de configuração e 4 integrações reais, todos PASS, incluindo EOF/deadline, SIGINT/SIGTERM, corpo413, negações Origin/cookie/PROJECT e preservação de AuthDocument/chave/discovery durante serviço. A preparação Records usa admissão Write real, claims com fence sensível e corpus reaberto; resultados browser não expõem esse fence.

```text
cargo xtask cargo test --offline --locked -p graphit-app --lib foreground_tasks::tests -j1
cargo xtask cargo test --offline --locked -p graphit-app --test integration -j1 -- foreground_tasks::
cargo xtask cargo clippy --offline --locked -p graphit-app --lib --test integration -j1 --no-deps -- -D warnings
```

As regressões SDK focais de B2 (rotating refresh, EOF com Original finito e drain) e Source (cancelamento, sibling e interrupção/reentrada do waiter) passaram, uma por filtro. O clippy estrito de lib/integração também passou. O waiter interrompido foi exercitado no componente Source; isso não é prova de cancelamento arbitrário do host consumidor. Prova Linux não certifica Windows/macOS, frontend React, nove hosts ou validação ponta a ponta. Veja também [sessões browser](rust_browser_auth.md), [Task nativo](rust_task_mcp.md), [Gantt](rust_task_gantt.md) e [Foreground](rust_source_foreground.md).

## Leituras Memory PROJECT

O mesmo componente browser compõe as [leituras nativas Memory PROJECT](rust_foreground_memory.md). Essas operações exigem autorização Memory/read própria antes de acessar o corpus; Task/read não concede esse acesso.

## Leituras Knowledge PROJECT

O browser também compõe [heads e página Knowledge PROJECT](rust_foreground_knowledge.md), com seleção nativa exata e autorização Knowledge/read própria. Heads projeta metadata e page lê apenas o body selecionado, validando a geração até EOF antes da entrega.

## Referências e discovery por módulo

A rota `/api/records/references` compõe os DTOs reais de cada fonte com sua authority própria. Memory/Knowledge possuem discovery por módulo; não usam a decisão Task como grant. O [contrato de referências nativas](rust_foreground_record_references.md) conserva as páginas/cursors do SDK e readers POST operacionais. Navegação rica e SPA com seleção exata têm sua qualificação própria; não substituir silenciosamente o workspace. As rotas Task existentes mantêm seu contrato.

A composição trusted captura sempre o TaskPolicy nativo no Source e injeta diretamente seu Registry global existente e os limites nativos, mesmo sem configuração browser. Quando configurado, o browser reutiliza esse mesmo TaskServer. `with_web_imported_knowledge_read` é aplicado somente aos providers `task.browser.providers` capturados pelas camadas de servidor. Sem Registry, um TaskServer standalone informa indisponibilidade causal do leitor importado.

## Armazenamento selecionado

O issuer compartilhado resolve o backend a partir da admissão viva de cada operação.
Memory/Task, seus índices e jobs usam a tabela Lance local ou S3 selecionada pelo provider,
com branches por versão PROJECT; USER Memory mantém seu proprietário verificado e a instalação,
sem branch Git. O Broker autoriza e emite STSv3; os dados trafegam diretamente com S3.
Falhas remotas não provocam escrita local. Novas operações capturam o head atual do SDK,
preservando CAS, histórico e snapshots já abertos. Contrato, configuração e layout em
[S3 STS storage](../guides/s3-and-ui-network.md#memory-and-task-tables-and-versions).
