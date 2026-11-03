---
type: specification
title: Primeira publicação Knowledge com metadata authored em Rust
description: Criação exclusiva de documentos compilados com projeções de referência completas ou incompletas na mesma imagem.
references:
  - target: {type: task, id: tsk-6101}
    relation: implements
  - target: {type: task, id: tsk-41d1}
    relation: derived_from
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8b24}
    relation: uses
  - target: {type: task, id: tsk-9983}
    relation: uses
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-f02f}
    relation: uses
  - target: {type: task, id: tsk-185c}
    relation: refined_by
---

# Primeira publicação Knowledge com metadata authored em Rust

Para integradores da biblioteca Knowledge que publicam pela primeira vez um
conjunto explícito de arquivos capturados e compilados. Escopo Native Linux;
a implementação da tsk-6101 está qualificada em Linux por 28 funções distintas:
11 novas (Data5, Knowledge3 e phases SDK3) e 17 existentes do índice/codec afetado.
A extensão que protege o publisher complete antigo pertence à mesma função de
teste e acrescenta zero casos distintos. Strict focal de duas libs/dois targets,
formatação, diff e cinco links locais passaram. Esta operação cria um corpus
novo, incluindo um corpus vazio, com metadata documental e referências na mesma
imagem física de 18 campos.

## Jornada e autoridade

O consumidor captura e compila os arquivos com o Original da requisição, prepara
`AuthoredDocumentInput` e chama a nova API com um admission real `WriteKnowledge`
daquele Original. O projeto, diretório Native e branch vêm do admission; não são
selecionados por YAML, URI fornecida pelo consumidor ou pelas referências.
O resultado `KnowledgeFirstPublication` conserva a geração default e, quando
selecionada, a geração da branch materializada pelo SDK existente.

A criação exige apenas escrita. Uma leitura posterior do corpus ou das referências
exige seu próprio admission `ReadKnowledge`. Targets PROJECT estrangeiros,
USER e contextos importados são valores literais; nenhum grant de target é inferido.

```rust
pub async fn publish_native_documents_with_authored_references(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    documents: &[AuthoredDocumentInput<'_, '_>],
    limits: NativeDocumentPublicationLimits,
) -> Result<KnowledgeFirstPublication, NativeAuthoredFirstPublicationError>;
```

O adapter reutiliza o mapping, fingerprint, campos H e parsing authored existentes.
Ele parseia `CompiledDocument::whole_source`, com o fallback e os limites reais da
receita. Strings YAML decodificadas e views de tuples permanecem emprestadas de
owners vivos até o retorno de Data. O corpo completo, digests, timestamps,
parâmetros da receita, staleness e wiki targets chegam na mesma publicação.
Data não depende de Records nem infere referências parseando o corpo.

## Decisões authored na primeira criação

| Metadata do documento | Decisão | Projeção persistida |
|---|---|---|
| Campo `references` omitido | Preserve | `None`, incompleta |
| `references: []` | Replace vazio | `Some`, conhecida e vazia |
| Sequência válida de referências | Replace literal | `Some`, com tuples e witness |
| Null, tipo incorreto ou tuple inválida | Erro authored | Nenhum corpus criado |

Um corpus pode conter os três primeiros estados na mesma imagem. Preserve não
inventa uma lista conhecida vazia. Todos os documentos nascem `Active`.
IDs, relações, espaços, Unicode, ordem, duplicatas e distinção `field: ''`/ausente
seguem o [contrato authored](../architecture/rust-knowledge-authored-references.md).
Wiki targets são persistidos literalmente, inclusive vazios e duplicados; a
[consulta de referências](rust_knowledge_reference_query.md) aplica suas próprias
regras de apresentação e reporta cobertura incompleta se houver projeções None.

A API ordinary `publish_native_documents` conserva seu contrato: produz Active
com None, mesmo que o whole source contenha YAML authored. Para processar essa
metadata na primeira publicação, o consumidor escolhe explicitamente a nova API.

## Fronteira Data e integridade

Consumidores com H já preparada usam:

```rust
pub async fn publish_native_authored_with_reference_updates(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &[KnowledgeDocumentReferenceUpdate<'_>],
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError>;
```

`KnowledgeDocumentReferenceUpdate { slug, authored }` e
`KnowledgeDocumentAuthoredUpdate::{Preserve, Replace(&[KnowledgeAuthoredReference])}`
são os valores compartilhados com a [republicação](rust_knowledge_document_republication.md).
Cada header exige exatamente uma decisão; entradas ausentes, adicionais ou
repetidas e observações de compilação falham antes de I/O. A namespace source é
PROJECT com o ProjectId real do admission e `context: None`.

Os campos públicos 0–15 mantêm o schema documental. O campo 16 é Active e o campo
17 é uma projeção nullable. Some tem witness SHA256 de 32 bytes verificado com
slug, título, caminho, digest de compilação, estado, namespace, tuples e wiki
raw. Nenhuma API pública aceita um RecordBatch livre.

O preflight valida identidade individual, recipe, timestamps nanosegundos,
wiki targets, namespaces e tuples antes de construir as projeções. Content
contém somente `max_identifier_bytes`; igualdade passa e excesso falha.
Não há soma agregada de páginas, referências cruas ou texto. A imagem
canonical18 completa passa pelo validador C8 antes da criação exclusiva.
Arrow/schema/conversões e causas permanecem reais; Artifact/Payload no writer
não representam quota de bytes serializados da imagem Lance.

## Erros e efeitos confirmados

O adapter retorna `Mapping`, `Authored`, `Projection`, `Data` ou `Allocation`,
com causa alcançável por `Error::source`. Wrong admission, Original diferente,
revogação, cancelamento, deadline e limites não se tornam sucesso vazio.
Em `Data(Artifact(...))`, `Error::source` fornece o `ArtifactError` de Core,
que conserva `stage()` e `kind()`, incluindo Lock/Interrupted. Core não retém o
`io::Error` original; consumidores usam esses getters tipados para diagnóstico.
O writer Native aguarda fora do executor assíncrono e conserva o lock até a
checagem terminal. Admission e captura são rechecados nas fronteiras reais.

Um Main publicado nunca é adotado ou sobrescrito por esta API. Para documentos
Active existentes use republicação com ReadKnowledge, WriteKnowledge e geração
esperada. Materializar uma branch herdada é outra operação de lifecycle.

Preparação SDK sem publicação pode deixar diretório e fragments. `NoCorpus`
significa ausência de Main publicado: probes nativos válidos podem observar
`DirectoryPresence::Absent`; quando index.lance existe, somente `DatasetNotFound`
no open SDK do Main sem pin permite essa observação, com identidade da captura e autoridade
revalidadas antes e depois do await. Erros de schema, corrupção, IO, permissão,
`VersionNotFound` e falha de captura/pin continuam causais. A observação não
prova rollback nem que nunca houve histórico.

Uma nova tentativa exige novas admissões, captura das fontes autorizadas e writer
real. Create/CAS exclusivo publica dados novos; não lê, adota, apaga, recria ou
importa fragments anteriores. Um concorrente que publique Main impede a criação;
não há retry de commit ou resolução automática de resultado incerto.

Se o SDK confirmou o default e uma etapa posterior falha, retorna
`KnowledgeIndexError::PartialDefault { generation, cause }`. A geração é o
default efetivamente confirmado; a causa original de cancelamento, Auth ou SDK
permanece acessível. Isso não promete rollback ou atomicidade entre Auth e SDK.
Antes de qualquer tentativa de recuperação, um consumidor autorizado deve
observar o corpus/branch com um novo admission de leitura; não repetir criação
exclusiva esperando que ela adote o efeito anterior.

## Evidência e limites da qualificação

Fontes: `crates/graphit-data/src/knowledge_first_publication.rs`,
`knowledge_index.rs` (sink exclusivo e finalização) e `knowledge_references.rs`
(validadores/witness/builder C8); adapter
`crates/graphit-knowledge/src/authored_first_publication.rs`.

As fixtures exercitam Native PROJECT real, arquivos FILE capturados e compilados,
SDK default/named, imagem mista, empty EOF, exclusive reject, raw tuples/wiki,
limites exatos, Original estrangeiro, revogação, writer waiting e efeitos
confirmados sob cancelamento/deadline. Os units privados chamam o mesmo produtor,
com observação de fases, sem substituir SDK, grants ou writer.

```text
cargo test --offline --locked -p graphit-data --test knowledge_first_publication -j1
cargo test --offline --locked -p graphit-knowledge --test authored_first_publication -j1
cargo test --offline --locked -p graphit-data --lib knowledge_first_publication:: -j1
```

Os dois targets --test não executam os units privados. Receipt local:
`.build/rust-knowledge-authored-first-publication/validation.json`. App, HTTP,
leitura importada, validação ponta a ponta, outros sistemas operacionais e desempenho
não são qualificados por esta folha. O [schema de persistência](../architecture/rust-knowledge-reference-storage.md)
e a [publicação ordinary](../architecture/rust-knowledge-native-publication.md)
conservam seus contratos e receipts históricos.

O armazenamento preserva schema físico FULL18, leitura até EOF, witnesses, namespace/estado, CAS, causas reais e limites individuais de identificadores, sem contabilidade agregada do corpus.
