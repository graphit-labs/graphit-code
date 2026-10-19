---
title: Primeira publicação nativa de documentos Knowledge
doc_type: architecture
references:
  - target: {type: task, id: tsk-c8e9}
    relation: relates_to
  - target: {type: task, id: tsk-8b24}
    relation: implemented_by
  - target: {type: task, id: tsk-84d5}
    relation: specified_by
  - target: {type: task, id: tsk-93e8}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8218}
    relation: integrates
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
---

# Primeira publicação nativa de documentos Knowledge

O módulo `graphit_knowledge::native_index_publication` compõe documentos já
compilados com o primeiro publisher de Data. Sua disponibilidade acompanha o
produtor atual: Linux. A entrada é um conjunto finito explícito; ela não afirma
que uma árvore ou projeto inteiro foi selecionado. O chamador preserva a captura
e os `CompiledDocument` de P1 durante a chamada.

```rust,ignore
pub async fn publish_native_documents(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    documents: &[AuthoredDocumentInput<'_, '_>],
    limits: NativeDocumentPublicationLimits,
) -> Result<KnowledgeFirstPublication, NativeDocumentPublicationError>
```

Cada entrada fornece o documento compilado, slug, origem lógica `source_path`,
`updated_at`, `stale_since` e `stale_after`. A ponte não deduz esses valores do
nome físico nem cria datas. Ela verifica operação Knowledge, o request original
ativo, identidade do mesmo `Arc` na admissão e `recheck` Auth antes da preparação.
Somente WRITE Knowledge é necessário para publicar; a leitura posterior requer
sua própria admissão READ Knowledge. A versão física deriva da admissão Data.

O [mapping nativo](rust-knowledge-native-document-mapping.md) produz a receita,
fingerprint de compilação e targets. Views comuns de `&str` mantêm esses targets
emprestados, sem owner que referencia a própria alocação. H de Data prepara os
campos existentes com observações Current vazias. Compiled documents, mapping,
views, campos e rows continuam vivos até terminar o `await` do publisher real.
Toda a preparação termina antes da primeira chamada de persistência.

`NativeDocumentPublicationLimits` reúne sete políticas explícitas: `parse`,
`mapping`, `fingerprint`, `references`, `authored_references`, `projection` e
`index`. `references: ReferenceLimits` contém somente o limite individual de
identificador do grafo. `authored_references: DocumentAuthoredReferenceLimits`
limita a saída authored 1:1 por documento e seus identificadores; os adapters
first-authored e republication usam esse campo separado. O chamador fornece
todos explicitamente.
A qualificação atual de `tsk-c9e2` passou em Linux: 157 funções afetadas,
Clippy estrito, compilação das duas fixtures App e três doctests modificados.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa falhas iniciais e replays. Windows e macOS não foram executados; os
receipts históricos permanecem intactos. Cada produtor aplica seus próprios limites lógicos; esses
valores não prometem teto de RSS ou preempção física. A ponte reserva suas views
com `try_reserve_exact` e mantém erros de alocação na cadeia causal.

## Resultado e falhas

O retorno Data `KnowledgeFirstPublication` contém a geração default e, quando a
versão autenticada é nomeada, sua geração nomeada. Um conjunto vazio cria uma
geração publicada vazia com schema, distinta de `KnowledgeIndexRead::NoCorpus`.
Slug e origem continuam literais; o reader Data devolve documentos owned em ordem
UTF-8 por slug. O reader e a validação do schema físico pertencem a Data.
Os DTOs documentais conservam 16 campos; o schema nativo tem 18. Esta ponte usa
`publish_native_authored`, que grava Active com projeção de referências ausente,
portanto incompleta. A [API Data com referências](rust-knowledge-reference-storage.md)
publica origem, autoria e witness completos; integrá-la ao publisher Knowledge
pertence à sua própria folha.

Os campos persistidos conservam corpo integral, título, kind, summary,
description opcional, truncamento, origem, digest físico binário, digest de
compilação binário, receita e parâmetros, datas opcionais, intervalo de staleness
e targets. Digests físico e de compilação são identidades diferentes. Targets
persistidos seguem o mapping; a normalização de relações é feita pelo kernel de
referências enquanto os documentos e as views do chamador estão vivos.

`NativeDocumentPublicationError` distingue `Mapping`, `Projection`, `Data` e
`Allocation`, cada uma conservando o erro original como `source()`. Denial,
request estrangeiro, cancelamento e deadline anteriores à preparação impedem
birth mesmo quando a entrada é inválida. Limites e falha em qualquer documento
durante preparação não publicam os documentos anteriores do conjunto.

Data publica exclusivamente o Main de `knowledge/index.lance`: não adota nem
sobrescreve Main publicado e não faz update, retry de commit ou recuperação
automática de resultado incerto. Diretório e fragments sem Main podem permanecer
após preparação. Uma nova leitura autorizada observa `NoCorpus` por probe nativo
válido `DirectoryPresence::Absent` ou, quando index.lance existe, somente por
`DatasetNotFound` no open SDK do Main sem pin, revalidando captura, identidade e
autoridade também após o await. Schema, corrupção, IO/permissão, versões e pins
inválidos continuam erros. Essa observação não prova rollback ou ausência de
histórico. Uma nova tentativa usa novas admissões, fontes capturadas e writer
real; Create/CAS publica dados novos sem ler, adotar, apagar ou recriar os
fragments anteriores. Main publicado por concorrente impede a criação. `Data(KnowledgeIndexError::PartialDefault { generation, cause })`
significa que Data já confirmou o default e preserva essa geração e a causa da
falha seguinte. Uma falha anterior conserva seu erro original. Não se promete
rollback ou ausência de efeitos físicos para toda falha de storage.

Cancelamento e deadline permanecem cooperativos no request original. Data
revalida nos limites de publicação; descartar a future não prova que o worker
de aquisição do lock terminou ou foi interrompido fisicamente. Essas fronteiras
são descritas em [Knowledge, Data e Records](rust-knowledge-data-boundaries.md).

## Verificação e escopo de integração

`crates/graphit-knowledge/tests/native_index_publication.rs` usa o issuer real,
capturas FILE retidas, compilação P1, SDK e reader Data. Confere os campos FULL16
de duas páginas Unicode, referências cruzadas e dangling, geração nomeada, vazio,
republish, Auth/revogação, cancelamento, deadline e preparação integral com caps
nos três produtores. Provas internas SDK de pin, EOF, schema, concorrência e
PartialDefault continuam nas suítes Data; esta ponte não as reimplementa.

```text
cargo test --locked -p graphit-knowledge --test native_index_publication
cargo clippy --locked -p graphit-knowledge --lib --test native_index_publication -- -D warnings
```

Esses comandos descrevem a suíte focal; a Task registra execução e plataforma
efetivamente qualificadas. Reader público Knowledge, atualização incremental,
busca, geração por domínio, exportação e lifecycle permanecem entregas próprias.

A leitura posterior pode usar a [fonte virtual nativa](rust-knowledge-document-source.md)
para projetar os metadados FULL16 e selecionar o corpo integral pelo Core.

A etapa de [diagnóstico de metadados publicados](rust-knowledge-published-metadata-lint.md)
consome os documentos do EOF Data e conserva description integral separada de summary.

## Preparação de exportação Markdown

Após EOF da leitura Data, documentos e geração podem alimentar a
[exportação Markdown nativa](rust-knowledge-markdown-export.md). O resultado
é um plano Core puro; a publicação FS exige autoridade e destino próprios do
chamador. A fixture nativa também cobre essa composição e PublishedEmpty.
