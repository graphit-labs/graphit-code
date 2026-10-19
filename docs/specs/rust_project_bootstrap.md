---
title: Bootstrap de projeto Runtime Rust
content-type: reference
audience: developers
description: Observação do checkout e registro da seleção no inventário Core.
references:
  - target: {type: task, id: tsk-9d34}
    relation: corrects
  - target: {type: task, id: tsk-bcf2}
    relation: uses_api
  - target: {type: task, id: tsk-7047}
    relation: implements
  - target: {type: task, id: tsk-a437}
    relation: uses
  - target: {type: task, id: tsk-e074}
    relation: uses
---

# Bootstrap de projeto Runtime Rust

O módulo graphit-daemon::project_bootstrap observa um checkout existente e registra a versão selecionada no inventário local do Core. Seu caller deve fornecer ProjectPaths, seleção explícita/pin/base detached, limites físicos do Core, GlobalProjectInventory, PrivateInstallation já preparada e o Arc<SnapshotContext> original.

A identidade persistente deve existir e permitir stores persistentes. O bootstrap não cria nem repara identidade, instalação, autenticação ou configuração. PrivateInstallation é uma observação física ligada ao request, sem autoridade de login. Consulte [inventário Core](rust_project_inventory.md), [identidade](project_identity.md) e [lifecycle do daemon](rust_daemon_memory_lifecycle.md).

## API e ordem

ProjectBootstrapInput possui project, explicit_version, pin e detached_base emprestados. ProjectBootstrapLimits possui scope: ScopeLimits e native: NativeDocumentFileLimits; não estabelece defaults ou novas reservas.

bootstrap_project(input, limits, &store, installation, &original_request) executa:

1. Core `required_deadline` valida cancel/expiry e exige o deadline original finito antes de revalidar a instalação; compara brands antes do Git.
2. GitCheckout::inspect com tempo restante e cancellation original. Checa o request original depois do SDK, inclusive quando a inspeção falha.
3. VersionContext::select: explícito, pin do mesmo escopo, branch, base detached ou default. Pin de outro escopo é ignorado pela política Core.
4. CapturedProjectInventory::capture com checkout, seleção, limites e Arc original.
5. Writer Core e register_checkout com a captura real. O writer libera lock e instalação por RAII antes de retornar.

BootstrappedProject fornece version(), checkout() e disposition(). InventoryRegistration::Registered representa alteração confirmada pelo Core; Unchanged representa no-op. A saída contém somente dados owned e getters emprestados, sem captura, FD, request, grant, ProjectId inventado ou autoridade de supervisor.

Nova observação após mudar checkout ou seleção atualiza o checkout e mantém versões anteriores. Esta API não executa git checkout, não escreve pins e não infere versões mantidas de Git refs. Retenção e retirada dependem de fatos dos respectivos domínios; park/offline não representam remoção do inventário.

## Falhas e limites

ProjectBootstrapError preserva a fase e fonte real: Request(io), Installation(AuthDocumentError físico do Core), Checkout(io), Selection(io), Capture(io) ou Inventory(io). source() preserva a fonte e Display oferece diagnóstico local; isso não autoriza exportar erros ou caminhos a serviços remotos.

Contexto de serviço sem deadline retorna Request/InvalidInput antes de revalidação ou Git. Cancel/expiry inicial impedem IO. Instalação de outro request é rejeitada, brand divergente é Capture/InvalidInput, identidade ausente/ephemeral e limites físicos falham em Capture, versão inválida em Selection, Git inválido em Checkout. Erros de inventário permanecem Inventory. O Core valida antes da publicação atômica; não há novo check depois de commit confirmado convertendo sucesso em falha ambígua. Esta API não promete rollback multistore.

Git usa budget relativo cooperativo e IO do sistema operacional. Os fences do request original impedem publicação após falha do request observado; não comprovam hard deadline, preempção/drain nativo, power-loss ou atomicidade contra filesystem hostil.

## Verificação e integração

Invocações portáveis a partir da raiz do workspace:

~~~text
cargo test --locked --offline -j1 -p graphit-daemon --lib project_bootstrap
cargo test --locked --offline -j1 -p graphit-daemon --doc project_bootstrap
cargo clippy --locked --offline -j1 -p graphit-daemon --all-targets -- -D warnings
rustfmt --edition 2024 --check crates/graphit-daemon/src/project_bootstrap.rs crates/graphit-daemon/src/project_bootstrap/tests.rs
~~~

Testes usam Git e SDKs Core reais em diretórios temporários: A/B, dois worktrees com identidade compartilhada, branch/pin/explicit/foreign pin/detached/NonGit, no-op sem rewrite, switch com histórico, cancel/expiry/foreign request, missing/ephemeral identity, brand/installation divergentes, instalação substituída, identidade corrompida, caps físicos/decoder, Git inválido, permissão insegura do documento e instalação readonly sem repair. O helper Git remove variáveis GIT_*, desabilita configuração global/system, hooks e signing por invocação. Preparação de instalação e ensure da identidade existem somente nas fixtures; não alteram perfis pessoais, trust ou credenciais.

A execução destas jornadas qualifica a biblioteca no Linux disponível. CLI/App, daemon de produto, manutenção/retirada de domínios, Now, lifecycle de hosts, Kiro V2 e V3 preview explícito via --v3 possuem integração própria ainda pendente. Windows/macOS não foram executados nesta unidade. Esta folha não introduz fallback Go.
