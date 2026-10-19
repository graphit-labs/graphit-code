---
title: Rerank opt-in de uma página de busca em Rust
type: spec
---

# Rerank opt-in de uma página de busca em Rust

`config.search.rerank` tem padrão `false`. O campo opcional `search.rerank` da
requisição prevalece sobre a configuração: valor explícito `false` desabilita,
`true` solicita e ausência herda a configuração. Portanto, ausência sem um opt-in
explícito na configuração continua desabilitada. Configurar um provider não ativa
rerank. `null` não equivale a campo ausente.

| Input `search.rerank` | Config `false` ou padrão | Config explicitamente `true` |
| --- | --- | --- |
| Ausente | Desabilitado | Habilitado pelo opt-in da configuração |
| `false` | Desabilitado | Desabilitado |
| `true` | Habilitado | Habilitado |

Quando o valor efetivo é `false`, o consumidor encerra esse caminho antes de
resolver recursos, descriptor ou slot de rerank. Não captura o serviço, descobre
endpoints, baixa, instala ou carrega o modelo, nem chama inferência ou rede de
rerank. O guard fica no consumidor, antes da composição dos recursos e da captura
Auth; construir recursos avidamente e só pular a chamada do helper não satisfaz
esse contrato. Embedding, quando solicitado pelo modo da busca, mantém seu fluxo
independente.

Quando `true`, o consumidor pode reordenar somente os candidatos da página já
capturada no mesmo pin e geração. Preserva identificadores, membership, `total`,
`top_k`, cursor, scores lexicais, distâncias semânticas e scores de fusão. Acrescenta
o score de rerank de cada candidato. Não busca candidatos adicionais nem promete
ordenação global entre páginas. O cursor continua a percorrer a seleção original;
rerank não concede acesso nem adota outra geração.

## Helper e produtores

O helper App `search_rerank::score` recebe o `Arc<SnapshotContext>` original,
`&mut ServiceSelection` já capturada, recursos emprestados, a query e os textos
dos candidatos em ordem. Rejeita outra Arc ou propósito diferente de Rerank.
Retorna scores `f64` na ordem de entrada e uma permutação completa das posições.
O caller associa scores e posições aos mesmos candidatos, sem substituir seus
scores de retrieval. Empates numéricos conservam a ordem de entrada, inclusive
`+0.0` e `-0.0`.

Local usa `ModelServiceClient::rerank` com a lease real, `RuntimeKeyOwner` e
`ExpectedModel` da seleção GGUF resolvida pelo produtor Models. O backend local
usa o SDK llama/cxx com `BAAI/bge-reranker-v2-m3` GGUF F16. O modelo é multilíngue,
com licença Apache-2.0 e capacidade de até 8192 tokens, sujeita ao limite efetivo
capturado. A execução CPU/GPU e sua qualificação pertencem ao produtor Models.
Gemma Embedding 2 continua no caminho de embedding: vetores e similaridade não
substituem scores de rerank.

Broker usa `BrokerVerifierCache` e `ServiceSelection::native_credential` quando
o modo Native exige verificação; os demais modos usam sua lease de credencial
real. O endpoint efetivo vem do resolver Broker existente da composição, nunca
de concatenação presumida do issuer. `GraphitReranker` mantém o wire Graphit
existente, com scores completos por índice de lote. Direct usa
`direct_native_rerank::score` e os protocolos já suportados pelo binding Auth.
Chaves Direct, Broker e MCP não são intercambiáveis.

O descriptor Broker de rerank exige `protocol: graphit-rerank-v1`, endpoint
resolvido do `path`, `revision` do serviço, `model`, `model_revision`, `recipe` e
`max_documents`. Cada resposta deve trazer `graphit` com `revision`, `model`,
`model_revision` e `recipe` exatamente iguais ao descriptor capturado. O limite
anunciado de documentos também limita o lote. Ausência, duplicação ou divergência
desses metadados é erro terminal; scores válidos por si só não qualificam a
identidade da resposta. A revisão do serviço e a revisão do modelo são campos
distintos.

Recursos são referências a produtores já existentes; não criam outro serviço,
runtime de modelos, cache de grants ou lifecycle. `ExpectedModel` é necessário
apenas para Local. Uma página vazia dispensa credenciais, discovery e conexão,
mas ainda verifica request, propósito e prazo finito.

## Integridade, limites e falhas

O helper verifica o mesmo Original e a seleção viva antes de consumir entradas
e depois da inferência e ordenação. Os produtores preservam cancelamento,
deadline, limites de inputs, lotes, JSON e output já configurados. A ordenação usa
`graphit_ai::ranking::rank_order`, rejeita cardinalidade incorreta ou score não
finito e retorna cada posição exatamente uma vez.

Resposta inválida, revogação física de Auth, cancelamento, timeout, erro de modelo
ou rede são terminais. Nenhuma falha retorna scores parciais ou é disfarçada de
busca reranked bem-sucedida com a ordem anterior. A causa tipada e o efeito de rede
observado permanecem disponíveis; um RPC local iniciado conserva efeito possível
quando sua execução não pôde ser observada até o fim. Diagnósticos do helper não
incluem query, body, endpoint ou credenciais.

## Estado desta implementação

O módulo `crates/graphit-app/src/search_rerank.rs` está registrado no App. O
orquestrador `run` consome o `ProjectControl` existente apenas para um binding
Local efetivamente habilitado; Direct e Broker usam os clientes e o cache Auth
já existentes. Nenhum recurso de embedding é necessário para habilitar rerank.

Os consumidores Knowledge Source, Browser e Role aplicam o guard efetivo depois
da seleção da página e antes de capturar Rerank. Source e Browser cobrem PROJECT
e Knowledge importado, tanto cópia Native quanto mount remoto. Nos importados,
a configuração Global é resolvida pelo HookPolicy ou TaskServer reais, sem
inventar uma seleção PROJECT. O reader de cópia conserva o Source snapshot do
reader admitido. O mount remoto conserva o snapshot usado pela busca; ambos
leem apenas os corpos selecionados e revalidam seus próprios guards após o
await, inclusive quando a inferência falha. Não reabrem Latest.

## Fixtures e qualificação

As fixtures usam Auth, TCP e owners produtivos. O cenário Source Direct verifica
publicação documental/NN, repetição sem inferência, HTTP 503 posterior ao efeito
documental e cancelamento anterior à admissão. Pelo codec CLI e pela sessão MCP
Source, exercita as seis combinações de configuração/input sobre a mesma página
com cursor. O cenário Source Local mantém embedding Direct e consome
`install_rerank_fixture_model` do produtor Data, com BGE pinado e ProjectControl
real. O owner executa `ForegroundSource::run` antes das chamadas que precisam de
controle Local; cleanup cancela e aguarda esse runner e os demais owners,
preservando a primeira falha. Não exige recurso Local de embedding.

Passaram os oito testes do helper, cinco de Role Knowledge e seis do cliente AI.
Esses últimos incluem onze respostas inválidas no segundo lote, recusadas sem
retorno parcial ou terceiro envio. Os cenários Source Direct e Source Local
passaram, preservando candidatos, geração, scores de retrieval e cursor; Local
verifica scores BGE finitos e ordenados. A qualificação dos callbacks e da
integração Direct/NN está na [projeção semântica](../architecture/rust-knowledge-semantic-pipeline.md#qualificação-do-consumidor).
Os focais nativos Source usam thread joined de 8 MiB, delimitando essa prova de
execução debug. O strict Clippy afetado continua pendente.

O cenário Browser usa Broker/STS/S3 reais, Knowledge RemoteNative de outro
namespace e Gemma/BGE pinados. Seus oráculos exigem preservação da seleção nas
páginas de dois e um candidatos, avanço do cursor e rejeição do cursor em outra
versão. **Ainda não há PASS desse cenário com a pilha padrão.** O diagnóstico
GDB concluído localizou a falha no consumidor HTTP de rerank importado, depois
da preparação do produtor e do Broker. A cadeia passa por Auth de Knowledge
importado, cliente de rerank, RemoteNative, ReferenceExplorer e Task/Browser.
`graphit_core::private_document::compare_bytes_buffered::<65536>` apresenta um
frame observado de 102408 bytes, numa pilha acumulada de 2091288 bytes, cerca de
2 MiB. A composição de futuros grandes é a hipótese de pressão cumulativa;
a correção completa ainda não foi demonstrada. Esse diagnóstico não qualifica
o Browser nem comprova que o frame Core isolado seja a única causa.

O focal Source Direct pode ser selecionado sem modelos locais:

```text
cargo xtask cargo test --offline --locked -p graphit-app --lib foreground_source::knowledge_source_tests::knowledge_foreground_actual_direct_preserves_document_effect_on_semantic_failure_and_joins -- --exact
```

Source Local requer `GRAPHIT_TEST_MODEL_SERVER` e
`GRAPHIT_BGE_RERANKER_V2_M3_MODEL`. Browser requer também
`GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN` e `GRAPHIT_EMBEDDINGGEMMA2_MODEL`.
Os caminhos devem apontar para os executáveis e artefatos pinados dos produtores;
os casos ignorados não são executados pela suíte comum:

```text
cargo xtask cargo test --offline --locked -p graphit-app --lib foreground_source::knowledge_source_tests::knowledge_foreground_actual_local_bge_rerank_uses_control_without_local_embedding -- --ignored --exact
cargo xtask cargo test --offline --locked -p graphit-app --lib browser_auth::tests::remote_native_web::real_broker_web_imported_knowledge_local_bge_rerank_preserves_candidates_and_cursor -- --ignored --exact
```

Contratos existentes: [ranking](rust_reranking.md),
[wire nativo](rust_native_rerank_wire.md),
[Direct](rust_direct_native_rerank.md),
[busca Knowledge](rust_knowledge_search.md) e
[Models local](rust_model_service.md).
