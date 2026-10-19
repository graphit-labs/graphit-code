---
type: architecture
title: Persistência nativa de referências Knowledge em Rust
description: Estado documental e witnesses íntegros no corpus nativo Knowledge, preservando os DTOs públicos de 16 campos.
references:
  - target: {type: task, id: tsk-c8e9}
    relation: implements
  - target: {type: task, id: tsk-41d1}
    relation: relates_to
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-c1a4}
    relation: uses
  - target: {type: task, id: tsk-8b24}
    relation: relates_to
  - target: {type: task, id: tsk-9983}
    relation: relates_to
  - target: {type: task, id: tsk-185c}
    relation: refined_by
---

# Persistência nativa de referências Knowledge em Rust

Data persiste estado documental e referências junto ao documento em um único
RecordBatch com 18 campos. `KnowledgeDocument` conserva seus 16 campos públicos.
Data não importa Records: o caller fornece valores Data admitidos. Os produtores [de publicação](rust-knowledge-native-publication.md) e
[metadata authored](rust-knowledge-authored-references.md) permanecem separados;
a [primeira publicação authored](../specs/rust_knowledge_authored_first_publication.md)
compõe os produtores em uma API opt-in, com None incompleto e Some explícito na
mesma imagem. A qualificação dessa composição pertence à sua folha; o publisher
ordinary conserva Active/None.

## Preparar e publicar

`graphit_data::knowledge_references` expõe `KnowledgeDocumentState::{Active,Removed}`,
`KnowledgeReferenceTargetKind::{Task,Session,Memory,Knowledge}`,
`KnowledgeReferenceScope::{Project(ProjectId),User(&str)}`,
`KnowledgeReferenceNamespace {scope,context}`,
`KnowledgeAuthoredReference {target_kind,scope,id,context,relation,field}` e
`KnowledgeReferenceFields {slug,state,namespace,authored}`.

IDs, relações, USER e contextos presentes são não vazios. Contexto de target
é exclusivo de Knowledge. `field: Some("")` difere de `None`. Valores, ordem e
duplicatas não são normalizados.

```rust
pub fn prepare_knowledge_references<'data, 'request>(
    original: &'request SnapshotContext,
    documents: &PreparedKnowledgeDocumentRows<'data, '_>,
    fields: &[KnowledgeReferenceFields<'data>],
    limits: KnowledgeIndexContentLimits,
) -> Result<PreparedKnowledgeReferences<'data, 'request>, KnowledgeIndexError>;
```

Antes dos witnesses, a preparação valida documentos, wiki-links, referências
authored e cobertura exata dos headers, sem orçamento agregado. Cada header tem exatamente
uma entrada por slug. Slugs ausentes, adicionais
ou duplicados falham. Observações de compilação não são linhas publicáveis.
O produto copia views dos owners externos e possui witnesses de 32 bytes:
independe do slice temporário de fields, mas empresta textos, listas e request.
Não realiza I/O, não possui autorreferência e rejeita composição com headers
ou request diferentes.

```rust
pub async fn publish_native_authored_with_references(
    original: &Arc<SnapshotContext>,
    admission: &ProjectWriteAdmission<'_>,
    rows: &PreparedKnowledgeDocumentRows<'_, '_>,
    references: &PreparedKnowledgeReferences<'_, '_>,
    limits: KnowledgeIndexWriteLimits,
) -> Result<KnowledgeFirstPublication, KnowledgeIndexError>;
```

A publicação nativa Linux exige origem PROJECT com o ProjectId capturado e
`context: None`. Targets USER ou estrangeiros são dados, sem grant.
O publisher existente `publish_native_authored` conserva assinatura e grava
`Active` com projeção ausente, declarando incompletude sem marcador fictício.

Ambos reutilizam primeira publicação, writer exclusivo e finalização SDK.
Não adotam corpus existente, sobrescrevem por fallback ou fazem retry.
`PartialDefault` conserva a geração já confirmada e a causa de falha posterior.

## Schema fechado

Os campos 0–15 conservam seus nomes, ordem e tipos: slug, title, body, kind,
summary, description nullable, summary_truncated, source_path, physical_digest
nullable, compilation_digest, recipe, recipe_parameters, updated_at nullable,
stale_since nullable, stale_after nullable e resolved_wiki_targets.
Os tipos são os do schema documental anterior, incluindo digests binários de
32 bytes, timestamps UTC nanosegundos e targets como List de item Utf8 não nullable.
`recipe_parameters` mantém fallback_title, kind_on_absent e max_summary_bytes.

As duas colunas adicionais são:

| Posição | Campo | Tipo | Nullable |
|---:|---|---|:---:|
| 16 | document_state | UInt8: Active=1, Removed=2 | Não |
| 17 | reference_projection | Struct | Sim |

A Struct contém, em ordem: `source_scope_kind: UInt8`, `source_scope_id: Utf8`,
`source_context: Utf8 nullable`, `authored_refs: List` e
`witness: FixedSizeBinary(32)`. Os demais campos são não nullable.

Cada item não nullable de authored_refs é uma Struct com, em ordem:
`target_kind: UInt8`, `scope_kind: UInt8`, `scope_id: Utf8`, `id: Utf8`,
`context: Utf8 nullable`, `relation: Utf8`, `field: Utf8 nullable`.
PROJECT/USER têm códigos 1/2; Task/Session/Memory/Knowledge, 1/2/3/4.

Nomes, ordem, tipos e nullability são exatos. Outros códigos são inválidos.
Não há decoder de compatibilidade do antigo schema físico de 16 campos.
Uma Struct presente exige witness: nulo ou incorreto é corrupção, sem fallback.
Struct ausente é a única incompletude legítima nesta composição.

## Witness

SHA-256 recebe o domínio UTF-8 `graphit.knowledge.reference-projection`, NUL,
e JSON compacto da tupla:

```text
[1, slug, title, source_path, compilation_digest32, document_state,
 source_scope_kind, source_scope_id, source_context,
 ordered_authored_refs, ordered_resolved_wiki_targets]
```

O digest documental é serializado como array de 32 bytes. Cada referência é:

```text
[target_kind, target_scope_kind, target_scope_id, id, context, relation, field]
```

PROJECT e USER com o mesmo texto de ID produzem hashes distintos. String vazia
difere de null; ordem e duplicatas participam do hash. O decoder recalcula o
witness com os campos físicos antes de expor resultados. O witness não é grant,
geração SDK ou alias de compilation_digest.

Uma projeção válida com listas vazias é completa para os dados persistidos.
Isso não certifica resolução de destinos nem cobertura global. Namespace e
alvos armazenados permanecem literais: uma observação de importação
artifact/context não os renomeia ou altera o witness.

## Estado e readers

Removed exige projeção válida e listas authored e Wiki vazias. Ausência de
projeção ou referências residuais torna a linha inválida. Rows Removed são
validadas antes de excluir; duplicatas incluem slugs de rows excluídas.

Corpus fisicamente ausente produz NoCorpus. Zero linhas físicas produz
PublishedEmpty. Linhas físicas todas Removed produzem PageAbsent para uma
página, com geração publicada. Catálogo e heads ficam vazios, conservando
Published e geração; não significam ausência de corpus.

O DTO head continua `{slug,title,updated_at}`. A projeção física de oito campos
inclui esses três, source_path, compilation_digest, resolved_wiki_targets,
document_state e reference_projection. Body não é projetado, retido ou validado
pelo head reader. Page/index validam os fields de todas as linhas até EOF.

A leitura local vincula origem ao ProjectId capturado. O reader de pin fornecida
continua Data-only; aquisição, autorização e guard externo de importação
pertencem aos produtores próprios. Veja [página local](../specs/knowledge_index_page.md)
e [ModuleSnapshot](../specs/knowledge_module_snapshot.md).

## Limites e falhas

`KnowledgeIndexContentLimits` conserva somente `max_identifier_bytes`.
Preparação, full read, query e Write usam esse limite individual real,
inclusive para IDs, scope IDs e contextos onde o contrato exige identifiers.
Não há quota agregada de rows, referências cruas, texto admitido ou output
copiado, nem limite de páginas para o inventário de unicidade.

`KnowledgeHeadReadLimits` conserva `max_slug_bytes`, limites nativos e
`batch_rows`. Heads usa o mesmo validator compartilhado com limite
individual ausente; não deriva uma quota de identifier do texto.
Schema, namespace, estado, witness, duplicatas e Removed são validados antes
da exclusão, até EOF. Slug e controles físicos de batch/captura continuam
nos caminhos reais. Nenhuma dessas políticas mede RSS ou a imagem Lance.

Original e guard são verificados antes/depois das etapas controladas, por
batch/linha, EOF e entrega. O witness serializa somente views imutáveis e
implementações locais puras de `Serialize`: o guard do caller executa antes
da serialização e depois do hash, antes de expor o digest. Ele verifica o mesmo
Original e, nos caminhos admitidos de I/O, a admissão correspondente. Fragmentos JSON
(aspas, escapes e separadores) não revalidam autenticação nem filesystem.
O adapter de SHA-256 apenas recebe bytes, sem materializar o JSON completo.
Falhas nas fronteiras conservam a primeira causa tipada, inclusive IO,
cancelamento e prazo. Os callers mantêm checks antes de efeitos, envio ao SDK
e commit; limites individuais e validação integral do witness permanecem.
Isso não oferece preempção interna da serialização ou do SDK,
confinamento contínuo de pathname ou recuperação de toda OOM nativa.

KnowledgeIndexError conserva causas Auth, Core/I/O, H, Arrow, store, branch e
ReferenceSerialization quando presentes. Witness inválido é
`InvalidField("reference_witness")`. Corrupção tardia impede prefixo saudável.
Leitura não reinterpreta body, cria listas vazias, repara projection ou recupera
witness inválido como ausência. Operações futuras de escrita devem usar dados
estruturados admitidos e tratar efeitos confirmados causalmente.

## Verificação

Qualificação em Linux: 17 testes focados do índice, 10 integrações de página,
cinco de snapshot e uma de schema passaram com o SDK real; a regressão da ponte
Knowledge passou seis casos. O doctest de owner emprestado passou. A matriz
protege scopes, nullables, ordem e duplicatas, state/Removed, adulteração de
witness/header/namespace/target-scope, limites, cancelamento e EOF.
Witness nulo é recusado pelo Arrow ao construir o Struct com filho não nullable;
a fixture não simula um batch fisicamente admissível. Removed corrompido tardio
e duas linhas Removed válidas com o mesmo slug impedem entrega em page, heads e
index.

```text
cargo test --offline --locked -p graphit-data --lib knowledge_index -j1
cargo test --offline --locked -p graphit-data --test knowledge_index_page -j1
cargo test --offline --locked -p graphit-data --test knowledge_module_snapshot -j1
```

O `receipt` `.build/rust-knowledge-reference-storage/validation.json` conserva
resultados, logs e falhas reais de qualificação. Esta folha não qualifica indexação incremental,
query global, leitura importada, lifecycle de escrita, Browser/IAB, plataformas
não executadas ou FullNoLoss.

O armazenamento preserva schema físico FULL18, leitura até EOF, witnesses, namespace/estado, CAS, causas reais e limites individuais de identificadores, sem contabilidade agregada do corpus.
