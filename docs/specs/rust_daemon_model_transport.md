---
type: specification
title: Desconexão do transporte de modelos em Rust
status: implemented-adapter
integration-status: pending
session: ses-c98a
references:
  - target: {type: task, id: tsk-b4e2, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target: {type: task, id: tsk-e90f, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: supports
  - target: {type: task, id: tsk-933e, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: relates_to
---

# Desconexão do transporte de modelos

`graphit_daemon::model_transport::DisconnectTransport::new(transport, cancellation)`
recebe um transporte SDK e o `graphit_core::file_lock::Cancellation` compartilhado
com o trabalho nativo daquela conexão. O transporte precisa implementar `Unpin`
e os traits `Stream<Item = Result<M, E>>` e `Sink<I>` aplicáveis.

O adaptador encaminha mensagens, estados de espera e erros originais sem codec,
buffer ou tarefa próprios. EOF, erro de leitura/escrita, fechamento concluído e
`Drop` sinalizam o mesmo cancelamento. Uma mensagem vazia continua sendo mensagem;
somente o término do stream representa EOF. Cada operação usa uma conexão e sinal
próprios, de modo que fechar uma não cancela outra.

O serviço de modelos mantém a responsabilidade por finalizar e aguardar seus
workers nativos. O adaptador sinaliza desconexão; sua destruição não significa que
um worker já terminou. Autenticação, mensagens RPC, serialização e limites de
frames pertencem ao serviço. Bootstrap, listener, processo filho e shutdown
autenticado pertencem à integração do filho; essas entregas têm Tasks próprios.

Os testes `crates/graphit-daemon/tests/model_transport.rs` usam sockets loopback
reais e o `LengthDelimitedCodec` do SDK somente como fixture. Verificam mensagens
nas duas direções, mensagem vazia, EOF com segunda conexão funcionando, erro
original de frame grande, fechamento e `Drop`. A execução qualificada nesta
entrega é Linux; integração nativa completa e outras plataformas permanecem nas
entregas do serviço/filho. Execute
`cargo test -p graphit-daemon --test model_transport --locked` e o Clippy focado no
mesmo alvo.
