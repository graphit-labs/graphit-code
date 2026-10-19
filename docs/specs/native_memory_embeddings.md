---
references:
  - target: {type: task, id: tsk-8140}
    relation: implements
  - target: {type: task, id: tsk-222c}
    relation: implements
  - target: {type: task, id: tsk-265d}
    relation: implements
  - target: {type: task, id: tsk-23f2}
    relation: implements
  - target: {type: task, id: tsk-5b7b}
    relation: supports
  - target: {type: task, id: tsk-f454}
    relation: implements
  - target: {type: task, id: tsk-da58}
    relation: specified_by
  - target: {type: task, id: tsk-4c15}
    relation: requires_contract
  - target: {type: task, id: tsk-dec0}
    relation: requires_contract
  - target: {type: task, id: tsk-a684}
    relation: requires_contract
  - target: {type: task, id: tsk-5c9b}
    relation: supports
  - target: {type: task, id: tsk-3e62}
    relation: relates_to
  - target: {type: task, id: tsk-1597}
    relation: relates_to
---

# Embeddings Memory PROJECT nativos: contrato planejado

Público: integradores Records/Data/AI/Auth/Core e compositor Runtime. **Biblioteca captura/attach/query/recuperação qualificada no Main/Linux por `tsk-a684`**: 12 testes SDK reais, 30 library Memory e 44 integrações Memory afetadas, total de 86 casos distintos, seis doctests Records separados e scoped strict/fmt exit 0. A investigação Source-free `tsk-da58` e a decisão Root de 2026-10-11 definiram Memory como primeira entrega semântica. Schema/projeção/codec `tsk-4c15` e kernel Data List cosine `tsk-dec0` conservam suas qualificações próprias abaixo, sem somar rodadas ou contagens históricas. A integração daemon/ModelService/GGUF permanece fora desta prova. Busca FTS Task pertence ao refinamento separado `tsk-3e62`, não é busca semântica Task.

O [armazenamento Memory PROJECT](native_memory_store.md), o [índice e a query lexical](native_memory_search.md) têm qualificações próprias. O Source observado em [rows.rs](../../crates/graphit-records/src/memory_store/rows.rs) agora define schema24, 18 campos originais não nulos e seis extras nullable, metadata `project-v2-embedding-list`; essa alteração está qualificada no Main/Linux no escopo `tsk-4c15`. O [Data semântico Knowledge](../architecture/rust-knowledge-semantic-data.md), `tsk-1597`, está qualificado para aquele domínio e usa `FixedSizeList`; não fornece identidade, seleção histórica ou publicação semântica Memory. As primitivas entregues de [BranchSnapshot](../../crates/graphit-data/src/lance_branch_store.rs) conservam schema e scanner do Dataset capturado; os adapters Memory qualificados pertencem a `tsk-a684`.

## Produtores e sequência

| Folha | Contrato e ownership aprovados | Dependência e prova ainda necessária |
|---|---|---|
| `tsk-4c15` | Schema Memory24, identidade/projeção e codec puro [implementados em embeddings.rs](../../crates/graphit-records/src/memory_store/embeddings.rs), com exports e dispatch estreitos. | Qualificado Main/Linux: integração44 + library28 = 72 casos Memory distintos; Recordsdoc6 separados, strict/fmt0. Headers neutros e payload fechado fazem parte do Source atual; isso não qualifica captura/attach/query semântica. |
| `tsk-dec0` | API Data `vector_distance::cosine_distances` implementada e exportada; [contrato e oito testes funcionais Linux PASS](rust_vector_distance.md#executed-linux-scope), quatro puros/quatro SDK. | Independente do DTO/schema Records; strict/fmt exit 0 e doc `include_str!` compilado na lib, sem novos doctests. Persistência List, distâncias e causas foram executadas no SDK13 real; não é ANN nem integração Memory. |
| `tsk-a684` | Source real em [capture.rs](../../crates/graphit-records/src/memory_store/embeddings/capture.rs), [publish.rs](../../crates/graphit-records/src/memory_store/embeddings/publish.rs) e [query.rs](../../crates/graphit-records/src/memory_store/embeddings/query.rs), facade e [fixtures](../../crates/graphit-records/tests/memory_embeddings.rs). | Biblioteca Main/Linux qualificada: SDK12 + library30 + integração afetada44 = 86 casos distintos; Recordsdoc6 separados, scoped strict/fmt0. Não qualifica o caller daemon/ModelService/GGUF. |
| `tsk-5c9b` | Consumer daemon [MemoryEmbeddingModel](../../crates/graphit-daemon/src/model_service/memory_embeddings.rs), sobre Auth/ModelService/Records reais. | Main/Linux: jornada BF16 RPC→Memory real PASS (um teste de integração, 102,79s). Scoped clippy daemon lib/test `--no-deps -D warnings`, fmt e diff exit0. Records não depende do daemon. |
| `tsk-3e62` | Investigação independente de FTS Task. | Contratos/produtores Task próprios; não altera Memory ou o CatalogueFilter de outro owner. |

Root compõe manifests, locks e exports compartilhados. As provas históricas citadas usaram fila Cargo normal, warmcache e um job. As novas emissões usam a configuração padrão de recursos do engenheiro, sem nova reserva ou janela. Schema/projeção/codec e captura/attach/query/recuperação têm Source e qualificação Main/Linux próprios; compositor e demais consumidores permanecem gated. O kernel Data tem evidência funcional/strict própria. Planejamento completo não equivale a Source ou runtime qualificado.

## Consumer daemon Document/Query

A API genérica abaixo está em qualificação focal. As provas históricas do RPC
BF16 preservadas nesta página não qualificam automaticamente os novos caminhos
remotos ou os callbacks genéricos.

`graphit_daemon::model_service::MemoryEmbeddingModel::new(original, identity,
rpc_timeout)` retém o Original e a `MemoryEmbeddingIdentity` resolvida pelo
compositor. A identidade contém `vector_id`, dimensão positiva e
`MEMORY_DOCUMENT_RECIPE`. O consumer não toma um client ou lease Local e não
cria issuer, grant, worker ou chave. O compositor App resolve Local, Broker ou
Direct pelo serviço admitido real, usando a mesma identidade em cada operação.
Local mantém `ExpectedModel`, ConfigId e RPC estritos; remoto dispensa slot,
manifest e processo local. Conteúdo, recipe ou revisão incompatíveis mudam o
espaço; paths, threads e credenciais não o identificam.

`rpc_timeout: Duration` é a política concreta capturada pelo caller. Cada RPC
Document ou Query recebe um `EmbedOperation` com `Instant::now().checked_add(rpc_timeout)`.
O cliente mantém o menor prazo entre essa operação e a Original quando finita.
Uma mesma Original de serviço sem deadline pode atravessar todos os lotes; o
prazo de um RPC não vira orçamento de corpus nem renova autoridade. Os caminhos
PROJECT e USER compartilham essa API. No construtor do job PROJECT de indexação
em background, o default por RPC é 900 segundos (15 minutos), somente para a
origem `DistributionDefault`, cobrindo espera pela admissão e execução local.
A origem `Configuration` preserva valores explicitamente configurados em
`hub.limits.request.deadline_ms`, menores ou maiores, mesmo quando iguais ao
default do catálogo. Isso dá margem de processamento e fila; não é SLA, não
acelera o modelo nem garante conclusão universal. HTTP/CLI e Query
foreground conservam suas Originals finitas e a política de requisição existente;
um prazo externo menor e o cancelamento continuam vencendo. Entrada, identidade,
pesos e validade JWT do Broker (600 segundos) permanecem iguais. Cancelamento, admissões,
EOF, CAS e os limites concretos de cada entrada permanecem ativos.

`index(read, write, read_limits, write_limits, batch_limits, embed)` exige admissions PROJECT
Memory do mesmo Original. A escrita é revalidada antes da inferência. Captura o
corpus real e retorna `NoCorpus` sem conectar ao serviço se ele não existe. Cada
chunk de `next_documents` fornece o JSON literal Document e é dividido em
subrequests contíguos pelos `RpcLimits` explícitos fornecidos pelo compositor:
`min(max_texts, max_output_values / dimensions)` documentos e soma checked dos
bytes UTF-8 até `max_input_bytes`. Igualdade cabe; o documento seguinte inicia
outro RPC. A varredura Records consome EOF sem quota textual de corpus; `batch_rows` permanece um hint SDK. Um documento inteiro que não cabe produz `Store(MemoryStoreError::Limit)`,
antes de chamar o callback para ele. Um prefixo válido acumulado é publicado antes dessa falha.
Nenhum texto é dividido, truncado ou reordenado.

O caller extrai ID/revisão/fingerprint, consome cada projeção com `into_text()`
e move seu String completo para o request, sem cópia ou reaplicação da recipe.
Durante o RPC ainda retém o restante do chunk SDK atual, além do subrequest e
sua metadata; não acumula o corpus inteiro. Os vetores retornados são movidos
para `attach_embeddings` com a mesma captura opaca, em uma publicação por
subrequest. O backend aplica o prefixo uma vez. EOF vazio não publica patch vazio;
capacidade zero só falha quando existe documento. `embed(EmbedOperation)` é um
callback assíncrono que devolve os vetores ou a causa concreta do serviço. O
compositor captura uma admissão finita para cada lote de background, mantendo o
cancelamento e o prazo menor do Original. Os validators Auth/client/RPC ou do
protocolo HTTP selecionado permanecem ativos. A partição limita count, bytes crus e contagem de valores Float32;
`max_frame_bytes` inclui o envelope serializado e continua validado pelo SDK,
sem margem inventada no consumidor.

`MemoryEmbeddingIndexRead::Complete` contém a geração da captura fonte e a última
attachment observada, ou `None` quando nenhuma nova attachment foi observada:
corpus publicado vazio ou revisões já integralmente covered. Cada lote é uma
publicação independente. `MemoryEmbeddingFailure<E>` conserva `cause` tipada
(`Service(E)` ou `Store`) e a última attachment que retornou sucesso antes dessa falha;
isso não é um recibo transacional de todo o índice. O erro Store continua podendo
carregar `EmbeddingConfirmed` com publicação/causa, ou `Commit` com recovery key
incerta. Para recuperar, use diretamente `NativeMemoryStore::resolve_embeddings`
com essa chave real e um Original/admission atuais autorizados. Não há replay,
rollback ou renovação automática de autoridade. As guardas concretas de request/Model/SDK permanecem; a leitura Records não tem quota agregada de corpus nem ledger entre fases.

`query(read, text, options, limits, embed)` usa a captura real para evitar
inferência quando o corpus é ausente ou publicado vazio. O getter observacional
`MemoryEmbeddingCorpus::is_empty()` consulta os IDs já capturados, sem projetar
documentos ou repetir a varredura. Ele não valida autoridade ou liveness. O EOF
de `next_documents` indica que nenhuma revisão precisa de embedding; um corpus
não vazio integralmente covered continua executando a query RPC. Para corpus não vazio, envia
uma única string em `EmbedInput::Query`, valida quantidade, dimensão e valores finitos e passa o
vetor para `MemoryEmbeddingCorpus::search_semantic` com a mesma identidade,
Original e pin capturado antes da inferência. O caminho híbrido usa a mesma
captura. Uma escrita durante a inferência aparece apenas na próxima captura;
a resposta atual conserva a geração, os heads e os vetores do seu snapshot.
Se as credenciais S3 expirarem durante a inferência, a renovação do transporte
reabre essa mesma versão, sem trocar por latest.
Preserva os quatro estados Records, filtros e erros sem fallback lexical.

A seleção PROJECT e USER percorre o scanner SDK ordenado por `record_id` até
EOF, validando uma cadeia e seus embeddings por vez. Não abre novos scans de
histórico e vetores para cada Memory. Retém somente a cadeia atual, candidatos
lexicais leves e ranks; os corpos dos `top_k` selecionados são lidos do mesmo
pin. As regras de histórico, fingerprints, revisão preferida e filtros atuais
continuam compartilhadas com os produtores existentes. Isso remove os scans
repetidos; não constitui uma promessa de latência independente do corpus ou
da inferência. Datas permanecem metadata do head.

As varreduras revalidam autorização e o namespace capturado na entrada, antes e
depois de cada leitura de batch do SDK, no EOF e antes da entrega. Decodificação,
validação de cadeias, scores e fusão de ranks já em memória verificam o Original
por linha; não reabrem a cadeia de diretórios em cada operação de CPU. A captura
PROJECT toma projeto e Original pela mesma revalidação da admissão. Cancelamento,
revogação e substituição de diretório continuam descartando resultados parciais.

A integração em [tests/memory_embeddings.rs](../../crates/graphit-daemon/tests/memory_embeddings.rs)
usa instalação temporária, caller Runtime/issuer único, admissions PROJECT e
lease local reais, listener Core/TCP, `serve_connection` e o BF16 GGUF fixado.
A qualificação histórica `tsk-5c9b` executou e passou no Main/Linux em 102,79s: MRL128 e 768, mudança
vector_id com corpus768, NoCorpus/PublishedEmpty sem RPC, falha física Store após
um lote publicado, falha de transporte, reindex, recuperação Applied com Original
novo, limite SDK, deadline, revogação da lease local e cancelamento. A revogação
do perfil local produz Model(Authentication); o caller Runtime dos Records tem
sua própria admissão. O shutdown real precede o descarte de NativeSdkRoot.
Preservação da variante Store completa, inclusive EmbeddingConfirmed/Commit,
é verificada pelo fluxo tipado; esta jornada não injeta falha na janela interna
de commit para produzir essas duas variantes. As qualificações SDK do produtor
continuam separadas. Ela não comprova endpoints App, hosts instalados, JWT Native
em produção, Windows/macOS ou a entrega N3 inteira.

A composição portátil atual passou no Linux em 2026-10-21: a jornada GGUF
Document/Query com MRL128/768 e efeitos parciais (142,27 s), as fronteiras de
partição (161,52 s), a consulta híbrida com Original contínuo (57,94 s) e o
cancelamento/join do servidor sem conexão. São quatro casos distintos; a prova
USER tem sua [contagem separada](native_user_memory_embeddings.md#qualificação-funcional).
Cada caso que inicializa o SDK executa em seu próprio processo. As fronteiras
usam corpora reais independentes, preservando cardinalidade, vetores e número de
RPCs. A jornada híbrida aquece o SDK com uma operação real antes de testar seu
limite inalterado de cinco segundos por operação.

A revogação do serviço Runtime modifica o binding de embedding do provider pelo
produtor Auth real e exige `Service(Authentication)` antes de consultar o futuro
de conexão. Trocar credenciais do perfil Local não revoga esse binding Runtime;
a admissão PROJECT continua independente. O teste longo isola a construção de
seus futuros para reduzir os temporários de debug, sem ampliar pilha ou prazos.
Essas provas usam o serviço Local e não qualificam endpoints App, serviço remoto
USER ou a política de idle de Models.

Defina `GRAPHIT_EMBEDDINGGEMMA2_MODEL` com caminho absoluto para o BF16 fixado,
557950176 bytes, SHA256 `68bae29d62fb8c7d23e98d21fd4662753ddd636e6b62b8a70dfe059a9844f216`.
A partir da raiz do projeto, a invocação Linux é:

```sh
cargo test --locked --offline -p graphit-daemon --test memory_embeddings actual_memory_gemma_rpc_index_query_identity_and_partial_failure -- --ignored --exact
```

O teste é ignorado por padrão porque exige esse artefato nativo. A compilação
ou execução sem `--ignored` não comprova a jornada. O log qualificado é
`.build/rust-memory-consumer-5c9b/native-3.log`; as duas primeiras falhas de
fixture permanecem nos logs `native-1-fixture-path.log` e `native-2.log`.

A correção de partição `tsk-5b7b` foi qualificada na biblioteca daemon/Linux. O focal
`actual_memory_rpc_partition_boundaries` usa três documentos no mesmo chunk,
RPCs reais limitados a dois por count, bytes UTF-8 ou valores de saída, cobertura
semântica de IDs/revisões/vetores e falha TCP no segundo transporte conservando
a primeira publicação. Reindexações conservam o resultado real Published ou
Existing. Também verifica prefixo válido antes de documento oversized, documento
indivisível e dimensão maior que a capacidade
antes de qualquer conexão. Native02 executou um teste PASS em 97,35s; a regressão
`actual_memory_gemma_rpc_index_query_identity_and_partial_failure` executou um
PASS em 94,83s. Scoped Clippy lib/test `--no-deps -D warnings` passou em 12,34s;
fmt, LF e links locais passaram. Logs atuais em
`.build/rust-runtime-memory-partition-5b7b/{native-02,regression-01,strict-01}.log`.
Native01 conserva a falha histórica da expectativa Published em reindexação;
o SDK real pode retornar Existing. Esta correção não qualifica o factory M,
Foreground/registry, hosts instalados, Windows/macOS ou a entrega N3 inteira.
Invocação do focal:

```sh
cargo test --locked --offline -j4 -p graphit-daemon --test memory_embeddings actual_memory_rpc_partition_boundaries -- --ignored --exact --test-threads=1
```

## Schema único e família de embeddings

O Source `tsk-4c15`, implementado e qualificado no Main/Linux, mantém a mesma tabela Memory: 18 colunas originais, mais seis nullable, com metadata canônica `project-v2-embedding-list`:

| Coluna extra | Tipo Arrow |
|---|---|
| `embedding_vector_id` | `FixedSizeBinary(32)` |
| `embedding_fingerprint` | `FixedSizeBinary(32)` |
| `embedding_recipe` | `Utf8` |
| `embedding_role` | `Utf8` |
| `embedding_dimensions` | `UInt32` |
| `embedding_vector` | `List<Float32>`, field filho nullable=true |

Head, creation e revision ordinários têm as seis extras NULL. A nova família `kind="embedding"` usa `row_key=embedding/{MemoryId}/{revision}`, `record_id=MemoryId`, projeto da fonte e revisão lógica positiva existente. Ela tem as seis extras não nulas juntas: vector_id/fingerprint exatos de 32 bytes, receita exata `memory-document-json-v1`, role `Document`, dimensão positiva igual ao comprimento da lista e componentes Float32 não nulos e finitos. Nullability do field filho não autoriza componentes nulos. Combinações parciais, tipos incompatíveis ou bindings forjados são erros.

Os demais headers dessa família são neutros exatos: `operation_version`, `idempotency_key`, `idempotency_actor`, `title`, `created_by` e `updated_by` vazios; `created_at`, `updated_at` e `reference_generation` zero; `superseded` e `tombstone` false; `references="[]"`. O payload JSON fechado contém apenas `{memory_id, memory_revision}`, com valores iguais aos headers e reencode canônico validado. Vetor e identidade ficam nas seis colunas, sem vetor JSON. Neutrais não representam a autoria ou data da Memory lógica; captura, branch física e versão SDK vinculam a publicação derivada.

Embedding não é nova Memory, revisão lógica ou evento de auditoria. Dispatch valida o `ProjectId` esperado **antes** de separar/ignorar a família e só a separa após validar shape/header/payload/reencode; ela não entra em attempts, creation/history, catalogue, seleção lexical ou referências. Leituras lógicas não copiam/retêm vetores. Orphans, fingerprint e elegibilidade semântica pertencem à query/attach posterior, não à validação de shape isolada. Todas as linhas, inclusive embeddings ignorados pela leitura lógica, continuam sujeitas a EOF e validação integral de schema/header/payload/recipe/role, sem quotas rows/text do corpus. A validação toma slices emprestadas do vetor. Create verifica presença **física**, não apenas linhas lógicas: uma família embedding-only com o mesmo ID é colisão, não corpus vazio disponível.

Schema18 antigo é erro explícito de incompatibilidade, sem migração, overwrite, adoção ou rebuild automático. `List` permite mudar dimensão sem alterar o tipo da coluna; isso não torna identidades de modelos compatíveis. Não há sidecar, ANN, dimensão arbitrária ou threshold novo.

## Texto documental e identidade

As APIs puras abaixo estão **implementadas e qualificadas no Main/Linux no escopo `tsk-4c15`**, exportadas por `graphit_records::memory_store`. `MemoryEmbeddingIdentity` tem campos privados: `new(vector_id:[u8;32], dimensions:NonZeroU32, recipe:String)` retorna Result e exige a receita exata `memory-document-json-v1`. Os getters são `vector_id()`, `dimensions()` e `recipe()`. `MemoryEmbeddingDocument` também tem campos privados e getters `memory_id()`, `revision()`, `text()`, `fingerprint()` e `identity()`; associação current/archive pertence à captura/query `tsk-a684`.

```text
prepare_memory_embedding_document(
    memory: &Memory, identity: &MemoryEmbeddingIdentity,
) -> Result<MemoryEmbeddingDocument, MemoryStoreError>

encode_memory_embedding_row(
    project_id: ProjectId, document: &MemoryEmbeddingDocument,
    vector: &[f32], limits: MemoryStoreWriteLimits,
) -> Result<RecordBatch, MemoryStoreError>
```

O encoder puro valida shape/dimensão/finitude e limite `max_row_bytes` de cada linha escrita, e retorna um RecordBatch schema24. Não publica, autentica, captura autoridade ou chama modelo. A projeção rejeita revisão zero; os getters preservam ID/revisão próprios sem inserir esses valores no texto ou fingerprint.

O texto é a serialização serde JSON da tuple `[title, body, tags]`, literal UTF-8, conservando whitespace e ordem das tags. ID, actor, reason, referências, flags e datas não entram no texto. O fingerprint é SHA256 dos bytes da tuple serde JSON `[recipe, "Document", vector_id32, dimensions, text]`, onde `text` é a string JSON literal da projeção, exatamente a enviada ao serviço; não é uma segunda coluna ou array de bytes no payload. O backend aplica o prefixo Document uma vez; o caller não o antecipa.

### Consumo owned do texto documental

`tsk-f454` acrescenta `MemoryEmbeddingDocument::into_text(self) -> String` em
`memory_store/embeddings.rs`. O caller lê `memory_id()`, `revision()`,
`fingerprint()` e `identity()` **antes** de consumir o documento, copiando ou
clonando apenas a metadata necessária à associação posterior; não conserva
borrows do documento consumido. `text()` emprestado e os
getters existentes permanecem inalterados.

`into_text` devolve `self.text` por move: texto completo já projetado, mesma
allocation, comprimento, capacidade e bytes UTF-8. Não copia/reencoda/trunca,
reaplica a receita ou acrescenta prefixo; a regra do backend acima permanece.
O documento é consumido. A API é pura e síncrona, sem I/O, grant, captura, schema,
DTO ou autoridade de modelo. O consumidor `tsk-5b7b` possui sua própria entrega;
a seção Runtime Document/Query e seus limites não são alterados por este getter.

O produtor mínimo `tsk-f454` está **qualificado no escopo puro Linux**. O teste
exato `memory_embedding_document_into_text_preserves_allocation_and_metadata`
passou em owned-02: um PASS/zero FAIL, 90 filtrados (0,01s). Usa preparação
real e confirma ponteiro/len/capacity, bytes UTF-8 completos e metadata obtida
antes do move. Compile-02 passou (88,19s) e strict-02 Records --lib --no-deps
-- -D warnings passou (92,53s); fmt-01/diff-01 também passaram. Tempos de execução/
compilação pertencem à evidência, sem inferência de benchmark.

O histórico preserva compile01/strict01 bloqueados por E0609 em Data alheio;
o dono Knowledge corrigiu source_slugs.contains antes dos resultados aprovados.
Três warnings antigos de helpers cfg-test sem uso pertencem ao lib-test, não
falha do novo Source; strict lib terminou exit0. Logs/resultados ficam em
`.build/rust-records-memory-embedding-text-move/evidence`; o receipt correspondente
é `.build/rust-records-memory-embedding-text-move/validation.json`.
Nota de disponibilidade (metadata readonly, 2026-10-12): o diretório de evidência
e o receipt f454 acima estão atualmente ausentes. `tsk-f454` permanece COMPLETED12;
os PASS concluídos antes do alerta são históricos e não foram reexecutados ou
convertidos em falha. Não se atribui causa/actor da ausência nem se reconstroem
arquivos ou hashes.

Esta prova qualifica somente o getter/data ownership, sem qualificar BF16/RPC,
integração Main/global, Task lifecycle ou o consumidor daemon `tsk-5b7b`.

Title/body/tags, vector_id, receita ou dimensão alterados mudam o fingerprint; flags/referências/data/autor/ID não mudam o texto documental nem seu fingerprint. Cada revisão lógica mantém sua chave própria. Igualdade de largura não substitui vector_id. A consulta usa role Query, a projeção usa Document, e geração do ModelService não é fence do corpus. Nenhuma coluna/hash de todos os metadados substitui o binding à fonte lógica.

## Captura, inferência, attach e resolução implementados

As APIs `tsk-a684` estão implementadas, exportadas por `graphit_records::memory_store` e qualificadas como biblioteca Memory PROJECT no Main/Linux. Os resultados funcionais/strict estão discriminados abaixo; não são apenas prova de compilação. ModelService, GGUF e o consumer daemon não foram qualificados por estas suítes SDK-only.

| Operação async | Entradas além do original/admissão reais | Retorno |
|---|---|---|
| `NativeMemoryStore::capture_embeddings` | `MemoryEmbeddingIdentity`, `MemoryStoreReadLimits` | `Result<MemoryEmbeddingCorpusRead, MemoryStoreError>` |
| `MemoryEmbeddingCorpus::next_documents` | `&mut self`, mesmo original e ReadMemory | `Result<Option<Vec<MemoryEmbeddingDocument>>, MemoryStoreError>` |
| `NativeMemoryStore::attach_embeddings` | `&MemoryEmbeddingCorpus`, `Vec<MemoryEmbeddingAttachment>`, `MemoryStoreWriteLimits` | `Result<MemoryEmbeddingAttachOutcome, MemoryStoreError>` |
| `NativeMemoryStore::resolve_embeddings` | `&DeltaRecoveryKey`, `MemoryStoreWriteLimits` | `Result<DeltaResolution, MemoryStoreError>` |
| `NativeMemoryStore::search_semantic` | identidade, `&[f32]` query, `MemorySemanticSearchOptions`, `MemoryStoreReadLimits` | `Result<MemorySemanticSearchRead, MemoryStoreError>` |

Original é `&Arc<SnapshotContext>`; capture/next/query exigem a `ProjectReadAdmission` real Memory, attach/resolve exigem ordinary `ProjectWriteAdmission` Memory. Auth, contexto e guard são rechecados antes/depois das fronteiras SDK/await, batches e output. Scope/projeto/versão/request e identidade não vêm de metadata caller forjada.

`MemoryEmbeddingCorpusRead::{NoCorpus, Published(MemoryEmbeddingCorpus)}` distingue ausência de corpus capturado. O opaque retém Arc original, pin/guard, `MemoryGeneration`, identidade, cursor e IDs/metadados leves. Seus getters observacionais são `generation()`, `identity()` e `is_empty()` (IDs capturados). `next_documents` só avança o cursor após o check final; `None` é EOF de Published, inclusive publicado vazio, não NoCorpus. Descoberta inclui IDs de todas as famílias físicas para não ocultar orphan embedding-only; a cadeia e os documentos são validados no mesmo pin.

A captura mantém **uma cadeia/documentchunk residente**, sem corpus duplicado de bodies/vetores; conserva metadados leves ID/revisão/fingerprint dos documentos emitidos para validar attach. Cada **scan SDK completo** valida toda linha física até EOF, inclusive embeddings ignorados pela leitura lógica e recipe/role, sem quotas ou contadores agregados rows/text. `batch_rows` é hint. Discovery, cadeia, embeddings e lookup final preservam schema/keys/owner/Auth/cancelamento e arquivos nativos; request/Model/input/encoded-row guards permanecem separados do corpus. Não há ledger, quota/reserva ou garantia RSS. Erro tardio/cancel invalida o resultado, e lookup final usa o mesmo pin.

<a id="project-emissão-somente-de-coverage-ausente-em-validação"></a>

### PROJECT: emissão somente de coverage ausente, SDK Linux qualificado

`tsk-23f2` implementa o novo comportamento PROJECT em
[capture.rs](../../crates/graphit-records/src/memory_store/embeddings/capture.rs).
A mudança está **qualificada como produtor SDK no Linux em 17 funções distintas**:
14 casos SDK PROJECT +duas unidades puras atuais +um focal USER do fingerprint
compartilhado. As qualificações históricas não são replay nem adições a 17;
os arquivos ausentes citados nas notas de disponibilidade não foram restaurados.
O consumidor Runtime/RPC/ModelService tem execução própria, fora deste produtor.

Antes de construir cada OwnedDocument, a captura valida a cadeia e metadata
física até EOF no mesmo pin: schema/owner/header/payload, keys duplicadas, shape/
finite vectors, orphans, fonte e fingerprint, além dos limites/checks atuais.
Um HashSet privado conserva somente as revisões cuja identidade exata
(vector_id32/dimensão/receita) e fingerprint da fonte foram validados. Revisões
covered avançam o cursor sem emissão; só revisões missing são projetadas e
incluídas na allowlist. `None` é EOF de Published quando tudo elegível já está
covered, não NoCorpus, publicação, novo key ou permissão para materializar.

Novo ID ou revisão elegível é emitido, inclusive revisão apenas de flags com
texto/fingerprint iguais. Trocar vector_id ou dimensão emite as revisões elegíveis
para a identidade nova. Corrupted metadata aparentemente Existing não vira no-op:
fingerprint forjado, vector inválido, orphan/wrongscope, limites, cancelamento
ou erro tardio continuam causais, após validar todo input consultado. Cada scan
usa seus próprios bounds completos; não há ledger entre fases/RSS/novo decoder.
O check final precede avanço do cursor e entrega de None.

O helper privado de fingerprint em
[embeddings.rs](../../crates/graphit-records/src/memory_store/embeddings.rs)
usa serde para escrever a tuple interna title/body/tags em stream e escapar
quotes/backslashes para sua inclusão exata como string na tuple externa
[recipe,Document,vector_id32,dimensions,text]. Conserva UTF8/controles/ordem e os
mesmos bytes/hash do prepare público, sem construir OwnedDocument/texto duplicado
para validar uma row covered. History continua decodificando/validando Memory
strings completas: não se promete evitar body parse ou substituir a cadeia por
um novo pipeline. prepare_memory_embedding_document, into_text, codec e
MemoryEmbeddingIdentity públicos permanecem inalterados; o wrapper USER mantém
validação compartilhada e a emissão USER anterior.

Uma captura inherited integralmente covered retorna EOF sem birth/materialização/
publicação/key. Attachment forjada de uma revisão omitida falha por un-emitted;
apenas documentos realmente emitidos autorizam essa associação. Recomputar por
nova revisão/model identity usa publicação/key reais quando houver attach;
Writer/CAS/Confirmed/uncertain cause/resolve e concorrência permanecem os atuais.
ExpectedModel escolhe ModelId/revisão/config/dim reais nos produtores Core/AI e
vector_id existente, sem acrescentar campos Records ou grant por metadata.

sdk03 passou 14/zero falhas com o ELF atual memory_embeddings-a8a38d573a0e74fe,
após compileSDK03 aguardado. unit02 passou as duas novas unidades puras no ELF
atual após compileunit02; USER fingerprint focal01 passou um caso, com o stream
de produção inalterado. Esses **17 distintos** incluem os dois novos SDK e os
quatro antigos legitimamente ajustados; nenhuma rodada histórica 86 ou subset é
somada. As unidades puras cobrem UTF8/controles/chunks/canonical equivalence e
rejeição de revisão zero.

Strict03 --lib --test memory_embeddings --no-deps -- -D warnings passou exit0
com Source05 coerente após a correção Core de seu próprio cfg-test. O fp/capture
de produção Source03 permaneceu inalterado; Source04 alterou apenas a fixture de
finite/Option deadline e Source05 o uso de quatro args NativeDocumentFileLimits
no cfg-test do dono. Não houve relaxamento de asserts, novo modelo ou grant.
USER compile02 atual concluiu exit0 (333,59s), seguido de user-fp-02 no ELF
atual: um focal exato PASS/zero falhas (4,90s), já incluído no único USER dos 17
distintos. fmt03 de Source05 passou (0,07s), e diff03 após o fechamento
documental anterior concluiu exit0. A QA documental tem escopo separado; nenhum
rerun amplia a contagem ou qualifica consumer RPC/Now.

O histórico preserva SDK01 E0369 do getter finite legado; SDK02/strict02
interrompidos por oito erros Core WIP; e strict01 exit0 que precedeu a mudança de
fixture Source3→4 e não serve como strict coerente final. Todos os logs atuais
estão em `.build/rust-records-memory-embedding-coverage-skip/evidence`. O receipt
tem como local `.build/rust-records-memory-embedding-coverage-skip/validation.json`
e escopo somente de execuções próprias atuais, sem reconstruir receipts
históricos ou antecipar existência/hash do arquivo.
RPC, consumidor M/3b96/5b7b, Main global e desempenho não são qualificados por
esta entrega SDK-only.

Inferência ocorre fora do ArtifactWriter pelo caller. `MemoryEmbeddingAttachment` contém `memory_id:MemoryId`, `revision:NonZeroU64`, `input_fingerprint:[u8;32]` e `vector:Vec<f32>`; identidade vem do opaque. Cada ID/revisão/fingerprint precisa ter sido efetivamente emitido pela captura. Batch duplicado, fonte inexistente/alterada ou vetor não finito/dimensão errada falha antes de publicação. Attach parcial é permitido e cada lote confirmado fica consultável. A query usa somente revisões elegíveis com vetores compatíveis, sem esperar cobertura total; nenhum completion marker/ledger é criado.

Sob Writer/WriteMemory originais, attach usa o **pin SDK atual do publisher**, revalida projeto/versão/branch física e fonte ID/revisão/projeção/fingerprint. Revisão arquivada válida pode receber vetor; update concorrente pode criar revisão ainda sem embedding; revisões já cobertas continuam consultáveis enquanto a nova aguarda captura/attach. Não usa a versão SDK antiga da captura como CAS de todos os batches. O merge/commit usa CAS atual e preserva a concorrência entre batches. Valores idênticos retornam `MemoryEmbeddingAttachOutcome::Existing { generation }`, sem RecoveryKey, materialização inherited ou publicação. Efeito novo materializa somente após validação e retorna `Published(MemoryEmbeddingPublication { generation, recovery_key })`.

`generation` conserva projeto/versão lógica, branch/inheritance e versão SDK; a recovery key real conserva URI, branch física, baseversion e transaction ID. Não há binding/authority framework novo. Key só existe após prepare real. Commit incerto preserva `MemoryStoreError::Commit(DeltaCommitFailure)` com causa/key originais, sem inferir rollback/replay. Commit conhecido seguido de check falho retorna `MemoryStoreError::EmbeddingConfirmed { publication: Box<_>, cause: Box<_> }`, conservando publicação e causa.

`resolve_embeddings` sob fresh original/admissão WRITE verifica URI/branch física da key antes de corpus I/O, captura o pin atual sob Writer sem birth/materialize/replay e usa a resolução SDK real. Ausência ou named target ainda herdado retorna Unresolved; demais Applied/Conflicted/Unresolved/erros preservam observação e causa. Para continuar emissão/attach/query após cancel, cria-se **nova captura ligada ao novo original e a uma nova admissão real**; o antigo opaque falha DifferentRequest, sem renovar authority. Records verifica original/projeto/versão/guard; a captura não retém nem compara explicitamente actor/issuer. Auth determina a admissão atual, e resolve verifica o binding físico da key. Uma obrigação futura de recuperação pelo mesmo actor no ModelService pertence ao contrato desse produtor. Não se promete identidade estável após restart do issuer, SDK lost-ack interceptado ou atomicidade contínua Auth/SDK.

## Distância e seleção semântica

O adapter Data **implementado** é [`graphit_data::vector_distance::cosine_distances`](rust_vector_distance.md#public-api-and-errors), `cosine_distances(vectors:&ListArray, query:&[f32]) -> Result<Vec<Option<f32>>, VectorDistanceError>`, usando a primitiva pública SDK13 importada como `lance_linalg::distance::cosine::cosine_distance`. Sua suite focal teve oito testes distintos PASS (quatro puros/quatro SDK), strict/fmt exit 0 e doc `include_str!` compilado na lib, sem novos doctests. Query de comprimento positivo e valores finitos e cada vetor não nulo devem ter dimensão exata, child Float32 não nulo/finito. Outer NULL produz None no kernel; Memory ignora revisões sem vetor na seleção semântica, mantendo as validações estruturais. Distância SDK deve ser finita; zero vector/zero query ou overflow que produzam NaN/Inf são erro causal, sem clamp, padding ou sentinel. Não se assume distância positiva universal.

A persistência List e cosine pública são contratos diferentes do scanner nearest de Knowledge, que aceita FixedSizeList. Este kernel não cria índice vetorial, top-k ou autoridade; toma slices emprestadas e retorna apenas scores por batch.

A query Memory implementada e qualificada no escopo da biblioteca Main/Linux valida todas as cadeias no EOF e forma documentos de creation/revisão1 e cada revisão não-delete; a cópia final tombstonada não acrescenta outro documento. As opções são `MemorySemanticSearchOptions { filter:MemoryListFilter, include_deleted_history:bool, top_k:NonZeroUsize }`. Os cinco predicados AND reais — classification, tag literal, mandatory, important, exclude_mandatory — e deleted usam o **head atual antes de scoring/top-k**. `updated_at` é metadata do head retornado, não filtro temporal/janela implícita; nenhum campo de data é adicionado. Histórico tombstonado só participa sob opção explícita, com `location=Archived` e metadata final da cadeia tombstonada em `current`, sem ressuscitar `get`.

Cada vetor utilizado precisa de identidade/fingerprint/dimensão compatíveis. Revisões pendentes não bloqueiam revisões já processadas, inclusive no mesmo Memory ID. Para cada Memory escolhe-se a menor distância cosine; empate favorece a revisão current e, depois, a maior revisão numérica. Após EOF e collapse por ID, ordena-se globalmente por **distância ASC, ID estável**, e só então aplica-se top-k. `MemorySemanticHit` contém somente `current`, `matched_revision`, `location` e `distance`, sem campo `deleted`. A geração capturada pertence ao resultado `MemorySemanticSearchRead::Published`. Não se misturam identidades vetoriais nem se retornam resultados após falha de integridade. Cobertura parcial válida é um resultado publicado, distinto de um stream interrompido ou corrompido.

`MemorySemanticSearchRead` distingue `NoCorpus`, `PublishedEmpty { generation }` só para tabela lógica e física realmente vazia, `Unavailable { generation, reason }` com `MemoryEmbeddingUnavailable::{MissingCoverage, IncompatibleIdentity}`, e `Published { generation, total_hits, hits }`. Quando não existe nenhum vetor compatível entre as revisões elegíveis, retorna Unavailable com MissingCoverage ou IncompatibleIdentity; quando há vetores compatíveis, a cobertura pendente não bloqueia Published. Zero hits por filtros em Published é NoHits, sem novo enum/sentinel. Query shape/dimensão/nonfinite, corrupção e erros SDK são causais, não Unavailable genérico. Duplicatas, orphans, fingerprint forjado, extras parciais ou cadeia incoerente dão erro sem resultado parcial. Vetores não são retidos como cópia de corpus; o produtor conserva melhores IDs/distâncias/metadados e resolve o conteúdo selecionado no mesmo pin, com EOF e as guardas concretas do lookup final.

A [query lexical](native_memory_search.md#query-lexical-tsk-612f) permanece inalterada: current match vence, senão maior revisão correspondente; ordem categoria/data/ID, score como metadata. A mudança para distância afeta **somente o contrato semântico qualificado de `tsk-a684`**, não catalogue, recall lexical ou provas anteriores.

## Incremental e reaproveitamento

PROJECT e USER omitem revisões já cobertas na captura. Para uma revisão nova,
o vetor é reaproveitado se existir na mesma cadeia um embedding validado com
identidade e fingerprint idênticos. O fingerprint considera título, corpo,
tags, modelo/dimensões e receita Document; mudanças apenas de atributos ou
referências não exigem nova inferência. O mesmo mecanismo atende stores locais
e S3, sem corpus paralelo. A nova associação de revisão passa pelo attach/CAS
normal, com as mesmas guardas; não cria revisão lógica nem evento de auditoria.

Mudanças de registros notificam os jobs de registros sem solicitar uma varredura
de código/documentação. Esses jobs compõem com AST/Knowledge, sem aguardar o
full desses módulos. Notificações de registros podem interromper inferência
pendente, mas preservam o percurso estrutural em andamento; o próximo lote
retoma os vetores pendentes a partir do corpus persistido.

O consumidor separa os vetores reaproveitados dos documentos que precisam de
inferência em cada lote e preserva a associação por revisão. Retomadas capturam
somente o restante. Troca de texto/modelo/receita exige novo embedding, e uma
falha posterior conserva os lotes já publicados e consultáveis.

## Qualificação de schema/projeção no Main/Linux

A evidência `tsk-4c15` é **integração `memory_store` 44 PASS/0 falhas**, runtime 64,68 s, e **library filtro `memory_` 28 PASS/0 falhas, 32 filtrados**, runtime 4,88 s: **72 casos Memory distintos**. Os **seis doctests Records PASS** (quatro normais/dois compile_fail) são separados, não seis novos doctests Memory. Scoped Clippy `--locked -p graphit-records --lib --test memory_store --no-deps -- -D warnings` concluiu **exit 0**, 17,67 s; fmt dos 13 arquivos próprios concluiu **exit 0**. Essas durações não são benchmarks. Contagens anteriores CRUD/catalogue/lifecycle/index/lexical são históricas e não somam a esses 72.

O histórico conserva a falha inicial de compilação corrigida no caller e o linker Bus error/ENOSPC 28, com recuperação externa Root; isso não vira falha de domínio ocultada. O warning de três métodos unused numa fixture Task alheia de library-test foi comunicado ao owner Root e não suprimido. O strict qualificado foi o escopo produção + integração Memory acima. Logs/resultados e recuperação pertencem à evidência da folha, sem inferir o lifecycle de conclusão da Task.

Esta prova `tsk-4c15` qualifica schema24/projeção/codec e seus consumidores Memory afetados. Captura/attach/query/recuperação têm a prova distinta `tsk-a684` abaixo; daemon `tsk-5c9b`, ModelService/GGUF, USER/Dream e outros sistemas operacionais permanecem fora desses escopos.

## Qualificação da biblioteca semântica Main/Linux

Os resultados reais de `tsk-a684` (`.build/rust-records-memory/semantic-source/results.json`, artifact histórico atualmente indisponível) registram **12 testes SDK PASS** (16,18 s), **library `memory_` 30 PASS**, 32 filtrados (5,24 s), e **44 integrações Memory afetadas PASS** (71,69 s): **86 casos distintos**, sem somar rodadas anteriores. Os **seis doctests Records PASS** (quatro ordinary/dois compile_fail) são separados, sem novo doctest Memory; compilação rustdoc 1,58 s, total 1,65 s. O scoped strict `--lib --test memory_store --test memory_embeddings --no-deps -- -D warnings` concluiu **exit 0** (1,83 s); fmt dos oito paths próprios e diff/whitespace concluíram **exit 0**. Tempos incluem as condições de execução e não são benchmarks. Os hashes Source (`.build/rust-records-memory/semantic-source/source-hashes.json`, artifact histórico atualmente indisponível) vinculam a prova aos arquivos qualificados.

Disponibilidade verificada por metadata readonly em 2026-10-12: results.json e
source-hashes.json acima não estão presentes. Contagens/PASS/source-fingerprint/
cap qualification anteriores permanecem registros históricos; os artifacts não
foram reconstruídos e nenhuma causa ou autoria é atribuída. A validação nova
23f2 conterá somente suas execuções próprias, sem substituir receipts antigos.

As fixtures executadas cobrem ausência/publicado vazio/EOF; cobertura parcial→completa; archive/current tie, maior revisão e os cinco filtros do head com data como metadata; vector_id e dimensão; update/delete e histórico explícito; Existing em branch herdada sem birth; batches concorrentes com CAS atual/fingerprint; original/admissão/projeto inválidos, revoke/cancel e fresh capture; limites exatos por scan e fases sem somar releituras; recuperação física, causas e corrupções tardias sem resultado parcial. As unidades de fronteira de produção preservam publicação/cause em KnownConfirmed, sem interceptar lost-ack SDK nem provar atomicidade contínua Auth/SDK.

Os reruns finais SDK12/library30 ocorreram após dois ajustes let-chain e a correção de fixture DropNonDrop. A integração afetada44 foi executada antes desses ajustes de lint nos novos helpers; ordinary CRUD/rows/read ficaram inalterados. O warning de três métodos unused numa fixture Task alheia foi preservado, fora do target strict unit. O histórico mantém a compilação inicial inválida de ProjectId; 9 PASS/1 falha de fixture com deadline de preparação de 200 ms; Lock/prefix exit101 ligado ao edge Root Hub; pausa com 81 MB livres e recuperação externa Root; diff alheio exit2; dois lints próprios e fixture DropNonDrop exit101. Esses logs permanecem no diretório de evidência. O antigo check T1 foi superseded na revisão12 para os cinco filtros/data metadata, sem apagar o histórico.

Os comandos focais executados são Records `cargo test -p graphit-records --test memory_embeddings`, integração afetada `--test memory_store`, library `--lib memory_`, doctests Records e Clippy no escopo acima. Root combinou warm target/j1 e execução serial, sem gate amplo. As qualificações históricas `tsk-4c15` e `tsk-dec0` mantêm seus próprios receipts; seus casos não são somados aos 86 desta folha.

O compositor `tsk-5c9b` precisa aplicar o contrato real de captura/attach/query e vector_id/Query/Document do ModelService, com backend prefix uma vez e sem reter authority/opaque capture através de requests. Seu correction package deve usar as APIs qualificadas a684 antes Source; caller ModelService/daemon e validação de produto integrado permanecem gates. A compilação Root/Now das folhas `tsk-9a88`/`tsk-4148` após ajuste estreito de erros não constitui prova de produto App por esta biblioteca Memory. USER, Dream, consolidação, Task semântico, produto N3, Windows/macOS e desempenho não são qualificados. Esta edição documental transcreve provas do produtor; não executa suítes nem altera lifecycle.

A política atual sem quotas agregadas de corpus tem [evidência separada em `tsk-265d`](native_memory_store.md#current-read-policy-qualification), com readers semânticos, corrupção tardia, colisão física e encoded-row guard. As qualificações antigas de capture/attach/RPC conservam seu escopo; compilação dos literais Daemon não é nova prova de Model.

### Query híbrida borrowed

`MemoryEmbeddingModel::query_hybrid(read, text:String, options, limits, embed)`
usa o mesmo Original, identidade portátil e callback do serviço e retorna o
`MemoryHybridSearchRead` do [produtor híbrido](native_memory_search.md#busca-híbrida-project).
Revalida ReadMemory/Original antes de preparar a expansão literal e capturar o
preflight. NoCorpus e is_empty verdadeiro não chamam o callback; fully covered com
next_documents=None continua não vazio. Descarta a captura antes de mover a
String crua, uma vez, ao RPC Query. O private query_vector compartilhado com
query semântico mantém vector_id, dimensão e exatamente um vetor do produtor.
O backend aplica o prefixo; o caller não duplica recipe ou texto.

Após RPC, search_hybrid captura o único pin final comum aos canais e heads.
Preflight e pin final são observações separadas: não existe snapshot atômico
através da inferência. Config/cobertura/estado alterados conservam a causa ou
Unavailable do produtor atual; não há replay, renovação ou fallback.
MemoryEmbeddingFailure<E> preserva Service(E)/Store e last_attachment=None
nessas leituras. Nenhum novo M, client factory, worker, issuer ou USER hybrid
é criado.

O caso histórico Linux actual_tiny_memory_hybrid_rpc usava CPU ORT1.27.1 e ONNX
embedding_rank2 mantido, RPC autenticado e índice SDK reais: NoRPC para ausente
ou vazio físico e Query RPC para fully-covered nonempty, com ambos metadados e
score `2/61`, passou em 6,399s. A fixture fornece observação funcional do caller,
sem alegação de qualidade de embeddings, Gemma, shared M ou MCP. O caso atual `actual_gguf_memory_hybrid_rpc_uses_borrowed_owners_and_queries_fully_covered_corpus` passou com o GGUF BF16 fixado em MRL128 na qualificação acima; a execução histórica ONNX não qualifica esse backend. O antigo caso ignored Gemma conserva sua evidência histórica separada.

### Consumer Source MCP

O [Source MCP PROJECT Memory search](native_memory_search.md#source-mcp-project-memory-search)
foi implementado por `tsk-8140` sobre `MemoryEmbeddingModel::query` e
`query_hybrid`. Usa o único M já anexado em `State.semantic`; não cria client,
ExpectedModel, provider, worker ou identidade substituta. ReadMemory e o lease
Embedding são permissões independentes do caller Runtime/local real, sob o mesmo
Original. FTS permanece uma leitura SDK sem recurso do modelo.

Semantic/hybrid conservam o preflight sem RPC para NoCorpus/vazio físico e uma
Query real para conteúdo não vazio com vetores compatíveis disponíveis, inclusive cobertura parcial. Pré-captura, inferência e pin final são
observações separadas. O endpoint transmite a geração e os metadados do produtor,
sem inventar atomicidade através da inferência. Cobertura parcial com scores válidos permanece consultável; nenhum score compatível
ou identidade incompatível produz Unavailable com a causa SDK, sem fallback FTS.
M desabilitado/ausente é uma indisponibilidade do consumer anterior ao SDK e não
prova corpus vazio. Os estados, defaults, seleção e recuperação estão no contrato
Source linkado acima.

O Source retém o job até join e mantém o lease vivo até após a serialização e o
guard de bytes de saída. A consulta não adquire um mutex global de computação. Revalida esse
lease, ReadMemory e Original antes e depois dessa fronteira; isso não promete
entrega atômica entre Auth e transporte. Nenhuma autoridade é renovada durante
inferência ou delivery. A fixture HTTP atual usa o GGUF BF16 fixado em MRL128,
ModelChild/RPC autenticado e NativeMemoryStore reais, em qualificação focal; o header
incompatível negativo usa vetores produzidos pelo RPC Document real e dimensão
derivada do ExpectedModel, sem vetores fake ou alegação de qualidade de modelo.

A [qualificação Source MCP](native_memory_search.md#qualificação-source-mcp) registra
os três casos distintos e suas tentativas; resultados do modelo foram
requalificados depois da correção do lifetime de lease no delivery.
