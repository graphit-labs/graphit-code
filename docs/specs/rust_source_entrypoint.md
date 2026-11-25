---
title: Entrada Rust de Source
type: specification
references:
  - target: {type: task, id: tsk-cab0}
    relation: corrected_by
  - target: {type: task, id: tsk-a9c4}
    relation: supports
  - target: {type: task, id: tsk-bdd8}
    relation: implemented_by
  - target: {type: task, id: tsk-ff08}
    relation: implemented_by
  - target: {type: task, id: tsk-5a6c}
    relation: supports
  - target: {type: task, id: tsk-8f29}
    relation: derived_from
  - target: {type: task, id: tsk-7be7}
    relation: implemented_by
  - target: {type: task, id: tsk-fa07}
    relation: implemented_by
  - target: {type: task, id: tsk-3c89}
    relation: derived_from
  - target: {type: task, id: tsk-c1c2}
    relation: derived_from
---

# Entrada Rust de Source

`source_entry::call_source` recebe a instalação já selecionada, cliente HTTP,
`SourceArguments`, limites de documento, contexto original e destino `Write`.
Lê a descoberta atual `daemon/mcp.json` pelo SDK Core e usa seu endereço literal
para o cliente autenticado. A leitura nativa usa o mesmo snapshot original em
um trabalho Tokio aguardado antes do retorno. Ausência é `NotFound`; schema,
acesso, cancelamento, deadline e transporte preservam erros e causas. O fluxo
não cria instalação, emissor, chave ou endereço substituto.

A composição CLI Linux executa o [autostart](rust_source_foreground.md#autostart-dos-consumidores-cli-no-linux)
antes desta leitura. Valida o pedido uma vez e reutiliza seus dados no RPC;
quando aplicável, mantém o checkout real antes da operação. O cliente
`call_source` e a função `address` permanecem readonly. O helper público
`maintain_checkout` concentra a manutenção autenticada já usada pelo stdio,
sem duplicar a chamada nessa sessão nem conceder autoridade pela seleção.
Essa manutenção automática envia a versão vazia para o serviço observar o
checkout físico atual. A versão explícita de leitura segue somente para o RPC
ou para os defaults do SourceProxy: não registra uma versão mantida, não muda
o checkout e não transforma uma seleção inexistente em seleção disponível.
`setup` também mantém somente o checkout atual e rejeita `--project-version`
antes de configurar a instalação; a manutenção explícita de uma versão usa
`projects maintain-checkout`.

Os argumentos vêm do Clap central. `--config/-c` é uma seleção inline `KEY=VALUE`,
conforme a referência atual; não é interpretado como arquivo por esse módulo.
Configuração e versão são selecionadas pelo chamador uma vez. Source não
reparseia argv, inicializa projeto ou consulta Git para permitir uma leitura.
O texto e os erros são os mesmos de [Clientes Rust de Source](rust_source_callers.md):
texto exato, vazio disponível preservado e fallback físico somente na ausência
tipada. Cada chamada lê a credencial atual; descoberta/PID não são autoridade.

`SourceProxy::new` e `source_entry::stdio` recebem
`current: Option<(&Path, Option<&str>)>` imediatamente antes de `original`.
O chamador fornece o root físico atual e, quando explícita, a versão selecionada; o proxy
retém `PathBuf`/`Option<String>` como dados de seleção. A ausência de versão
não descarta o projeto físico nem gera uma versão padrão. A conversão do root
para o wire UTF-8 é exata: root não representável retorna `InvalidInput` antes da conexão.
O proxy não resolve Git, muda cwd/ambiente ou canonicaliza caminhos durante
uma leitura.

O dispatcher de `graphit mcp --stdio` também aceita execução global fora de
um PROJECT ou no diretório da instalação. Nessa entrada, resolve somente
configuração de distribuição, instalação, ambiente e argumentos, e passa
`current: None`; não envia MaintainCheckout. Uma versão de PROJECT sem
projeto é inválida. `--project-dir` exige descoberta física válida; erros de
acesso, configuração ou identidade não viram execução global. Com um PROJECT
descoberto, a entrada passa seu root e versão solicitada ao proxy e mantém o
checkout físico antes da sessão stdio. A entrada `--role` conserva seu transporte
próprio. A seleção global não concede acesso a corpora ou projetos.

`graphit ast imports` e operações AST com `--context id@version` sem fallback
de PROJECT usam a mesma configuração de instalação, inclusive quando o cwd
é um checkout. Não descobrem um PROJECT nem inventam seletor para ler um
artefato instalado. Imports envia `graphit_ast_imports` com objeto vazio;
query/schema/sample/neighborhood e Source conservam seus inputs nativos.
`--project-version` não se aplica a essa entrada; a versão do artefato está
no contexto explícito. Source com `--project-dir` captura esse PROJECT real
quando há um fallback explícito, conservando a autorização própria da leitura.

Com `current`, `project_dir` ausente recebe esse root mesmo quando a chamada
fornece `project_version`. Diretório ausente ou igual ao root por comparação
de `Path` usa o root atual; uma versão global presente preenche somente a
ausência de `project_version`. Sem versão global, o campo permanece ausente
no wire. A versão explícita prevalece. Um diretório diferente mantém
todos os argumentos da chamada e a seleção/default do próprio alvo, sem
herdar a versão global. `current: None` preserva a seleção existente. Valores
null presentes continuam inválidos. Os defaults seguem o catálogo anunciado
ao proxy; o serviço atual continua validando e autorizando cada chamada.

No servidor, um pedido para o root corrente sem versão usa o índice corrente
já retido em `SourceServer`. Esse índice pode representar main ou feature;
não vem da última versão registrada no inventário. Um projeto irmão com
múltiplas versões retidas exige versão explícita e continua ambíguo sem ela.
Source não consulta inventário nem Git para escolher essa versão.

A regressão de `fa07` atravessa rmcp stdio e HTTP Source reais com o projeto
atual diferente do default do servidor. Branches Lance materializadas distinguem
main/feature; os casos verificam herança, override explícito sem/com diretório,
projeto irmão e `None`, além dos erros reais de seleção e parâmetros inválidos.
Essa prova pertence ao componente. A regressão de `7be7` acrescenta projeto
físico com versão omitida, servidor corrente main/feature, overrides explícitos
e irmão main/feature com ambiguidade preservada; usa corpus Lance, stdio e
HTTP reais. A qualificação do executável compilado permanece separada em
`5a6c`; os testes desses SDKs não a substituem.

`source_entry::stdio` recebe os mesmos dados, a política
`(NativeDocumentFileLimits, ContentLimits, Duration)`, handles Tokio e um
`Cancellation` próprio do serviço. O original de startup é finito e não fica
retido no proxy. O serviço pode continuar além desse deadline; cada chamada
recebe seu próprio contexto no cliente/servidor. O SDK rmcp faz negociação,
catálogo e framing. EOF aguarda protocolo e jobs; cancelamento do proprietário
interrompe a sessão, aguarda seus jobs e retorna `Interrupted`, preservando a
causa quando o SDK reporta erro no encerramento. O servidor HTTP mantém a
responsabilidade por seus próprios trabalhos nativos.

A folha `3c89` valida esse componente com instalação privada, emissor,
listener, descoberta, versões Runtime e corpus Lance reais. Usa um cliente
stdio SDK e verifica texto/empty/selectors, erros de descoberta e original,
chave estável e conexão independente depois de EOF/cancelamento. Esses testes
não são a execução da CLI compilada.

A composição do executável real pertence a `5a6c`: `graphit ast source` e
`graphit mcp --stdio` usam configuração central e projetos físicos explícitos.
O foreground e seus projetos retidos têm a qualificação própria de `8f29`.
Esta folha valida os SDKs; não certifica a execução da CLI compilada. O
contrato de descoberta está em
[Source service discovery](rust_source_discovery.md); hosts Kiro,
outros sistemas operacionais e o restante do daemon têm validações próprias.

O chamador CLI verifica também o `flush` de stdout antes de retornar sucesso.
Assim, um pipe fechado retorna exit 1 mesmo quando texto sem quebra de linha
foi inicialmente aceito pelo buffer. A saída não é transacional: uma falha de
escrita pode ocorrer depois de bytes já entregues.

Os casos executáveis de `ff08` complementam a cobertura de `5a6c` em Linux:
pipe fechado e interrupção por SIGINT de um RPC em andamento. O segundo caso
encaminha tráfego autenticado ao servidor Source real por um relay Axum local
que retém sua resposta; não substitui o emissor nem o resultado do parser.
O cliente termina com exit 130 sem payload, e o daemon independente permanece
utilizável com a mesma chave. O caso não promete preempção de trabalho nativo.
Ambos reutilizam a instalação privada e o corpus Lance da suíte existente.
O guard recorrente é `cargo xtask verify-fast-guards graphit-app/source_executable`.

## Stdio persistente durante a atualização do daemon

A sessão `graphit mcp --stdio` pertence ao agente e pode permanecer aberta
durante várias versões do daemon HTTP. Atualizar o daemon deve preservar as
chamadas e os contratos já anunciados a esses proxies; não exige igualdade de
versão ou de hash entre os executáveis, nem reiniciar o agente como parte da
ativação. A cada nova chamada o proxy lê a descoberta e a credencial atuais e
negocia a conexão pelo SDK MCP. Autenticação e autorização continuam sendo
verificadas pelo serviço atual.

Uma chamada interrompida durante a substituição pode retornar erro explícito.
Ela não deve ser repetida automaticamente: uma mutação pode já ter sido
aplicada. A chamada seguinte redescobre o endpoint. O teste
`same_stdio_proxy_discovers_replacement_endpoint_for_each_new_request` mantém
o mesmo stdio aberto, substitui o listener e verifica recuperação e negação de
acesso no novo serviço.

O catálogo deste proxy é o snapshot obtido na abertura; ele não anuncia
`tools.listChanged`. Portanto, a continuidade de chamadas existentes não
comprova descoberta dinâmica de ferramentas ou campos adicionados depois.
Uma atualização do daemon precisa manter compatíveis os contratos já
anunciados. A correção do Schema live preserva seu formato e altera somente
a obtenção dos dados, sem exigir atualização do proxy aberto.

## Ask opcional no ingresso Source

`graphit mcp --stdio` usa o mesmo SourceProxy e descoberta Runtime. Seu catálogo preserva os tools e metadados que o HTTP autenticado oferece. AST Source e Knowledge Ask conservam seus handlers; os demais tools anunciados são encaminhados pelo SDK sob a autoridade do Source, sem admitir nomes desconhecidos. Knowledge Ask é anunciado somente quando o serviço o oferece. Cada chamada Ask encaminha os campos SDK, preenche apenas os seletores ausentes e preserva o Original por job. Provider unknown, null ou não string é rejeitado sem fallback Codex. `observed_notifications` inclui notifications ignoradas, estrangeiras ou anteriores à fase aceita e não é prefixo. Contador ausente, null, negativo ou fracionário não vira zero, inclusive em erro nativo; erro sem receipt mantém alvo não observado. A negação pós-push do Original pode produzir Tail legítimo: conserva receipt, causa, usage, process e metadata e remove Answer. Não se fabrica contador Codex para Kiro nem se infere aceitação pelo último envio. Detalhes, metadata SDK e limites de logging estão em [Consulta documental Knowledge](../architecture/rust-knowledge-question.md). O modo Role usa seu ingresso próprio `mcp --stdio --role` e `RoleProxy`. Qualquer input de transporte `GRAPHIT_ROLE_*` da factory, mesmo parcial ou vazio, faz `SourceProxy::new` negar o ingresso Root antes de ler sua credencial. Presença de env não autentica a Role nem concede permissão. Credenciais Role ausentes não transformam esta conexão Runtime em fallback. `SourceProxy::new` recebe a seleção Root opcional de PROJECT/version; `source_entry::stdio` e o ramo SourceArguments mantêm seu ingresso próprio.

## Root PROJECT locator

The Root CLI accepts `graphit mcp --stdio --project-dir <directory>`.
`--project-dir` belongs to the `mcp` subcommand, conflicts with `--role` and
supplies a locator rather than authority. The caller passes the selected
PROJECT and captured version to SourceProxy.

Generic forwarding respects the actual tool schema: an absent `project_dir`
receives a Unicode path string matching the announced type, and
an absent `project_version` receives the captured version only for the same
PROJECT when that field is declared. Different directories, explicit invalid
or null values and imported contexts retain their original selection.
AST Source, Ask and Now continue through their dedicated handlers. Source
authenticates and authorizes each operation through its real producers.

The tsk-be6e Linux Xtask Integration run passed five catalog/gate/selection
cases without ignored tests, including multiple PROJECTs and versions and
actual schema-directed defaults. Its actual unavailable AST operation error
does not establish AST publication, Coordinator publication or Kiro execution.

The catalog and selector consumer has an existing-runner guard:

```text
cargo xtask test graphit-app/mcp_source
```

It is available through the test selector catalog and covered by the pre-push workspace suite. The current post-cleanup Integration run passed
all five cases in 0.26s. Source Original/cancellation regressions passed two
tests and hook configuration/caller regressions passed six. The current selected
App strict passed with warnings denied for the production library and
MCP/publisher Integration targets. Library and native host scopes remain separate.
