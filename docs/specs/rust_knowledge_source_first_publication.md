---
title: Primeira publicação Knowledge com Source revalidada em Rust
type: spec
references:
  - target: {type: task, id: tsk-943a}
    relation: implements
  - target: {type: task, id: tsk-e25e}
    relation: uses
  - target: {type: task, id: tsk-0fb2}
    relation: uses
  - target: {type: task, id: tsk-6101}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-0225}
    relation: uses
  - target: {type: task, id: tsk-f5a7}
    relation: consumer_boundary
  - target: {type: task, id: tsk-6ded}
    relation: refined_by
---

# Primeira publicação Knowledge com Source revalidada em Rust

A primeira batch de um FILE real cria o índice Knowledge PROJECT antes de o
restante da seleção ou os embeddings terminarem. As próximas batches usam a
[reconciliação scoped](rust_knowledge_document_lifecycle.md). Uma seleção
realmente vazia pode publicar o índice vazio ao EOF; ausência de índice e índice
vazio publicado permanecem distintos.

```text
publish_native_knowledge_source_file(original, read, write, source, batch, limits)
  -> Result<(KnowledgeFirstPublication, RecoveryKey),
            KnowledgeSourceFirstPublicationError>
publish_native_knowledge_source_stream(original, read, write, source, input, limits)
```

A variante first-stream é consumida para o vazio genuíno. Ela e o primeiro FILE
compartilham o mesmo Create/finish, schema FULL18, autorização, keys e guards.
Não existe outro publisher de compatibilidade ou importador.

## Admissão e integridade

Read e Write usam a mesma Arc Original e a mesma seleção PROJECT/runtime/version/
branch. O Reader conserva o Original e os handles nativos. A factory liga Source,
batch e PROJECT; dados comuns, paths e hashes não reconstroem essa autoridade.
O runtime/control do projeto é preparado antes de capturar sua seleção.

Schema, namespace, estado Active, identificadores e projeção authored C8 são
validados pelo codec existente. O FILE entrega `whole_source` byteexact: o guard
Core `validate_file_bytes` compara comprimento/SHA e bindings desse FILE nos
boundaries de preparação, commit e entrega, sem rehash do corpus por arquivo.
Mudar um irmão não invalida o FILE já preparado; mudar esse FILE falha. O vazio
first-stream conserva a validação da seleção completa e de seu EOF.

O destino é adquirido exclusivamente. Um corpus já publicado não é adotado nem
sobrescrito por uma operação First. Named/default mantêm seus efeitos próprios.
Cancelamento, deadline, revogação Read/Write e falhas físicas continuam sendo
causas reais; não há retry ou confirmação inferida por pathname.

## Efeitos e recuperação

A preparação e o commit SDK são separados. Fragments staged podem permanecer
após erro; Drop não promete rollback. A key é capturada antes do commit e um
commit iniciado é aguardado.

`BeforeCommit` indica que esta operação não iniciou o commit. Um erro do commit
iniciado conserva `CommitOutcomeUnknown` e a recovery key. Default é registrado
assim que observado; named é registrado depois do Clone real. Falha posterior
mantém `PublicationObserved` e todas as gerações efetivamente conhecidas. A key
Main não prova sozinha que o efeito named foi concluído.

Os getters `phase`, `effect`, `recovery_key`, `known_publication` e `cause` mantêm
a causa tipada e os efeitos confirmados. Uma falha num FILE posterior conserva o
recibo anterior no consumidor; não desfaz publicações já disponíveis.

Os summaries de sincronização pertencem ao UUID/tempo do commit nativo que os
publicou. Create default e Clone named registram seus próprios eventos, sem
inventar correspondência entre versões ou importar histórico retrospectivo.

## Validação

A emissão22 Linux executou o primeiro FILE com uma alteração independente num
irmão e verificou a rejeição do próprio FILE alterado (PASS, 2,54 s). O cenário
scoped de publicação/remoção/rename/CAS passou em 8,47 s. Os seis testes de
`knowledge_source_stream` protegem os guards do kernel compartilhado: schema/
Original, revogação/cancelamento/deadline, erro de IO, keys default/named e
semântica do vazio. Suas evidências válidas são reutilizadas quando esse código
está inalterado.

A emissão25 executou os quatro focais de histórico já portados para FILE,
first-empty e os helpers de commit ativos: 4 PASS em 4,85 s. Cobrem update/no-op,
cursor/cutoff, corrupção, named, referências não contíguas e publicação confirmada
seguida de erro. Não foi necessário conservar o publisher finito removido ou
seus hooks de fase. Recibo: `knowledge-progressive-data-20261023/attempt25-first-publication-history.log`
no diretório de validações da instalação.

As evidências não qualificam Windows/macOS nativos ou uma entrega isolada como
conclusão do produto. Os consumidores ficam na
[composição Knowledge](rust_foreground_knowledge.md).
