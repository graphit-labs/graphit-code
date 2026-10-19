---

title: Referências nativas no browser Foreground Rust
doc_type: spec
description: Fontes independentes, autoridade por módulo e continuação nativa das relações entre registros.
references:
  - target: {type: task, id: tsk-ef75}
    relation: implements
  - target: {type: task, id: tsk-185c}
    relation: uses_api
  - target: {type: task, id: tsk-9a23}
    relation: uses_api
  - target: {type: task, id: tsk-fa7b}
    relation: uses_api
  - target: {type: task, id: tsk-12df}
    relation: specified_by
  - target: {type: task, id: tsk-7fc2}
    relation: specified_by
  - target: {type: task, id: tsk-17a7}
    relation: requires_contract
  - target: {type: task, id: tsk-51f2}
    relation: specified_by
  - target: {type: task, id: tsk-db07}
    relation: supports
  - target: {type: task, id: tsk-69f2}
    relation: uses_api
---

# Referências nativas no browser Foreground Rust

Current path contract: PROJECT roots and relative paths cross JSON/API/CLI as literal Unicode strings. Serde preserves spaces, accents, escapes and separators without lossy conversion. Non-Unicode input fails explicitly; `{os,units}` is rejected. Historical evidence below describes the code that was exercised then and does not qualify the removed path codec.

Contrato para integradores do listener Foreground e consumidores das relações
entre Task, Session, Memory e Knowledge. O backend de `tsk-ef75` foi qualificado
no Linux com as suites focais de referências, encoder e página importada manual,
além do strict App. O aceite do backend não substitui a jornada browser de
`tsk-17a7` e seus consumidores UI.

## Consultar relações

`POST /api/records/references` usa JSON, a sessão opaca do
[browser autenticado](rust_browser_auth.md) e Origin exato. Não aceita um bearer,
provider ou grant escolhido pelo corpo. A seleção PROJECT contém o `NativePath`,
ID e versão literais retornados pelo discovery do módulo.

Exemplo ilustrativo: o ID de alvo incoming é identidade lógica, inclusive quando
não existe um registro nativo correspondente. Use uma seleção real do serviço.

```json
{
  "selection": {
    "project_dir": "/projetos espaço/café Ω",
    "project_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    "project_version": "main"
  },
  "input": {
    "focus": {
      "kind": "memory",
      "scope": {"project": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
      "id": "literal-memory"
    },
    "pages": [{"source": {"kind": "project_task"}, "direction": "incoming"}]
  }
}
```

`focus.kind` aceita `task`, `session`, `memory` e `knowledge`. Scope é
`{project: ProjectId}` ou `{user: identidade lógica literal}`. Somente Knowledge
aceita `context`. Esses campos descrevem o nó e não autorizam acesso ao alvo.
Outgoing exige a identidade do registro na fonte física selecionada; incoming mantém o ID
lógico sem transformá-lo em lookup do alvo.

`page_size` é um inteiro de 1 a 100, default 20. `include_user` tem default false;
false não captura USER. `contexts` tem default `[]` e contém seletores importados
exatos `{id, version, context_alias?}`. SourceKey é `{kind: project_task}`,
`project_memory`, `project_knowledge`, `user_memory` ou
`{kind: imported_knowledge, artifact: seletor}`. Alias do mount não substitui o
namespace persistido da referência. No canal `project_knowledge` atual, publisher
e reader SDK validam a fonte como PROJECT da admission e `context=None`.
O reader importado conserva o namespace do payload, independente do produtor
registrado e do alias; pode observar Project/User e contexto persistido. Use
`outgoing_source` explícito para fixar a fonte importada. A authority pertence à
fonte física: ReadKnowledge PROJECT na seleção ou admission de import da
instalação. Scope User numa projeção importada não captura memória USER.

Sem `pages`, o universo incoming contém os três módulos PROJECT, USER quando
habilitado e os imports exatos solicitados. Outgoing usa o owner correspondente
ao foco; `outgoing_source` fixa esse owner quando necessário. Com `pages`, somente
as fontes/direções descritas são consultadas. A seleção PROJECT pode ser omitida
em consultas explicitamente USER-only/import-only; elas não admitem nem capturam
PROJECT. Outgoing USER exige `include_user:true` e o scope literal da memória do
próprio caller, observado pelo leitor USER.

Campos desconhecidos, duplicados, scalar null presente, texto vazio/NUL,
tipos incorretos, fonte/direção duplicada e limites físicos excedidos falham.
Os null internos dos cursores continuam pertencendo ao contrato do SDK.

## Fontes, páginas e recuperação

A resposta contém `selection` quando PROJECT foi efetivamente admitido,
`sources`, `outgoing_complete`, `incoming_complete` e `warnings`. Cada fonte
mantém seu `native` completo e as observações de navegação alinhadas às arestas.
Enums nativos de referências usam a representação do produtor, por exemplo
`"no_corpus"` ou `{published: ...}`; não são o envelope `state` dos leitores de
página/catálogo. Os outputs Data/Knowledge recebem somente `Serialize`; não são
admissions, readers, snapshots ou objetos de authority serializados.

Task focal devolve ambas as páginas do mesmo pin, mesmo quando o request pede
uma só direção. Backlinks Task genéricos têm sua própria família de cursor.
Memory e Knowledge deixam a direção não solicitada em `None`. Preserve todos os
campos: geração, namespace, payload authored/wiki-link, provenance, ordinal,
revisões, witness, coverage, warnings e cursor. Não transforme inteiros `u64` em
valores de ponto flutuante.

`complete` de uma página nativa descreve a integridade da projeção; `next_cursor`
descreve continuidade. Pode haver `complete:true` com mais páginas. Os indicadores
agregados exigem integridade e EOF das direções/fontes solicitadas, sem afirmar
uma transação ou completude do catálogo global. Fontes omitidas não viram corpus
vazios. `NoCorpus`, Registry ausente, páginas publicadas sem matches,
`MissingCurrentProjection`, direção não solicitada e falha causal são estados
distintos. Zero matches não prova corpus fisicamente vazio.

Copie o cursor do SDK para `pages[].cursor` da fonte/direção correspondente e
repita sua seleção e shape de query. Se a fonte foi consultada em ambas as
direções, mantenha ambas no request de continuação; omitir uma muda o binding.
Mudar path, versão, foco, namespace, fonte, destino de armazenamento ou geração não rebaseia o cursor.
Memory/Task vinculam a continuação à identidade do endpoint e da tabela efetivos;
renovar credenciais sem trocar esse destino não muda a identidade do corpus.
Continuidade rejeitada exige iniciar uma nova leitura explicitamente. Não há
retry, refresh de pin, cursor global ou fallback automático. Um import removido
com cursor existente produz erro de continuidade, não ausência saudável.

Cada fonte PROJECT exige sua própria decisão fresca Task/Memory/Knowledge do
Broker. USER exige ReadUser e ReadMemory da identidade Web atual. Imports exigem
a authority Web explicitamente configurada na instalação. Uma falha de fonte
solicitada aborta o request sem entregar o prefixo saudável. Negação/ausência de
um alvo rico opcional mantém `target_rich:null`; falhas de infraestrutura e
cancelamento continuam causais.

## Discovery e leitores operacionais

`POST /api/memory/contexts` e `/api/knowledge/contexts` recebem `{input:{}}` e
usam o módulo fixado pela rota. Retornam `{contexts, current}`: apenas tuplas
ativas e admitidas. O default operacional usa o contexto configurado enquanto
ativo; quando retirado do daemon, passa ao primeiro contexto ainda ativo. Se esse
default ativo for negado por Auth, `current` é null e a descoberta não escolhe
outro projeto por sua autorização. Se não houver contexto ativo, retorna
`contexts:[]` e `current:null`. Task negado não bloqueia Memory/Knowledge
permitidos. As rotas Task mantêm seu próprio contrato.

O limite de coleção é aplicado aos contextos ativos e autorizados da resposta,
sem rejeitar o inventário bruto antes dos filtros. Uma resposta acima desse
limite falha com `response_limit`; não retorna uma lista truncada. O limite
físico de bytes e os guards de Original continuam valendo.

Membership do daemon e authority são condições independentes. Os consumidores
capturam um `Arc<LiveProjectSnapshot>` fresco por request e emprestam seu roster
e current até concluir o encoding. Inclusões e trocas de current posteriores ao
startup são observadas na próxima chamada. O snapshot operacional não concede
authority. Seleção explícita ausente nesse snapshot falha com HTTP 400 e
`selection_unavailable`, antes da captura e do Broker. Tuplas capturadas exigem
`LiveProjectSelections::is_active` antes da captura e antes/depois do encoding;
retirada concorrente retorna `inactive_project_selection` sem prefixo saudável.
As fixtures HTTP refs12 qualificaram retirada antes da chamada, durante a
captura e o encoding, além de inclusão/troca de current após startup.

`POST /api/knowledge/imported/contexts`, disponível com o SDK importado Linux,
recebe `{input:{}}` sem PROJECT. Lista seletores Knowledge registrados
GlobalNative/Native/Original da mesma instalação. Não instala, hidrata, repara,
renomeia alias ou revela caminhos do Registry.

`POST /api/knowledge/imported/page` recebe
`{input:{source: SourceKey importado, slug: literal}}`. A resposta é
`{source, focused_source, native}`. `native.state` distingue `registry_absent` de
`published {generation, page}`. A view de página preserva `published_empty`,
`page_absent` e `found {document}` com os 16 campos do
[leitor Knowledge](rust_foreground_knowledge.md), digests e nanos. Foreground
injeta o Registry global existente e os limites nativos capturados; um
TaskServer standalone sem esse recurso retorna indisponibilidade causal.

A página manual Found observa `focused_source` por `reader.references` outgoing
com page_size 1, no mesmo reader/pin/Original da página. Esse DTO do SDK fica
fora dos 16 campos e preserva `slug`, `title`, `source_path`,
`compilation_digest`, `namespace` e `witness`. A identidade lógica vem de
`namespace.scope` e `namespace.context`; produtor registrado, alias e corpo do
documento não os substituem. Zero arestas ainda pode trazer namespace e witness
validados. Projeção ausente conserva metadata com namespace/witness null;
fonte desconhecida traz focused_source null. Não invente um foco lógico nesses
casos. PublishedEmpty/PageAbsent/RegistryAbsent mantêm focused_source null e
seus próprios estados. Falhas de integridade, infraestrutura ou cancelamento
continuam causais, sem novo pin ou fallback. A navegação pode carregar um
`Focus` esperado como dado e compará-lo com o observado, sem conceder authority.

O leitor PROJECT `/api/knowledge/page` agora fornece o mesmo binding observacional,
com `source:{kind:"project_knowledge"}` e seleção NativePath/ID/versão admitida.
Os dois readers nativos da página e de suas referências precisam observar a
mesma `KnowledgeGeneration` desse corpus; divergência retorna HTTP409
`knowledge_generation_changed`, sem reabrir pin ou entregar prefixo saudável.
Esse requisito é local ao mesmo recurso Knowledge, sem comparar gerações entre
módulos. O serializer borrowed é compartilhado com a página importada, que
continua omitindo seleção PROJECT e usando seu reader/pin retido. Namespace e
witness null permanecem incompletos, nunca inferidos de seleção ou mount.

Navegação rica contém o título realmente observado e um `reader` com método
POST, rota, input fechado, seleção PROJECT exata quando aplicável, SourceKey,
foco e disponibilidade. Esse descriptor pode ser executado no mesmo serviço.
`href` permanece null até a jornada SPA com essa seleção/namespace ser
qualificada. Não fabrique título para alvo ausente nem substitua o contexto ao
navegar. Knowledge valida a identidade lógica do foco outgoing pelo `focused_source` do
SDK, inclusive para páginas sem arestas. O namespace persistido fornece scope e
context; alias de mount e identidade do produtor não os substituem. Namespace
divergente falha com `invalid_knowledge_namespace` ou `invalid_import_namespace`.
Projeção sem namespace mantém a incompletude nativa e a resolução rica da fonte indisponível.

Um alvo Knowledge procura candidatos com slug, scope e context exatamente iguais
nas fontes solicitadas e admitidas. Candidatos físicos distintos tornam
`target_rich:null`; aliases do mesmo id/version importado contam uma única vez.
O leitor de uma fonte permanece preso àquela fonte. Metadata e página importadas
usam o reader retido; no corpus nativo, gerações divergentes entre metadata e
página deixam o alvo indisponível, sem retry ou atualização implícita. A fixture
HTTP qualificou ambiguidade, aliases e namespace; a divergência concorrente de
gerações foi verificada no guard, sem fixture determinística de corrida. O produto PROJECT atual não publica
fonte com contexto `Some`; o backend importado conserva contextos de payloads
que um produtor realmente publique. A prova funcional atual usa B canônico
registrado sob produtor A e alias presente distinto do contexto persistido None.
As fixtures SDK do reader não constituem um novo publisher App, e esta
investigação não modifica batches físicos.

Admissions, capturas e readers importados permanecem retidos, com recheck antes
e depois do encoding. Um único writer empresta os DTOs e verifica bytes físicos,
Original, deadline e parada compartilhada. Cancelamento durante serialização
encerra o writer sem repetir `Interrupted`: preserva HTTP 408 para Original e
503 para stop. Não entrega bytes parciais. O frame resultante é movido ao HTTP,
e os jobs aceitos continuam no JoinSet antes de teardown Auth.

## Consumidor React — cobertura funcional qualificada em Linux

`tsk-51f2` implementa `api/nativeRecordReferences.ts` como cliente POST fino
por `nativeRecordRead`, codec lossless, AbortSignal e conferência da seleção
devolvida. O widget compartilhado removeu o GET `/api/references`; não mantém
cliente dual. Os enums SDK untagged e o payload original são preservados.
Scope HTTP usa `{project:...}|{user:...}`; o CamelCase do SDK Memory é convertido
somente na projeção. Páginas e cursores opacos permanecem por fonte/direção,
com inteiros literais, warnings, NoCorpus, publicado vazio, projeção ausente,
erro, cancelamento e resposta antiga distintos. Uma direção solicitada nula
continua parcial/indisponível, inclusive nos agregados de completude.

A política aprovada exige fonte focal física explícita em `pages`, outgoing e
incoming, com fontes adicionais opt-in. PROJECT conserva os controles do
próprio módulo e NativePath/ID/versão capturados; import-only não inventa
PROJECT. USER começa desabilitado e exige opt-in com scope próprio observado;
imports vêm somente do inventário registrado. `RecordReferences.tsx` agora
inicia `includeProject=false`: outras fontes PROJECT entram somente pelo
controle "Include other project sources". A inclusão automática anterior foi
corrigida. O novo caso de request padrão, opt-in/out e abort de resposta antiga
passou na rodada de quatro suites. Para uma fonte focal imported_knowledge,
contexts inclui seu artifact realmente observado junto dos imports opt-in,
deduplicados por SourceKey. Isso não acrescenta seleção PROJECT. O backend
existente rejeita request sem esse contexto com400; não foi alterado.

Task/Session usam seleção capturada e IDs literais; Memory PROJECT/USER usa os
leitores atuais e de histórico. Page PROJECT/importada J9 conserva
`source` físico e `focused_source` genuíno: o foco usa slug e namespace
(scope/context), e o witness permanece observação SDK. Heads com foco nulo ou
projeção sem namespace/witness não inventam binding. O reader antigo de Context
sem esse binding declara indisponibilidade. Alias e identidade do produtor não
substituem namespace observado. O SDK Task pode devolver ambas as páginas,
mas estado e continuação respeitam as direções pedidas; erro de fonte não vira
vazio ou prefixo saudável.

Um Reader available com href nulo pode fornecer locator: o mapping fechado
aceita somente POST/path/input coerentes para rotas nativas existentes. A URI
conserva seleção, SourceKey e foco esperado; o destino readmite acesso e
confere o binding no reload. Reader não é grant. Reader nulo, path desconhecido
ou input incoerente permanece sem link; input importado inválido retorna nulo,
sem fabricar destino. Outgoing usa `target_rich.reader`; incoming usa
`source_reader` alinhado às arestas.

A rodada final de quatro suites passou 23 unidades (cliente quatro, navegação
sete, view três e widget nove; 1,78 s). Os dois novos casos de navegação cobrem
Task/Session/Memory PROJECT/USER e recusam locators incoerentes. As rodadas
anteriores de 21 e 20 unidades permanecem históricas, sem somar reruns.
São mocks/unidades de transporte, não IAB nem os 14
testes HTTP do produtor. Os dois achados de direção solicitada nula e input
importado inválido foram corrigidos e tiveram casos focais aprovados.
Os callers passaram 33 casos em 7,12 s: TaskExplorer 11, Session seis, Memory
sete, WikiNative quatro e Imported cinco. A rodada anterior teve 13 PASS,
três FAIL e um SKIP por fixtures sem campos J9; as fixtures foram corrigidas.
TaskBrowserNative opt-in permanece SKIP sem Origin/cookie, sem prova IAB.
O último typecheck saiu com código zero. O scoped lint passou com zero erros
e seis warnings no trecho legado ContextKnowledgeExplorer, sem warnings nos
adapters, widget, navegação ou novos callers nativos. As falhas anteriores de
props/fixtures J9, Object.hasOwn e narrowing de NativeRead foram corrigidas
sem alterar tsconfig ou pins. O build da rodada de referências
`node scripts/build-rust-ui.mjs` passou com typecheck e Vite (3.791 módulos;
5,18 s Vite); os três assets foram byte-equal a dist, com index de
2.620.645 bytes. Essa prova é histórica frente ao shipping conjunto UID/Source
posterior, registrado separadamente no recibo; nenhuma delas substitui a
jornada browser. A rodada anterior de
23 casos teve 22 PASS e um FAIL em 16,30 s por expectativa de teste que omitia
o segmento ID no pathname; a correção foi somente no teste, sem mudança de
Source produtivo ou novo build. O typecheck final saiu com código zero.
Naquela rodada, com os 33 callers, a união histórica foi de56 casos UI/mocks
distintos. Os quatro novos casos descritos ao final elevam a união atual a60,
sem somar reruns.
Hosting/IAB AST do Root não demonstra a jornada Task → Memory/Knowledge/USER/
imports desta folha nem atribui PASS a UT2 ou `tsk-17a7` A6/T5.
O Hosting01 histórico falhou antes do harness por sete erros externos de lifetime de
ProgressSink/Conversation; nenhuma prova de Origin ou IAB foi executada.
O recibo privado é `.build/rust-records-references/ui-fullnoloss/qualification.json`.
`tsk-17a7` A6/T5 permanece com o Root; a cobertura funcional FullNoLossRefs
desta folha foi aceita como conjunto de provas ao final.

O novo caso test-only `hosting::hosted_record_references_signed_oauth_and_browser_probe`
está implementado no mesmo target `foreground_tasks`, com preflight HTTP/SDK
aprovado abaixo; a cobertura browser combinada foi aceita pelo coordenador.
A folha tsk-51f2 foi concluída em COMPLETED79, com15/15 checks ativos PASS.
Reusa Broker, Seed, Prepared, Foreground e login/cookie/probe existentes.
`with_record_sources` habilita Memory/Knowledge apenas nesse modo, mantendo
o caso AST/Task negado e o default de metadata `mcp_resources:[]`. A preparação
usa SDK para criação/revisão/história de Memory e referências Task/Session.
Knowledge de A é publicado com seu próprio namespace/admissão. O corpus B
é publicado sob paths/versão/admissão B e ProjectWriteOperation Knowledge
real; somente depois é copiado para o import com produtor físico A/Registry.
Isso preserva namespace lógico B, distinto do produtor físico A, incluindo
zero-edge e referências tipadas, sem permitir publicação nativa estrangeira.
USER é verificado por issuer/sub com scope nativo de escrita; Registry e
reabertura dos mesmos projetos na instalação também fazem parte da preparação.
Copia o corpus realmente
publicado para o cache de fixture; não inventa corpus ou grant por DTO.

O preflight consulta os cinco corpora, cursor incoming opaco e negação
Memory403 causal, sem prefixo saudável. O fixture mantém conclusão `.done`,
teto de 900 s, cancelamento/join/cleanup e verificação UserInfo0; os resultados
executados estão separados abaixo da definição desses limites. Rustfmt dos
dois arquivos passou. O comando iniciado foi
`cargo test --locked --offline -p graphit-app --test foreground_tasks -j4 hosting::hosted_record_references_signed_oauth_and_browser_probe -- --exact`.
Root pediu interromper a compilação no target padrão para evitar target frio
duplicado/pressão de espaço; Ctrl-C terminou com exit130, ainda em dependências,
sem teste iniciado. Esse histórico não é PASS. As tentativas retomadas usam
`CARGO_TARGET_DIR=.build/rust-records-data/knowledge-index-source/target`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` e
`CARGO_INCREMENTAL=0`, mantendo `--offline --locked -j4` e o caso exato.
A espera inicial pelo import Permission em `graph_server.rs:953/968` é
histórica: o evento SourceGraph coerente chegou. A tentativa 01 no target
reutilizado terminou101 antes dos testes por imports errados de
MemoryAttributes/Class, corrigidos para `graphit_records::memory_attributes`.
Seu log conserva os chunks emitidos, com truncamento de um trecho de warning
ANTLR; não é alegado como stdout completo.

A tentativa 02 compilou e executou um teste que falhou em 3,34 s, exit101:
`InvalidField(source_namespace)` na publicação Knowledge de namespace B sob
admissão A. A correção descrita acima foi revisada antes do patch e reutiliza
o caso qualificado `browser_auth/tests/record_references.rs:1832`. Naquela tentativa, ainda não estava
qualificada. A tentativa 03 terminou101 antes de App/runtime por E0432 externo
em `broker-control/local_password.rs:3`, import `crate::local_password_admission`
não resolvido. Esse bloqueio é histórico; depois a tentativa 04 compilou
em 4 min 17 s e falhou um teste em 3,50 s, exit101, por Denied na admissão
Knowledge de B antes do HTTP. O guard do issuer exige owner_root físico exato
entre trusted_roots; roster/add_current_project não concede essa confiança.
O Seed anterior iniciou bootstrap/Auth somente com A e criou B depois.

A correção test-only agora cria B e sua identidade antes de bootstrap/Auth,
retém ProjectPaths B e passa `[A,B]`; o caso refs reutiliza esse B. O default
AST permanece somente A. Não altera guards, namespace, Auth/Broker/Core ou
admissões SDK. Rustfmt scoped passou; naquele momento a preparação corrigida ainda precisava
execução. A tentativa 05 saiu101 antes da compilação porque o Cargo.lock
compartilhado requer atualização impedida por `--locked`; o log completo tem
duas linhas. Metadata do Root permitiu um snapshot locked para a tentativa 06:
essa barreira foi superada, mas o comando terminou101 antes do integrationtest
por E0624 em `graphit-data/src/lance_module_store.rs:417`, que chama `execute`
privado no path então vigente `vendor/lance/src/index/create.rs:688`
(fonte atual: `graphit-libs (generated Cargo sources)/forks/lance/src/index/create.rs`). Data/Lance pertence a outro
owner, avisado, sem mudança de Source/manifests/lock pelo worker. Após o evento
Root — check App/lib FTS+RawNNsameCopy exit0 (Cargo 1 min 47 s) e snapshots
Code/Broker locked/offline aprovados — houve uma única tentativa 07 no mesmo
target/profile. Ela terminou101 antes do integrationtest por E0599 em
`graphit-ast/src/entity_search.rs:92`: `bytes.bytes()` usa um método inexistente
em RetainedSourceRange; o compilador identifica o campo privado. O log 07
registra três esperas por package cache e uma por build directory; não há
duração numérica confiável, nem equivalência entre parede, compile ou IAB.
E0624 Data não reapareceu, sem atribuir PASS ao backend AST por checks anteriores. Root
foi informado; AST está fora desta ownership. O evento posterior do Root
reportou Gemini global21 compilado/PASS, correção Graph de RetainedRange.bytes,
correção Knowledge de ErrorKind e remoção do classifier antecipado
Artifact/earlywriter, além de metadata Code/Broker full locked/offline PASS
após o ajuste broker-security. Esses outcomes permitem a retomada; não
qualificam esta folha nem transferem PASS de Auth/native/produtores.

A única tentativa 08 saiu da fila no target/profile já descrito, com
`--locked --offline -j4`, e terminou101 antes do integrationtest. Houve quatro
E0425 por `Value` ausente em `graphit-app/src/graph_server/search.rs:46/111/116/122`.
Graph/Search pertence a outro owner, avisado; nenhum Source foi alterado aqui.
`preflight-cargo-08.log` agora é completo e normalizado. As esperas por package
cache/build directory são históricas; não há duração numérica confiável.
Na tentativa 08, HTTP, AST default, host/probe e IAB não foram iniciados.

Após evento Graph/Search compilável, a sequência é single exact refs, AST
original, host/probe finito seguro (somente origin/IDs/locators) ao Root e
jornada browser real por ferramenta qualificada, com join/cleanup. Nenhum
host/cookie foi criado na tentativa 08. A prova NativeImportedKnowledge/Auth6 do
Root não é somada aos 56 mocks ou à qualificação desta folha.

A ferramenta CUA não disponibilizou Browser; `getState` retornou apps/browsers
vazios, e abertura Codex permaneceu queued mesmo após navegação retornar true.
Isso é limitação da ferramenta, não falha do produto ou prova browser.
Essa indisponibilidade histórica motivou a aprovação de
`scripts/verify-record-references-browser.py`, com Python Playwright 1.63.0,
Chromium 155.0.8059.39 e `--probe`/`--browser` explícitos. Somente o detalhe
CUA/IAB do antigo UT2 foi supersedido; `chk-57b54acdb34d` mantém pendente a
mesma jornada funcional real. O launch isolado blank passou viewport,
screenshot e cleanup, sem provar login ou jornada. Antes do último delta de
locator alert, py_compile, --help e scoped diffcheck do runner saíram zero;
cada recibo mantém `ut2_complete:false`, sem antecipar o registro do check.
A receita exige credencial expirada real, projeção ausente, mesmo ID/versão em
NativePaths distintos e abort em voo. Browser02/03 executaram jornadas reais
parciais, com falhas preservadas abaixo; a cobertura candidata desses
requisitos é consolidada ao final, sem converter uma tentativa falha em PASS.

A tentativa 09 compilou em 2 min 30 s e falhou um teste em 13,97 s na linha
1191 por oráculo `complete:false`. SDK/Wiki confirmam `complete` como
integridade e `next_cursor` como continuação/EOF. O widget já estava correto.
O packet45 corrigiu somente o oráculo: primeira página íntegra true/cursor
presente/envelope false; última true/cursor null/envelope true, verificando
arestas Task/Session distintas. O Seed publicou 21 memórias extras via SDK
para paginação real acima do page_size20, sem novo bundle ou mudança de UI.

O preflight10 HTTP/SDK passou um teste em 24,18 s, zero falhas/ignored e sete
filtrados. Cargo reportou 13 min 37 s incluindo fila; não é benchmark ou
latência operacional. Denial403 sem sources, shutdown/reopen e UserInfo0
passaram no recorte do fixture. O exact AST original separado passou um teste
em 2,66 s, sete filtrados; Cargo reportou 4 min 13 s com dependências
compartilhadas novas, sem inferir latência operacional.

Host01 falhou um teste em 25,10 s após preflight: `fs::write` do probe na linha
1248 retornou NotFound porque o env relativo foi interpretado no cwd da crate.
Em Host01 o browser não foi iniciado. Host02 reutilizou a fixture com path
resolvido via PWD e passou um teste em 118,86 s, sete filtrados; Cargo20,38 s.
Drain/reopen/clean/UserInfo0 foram comprovados, mas `.done` não é browser PASS.
Browser02 falhou com AssertionError em namespace_mismatch_and_ambiguous_selection;
Entrar/OIDC real, Task focal default e Memory current/history passaram como
três etapas, sem qualificar o restante. Host03 passou um teste em 49,24 s,
sete filtrados, Cargo2 min 11 s; Browser03 falhou precisamente em
`duplicate_selection_denial`, após observações de identidade/scope e rejeição
do namespace alterado. Não é conclusão da jornada.

Runner04 previa heading "Unable to select memory scope", texto.first e
article0, mas nunca executou. Source confirmou que WorkNotice de erro é div rolealert
com título strong, sem heading; a hipótese anterior estava incorreta. O runner
atual usa alert filtrado pelo título, to_contain_text da mensagem precisa e
article0. Naquele momento essa correção ainda precisava execução. Host04 falhou
no compile por E0599 KnowledgeSearchError::Auth/Index em
`foreground_knowledge/search.rs:555/556` e Failure::ModelUnavailable em
`task_server/browser.rs:179`, antes de probe/browser. O driver em espera foi
cancelado130, sem probe ou host vivo. Depois Root confirmou snapshot App
coerente. Host05 passou um teste em 46,58 s, Cargo6 min 02 s; Browser05 falhou
em duplicate-selection por esperar o aviso errado: screenshot mostrou o alert
Workspace de rejeição correto. Host06 passou um teste em 51,49 s, Cargo47,47 s.
Browser06 aprovou sete etapas — OIDC, Task, Memory, namespace denial, duplicate
selection, restore e Knowledge/backlinks — e falhou na paginação enquanto
resolvia referências, com assert default5s. Timeout como causa é hipótese,
não novo PASS. Não se somam etapas, loops ou reruns aos 56 mocks.

Revision50 acrescentou UT7 `chk-710ecd2b564c`, UT8 `chk-bf7d532d9f19`, UT9
`chk-14187b76ac8f` e UT10 `chk-f2944285f798`; IP54 foi confirmado. O fixture
prepara mirror de identidade real/trusted antes do start e VersionContext
próprio; publica Memory/Knowledge pelo SDK sem refs, para Page/refs sem
projeção com null/warnings/incomplete e paths literais. Hold/entered/release e
RAII activeIssuer controlam o fixture; markers não demonstram cancel/join.
Assertions30s dentro720s são limites do runner, sem garantia de latência.

Main07 passou um teste de hosting em 171,85 s, Cargo2 min 38 s, com shutdown
observado de 5 ms e issueractive0. Browser07 aprovou 14 etapas, incluindo
paginação real20→22, USER/imports e abort `net::ERR_ABORTED`, mas falhou no
mirror porque TaskExplorer perdeu a seleção. Etapas não são novos testes nem
conclusão de UT2. O ajuste de race/budget posterior a07 exige requalificação.

A correção produtiva estreita de TaskExplorer aguarda a seleção literal
validada do catálogo, conserva essa seleção na URI e limpa somente o ID do
registro/dossier. O cancelamento do export depende apenas de targetKey;
catálogo vazio legítimo não provoca get. Preserva os controles e o design
existentes. RED01 observou selection:null; green01/02 falharam por afirmar URI
antes do catálogo validado. Green03 passou 12 testes (5,81 s de testes,
9,04 s de comando), com lint e typecheck/build exit0. Essas unidades não se
somam aos 56 mocks distintos de referências. Esse build atualizou dist com
index de 2.622.635 bytes, SHA256
`31428d3d0dd41b6083786e6a0abc0a3cc65c8bb61acc847bf57e78dba0a01ce8`;
logo/favicon mantêm 249 bytes cada. Não era ainda o shipping servido em Main08:
`foreground_ui.rs` incorpora `assets/ui/index.html` por include_bytes, e esse
asset ainda era o UID/Source `7af…`, com 2.622.540 bytes. Portanto o pointer de
`tsk-1b06` não era divergência nesse snapshot. O build01 de shipping `314…` corresponde somente ao fix de redirect e é
histórico frente ao build final descrito abaixo; não demonstra bundle servido
em Main08.

Expiry01 passou o host em 68,04 s, Cargo2 min 08 s, e três etapas browser,
mas o recibo preserva status running por bug de finalização/indentação.
Expiry02 passou o host em 45,07 s, Cargo4 min 32 s, e seu recibo browser é
passed: login1304ms, leitor inicial1271ms e negação após expiração32961ms,
incluindo a espera30s. A sessão configurada30s, refresh margin1s e host300s
produziu auth401 fresca e negação protegida, sem article/dossier nem credencial
injetada. Não é logout ou prova de expiração de access token. O coordenador registrou UT7 PASS com
essa evidência; esta transcrição somente reflete o readback.

Main08 passou um teste de hosting em 206,29 s, oito filtrados, Cargo2 min 29 s.
Browser08 aprovou 16 etapas, acrescentando Session→Memory e zero-edge com
namespace observado, mas falhou com TimeoutError no mirror. Os quatro campos
mirror_diagnostic foram false: seleção URI, request de Task reader, catálogo e
contexts. O fixture ainda serviu o asset `7af…`; não qualifica o fix no dist
`314…`, UT2 ou o mirror. Essas etapas não são novas unidades nem conclusão
da jornada.

O delta seguinte, autorizado e implementado, faz reset e onBack usarem o mesmo
catalogueHref literal. O RED Back teve um FAIL/12 skipped em5,26 s; GreenBack
passou13 em7,67 s. Build02 falhou por TS2769 no teste e foi corrigido. O RED
Import teve um FAIL/nove skipped em6,02 s. A rodada final conjunta passou23
casos em duas suites,9,78 s (Task13/widget10), com scoped ESLint exit0. Não se
somam essas rodadas sobrepostas aos56 mocks de referências.

O build final real `node scripts/build-rust-ui.mjs` passou typecheck, Vite e
cópia, exit0,8,70 s Vite. Dist e os três assets shipped são byte-equal:
index2.622.731 bytes, SHA256
`48ed2bc721a1eb6fa6cf8f2218151e57ad462ae182519b0426c129ac35b46ffb`;
logo/favicon249 bytes cada, SHA256
`537271954e96afaff8307be5c272579e88f45ecc0ac11c7199b87d30e9cd1233`.
O pointer Source/UID de `tsk-1b06` ainda indica7af e agora precisa atualização
pelo owner, sem alterar aquela evidência histórica nesta folha. Os dois
whitespace em strings third-party do HTML gerado são preservados como bytes
reais do Vite; Source/docs diffcheck passou.

Main09 passou um teste de hosting em141,24 s, oito filtrados, Cargo2 min39 s,
com drain11ms, issuer0, UserInfo0 e Source joined/clean. Browser09 aprovou18
etapas: asset servidoHTTP igual ao48ed local em120ms; composição focal
importada200 com scopeB, sem seleção PROJECT e com artifact em contexts
em7198ms; mirror com mesmo ID/versão e NativePaths distintos em1714ms,
sem article antigo; abort real net::ERR_ABORTED em3567ms. Etapas não são
unidades distintas nem conclusão da jornada.

A etapa missing_projection_no_invented_navigation falhou com KeyError: o
oráculo acessava request['selection'] em request importado sem selection.
Antes do erro, UI indisponível/sem links e focused namespace/witness nulos
já haviam sido assertados; isso não é PASS completo de projeção ou UT9.
O runner foi corrigido para get(selection), sem mudança de UI ou novo build.
O exact AST original após os casos residuais passou um teste em4,09 s, oito
filtrados, Cargo1 min09 s incluindo fila. Não é novo IAB ou Source/UID PASS.

Main10 passou um teste de host em146,90 s, oito filtrados, Cargo13,53 s sem
Compiling, drain10ms, issuer0 e UserInfo0/clean. Browser10 aprovou19 etapas,
incluindo projeção ausente em1291ms, e falhou no oráculo narrow com
AssertionError. Seu status failed permanece histórico, sem converter19 etapas
em uma jornada integral aprovada.

Main11 usou o binário existente diretamente: host PASS1 em253,22 s, oito
filtrados. Browser11 aprovou19 etapas e falhou antes de medir layout;
narrow_layout está ausente, portanto não provou overflow global. A screenshot
mostrou o heading do reader focado; NativeKnowledgeDocument já foca no load.
Corrida de foco no oráculo é hipótese, não defeito de produto confirmado.

O helper narrow/logout e o modo `--only-narrow` agora aguardam heading do
reader focado e References visível antes de focar Reload links e pressionar
Enter. O diagnóstico conserva apenas linha do oráculo, erros, dimensões e
classes, sem corpo Auth/query. Não houve mudança CSS/UI ou novo bundle.
Main12 focal passou quatro etapas: asset48ed servido166ms, login1926ms,
narrow5367ms e logout816ms. Viewport390 e scroll390, overflow[]; Enter produziu
refsPOST200. Logout204, auth/session401 e Memory contexts401 deixaram ausência
de article antigo. O host direto passou um teste em51,69 s, oito filtrados,
drain10ms, issuer0 e UserInfo0/clean.

A cobertura funcional qualificada e aceita pelo coordenador combina as19
etapas reais de Main10, narrow/logout de focal12, a expiração real30s de
Expiry02 e a regressão AST original. Não é um único full-run PASS ou soma de
loops; as tentativas10/11 permanecem failed. Não há execução ativa ou residual
funcional de referências restante nesse conjunto observado.

A regressão literal409 passou11 testes do widget em1,60 s (326ms de testes):
NativeRecordReadError(409, knowledge_generation_changed) conserva erro causal,
References unavailable e ausência de links, sem empty saudável ou auto-retry;
somente Retry explícito refaz a chamada. ESLint e typecheck saíram0, sem
mudança de Source produtivo ou do bundle48ed. A união atual é60 unidades
distintas:56 históricas mais mirror, onBack, contexto focal importado e409.
As rodadas sobrepostas23/33/13/11 não são somadas.

O snapshot75, após readback74, é histórico: à época registrava13 checks
ativos PASS e U3/UT3 documentais pendentes. Essa entrega correspondia à spec de
39.110 bytes e ao recibo de82.585 bytes; seus digests estão preservados no
recibo atual. O coordenador registrou U3 em76, UT3 em77 e progress78, concluindo
`tsk-51f2` em COMPLETED79, sem flag, com15/15 checks ativos PASS e seu claim
liberado. O readback79 confirmou a conclusão. Não há execução ativa ou requisito
pendente desta entrega, nem novas suites/checks neste addendum documental.

## Evidência e ownership

Entradas App: `foreground_record_references.rs`, `reference_explorer.rs`, `browser_auth/task.rs` e `task_server/browser.rs`. Composição trusted: `foreground_source.rs`, `foreground_tasks.rs` e `TaskPolicy.imported_knowledge`. O helper `foreground_knowledge::PageView` reutiliza a view FULL16 já existente. Fixtures finitas: `browser_auth/tests/record_references.rs`; parsers/encoder em `reference_explorer::tests`.
