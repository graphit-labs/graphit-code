---
title: Now e instâncias no Source Rust
type: specification
status: source-qualified
references:
  - target: {type: task, id: tsk-b9b4}
    relation: derived_from
  - target: {type: task, id: tsk-7f8c}
    relation: implements
  - target: {type: task, id: tsk-b98d}
    relation: uses_api
  - target: {type: task, id: tsk-2f38}
    relation: uses_api
  - target: {type: task, id: tsk-60d0}
    relation: uses_api
  - target: {type: task, id: tsk-d1da}
    relation: uses_api
  - target: {type: task, id: tsk-5020}
    relation: uses_api
  - target: {type: task, id: 01M5C6B85MCWH5HX932YVEKN3Z}
    relation: implements
---

# Now e instâncias no Source Rust

O Source Rust expõe `graphit_now` e `graphit_instances` por MCP HTTP e pelo
proxy STDIO. O nome segue a marca configurada. O endpoint MCP Role usa as
mesmas tools quando o grant atual permite a operação. A implementação está em
`crates/graphit-app/src/source_server/now.rs`; o cliente de stream está em
`crates/graphit-app/src/now_transport.rs`. Esta composição depende dos produtores
[Now PROJECT](rust_runtime_project_now.md) e
[subscriber Broker](rust_runtime_now_subscription.md).

No Linux, o Foreground anexa esse serviço no startup do Source antes dos clones e do serving, inclusive sem publicação Broker e sem configuração Browser. `None` no owner opcional de verifier é legítimo para modo local. O Runtime Root tem permissões Task ordinárias explícitas; cada seção mantém sua admissão própria. Broker/instâncias, Browser/UI e Windows/macOS conservam seus próprios escopos.

## Tools finitas

`graphit_now` recebe `mode` obrigatório, `project_id` canônico e os seletores
abaixo. Campos desconhecidos, strings vazias e opcionais com `null` são rejeitados.

| Operação | Obrigatórios | Opcionais | Resultado |
| --- | --- | --- | --- |
| `graphit_now`, `mode: "local"` | `project_id`, `version` | nenhum | Now local da versão mantida |
| `graphit_now`, `mode: "broker"` | `project_id`, `provider` | `version`, `instance_id` | primeiro snapshot remoto completo |
| `graphit_instances` | `project_id`, `provider` | `version`, `instance_id` | primeiro snapshot remoto completo |

`graphit_instances` não recebe `mode`. Os filtros Broker são literais. Local
seleciona o tuple ID/versão já mantido pelo daemon; não aceita um caminho do
caller e não procura uma versão por Git. O resultado local conserva os estados
`available`, `no_corpus` e `failed` dos produtores. As permissões de Task, Memory
e Knowledge são avaliadas independentemente; Task e sessões compartilham a
admissão Task. Falta de uma permissão não elimina as outras seções autorizadas.

O schema anunciado em `tools/list` expõe os cinco campos no mesmo objeto, com
`project_id` e `mode` obrigatórios e `mode` limitado a `local`/`broker`. As
descrições conservam as regras por modo, aplicadas pelo decoder estrito: Local
exige `version` explícita e proíbe `provider`/`instance_id`; Broker exige
`provider`. Os campos de seleção não dependem de propriedades herdadas por
branches `oneOf` para aparecer no binding do host. A versão capturada pelo proxy
STDIO não preenche esse argumento Now.
Por exemplo, a chamada local usa
`{"project_id":"<ID canônico>","mode":"local","version":"main"}`;
omitir `version` retorna `now_invalid_arguments`.

Cada tool cria uma única Original finita com o timeout já configurado do Source.
A operação Broker encerra seu subscriber depois do primeiro snapshot, antes de
publicar o resultado. O proxy STDIO encaminha os argumentos diretamente com o
SDK MCP, mantendo as tools Source e Ask já existentes.

## Autenticação e seleção

Os handlers usam o issuer, os projetos mantidos e o cache de verificadores do
daemon existente. Runtime e Local são autenticados por suas credenciais reais;
Role mantém reference, bearer, run, grant e escopo atuais. O dispatcher Role
intercepta a tool antes de criar uma segunda Original. O catálogo Role anuncia
Now local quando há alguma permissão de leitura das três seções e anuncia
instâncias apenas com `RemoteRequest`. Uma chamada direta ainda passa pelos
mesmos checks de autorização.

Broker requer `RemoteRequest` e uma seleção de serviço explícita `PresenceRead`.
Native usa `BrokerVerifierCache::get` e `native_credential`; Local usa a
credencial local selecionada. Não há fallback entre esses modos. O tuple Role
selecionado precisa corresponder ao projeto e à versão solicitados. Nenhum
selector, snapshot DATA ou JWT válido concede permissão PROJECT por si só.
As admissões e a identidade física do projeto são revalidadas antes de emitir
resultados. O wire contém apenas os campos Now aprovados, sem perfil, segredo,
caminho de origem ou corpo integral de registros. ID da instância identifica
uma instância online; não identifica uma pessoa.

## Stream HTTP

`POST /api/now/subscribe` recebe JSON strict
`{ "project_id": "…", "provider": "…", "version": "…", "instance_id": "…" }`.
`version` e `instance_id` podem ser omitidos. O ingresso conserva Authorization,
origem de credencial, Host loopback e rejeição de Origin externo. Credenciais
não passam na URL. A resposta usa `application/x-ndjson`, `Cache-Control: no-store`
e `X-Content-Type-Options: nosniff`. Falhas antes da primeira entrega recebem
status HTTP finito e categoria sanitizada; falhas posteriores encerram o corpo.

Cada linha é um `RemoteNowSnapshot` completo validado pelo subscriber atual.
`instances: []` substitui a visão anterior. O stream cria uma Original service
cancelável, cujo deadline é `None` real. A validade da lease/JWT continua sendo
verificada pelo subscriber e imediatamente antes do enqueue. Esta mudança não
estende o token nem altera TTL: a duração do access token no Broker permanece
10 minutos. Revogação, bloqueio da conta e alteração dos claims podem ser
percebidos somente na expiração do access token, conforme a autenticação JWT
local. PROJECT continua sujeito à autorização própria do serviço.

A fila HTTP possui um slot. Enquanto esse slot está ocupado, o subscriber
continua processando frames, heartbeat e validade da lease. Conserva apenas
um snapshot pendente, substituído pelo snapshot completo mais recente, e
serializa quando a saída pode recebê-lo. Frames intermediários podem ser
coalescidos sob backpressure; o stream representa a visão atual. Não mantém
histórico nem faz polling local. Falha terminal cancela a Original e descarta
snapshots que ainda estavam na saída. Drop do corpo cancela somente sua
requisição. O owner mantém os jobs até o encerramento e join; cancelar um
waiter de shutdown não descarta esse owner.

`connect_now` lê a chave Runtime pelos limites existentes e usa
`reqwest::Response::bytes_stream`, `StreamReader`, `FramedRead` e `LinesCodec`.
`NowStream::next` retorna `RemoteNowSnapshot`, validado por
`decode_now_snapshot` do daemon com projeto e filtros esperados. O cap de linha
vem de `ContentLimits.max_output_bytes`. UTF-8, JSON, envelope, projeto, filtro
ou tamanho inválido encerra o corpo no primeiro erro. EOF, cancelamento e prazo
finito encerram a leitura; uma Original service não recebe prazo artificial.
O cliente não conserva a visão antiga nem cria jobs de background. Um renderer
CLI pode consumir essa API e substituir sua visão a cada snapshot; o renderer
dinâmico tem entrega separada.

## Verificação das superfícies

As suites focadas são `tests/now_tools.rs`, `tests/now_transport.rs` e os testes
privados `source_server::now::tests`. Elas exercitam Auth/project/Role reais,
catálogo e chamadas MCP, module denials, seleção estrita, primeira snapshot e
close, substituição vazia, fragmentação Unicode, frames inválidos, prazo,
cancelamento, body drop e ownership de shutdown. O servidor de protocolo
compartilhado é isolado e explícito; não representa uma prova de autorização
Broker SQL/TLS. Os checks internos de subscriber, codec e cache Native usam
as entregas qualificadas dos seus produtores. A validação desta composição
é Linux; outras plataformas precisam de seus respectivos consumidores.

A qualificação focada registra 16 funções Now distintas aprovadas: sete em
`now_tools`, quatro em `now_transport` e cinco nos testes privados Now. A
correção do I/O de handshake também passou nas seis regressões existentes dos
clientes Source/STDIO. Clippy da biblioteca App e dessas quatro suites passou
com `--no-deps -D warnings`; formatação e os links do tópico foram conferidos.
O receipt fica em `.build/rust-source-now/qualification.json`.
