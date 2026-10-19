---
type: specification
title: Consulta nativa de referências Knowledge em Rust
description: Proveniência literal, cobertura explícita e páginas vinculadas à geração observada.
references:
  - target: {type: task, id: tsk-9d61}
    relation: implements
  - target: {type: task, id: tsk-c8e9}
    relation: uses
  - target: {type: task, id: tsk-bd00}
    relation: uses
  - target: {type: task, id: tsk-41d1}
    relation: derived_from
  - target: {type: task, id: tsk-16a8}
    relation: relates_to
  - target: {type: task, id: tsk-7fc2}
    relation: relates_to
  - target: {type: task, id: tsk-3514}
    relation: relates_to
  - target: {type: task, id: tsk-185c}
    relation: refined_by
  - target: {type: task, id: tsk-fa7b}
    relation: corrected_by
---

# Consulta nativa de referências Knowledge em Rust

Esta API serve a consumidores da biblioteca que precisam consultar referências
outgoing de um documento ou incoming de uma identidade lógica em uma geração
Knowledge. Conserva namespace, categoria, ordem e duplicatas persistidas e
informa quando a projeção não permite uma resposta completa.

A implementação está validada em Linux por fixtures Native/Auth/SDK reais e
checks focais. O escopo e os limites de validação estão ao final desta página.

## Entrada Native e autoridade

O adapter `graphit_knowledge::native_reference_query` recebe os targets lógicos
Records sem transformar esses valores em autoridade:

```rust
pub struct KnowledgeLogicalTarget<'a> {
    pub target_kind: ReferenceTargetKind,
    pub scope: LogicalScope<'a>,
    pub id: &'a str,
    pub context: Option<&'a str>,
}
pub struct KnowledgeReferencesQuery<'a> {
    pub outgoing: Option<&'a str>,
    pub incoming: Option<KnowledgeLogicalTarget<'a>>,
    pub page_size: NonZeroUsize,
    pub outgoing_cursor: Option<&'a KnowledgeReferencesCursor>,
    pub incoming_cursor: Option<&'a KnowledgeReferencesCursor>,
}
pub async fn read_native_knowledge_references(
    original: &Arc<SnapshotContext>,
    admission: &ProjectReadAdmission<'_>,
    query: KnowledgeReferencesQuery<'_>,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeReferencesRead, KnowledgeReferenceQueryError>;
```

Ao menos um foco é obrigatório. Outgoing seleciona um slug literal; incoming
seleciona categoria, namespace completo e ID. Ambos podem ser solicitados na
mesma operação. `page_size` deve ser positivo e não exceder 100.

Targets podem ser Task, Session, Memory ou Knowledge, PROJECT ou USER. IDs,
USER e contextos presentes são não vazios e limitados por identificação.
Contexto presente é exclusivo de Knowledge. USER encontrado em uma referência
do corpus PROJECT não autoriza consulta USER ou ROOT.

O adapter delega a
`graphit_data::knowledge_reference_query::read_native_knowledge_reference_query`,
cujo `KnowledgeReferenceQuery` usa
`KnowledgeReferenceTarget { target_kind, namespace, id }`. DTOs de resultados,
cursor, limites e erro pertencem a Data. Não há DTO paralelo de grant.

Projeto, URI e seleção de branch derivam da admissão PROJECT Knowledge real,
vinculada ao mesmo Original. A cada chamada, inclusive continuação, o reader
aplica `limits.native` à captura física nova e abre schema18 canônico.
Auth, Original e captura são rechecados nas fronteiras SDK, durante os scans,
em EOF e antes da entrega. Branch herdada é observação SDK; não materializa
branches nem cria ou repara corpus.

```rust
pub enum KnowledgeReferencesRead {
    NoCorpus,
    Published {
        generation: KnowledgeGeneration,
        read: Box<KnowledgeReferenceSnapshotRead>,
    },
}
```

`NoCorpus` exige ausência física observada sob admissão válida. Auth, captura,
schema e SDK não são ausência. Continuação com corpus ausente é `InvalidCursor`.
`UnknownSource` significa que um corpus foi observado, mas o slug outgoing não
identifica origem Active após validar o primeiro scan até EOF: inclui slug
Removed e publicação vazia. Incoming sem
matches pode retornar página publicada vazia.

## Pin fornecida pelo caller

Data também oferece:

```rust
pub async fn read_knowledge_reference_snapshot_guarded(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    generation: &KnowledgeGeneration,
    query: KnowledgeReferenceQuery<'_>,
    limits: KnowledgeIndexReadLimits,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeReferenceSnapshotRead, KnowledgeReferenceQueryError>;
```

URI e versão declaradas devem corresponder à pin real. Esta API valida schema e
usa a seleção interna CallerPin. Não abre pathname, adquire grant ou transforma
generation em autoridade. O getter de URI de ModuleSnapshot é crate-private;
não expõe Dataset público. O consumidor importado é uma entrega separada
(tsk-3514), com [Source e contrato próprios](rust_imported_reference_query.md),
apoiada pelo [locator](../architecture/rust-knowledge-import-locator.md) e pela
[leitura importada](rust_imported_knowledge.md). A query importada está
qualificada no Linux em seu próprio escopo de oito funções SDK distintas, um
compile-fail separado e check/strict/fmt/diff aprovados; esta prova Native não a
substitui nem qualifica composição global.

## Scans e integridade

Dois scans completos da mesma pin projetam somente slug, title, updated_at,
source_path, compilation_digest, resolved_wiki_targets, document_state e
reference_projection. O primeiro valida metadata, calcula contagens e mantém
heads limitados. O segundo valida novamente e retém somente as edges das páginas
solicitadas. Não mantém uma lista completa de edges.

Em respostas bem-sucedidas, ambos chegam a EOF e verificam tipos, nullability,
estados, namespaces,
witnesses e duplicatas, incluindo Removed. Falha tardia impede prefixo de
sucesso. Body e recipe não são projetados nem validados por esta consulta.
O [contrato de persistência](../architecture/rust-knowledge-reference-storage.md)
define schema18, estado e witness. CompilationDigest32 é metadata opaca; não
recompila body. O witness é recalculado com metadata e referências persistidas.

## Proveniência literal

Cada `KnowledgeReferenceEdge` contém source (slug, título, source_path, digest,
namespace e witness opcionais), payload, provenance, ordinal e target_observation.
Payload authored conserva target/relation/field; WikiLink conserva target e não
inventa relation ou field authored.

Authored conserva ordem e duplicatas. Wiki conserva duplicatas, self-links e
destinos inexistentes. Somente target Wiki literalmente vazio não vira edge;
continua preservado na lista crua e na integridade, sem quota agregada de edges. Por exemplo,
`["self", "", "ghost", "self"]` gera ordinais Wiki 0, 2 e 3.

Wiki targets usam o namespace persistido da origem, sem inferência pelo projeto
capturado ou substituição por alias. Resultados se ordenam por slug da origem,
proveniência e ordinal. Incoming compara identidade literalmente.

`KnowledgeReferenceObservation::KnowledgeHead { slug, title }` aparece somente
para Knowledge encontrado entre heads Active com namespace idêntico. Outros
targets permanecem Unresolved. Um head observado não prova OwnRead, acesso ao
body, resolução externa ou autorização do destino. Nenhum resultado executa
OwnRead dos destinos.

## Cobertura e paginação

Projeção None é ausência de dados tipados atuais. Não inventa namespace nem
reconstrói referências a partir do body. Emite
`KnowledgeReferencesWarning::MissingCurrentProjection { source }`.
Outgoing dessa origem retorna zero edges conhecidas e `complete: false`, distinto
de projeção completa vazia.

`KnowledgeReferenceSnapshotRead` contém
`focused_source: Option<KnowledgeReferenceSource>`, outgoing/incoming
opcionais, coverage e warnings. Cada página conserva results, total,
complete e next_cursor.

A extensão `tsk-fa7b` devolve a fonte outgoing solicitada como dado ordinary
owned da mesma pin usada nas páginas. `KnowledgeReferenceSource` conserva
slug, title, source_path, CompilationDigest32 e namespace/witness opcionais
observados. Não reconstrói a fonte a partir da primeira edge.

Uma origem Active com projeção válida retorna focused_source Some mesmo
quando outgoing tem zero edges; namespace e witness são Some, e a página
vazia permanece completa. Uma origem Active sem projeção retorna Some
com namespace e witness None, mantendo MissingCurrentProjection e
complete=false. Incoming-only retorna focused_source None, mesmo quando
há origens nos matches. Slug outgoing ausente ou Removed conserva
UnknownSource; NoCorpus conserva sua variante e regra de ausência física.

O resultado bem-sucedido só é entregue após os dois scans completos
da mesma pin e os guards terminais existentes da operação admitida.
Falha tardia não entrega metadata parcial nem vira focused_source None.
Geração, cursors, paginação, proveniência e limites individuais permanecem
inalterados. A observação não concede OwnRead da origem ou dos destinos.
Completude e cobertura mantêm estas regras:

- Outgoing é completo para sua origem quando a projeção é válida.
- Incoming é completo para as origens Knowledge examinadas quando nenhuma
  origem Active está sem projeção.
- `coverage.knowledge_source_complete` registra essa cobertura Knowledge.
- `coverage.inbound_other_sources_not_qualified` permanece verdadeiro.

Complete descreve completude da projeção e pode coexistir com next_cursor.
Total conta edges conhecidas, não referências desconhecidas ou total global
entre módulos. O consumidor continua com o cursor da direção correspondente,
admissão viva e os mesmos bindings, até next_cursor None.

## Continuações e limites

`KnowledgeReferencesCursor` tem campos privados, Serde fechado e rejeição
de campos desconhecidos. Vincula URI/versão SDK, branch física/alvo herdado,
seleção Native ou CallerPin, ambos os focos, direção, offset e policy
individual de identifier. Troca desses bindings, cursor sem foco e offset
além do total conhecido causam `InvalidCursor`.

Page size continua em `1..=100` e não integra binding. Batch size é
tuning SDK e pode mudar numa continuação legítima. `read.native` também
não integra cursor: cada operação Native aplica seus controles atuais à
nova captura. Cursor não conserva grant, request ou captura anterior.

As APIs recebem `KnowledgeIndexReadLimits` diretamente. O wrapper
`KnowledgeReferenceQueryLimits`, quota de texto copiado e soma de textos
de query/cursor/URI/selector foram removidos. Identificadores lógicos
continuam validados por `content.max_identifier_bytes`; locators
físicos não são slugs.

Os dois scans permanecem completos no mesmo pin, com checks físicos de
batch, schema, namespace, estado, witness, duplicatas e EOF. Totais,
proveniência, ordinais e offsets continuam funcionais. Erro tardio impede
a resposta inteira. Não há budget compensatório ou promessa de RSS.

`KnowledgeReferenceQueryError` contém Data(KnowledgeIndexError),
InvalidQuery(&'static str), InvalidCursor e UnknownSource. Error::source mantém
cadeia Data. Cancelamento/deadline, revogação, captura alterada, corrupção e SDK
não viram páginas vazias ou warnings de ausência. O consumidor deve corrigir
input/policy individual ou readmitir/capturar a geração quando o binding muda; não deve usar
um prefixo descartado pela operação que falhou.

## Qualificação Linux

Data executou nove fixtures com Native/Auth/SDK reais (native06: 9/0/0,
8,86s). A extensão focal de corrupção Removed também passou (native07: 1/0/0,
8 filtradas, 1,92s) e não aumenta essa contagem. Knowledge executou uma fixture
com bearer real emitido/verificado (native02: 1/0/0, 3,52s).

Os casos protegem proveniência e ordinais, namespaces/targets literais,
projeções ausentes/publicação vazia, paginação e pin/URI/geração reais,
cursor/direção/focos/offsets/limites, captura física fresca, body fora da projeção,
witness/state/duplicates/schema/Removed inválidos e erros SDK integrais.
Também cobrem Original diferente, módulo errado, revogação real, captura
substituída, deadline e cancelamento concorrente/terminal. Limites exatos e +1
incluem linhas físicas Removed, edges cruas vazias, identificadores UTF-8,
metadata textual e warnings de saída.

Clippy estrito passou nas duas libs e dois targets afetados, com
`--no-deps -D warnings`. O compilefail do cursor privado passou (1/0/0), além de
fmt, diff-check e links físicos. Evidência e histórico de tentativas permanecem
em `.build/rust-knowledge-reference-query/validation.json`.

Check01 foi bloqueado pela dependência zeroize coordenada; check02 terminou com
exit101/log zero durante falta de espaço. Data native01 preserva 5 PASS/1 FAIL
por path_bytes inválido na fixture, corrigido antes das execuções aprovadas.
Essas falhas não contam como aprovação e reruns não aumentam fixtures distintas.

Comandos focais:

```text
cargo test --offline --locked -p graphit-data --test knowledge_reference_query -j1
cargo test --offline --locked -p graphit-knowledge --test native_reference_query -j1
cargo clippy --offline --locked -p graphit-data --lib --test knowledge_reference_query -j1 --no-deps -- -D warnings
cargo clippy --offline --locked -p graphit-knowledge --lib --test native_reference_query -j1 --no-deps -- -D warnings
```

Esta folha não qualifica consulta ROOT/USER/importada, OwnRead de destinos,
FullNoLoss, Browser/IAB, wire App, plataformas não executadas ou migração global.

O armazenamento preserva schema físico FULL18, leitura até EOF, witnesses, namespace/estado, CAS, causas reais e limites individuais de identificadores, sem contabilidade agregada do corpus.
