---
title: "Native USER Memory model consumer"
description: "USER admissions composed with the selected embedding service and native Memory SDK."
type: specification
updated: 2026-10-21
references:
  - target: {type: task, id: tsk-2b12}
    relation: implements
  - target: {type: task, id: tsk-8e7b}
    relation: implements
  - target: {type: task, id: tsk-85c4}
    relation: requires_contract
  - target: {type: task, id: tsk-5c9b}
    relation: shares_model_consumer
---

# USER Memory: consumidor Document/Query no daemon

Para integradores do daemon, Auth e Records. `MemoryEmbeddingModel::index_user`
e `query_user` compõem a identidade portátil e o callback do serviço de embedding
selecionado com as
[APIs nativas USER](native_user_memory_store.md#user-semantic-capture-and-publication-implemented-sdk-validation-pending).
O consumer é implementado no [submódulo USER](../../crates/graphit-daemon/src/model_service/memory_embeddings/user.rs)
e exportado pelo [model_service](../../crates/graphit-daemon/src/model_service.rs).
Reutiliza o mesmo consumer genérico de identidade e callback [PROJECT](native_memory_embeddings.md#consumer-daemon-documentquery).

## Autoridade e efeitos

`index_user` recebe `UserReadAdmission`, `UserWriteAdmission` com operação
`Memory`, limites Records, `RpcLimits` explícitos e um callback assíncrono
`embed(EmbedOperation)`. `query_user` recebe a admissão USER de leitura, texto,
filtros, limites e o callback Query. O lease de serviço autoriza a inferência, separadamente dessas
admissões: não concede acesso USER nem PROJECT. Nenhum novo issuer, credencial,
pool ou executor é criado pelo consumer.

As admissões e o consumer precisam pertencer ao mesmo Original. Leitura e
escrita precisam corresponder ao mesmo owner literal e runtime root. As APIs
Auth existentes verificam propósito/permissões e observações atuais; o SDK
preserva o binding físico da namespace. O consumer verifica esses vínculos antes
de inferência, depois dos awaits e antes de entregar o resultado. Mudanças no
provider/profile local, cancelamento, prazo vencido ou substituição da namespace
invalidam o uso da admissão. A validação JWT continua local até a expiração
assinada, sem UserInfo obrigatório por requisição.

Captura usa o corpus/default pin USER real, sem campos PROJECT ou seleção Git.
`NoCorpus` retorna sem chamar o callback de inferência ou criar
diretórios. Um corpus publicado sem documentos elegíveis retorna `Complete`
sem attachment no índice e `PublishedEmpty` na query, também sem RPC. O callback Query não é chamado nesses estados.

## Document, Query e resultado

Cada lote obtido por `next_documents` fornece a projeção JSON literal e o
fingerprint preparados por Records. O daemon envia os textos como `Document` ao callback do serviço admitido,
valida quantidade, dimensão e valores finitos e move os vetores recebidos para
`attach_embeddings`. O compositor usa Local/RPC, Broker ou Direct sem trocar a
identidade. Somente Local exige ConfigId e ExpectedModel; cada lote remoto de
background recebe uma admissão finita, sem renovar a autoridade USER.
Mantém somente metadados do lote e o último attachment observado; não retém
uma segunda cópia do corpus inteiro. Os limites funcionais são os dos produtores
Records e ModelService, sem contabilidade adicional de recursos.

`UserMemoryEmbeddingIndexRead` é `NoCorpus` ou `Complete` com a geração
capturada do corpus e `last_attachment: Option<Box<UserMemoryEmbeddingAttachOutcome>>`.
A geração capturada e a publicação do último lote são observações distintas;
cada lote publica independentemente. Um resultado completo sem attachment não
inventa versão nova, publicação ou recovery key.

Query envia uma única string como `Query`, verifica a identidade/vetor e chama
`UserMemoryEmbeddingCorpus::search_semantic` com o vetor e as opções originais.
Preserva os quatro estados reais: `NoCorpus`, `PublishedEmpty`, `Unavailable`
e `Published`. Identidade incompatível ou cobertura incompleta não vira fallback
lexical. Os filtros, histórico excluído por padrão, collapse por Memory ID,
localização da revisão e distância vêm do SDK. A captura é mantida durante a
inferência; a busca usa seu mesmo pin e geração. Se as credenciais S3 expirarem,
a renovação do transporte conserva esse snapshot, sem consultar latest.

`UserMemoryEmbeddingFailure` preserva `MemoryEmbeddingCause::Service(E)` ou `Store`
e expõe a causa por `Error::source`. `last_attachment` conserva o último lote
observado antes da falha. Um `UserEmbeddingConfirmed` do SDK conserva sua
publicação e recovery key dentro da causa, sem conversão em texto. A recuperação
usa novo Original/admissão de escrita e `resolve_embeddings` nativo; o consumer
não repete a mutação, não tenta rollback e não transforma uma chave em permissão.

## Qualificação funcional

O [target USER](../../crates/graphit-daemon/tests/user_memory_embeddings.rs)
usa JWT assinado/verificador real, admissions Native/Incoming reais, a namespace
USER física, transporte TCP ModelService, tabela Lance SDK e o GGUF BF16
EmbeddingGemma2 já qualificado pelo produtor. As duas funções cobrem:

- Document → attachment SDK → Query, ConfigId diferente, filtros, revisão
  atual e histórico deletado explícito; corpus ausente/vazio com zero RPC.
- Falhas físicas e de transporte no segundo lote com o primeiro attachment
  preservado, resolução com novo Original sem nova publicação e reindexação.
- Permissões USER insuficientes, owner/Original estrangeiros, operação fenced,
  logout Native, provider alterado, expiração assinada, cancelamento, deadline
  e substituição da namespace, antes de abrir conexão.

O receipt histórico era `.build/rust-user-memory-model-consumer/qualification.json`;
a rodada corrente de 2026-10-21 está discriminada abaixo.
O teste requer `GRAPHIT_EMBEDDINGGEMMA2_MODEL` apontando para o artefato existente;
execute cada caso com `--exact --include-ignored` em um processo separado.
`NativeSdkRoot` inicializa o SDK uma vez por processo; serializar vários casos
no mesmo processo não substitui esse isolamento. A
qualificação usa uma configuração real de 128 dimensões e não implica campanha
de modelos, prova de performance, endpoints App, Runtime Dream USER ou cobertura
de outros sistemas operacionais. As qualificações SDK de `tsk-85c4` e PROJECT
de `tsk-5c9b` continuam com seus próprios escopos e contagens.

Na qualificação Linux de 2026-10-21, o consumidor portátil passou os três casos
distintos do target, em processos separados: a jornada real de índice/query e
efeitos parciais (176,45 s), os guardas de admissão/Original/liveness antes de RPC
(74,63 s) e a consulta híbrida GGUF (54,24 s). Esses resultados comprovam a
composição USER com o serviço Local e reutilizam os produtores Auth/Records/Models;
não qualificam um serviço remoto USER nem os endpoints App.

## Query híbrida USER

`MemoryEmbeddingModel::query_user_hybrid<E, F, Fut>` recebe `&UserReadAdmission`,
`text: String`, `MemoryHybridSearchOptions`, `MemoryStoreReadLimits` e
`embed: FnMut(EmbedOperation) -> Fut`, retornando
`Result<UserMemoryHybridSearchRead, UserMemoryEmbeddingFailure<E>>`. Usa a mesma
identidade portátil e o callback Query de PROJECT, com rechecks USER antes/depois
da inferência e da entrega. Serviço e autoridade USER Incoming/Web permanecem
separados e ligados ao mesmo Original; não nasce outro engine, pool ou issuer.

A consulta FTS literal é preparada por `MemoryHybridQuery::new` antes de RPC.
O preflight captura o corpus USER real; `NoCorpus` e `PublishedEmpty` não
chamam o callback de inferência. `UserMemoryEmbeddingCorpus::is_empty` consulta os
IDs capturados. A exaustão de `next_documents` representa o estado da iteração,
e não redefine a existência de corpus; um corpus inteiramente coberto continua
não vazio e recebe uma Query real.

Após o vetor real, `UserMemoryEmbeddingCorpus::search_hybrid` usa o pin já
capturado e produz os dois canais conforme o
[contrato hybrid USER](native_memory_search.md#busca-híbrida-user).
Preflight, inferência e leitura final conservam o mesmo snapshot, inclusive
quando o transporte S3 precisa de credenciais renovadas. Falhas conservam causa Service(E)/Store e
`last_attachment: None`; a consulta é somente leitura e não repete mutações.

A regressão atual `actual_gguf_user_memory_hybrid_rpc` no target
[user_memory_embeddings](../../crates/graphit-daemon/tests/user_memory_embeddings.rs)
usa o GGUF BF16 fixado com MRL128, NativeSdkRoot, TCP e ModelServiceClient reais.
A execução GGUF passou na qualificação acima. Sua antiga execução com ONNX
é evidência histórica; nenhum desses casos qualifica o serviço remoto USER.
