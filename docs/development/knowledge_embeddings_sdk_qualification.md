---
references:
  - target: {type: task, id: tsk-4fde}
    relation: implements
  - target: {type: task, id: tsk-446e}
    relation: refines
  - target: {type: task, id: tsk-8a17}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: preserves
  - target: {type: memory, id: 01M4N8R1BM1TM7T5ZDAM79D2KC, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses
  - target: {type: memory, id: 01M4N9Z0VRXWQNC7KKF7A2RQET, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: uses
---

# Qualificação SDK de armazenamento e NN para embeddings Knowledge

Esta nota atende Data e o consumidor Knowledge `tsk-446e` em `graphit-code`. A folha `tsk-4fde` qualifica somente primitivas Lance/Lance Linalg **13.0.0**, com Arrow **58.3**, em datasets temporários próprios. A primeira execução Linux dos cinco testes de [knowledge_embeddings_sdk.rs](../../crates/graphit-data/tests/knowledge_embeddings_sdk.rs) terminou com **1 PASS/4 FAIL, exit 101**, conforme test-first.log (`../../.build/rust-records-data/knowledge-embeddings-sdk/test-first.log`; historical local artifact unavailable). O caso de inputs NN inválidos passou; as quatro comparações de schema/rows falharam pela reconstrução SDK do child FixedSizeList de nullable=false para true. O contrato foi revisado antes da correção Source. A segunda execução Linux (`../../.build/rust-records-data/knowledge-embeddings-sdk/test-second.log`; historical local artifact unavailable) concluiu **5 PASS/0 FAIL/0 ignorados/0 filtrados, exit 0**, runtime 0,03 s e total reportado de 16,633 s. A expectativa SDK explícita, componentes efetivamente não nulos, seis valores completos, métricas e CAS foram observados. As duas primeiras tentativas Clippy falharam antes do probe por alterações upstream Core; a terceira concluiu exit 0 com `-D warnings`, fechando a qualificação SDK/Linux nos limites desta nota. Os tempos não são benchmark, e lifecycle/checks são consultados na Task.

Root compôs uma única dependência de produção `lance-linalg` 13, sem features extras ou troca de versões Lock, e reportou metadata scoped real. A composição permite o uso de `DistanceType`; não qualifica o probe. Foundation/Branch `tsk-8a17` e FULL16 `tsk-72ce` são precedentes preservados, sem edições ou replay dessas suítes. Vetores determinísticos do fixture exercitam storage/NN, não ModelService ou uma API semântica de domínio.

## Fixture físico e leitura NN: Q1–Q2

O dataset real é criado por `Dataset::write` e retido em `Arc<Dataset>`. O schema de input local tem seis campos externos não nulos:

| Campo | Tipo/valor do fixture |
| --- | --- |
| `slug` | Utf8, identidade textual do caso. |
| `input_fingerprint` | FixedSizeBinary(32), bytes determinísticos por linha. |
| `config_id` | FixedSizeBinary(32), bytes `[7; 32]`. |
| `projection_recipe` | Utf8, literal `sdk-fixture-v1`. |
| `role` | Utf8, literal `Document`. |
| `vector` | FixedSizeList de largura positiva, child `item: Float32` não nulo. |

Dimensões 2/3/4 são larguras da fixture, não dimensões de modelo, ABI produtiva ou autoridade. O input conserva child `item`, Float32 não nulo. O schema SDK esperado conserva o nome `item`, mas reconstrói esse child com nullable=true. `expected_sdk_rows` constrói essa expectativa explicitamente e compartilha os buffers Arrow do input; os batches efetivamente observados nunca são normalizados. `image` percorre EOF e verifica null_count zero em todas as colunas externas e em `vector.values()`, além de nome `item`/nullable=true no child. O probe não modifica o índice Knowledge FULL16.

A diferença não é nome vazio: o Debug Arrow omite `item` com nullable=false (`arrow-schema` field.rs:63–85). No SDK, `lance-core-13.0.0/src/datatypes/field.rs:1217–1256` descarta o Field primitivo do child; `datatypes.rs:379–391` conserva tipo do elemento/dimensão e `482–488` reconstrói `Field::new("item", elem, true)`. Isso já aparece no schema de `Operation::Overwrite`, antes do commit (`lance-13.0.0/src/dataset/write/insert.rs:202–205,301–305`). A segunda execução comprovou o schema SDK explícito e zero valores nulos nas colunas/componentes. Esse comportamento também está registrado em Memory `01M4N9Z0VRXWQNC7KKF7A2RQET`. Preservação estrita da nullability/metadados do child não é qualificada por este probe. A observação não altera o contrato produtivo Knowledge `tsk-446e` nem autoriza downgrade de sua representação.

NN usa `Scanner::nearest("vector", &Float32Array, 3)`, `distance_metric(DistanceType)` e `use_index(false)`. `fast_search` permanece no default false. A projeção inclui `slug` e o SDK fornece `_distance` Float32 nullable; as linhas concretas do teste devem ter zero nulls e distâncias finitas. O resultado é coletado até EOF do stream NN, cujo top-k é três. Não há índice aproximado ou promessa de busca exaustiva além dessa consulta flat.

Para `a=[1,0]`, `b=[2,1]`, `c=[0,2]` e query `[1,0]`, as assertions usam tolerância Float32 `< 1e-5`:

| Métrica | Fórmula SDK | Ordem/distâncias esperadas |
| --- | --- | --- |
| L2 | Soma dos quadrados, sem raiz. | a:0, b:2, c:5 |
| Cosine | `1 - dot/(norma(query)*norma(vector))` | a:0, b:`1-2/sqrt(5)`, c:1 |
| Dot | `1 - dot` | b:-1, a:0, c:1 |

Dot pode ser negativa; não se impõe distância >=0. Query vazia, k=0 e dimensão errada devem preservar `lance::Error::InvalidInput`, sem converter erro em resultado vazio saudável. NN não avança manifesto. Esses parâmetros e o campo de saída estão nas fontes SDK `lance-13.0.0/src/dataset/scanner.rs:1990–2086,2263,2478,6350`; o caso de inputs inválidos passou nas duas execuções e as assertions de NN positivo passaram na segunda, após corrigir a expectativa do schema.

## Troca de schema/dimensão por Overwrite: Q3–Q5

A preparação usa a API real `InsertBuilder::new(Arc::clone(&captured))`, `with_params(&WriteParams { mode: WriteMode::Overwrite, ..Default::default() })` e `execute_uncommitted(vec![batch])`, retornando `Transaction`. `transaction.read_version` conserva a base e `Operation::Overwrite` contém o novo schema. Append, usado pelo vencedor estrangeiro da fixture, conserva compatibilidade do schema existente. Fontes: `lance-13.0.0/src/dataset/write/insert.rs:144,301–319,337–339`.

Arquivos físicos podem ser escritos na preparação, mas geração latest, schema e linhas publicadas não devem mudar antes do commit. A publicação é `CommitBuilder::new(Arc::clone(&captured)).with_max_retries(0).with_skip_auto_cleanup(true).execute(transaction)`, sobre o mesmo handle capturado, sem destination URI que recarregue latest.

A decisão preservada em Memory `01M4N8R1BM1TM7T5ZDAM79D2KC`, atualizada na revisão 2 com os fatos runtime, é específica: **Overwrite + zero retries ativa strict_overwrite e pula inclusive o rebase inicial**, publicando em `transaction.read_version + 1`. As fontes estáticas são `lance-13.0.0/src/io/commit.rs:1595–1608,1634–1675,1686,1693,1918–1926`. CreateIndex/Merge não têm essa garantia e conservam seus guards existentes. Esta folha não copia o IndexFence da [qualificação FTS](lance13_fts_index_qualification.md), nem cria wrapper/handler artificial de publicação.

O caso dim2→dim3 verifica schema e rows completos após base+1, query3 aceita e query2 rejeitada no novo pin. O pin antigo conserva dim2, suas rows e query2. Dois Overwrites concorrentes dim3/dim4 têm UUIDs distintos antes de publicar: o primeiro vence, o segundo deve retornar CommitConflict sem rebase/avanço/adoção. Um Append estrangeiro também deve impedir Overwrite antigo, conservando o schema dim2 e as rows do vencedor.

Readback usa `Dataset::read_version_transaction(base + 1)` e compara UUID vencedor/perdedor. Não há novo resolver, retry, limpeza ou rollback. O fixture deve preservar o erro SDK causal; commit-status-unknown, se ocorrer, permanece indeterminado e não pode ser convertido em Conflict ou ausência segura. Zero retries não significa ausência de arquivos uncommitted.

## Mapa das cinco fixtures e limites: Q6

A primeira falha e a segunda execução bem-sucedida ficam separadas abaixo. As quatro falhas originais permanecem no log; os PASS seguintes vêm da execução real:

| Teste | Contrato exigido | Estado runtime |
| --- | --- | --- |
| `six_columns_roundtrip_and_flat_nn_metrics_keep_the_same_pin` | Comparação com schema SDK explícito de seis campos e três métricas/ordem/distâncias; nenhuma publicação pela query. | PASS na segunda execução; FAIL histórico por schema. |
| `invalid_k_empty_query_and_wrong_dimensions_are_sdk_errors_without_publication` | InvalidInput causal para k0/query vazia/dimensões erradas; geração preservada. | PASS Linux nas duas execuções. |
| `uncommitted_overwrite_changes_dimension_only_after_strict_commit` | Prepare sem manifesto, schema SDK novo já na transação, base+1/UUID e pin anterior conservado. | PASS na segunda execução; FAIL histórico por schema. |
| `competing_dimension_overwrites_keep_the_winner_and_its_transaction_uuid` | UUIDs distintos, loser CommitConflict, schema/rows/geração do vencedor intactos. | PASS na segunda execução; FAIL histórico por schema. |
| `a_foreign_append_prevents_old_overwrite_from_rebasing_or_adopting_rows` | Append estrangeiro vence; Overwrite antigo falha sem adotar ou remover rows. | PASS na segunda execução; FAIL histórico por schema. |

O probe não entrega domínio/API/ABI de embeddings Knowledge, matching ConfigId, receita de projeção produtiva, ModelService, política Unchanged/no-op, identidade de corpus ou reset físico/ABA. Também não prova Auth/Core, caller admission/write lease, captura Core, atomicidade multitable, branch-domain, USER ou outros sistemas operacionais. Metadados do fixture não resolvem esses contratos pendentes de `tsk-446e`. Nenhum nonce, ledger de quota, benchmark, formato Go ou geração fictícia é introduzido.

## Execução Linux e receipt consolidado

Do root do projeto, usar uma execução Cargo por vez na fila normal, target quente e `-j1`; configurar `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` e `CARGO_INCREMENTAL=0`:

```sh
cargo test --offline --locked -p graphit-data --test knowledge_embeddings_sdk \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1 -- --show-output
cargo clippy --offline --locked -p graphit-data --test knowledge_embeddings_sdk \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1 -- -D warnings
rustfmt --edition 2024 --check crates/graphit-data/tests/knowledge_embeddings_sdk.rs
```

O log `test-first.log` preserva 1 PASS/4 FAIL e suas causas de schema, sem tratar mudança de nullability como nome vazio. `test-second.log` comprova os cinco testes PASS, incluindo NN e Overwrite/CAS. O primeiro Clippy (`../../.build/rust-records-data/knowledge-embeddings-sdk/clippy.log`; historical local artifact unavailable), com resultado exit 101 (`../../.build/rust-records-data/knowledge-embeddings-sdk/clippy-result.json`; historical local artifact unavailable), levou 3,624 s e falhou na compilação upstream Core antes do probe: incoerências envolvendo `PayloadLimits.pins/capacity`, `unusedContent` e `Operation.transient` removidos. Após ACK Root, o segundo Clippy (`../../.build/rust-records-data/knowledge-embeddings-sdk/clippy-second.log`; historical local artifact unavailable), com resultado exit 101 (`../../.build/rust-records-data/knowledge-embeddings-sdk/clippy-second-result.json`; historical local artifact unavailable), levou 3,378 s e também falhou antes do probe: `NativeDocumentReceipt::largest` não utilizado em `native_document_file.rs:1598`, sob `-D dead-code`. As incoerências foram resolvidas pelos respectivos donos. O terceiro Clippy (`../../.build/rust-records-data/knowledge-embeddings-sdk/clippy-third.log`; historical local artifact unavailable), com resultado exit 0 (`../../.build/rust-records-data/knowledge-embeddings-sdk/clippy-third-result.json`; historical local artifact unavailable), concluiu `-D warnings` no target compartilhado, `-j1` e profiles acordados, em 284,362 s incluindo lock. Não houve afrouxamento de warnings ou edição estrangeira por esta folha. O Source SHA256 `29ec2581a0aee0f22a21b9f378b9cc05db528b109d52051052370b96b96cb4ab` é o mesmo da execução runtime com cinco PASS. Os dois logs de falha e suas causas permanecem preservados; esses tempos não são benchmark. O receipt (`../../.build/rust-records-data/knowledge-embeddings-sdk/validation.json`; historical local artifact unavailable) consolida logs, resultados, SHAs e limites.

Não repetir SourceData/Branch/FTS/Models sem alteração afetada, nem ampliar para gate workspace. Doc→Source confirma input, expectativa SDK explícita, ausência de normalização observada, APIs e assertions; Source→doc delimita ausência de entrega produtiva. Runtime dos cinco fixtures e Clippy estrito estão comprovados: qualificação SDK/Linux concluída neste escopo. Estado lifecycle/checks permanece na Task; esta nota não declara prontidão de produção ou entrega do domínio Knowledge. Windows/macOS UNRUN; os PASS Linux não qualificam esses sistemas, recursos/performance ou consumidores pendentes.
