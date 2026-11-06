---
references:
  - target: {type: task, id: tsk-67e7}
    relation: implements
  - target: {type: task, id: tsk-2b12}
    relation: implements
  - target: {type: task, id: tsk-8140}
    relation: implements
  - target: {type: task, id: tsk-222c}
    relation: implements
  - target: {type: task, id: tsk-265d}
    relation: implements
  - target: {type: task, id: tsk-85c4}
    relation: supports
  - target: {type: task, id: tsk-8e7b}
    relation: supports
  - target: {type: task, id: tsk-c623}
    relation: supports
  - target: {type: task, id: tsk-63c5}
    relation: implements
  - target: {type: task, id: tsk-3a8e}
    relation: specified_by
  - target: {type: task, id: tsk-a666}
    relation: uses
  - target: {type: task, id: tsk-631e}
    relation: uses
  - target: {type: task, id: tsk-1228}
    relation: extends
  - target: {type: task, id: tsk-612f}
    relation: supports
  - target: {type: task, id: tsk-be9c}
    relation: extends
  - target: {type: task, id: tsk-da58}
    relation: relates_to
  - target: {type: task, id: tsk-4c15}
    relation: uses
  - target: {type: task, id: tsk-a684}
    relation: relates_to
  - target: {type: task, id: tsk-9d92}
    relation: supports
  - target: {type: task, id: tsk-db07}
    relation: supports
  - target: {type: task, id: tsk-7b36}
    relation: supports
---

# Índice lexical Memory PROJECT nativo

Este contrato atende integradores Records/Data/Auth/Core. `tsk-63c5` implementa a publicação explícita e a resolução do índice lexical sobre o corpus Memory PROJECT existente. O Source em [lexical_index.rs](../../crates/graphit-records/src/memory_store/lexical_index.rs) está **qualificado no Linux para publicação/resolução explícita do índice**, com 45 testes distintos atuais e strict próprio exit 0. O estado lifecycle/checks é consultado na Task; a evidência abaixo não antecipa COMPLETED. A leitura/busca `tsk-612f` está **qualificada no Linux** com 57 testes Memory distintos e seis doctests Records afetados, além de strict scoped exit 0. Esta página não declara qualificação de busca Memory por herança dos produtores Data.

O [armazenamento Memory](native_memory_store.md) já possui seus recibos separados de criação, catálogo e lifecycle. O [contrato Memory](rust_memory_records.md#canonical-selection-and-complete-bounded-recall) preserva current/archive, filtros, collapse e top-k para a busca lexical qualificada. O [publisher e inspector JSON FTS Data](../development/native_json_fts_index.md), `tsk-a666` e o inspector `tsk-631e` (COMPLETED r15), fornece os seams SDK qualificados; não certifica seleção Memory ou cobertura após merge/delete.

O Source `tsk-4c15` agora usa schema24/`project-v2-embedding-list` e dispatch validado de embeddings; schema/projeção/codec estão qualificados no Main/Linux com 72 casos Memory distintos (integração44 + library28), seis doctests Records separados e scoped strict/fmt0. Os receipts lexical/índice abaixo preservam seus escopos históricos e não somam suas contagens aos 72 atuais nem qualificam busca semântica. [Schema e projeção](native_memory_embeddings.md#schema-único-e-família-de-embeddings) têm Source implementado; captura/attach/query/resolve semânticos `tsk-a684` têm qualificação própria Main/Linux: SDK12 + library30 + integração afetada44 = 86 casos distintos, seis doctests Records separados e scoped strict/fmt0. Essa prova não qualifica o compositor daemon/ModelService/GGUF.

<a id="publicação-explícita-do-índice-tsk-63c5"></a>

## Inicialização e manutenção do índice

Os escritores PROJECT e USER inicializam o índice derivado na primeira escrita válida, incluindo criação Fenced legítima. A inicialização usa o mesmo writer, admissão, Original e guard da escrita; não transforma Fenced em ordinary WriteMemory. Validação semântica, CAS, equivalência idempotente, no-op e codificação do batch precedem esse efeito adicional. Tentativas idempotentes já observadas retornam o resultado anterior sem nova publicação.

`Ready` reutiliza o índice. `Missing` valida o corpus e publica uma vez antes do merge CRUD, usando como base o pin realmente retornado pelo commit do índice. `Incompatible` falha causalmente. Update/delete/fold posteriores usam os dados não indexados/alterados suportados pelo SDK; não há rebuild por escrita. Read/search/GET continuam readonly, sem reparo, publicação ou fallback textual.

A manutenção explícita pertence a `graphit_records::memory_store`:

```rust
impl NativeMemoryStore {
    pub async fn maintain_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        operation: MemoryLexicalIndexOperation,
        limits: MemoryStoreWriteLimits,
    ) -> Result<MemoryLexicalIndexOutcome, MemoryStoreError>;

    pub async fn resolve_lexical_index(
        original: &Arc<SnapshotContext>,
        admission: &ProjectWriteAdmission<'_>,
        key: &DeltaRecoveryKey,
        limits: MemoryStoreWriteLimits,
    ) -> Result<DeltaResolution, MemoryStoreError>;
}
```

`MemoryLexicalIndexOperation::Ensure` reutiliza `Ready` ou cria o índice ausente. `Refresh` exige índice compatível existente e substitui seus segmentos físicos. Ausência ou incompatibilidade não autorizam reparo implícito por Refresh. A operação pública exige **ordinary WriteMemory PROJECT** real e o mesmo Arc Original; READ, WRITE Task e Fenced não a autorizam. A inicialização privada derivada segue a política real do writer legítimo.

`MemoryLexicalIndexOutcome` distingue `NoCorpus`, `Ready(MemoryGeneration)` e `Published(MemoryLexicalIndexPublication)`. Ready contém a geração observada, sem commit nem chave de transação fabricada. Published contém `generation` e `recovery_key` reais; geração/chave não são grants. `resolve_lexical_index` observa a tentativa sob autoridade atual.

### Corpus existente e validação

A manutenção explícita adquire o writer com `create=false`. Ausência autoritativa retorna NoCorpus após recheck, sem nascimento Memory. Diretório existente sem index.lance é IncompleteCorpus. Corpus válido vazio pode receber índice e retorna Published.

Antes de construir ou substituir um índice, PROJECT executa `read::exact_rows(pin, "true", ...)` até EOF com schema24/codec atual e unicidade global de keys; move as rows para grupos por Memory e aplica `history::assemble` a todos. USER valida suas chains e owner completos. Não há scan por identidade, nova cópia raw integral, quotas agregadas ou retenção de corpus. Ensure+Ready apenas observa a política física e a geração; não certifica novamente a integridade do histórico Memory.

Após revalidar Original/Auth/guard, PROJECT materializa o target herdado somente sob escrita autorizada. Materialização pode permanecer se uma etapa seguinte falhar. Os limites `MemoryStoreWriteLimits` e `limits.read` continuam nos seus contratos existentes.

### Política física e efeitos conhecidos

A coluna é payload e o nome é memory-json-content-v1. A política Data é Row, JSON/simple, lowercase=true, stemming/stopwords/ascii folding=false e max token length None. O índice físico abrange JSON validado; a busca Memory permite somente title/body/tags/class, excluindo audit/reason/keys.

`prepare_json_fts_index` recebe `JsonFtsIndexMode::Create` na inicialização e Ensure+Missing; Refresh usa Replace. Replace remove os segmentos reais do nome/campo/kind alvo no pin capturado, preservando outros índices e pins antigos. A publicação conserva a chave real antes de commit, base+1, zero retries e o handler estrito Data. Não recaptura latest, adota vencedor ou repete uma transação. Arquivos uncommitted podem permanecer após falha de preparação.

O índice altera a geração SDK, sem reescrever rows, revisões, referências, markers, auditoria ou atores. Se publicou e o CRUD seguinte falhou, IndexConfirmed conserva a publicação do índice e a causa CRUD, incluindo sua DeltaRecoveryKey quando houver commit indeterminado. Se o CRUD também confirmou mas falhou no pós-check, Confirmed permanece no topo com sua versão CRUD; a publicação do índice fica na cadeia causal. Índice confirmado nunca significa Memory criada ou mutada. USER conserva o equivalente em UserIndexConfirmed e UserConfirmed, com owner/geração/chaves reais.

| Outcome/erro | Significado |
| --- | --- |
| NoCorpus | Ausência autoritativa; manutenção não criou corpus. |
| Ready | Geração observada, sem publicação adicional. |
| Published | Geração/chave confirmadas e recheck final aprovado. |
| IncompleteCorpus/schema/codec/limite | Validação falhou; não publicar prefixo ou tratar como empty. |
| Commit(DeltaCommitFailure) | Chave real e causa SDK; conflito/unknown não autorizam replay. |
| IndexConfirmed | Índice confirmado e causa posterior conservados; não é recibo CRUD. |
| Confirmed com IndexConfirmed na causa | CRUD e índice confirmados, ambos preservados separadamente. |

As causas Auth/request/Core/SDK seguem `MemoryStoreError::source`. O writer permanece retido durante a composição. Nenhuma falha significa rollback, ausência de efeito conhecido, atomicidade Auth/SDK ou lost ack universal. As qualificações históricas de publicação e leitura desta página conservam seus escopos; a API de manutenção e seus consumidores exigem suas próprias provas afetadas.

## Resolver sob WRITE atual

`resolve_lexical_index` exige ordinary WRITE e original válidos novamente. Antes de corpus I/O, verifica URI da chave e branch física contra o scope e a branch solicitada pela admissão atual. Chave estrangeira retorna `RecoveryBinding`; a chave não reconstrói a autoridade histórica.

A aquisição continua `create=false`. NoCorpus ou target nomeado correto ainda herdado/ausente retorna `Unresolved`, sem materializar. Para target existente, o pin resolve a transação por UUID/base/URI/branch, devolvendo Applied, Conflicted ou Unresolved; erros SDK preservam a causa. Rechecks cercam a observação e sua saída.

Resolve não nasce corpus, prepara índice, publica, repete transação, limpa, repara ou adota vencedor. `Unresolved` não é autorização para replay; erro indeterminado conserva a incerteza. Uma nova admissão atual permite observar a tentativa sob seus próprios vínculos, sem reassumir grants antigos.

## Query lexical: `tsk-612f`

A API implementada em [lexical_search.rs](../../crates/graphit-records/src/memory_store/lexical_search.rs) é readonly e está **qualificada no Linux**. As oito fixtures em [lexical_search.rs de integração](../../crates/graphit-records/tests/memory_store/lexical_search.rs) passaram no quarto focal, incluindo índice seguido de update/merge/delete e seleção current/archive/tombstone. A qualificação do publisher acima e do inspector Data não é evidência de busca Memory.

```rust
impl NativeMemoryStore {
    pub async fn search_lexical(
        original: &Arc<SnapshotContext>,
        admission: &ProjectReadAdmission<'_>,
        query: MemoryLexicalQuery,
        limits: MemoryStoreReadLimits,
    ) -> Result<MemoryLexicalSearchRead, MemoryStoreError>;
}
```

O [model](../../crates/graphit-records/src/memory_store/model.rs) expõe `MemoryLexicalQuery { text: String, filter: MemoryListFilter, include_deleted_history: bool, top_k: NonZeroUsize }`. Os filtros são AND e usam classificação, tag literal, mandatory, important e exclude_mandatory do **head atual**, inclusive quando o conteúdo correspondente é arquivado. Tag fornecida em branco é erro após Auth e antes de corpus I/O.

### Entrada literal e política física

Cada segmento não vazio de `text.split(';')` é expandido para estes doze paths fixos:

| Grupo | Paths permitidos |
| --- | --- |
| Head | `title`, `body`, `tags`, `attributes.classification` |
| Creation | `memory.title`, `memory.body`, `memory.tags`, `memory.attributes.classification` |
| Revision | `after.title`, `after.body`, `after.tags`, `after.attributes.classification` |

Segmentos compostos só de whitespace são ignorados; os demais preservam o literal sem trim. `;` delimita segmentos, enquanto vírgula e newline permanecem no valor. O caller não fornece paths, operadores ou uma DSL Boolean própria. Reason, actor, referências, audit, input e chaves não são campos de busca.

A expansão produz triplets `path,str,literal` e verifica comprimento raw e expandido com aritmética checked contra `MemoryStoreReadLimits.max_query_bytes` antes da alocação/I/O. Query em branco, nenhum segmento válido, overflow ou excesso são erros. A política física exigida é JSON/simple, lowercase=true, stemming/stopwords/ascii folding=false, max token length None e posting Row. A query SDK usa `Operator::Or`, fuzziness `Some(0)` e `limit(None)`. `MatchQuery::max_expansions` mantém o default SDK de 50; não é uma quota criada por Records. Não há hardcoded token length 40 ou caminho de busca flat quando o índice está ausente.

### Pin, EOF e seleção Memory

Ordinary PROJECT ReadMemory, o Arc original da admissão e o guard Core são verificados antes de I/O. As rechecagens nas fronteiras públicas SDK, na criação/avanço do stream, nos batches, nas rows e na saída mantêm o mesmo request/Auth/guard; não há renovação de contexto nem promessa de fence contínuo durante I/O SDK.

A operação captura um pin, verifica o schema físico e distingue corpus vazio por uma contagem nativa de rows lógicas, sem decodificar payloads. O índice FTS seleciona primeiro os IDs candidatos. Somente as chains desses IDs são lidas e validadas integralmente, incluindo criação, revisões, head, referências e marcadores associados, no mesmo pin. A validação antecede os filtros do head, de modo que um candidato inválido não desaparece por filtragem. Chains sem correspondência não são carregadas: a busca não é uma auditoria integral do corpus. Essa exigência antiga foi substituída; a manutenção explícita continua validando o corpus quando constrói o índice.

Os IDs candidatos alimentam um único scan nativo com predicado IN e ordenação por record_id. As chains são validadas e descartadas por grupo, preservando somente os corpos dos melhores top-k resultados e a contagem total. Não há scan por candidato, releitura de top-k, cache de corpus, recaptura de latest nem retenção de todos os históricos durante a busca.

No mesmo pin, `json_fts_index_state("payload")` deve retornar Ready. Missing ou Incompatible produz `LexicalIndexUnavailable`; nenhum deles pode virar NoMatch por fallback flat. O stream FTS projeta `row_key`, `record_id` e `_score`, sem limit/offset, e é consumido até EOF. A validação exige schema/projeção não nulos, score Float32 finito, unicidade das keys, identidade/revisão vinculadas às chains sem quotas cumulativas de candidatos/bytes, inclusive candidatos posteriormente excluídos. Causa tardia invalida a consulta inteira; prefixo não é resultado saudável. `batch_rows` é um hint SDK, não uma garantia de fragmentação FTS: uma execução emitiu os 120 candidatos em um único batch apesar de `batch_size(1)`. A implementação aceita todos os batches até EOF, qualquer que seja seu tamanho. A fixture separada de leitura do corpus tipado exercita 180 rows em múltiplos batches; seu escopo não implica fragmentação do stream FTS.

Normalmente só chains vivas participam. Current correspondente vence qualquer archive correspondente; sem current correspondente, vence a maior **revisão numérica** correspondente. `include_deleted_history=true` acrescenta somente matches arquivados de chains tombstonadas, marcados `deleted=true`; não devolve tombstone como Current nem ressuscita registro. Filtros, categoria e data continuam sendo do head atual.

Após collapse por ID, ordena mandatory antes de important antes de normal, depois head update DESC e ID ASC. `total_hits` conta identidades elegíveis únicas **antes** de truncar por `top_k`. Score é metadata do match escolhido, sem determinar revisão ou ordem canônica e sem abs/clamp. `MemoryLexicalHit` conserva `current`, `matched`, `location: Current | Archived { revision: NonZeroU64 }`, `deleted` e `score`; o conteúdo e a provenance da revisão correspondente ficam distintos do head que governa filtros e ordem.

| Resultado | Significado |
| --- | --- |
| `NoCorpus` | Ausência autoritativa, com recheck final; não nasce corpus ou índice. |
| `PublishedEmpty { generation }` | Pin publicado com zero rows lógicas, conforme contagem nativa e schema validado. Chains tombstonadas não estabelecem empty. |
| `Published { generation, total_hits, hits }` | Consulta completa sobre índice Ready; zero hits é NoMatch válido, distinto de ausência/empty físico. |
| `LexicalIndexUnavailable(Missing | Incompatible)` | Índice físico indisponível/incompatível; falha explícita, sem fallback ou refresh. |
| Auth/Core/SDK/schema/codec/limite/cancel/deadline | Erro causal integral, sem resultado parcial ou reclassificação como ausência/NoMatch. |

Leitura não adquire Writer, materializa branch, nasce índice, publica, repara ou faz refresh. Geração é observação do pin, não autoridade. Ready não certifica cobertura Memory após merge/delete por si só: essa prova pertence às fixtures desta folha.

### Qualificação da query no Linux

As tentativas anteriores permanecem no histórico: a primeira falhou por ENOSPC antes dos testes; a segunda falhou na compilação de fixture que chamava `prepare_append` inexistente. O terceiro focal executou oito casos: **6 PASS e duas falhas de fixture**, em 40,01 s; não é a qualificação final. Uma fixture repetiu body sem alteração e recebeu corretamente `NoChange`; a correção varia o body nas revisões 3 a 9. A outra exigia batches FTS unitários, mas o SDK entregou 120 candidatos em um batch; a expectativa foi corrigida para respeitar o hint. Os logs de falha permanecem no histórico.

O quarto focal passou os oito casos corrigidos. A integração afetada passou **39 casos** (inclui os oito focais); a library afetada passou **18 casos**, incluindo quatro novas unidades de Candidates, expansão literal e comparador category/date DESC/ID ASC. São **57 testes Memory distintos**, não 65 ao somar o focal novamente. Os doctests Records afetados passaram **seis casos: quatro ordinary e dois compile_fail**; não são seis doctests Memory executáveis. O strict scoped `--lib --test memory_store --no-deps -- -D warnings` concluiu exit 0. O tracker reportou `NOFAULT_STATIC_FINAL`, com as lacunas de oráculo de ordenação e token Unicode acima de 40 bytes fechadas. O rustfmt check dos seis arquivos Source/test owned concluiu exit 0 e o diff check dos nove arquivos owned concluiu exit 0, conforme o produtor; links e whitespace documentais foram conferidos separadamente. A composição privada de dois batches verifica erro tardio/cancelamento do acumulador; não intercepta o SDK e não prova que ele fragmenta FTS. As fixtures e unidades executadas cobrem current/archive/deleted, full EOF/collapse/order/top-k, limites, provenance, Auth e o caso de índice seguido de merge/delete, nos escopos descritos. A fase privada não demonstra interceptação SDK nem linearização atômica Auth/SDK. Busca lexical não depende de AI. Não se infere busca semântica, USER, Dream ou Windows/macOS.

Comandos focais da qualificação na janela serial Root, sem gate amplo:

```sh
cargo test --offline --locked -p graphit-records --test memory_store lexical_search \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1
cargo test --offline --locked -p graphit-records --test memory_store \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1
cargo test --offline --locked -p graphit-records --lib memory_store \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1
cargo test --offline --locked -p graphit-records --doc \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1
cargo clippy --offline --locked -p graphit-records --lib --test memory_store \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1 --no-deps -- -D warnings
rustfmt --edition 2024 --check \
  crates/graphit-records/src/memory_store/lexical_search.rs \
  crates/graphit-records/tests/memory_store/lexical_search.rs
```

Os logs e result JSON ficam em `.build/rust-records-memory/search-contract/memory-search/`; o receipt final `validation.json` será consolidado pelo produtor após os hashes documentais. A qualificação de testes/strict descrita aqui é real; nenhum hash de receipt ou estado COMPLETED é antecipado. Os recibos CRUD/list/history e índice explícito mantêm seus próprios escopos.

## Provas do índice explícito e limites da entrega

A qualificação histórica do publisher create-only: os [fixtures Memory index](../../crates/graphit-records/tests/memory_store/lexical_index.rs) foram executados no Linux naquele escopo. Exercitaram ausência/incomplete/empty, criação real base+1, colisão causal, Main/named/inherited, rows/history/refmarkers preservados, negações de autoridade/original, caps/chain inválida, conflito SDK, outcome confirmado com falha tardia e resolução bound sem writes. Testes de fases usam cancelamento real e conflito SDK, mas não injetam lost ack ou atomicidade Auth/SDK end-to-end.

| Execução | Resultado real | Runtime |
| --- | --- | --- |
| Segundo focal `lexical_index` | 7 PASS/0 falhas, 24 filtrados | 5,97 s |
| Integração `memory_store` completa afetada | 31 PASS/0 falhas/0 filtrados; inclui os sete novos e 24 anteriores | 18,19 s |
| Library `memory_store` afetada | 14 PASS/0 falhas, 38 filtrados; 12 anteriores e dois novos testes de fases | 5,61 s |

São **45 testes distintos**, 31 de integração + 14 de library; o focal de sete está incluído nos 31, não soma novamente para 52. A compilação focal levou 12 min 13 s incluindo fila, sem significado de benchmark. O strict próprio Records `--lib --test memory_store --no-deps -- -D warnings` concluiu **exit 0**, em 25,39 s.

O comando de docs filtrado por `memory_store` concluiu exit 0 com **zero casos executados e seis filtrados**. Nenhum doctest Memory executável foi selecionado: não é Doc6 PASS. APIs Markdown, Source e links foram inspecionados; não se declara replay mais amplo de docs.

O primeiro focal teve 4 PASS/1 FAIL por `AlreadyExists` num diretório de fixture duplicado; o log foi preservado e o teste passou após usar fixture nova. Root também preservou o diagnóstico `needless_borrow` de `history::assemble`: a chamada foi corrigida de `&before` para `before`, sem mudar o contrato. Runtime/App inicialmente precisou tratar o novo `IndexConfirmed` em seu match exaustivo. O owner Root aplicou o mesmo mapeamento Unavailable de Confirmed, conservando origem Records, chave, causa e versão, sem novo outcome Now ou promessa NoEffect; Root reportou quatro casos de fechamento próprios PASS. Essa evidência externa não integra os 45 casos e não foi reexecutada por esta folha.

Comandos focais após composição Root e janela serial, desde o root do projeto:

```sh
cargo test --offline --locked -p graphit-records --test memory_store lexical_index \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1
cargo test --offline --locked -p graphit-records --lib memory_store \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1
cargo clippy --offline --locked -p graphit-records --lib --test memory_store \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1 --no-deps -- -D warnings
rustfmt --edition 2024 --check \
  crates/graphit-records/src/memory_store/lexical_index.rs \
  crates/graphit-records/tests/memory_store/lexical_index.rs
```

As suites CRUD/list/history afetadas pelo helper foram executadas nos escopos acima. Não houve gate amplo ou replay Data/SDK/NN, Runtime/App ou docs inalterados. O receipt da qualificação fica em `.build/rust-records-memory/search-contract/memory-index/validation.json`; o produtor consolida resultados, falhas, hashes e limites após esta atualização documental. Owned fmt/diff/links estão no fechamento documental. A evidência de testes/strict é distinta da inspeção Source↔doc.

O lexical reader `tsk-612f` usa o produtor de índice concluído `tsk-63c5` (COMPLETED r14) e o inspector qualificado `tsk-631e` (COMPLETED r15); sua busca PROJECT é Linux-qualified no escopo de 57 testes Memory distintos, seis doctests Records e strict scoped. Leitura não nasce índice. Essa qualificação não entrega embeddings Memory, USER, Dream, consolidação, N3 ou Windows/macOS.

O [contrato de embeddings Memory](native_memory_embeddings.md), `tsk-da58`, registra schema24/List e seleção por distância semântica. Schema/projeção/codec `tsk-4c15` estão qualificados Main/Linux no escopo próprio de 72 casos Memory, doc6 separados e strict/fmt0; captura/attach/query/resolve `tsk-a684` estão entregues e qualificados como biblioteca Main/Linux, com 86 casos Memory distintos e seis doctests Records separados, scoped strict/fmt0; compositor `tsk-5c9b` segue gated; current/archive e categoria/data/ID desta query lexical permanecem inalterados.

A query semântica qualificada de `tsk-a684` usa os cinco predicados reais `MemoryListFilter` e deleted do head antes do cosine; `updated_at` é metadata, sem filtro temporal. Collapse escolhe a menor distância por ID, empate current/maior revisão, e ordena distância/ID após EOF antes de top-k. Cada scan SDK e lookup final consome EOF com schema/keys/Auth/cancelamento, sem limites agregados rows/text do corpus. Guardas de query, entrada, linha escrita e arquivos nativos permanecem concretas. Isso não altera a ordem lexical categoria/data/ID nem sua evidência histórica. A prova semântica executada está nos resultados próprios (`../../.build/rust-records-memory/semantic-source/results.json`; historical local artifact unavailable); não é inferida de check lib, lexical ou SDK/Data de outra folha.

<a id="user-lexical-source-implementado-validação-pendente"></a>

## USER lexical Source: APIs ordinárias qualificadas no Linux

A pipeline USER de `tsk-c623` implementa no Linux NativeUserMemoryStore::search_lexical
e resolve_lexical_index. A manutenção atual usa maintain_lexical_index com UserReadAdmission/ordinary
UserWriteAdmission reais e o mesmo Original/capture USER. O [contrato USER](native_user_memory_store.md#user-lexical-index-and-query)
detalha USER25/owner, geração SDK default sem Git/PROJECT, EOF/caps/markers e
UserIndexConfirmed com generation/key/causa. A Source usa payload e política
memory-json-content-v1, inspector físico e seleção lexical compartilhada;
Missing/Incompatible é erro, não fallback flat-scan ou healthy NoHits.

Ausência válida é NoCorpus; search físico vazio tem PublishedEmpty. List vazio
usa Published{memories:[]} e metadata vazio Published{heads:[]}, com a geração
real. Leitura não nasce corpus/índice, materializa, repara ou publica.
A [qualificação própria c623](native_user_memory_store.md#c623-linux-qualification-and-evidence-boundaries) registra **115 funções distintas PASS**:
USER SDK15 +USER lib5 +PROJECT SDK68 +PROJECT lib27, com scoped strict0 e
fmt23/diff0. Os dois testes USER de pós-commit usam SDK real, cancelamento e
nova admissão para reobserve/resolve sem segundo commit; não interceptam lost-ack.
Seis doctests Records passaram separadamente (quatro ordinary/dois compile_fail),
sem somar aos 115 funcionais ou atribuí-los a Memory executable. Os resultados históricos 9613
e os reruns sobrepostos não são somados; falhas anteriores permanecem auditadas
no receipt `.build/rust-records-references/user-memory-ordinary/validation.json`.
Fenced/automatic, semântica, query de referências, daemon/App, FullNoLoss e
non-Linux estão fora dessa qualificação ordinária.

<a id="user-semantic-source-compilado-validação-sdk-pendente"></a>

## USER semantic Source: SDK nativo qualificado no Linux

`tsk-85c4` implementa capture_embeddings, attach_embeddings, resolve_embeddings
e search_semantic com admissões USER concretas, mesmo Original/owner/root,
USER25/default SDK pin e guards reais. O [contrato USER semântico](native_user_memory_store.md#user-semantic-capture-and-publication-implemented-sdk-validation-pending)
descreve corpus opaco/getters emprestados, next_documents com admissão por chamada,
encoder puro, allowlist dos documentos emitidos, atual writer/CAS e
UserEmbeddingConfirmed com generation/key/causa. Não usa ProjectGeneration.

A query retorna NoCorpus, PublishedEmpty{generation}, Unavailable{generation,reason}
ou Published{generation,total_hits,hits}; coverage ausente/incompatível não vira
fallback lexical. Current-head filters e include_deleted_history precedem cosine
Data real sobre List<Float32>; seleciona a menor distância por Memory ID entre
revisões elegíveis, empate current/maior revisão, e ordena distância/ID após EOF
antes do top-k. O resultado mantém current head +matched_revision/location/distance,
sem novo campo deleted ou ressurreição. Reference markers são família explícita,
excluída de documentos/candidatos/attachments/hits. Os pins capture/query são
observações próprias, sem snapshot global; a ordem lexical anterior permanece.

A qualificação Linux tem **60 funções distintas PASS**: USER45 (semantic9,
causal store35 e Confirmed lib1) +PROJECT15 (semantic12 e codec/publication lib3).
O filtro lib4 inclui USER1 já contado; os focused PROJECT chunk/cap/inherited e
reruns de ranking são subconjuntos. Seis doctests Records passaram separadamente,
sem contar como Memory executable ou somar aos 60. Fixture compile07 e
strict-final07 passaram exit0; Product Source04 permaneceu inalterado em 05/06/07,
com alterações apenas de fixtures. Os testes causais exercitaram CRUD/history/
lexical/references/automatic após embeddings reais.

O log user-rank-one-05 com binário stale antes da compilação atual não qualifica
ranking; o rerun corretamente compilado passou e integra semantic9. Todas as
falhas anteriores permanecem nos logs; nenhum replay amplia a contagem distinta.
Tempos wall/runtime permanecem na proveniência JSON/logs, sem inferência de
benchmark. Os recibos PROJECT/USER anteriores preservam seus próprios escopos.
Daemon `tsk-8e7b` pertence ao Root para ModelService/leases/ExpectedModel: nenhum
RPC, GGUF/model authority, App/global, Runtime Dream USER, non-Linux ou desempenho
é qualificado pela API nativa SDK entregue aqui.

A remoção das quotas agregadas de corpus de `tsk-265d` tem [qualificação própria dos leitores](native_memory_store.md#current-read-policy-qualification), incluindo FTS, filtros, EOF e guardas reais de query. Os recibos anteriores conservam sua política e contagens históricas; essa entrega não introduz busca híbrida nem qualifica Model.

## Busca híbrida PROJECT

`tsk-222c` implementa o produtor Rust híbrido. `MemoryHybridQuery::new(text,
options, limits)` prepara uma única String FTS com os paths literais existentes e
`max_query_bytes`; conserva options privadas, sem pin, autoridade ou modelo.
`MemoryHybridSearchOptions` contém `filter`, `include_deleted_history` e `top_k`.

```rust,ignore
NativeMemoryStore::search_hybrid(
    original: &Arc<SnapshotContext>, read: &ProjectReadAdmission<'_>,
    identity: MemoryEmbeddingIdentity, query: MemoryHybridQuery,
    query_vector: &[f32], limits: MemoryStoreReadLimits,
) -> Result<MemoryHybridSearchRead, MemoryStoreError>
```

A operação captura um único pin final e sua geração. Valida todo o corpus até
EOF e monta as chains uma vez; descoberta de IDs físicos e os scans por chain
reutilizam esse pin e rejeitam orphans/corrupção. FTS e cosine usam os cinco
predicados AND do head e a opção de histórico deletado. O canal lexical mantém
current-match antes de archive/maior revisão, depois categoria/data/ID; o canal
semântico mantém menor distância, current em empate/maior revisão, depois
distância/ID. Cada canal contribui uma vez por Memory ID lógico.

A fusão RRF usa peso1, k60 e rank1-based: `sum(1 / (60 + rank))`. Consome os
rankings completos antes de ordenar score DESC/ID ASC e aplicar top_k.
`total_hits` conta a união distinta antes da seleção. A fusão conserva apenas
IDs, ranks e metadados; move somente os heads finais selecionados. O wrapper
lexical também resolve/copia os bodies somente depois de seu próprio top_k,
sem mudar seu contrato ou sua ordem canônica. Com coverage completa, o canal
semântico inclui todos os IDs elegíveis; fixtures SDK exercitam overlap,
semantic-only e lexical vazio. Inputs disjoint pertencem à prova pura de RRF,
sem threshold ou estado SDK inventado.

Cada `MemoryHybridHit` contém um `current: Memory`, score RRF f64, e metadados
opcionais por canal: lexical tem matched_revision/location/raw score; semantic
tem matched_revision/location/distance. As revisões correspondentes podem ser
diferentes e continuam separadas do head comum do mesmo pin.

`MemoryHybridSearchRead` distingue NoCorpus, PublishedEmpty{generation},
Unavailable{generation, reason:MemoryEmbeddingUnavailable} e
Published{generation,total_hits,hits}. Vazio físico validado é PublishedEmpty;
zero hits em corpus não vazio continua Published. A consulta usa os vetores compatíveis disponíveis enquanto o incremental
avança. MissingCoverage/IncompatibleIdentity só tornam a consulta Unavailable
quando nenhum vetor elegível compatível existe; não há fallback FTS silencioso. Índice FTS ausente/incompatível conserva LexicalIndexUnavailable.
Shape/nonfinite, schema/codec, Auth, cancel/deadline e SDK conservam suas causas,
sem resultado parcial. Leitura não cria índice, Writer ou novo dataset. O caminho PROJECT também
conserva o pin capturado antes da inferência e agrupa a varredura por Memory ID,
sem repetir scans de histórico e vetores para cada registro.

A implementação passou check dos produtores Records/Daemon; 17 casos focais
Records/lexical e um caller tiny ONNX/RPC real passaram no Linux. A regressão
lexical USER existente de archive/filtros atuais também passou no executável
recém-compilado, sem acrescentar USER hybrid: são 19 casos distintos nesta folha.
Scoped strict Records e Daemon também passaram; fmt/doc QA e o recibo final
conservam as tentativas e os limites dessa evidência. Não se infere
rota Memory MCP híbrida ou integração de SourceServer/Foreground/shared M:
esses consumidores continuam nas folhas próprias do Root.

## Busca híbrida USER

`NativeUserMemoryStore::search_hybrid` recebe o mesmo Original, uma
`UserReadAdmission` real, `MemoryEmbeddingIdentity`, a `MemoryHybridQuery`
preparada, o vetor Query e `MemoryStoreReadLimits`. Retorna
`UserMemoryHybridSearchRead`: `NoCorpus`, `PublishedEmpty { generation }`,
`Unavailable { generation, reason }` ou
`Published { generation, total_hits, hits }`. A geração é exclusivamente
`UserMemoryGeneration`, derivada do owner USER literal e do default pin SDK;
não recebe seleção PROJECT ou versão Git. Corpus ausente não nasce em leitura.
Corpus vazio físico validado difere de um corpus não vazio com zero hits.

O [produtor USER](../../crates/graphit-records/src/user_memory_store/hybrid.rs)
retém um único pin, guard, owner e Original, inclusive durante a inferência Query.
O scanner SDK ordenado por `record_id` monta e valida uma cadeia por vez;
lexical `HistoryView::User` e semantic `SourceOwner::User` usam esse mesmo
pin. Entre grupos permanecem ranks e candidatos lexicais leves, não os corpos
do corpus. Apenas os corpos top_k são lidos novamente no pin capturado. Ambos os rankings terminam antes da fusão RRF compartilhada acima, com
k60, peso1 e ranks iniciando em1. `total_hits` conta a união antes de top_k;
empates de score usam Memory ID literal. Cada hit conserva um head comum e
metadados independentes de revisão/localização/raw score ou distância por canal.
Filtros são AND sobre esse head, inclusive quando o match pertence ao histórico;
deleted é excluído salvo `include_deleted_history` explícito.

Cobertura pendente não descarta os rankings já disponíveis. Sem qualquer
vetor elegível compatível, a consulta retorna `Unavailable`, sem fallback lexical. Índice FTS ausente
conserva `LexicalIndexUnavailable`; codecs, owner, admissão retirada, Original,
vetor inválido, cancelamento, prazo e guard físico falham com a causa nativa.
Não faz duas buscas públicas com pins independentes, não toca corpus PROJECT
nem introduz schema, índice, quotas de corpus ou retries.

O [consumer de modelo USER](native_user_memory_embeddings.md#query-híbrida-user) usa os focais SDK de estados, ranking completo antes de top_k, geração e cancelamento.

## Source MCP PROJECT Memory search

`tsk-8140` entrega `graphit_memory_search` no SourceServer HTTP `/mcp`,
consumindo os produtores acima. O endpoint Role conserva sua busca FTS própria.
O Source tool recebe os argumentos MCP abaixo; omitir `mode` seleciona `fts`.

```json
{
  "text": "publicação de memória",
  "top_k": 5,
  "mode": "hybrid",
  "filter": {
    "classification": "decision",
    "tag": "records",
    "mandatory": false,
    "important": true,
    "exclude_mandatory": true
  },
  "include_deleted_history": false
}
```

`text` e `top_k` são obrigatórios. Os modos são `fts`, `semantic` e `hybrid`.
`filter` omitido equivale a `{}`; os cinco predicados são AND sobre o head atual,
mesmo quando o match vem de archive. `classification` aceita fact, skill, tension,
decision, convention e correction. `exclude_mandatory` e
`include_deleted_history` têm default false. Os seletores opcionais `project_dir`
(NativePath do inventário Source) e `project_version` escolhem somente um tuple
capturado existente; omiti-los usa o projeto/versão atual. Seleção não concede
permissão nem abre USER ou contextos importados.

Argumentos/filter desconhecidos, null explícito, tipos ou enums inválidos,
`top_k` zero/não inteiro/acima de `max_collection_entries`, texto/tag/versão
vazio, com NUL ou acima de `max_scalar_bytes` falham antes do I/O de corpus/modelo.
A expansão literal mantém o guard de query do produtor; seu `max_query_bytes`
usa `max_output_bytes` positivo da policy Source. `batch_rows=8192` é tuning,
sem quota agregada de corpus. O resultado serializado também respeita
`max_output_bytes`.

A chamada usa `Authorization: Bearer <credential>` e
`x-graphit-auth-origin: runtime` ou `local` com o caller real. Requer ReadMemory
PROJECT; semantic/hybrid requerem separadamente o lease local Embedding do
mesmo caller e o M existente. O transporte autentica uma vez e conserva o mesmo
Original/admissão no SDK e no caller borrowed. FTS funciona sem M ou lease
Embedding. Ver [lifecycle do consumer](rust_model_service.md#source-memory-consumer).

Todos os resultados de busca incluem `mode`:

| `kind` | Significado |
|---|---|
| `no_corpus` | Não há corpus observado; sem geração inventada. |
| `published_empty` | Corpus físico vazio validado, com `generation`. |
| `published` | `generation`, `total_hits` antes de top_k e `hits`; hits zero por filtro não é vazio físico. |
| `unavailable` | Coverage/identidade incompatível contém geração SDK e reason `missing_coverage`/`incompatible_identity`; módulo desabilitado ou M ausente contém reason `model_disabled`/`model_unavailable`, sem geração inventada. |

`generation` conserva `sdk_version`, `project_id`, `project_version`,
`physical_branch` e `inherited_target` do produtor. FTS entrega `current`,
`matched`, `location`, `deleted` e raw `score`; semantic entrega `current`,
`matched_revision`, `location` e `distance`. Hybrid entrega um `current` comum,
`score` RRF f64 e canais nullable: `lexical` tem matched_revision/location/raw
score f32; `semantic` tem matched_revision/location/distance f32. Location é
`{kind:current}` ou `{kind:archived,revision:N}`. A ordem/fusão são dos produtores Records sobre o pin SDK final;
App não refaz RRF nem combina duas buscas públicas.

Semantic/hybrid publicam resultados com cobertura semântica parcial válida.
Revisões pendentes e vetores de outra identidade são excluídos do ranking
semântico; somente a ausência de qualquer vetor compatível indisponibiliza
esses modos. Falhas de integridade continuam invalidando a consulta inteira. Ausência/vazio só dispensam RPC quando observados
pelo preflight do M borrowed. Sem M a resposta é model_unavailable, mesmo que o
corpus possa estar ausente. Com M, fully-covered nonempty ainda executa Query.
FTS sem índice conserva o erro real `lexical_index_unavailable`.

Falhas MCP têm `is_error` e structured content `{kind:error,code,message}`.
Wire inválido usa `invalid_arguments`; validação/seleção usa `invalid_query`;
Auth usa `access_denied`, lease usa `service_denied`, cancel/deadline usa
`request_cancelled_or_expired`. Outras causas conservam classes native/model/
operation/response e a mensagem real, sem converter falha em vazio ou fallback.
Recuperação de coverage exige indexação real, não uma leitura que crie índice.

Fontes: `crates/graphit-app/src/source_server/memory_search.rs` (definition,
Arguments, execute, delivery e serializers); `source_server.rs` apenas registra
e despacha esse tool. Os três casos HTTP/RMCP do módulo `memory_search/tests.rs`
exercitam os contratos de modo, availability e cancel/join com SDK e modelo real.

### Qualificação Source MCP

Três funções distintas HTTP/RMCP passaram no Linux com modelo tiny/SDK reais.
Após a correção de lifetime do lease no delivery, os dois casos que retornam
resultados do modelo passaram contra Source cfgtest recém-compilado:
`actual_memory_modes_http_rpc_preserve_archive_channels_filters_and_unavailable`
em 45,147s e `actual_memory_no_model_fts_and_closed_model_absence_empty_are_distinct`
em 7,653s. O caso
`actual_memory_closed_wire_separate_service_permission_deadline_and_cancel_join`
passou em 1,848s; seus caminhos terminam antes do trecho corrigido e não foram
repetidos. A compilação atual passou em 3m09s. São três funções qualificadas,
sem somar reruns ou testes de produtores a essa contagem.

As sete execuções totais, incluindo duas falhas iniciais de fixture (dimensão
hardcoded e perfil local não ativado), e os sucessos anteriores à correção
permanecem nos logs/JSON. O recibo da folha (`../../.build/rust-records-memory-mcp-consumer/validation.json`; historical local artifact unavailable)
registra comandos, source, tentativas e escopos. Essa qualificação é funcional
PROJECT MCP Linux; não certifica qualidade de embeddings, USER, Task/Dream,
Gemma, desempenho ou outras plataformas.
