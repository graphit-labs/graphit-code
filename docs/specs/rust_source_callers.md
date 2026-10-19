---
references:
  - target: {type: task, id: tsk-be6e}
    relation: implements
  - target: {type: task, id: tsk-a9c4}
    relation: supports
  - target: {type: task, id: tsk-bdd8}
    relation: implements
  - target: {type: task, id: tsk-2845}
    relation: supports
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Clientes Rust de Source

`ast_source` recebe a instalação selecionada, o endereço loopback publicado pelo
serviço, um `reqwest::Client` compartilhado, limites de documento e o contexto
original da operação. Não inicia emissor, cria instalação ou troca a chave do
daemon. A descoberta do endereço e a composição do executável pertencem à
entrega `ff08`.

Nos executáveis Linux, Windows e macOS, a composição prepara o pedido e garante
o daemon pronto antes de chamar este cliente. Quando há checkout real, mantém sua
seleção pela admissão Runtime. Antes de criar o proxy MCP stdio, `source_entry`
captura o PROJECT selecionado, verifica seu path UTF-8 e envia `MaintainCheckout`
ao Source descoberto, usando sua identidade existente, versão e Original. Falhas
de manutenção impedem a conexão do proxy. Sem PROJECT selecionado, o proxy usa
a instalação. O helper não inicia serviço nem cria identidade. Essa etapa não
pertence ao SDK de leitura e não altera seu erro de descoberta ausente; veja
[autostart Source](rust_source_foreground.md#autostart-dos-consumidores-cli-no-linux).

Os entrypoints `knowledge ask`, `knowledge search`, `knowledge source`, `knowledge log` e `ast export`
compõem seus mesmos pedidos tipados em Linux, Windows e macOS. Eles usam a
descoberta e conexão Source autenticadas existentes; não executam produtores
alternativos por SO. Seletores PROJECT/artefato, limites, cancelamento, prazo e
encerramento da conexão mantêm seus contratos. Essa disponibilidade de composição
não substitui a validação de execução no SO.

O leitor Native `graphit_wiki_source` e `graphit knowledge source <slug>` usam
a identidade exata da página, nunca um pathname do host. O MCP recebe `slug`,
`selectors` Core opcionais, `source` e a seleção PROJECT. `source` aceita
`project_knowledge` ou `imported_knowledge` com `artifact {id, version,
context_alias?}`; importado não aceita seletores PROJECT. O CLI preserva
`--project-dir`/`--project-version` ou `--artifact-id`/`--artifact-version`,
além de `--head`, `--tail`, `--start`, `--end`, `--pattern`, `--regex`,
`--before`, `--after` e `--line-numbers`. Não há alias Native `context` ou `path`.

Sem recorte, o renderer existente retorna metadata YAML nativa e o corpo
Markdown literal inteiro. O preview de busca é um resumo ou trecho do corpo
limitado a 1000 escalares e não substitui essa leitura. O resultado conserva
geração, source, slug, linhas originais e focused_source/namespace quando o
corpus possui essa proveniência. Ausência de corpus, corpus vazio, página ausente
e instalação não registrada permanecem distintas. PROJECT usa o leitor
autenticado existente; importado usa seu pin, registry e guards físicos, inclusive
o leitor remoto admitido quando aplicável. Nenhuma seleção concede acesso.

O scanner filtra o slug lógico no pin admitido e chega a EOF do stream
selecionado antes da entrega. Duplicidade, namespace, witness e integridade dos
registros selecionados são obrigatórios; corrupção em documentos alheios não
bloqueia esse get. Corpo e focused_source vêm do mesmo registro/pin, sem outra
consulta global de referências nem mistura de gerações. Um probe de metadata
limitado a uma identidade distingue corpus vazio de página ausente sem projetar
outros corpos. Exportação e listagem integral conservam sua validação global.
Essa regra supersede a exigência anterior de falha por corrupção não selecionada.
Os limites de renderização e resposta falham com `knowledge_source_limit` ou
`response_limit`, sem truncamento implícito. O worker pertence ao Source e conserva Auth, Original, cancelamento,
prazo e join no shutdown. O CLI usa a mesma chamada SDK autenticada do consumidor
de busca, sem montar outro leitor ou iniciar inferência.

`SourceArguments` deriva Clap `Args` e o parser central do executável define
`graphit ast source <relative-path>`. Preserva `--context`, `--entity`,
`--entity-type`, `--head`, `--tail`, `--start`, `--end`, `--pattern`, `--regex`,
`--before`, `--after`, `--line-numbers`, `--project-dir` e `--project-version`.
`--config` e `--non-interactive` são dados do chamador. Não converte paths
inválidos para UTF-8 com perda, aceita floats como inteiros ou adiciona aliases.
A fixture privada valida argumentos `OsString` sem duplicar árvore na produção.
Help retorna o resultado próprio do Clap, com exit0; erros de argumentos usam
exit2. Seletores são enviados ao conjunto retido do serviço; leituras não
consultam Git ou inicializam projetos.

`--context vendor/context@release-1` seleciona o ID completo e a versão concreta
do artefato AST instalado. CLI e proxy preservam esse valor sem inserir a raiz
ou versão do projeto atual. O serviço consulta o registro da instalação real e
usa o compositor AST autenticado. Raw disponível, inclusive vazio, vence disco;
misses sem fallback explícito retornam erro de Source indisponível. O fluxo não
instala, baixa ou repara o contexto. Para fallback físico, `--project-dir` deve
selecionar um tuple retido pelo Runtime; `--project-version` é obrigatório se
houver mais de um tuple nessa raiz. Os dois seletores explícitos são preservados.
A versão do artefato não é convertida em versão de projeto/Git. Identidade e
pathname do SourceRef são metadata, sem conceder permissão.

Com `--context` e `--entity`, o serviço seleciona a entidade no grafo Native e
aplica os recortes ao Source indexado da mesma captura. A janela usa
`start_line`/`end_line` do produtor, somente em linhas disponíveis com
`extent_status = Exact`; `line_number` indica o nome e pode vir depois do início
da declaração. Isso também vale para o consumidor Native remoto.
`--entity-type` é o label exato do schema para desambiguar. Entidades ausentes/ambíguas, metadata inválida
e raw ausente retornam erro, sem aplicar coordenadas a bytes físicos sem vínculo.
O caminho está descrito em [Source por entidade](rust_ast_entity_source.md).
A seleção por entidade no PROJECT local usa o produtor e o resolver existentes,
com vínculo entre grafo e raw; sua qualificação permanece separada da correção
do extent importado.

`runtime_headers` captura e lê somente `daemon/mcp.key` com o SDK Core, usando
o mesmo snapshot original. Ausência, formato diferente de 64 dígitos hexadecimais
minúsculos, troca da observação, falhas de acesso, cancelamento e deadline são
erros. O fluxo não cria nada. O buffer temporário usa `Zeroizing`; Authorization
é um `HeaderValue` sensível e a origem é explicitamente `runtime`. O SDK HTTP
mantém suas próprias cópias até encerrar a operação; não há promessa de apagar
buffers de bibliotecas externas. Credenciais local/managed permanecem nas
entregas específicas, sem escolha implícita de perfil ou chave.

`call_source_result` abre uma conexão SDK privada por operação, compartilhando
o pool do cliente HTTP. Usa o ciclo oficial Discover do rmcp3.5.1 e chama o
tool real `graphit_ast_source`. O contexto original é verificado antes da
leitura, do envio e da entrega. Cancelamento/deadline encerram a conexão e
aguardam sua limpeza. O servidor autoriza sua própria operação: contextos não
são serializados nem tratados como uma identidade única entre processos.

O adapter privado `SourceHttpClient` liga GET, POST e DELETE do SDK ao mesmo
`Arc<SnapshotContext>` e prazo absoluto já admitido. Preserva o pool reqwest,
Discover, headers e os caps SSE do SDK. O POST inicial do rmcp3.5.1 aguarda a
resposta antes de entrar no loop que observa o cancelamento do transporte;
por isso, cancelar apenas o future de bootstrap podia deixar o socket aberto.
O adapter cancela esse I/O e aplica `take_until` aos streams SSE recebidos,
inclusive durante bootstrap. Não cria protocolo, polling, novo TTL ou um
segundo transporte. O retorno da conexão revalida a Original, preserva
`Interrupted`/`TimedOut` e encerra uma conexão que terminou de abrir depois
desse limite.

O proxy também observa cancelamento, stop e prazo enquanto aguarda a conexão.
Seu guard sinaliza a mesma Original do job, que permanece no JoinSet até
terminar. A regressão de handshake usa bootstrap Source real seguido de um
endpoint que não responde; shutdown precisa concluir o join e fechar esse
socket, sem aguardar o timeout inteiro da chamada. As ferramentas e o stream
Now usam o [contrato Source Now](rust_source_now.md).

`run_source` escreve o texto recebido exatamente em `Write`, inclusive vazio,
sem trim, normalização de linhas ou mensagem substituta para ausência de
matches. Erros de domínio retêm o resultado SDK como causa; erros de transporte
e escrita preservam sua causa. Interrupção usa exit130 e demais falhas de
operação/escrita exit1. A escrita pode ter sido parcial quando o destino falha.

`mcp_source::SourceProxy` retém todos os tools e seus metadados retornados pelo
catálogo autenticado do Source e usa o SDK para stdio ou streams fornecidos.
AST Source mantém seu ramo tipado, Ask seu handler de notifications e Now seu
forwarding existente. Os demais tools anunciados, incluindo Memory, semântica,
hooks e Conversation quando disponíveis, são encaminhados por `call_tool` do
SDK sem traduzir seus argumentos para `SourceArguments`. Nome não anunciado
retorna `METHOD_NOT_FOUND`; cada ingresso no Source continua responsável por
sua autorização real. Um schema anunciado não concede acesso ao módulo. Mantém schema e pool compartilhados, com
credencial atual e cancelamento próprios para cada chamada. Cada nova request,
inclusive AST Source, forwarding genérico e Ask, resolve a descoberta da mesma
instalação com sua própria Original antes de conectar e ler a credencial atual.
O endereço usado na negociação inicial não fica retido para requests futuras;
PROJECT, versão e catálogo negociados permanecem capturados. Uma descoberta
ausente ou inválida é erro dessa request. Uma falha de conexão ou operação não
repete a chamada nem inicia o serviço; uma request posterior pode observar o
endpoint substituído. A regressão
`same_stdio_proxy_discovers_replacement_endpoint_for_each_new_request` mantém a
mesma sessão stdio entre a retirada do primeiro endpoint e a publicação do segundo,
com preservação da seleção e da negação real de autoridade. Argumentos extras,
null presentes e tipos incorretos são rejeitados. EOF/shutdown interrompem
chamadas em curso e aguardam os trabalhos assíncronos pertencentes ao proxy;
o serviço Source é responsável por seus próprios trabalhos nativos. Cancelar
um proxy não cancela outro que usa o mesmo serviço. Nenhum parser JSON-RPC ou
byte tunnel próprio é usado.

Antes de conectar ou ler a credencial Runtime, `SourceProxy::new` nega a presença de qualquer input `GRAPHIT_ROLE_BEARER`, `GRAPHIT_ROLE_REFERENCE` ou `GRAPHIT_ROLE_SELECTION`, inclusive vazio, parcial ou não UTF-8. Esses inputs só impedem o ingresso Root; não autenticam uma Role. O ingresso autorizado de Role continua `mcp --stdio --role` e `RoleProxy`. A prova focal compara o catálogo HTTP e a resposta de Memory com o proxy usando Auth real; a fixture Read(Ast) conserva a negação Memory em ambos caminhos, sem novos grants. O peer ACP é controlado; isso não comprova instalação completa do Coordinator nem eventos do Kiro instalado.

Essa entrega `c1c2` valida o componente cliente e proxy contra Source HTTP real
`1b8b`, com instalação, emissor e corpus Lance físicos. Não qualifica o
executável/discovery completo, hosts Kiro, outros sistemas operacionais ou
rotas managed/entity qualificadas separadamente. O consumidor importado `7740` tem
qualificação própria abaixo. O contrato atual usa Kiro V3 com negociação e framing
do SDK no serviço externo.

A qualificação `7740`, em 2026-10-11, passou os três grupos do cliente
`ast_source`, os três do proxy `mcp_source`, os sete do executável
`source_executable` e os onze HTTP de `source_server`. As jornadas importadas
usam duas origens nativas registradas pelo HubWriter, com o mesmo nome curto e
path, corpus Lance físico e autorização real. Validam raw sem captura do projeto
atual, texto vazio, miss sem fallback implícito e preservação dos seletores
Runtime explícitos pelo CLI. As regressões locais de stdout, sinais, EOF,
credenciais, discovery e cleanup também passaram. Os comandos e resultados,
incluindo as falhas históricas de compilação, estão em `tsk-7740`. O
[contrato importado](rust_imported_source.md) permanece separado da produção de
um registro a partir de um pacote baixado e da restauração semântica do AST.

## Consumidor Ask no mesmo Source-mode

O proxy conserva o ramo tipado AST Source e encaminha Ask opcional pelo
[handler específico](../../crates/graphit-app/src/mcp_source/knowledge_ask.rs).
A disponibilidade vem do catálogo HTTP dos providers ativos da [composição App](../architecture/rust-knowledge-question.md); Gemini CLI foi removido, preservando a API Google Gemini.
chamada explícita à ferramenta
ausente conserva o erro real. O helper privado `connect_source<H: ClientHandler>`
exige handler no último argumento e retorna `RunningService<RoleClient,H>`;
os três callers anteriores fornecem `()`. Não há wrapper da assinatura antiga.

Ask preserva request fields SDK por move e só preenche seletores ausentes
conforme o root físico atual. O token recebido no STDIO é ligado ao token
real do handle downstream; ordem e final aguardam o prefixo nativo aceito:
`receipt.known_events` para provider ausente (Codex existente) ou literal
`codex`, e `receipt.accepted_notifications` para provider literal `kiro`.
Provider unknown, null ou não string é rejeitado sem fallback Codex.
`observed_notifications` inclui notifications ignoradas,
estrangeiras ou anteriores à fase aceita e não define o prefixo. O contador
aceito ausente, null, negativo ou fracionário não vira zero, inclusive em erro
nativo; erro sem receipt mantém alvo não observado. Uma negação do Original
após push pode deixar envio além do prefixo aceito, produzindo Tail legítimo.
A projeção conserva receipt, causa, usage, process e metadata e remove Answer
sob negação, sem fabricar contador ou reinterpretar último envio como aceitação.
Metadata de notification rmcp3.5.1 é movida das extensions, enquanto request
meta permanece em `Incoming.meta`. Cancelamento fecha admissão e drena
callbacks iniciados; callbacks ainda não polled no SDK encontram closed/zero IO.
As fronteiras de erro, limites e logging estão em
[Knowledge Ask por STDIO](../architecture/rust-knowledge-question.md).
A qualificação atual `tsk-bdd8` passou 24 casos Linux sem ignored, incluindo
12 Gate, cinco Ask STDIO e sete regressões dos três callers anteriores:
MCP Source (3), AST Source (2 focais) e Ask CLI (2). Clippy selecionado de App
lib/bin e quatro integrações passou com `--no-deps -D warnings`. A composição
compilada, limites de logging, comandos, tempos e falhas preservadas estão na
[qualificação Ask](../architecture/rust-knowledge-question.md). Essas provas
não ampliam as jornadas importadas, hosts ou plataformas descritas acima.

## Root MCP selection

`graphit mcp --stdio --project-dir <directory>` selects the proxy's default
PROJECT. The option belongs to the `mcp` subcommand and conflicts with `--role`.
The retained PROJECT and version are selectors and grant no authority.

For generically forwarded tools, the proxy reads the actual Source
`input_schema`. An absent `project_dir` receives a literal Unicode string
only when that schema declares a string field. An absent `project_version` receives the
captured version only when the schema declares it and the selected directory
corresponds to the captured PROJECT. A different explicit directory never
inherits the current version. Explicit invalid or null values and any request
containing `context` remain unchanged. Schemas without these fields receive
no extra arguments. AST Source, Ask and Now retain their dedicated handlers.
An explicit `scope: user` also passes through without captured PROJECT or
version defaults. Source validates its selectors and verifies the actual native
user independently.
Every call still uses actual Source authentication and authorization.

The tsk-be6e Linux Xtask Integration run passed five cases, including multiple
PROJECT/version selection, actual schema defaults, explicit invalid selectors,
Root Role-input denial and real catalog forwarding. The unavailable AST native
operation returns its actual failure; this is not AST publication success.
Coordinator publication and installed Kiro events retain separate evidence.

The catalog and selector consumer has an existing-runner guard:

```text
cargo xtask test graphit-app/mcp_source
```

It is available through the test selector catalog and covered by the pre-push workspace suite. The current post-cleanup Integration run passed
all five cases in 0.26s. Source Original/cancellation regressions passed two
tests and hook configuration/caller regressions passed six. The current selected
App strict passed with warnings denied for the production library and
MCP/publisher Integration targets. Library and native host scopes remain separate.

Memory Role tools preserve the effect that actually completed. A record mutation
uses `write_confirmed`; a lexical publication uses `index_confirmed`, and an
embedding publication uses `embedding_confirmed`. Each keeps the observed SDK
version and nested cause. An index publication alone does not confirm creation
or revision of a Memory record. If both effects completed, the Records error
retains the record confirmation outside the index cause. Only a successful or
confirmed record mutation requests the existing project rescan.
