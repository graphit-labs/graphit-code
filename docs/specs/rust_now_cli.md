---
title: CLI Now contínua em Rust
type: specification
status: linux-cli-qualified
references:
  - target: {type: task, id: tsk-9e52}
    relation: implements
  - target: {type: task, id: tsk-7f8c}
    relation: uses_api
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# CLI Now contínua em Rust

`graphit now` acompanha as instâncias online no Broker através do daemon Source
ativo na instalação selecionada. O projeto é um ID canônico explícito; o comando
não escolhe um projeto pelo diretório atual.

```sh
graphit now --project 01ARZ3NDEKTSV4RRFFQ69G5FAV --provider native
graphit now --project 01ARZ3NDEKTSV4RRFFQ69G5FAV --provider native --version main --instance a
graphit --global-dir /path/to/installation now --project 01ARZ3NDEKTSV4RRFFQ69G5FAV --provider native | consumer
```

`--project` e `--provider` são obrigatórios. `--version` e `--instance` são filtros
literais da visão Broker, sem conferir permissões. `--project-version` não
substitui o filtro `--version`. A política de conteúdo vem da configuração
distribution/global/environment/command já existente; o comando não carrega
configuração de um projeto encontrado por cwd. A marca compilada define o nome
do executável e a instalação conforme as regras normais da CLI.

## Saída e vida da conexão

Com stdout em pipe, cada linha é um snapshot JSON completo validado pelo
[transporte Source](rust_source_now.md), com flush imediato. `instances: []`
substitui integralmente a visão anterior. Os timestamps decimais continuam
strings e os contadores mantêm precisão inteira. Não há cabeçalhos, ANSI,
histórico, deduplicação ou conversão para ponto flutuante. Em Unix, a escrita de pipe
usa Tokio `AsyncFd` e Rustix: captura um `OwnedFd` com close-on-exec, altera
`O_NONBLOCK` somente quando necessário e restaura a flag ao terminar. Espera de
writable e escrita parcial usam a mesma Original; SIGTERM também encerra um
pipe aberto cujo reader parou de consumir. Um único buffer JSON mantém o
progresso de escrita sem copiar DTOs. Redirecionamento para arquivo regular
serializa diretamente no arquivo. Cancelamento durante uma escrita pode deixar
bytes finais sem newline; o consumer descarta esse record incompleto.

O backend de stdout Windows duplica o handle emprestado com OwnedHandle e usa
uma thread exclusiva por invocação para a escrita síncrona do destino herdado.
Cada frame é movido para esse owner; o consumidor aguarda sua conclusão antes
de enviar o próximo, sem reter snapshots anteriores. Cancelamento ou deadline
da mesma Original aciona CancelSynchronousIo somente nessa thread e aguarda
seu join. Fechar o canal também acorda o worker ocioso. O guard executa esse
encerramento ao ser descartado; descartar uma future de Tokio stdout não seria
suficiente para interromper uma escrita bloqueada.

A corrida entre o teste de parada e o início do syscall é tratada sem abandonar
o owner: ERROR_NOT_FOUND não prova que a thread terminou. Nesse caso, a espera
nativa pelo término da thread precede nova tentativa de cancelar o IO físico.
Após cancelamento aceito, o join confirma o término. Isso não consulta snapshots
periodicamente nem introduz deadline ou quota para a sessão. As flags do handle
herdado não são alteradas. Uma falha pode deixar o último record incompleto,
com a mesma regra de descarte do consumer usada no backend Unix.

No terminal, o comando usa Crossterm apenas para alternate screen e cursor/clear.
A visão mantém o contexto no topo, respeitando a geometria atual do terminal.
Conteúdo que excede a altura/largura mostra um aviso para usar pipe e ler o
snapshot completo; o corte respeita UTF-8 e largura Unicode via unicode-width.
A visão mostra projeto/filtros, instâncias/versões, disponibilidade e contadores
das quatro seções e títulos dos itens. Cada snapshot substitui a tela completa,
inclusive quando fica vazio. Controles C0/C1/DEL/ESC e caracteres bidi são
escapados como `\u{001b}`; Unicode legível permanece. Não usa raw input nem oculta
o cursor. Ao sair ou falhar, restaura a tela principal do terminal.

A CLI mantém uma única conexão ao Source e uma Original service cancelável,
com deadline `None`. Não há timeout arbitrário, polling ou reconexão automática.
Um caller da API `now_entry::run_pipe` / `run_terminal` também pode fornecer uma Original finita, que
é preservada por discovery, conexão e leitura. SIGINT e SIGTERM acionam a
cancelabilidade existente (`ctrlc`, com `termination`), terminam o consumo e
retornam 130. Pipe interrompido encerra sem erro adicional e retorna 0. Falhas
de configuração, autenticação, permissão, protocolo ou conexão retornam 1;
flags inválidas retornam o código do parser. EOF encerra com diagnóstico finito
e não tenta abrir uma segunda conexão. Erros não imprimem causas de rede,
credenciais, paths ou corpo das respostas.

Ao retirar o corpo HTTP, o dono Source cancela e junta seu subscriber conforme
o contrato já qualificado. A CLI não cria outro owner, credencial, AuthStore ou
assinatura WS; reutiliza discovery, runtime headers e `connect_now`/`next`.

## Autenticação e autorização

O daemon e o Broker mantêm o perfil selecionado, a admissão `PresenceRead` e a
autorização PROJECT separadas. JWT válido não concede PROJECT. A validação JWT
local continua confiando nos claims até a expiração assinada; revogação,
bloqueio e alterações de claims podem demorar até essa expiração para aparecer.
O access token Broker permanece com a duração existente de 10 minutos, sem
novo TTL pela CLI. A conexão encerra se sua lease/autorização falha. Para
retomar, faça login/refresh quando necessário e execute novamente o comando.

## Implementação e qualificação

Fontes: `crates/graphit-app/src/now_entry.rs`, `now_renderer.rs`, `now_transport.rs`
e o dispatch Now em `cli.rs`. O teste `tests/now_executable.rs` usa o executável
compilado, Auth/Profile/Source reais e publicação de discovery pelo owner real.
Seu peer WS valida o protocolo Broker; não qualifica SQL/TLS do Broker. A PTY
Linux vem da biblioteca padrão Python, sem simulador de terminal ou API pública
de teste de autoridade. A suite afetada é
`cargo test --locked --offline -p graphit-app --test now_executable`.
A CLI Now e o IO cancelável deste contrato estão habilitados no Linux.
Resultados Linux não implicam execução em Windows/macOS. A integração de stdout
Windows em now_entry é parte da composição de plataforma em cab0; a habilitação
da CLI/Source pelo wiring comum e a execução em host Windows permanecem
qualificações separadas. Os testes específicos usam arquivo e pipe reais,
incluindo testemunha GetThreadIOPendingFlag antes do cancelamento e do Drop;
sem host Windows, esses testes são registrados como não executados. A ligação de
`render` com cancelamento/deadline da Original foi inspecionada; os cinco testes
Windows do backend exercitam o worker e seu encerramento diretamente.

## Evidência atual Linux

Sete casos funcionais distintos passaram: seis em `tests05.log`, incluindo
SIGTERM com pipe aberto que parou de ler, e restauração de flags blocking / já
nonblocking em `flags06.log`. Após o ajuste exclusivamente visual, o caso PTY
atual em `pty09.log` passou com geometria 80×24 real: contexto no topo, aviso de
conteúdo restante, snapshot vazio substituindo o anterior e tela principal
restaurada após SIGTERM. Essa PTY valida o executável recompilado; não reaproveita
um binário anterior ao ajuste. Os casos de IO que não mudaram foram preservados.

Clippy `--bin graphit --test now_executable --no-deps -- -D warnings` passou em
`strict10.log`. O guardrail existente seleciona apenas
`graphit-app/now_executable`: teste com status 0 retorna 0; status injetado 7
retorna 1. O mesmo runner verificou o hook Git real em diretórios temporários:
formatação incorreta bloqueia a próxima ação; formatação corrigida permite.
Nenhum commit/push foi feito no workspace e nenhuma suite ampla foi executada.
Receipts relativos ao projeto estão em `.build/rust-now-cli/qualification.json`
e `.build/rust-now-cli/guard-receipt.json`; as primeiras falhas de fixture, IO
bloqueante, ENOSPC e erro estrangeiro de Data permanecem nos logs históricos.
