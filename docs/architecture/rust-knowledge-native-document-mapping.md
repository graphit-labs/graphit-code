---
type: architecture
title: Mapeamento nativo de documentos para Knowledge em Rust
description: Composição pura de documentos compilados, identidade lógica e destinos Markdown para a projeção Knowledge.
references:
  - target: {type: task, id: tsk-93e8}
    relation: implements
  - target: {type: task, id: tsk-c1a4}
    relation: derived_from
  - target: {type: task, id: tsk-41ba}
    relation: uses
  - target: {type: task, id: tsk-5530}
    relation: uses
  - target: {type: task, id: tsk-55e8}
    relation: uses
  - target: {type: task, id: tsk-a32d}
    relation: uses
  - target: {type: task, id: tsk-0225}
    relation: uses
  - target: {type: task, id: tsk-6dec}
    relation: refined_by
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
---

# Mapeamento nativo de documentos para Knowledge

Este contrato orienta consumidores Rust que já possuem documentos compilados
pela [factory de metadados](rust-knowledge-publication-metadata.md).
`graphit_knowledge::native_document_mapping` associa esses documentos a IDs e
origens lógicas explícitos, extrai destinos Markdown e compõe os campos da
[projeção Knowledge em Data](rust-knowledge-document-projection.md).

A API pura de [referências authored](rust-knowledge-authored-references.md)
admite o campo `references` com o kernel Records. Sua integração na projeção
persistida pertence a uma folha própria; o mapeamento atual não a publica.

A folha M1a é uma composição pura. Na qualificação original de `tsk-93e8`,
anterior à limpeza `tsk-6dec`, passaram em Linux 11 integrações com
FILE/P1/H/ReferenceGraph reais e o teste unitário do acumulador de overflow.
Esse acumulador e seu teste foram removidos junto com o ledger; o recibo
histórico permanece intacto.
Os dois doctests delimitados passaram: composição do exemplo e rejeição de fields que sobrevivam a views locais. Clippy estrito e rustfmt dos dois arquivos Rust passaram. Isso não qualifica
publicação M1b, outras plataformas ou a migração global de Knowledge/Wiki.

## Integrar um plano documental

Conserve o reader Core, os `CompiledDocument` reais e o mesmo
`Arc<SnapshotContext>` da captura/compilação durante o uso do produto.
O caller fornece um plano finito com uma entrada por documento:

```rust
pub struct AuthoredDocumentInput<'document, 'source> {
    pub compiled: &'document CompiledDocument<'source>,
    pub slug: &'document str,
    pub source_path: &'document str,
    pub updated_at: Option<SystemTime>,
    pub stale_since: Option<SystemTime>,
    pub stale_after: Option<Duration>,
}
```

`prepare_native_authored_mapping(request, inputs, parser_limits, mapping_limits,
fingerprint_limits, reference_limits)` devolve
`PreparedAuthoredMapping<'document, 'source, 'request>`.
Todos os limites são explícitos. A operação reaproveita o parser, fingerprint e
kernel de referências existentes e valida o plano completo antes de retornar.
`reference_limits` é `ReferenceLimits { max_identifier_bytes }`: o grafo não tem
quotas de páginas, edges raw ou texto agregado. Seus stats funcionais, validação
de identidade, normalização e destinos ausentes permanecem; listas consultadas
recebem `max_results` por query. Os limites próprios de parser, mapping,
fingerprint e H continuam independentes.

O produto empresta documentos/request e possui seus destinos `String` e digests
derivados. Não contém referências para suas próprias alocações.
`documents(request)` retorna o slice validado; cada documento oferece
`resolved_wiki_targets() -> &[String]`. Esses valores são dados ordinários,
e um destino resolvido pode estar ausente do catálogo.

Construa views externos desses grupos e mantenha seus owners vivos.
`authored_fields(request, &target_views)` exige correspondência exata do número
de grupos, conteúdos, ordem, duplicatas e vazios. Alterar ou omitir esses dados
retorna `InvalidTargetViews`.

O exemplo completo abaixo consome um mapping já preparado e observações Current
reais fornecidas pelo caller. Retorna apenas a contagem de headers; as rows são
consumidas enquanto mapping e views permanecem vivos.

```rust
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_document_projection::{
    self as projection, CompilationObservationFields,
    KnowledgeDocumentProjectionLimits, MemberRow,
};
use graphit_knowledge::native_document_mapping::PreparedAuthoredMapping;
use std::{error::Error, sync::Arc};
fn count_headers(
    request: &Arc<SnapshotContext>,
    mapped: &PreparedAuthoredMapping<'_, '_, '_>,
    current: &[CompilationObservationFields<'_>],
    limits: KnowledgeDocumentProjectionLimits,
) -> Result<usize, Box<dyn Error>> {
    let views: Vec<Vec<&str>> = mapped.documents(request)?.iter()
        .map(|document| document.resolved_wiki_targets().iter()
            .map(String::as_str).collect()).collect();
    let fields = mapped.authored_fields(request, &views)?;
    let prepared = projection::prepare(request, &fields, current, limits)?;
    prepared.supplied_context(request)?;
    Ok(prepared.rows(request)?.iter()
        .filter(|row| matches!(row, MemberRow::DocumentHeader(_))).count())
}
```

Para continuar utilizando a projeção, conserve compiled documents, mapping,
views, fields e contexto no mesmo escopo de uso. A documentação Rust de
`authored_fields` verifica a composição por compilação e rejeita o retorno de
fields que sobreviveriam a views locais. A captura, seleção e compilação têm
produtores próprios; este módulo não fabrica autoridade nem faz I/O.

## Identidade, corpo e estado

Slug e `source_path` são strings UTF-8 explícitas, comparadas por bytes exatos.
Não são derivados de filename físico. A fixture Unix aceita um filename não
UTF-8 com identificação lógica explícita, sem conversão lossy ou hash do nome.

Slug e suas colisões são validados pelo `ReferenceGraph` real: ID não vazio, sem
prefixo `..`, controles, barra ou backslash. Não há normalização, case folding,
aliases por título ou geração de sufixos.

A origem lógica `source_path` deve ser não vazia, sem `:`, backslash ou controles.
Componentes separados por barra devem ser não vazios e diferentes de `.` e
`..`. Assim, `a/b` é válido; `urn:a`, `/a`, `a/`, `a//b`, `a/./b` e
`a/../b` são inválidos. Origens duplicadas no plano são erro.
Essa gramática permite o consumo por C/staleness; não restringe bytes de nomes
físicos nem concede acesso a arquivos.

O body de H é `CompiledDocument.whole_source()` integral e emprestado: BOM,
frontmatter YAML, CRLF/LF, whitespace e referências tipadas authored permanecem.
A janela `envelope.body` do
[parser](rust-knowledge-document-parser.md) orienta somente os eventos Markdown.
O mapping não reescreve labels, links ou conteúdo.

Título, kind, summary, description e `summary_truncated` vêm dos getters P1,
sem recomputar metadados ou aplicar defaults. Description mantém `None` versus
`Some("")`. Recipe `authored-document-v1` conserva a policy efetiva e converte o
summary limit `u64` para `NonZeroUsize` de forma checked.

O digest físico observado em P1 compõe `PhysicalSourceDigest32`; o
[fingerprint](rust-knowledge-compilation-fingerprint.md) compõe
`CompilationDigest32`. Não são intercambiáveis, e copiar seus bytes não prova
publicação. O fingerprint não é checksum de catálogo, targets ou geração.

`updated_at`, `stale_since` e `stale_after` são valores explícitos do caller.
O mapping não infere mtime, consulta clock ou limpa staleness. H valida a policy:
`stale_after`, quando presente, deve ser positivo e exige `updated_at`.
Observações Current e avaliação de staleness pertencem a seus consumidores;
ausência de observação não é uma remoção inventada.

## Destinos Markdown

P1 não conserva a arena de eventos. O mapping reaplica o parser existente ao
source emprestado e consome apenas `Start(Tag::Link)` reais, em ordem.
Imagens, código inline/fenced e texto YAML não geram edges por análise textual.

A contagem raw de eventos, links e URIs ocorre antes da classificação e do
percent decoding, inclusive para links externos.

| Entrada | Resultado |
|---|---|
| WikiLink local | ID literal; `.md` permanece parte do ID. |
| Href Markdown terminado em `.md` | Remove exatamente um sufixo literal, case-sensitive. |
| Href local sem extensão | ID lógico local. |
| Outra extensão, scheme, barra ou backslash | Fonte/proveniência; não produz target local. |
| Query ou fragment literais | Separados antes de percent decoding. |
| Somente query ou fragment | Self reference para o slug atual. |
| Href originalmente vazio | Target vazio; conta raw. |
| Href `.md` | `InvalidTarget`: entrada não vazia derivou ID vazio. |

`percent-encoding` 2.3.2 realiza `decode_utf8()` e conserva a causa `Utf8Error`.
Escapes malformados seguem a interpretação literal do SDK, sem gramática
paralela ou conversão lossy. `b.md#part` produz `b`; `a%23b.md` produz `a#b`;
`[[id.md]]` conserva `id.md`. Destino conhecido não muda a classificação.

Os grupos conservam raw order, duplicatas, self, vazios e dangling targets.
H recebe esses mesmos grupos; o kernel de referências admite os counts raw e
normaliza seu grafo conforme seu contrato. Destinos ausentes ficam disponíveis
para diagnóstico de links quebrados.

## Limites e falhas

`NativeAuthoredMappingLimits` tem três campos positivos, sem defaults:

| Campo | Admissão |
|---|---|
| `max_field_bytes` | Cada campo textual, incluindo body e policy. |
| `max_uri_bytes` | Uma URI raw. |
| `max_target_bytes` | Um target local decodificado. |

Igualdade é admitida para cada campo, URI e target. A URI raw é verificada mesmo
quando externa; o target local é verificado após percent-decoding e antes de
inserção no grupo. Esse check não limita a alocação anterior do decoder.

Não há contador ou saldo global de documentos, bytes, eventos, links ou somas de
URI/target no mapping. O parser limita eventos e links por documento, e o
fingerprint limita a preimagem realmente serializada por compilado. Referências
e H preservam seus contratos independentes; unicidade de slug/origem,
completude do plano e request original continuam sendo verificações próprias.
Reservas controladas de Vec/String são fallíveis; limites lógicos não prometem
hard RSS, recuperação universal de OOM ou preempção de alocações dos SDKs.

O preparo e os getters verificam a request original, inclusive no plano vazio.
Nos getters, o contexto conservado é verificado antes de comparar um contexto
estrangeiro. Cancelamento e expiração mantêm `Interrupted` e `TimedOut`; um
contexto novo não reativa o produto.

| Erro | Recuperação |
|---|---|
| `InvalidOrigin` / `DuplicateOrigin` | Corrigir o plano lógico explícito. |
| `References` | Inspecionar a causa real de ID, colisão ou limite do kernel. |
| `InvalidTarget` / `PercentUtf8` | Corrigir a sintaxe authored e recompilar pelo produtor real. |
| `InvalidTargetViews` | Reconstruir views dos grupos originais, sem editar conteúdo/ordem. |
| `InvalidSummaryLimit` | Resolver a policy representável, sem clamp. |
| `LimitExceeded` | Corrigir o campo, URI ou target que excede seu limite individual. |
| `Parser` / `Fingerprint` | Inspecionar a causa preservada do produtor. |
| `Request` / `DifferentRequest` | Respeitar prazo/cancelamento e binding original. |
| `Allocation` | Tratar falha de reserva e descartar o preparo incompleto. |

`Error::source()` conserva I/O, parser, fingerprint, referências, UTF-8 e reserva.
Diagnóstico não deve registrar body integral ou classificar erros por mensagens.
Uma falha não devolve mapping ou fields parciais; não há retry automático.
Nova aquisição/compilação usa os produtores reais e não substitui o contexto de
um documento cancelado.

## Verificação e fronteira M1b

Os resultados e receipts anteriores são históricos e permanecem intactos.
A mudança de limites em `tsk-c9e2` foi qualificada em Linux: 157 funções
da biblioteca e dos 12 targets afetados, Clippy estrito nos mesmos targets,
compilação das duas fixtures App e três doctests modificados passaram.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa as falhas iniciais de compilação, o replay de referências e a evidência
atual. Windows e macOS não foram executados.

Source: `crates/graphit-knowledge/src/native_document_mapping.rs`.
Fixtures: `crates/graphit-knowledge/tests/native_document_mapping.rs`.
A suíte cobre corpo/metadados/digests/borrows, identidade e nome físico,
classificação e targets raw, três caps individuais igualdade+1, limites do
parser por documento e causas dos produtores,
H temporal, views adulterados e requests canceladas/expiradas/estrangeiras.
Deadline usa espera bounded até o `Instant` original imutável, sem clock fake.

Invocações atuais delimitadas, na janela coordenada:

```text
cargo test --offline --locked -p graphit-knowledge --test native_document_mapping -j4
cargo clippy --offline --locked -p graphit-knowledge --lib --test native_document_mapping --no-deps -j4 -- -D warnings
rustfmt --edition 2024 --check --config skip_children=true crates/graphit-knowledge/src/native_document_mapping.rs crates/graphit-knowledge/tests/native_document_mapping.rs
```

A limpeza `tsk-6dec` executou as regressões Knowledge afetadas: 98 funções
passaram, incluindo 11 deste target. O caso do parser foi reforçado e passou
isoladamente, sem criar outra função: dois documentos admitem limites iguais
por documento, e o excesso de eventos ou links conserva a causa do parser.
O Clippy dos targets afetados passou com `-D warnings`. Resultados e comandos
estão em `.build/rust-knowledge-mapping-ledger-cleanup/validation-current.json`.
Essa qualificação não entrega streaming nem reduz a retenção atual dos corpos.
Os dois doctests citados acima pertencem à qualificação histórica; não foram
reexecutados nesta limpeza. Windows/macOS também não foram executados.
Gate amplo de integração pertence ao coordenador.

M1b conserva publisher/caller Data D3, schema persistido, Arrow/SDK, referências
tipadas semânticas, Auth e Core birth com seus owners. Preservar referências no
body não valida sua semântica; headers H não provam commit, first publication,
reopen ou integridade de dataset persistido. Corpus walk, busca, lint,
exportação/importação e lifecycle global permanecem em suas folhas.
Consulte as [fronteiras Knowledge/Data/Records](rust-knowledge-data-boundaries.md).

A composição da primeira publicação do conjunto já compilado é documentada em
[primeira publicação nativa Knowledge](rust-knowledge-native-publication.md).
O consumidor atual acompanha a disponibilidade Linux do publisher Data; sua
qualificação e a integração Main são registradas separadamente em `tsk-8b24`.
