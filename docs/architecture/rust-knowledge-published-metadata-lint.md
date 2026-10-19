---
type: architecture
title: Diagnóstico de metadados publicados Knowledge em Rust
description: Conformidade de kind, title e description integral no corpus FULL16, com limites e erros causais.
references:
  - target: {type: task, id: tsk-3a10}
    relation: implements
  - target: {type: task, id: tsk-0550}
    relation: derived_from
  - target: {type: task, id: tsk-8b24}
    relation: uses
  - target: {type: task, id: tsk-8218}
    relation: uses
  - target: {type: memory, id: 01M4MT1HZN2BNAKXPG6C81X5ZE}
    relation: supports
---

# Diagnóstico de metadados publicados Knowledge em Rust

Esta referência atende quem integra a etapa de metadados da jornada PROJECT
`knowledge/lint`. O módulo `graphit_knowledge::published_metadata_lint` examina
os valores efetivos de `KnowledgeDocument` após uma leitura Data completa.
A qualificação focal Linux passou. Routing CLI/MCP e lint completo permanecem
pendentes: o módulo entrega somente o diagnóstico publicado de metadados.

## Entrada e sequência do consumidor

O consumidor usa `read_native_knowledge_index` com a admissão real
`ReadKnowledge` e o `SnapshotContext` original. Mantém o proprietário de
`documents` e a `generation` resultantes até consumir o relatório. A leitura
Data confirma EOF, schema, pin e integridade antes de retornar documentos;
esta etapa não cria admissão, contexto, snapshot ou outro leitor.

1. Tratar `KnowledgeIndexRead::NoCorpus` no caller, sem convertê-lo em lint limpo.
2. Para `Published`, passar o corpus completo e o original a
   `lint_published_metadata`. `Published` com zero documentos é um estado válido
   de storage, mas produz `NoDocuments` para este diagnóstico.
3. Em sucesso, conservar os findings de qualidade e sua geração de origem.
   Em erro, propagar a causa; não apresentar um relatório parcial ou limpo.

Qualidade não rejeita publicação: campos recomendados não incrementam
`required_errors`. A decisão de apresentação pertence ao consumidor App.
Esta biblioteca não entrega novo comando PROJECT nem política de exit status.

## Campos e regras

| Campo publicado | Condição | Finding | Incremento de required_errors |
| --- | --- | --- | --- |
| kind | `trim().is_empty()` | Kind / Required | 1 por página |
| title | `trim().is_empty()` | Title / Recommended | 0 |
| description | None ou texto cujo trim é vazio | Description / Recommended | 0 |

O trim é o `str::trim` do Rust, usado apenas no predicado. O diagnóstico não
altera texto, não classifica kinds custom, não preenche defaults e não exige
que title/kind sejam authored: valores derivados de policy são efetivos.

`description` é o valor nullable integral preservado por
[P1](rust-knowledge-publication-metadata.md) e
[H](rust-knowledge-document-projection.md). `summary` é uma projeção
editorial que pode vir do primeiro parágrafo e sofrer truncamento UTF-8;
ela não é lida nem usada como fallback nesta etapa.

| Description publicada | Summary editorial | Resultado desta regra |
| --- | --- | --- |
| None | parágrafo preenchido | Description / Recommended |
| Some("") ou Some("   ") | parágrafo preenchido | Description / Recommended |
| Some("  α\nβ  ") | vazio por limite de preview | Nenhum finding de description |

None não revela se o autor omitiu o campo ou escreveu null. O FULL16 não contém
essa provenance de P1. Conformidade authored completa precisa de
`CompiledDocument::provenance()` e permanece uma jornada separada. Esta etapa
também não entrega audit de compilação, contagem de palavras, staleness ou
diagnóstico de referências. O digest de compilação tipado tem 32 bytes;
nenhum guard de hash string ausente é portado.

## API e ownership

```rust
pub fn lint_published_metadata<'document>(
    original: &SnapshotContext,
    documents: &'document [KnowledgeDocument],
    limits: PublishedMetadataLintLimits,
) -> Result<PublishedMetadataLintReport<'document>, PublishedMetadataLintError>
```

O relatório contém `total_pages`, `required_errors` e
`Vec<PublishedMetadataFinding<'document>>`. Cada finding contém `slug: &str`,
`field: PublishedMetadataField::{Kind, Title, Description}` e
`severity: PublishedMetadataSeverity::{Required, Recommended}`.
Somente a Vec é proprietária; slugs são emprestados sem cópia ou normalização.
Não há DTO paralelo de documentos, IO ou mutação do corpus.

A ordem acompanha as páginas de entrada; dentro de cada página segue
Kind → Title → Description. O reader Data entrega ordem UTF-8 de slug;
o kernel não ordena novamente nem duplica sua validação de identidade.
O caller mantém a geração separadamente; o relatório não transforma uma
observação de versão em autoridade.

## Capacidades, cancelamento e erros

`PublishedMetadataLintLimits` exige três `NonZeroUsize`:

| Capacidade | Unidade e fronteira |
| --- | --- |
| max_pages | número de documentos, antes do processamento |
| max_input_bytes | soma de bytes UTF-8 de slug/title/kind/description presente, antes do trim e dos findings |
| max_findings | findings retornados, antes de cada reserva/push |

Body, summary, digests, recipe, timestamps e targets têm seus próprios budgets
Data/produtores e não entram nesta capacidade de campos examinados. Um limite
exato passa; uma unidade abaixo falha. Somatórios usam aritmética checked;
a Vec usa `try_reserve`. As capacidades são lógicas, não garantias de RSS,
headroom, tempo ou tamanho físico das reservas do allocator.

O original é verificado antes de qualquer decisão, por campo, antes/depois do
trim, após reserva e no retorno final. Cancelamento é cooperativo nessas
fronteiras; não há preempção dentro do trim da biblioteca padrão. Contexto já
cancelado ou deadline vencido precede corpus vazio ou violação de capacidade.
Uma falha já produzida conserva sua causa, sem reinterpretá-la como sucesso.

| Erro | Interpretação e tratamento |
| --- | --- |
| Request(io::Error) | causa original conservada, incluindo Interrupted/TimedOut; propagar sem relatório |
| NoDocuments | corpus publicado vazio; recuperar pela jornada de indexação |
| LimitExceeded(nome) | capacidade pages/input bytes/findings excedida; revisar capacidade do caller |
| Overflow(nome) | soma não representável; rejeitar a operação |
| Allocation(TryReserveError) | falha de reserva com `Error::source`; rejeitar sem prefixo público |

## Evidência e limites da qualificação

Os testes focais em `tests/published_metadata_lint.rs` isolam cada campo,
Unicode whitespace, kinds custom, ordem/contagem, descrição integral distinta
do preview, capacidade exata/abaixo, corpus vazio e causas originais de
cancelamento/deadline. O teste privado protege overflow distinto de limite.
O target `native_index_publication` acrescenta a composição
Core/P1 → M1a/H → Data FULL16/ReadKnowledge real → diagnóstico, incluindo
description null/vazia com summary presente e publicação vazia.

Invocação focal, idêntica em Windows/macOS/Linux:

```text
cargo test --offline --locked -p graphit-knowledge --test published_metadata_lint --test native_index_publication -j1
```

Qualificação executada em Linux: seis testes do diagnóstico e seis do bridge
real passaram (0,00s e 3,00s de runtime; 4m51s de build incluindo fila do cache).
O filtro privado `cargo test --offline --locked -p graphit-knowledge --lib
published_metadata_lint -j1` passou um teste de overflow, com 48 não relacionados
filtrados. Clippy focal `--lib --test published_metadata_lint --test
native_index_publication -j1 -- -D warnings` passou; rustfmt dos três arquivos
Rust afetados e diff check passaram. As execuções usaram o cache compartilhado
existente, uma execução por etapa, perfis debug0/incremental0 e -j1, na janela
cedida pelo principal. Receipts locais: `.build/knowledge-published-metadata-lint-tests.log`,
`.build/knowledge-published-metadata-lint-unit.log` e
`.build/knowledge-published-metadata-lint-clippy.log`.

O bridge de publicação possui cfg Linux; Windows/macOS seguem UNRUN. Não
foi forçada uma falha de allocator nem executado routing App; o relatório
conserva TryReserveError quando a reserva real falha. O gate amplo e seu runner
pertencem ao principal, sem repetição por folha. Não houve dependência ou
lockfile novo. Esta qualificação não conclui a migração global.

Veja [publicação nativa](rust-knowledge-native-publication.md) e
[fronteiras Knowledge/Wiki](rust-knowledge-wiki.md).

## Lint integral do snapshot publicado

A etapa de metadata acima mantém seu contrato específico. A composição do
corpus publicado está documentada em [Lint do snapshot Knowledge publicado em
Rust](rust-knowledge-published-lint.md), reunindo referências, corpo curto e
staleness persistido/idade explícita. Routing CLI/MCP e avaliação de fontes
atuais continuam na fronteira App; essa composição não prova freshness atual.
