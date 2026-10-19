---
title: Busca Knowledge em Rust
type: spec
references:
  - target: {type: task, id: tsk-cab0}
    relation: supports
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: consumer_boundary
---

# Busca Knowledge em Rust

Esta folha pertence a Knowledge/Wiki de ses-c98a. Data e a bridge Knowledge
implementam busca lexical, Semantic e Hybrid Native sobre pins reais.
A evidência S3 atual em Linux reúne 24 funções distintas PASS:
12 Search, incluindo o seam NoM26, e 12 helpers NN requalificados em Helper22.
Replays e qualificações anteriores não aumentam esse total.
O scoped strict30 passou Data/Knowledge e os cinco targets afetados.

Essa prova não qualifica automaticamente Models.Query/App, Imported NN,
multisource, HTTP/MCP/Ask ou UI. As provas S2, publicação semântica e
referências browser conservam seus escopos próprios.

O rerank opcional do consumidor segue o [contrato de opt-in](rust_search_rerank_consumer.md):
input explícito prevalece sobre `search.rerank` da configuração, cujo padrão é
`false`. A etapa atua somente nos hits da página já capturada, acrescenta
`rerank_score` e preserva os scores de retrieval, identidades, geração, total e
cursor. Desabilitada ou sem candidatos, não captura o serviço de rerank. Os
novos consumidores App ainda dependem de sua própria qualificação funcional;
as provas Data/Knowledge acima conservam seu escopo.

## Seleção e fluxo

O consumidor seleciona PROJECT ou uma instalação Hub concreta. Cada fonte precisa de sua admissão própria. Filtros públicos são seleção de wikis, hub_refs e versão; não são propriedades internas tags/type/data/mandatory. O browser e MCP usam fts, semantic ou hybrid (padrão hybrid); top_k negativo é erro de entrada, zero significa todos os matches paginados. O consumidor aplica página padrão20 e máximo100. Heads/Page não são busca.

Data expõe capture_native_knowledge_search(Original, ReadKnowledge, limites) como NoCorpus ou Published com captura privada. query_native_knowledge_search recebe essa captura, KnowledgeLexicalQuery e limites da operação. Captura conserva binding/branch/Original, diretório e manifest físicos, pin SDK e identidades Active compactas. Não conserva corpus de bodies/vetores, não projeta campos vetoriais na consulta lexical e não faz cache de autoridade entre requisições. Todo read revalida a admissão viva e SAME Original.

Na captura lexical e em cada check de sua admissão, projeto e request vêm da
mesma observação conjunta dos helpers Index. Classificação Knowledge e
identidade da Arc são verificadas; comparar binding/branch usa essa captura
emprestada, sem revalidar Auth por um segundo getter consecutivo. Somente
Auth Request consulta novamente o mesmo Original para conservar seu erro IO
concreto de cancelamento/prazo; Original vivo conserva Auth Request e os demais
erros permanecem intactos. Guards de diretório/manifesto, integridade FULL18/C8,
EOF e as próximas fronteiras de autorização continuam obrigatórios.

Nos readers Native heads e de membership lexical, batches Arrow já possuídos
são decodificados em RAM com checks baratos do mesmo Original por linha e
referência. Auth e observações físicas continuam completas antes/depois da leitura SDK,
nas fronteiras de batch, EOF e entrega. Depois do decode puro, o guard completo
executa antes de consumir seu resultado, inclusive erro; nenhum resultado parcial
é entregue se a autoridade foi revogada ou o Original terminou. A espera SDK usa o sinal de cancelamento e o prazo original via Tokio, sem
polling periódico de Auth/FS. O guard completo executa antes da espera e depois
do retorno, antes de aceitar os dados. Seleção/validação de linhas e preview
já em memória verifica o Original sem adquirir locks de credenciais por campo. Essa separação elimina
revalidações de Auth/FS por campo em RAM sem cache de grants, mudança de formato
ou relaxamento de FULL18/C8, limites, duplicatas e validação até EOF. A correção
do timeout observado no Source depende de nova execução instalada.

O target DEBUG `graphit_app::source_server::knowledge_search` registra somente fases estáticas:
entrada na fila, entrada no worker e entrada no reader Data lexical. O span
omite todos os argumentos e acompanha a closure pelo contexto padrão de tracing.
Esses eventos não incluem query, URI, path, body ou credenciais e não alteram
admissão, Original, prazo, cancelamento ou tratamento de erros. Diagnóstico de
fase não comprova a integração nem identifica sozinho a causa de uma espera.
No target `graphit_data::lance_branch_store`, os spans `open` e
`capture_observed` também omitem todos os argumentos. Os eventos estáticos
`commit_handler_start`/`commit_handler_end` e `load_start`/`load_end` delimitam
os awaits correspondentes. Um evento de término registra o retorno do await,
inclusive quando o resultado preservado é um erro; não anuncia sucesso.
A captura lexical delimita ainda `knowledge_pin`, `knowledge_manifest` e
`knowledge_membership` no target `graphit_data::knowledge_embeddings::search`,
sem campos de entrada. O primeiro abre o pin; o segundo observa e reabre o
manifesto físico da mesma versão; o terceiro valida FULL18/C8, membership e EOF.
São spans de fase, não eventos de sucesso. Sua criação/fechamento aparece quando
o subscriber habilita eventos de ciclo de span; os eventos internos carregam
o contexto da fase. Fechamento por retorno com erro ou descarte do future não
qualifica a busca. Esses diagnósticos não alteram a frequência dos controles,
autorização, limites, cancelamento, pin ou integridade.

O seam search_knowledge_snapshot_guarded usa ModuleSnapshot real do caller e sua geração observada; não abre locator, não adota latest nem concede acesso. CapturedImportedKnowledge::search usa seu reader privado, instalação/registro reais e guard por row/await/EOF/delivery. O binding de seleção existente inclui registro, instalação, artifactID, versão e alias. Erros do reader conservam a primeira causa tipada; marcador interno external_guard não substitui essa causa.

KnowledgeSearchCapture::next_documents(original, &ProjectReadAdmission) retorna
Result<Option<Vec<KnowledgeDocument>>, KnowledgeSearchError>. O cursor usa a mesma
Source pin/geração, inicia scanner documental lazy e não abre NN. Cada lote tem
no máximo batch_rows. Some(empty) pode conter apenas Removed validados; só None
certifica EOF. failed é marcado antes do consumo aguardado: erro/Drop in-flight
não permite retomar como EOF saudável. Original/admissão/observações físicas
continuam verificados. Source44 passou uma função Linux (35s runtime/38.880s total):
pin retido após SDK replace, literal CRLF/Unicode, AllRemoved, OtherOriginal,
Drop e cancelamento sem NN. Prova distinta de NativeS3 e I1.

## Ranking, integridade e paginação

FTS usa FullTextSearchQuery maduro do Lance13, texto literal e colunas explícitas title/body. A combinação multi-field do SDK usa best_fields/MAX; não se anuncia soma ou BM25F. Projeção explícita slug/_score evita depender da autoprojeção depreciada. Flat search é permitido: fast_search e índice JSON sobre Markdown não participam deste contrato. Active é filtrado pelo SDK antes do ranking; Removed não entra.

Captura standalone valida FULL18, C8, estado, namespaces, campos/nulls, slugs/duplicatas e EOF, sem copiar bodies. Consultas na captura validada reutilizam esse pin. O reader standalone importado executa somente sua validação necessária. Native Semantic/Hybrid reutiliza o pin documental/vetorial único de KnowledgeEmbeddingCapture, junto da validação CurrentInputs/EOF existente, sem segunda captura universal. Scores Float32 devem ser presentes/finitos; slugs precisam de membership e unicidade, EOF completo antes de responder. Ordenação de empate é determinística por slug.

Resultados distinguem NoCorpus (captura), PublishedEmpty (nenhuma fonte Active), NoHits e Hits. total conta matches após top_k, independentemente da página. Cada hit lexical conserva score e KnowledgeReferenceSource observado: slug, título, path, compilation_digest e namespace/witness literais. MissingProjection conserva namespace/witnessNone; não inventa owner a partir de seleção/alias. Physical digest e embedding fingerprint são conceitos distintos.

Os hits de uma página são lidos juntos, com um predicado `slug IN (...)` no
mesmo pin, em um único scanner até EOF. Somente slices Arrow dos registros
selecionados são retidos durante a montagem; não há uma abertura por hit ou
cópia dos corpos para DTOs. A montagem segue a ordem do ranking, mesmo quando
ela difere da ordem física, preservando o orçamento restante de cada preview.
preview=false guarda somente metadados/scores na resposta. Com preview=true,
Data passa body e summary emprestados ao formatter. A bridge Knowledge usa
preview_markdown existente; a página guarda somente text, source Summary/Body,
range UTF-8 e truncated. Os slices são liberados após a montagem da página;
não há Box<KnowledgeDocument> ou coleção de corpos na resposta Search.
Data valida intervalo e correspondência literal ao input escolhido, incluindo
a elipse quando há corte. O formatter recebe os bytes restantes da página e
seus limites próprios; configuração de preview integra o mesmo cursor.
Falha do formatter conserva o erro concreto e não entrega prefixo saudável.
max_output_bytes Data limita UTF-8 copiado da página; o consumidor ainda aplica
seu limite de serialização/HTTP. O endpoint Page completo conserva seu contrato.

KnowledgeSearchCursor é dado opaco com campos privados/Serde fechado. Vincula literalquery, modo lexical/colunas, seleção/session fornecida pelo caller, top_k, preview, página/limites, URI/version/branch/inherited e offset. Toda continuação precisa de admissão viva. Binding trocado é InvalidCursor; pin/geração divergente é GenerationChanged, mapeado a conflito causal pelo consumidor. Não há retry, adoção de latest, corpus-hash ou grant no cursor.

## Fronteira semântica e consumidores pendentes

query_native_knowledge_vector_search já implementa Semantic/Hybrid Native
sobre os pins alinhados da captura real, com current_inputs, fingerprints,
vector_id, dimensões e recipe verificados. RRF usa 1/(60+rank), combina os
rankings completos por slug e aplica top_k somente depois da fusão.
Scores lexical, distância semântica e RRF permanecem distintos.

O consumidor Models.Query/App conserva sua autorização e fornece vetor real
quando readiness for Ready. PublishedEmpty ou Unavailable observados dispensam
vetor fictício. Indisponibilidade Models realmente observada pelo App pode
produzir Hybrid Partial lexical; Auth, cancelamento, deadline, revogação, rede,
corrupção e erro tardio continuam falhas causais.

Os kernels readonly Data e o reader Imported semântico estão implementados;
I1 runtime49 qualificou os kernels Imported; o consumidor Models.Query/App permanece pendente. Multisource
continua exigindo SourceKey e estados/erros explícitos; fonte com falha não é
omitida nem tratada como vazia. HTTP/MCP/Ask/Search→Page→Refs/UI conservam os
escopos dos consumidores Main.

Falhas que chegam ao código público `knowledge_semantic_failure` registram no
stderr do daemon Source somente uma classificação fixa da causa tipada, como
`runtime_key`, `model_dimensions_missing`, `document_projection` ou
`embedding_schema`/`embedding_storage`. O diagnóstico não imprime a cadeia
Debug, mensagens internas, texto consultado, documentos, vetores, URLs ou
credenciais. O código HTTP/MCP permanece igual; a classificação não comprova
inferência Models nem publicação NN. O proxy CLI/MCP apenas recebe a resposta:
esse diagnóstico depende do código efetivamente ativo no Source.

## Fixtures e verificação

Contratos relacionados: [reconciliação documental](rust_knowledge_document_lifecycle.md), [seleção Knowledge](rust_knowledge_desired_reconciliation.md), [dados semânticos](../architecture/rust-knowledge-semantic-data.md), [Foreground](../architecture/rust-knowledge-foreground.md).

## Bridge de preview e prontidão NN atuais

Knowledge/search.rs expõe a query lexical com MarkdownPreviewLimits explícitos,
sobre a captura já existente. CapturedImportedKnowledge::search usa o mesmo
formatter e binding de seleção. Data separa ranking da materialização samepin.
knowledge_embedding_search_readiness compartilha o kernel de current_inputs,
cursor EOF, identity, dimensão, cobertura e fingerprint do query NN; somente
estados SDK observados dispensam Models.Query.

query_native_knowledge_vector_search recebe a única KnowledgeEmbeddingCapture,
identity, current_inputs e KnowledgeVectorSearchQuery: page lexical, modo
Semantic/Hybrid, vector opcional e métrica. Ready exige vetor válido; ausência
do vetor é erro nesse estado. PublishedEmpty ou Unavailable observados admitem
None, sem fabricar um vetor ou chamar Models. O consumer mantém sua autoridade
Models e executa uma Query real somente quando houver capacidade e readiness.

O kernel de baixo nível `query_knowledge_embeddings` recebe, por sua vez, um
`KnowledgeEmbeddingQuery` com vetor explícito: largura e finitude são validadas
também para um corpus vazio. Consumidores que precisam dispensar Models usam
readiness e a API de busca com vetor opcional; não passam `[]` como sentinela.

Um ou mais embeddings compatíveis com identidade e fingerprints atuais tornam
o corpus Ready, inclusive durante publicação incremental. Semantic retorna só
esse subconjunto; Hybrid mantém também os documentos lexicais sem vetor atual,
com semantic_distance ausente. Zero embeddings compatíveis conserva
Unavailable(Incomplete), sem fabricar resultados semânticos. current_inputs
continua cobrindo toda a Source e só pode ser usado depois do EOF documental.

Lexical usa source.pin e NN usa semantic.pin dessa mesma captura. NN pede a
cardinalidade dos inputs elegíveis pelo mesmo compatible_input do prefilter;
os hits precisam cobrir esse conjunto inteiro. Fontes ainda não processadas e
fingerprints obsoletos não entram na contagem NN. Ambos os rankings são
completos em seus conjuntos antes da fusão.
RRF usa 1/(60+rank), com ranks a partir de um, união por slug no mesmo pin e
desempate por slug. top_k corta depois da fusão; zero preserva todos paginados.
total conta o resultado após esse corte. Semantic ordena distância e slug.

O cursor fechado único vincula Source e geração NN observadas, seleção, modo,
texto, identidade/recipe/configuração, vetor, métrica, fingerprints compactos
e limites de preview/página. Nenhum cursor concede acesso ou adota latest.
Hits vector conservam lexical_score, semantic_distance e rrf_score separados.
Hybrid indisponível retorna Partial com motivo observado e somente resultados
lexicais; não informa rrf_score. Semantic indisponível retorna Unavailable.

A bridge guarda somente o preview owned e deixa o body/summary no batch local.
Um preview vazio pode caber exatamente quando restam zero bytes após metadata;
o formatter existente é preservado e Data aplica o cap real da página.

As execuções até attempt21 qualificaram onze funções Search distintas.
NoM26 acrescentou o seam lexical sobre a captura NN retida, totalizando
12 Search. Com as 12 funções Helper22, a evidência S3 atual reúne
24 funções distintas PASS. Data13, o replay Native18 e as regressões
posteriores não acrescentam funções sobrepostas. Os receipts preservam
resultados e tentativas históricas separadamente.
Vector17 protege RRF global antes do corte, cursor ligado à geração NN e
indisponibilidade observada; seus vetores ordinários não provam Models.Query.
Native18 protege preview vazio no limite exato e falha um byte abaixo.
Vector20 exige EOF documental antes da consulta e dispensa vetor somente
para PublishedEmpty ou Unavailable realmente observados.

Helper22 passou doze funções afetadas de query NN: corpus vazio, current_inputs,
identidade/dimensão, distância não finita real, no-op/reindex, first/named,
pins antigos, recriação física, CAS/Removed e Original/cancel/revoke/deadline.
Também protege EOF do cursor antes de query. São doze funções distintas
da API existente requalificadas pelo factoring, junto das 12 Search;
nenhuma soma com qualificação histórica R10 ou com replays.

## Source lexical na captura semântica

query_native_knowledge_embedding_source_search recebe a
KnowledgeEmbeddingCapture já retida e devolve a página lexical do mesmo
source.pin. A bridge Knowledge acrescenta MarkdownPreviewLimits e usa o
formatter borrowed existente. Não recebe identidade, fingerprints ou
vetor de modelo e não captura outro corpus. A capture já auditou FULL18 e
metadata/membership até EOF; este canal não certifica EOF do cursor
documental separado usado por CurrentInputs NN.

Reaplica Auth/Original/observações físicas e os mesmos read limits, usando
rank_lexical/search_validated/materialize_page. O cursor fechado existente
distingue esse domínio e vincula Source e NN generation observadas. O
consumer App declara Models indisponível realmente observado e Hybrid
Partial com score lexical, sem RRF; SemanticUnavailable é outro resultado
do consumer. Auth, rede, corrupção, cancelamento e prazo continuam erros.
A fixture embedding-source-lexical-runtime-26 passou (4.59s), incluindo
Source sem identidade de modelo, duas páginas, NN generation nova na mesma
Source conflitante, limite de output e cancelamento. Ela não prova
disponibilidade Models ou Models.Query. A regressão lexical27 passou seis
casos (28.91s runtime/39.338s total); Native preview28 passou dois
(2.78s/231.637s com lock compartilhado). São replays sem aumento das 24
funções distintas; strict30 passou (90.510s total). Tempos não são benchmark.

## I1 Imported: leitura semântica qualificada no SDK

Data knowledge_embeddings/readonly.rs e Knowledge imported_knowledge/semantic.rs
estão implementados. reader.capture_semantic(ReadLimits) retorna captura que
empresta o reader original e mantém pins/observações privados.

- next_documents() drena a mesma Source sem identidade, inclusive no caminho NoM;
  compartilha o cursor com next_inputs(identity), com drop/falha terminal.
- next_inputs(identity) usa a recipe vigente e calcula FP atuais por documento;
  não reetiqueta fingerprints armazenados; Some(empty) não certifica EOF.
- search_readiness(identity, current_inputs) verifica EOF, identidade, cobertura e FP.
- search recebe identity/current_inputs/vector query/SearchLimits/limites de preview.
- source_search consulta a mesma Source sem identidade ou vetor fictício.

Original, Registry/autorização, raiz e manifest do corpus único continuam
guarded com primeira causa concreta preservada. A extensão vetorial nullable é
validada no próprio schema. Se há visão NN, ela deve ter o mesmo URI, versão e
schema do pin documental; nenhuma observação pode selecionar outro head. Namespace/witness permanecem
literais por linha, independentes do mount; ver o
[contrato Data semântico](../architecture/rust-knowledge-semantic-data.md).

As qualificações históricas de dois datasets não substituem a regressão atual
de corpus único, seus readers Imported, CAS, EOF e transporte S3 real.

## Leitura selecionada para Ask

KnowledgeSearchCapture e KnowledgeEmbeddingCapture oferecem
read_selected_documents(Original, ReadKnowledge, slugs). O retorno
KnowledgeSelectedDocuments contém generation, state e documents owned.
NoCorpus continua uma observação anterior, na captura. IDs vazios/inválidos
ou repetidos são InvalidQuery(selected_slugs); o limite real de cada
identificador é preservado. Após essa validação, Source sem Active retorna
PublishedEmpty mesmo com IDs pedidos. Source ativa com [] retorna
EmptySelection; IDs desconhecidos ou Removed são erro nessa Source ativa.
Selected contém exatamente os documentos pedidos em ordem lexical de slug.

A leitura usa um único scanner do Source pin retido, Original/admissão viva
e as mesmas observações físicas. O predicado `slug IN (...)` seleciona no SDK
somente os documentos solicitados; o collector compartilhado com single Page
valida FULL18/C8, duplicatas, limites de batch e EOF desse stream selecionado.
A validação global da captura é preservada, sem uma nova varredura dos corpos
não selecionados. Apenas DTOs selecionados são copiados/retidos. Nenhuma captura
é reaberta e nenhum grant ou snapshot writable é exposto. A leitura
selecionada não consome nem certifica EOF do cursor CurrentInputs NN.
Erros ou falhas após prefixo/EOF descartam todo o resultado. SearchPage
continua guardando somente metadados, scores e preview; Ask usa a leitura
selecionada separada. Sua integração aos providers pertence ao consumidor App.

Cargo check Data33 passou. Selected-documents-runtime-34 passou duas funções
SDK/Auth nas duas capturas (138.05s runtime e175.107s total): Active/AllRemoved,
ordem/bodyliteral, estados/IDs, pin antigo, Original/cancel, identifier128/129
e cursor NN ainda incompleto. O collector single Page passou os casos focais35 de cauda inválida/duplicada
(0.06s runtime) e36 de guard terminal/cancel após EOF (0.09s). Strict Data37 passou lib e os targets knowledge_embeddings/knowledge_module_snapshot
com -D warnings (31.891s total).
Esses testes não reexecutam S3/RRF, clones ou consumers Main inalterados.

## Imported Source-only e exportação física

`reader.capture_source(ReadLimits)` retém a mesma Source pin e oferece
`next_documents()`, `source_generation()` e `recheck()`. Não projeta vetores nem requer modelo;
um valor vetorial inválido não bloqueia a leitura Markdown quando o schema
documental e sua extensão fechada são válidos. Lotes obedecem batch_rows;
Some(empty) é Removed e somente None certifica EOF. Drop/falha terminal,
Registry/admissão/Original e manifests preservam suas causas concretas.

`semantic.capture_physical_source(&SnapshotPolicy, SourceReadLimits)` retorna
um wrapper com o CapturedSourceReader real do Core e Pair emprestado já validado.
`reader()` verifica bindings; `recheck().await` compara heads SDK observados com
as gerações retidas, verifica conteúdo físico e repete os guards. O caller deve
mantê-lo vivo e revalidar antes e depois do transporte joined. Divergência é recusa;
a observação de head não substitui pins. Não há atomicidade entre dois datasets.
Policies/limites vêm do consumidor, sem novos defaults ou concessão PROJECT.

Check53 passou Knowledge lib em28.673s. A função física56 passou os quatro
cenários Source head, NN head, Pair e manifest retido alterado mantendo geração.
A tentativa56 teve uma falha separada na asserção Source-only de cursor abandonado;
a string foi alinhada ao contrato existente, sem mudar o kernel. Source-only58
passou (7.59s runtime): limites, NN/Pair inválidos, mixed/AllRemoved/EOF, pin antigo,
drop, retirement e Original Interrupted causal. I1 soma seis funções distintas
com49+56+58;51 requalifica NoM sem duplicar contagem. Strict Data59 passou (7.055s) e Knowledge61 passou após60 apontar
somente err_expect na fixture, corrigido para expect_err sem replay runtime.
As validações cobrem libs e os targets embeddings/search/first-publication,
imported_semantic/native_index_publication; transporte/export/install e Models.Query Imported são provas
separadas do App. NoM51 passou empty/Removed/EOF/Source-search/cancel sem identidade
(7.39s runtime); regressão Native vector52 passou duas funções (18.62s runtime).

Antes de staging, `KnowledgeSearchCapture::validate_export_destination` (Original, ReadKnowledge, destination) exige destino fora do dataset Source PROJECT efetivamente capturado; `CapturedImportedKnowledge::validate_export_destination` (destination) exige destino fora de todo o payload Imported readonly. Usam os bindings e autoridade retidos, verificados antes e depois, sem getter, segunda captura, NN, novo grant ou escrita. O destino deve ser absoluto, com pai existente canonicalizado; a comparação usa Path::starts_with por componentes contra a raiz capturada. Pai ausente conserva NotFound; destino interno é InvalidInput. O destino final existente é canonicalizado para recusar apontamento para Source. A guarda não impõe tipo ao destino externo ocupado: arquivo ou diretório seguem ao publisher NOREPLACE, que conserva Publication AlreadyExists sem alterar conteúdo. O App deve normalizar o caminho efetivamente publicado e manter seus rechecks cooperativos; não há isolamento atômico contra mudanças hostis de pathname.

A leitura de documentos selecionados aceita diretamente um callback FnMut, sem
Mutex local para adaptar a assinatura. O coletor conserva validação completa até
EOF e guardas físicos por batch; cancelamento e prazo acordam a espera SDK. O
cursor mutável compartilhado de streaming e os locks de publicação continuam
necessários e preservados.
