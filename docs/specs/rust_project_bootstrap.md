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

`graphit_daemon::project_bootstrap` observa um checkout existente e registra sua
versão selecionada no [inventário Core](rust_project_inventory.md). A identidade
persistente deve existir e admitir stores persistentes. O caller prepara o
parent do inventário e fornece o Original finito da operação.

## API e sequência

`ProjectBootstrapInput` empresta `project`, `explicit_version`, `pin` e
`detached_base`. A função é `bootstrap_project(input, &store, &original_request)`;
ela não recebe limites físicos nem uma capability de instalação.

1. `required_deadline` exige o prazo original finito e verifica cancelamento.
2. `ProjectIdentityStore::load` lê o ProjectId esperado, sem criar identidade.
3. `GitCheckout::inspect` observa o checkout com tempo restante e cancellation
   original; o Original é verificado novamente após o SDK, inclusive na falha.
4. `VersionContext::select` aplica a política Core: explícito, pin do mesmo
   escopo, branch, base detached ou default. Pin de outro escopo é ignorado.
5. `ProjectInventoryObservation::observe` associa checkout, versão e ProjectId.
6. `store.writer(&original)` executa `register_checkout` sob o lock Core, que
   é liberado por RAII antes do retorno.

O bootstrap devolve o `VersionContext` selecionado. Uma nova observação após
mudar o checkout ou a seleção atualiza o registro e preserva as versões antigas.
A função não troca branches, escreve pins ou enumera versões em Git refs.

## Falhas

`ProjectBootstrapError` preserva fase e causa: `Request(io)`, `Checkout(io)`,
`Selection(io)`, `Identity(io)` ou `Inventory(io)`. `source()` conserva a fonte.
Serviço sem prazo finito falha como Request/InvalidInput antes de I/O.
Cancelamento/prazo inicial impedem a observação; identidade ausente, corrompida,
substituída ou ephemeral falha explicitamente. Seleção inválida permanece Selection;
Git inválido permanece Checkout. Brand divergente, JSON inválido, parent ausente
e falha do writer permanecem Inventory. JSON de inventário não contém credenciais
e aceita permissões normais do sistema.

O Core verifica as condições antes da troca atômica. Não há revalidação genérica
de diretórios, receipts de filesystem ou recuperação automática. Uma nova execução
refaz explicitamente a observação. A API não promete rollback entre stores,
hard deadline sobre I/O síncrono ou durabilidade contra perda de energia.

## Verificação

`crates/graphit-daemon/src/project_bootstrap/tests.rs` usa Git e APIs Core reais
em diretórios temporários: NonGit, branch/pin/explicit/foreign pin/detached,
worktrees com identidade compartilhada, isolamento A/B, no-op sem rewrite,
histórico após switch, cancelamento/prazo, identidade missing/ephemeral/corrupta,
brand divergente, Git inválido, JSON bounded, permissões normais e parent ausente.
A qualificação atual deve ser lida no checkpoint da sessão; os antigos testes de
captura física não qualificam esta implementação. CLI/App e native Windows/macOS
exigem evidência própria.
