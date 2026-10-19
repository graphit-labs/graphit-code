---
references:
  - target: {type: task, id: tsk-4f78}
    relation: implements
  - target: {type: task, id: tsk-dc2d}
    relation: specified_by
  - target: {type: task, id: tsk-8a17}
    relation: uses
  - target: {type: task, id: tsk-3a8e}
    relation: relates_to
---

# Qualificação de FTS JSON e publicação estrita de índices Lance13

Esta nota atende mantenedores Data e Records do projeto `graphit-code`. O alvo é Lance, Lance Table e Lance Index **13.0.0**, com datasets temporários próprios. Os oito testes de [lance_fts_index.rs](../../crates/graphit-data/tests/lance_fts_index.rs) foram executados pelo primary de `tsk-4f78`: **8 PASS, 0 falhas, 0 ignorados no Linux**, conforme o quarto log (`../../.build/rust-records-memory/search-contract/fts-qualification-fourth.log`; historical local artifact unavailable). O build levou 2,76 s e os testes 0,15 s. As duas assertions de erro reforçadas passaram depois no log final de erros (`../../.build/rust-records-memory/search-contract/fts-qualification-errors-final.log`; historical local artifact unavailable): 2 PASS, seis filtrados. Clippy estrito terminou com exit 0; owned rustfmt e diff check também passaram. Esta transcrição reutiliza evidências do primary e inspeciona Source/logs; não executou Cargo.

O report de contrato (`../../.build/rust-records-memory/search-contract/index-publication-report.json`; historical local artifact unavailable) de `tsk-dc2d` delimita as APIs reais, a dependência Foundation/Branch `tsk-8a17` e os gaps para o produtor futuro. O report é evidência anterior à execução, não um receipt de runtime. Root compôs a devdependency direta `lance-index` 13 com `default-features = false`; esta folha não altera manifests ou lockfiles.

Esses resultados e as verificações estritas qualificam as assertions SDK delimitadas nesta nota no Linux. A API pública Data de publicação de índice e o Source lexical Memory continuam trabalhos separados. Não há certificação de Auth, cadeias Memory, seleção de histórico, USER, Dream, AI ou produto completo. Os fixtures não fornecem autorização para operações de produção.

## Primitivas e protocolo de publicação: Q1–Q2

`DatasetIndexExt::create_index_builder` e `CreateIndexBuilder::execute_uncommitted(&mut self)` constroem arquivos físicos e retornam `IndexMetadata`. `metadata.dataset_version` registra a versão capturada para a construção. O protocolo exige observar que a construção não publicou manifesto nem avançou a geração. Arquivos físicos escritos não equivalem a índice publicado. As conveniences `create_index` e `optimize_indices` chamam `apply_commit` internamente e não demonstram o protocolo estrito desta folha.

As fontes SDK registradas no report são `lance-13.0.0/src/index/api.rs:199–216`, `index/create.rs:155–164,659–721` e `index.rs:2716–2727`. O teste usa diretamente `CreateIndexBuilder::new(..., &["payload"], IndexType::Inverted, &params)`, com nome explícito e política de substituição explícita.

A publicação segue esta sequência:

1. Capturar o `Dataset` real, sua localização, branch e versão base; conservar o pin enquanto o índice é construído.
2. Construir o índice sem commit e preparar `Operation::CreateIndex { new_indices, removed_indices }`.
3. Criar `TransactionBuilder::new(base, operation).build()`. O UUID da transação identifica a tentativa; o UUID do índice identifica o índice. Eles não são intercambiáveis.
4. Executar `CommitBuilder` sobre o dataset capturado, com `max_retries(0)` e `skip_auto_cleanup(true)`.
5. Antes de delegar `commit` ou `commit_after`, o `IndexFence` do teste compara a localização real, a branch e a versão do manifesto com **base + 1**. Demais operações são delegadas ao handler SDK original.
6. Reabrir a branch real e observar geração, índice e UUID da transação. Uma tentativa antiga após vencedor estrangeiro deve falhar sem rebase, adoção da transação estrangeira ou avanço adicional.

`TransactionBuilder` e `Operation::CreateIndex` estão em `lance-table-13.0.0/src/transaction/builder.rs:17–76` e `operation.rs:69–76`. A política é confrontada com [lance_branch_store.rs](../../crates/graphit-data/src/lance_branch_store.rs), especialmente o commit estrito de delta e a recuperação por branch/UUID. O helper local `IndexFence` permanece exclusivamente no teste; não entrega um adapter `PreparedIndex` de produção.

O teste F1 lê `read_version_transaction(base + 1)` da branch publicada. F2 compara o UUID do vencedor com o UUID da tentativa perdida. O erro observado foi commit conflict para a versão 3 após zero retries, preservando a geração base+1 e o UUID do vencedor. Isso não injeta lost acknowledgement nem qualifica todos os estados de recuperação. O futuro produtor deve preservar a primeira causa e os resultados inconclusivos; uma observação de outra branch ou outro UUID não prova Applied. Não há limpeza de órfãos ou retry automático nesta folha.

## Analyzer e consulta literal: Q3–Q4

O analyzer do fixture é configurado explicitamente:

```rust
InvertedIndexParams::default()
    .lance_tokenizer("json".into())
    .base_tokenizer("simple".into())
    .max_token_length(None)
    .lower_case(true)
    .stem(false)
    .remove_stop_words(false)
    .ascii_folding(false)
```

`max_token_length(None)` desabilita o filtro de tokens longos. `Some(n)` aceita apenas tokens cujo comprimento em **bytes UTF-8 é menor que n**, antes dos filtros de lowercase, stemming e folding. Não existe um limite 40 adotado implicitamente por este contrato. As fontes são `lance-index-13.0.0/src/scalar/inverted/tokenizer.rs:795,827,859–887,1079–1102` e `lance-tokenizer-13.0.0/src/remove_long.rs:53–55`. O analyzer final do produto depende do aceite dos resultados por seu owner.

A consulta usa `MatchQuery::new(literal_triplets)`, coluna `payload`, `Operator::Or` e fuzziness zero; `FullTextSearchQuery::new_query(...).limit(None)` alimenta `Scanner::full_text_search`. O scanner projeta `row_key`, sem offset/limit de candidatos, e o teste coleta o stream até EOF. As fontes públicas estão em `lance-index-13.0.0/src/scalar/inverted/query.rs:391,412,432`, `scalar.rs:137–163` e `lance-13.0.0/src/dataset/scanner.rs:1746,4804–4812`.

A allowlist fixa contém somente:

| Forma do payload | Paths permitidos |
| --- | --- |
| Head | `title`, `body`, `tags`, `attributes.classification` |
| Creation | os mesmos quatro sob `memory.*` |
| Revision | os mesmos quatro sob `after.*` |

`before`, `input`, `reason`, `audit` e IDs não entram na allowlist. O valor da consulta não escolhe paths. O helper divide o valor por `;`, descarta segmentos vazios e prefixa **cada segmento** com cada path fixo, no formato `path,str,value`. A divisão em três partes mantém vírgulas no valor; newline também permanece valor. As assertions SDK executadas verificam esse encoding: não é um parser Boolean nem uma API geral de query fornecida ao usuário.

O fixture aprovado verifica palavras permitidas, `TitleMarker`/`titlemarker`, `função`/`FUNÇÃO`, ausência de `funcao` e `run`, stopword `the`, `Running`, `my_symbol`, vírgula/newline e tentativas de selecionar `audit`/`reason`. Tags e classificação retornam exatamente as quatro famílias codificadas. OR é a combinação entre triplets; não se infere semântica adicional de operadores que o teste não exerce.

## Fixture físico, EOF e falhas: Q5–Q6

O fixture Arrow preserva a forma de **18 colunas**: `row_key`, `kind`, `record_id`, `project_id`, `operation_version`, `idempotency_key`, `idempotency_actor`, `title`, `revision`, `created_at`, `updated_at`, `created_by`, `updated_by`, `superseded`, `tombstone`, `reference_generation`, `references` e `payload`. As colunas string são Utf8, os quatro campos numéricos são UInt64 e `superseded`/`tombstone` são Boolean, todos não nulos.

São linhas sintéticas head/creation/revision/tombstone com JSON serializado. O schema não certifica metadados físicos completos de Memory, IDs canônicos, história contígua ou semântica de tombstone. A filtragem Data `tombstone = false` prova apenas a seleção dessas linhas codificadas. O teste não importa Records para Data e não cria schema vetorial paralelo.

A leitura de candidatos usa uma versão capturada e drena EOF. O caso de 120 linhas excede um top-k pequeno e o append adiciona uma linha ainda não coberta pelo índice anterior. O teste consulta essa linha e depois substitui **somente o índice conhecido** `content` por construção uncommitted e commit estrito. O pin antigo mantém 120 resultados; o novo fornece 121. Duas linhas físicas revision, ambas com `record_id = same-logical-memory`, aparecem integralmente entre os candidatos, sem prefixo parcial de EOF. Isso não implementa collapse Memory, prioridade, ordenação canônica ou top-k de heads únicos.

`load_indices` é público, mas omite índices ilegíveis ou não suportados; `load_all_indices` é `pub(crate)`. Essas limitações estão em `lance-13.0.0/src/index/api.rs:283–307` e `index.rs:3105`. Há uma leitura pública completa dos metadados de índices do manifesto: `lance_table::io::manifest::read_manifest_indexes(store.as_ref(), dataset.manifest_location(), dataset.manifest())`, usando o store obtido por `dataset.object_store(None)`. O scout confirmou essa API no Source; ela não foi exercida pelo fixture de refresh arbitrário. A leitura completa do manifesto não equivale a conseguir carregar ou atualizar todo índice. Substituir o índice criado e conhecido pelo fixture continua sendo a única prova runtime de refresh nesta folha; a integração do produtor e a qualificação de refresh arbitrário permanecem pendentes. A operação CreateIndex preserva entradas não removidas, conforme `lance-table-13.0.0/src/transaction/manifest_build.rs:1157–1178`.

F6 observou panic de parse JSON em `lance-index-13.0.0/src/scalar/inverted/tokenizer/document_tokenizer.rs:158`, propagado pelo SDK como `Error::IO { source: JoinError::Panic(...) }` em `scalar/inverted/builder.rs:553`. O teste grava `"{invalid"`, isola a construção em `tokio::spawn`, preserva a causa e confirma geração 1. A assertion reforçada verifica o variant IO e a causa `JSON parse error`; mantém também o tratamento explícito de panic propagado pelo join externo. O teste de query raw `body,str,alpha;beta` recebe `Error::InvalidInput` com `Invalid triple format: beta`, sem retornar resultado vazio bem-sucedido. O encoding seguro `alpha;beta` continua retornando a linha esperada. Isso não entrega um guard de produção nem torna o SDK graceful: o produtor futuro precisa validar integralmente o payload tipado antes da construção. Falha ou stream incompleto não pode virar NoMatch.

## Mapa de evidência e obrigações de qualificação

As assertions abaixo passaram no quarto log; os dois testes de erro também passaram após seu reforço tipado. A cobertura Q1–Q6/F1–F6 continua delimitada pelos fixtures SDK e pelo gap de refresh público descrito acima.

| Plano | Teste atual | Assertions e limites |
| --- | --- | --- |
| F1 / Q1–Q2 | `uncommitted_strict_publication_pin_branch_and_uuid_recovery` | Construção sem avanço, UUIDs distintos, branch `feature` em base+1, transação observada, main e pin antigo preservados, consulta após reopen. |
| F2 / Q2 | `foreign_winner_prevents_rebase_and_uuid_adoption` | Append SDK estrangeiro vence; tentativa antiga falha; geração/linhas do vencedor e UUID estrangeiro preservados, índice perdido não publicado. Não injeta lost acknowledgement. |
| F3 / Q4–Q5 | `json_paths_literal_encoding_unicode_long_tokens_and_tombstones` | Paths permitidos e negativos, encoding, case/acento/código, tokens ASCII de 160 e Unicode de 80 bytes com `None`, tags/classificação nas quatro famílias, tombstone filtrado e NoMatch. Sem certificação de archive/cadeia Memory. |
| F4 / Q3 | `explicit_token_cap_is_strict_utf8_bytes_and_none_removes_filter` | `Some(6)`: ASCII 4/5 bytes aceitos e 6/7 rejeitados; `éé` 4 bytes aceito e `ééé` 6 rejeitado. `None` aceita os tokens longos ASCII/Unicode do F3. |
| F5 / Q6 | `eof_unindexed_append_known_refresh_and_old_pin` | 120 candidatos até EOF; append não indexado; refresh do índice conhecido; novo pin 121 e antigo 120. Duas linhas físicas revision compartilham o mesmo ID lógico e ambas são coletadas; collapse Memory não é implementado. |
| F6 / Q5 | `malformed_json_sdk_outcome_is_not_no_match` | IO com causa de panic JSON, geração preservada e query raw inválida com InvalidInput; não prova validação tipada de produção. |
| F4 / Q5 | `sdk_missing_empty_and_no_match_are_distinct_observations` | Dataset publicado vazio: zero linhas/NoMatch/geração 1; URI ausente: erro de open e nenhum diretório criado. Não define NoCorpus de produção. |
| F6 / Q4 | `malformed_raw_triplet_query_is_an_error_not_empty_results` | Query raw inválida retorna InvalidInput; query literal codificada preserva resultado esperado. |

A1 (`chk-28c58425fa87`) cobre Q1–Q2; A2 (`chk-dbcdd58408ed`) Q3–Q5; A3 (`chk-10f794d7e7ce`) Q6; A4 (`chk-764496eee4c6`) a documentação. T1 (`chk-4e21895cfa47`) cobre F1/F2; T2 (`chk-37478f864adf`) F3–F6; T3 (`chk-473e452efd65`) comandos e consistência. Esses checks são do coordenador e estão pendentes no snapshot recebido, sem atualização por esta transcrição.

## Execução afetada e receipt

Do root do projeto, Root autorizou o target compartilhado quente e execução serial `-j1`. Um Cargo por vez, com debug/incremental desabilitados:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo test --offline --locked -p graphit-data --test lance_fts_index \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1 -- --show-output

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo clippy --offline --locked -p graphit-data --test lance_fts_index \
  --target-dir .build/rust-records-data/knowledge-index-source/target -j1 -- -D warnings

rustfmt --edition 2024 --check crates/graphit-data/tests/lance_fts_index.rs
```

Os logs atuais ficam em `.build/rust-records-memory/search-contract/fts-qualification-*.log`; o diretório de artifacts previsto no packet é `.build/rust-records-memory/search-contract/fts-qualification`. A sequência preserva o primeiro erro de compilação (`../../.build/rust-records-memory/search-contract/fts-qualification-first.log`; historical local artifact unavailable), import de Transaction wire corrigido para o alias `SdkTransaction`; o segundo erro de sintaxe (`../../.build/rust-records-memory/search-contract/fts-qualification-second.log`; historical local artifact unavailable), corrigido; e a terceira execução (`../../.build/rust-records-memory/search-contract/fts-qualification-third.log`; historical local artifact unavailable), seis PASS e uma falha porque o token Unicode longo consultado não estava no payload. Essa omissão foi corrigida antes dos oito PASS do quarto log. As falhas permanecem auditáveis; os logs não são substituídos por um resumo de sucesso.

O receipt atual é **oito testes SDK PASS no Linux**, mais dois testes de erro reforçados PASS (seis filtrados), **Clippy estrito exit 0 e owned rustfmt/diff check PASS**. O log estrito (`../../.build/rust-records-memory/search-contract/fts-qualification-clippy-qualified.log`; historical local artifact unavailable) registra 3 min 47 s, incluindo espera por file lock. Falhas externas anteriores de compilação (`E0425` em Auth purpose:119) e Clippy (`needless_borrow` em client:21) foram corrigidas pelos respectivos donos; a folha SDK não alterou esses arquivos nem repetiu as operações SDK para corrigi-los. O primary consolidará `.build/rust-records-memory/search-contract/fts-qualification/validation.json`; este texto não antecipa atualização de checks ou fechamento Task. Nenhuma suíte ampla de workspace é exigida por esta folha SDK; não houve implementação autorizada de publisher Data ou consumidor Memory. Windows/macOS permanecem UNRUN; os tempos observados não são benchmark nem prova de performance, memória residente ou autorização de produto.

Para integração futura, Data deve fechar publicação/recuperação e refresh conhecido ou completo em uma folha Source própria. Records mantém o contrato [Rust Memory](../specs/rust_memory_records.md): current/archive/deleted, same pin, EOF, collapse e ordenação antes do top-k. A qualificação SDK não fecha esses comportamentos de Memory nem requer o serviço AI para a busca lexical.
