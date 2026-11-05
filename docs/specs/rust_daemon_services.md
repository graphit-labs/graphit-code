---
status: implemented-adapters
integration-status: pending
session: ses-c98a
implementation: tsk-368b
integration: tsk-0529
references:
  - target: {type: task, id: tsk-1528}
    relation: implements
  - target: {type: task, id: tsk-368b, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-0529, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
  - target: {type: task, id: tsk-9d92, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: supports
---

# Serviços do usuário em Rust

O adaptador controla o serviço do usuário por systemd, LaunchAgent no domínio GUI ou Task Scheduler. O daemon continua headless; seus listeners, jobs, PID e descoberta pertencem à integração do entrypoint, não às definições de serviço.

A unidade systemd emite `KillMode=mixed`: o SIGTERM inicial chega somente ao
MainPID do daemon. Assim o Source cancela e aguarda os jobs, realiza o RPC
Models autenticado para selar/drenar os workers e só depois solicita Process
stop e aguarda o child. O `exec` do launcher mantém esse MainPID no Rust.
O SIGKILL posterior continua abrangendo o cgroup se o processo principal sair
com descendentes ou se o teto de stop do manager for atingido, conforme
[systemd.kill](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html).
O produtor não altera KillSignal, TimeoutStopSec nem suprime falhas reais de
transporte. `control-group` enviaria TERM simultâneo ao child, permitindo que
seu listener fechasse antes do RPC de drain do pai. A definição nova precisa
ser publicada pelo instalador; validar somente o documento não comprova o
stop da unidade já instalada.

Após `ForegroundSource::run` retornar `Ok(())`, inclusive seu cleanup,
`--managed` e `--detached` retornam exit0 no stop esperado. O cancelamento de
`daemon foreground` interativo conserva exit130.
Falhas de startup ou cleanup continuam seguindo o caminho de erro original;
o mapeamento de sucesso não captura `Interrupted` nem declara exit130 como
sucesso da unidade.

## Interfaces

Antes de compor o Source no Linux, a entrada dos modos managed, detached,
e foreground eleva o soft `RLIMIT_NOFILE` ao hard herdado usando rustix.
O hard permanece intacto, sem valor fixo, alteração do manager ou configuração
pessoal. Esse processo usa o reactor epoll do Tokio; descritores acima de 1023
não passam por `select(2)`. Falha de `setrlimit` interrompe o startup pelo erro
causal existente. Comandos de controle e MCP não executam esse ajuste.
O comportamento acompanha o executável e não exige republicar a unidade;
reiniciar com o binário atualizado é necessário para um daemon já ativo.
A promoção não substitui o fechamento dos handles ao final do trabalho.
Essa escolha segue a orientação de
[systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#LimitCPU=)
para aplicações que suportam descritores acima do limite de `select(2)`.

ServiceDefinition recebe `global_dir: PathBuf` absoluto obrigatório e gera a configuração per-user com o executável Graphit e argumentos `--global-dir <exato> daemon --managed`. O mesmo diretório é o WorkingDirectory nos três documentos; o cwd do utilitário de controle é uma operação distinta. Os paths do executável e do diretório precisam ser absolutos, Unicode e sem NUL; valores inválidos são rejeitados antes da publicação. Task Scheduler usa InteractiveToken e LeastPrivilege; seu trigger de login é separado e só existe quando solicitado, e executa `--global-dir <exato> daemon start`. LaunchAgent usa ProgramArguments em array e reinício após falha. Não há instalação SCM/admin ou migração de registros antigos.

ServiceCommands resolve o utilitário nativo no PATH com which. Ele recebe ProcessHost emprestado de um owner de serviço e Arc<SnapshotContext> original em cada capture/execute. O host usa Cancellation próprio; nenhum contexto da primeira requisição fica retido. Não há runtime adicional por chamada/projeto. UID do usuário atual preenche XDG_RUNTIME_DIR=/run/user/<uid> e DBUS_SESSION_BUS_ADDRESS somente quando ausentes; valores explícitos são conservados.

UserService controla start, stop, restart, status, instalação, login e uninstall. FileLock existente serializa mudanças de configuração sem remover/substituir o inode do lock. Configuração é publicada atomicamente por tempfile no próprio diretório. O erro de manager preserva Outcome/Failure do SDK e o owner de qualquer processo cuja cleanup ainda não terminou. Falha de rollback preserva também o erro original.

Status retorna a observação nativa; um código de saída não é convertido silenciosamente em “ausente”. A decisão de iniciar daemon detached quando o manager está indisponível e a interpretação de PID/listeners são jornadas da integração tsk-0529.

### Entry tipado da aplicação

`crates/graphit-app/src/service_entry.rs` expõe `ServiceCommand` para o parser central Clap com `start`, `stop`, `restart`, `status`, `install --login`, `login enable`, `login disable` e `uninstall`. `install` sem `--login` deixa o início no login desligado. O entry recebe `UserService` já composto e o `Arc<SnapshotContext>` finito da operação; não cria outro ProcessHost nem retém o Original da primeira requisição.

`graphit --global-dir <instalação> daemon install` publica novamente o documento
atual e executa `daemon-reload`; `--login` conserva a escolha explícita de iniciar
no login. `daemon start` só instala quando não há registro e não reescreve uma
unidade existente. Após atualizar o binário, a nova política de sinais depende
dessa publicação pelo comando de instalação, seguida da verificação do manager
e do stop real; não basta apenas iniciar a unidade antiga.

`execute` retorna `ServiceResult::Completed` ou `ServiceResult::Status(Outcome)`. Status conserva exit status, transcript, cleanup e recursos pendentes do SDK. As demais ações retornam o `ServiceFailure` causal quando o manager falha; essa falha não autoriza iniciar outro processo. Cancelamento é verificado antes dos efeitos, incluindo publicação de registro.

A composição produtiva mantém paths e ambiente capturados, host de processos, singleton e ForegroundSource reais. Ela distingue manager realmente indisponível de manager disponível que rejeitou um comando. No Linux, somente a primeira condição permite o daemon detached. Stop comunica o manager antes de drenar e juntar os owners; restart aguarda liberação do singleton. `--global-dir <exato> daemon --managed` e o `daemon start` usado pelo login Windows usam o mesmo parser central. A qualificação produtiva Linux, incluindo replacement, está discriminada abaixo. Task Scheduler e launchd ainda precisam das respectivas jornadas nativas; o entry isolado não demonstra esses fluxos.

### Composição do launcher

`crates/graphit-app/src/daemon_launcher.rs` compõe `UserService` com o
`ProcessHost` emprestado da invocação, executável absoluto, instalação e ambiente
capturados. O nome combina a marca com SHA256 do caminho literal da instalação;
instalações distintas têm registros distintos. A definição conserva `global_dir`
exato e o diretório de trabalho é a instalação. O Original finito é verificado
durante a composição e recebido novamente em cada operação do entry.

Esses documentos são registros privados do sistema operacional, gerados para a
instalação local; não são configurações de projeto ou artefatos exportáveis.
O entry captura `std::env::current_exe()` na invocação e recebe a instalação
resolvida por `InstallationPaths::resolve`; `compose_service` usa esses valores,
sem caminho de usuário ou de máquina fixado nas fontes. A portabilidade das
configurações de projeto compartilhadas exige o executável no PATH e caminhos
relativos ao projeto. Ao
mudar de máquina ou instalação, recrie o registro com `daemon install` no destino,
em vez de copiar o descritor local. Essa distinção preserva os paths absolutos
exigidos pelo SDK e a semântica própria de systemd, launchd e Task Scheduler.

No Linux, o registro fica em `XDG_CONFIG_HOME/systemd/user`, com fallback para
`HOME/.config` do ambiente capturado. Launchd recebe um plist privado na
instalação e um registro de login separado em `HOME/Library/LaunchAgents`.
Task Scheduler recebe XMLs separados para serviço e login na instalação. A conta
vem do sistema operacional; o UID Unix vem de rustix e Windows usa a conta
nativa completa.

`manager_availability` consulta o proprietário DBus de
`org.freedesktop.systemd1` usando zbus e o endereço do ambiente capturado.
`NameHasOwner=false`, conexão recusada e socket ausente representam
indisponibilidade. Falhas de autenticação/protocolo continuam erros causais.
Cancelamento e deadline do Original cobrem a consulta. Essa observação precede
a decisão de lançamento; o exit status de um comando do manager não concede
fallback.

`execute` usa o Original finito e um cliente HTTP sem proxy nem redirecionamento.
Start reutiliza Running autenticado antes de consultar o manager. Quando o
manager está disponível, assegura o registro pelo SDK somente se ausente,
instala sem login e inicia; um registro existente não é reescrito e seu login
é preservado. `UserService::start` continua iniciar apenas um serviço registrado.
Falha do manager disponível não seleciona detached. No Linux sem manager,
o SDK detached inicia o executável absoluto com `--global-dir <exato> daemon
--detached` e CWD da instalação. A readiness autentica status e exige PID igual
ao processo possuído antes do commit; uma falha cancela/junta e conserva Remaining
quando cleanup não termina. Stop obtém ACK e aguarda liberação do mesmo inode
do singleton. Registro/login com manager indisponível retorna erro causal.

O contrato produtivo de autostart garante daemon antes de comandos elegíveis
que usam Source, depois dos guards de configuração/seleção/ROLE. PROJECT faz
MaintainCheckout real com `select=false` antes RPC; global/contextos não
inventam PROJECT. Managed/foreground, observação/status/stop/retire,
Auth/bootstrap/config, operações locais e Role transport ficam excluídos.
Foreground permanece explícito. Now usa Original finito somente no startup,
preservando o Original do stream. Essa ligação pertence aos owners Root/Source;
seus testes produtivos são necessários para demonstrar a jornada.

## Serialização e execução

Os valores emitidos em XML de Launchd/Task Scheduler precisam pertencer ao conjunto [Char de XML 1.0](https://www.w3.org/TR/xml/#charsets). Caracteres proibidos são recusados com InvalidInput antes de atomic_write e da chamada ao manager, incluindo controles C0 fora TAB/LF/CR e U+FFFE/U+FFFF. A checagem XML de user aplica-se apenas ao Task Scheduler, pois Launchd usa a identidade GUI por UID e não emite esse campo. As restrições já existentes do nome e da identidade do usuário continuam aplicáveis. A validação não implementa outro escapador: [quick-xml 0.42.0 escape](https://docs.rs/quick-xml/0.42.0/quick_xml/escape/fn.escape.html) preserva CR legítimo nos paths com referência numérica, além dos escapes XML usuais. Assim CR não é rejeitado nos paths XML por hipótese de normalização; systemd mantém a regra própria de WorkingDirectory abaixo.

XML/plist usa quick-xml. A string ExecStart usa serde_json para os escapes C aceitos pelo systemd. O prefixo : desativa expansão de variáveis; %% representa % literal. O próprio systemd rejeita aspas e backslash no primeiro executable path. O sh do SO, resolvido por which, executa o programa constante `exec "$@"`; o executável e os argumentos chegam em argv separados. O shell se substitui pelo executável e conserva o PID. Nenhum dado é incorporado ao código shell e nenhum parser/escapador próprio é usado. A invocação segue [sh -c do POSIX](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/sh.html). Isso também preserva caminhos com `=`, que env interpretaria como atribuições.

O [manual de serviço](https://github.com/systemd/systemd/blob/main/man/systemd.service.xml) documenta o prefixo, e a [sintaxe](https://github.com/systemd/systemd/blob/main/man/systemd.syntax.xml) define os escapes e UTF-8. O [código do systemd](https://github.com/systemd/systemd/blob/v255/src/core/load-fragment.c) aplica string_is_safe ao executable path. [which](https://docs.rs/which/latest/which/) resolve o executável e [tempfile](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html) publica a configuração atomicamente no mesmo filesystem.

## Estado da validação

`WorkingDirectory` do systemd usa o caminho literal e `%%` para o percentual,
separadamente do quoting de argv. O [parser da diretiva](https://github.com/systemd/systemd/blob/v255/src/core/load-fragment.c#L2400)
não remove aspas. O [leitor de unidades](https://github.com/systemd/systemd/blob/v255/src/shared/conf-parser.c#L155)
remove whitespace final e continua linhas terminadas em quantidade ímpar de
barras. CR/LF, whitespace ASCII final e barra final ímpar são rejeitados antes
da publicação para evitar um CWD diferente. Espaços, aspas e barras internos
permanecem literais. ExecStart mantém seu serializer de argv.

O argumento Windows usa `shell_escape::windows::escape` de shell-escape 0.1.5 antes do escape XML, sem executar cmd.exe ou expandir `%VAR%`. A [implementação da biblioteca](https://docs.rs/shell-escape/0.1.5/src/shell_escape/lib.rs.html) conserva aspas e barras conforme as [regras de argv da Microsoft](https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments?view=msvc-170). O teste usa winsplit 0.1.0 como parser independente após decodificar XML, cobrindo espaços, aspas, percentual, Unicode e barra final; o round-trip portátil não qualifica execução nativa Windows.

### Controle autenticado e composição

O contrato do controle produtivo exige caller verdadeiro e `RuntimeConfigure` unconditional da instalação atual. Discovery/PID e liveness não concedem essa autoridade. Origin inadequado e headers ambíguos são negados; status observa o PID autenticado do processo, não um número confiado apenas pelo arquivo. Stop cancela o serviço e o transporte mantém a entrega do ACK autenticado até o deadline Original da requisição, antes de concluir o drain/join e a retirada dos recursos. A composição Root mantém esse router/controller, e o launcher empresta o host para usar o UserService existente. Availability Linux usa probe D-Bus madura, não stderr nem exit genérico. A implementação e fixture dos owners precisam qualificar essas obrigações antes do aceite.

A fixture Linux usa um registro runtime com nome único, arquivos privados e cleanup próprio; nunca reinicia o daemon pessoal. Ela verifica instalação, start/status, reinício com novo PID, alteração de login, uninstall e argv literal com espaço, aspas, dólar, percentual, backslash, igual e Unicode. Essa fixture representa o adaptador de serviço, não o daemon Rust funcional.

Os testes padrão de `daemon_service_entry` para falha do manager resolvem `systemctl` no PATH privado de `app-runtime-fixture`, preparado pelo Cargo. O processo controlado verifica os argumentos reais de `show` e `start`, retorna código 69 e stderr próprio, e o consumidor deve conservar status e cleanup. Cancelamento antes de instalar também conserva o caso seguinte. A execução do manager de usuário real permanece na qualificação explícita já mantida.

Após a inclusão de global_dir/WorkingDirectory, `cargo test --locked --offline -p graphit-daemon --lib services::` passou em Linux com sete testes e um ignorado. O Clippy afetado `cargo clippy --locked --offline -p graphit-daemon --lib --tests --no-deps -- -D warnings` também passou. A cobertura inclui os documentos, round-trip Windows independente, rejeição de global_dir relativo/NUL, locks, restauração do registro quando o manager falha e cancelamento antes da publicação. A fixture positiva systemd é opt-in porque exige um manager do usuário ativo: `cargo test --locked --offline -p graphit-daemon --lib native_systemd_execstart -- --ignored`. O caso ignorado não é prova de startup positivo. Windows/macOS ainda precisam de execução nativa; XML/plist/build não demonstram esses fluxos. A folha tsk-0529 mantém singleton/PID/discovery, fallback detached, jobs/listeners/children e replacement reais; tsk-cab0 depende dela.

O teste focal do entry é `cargo test --locked --offline -p graphit-app --test daemon_service_entry`. Os dois casos do manager privado passaram, sem ignorados. O novo caso `compiled_foreground_singleton_discovery_sigterm_and_stale_restart` também passou (0,14 s): usa o executável Rust real, singleton/PID/discovery, recusa double-start, reinicia após publisher morto e verifica SIGTERM/exit130/join/remoção do record/liberação do lock. O Clippy isolado da fixture passou. Essa prova precede a nova composição managed/global-dir e não qualifica detached, stop remoto, manager positivo ou plataformas sem host. Os testes/Clippy de global-dir acima precedem a correção de WorkingDirectory; sua revalidação está discriminada abaixo.

Implementação: `crates/graphit-daemon/src/services.rs`, `crates/graphit-daemon/src/control.rs` e `crates/graphit-app/src/service_entry.rs`. Operação: [Daemon Operations](../guides/daemon_operations.md).

Na qualificação histórica, `compiled_managed_empty_inventory_outside_project_maintains_and_retires_real_checkout` passou com o Main real (2,19 s). Inicializa apenas a instalação pelo SDK, inicia `--global-dir <exato> daemon --managed` de um CWD sem PROJECT e remove esse diretório. O CLI `projects maintain-checkout` então percorre HTTP e o produtor Root de trust/bootstrap para um checkout real, e `projects retire-checkout` retorna ao estado vazio. Verificou os efeitos observados e SIGTERM/exit130/remoção da discovery. Esse PASS precede a separação do mapeamento de saída: a fixture managed atual exige exit0 e precisa da reexecução com o CLI correspondente. Nenhum PROJECT ou autoridade foi fabricado para iniciar o daemon; a prova não substitui launcher/detached/controle remoto nem plataformas sem host.

O Clippy da fixture managed também passou. O owner do launcher qualificou
separadamente a execução com manager indisponível, processo Rust compilado,
argv/CWD, Start/Status/Restart com PID distinto/Stop e singleton livre
(um caso, 4,16 s); esse consumidor chama a API App e não os comandos CLI.

A fixture systemd nativa executou um caso e falhou em start (Manager exit 1,
cleanup completo); o histórico permanece. Após a correção, oito testes do SDK
passaram, incluindo preservação/rejeição da diretiva. O caso nativo passou
separadamente (2,47 s), com instalação/start/status/restart/PID novo, argv
literal, CWD observado pelo próprio executável com `pwd -P`, login enable/disable,
uninstall e HostJoined. A leitura de `/proc/<pid>/cwd` foi negada pelo host;
caller e manager tinham UID 1000. O guard preserva unidade/instalação e
diagnóstico se não confirmar encerramento. Falhas intermediárias de compilação
e o oracle anterior de `/proc` permanecem no histórico; não são passes.
Os novos casos
`compiled_direct_commands_without_user_manager` e
`compiled_direct_commands_with_user_manager` usam os comandos do binário real
com instalações privadas; o segundo é opt-in e exige o manager real. Eles
verificam PID do Start autenticado, status nativo com manager ou autenticado sem
manager, stop, singleton e preservação de registro/login existente. O caso sem
manager passou no binário real. O primeiro oracle do caso com manager exigiu
JSON de status incorretamente; a API conserva stdout/stderr/exit nativos nessa
condição. Esse caso foi alinhado ao contrato e passou separadamente (9,10 s):
Start registra/inicia quando ausente, status MainPID/ActiveState corresponde ao
PID autenticado, Start reutiliza Running, Stop remove discovery e libera o
singleton. Após habilitar login da unidade única, Start com registro existente
preserva bytes/mtime e estado do login; Stop/Uninstall removem o registro.
Somente o caso com manager foi repetido; a prova sem manager foi conservada.
O comando focal é
`cargo test --locked --offline -p graphit-app --test daemon_service_entry compiled_direct_commands -- --include-ignored`.

O Clippy final dos targets afetados passou: SDK
`cargo clippy --locked --offline -p graphit-daemon --lib --tests --no-deps -- -D warnings`
(16,91 s) e Main
`cargo clippy --locked --offline -p graphit-app --test daemon_service_entry --no-deps -- -D warnings`
(2 min 24 s). Fmt e diff-check dos paths mantidos também passaram. Esses
resultados iniciais não qualificam execução nativa Windows/macOS e precedem
o replacement Linux posteriormente aprovado em native04/native05.

O `strict26` de `tsk-9d92` também aprovou `graphit-app --lib --tests` com
warnings negados (61,491 s), incluindo a composição final de ACK/drain. Essas
provas válidas e os resultados Linux acima são reutilizados, sem repetir a
campanha para encerrar um registro. Na `tsk-0529`, qualidade de Source/docs e
as duas jornadas de SO têm checks separados.

A fixture `daemon_service_entry.rs` é exclusiva de Linux. Executá-la em outro
SO não comprova Task Scheduler nem launchd. Em Windows, a jornada pendente usa
Main/CLI e Task Scheduler reais com instalação e nomes privados: registro,
start/status com PID autenticado, restart/substituição, stop/drain, liberação do
singleton, login/uninstall e argv/global_dir exatos. O encerramento de processos
precisa da prova Windows pertinente, preservando causas e recursos pendentes.
Em macOS, os mesmos efeitos precisam de Main/CLI e launchd no domínio per-user
real, com instalação e labels privados. Nenhuma dessas jornadas altera serviços
pessoais; ausência do host mantém a respectiva cobertura pendente. XML, plist,
crosscompile e execução Linux não substituem essas observações.

Na recuperação da fixture nativa, um stop/disable incerto mantém registro e
root. Se o registro já foi removido e o `daemon-reload` falhar depois, o root
é preservado e a remoção é um efeito conhecido; não há afirmação de rollback
do arquivo. O diagnóstico e a unidade única permitem retomar a limpeza.

## Configuração de parsers e prontidão

A extensão de parsers usa imagens Docker em `ast.scip.parsers`. A resolução e o
cancelamento de cada indexador pertencem ao pipeline SCIP; mudanças dos arquivos
de configuração usam os controles existentes e não exigem um processo pai,
worker interno ou exit especial para trocar bibliotecas. O fallback Tree-sitter/
ANTLR usa parsers compilados no pacote. O encerramento normal continua drenando
servidores, jobs, processos e descendentes pelos owners existentes.

A leitura de discovery distingue ausência revalidada de observação alterada durante
a leitura ou revalidação exata. Core sinaliza essa mudança com ObservationChanged
dentro de io::Error, conservando a causa de I/O quando existe; Discovery mantém a
variante tipada. Somente a espera de readiness descarta essa observação e repete a
leitura/autenticação, respeitando o mesmo Original, deadline e espera existentes.
JSON ou record inválidos, permissões, symlink, diferenças observadas de acesso e
conflitos de escopo continuam erros terminais; InvalidData genérico não autoriza
repetição. Nenhum erro de observação concede autoridade ou significa ausência.
