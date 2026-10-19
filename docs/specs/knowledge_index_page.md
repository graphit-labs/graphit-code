---
title: Leitura local selecionada de página Knowledge
doc_type: spec
description: Página Knowledge local autenticada, com corpo e proveniência do mesmo pin e validação selecionada até EOF.
references:
  - target: {type: task, id: tsk-21cd}
    relation: implements
  - target: {type: task, id: tsk-70cf}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-1f4a}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: relates_to
  - target: {type: task, id: tsk-185c}
    relation: refined_by
  - target: {type: task, id: tsk-cab0}
    relation: refined_by
---

# Leitura local selecionada de página Knowledge

A leitura de uma página retém somente o documento solicitado, preserva os 16 campos públicos e valida todo o stream selecionado antes de retornar sucesso. Registros alheios não são carregados nem validados por esse get.

```rust
pub async fn read_native_knowledge_page(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    slug: &str,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeIndexPageRead, KnowledgeIndexError>;
```

A API pertence a `graphit_data::knowledge_index`. O caller fornece uma admissão real de leitura PROJECT Knowledge vinculada ao mesmo `original`, conforme a [admissão de registros](project_record_admission.md). Projeto e branch derivam dessa admissão; não há URI ou versão alternativa no request. Um `ModuleSnapshot` externo não substitui a autorização local.

## Estados e geração

| Resultado | Observação após validação completa |
| --- | --- |
| `NoCorpus` | Corpus local ausente após aquisição autorizada. |
| `Published { generation, page: PublishedEmpty }` | Snapshot publicado válido com zero linhas físicas. |
| `Published { generation, page: PageAbsent }` | Snapshot válido com linhas físicas, sem documento Active do slug solicitado. |
| `Published { generation, page: Found { document, focused_source } }` | Documento FULL16 owned, retornado após EOF. |

`PublishedEmpty`, `PageAbsent` e `Found { document: Box<KnowledgeDocument>, focused_source }` pertencem a `KnowledgeSnapshotPage`. Rows Removed selecionadas são validadas antes de excluir; um corpus só com tombstones continua Published e retorna PageAbsent. O [contrato de persistência](../architecture/rust-knowledge-reference-storage.md) define estado/projeção, completude e integridade. A geração vem do pin realmente capturado, incluindo versão, branch física e alvo herdado. Branches nomeadas/herdadas seguem a admissão, sem materialização. Ausência não cria corpus; a leitura não utiliza writer ou reparo.

Stream, scanner, pin e store são descartados antes da verificação final do contexto, da autorização e do guard. O documento retornado continua válido após o descarte dos owners do caller.

## Slug e limites

O slug é comparado literalmente como UTF-8. Não há remoção de `.md`, espaços ou fragmentos, normalização Unicode ou comparação sem distinção de caixa.

Após verificar a admissão e antes da aquisição nativa, a função rejeita slug vazio, `/`, `\`, prefixo `..` ou caracteres de controle com `InvalidField("slug")`. Mais bytes que `max_identifier_bytes` produzem `LimitExceeded("identifier_bytes")`.

O scanner normal do store usa um filtro tipado pela identidade `slug`; o pin e
seu namespace vêm da admissão. O schema físico fechado de 18 campos é verificado
antes da consulta. O collector valida estado, namespace, witness, tipos,
nulabilidade, campos editoriais, receitas, valores temporais, digests e targets
dos registros selecionados. Não há `LIMIT 1` no stream selecionado: duplicatas
do slug devem falhar mesmo quando um documento válido já foi encontrado.

Corpo e `focused_source` são extraídos do mesmo registro/pin. A proveniência é
opcional para rows sem projeção de referências; nenhum namespace é fabricado.
Não há uma segunda consulta de referências para compor a página. Um resultado
selecionado vazio usa somente um probe de metadata `slug` limitado a uma linha
física para distinguir `PublishedEmpty` de `PageAbsent`, sem ler outro corpo.

- `max_identifier_bytes` limita individualmente os identificadores validados.
- `batch_rows` configura o SDK e conserva o check físico de batch do collector.
- Limites nativos protegem a captura LOCAL correspondente.

Não há `max_pages`, `max_text_bytes` ou `max_input_edges` agregados neste
contrato. Targets retornados preservam ordem, duplicatas e valores originais.
O stream selecionado precisa chegar a EOF antes de entregar sucesso. A antiga
exigência de rejeitar o get por corrupção/duplicidade em documento não
selecionado está supersedida; exportação e enumeração integral conservam seus
contratos globais. Um erro no registro selecionado continua erro sem prefixo.

## Autorização e falhas

O callback LOCAL verifica Original, admissão e guard físico antes/depois dos awaits controlados, entre batches/linhas/targets e no EOF. `NoCorpus` também exige uma verificação final.

Cancelamento, prazo expirado, negação, invalidação física, erro SDK ou documento inválido produzem `KnowledgeIndexError`; a causa propagada não vira ausência. Os checks são cooperativos nas fronteiras do SDK, sem promessa de preempção nativa. A API não realiza retry ou reparo automático.

Um arquivo no lugar do diretório de corpus é erro de aquisição; diretório corrompido é erro SDK. Nenhum deles significa `NoCorpus`. O resultado e seu `Debug` contêm geração e documento; o caller escolhe o que registrar.

## Relação entre readers

A [leitura de snapshot SDK](knowledge_module_snapshot.md) conserva sua API, enum e proveniência do caller. Ela utiliza o mesmo scanner selecionado e collector com checks de Original, sem conceder autorização para aquisição LOCAL.

`read_native_knowledge_index` permanece integral e retém todos os documentos,
sem as antigas quotas cumulativas. Full e selected readers conservam schema,
identidade, witness, duplicatas e EOF no conjunto que cada operação lê. A integração com o
[renderer de fontes](../architecture/rust-knowledge-document-source.md)
pertence ao App consumer.

## Verificação

O target `knowledge_index_page` usa RuntimeKeyOwner, scopes capturados, `authenticate_runtime` e `admit_authenticated_read/write` reais. Publicação H/Data e mutações de linhas usam o SDK efetivo. As fixtures protegem estados, geração nomeada/herdada, ausência de escrita, documento owned, soma dos bodies maior que o limite por documento, independência de registros alheios, invalidade/duplicação selecionadas, factory negada, classe/Original incorretos, slug, cancelamento concorrente/prévio, prazo expirado antes da chamada e Other/corrupt reais.

```text
cargo test --offline --locked -p graphit-data --test knowledge_index_page -j2
cargo clippy --offline --locked -p graphit-data --lib --test knowledge_index_page -j2 --no-deps -- -D warnings
```

Qualificação histórica do reader anterior: suíte local **7 PASS**, sem falhas/ignored (harness 3,17 s); regressão focal do reader remoto **1 PASS** (0,02 s, quatro testes fora do filtro); Clippy estrito **exit 0**; rustfmt/diff e três links relativos válidos. O caso focal de documento owned também passou após a remoção do `drop` desnecessário (1 PASS, 2,88 s, seis fora do filtro). Receipts em `.build/rust-knowledge-selected-local`.

Histórico preservado: primeira execução local 6 PASS e falha própria ao tratar URI `file:` como Path, corrigida com conversão e permissões publicadas; segunda execução bloqueada por edição transitória do Core; terceira execução local aprovada. O primeiro Clippy encontrou um `drop` desnecessário de wrapper sem `Drop` na fixture; a remoção foi compilada pelo segundo Clippy, sem alteração da produção ou dos asserts. Outras plataformas não foram executadas.

A extensão física de c8e9 preserva o DTO público; sua qualificação própria de schema/estado/witness permanece registrada separadamente no contrato de persistência.

O armazenamento preserva schema físico FULL18, leitura do conjunto selecionado até EOF, witnesses, namespace/estado, causas reais e limites individuais de identificadores. O get não efetua CAS ou publicação.
