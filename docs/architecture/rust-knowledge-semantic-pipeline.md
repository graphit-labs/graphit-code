---
title: Projeção semântica Knowledge em Rust
type: architecture
references:
  - target: {type: task, id: tsk-69f2}
    relation: refined_by
  - target: {type: task, id: tsk-ac1e}
    relation: implements
  - target: {type: task, id: tsk-598c}
    relation: uses
  - target: {type: task, id: tsk-9d92}
    relation: consumer_boundary
  - target: {type: task, id: tsk-1012}
    relation: implements
  - target: {type: task, id: tsk-1597}
    relation: derived_from
  - target: {type: task, id: tsk-879d}
    relation: relates_to
  - target: {type: task, id: tsk-8bef}
    relation: implements
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Projeção semântica Knowledge em Rust

O módulo `graphit_knowledge::embedding_projection` prepara os inputs Document
de um corpus Knowledge capturado pelo Data. A operação produz texto, fingerprint,
identidade e seleção de documentos pendentes, sem escrever no corpus. O consumidor
`graphit_app::knowledge_semantic` compõe a projeção com os produtores de inferência
e Data. A projeção isolada não comprova a execução desse pipeline nem seus
consumidores Foreground e Browser.

## Entrada e ciclo de vida

O caller captura o corpus por `capture_knowledge_embedding_corpus` com a admissão
PROJECT real e o `Arc<SnapshotContext>` original. `NoCorpus` continua um resultado
distinto: não deve virar uma captura vazia fabricada. Para `Published`, o caller usa:

```rust,ignore
let projection = prepare_knowledge_embedding_projection(
    original.as_ref(), &capture, &documents, effective_vector_id, effective_dimensions,
)?;
```

`effective_vector_id: [u8; 32]` e `effective_dimensions: u64` são observações do
modelo efetivamente resolvido. Não derivar identidade do nome do modelo ou da geração
do processo, nem fixar uma largura padrão. Dimensões devem ser positivas e caber em
`u32` e `i32`, conforme o armazenamento Data. A função rejeita valores inválidos antes
da construção dos textos.

`KnowledgeEmbeddingProjection<'a>` empresta o `KnowledgeEmbeddingCapture` real até
seu descarte. Os campos são privados, sem `Clone`, serialização ou construtor público.
`capture()` devolve esse mesmo objeto; `documents()` mantém a ordem do lote supplied;
`current_inputs()` devolve os slugs e fingerprints desse lote; `identity()` expõe a
identidade observacional para as APIs Data. Cada `ProjectedKnowledgeDocument` oferece
`slug()`, `text()`, `input_fingerprint()` e `is_pending()`.
`into_documents()` consome a projeção e `into_text()` consome cada input para mover
seu String ao RPC sem copiar todo o texto.

`documents: &[KnowledgeDocument]` é entrada comum supplied pelo caller para essa
função pura. Borrow/lifetime não prova que os DTOs vieram do pin nem que passaram
por limites Data; a projeção não valida provenance ou concede autoridade.
O App produtivo fornece exclusivamente lotes do `next_documents` Data autorizado,
processa e descarta seus bodies/textos antes do próximo lote. Para query, conserva
apenas slugs e fingerprints completos; para indexação, conserva também os vetores
necessários à publicação. DTOs arbitrários não tornam uma captura ou grant válidos.

O guard da função pura não verifica autoridade nem substitui os checks de mesma Arc,
binding, pins físicos e heads realizados pelo Data. O caller mantém suas admissões e
usa a captura original para publicação e consulta. vector_id, fingerprint e geração
jamais funcionam como grants.

## ABI do texto e fingerprint

A recipe é `knowledge-document-json-v1`. O texto contém JSON compacto serializado
por uma struct serde, nesta ordem exata:

```json
{"recipe":"knowledge-document-json-v1","title":"Título","kind":"page","description":null,"summary":"Resumo"}
```

Depois do JSON há **um LF**, seguido de todo o `body` documental literal. `None` produz
`null`; `Some("")` produz `""`. Unicode, aspas e controles dos campos do cabeçalho
seguem o escaping JSON. CRLF, controles, frontmatter e sintaxe de templates no body
permanecem literais. Não há trim, corte editorial, padding ou argumento separado de
título. O backend Models aplica seu prefixo Document uma vez; a projeção não o inclui.

O fingerprint é SHA256 dos bytes de outro JSON compacto de struct serde, com campos
na ordem `recipe`, `role` (literal `"Document"`), `vector_id` (array de 32 bytes),
`dimensions` (`u32`) e `text` (o texto exato acima). Alterar title, kind, description,
summary, body, vector_id ou largura muda o input. Slug, source path, referências,
staleness, datas e compilation digest não entram implicitamente nesse hash. As
observações físicas e a integridade dessas informações continuam no contrato Data.

## Pendências, integridade e erros

`pending_documents()` seleciona um documento quando seu slug não tem input armazenado
ou quando fingerprint ou identidade completa diferem. Um input saudável com a mesma
identidade é excluído dessa seleção, mas continua em `current_inputs()`. Stored extras
de slugs removidos não são reparados pela projeção: o Data pode responder
`Unavailable(Incomplete)`. `Attach` e `ReplaceComplete`, incluindo incompatibilidade e
reindexação, pertencem ao caller e ao [contrato Data](rust-knowledge-semantic-data.md).

`KnowledgeEmbeddingProjectionError` distingue `Request`, `Serialization`,
`InvalidDimensions`, `ConstructionOverflow` e `Allocation`, preservando causas quando
existem. Os guards observam cancelamento/deadline antes e depois de cada documento e
antes do resultado. Um erro não retorna uma projeção parcial. Não há promessa de
preempção dentro da serialização, teto de RSS ou nova quota de vetores. O body já foi
admitido pelos limites físicos e de conteúdo da captura. A construção verifica
overflow e reserva espaço para o resultado; o hash é escrito incrementalmente sem
duplicar o texto serializado do fingerprint. Debug da projeção mostra somente contagem,
largura e comprimento, sem corpo documental.

## Verificação e fontes

Os testes `crates/graphit-knowledge/tests/embedding_projection.rs` usam instalação
temporária, RuntimeKeyOwner, RoleIssuer e admissões PROJECT reais, publicação authored
e projeção documental de 16 campos com batches de uma linha. Protegem texto completo maior que 1600 bytes,
digest fixo independente, Unicode/controles/CRLF, None versus vazio, campos incluídos e
metadados externos, seleção após Attach de um vetor de storage, fingerprint diferente
sob a mesma identidade, mudança de identidade,
dimensões inválidas, cancelamento/deadline e ausência de efeitos da preparação.
Vetores determinísticos nessa fixture qualificam armazenamento e seleção, sem inferência.

```text
cargo test --offline --locked -p graphit-knowledge --test embedding_projection -j1
cargo clippy --offline --locked -p graphit-knowledge --lib --test embedding_projection -j1 --no-deps -- -D warnings
```

Implementação: `crates/graphit-knowledge/src/embedding_projection.rs`.
Contratos relacionados: [Dados semânticos Knowledge](rust-knowledge-semantic-data.md)
e [ModelService local](../specs/rust_model_service.md).

## Consumidor App: indexação e consulta

O caller entrega o `Arc<SnapshotContext>` original, as admissões PROJECT Knowledge e
`KnowledgeSemanticModel { metadata, batch_limits, rpc_timeout }`. `metadata` empresta
`EmbeddingMetadata` com vector_id, dimensão e modelo/revisão efetivamente resolvidos;
`batch_limits` conserva o `RpcLimits` já capturado pela composição. O pipeline não
exige slot local, `ExpectedModel`, lease Local ou endereço TCP para um provider remoto.
Não há probe, dimensão padrão, fábrica de grants ou lifecycle paralelo de modelos.

As admissões documentais read/write conservam exatamente a mesma Arc externa, o
binding PROJECT e seus rechecks. Cada chamada de inferência recebe `EmbedOperation`
por um callback `FnMut(EmbedOperation) -> Future<Result<Vec<Vec<f32>>, E>>`. O App
usa o mesmo wrapper por lote de Memory: cria uma subrequisição finita com a mesma
referência de cancelamento, captura o caller/serviço Auth real e usa o dispatcher
Local/Broker/Direct existente. O prazo é o menor entre o prazo externo, quando
finito, e agora mais a política de inferência. Um Original contínuo de indexação
não se torna uma lease remota sem prazo; a subrequisição é uma observação separada,
sem exigir igualdade de sua Arc com a do corpus. As admissões do corpus permanecem
vivas e são revalidadas antes, durante e depois dessa chamada.

O sink recebe um lote ordinário com slug, fingerprint, vector_id, recipe, role
e vetor. Data o associa às linhas do mesmo `index.lance`, acrescentando colunas
nullable somente depois de conhecer a dimensão real. Não há um segundo dataset
NN nem geração de processo local persistida no corpus. `EmbeddingMetadata::local_generation == None`
em um provider remoto não é substituído por uma geração fabricada. A prova de
indexação Direct desta folha deve verificar o commit e a leitura/reutilização
dessa extensão no mesmo corpus real. Ela não qualifica o armazenamento de outros módulos.

`ExpectedModel` continua somente no produtor Local. Os produtores de inferência
verificam suas leases, execução, identidade e saída; Data verifica binding,
observações físicas, pins e heads. PROJECT read, PROJECT write e serviço AI são
concessões distintas. O pipeline também valida a cardinalidade, a largura exata e
finitude de todos os vetores recebidos antes de associá-los aos inputs: um callback
com saída incompleta não pode ser truncado silenciosamente por `zip`.

`index_knowledge_semantics` recebe original, read, write, model, modo Data,
`KnowledgeSemanticIndexLimits { read, write }` e o callback de inferência. Data
valida primeiro o corpus até EOF sem colecionar corpos; depois o caller drena o
cursor privado no mesmo pin. Nos dois modos, infere apenas inputs pendentes.
`Attach` omite os demais; `ReplaceComplete` lê seus vetores reais por
`KnowledgeEmbeddingCapture::read_reusable_embedding_batch` e entrega os batches
canônicos ao mesmo publisher. A reutilização exige slug, fingerprint e identidade
completos iguais aos auditados. Textos e vetores ficam restritos ao lote atual.
Erro preserva sua causa e encerra a produção. O publisher exige cobertura completa
e EOF antes do no-op ou commit; omitir documentos apagados remove suas linhas NN.

Os textos pendentes são particionados pela quantidade, soma checked dos bytes
UTF-8 e quantidade vezes dimensão dos limites efetivos. A composição reduz a
quantidade para um quando a execução Local observada é escalar, preservando o
backend Gemma Embedding 2. Essa escolha não é inferida pelo nome, device ou path;
providers remotos conservam seus próprios limites e protocolos. O texto individual
continua integral: se sozinho exceder o limite do produtor, seu erro é preservado,
sem trim, truncamento, fallback ou retry. Providers continuam validando limites de
input, JSON/frame, output, modelo/revisão e autenticação.

O callback só é chamado para um grupo não vazio. Sem corpus ou inputs pendentes
não há inferência. Os guards existentes observam o Original e as admissões durante
a espera; depois do await, inclusive retorno com erro, o mesmo pin físico é
revalidado sem rescan ou captura de latest. Só depois de EOF válido publica uma
vez pelos writer/heads/CAS/recovery Data existentes. Falha anterior ao commit não
publica um prefixo semântico. O no-op ainda passa pelos checks reais do writer.

`query_knowledge_semantics` recebe original, read, model, texto literal,
`KnowledgeSemanticQueryOptions { top_k, metric }`, limites Data e um callback
`FnOnce(EmbedOperation)`, pois há no máximo uma inferência Query. A indexação usa
`FnMut` para seus lotes sucessivos. Drena o cursor, projeta e descarta textos por lote, retendo current_inputs
completos. Infere uma Query literal e entrega o vetor ao pin NN capturado. Para
corpus publicado vazio ou sem embeddings, essa API de biblioteca ainda infere;
Data produz seu resultado tipado, sem vetor fabricado ou busca alternativa.

A busca CLI/Source usa `query_knowledge_search_capture`: drena o cursor até EOF e
verifica prontidão no mesmo pin antes de inferir. Só `Ready` chama o callback Query;
`PublishedEmpty` e `Unavailable`, incluindo `MissingSemantic`, seguem para Data sem
inferência. Uma resposta MissingSemantic não comprova carga de modelo. A publicação
semântica em background conserva seu resultado separado da publicação documental.

O produtor, o publisher e o futuro de inferência são mantidos em `Box::pin`
para limitar o estado retido diretamente nos frames assíncronos. Produtor e
publisher continuam no mesmo `tokio::join!`, com canal de capacidade um,
Original, guards, EOF e primeira causa preservados. Não há tarefa destacada,
dispatch dinâmico ou alteração da API nessa composição.

## Pins e memória da operação

Source e NN são datasets Lance distintos. `KnowledgeEmbeddingCapture` conserva
seus snapshots, observações físicas e cursor de documentos até terminar o
publisher e o produtor. Manter esses pins preserva geração, autorização e
integridade durante inferência; não equivale a abrir um grafo Ladybug ou Icebug.
A configuração `ast.query.buffer_pool_bytes` não é consumida por esse pipeline.

O Foreground chama `index_knowledge_semantics_selection` para o slug já confirmado
de cada FILE. O scanner Source filtra essa seleção. A observação NN projeta
identidade, fingerprint e role sem carregar a coluna de vetores; a reutilização
lê apenas os vetores solicitados do pin retido. A API de corpus completo conserva
metadata de todos os slugs auditados; esse custo difere do armazenamento dos
corpos e dos vetores, que permanecem em lotes.

`next_documents` obtém o lote documental antes da projeção dos inputs.
`send_document_batch` reserva capacidade no canal antes de chamar inferência;
o lote documental já lido continua vivo durante esse await. Textos e vetores
temporários terminam na iteração, e o cursor pode conservar um batch pendente e
seu stream SDK até EOF ou descarte. Capacidade um limita a fila do consumidor,
sem prometer quantidade constante de memória, teto de RSS ou liberação imediata
dos caches do SDK e do alocador. Reduzir memória não autoriza descartar pins,
fingerprints ou vetores persistidos necessários à reutilização.

## Resultados e recuperação do caller

| Resultado | Ação do consumidor |
|---|---|
| `IndexResult::NoCorpus` / `QueryResult::NoCorpus` | Indexar/publicar documentos antes de pedir embeddings; nenhum RPC ocorreu. |
| `IndexResult::Published(Unchanged)` | O no-op foi validado pelo Data e não houve novo commit. Sem pendências, ambos os modos evitam Models; ReplaceComplete lê os vetores reutilizáveis e ainda drena o stream e o Source até EOF. |
| `IndexResult::Published(Committed { ... })` | Conservar generation, default e recovery para diagnosticar o resultado real. |
| `QueryResult::Published(PublishedEmpty)` | O corpus publicado está vazio. |
| `QueryResult::Published(Unavailable(...))` | Tratar MissingSemantic/Incomplete/incompatibilidade segundo Data; não apresentar como NoHits. |
| `QueryResult::Published(Hits { ... })` | Conservar gerações e distâncias; um vetor de hits vazio é NoHits saudável. |

Os nomes completos dos enums são `KnowledgeSemanticIndexResult` e
`KnowledgeSemanticQueryResult`. Mudança efetiva de vector_id, recipe ou dimensão torna
o corpus incompatível. O consumidor escolhe `ReplaceComplete` para reindexar; `Attach`
não transforma automaticamente a identidade de todo o corpus.

`KnowledgeSemanticError` preserva causas de Request, Auth, Service, Runtime, Projection,
Models, Inference e Data. Distingue classe de admissão inválida, Arc externa
estrangeira, dimensão ausente, política de inferência inválida e output incompleto
ou não finito. A causa do callback permanece boxed; erros Local ModelServiceError
conservam sua classificação Models. O propósito da lease é verificado pelo
produtor Auth/AI efetivo, não por uma exigência Local no kernel Knowledge. O erro Data fica boxed, com suas variantes e campos intactos:
`PartialDefault` e `CommittedButRecheckFailed` podem conter evidência de publicação;
não significam ausência de efeitos. Recovery First certifica Main, com fase named
separada, conforme Data. Não há rollback, replay automático ou adoção de pins antigos.

## Qualificação do consumidor

A fixture `crates/graphit-app/tests/knowledge_semantic.rs` usa Auth/Core PROJECT,
publicação e leitura NN reais. A integração Direct por TCP passou um cenário,
cobrindo geração, Attach/ReplaceComplete, batching, reutilização sem inferência,
limites, erros e cancelamento. Os dois testes de callbacks também passaram:
protegem o prazo finito da operação filha, a causa original de cancelamento e a
rejeição de saída incompleta ou não finita. Esses resultados qualificam o kernel
atual independente de provider.

A fixture Local usa GGUF BF16 pinado, Runtime, ExpectedModel e RPC TCP reais.
Sua qualificação anterior cobre indexação e consulta, mudança de identidade e
largura, reindexação, no-op, reaproveitamento de vizinhos, remoção de linhas NN,
limites e falha sem publicação parcial. Ela antecede o callback atual e não
substitui a prova do dispatcher remoto. `ModelService::shutdown` e as tasks RPC
são aguardados antes do descarte do NativeSdkRoot.

Os testes nativos usam thread joined de 8 MiB e runtime Tokio current-thread.
Esse limite do harness debug não estabelece um orçamento de stack do produto.
O Browser com pilha padrão continua pendente; o diagnóstico do consumidor
importado está no [contrato de rerank](../specs/rust_search_rerank_consumer.md#fixtures-e-qualificação).
Strict Clippy afetado também permanece pendente. Esses escopos não qualificam
outras plataformas ou o cutover global.

O caso GGUF requer `GRAPHIT_EMBEDDINGGEMMA2_MODEL` apontando para o artefato
pinado. Estar ignorado na suíte comum não constitui prova de execução. Com os
produtores nativos configurados, os focais podem ser selecionados por nome:

```text
cargo xtask cargo test --offline --locked -p graphit-app --test integration -- --ignored --exact knowledge_semantic::authorized_gguf_corpus_index_query_reindex_errors_and_shutdown
cargo xtask cargo test --offline --locked -p graphit-app --test integration -- --exact knowledge_semantic::actual_direct_embedding_stream_generation_attach_skip_limits_and_cancel
cargo xtask cargo test --offline --locked -p graphit-app --lib knowledge_semantic::callback_tests::
cargo xtask cargo clippy --offline --locked -p graphit-app --lib --test integration --no-deps -- -D warnings
```

## Geração documental observada pelo ForeGround

`index_knowledge_semantics_at_generation` recebe a geração S4 observada pelo
caller. Depois da captura genuína Data, compara essa geração integral com
`source_generation()`; ausência ou divergência retorna `Conflict`, sem adotar
um head posterior. O kernel é compartilhado com a API library existente.

O consumidor Foreground usa `index_knowledge_semantics_selection` para cada slug
documental confirmado, com metadata resolvida e o callback do dispatcher
Local/Broker/Direct real. A variante de geração explícita continua disponível
para callers que exigem igualdade integral com uma geração observada.
Falha semântica conserva
inteiro o resultado documental em `KnowledgeAfterPublication`; ausência de Models
é estado explícito e não comprova atualização semântica.

Na implementação de `tsk-69f2`,
`prepare_knowledge_embedding_document` projeta um documento emprestado pela mesma
recipe JSON/LF/body e fingerprint já estabelecida; identidade com outra recipe é
recusada como InvalidRecipe. Os DTOs vêm do cursor Data
admitido; o helper não concede autoridade ou prova de origem. O consumidor agrupa
textos pelos limites reais de count/input/output Models, reserva o canal antes
RPC e mantém somente o lote local de vetores para encode. Não conserva observed
ou rows de todos os documentos. Attach conserva o skip de inputs já validados;
ReplaceComplete lê seus vetores pelo accessor Data no mesmo pin, sem Models,
e o publisher compara valores reais. O consumidor reserva espaço no canal antes
do RPC e mantém Original, PROJECT read/write e Models vivos durante o await.

Produtor e publisher são joined por canal de capacidade 1. O produtor termina o
cursor documental, preserva a primeira causa Models/Original tipada antes do
wrapper SDK e envia batch vazio explícito quando nenhum batch foi produzido. Fechar
o consumidor conserva a causa real do publisher, sem fabricar BrokenPipe. Data
não comita um prefixo ou cursor incompleto. A publicação segue o pin da geração S4
observada e os guards/recovery atuais, conforme [o contrato Data](rust-knowledge-semantic-data.md).

As fixtures do pipeline usam child, Runtime, ExpectedModel/vector_id e TCP reais. Conflitos de geração e limites de input devem ser recusados antes do RPC e sem alterar o índice NN. Cancelamento preserva o Original, drain e join do child.

## Composição com o owner Foreground

O Foreground fornece `MemoryJobResources` do owner existente. Metadata e
inferência resolvem o binding efetivo em cada operação Auth real. Local usa
manifesto, child e client do produtor Models; Direct e Broker não exigem
`ExpectedModel` nem processo local. Knowledge usa a mesma AuthArc e suas próprias
admissões documentais, mantendo o corpus e o serviço AI em concessões distintas.
O lifecycle continua nos owners existentes; Knowledge não cria outro launcher,
issuer ou gate global de computação.

O job semântico em background cria uma `RequestContext::service` cancelável,
independente da operação documental. Cada seleção conserva suas capturas e
admissões PROJECT até os efeitos e joins.
O corpus inteiro não recebe a expiração genérica de uma requisição Hub.
No construtor do job de indexação, `KnowledgeSemanticModel` recebe um default
BG por RPC Document de 900 segundos (15 minutos), apenas quando a origem da
política é `DistributionDefault`. Esse prazo cobre espera de admissão e execução
local; não é um SLA de conclusão. A origem `Configuration` preserva todo valor
explícito de `hub.limits.request.deadline_ms`, menor ou maior, inclusive quando
igual ao default do catálogo. Query foreground e o default global do Hub
continuam iguais.
Cada lote calcula seu `EmbedOperation.deadline` por `Instant::checked_add`. O
callback do dispatcher cria uma requisição finita com o mesmo cancelamento; usa
o menor desse prazo e do prazo da Original externa, quando ela é finita. Não se trata
de ganho de velocidade ou promessa universal; cancelamento, limites de entrada,
identidade/pesos e JWT do Broker de 600 segundos não mudam. Assim os
callers HTTP/CLI mantêm seu limite original e o job contínuo mantém cancelamento,
autoridade e prazos concretos por RPC. Não há renovação da Original ou da lease,
novo grant, contexto por documento ou prazo MAX substituindo o orçamento removido.

Quando o modelo passa de MNone a MReady, os supervisores Knowledge existentes
devem refazer a seleção sem exigir evento de arquivo ou configuração. A
[publicação Foreground](rust-knowledge-foreground.md) descreve configuração
recarregada, efeitos documentais confirmados e controles do watcher. As
qualificações de biblioteca, do dispatcher e de cada ingresso permanecem
separadas.
