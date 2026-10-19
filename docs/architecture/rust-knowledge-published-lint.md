---
type: architecture
title: Lint do snapshot Knowledge publicado em Rust
description: Diagnóstico integral do corpus publicado com metadados, referências, corpo curto e staleness observado, sem nova captura de fontes.
references:
  - target: {type: task, id: tsk-15ef}
    relation: implements
  - target: {type: task, id: tsk-edaf}
    relation: derived_from
  - target: {type: task, id: tsk-3a10}
    relation: composes
  - target: {type: task, id: tsk-55e8}
    relation: composes
  - target: {type: task, id: tsk-5de4}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-95fe}
    relation: distinguishes
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
---

# Lint do snapshot Knowledge publicado em Rust

Esta referência atende integradores e mantenedores da jornada PROJECT
`knowledge/lint`. O kernel `graphit_knowledge::published_lint` reúne diagnósticos
do corpus publicado completo: metadados efetivos, referências, corpo curto e
staleness persistido ou por idade explícita. Routing CLI/MCP, política de saída
do produto e avaliação das fontes atuais pertencem ao caller App.

## Jornada e fronteiras

1. O caller obtém admissão real `ReadKnowledge` e chama
   `read_native_knowledge_index` com o `SnapshotContext` original e limites Data.
   Data verifica schema, pin, integridade e EOF antes de entregar o corpus.
2. Tratar `KnowledgeIndexRead::NoCorpus` como ausência de publicação. Esse estado
   não equivale a um relatório limpo nem a uma publicação vazia.
3. Para `Published`, manter `generation` e `documents` vivos; passar a mesma
   requisição, corpus completo, política de idade e limites ao kernel.
4. Consumir o relatório ou o erro. `Published` com documentos vazios retorna
   `NoDocuments`. Erros não expõem relatórios parciais.
5. O caller decide como apresentar findings e tratar `issue_count`. O kernel
   não bloqueia publicação, escolhe exit code, captura fontes ou altera registros.

O relatório empresta a geração observada; ela não concede autoridade e não
constitui novo pin. Não há I/O, clock implícito, nova requisição ou admissão
nesta API. O corpus permanece intacto em sucesso e falha.

## API e resultados

```rust
pub fn lint_published_snapshot<'document, 'generation>(
    original: &SnapshotContext,
    generation: &'generation KnowledgeGeneration,
    documents: &'document [KnowledgeDocument],
    policy: PublishedLintPolicy,
    limits: PublishedLintLimits,
) -> Result<PublishedLintReport<'document, 'generation>, PublishedLintError>;
```

| Campo do relatório | Significado e ordem |
|---|---|
| `generation` | Referência à geração fornecida, com lifetime independente dos documentos |
| `total_pages` | Tamanho integral do corpus |
| `metadata` | Relatório real de `lint_published_metadata`; ordem de entrada e depois kind/title/description |
| `orphans` | Slugs próprios (`Vec<String>`), ordenados por UTF-8 pelo grafo |
| `broken_links` | Pares próprios `{source: String, target: String}`, ordenados por origem e destino |
| `short_pages` | `{slug: &str, word_count: usize}` em ordem de entrada; count completo entre 0 e 10 |
| `stale_pages` | `{slug: &str, reason}` em ordem de entrada; no máximo um motivo por página |
| `issue_count` | Soma checked de required metadata, órfãs, pares quebrados, páginas curtas e stale |

Findings recomendados de metadata consomem a capacidade de findings, mas não
incrementam `issue_count`. Uma página em categorias distintas contribui em cada
categoria. O mesmo destino ausente em duas origens produz dois pares e duas
issues. Data entrega os documentos em ordem UTF-8; a API não reordena inputs
arbitrários nem promete invariância da ordem de todo o relatório.

Referências são próprias porque o grafo empresta grupos temporários de targets.
Esses grupos são descartados antes do retorno. Metadata, short e stale
emprestam somente os documentos do caller; nenhum owner autorreferente é criado.

## Regras de diagnóstico

Metadados usam o [contrato de metadata publicado](rust-knowledge-published-metadata-lint.md):
kind efetivo vazio ou whitespace é obrigatório; title e description integral
nullable ausentes/vazios são recomendados. Custom kinds são aceitos. Summary é
preview editorial independente e não substitui description.

Referências reutilizam `ReferenceGraph` sobre o corpus completo e targets já
resolvidos pelo produtor. Self links não impedem uma página isolada de ser órfã;
um ciclo entre páginas impede isolamento. Edges duplicadas são deduplicadas
após os limites raw. Identificadores inválidos e slugs duplicados são erros
reais do produtor, não findings reparados ou ignorados.

O corpo vem de `parse_document` (P1), usando o body publicado integral e o
fallback title publicado. Só `parsed.source[parsed.envelope.body]` participa da
contagem `split_whitespace`. BOM e frontmatter YAML ficam fora. Markdown,
code fences, HTML e URLs contam como texto literal, sem renderização ou regex.
0–10 palavras é curto; ao encontrar a 11ª a página é longa e não retorna count.
YAML malformado permanece erro P1 mesmo que o texto visível pareça suficiente.

## Staleness do snapshot e clock

`PublishedLintPolicy` exige `evaluated_at: SystemTime` e
`max_age: Option<Duration>` sem default implícito.

| Condição | Resultado |
|---|---|
| `max_age = Some(Duration::ZERO)` | `InvalidAgePolicy` |
| `stale_since = Some(since)` | `Persisted { since }`, mesmo com idade desabilitada ou timestamp futuro |
| Sem marker; idade desabilitada | Nenhum finding de idade |
| Sem marker; updated_at ausente ou futuro | Nenhum finding de idade |
| Sem marker; elapsed igual à janela | Nenhum finding de idade |
| Sem marker; elapsed estritamente maior que a janela | `Age { updated_at }` com a data observada |

O marker prevalece sobre idade. A API não usa `stale_after` por página como
política, não fabrica data de expiração/since nem adiciona duração ao timestamp.
Ausência de finding não prova que fontes atuais estejam fresh.
`SourceChanged`, `SourceRemoved` e `Dependency` pertencem à
[avaliação de staleness com observações atuais](rust-knowledge-staleness-projection.md),
que requer corpus/observações completos e a fronteira de captura do caller.

## Limites e cancelamento

Todos os limites são explícitos `NonZeroUsize`; os três produtores mantêm seus
próprios limites (`DocumentParseLimits`, `ReferenceLimits`,
`PublishedMetadataLintLimits`). `ReferenceLimits` contém somente
`max_identifier_bytes`; orphans e broken_links recebem `max_results` por query.
O lint mantém seus limites próprios de entrada e findings, sem reimpor quotas
agregadas do grafo.

| Capacidade própria | O que conta |
|---|---|
| `max_pages` | Número de documentos |
| `max_input_bytes` | UTF-8 raw de slug/title/kind/description presente/body/fallback title e cada target, antes de trim/dedup |
| `max_findings` | Todos os findings: metadata obrigatória/recomendada, orphans, pares quebrados, short e stale |
| `max_reference_result_bytes` | UTF-8 copiado de orphans e ambas strings de cada par quebrado; origem repetida conta novamente |

Summary, os demais campos da recipe, source_path, digests e timestamps não entram
no orçamento de texto inspecionado; `fallback_title` entra conforme a tabela.
O preflight aplica os limites próprios de documentos e texto
inspecionado antes de criar grupos de targets; não reimpõe quota raw de edges do
grafo. Estatísticas reais do grafo permitem verificar a soma dos
findings já conhecidos antes de materializar as listas de referências.
Cópias e vetores próprios usam reservas falíveis; somas são checked.

O Original é checado antes de empty/policy/limites, por campo/target/página/token,
nas fronteiras dos produtores, durante cópias e antes do retorno. Cancelamento
e deadline preservam a causa IO real. Esses limites são lógicos e cooperativos:
não medem RSS nem preemptam chamadas síncronas internas dos SDKs ou varreduras
de strings. Não há teste que force OOM nem promessa de hard allocator quota.

## Erros e recuperação

| Erro | Ação do caller |
|---|---|
| `Request(io::Error)` | Encerrar a operação cancelada/expirada; preservar a causa |
| `NoDocuments` | Apresentar publicação vazia conforme a jornada, sem lint limpo sintético |
| `InvalidAgePolicy` | Corrigir a janela explícita para valor positivo ou `None` |
| `Limit` / `Overflow` | Reportar limite/contagem; não truncar o corpus para produzir sucesso |
| `Metadata` / `References` / `Parse` | Preservar o erro tipado real; corrigir dados ou orçamento na fronteira responsável |
| `Allocation` | Reportar falha de reserva; nenhum relatório parcial é retornado |

Precedência: Original → documentos vazios → zero age → preflight próprio →
metadata → referências → P1/body/stale. Dentro do trabalho prevalece a primeira
falha observada. `Error::source` expõe causas compostas reais, sem fabricar causa
para os erros próprios de domínio.

## Evidência e manutenção

Os resultados e receipts anteriores são históricos e permanecem intactos.
A mudança de limites em `tsk-c9e2` foi qualificada em Linux: 157 funções
da biblioteca e dos 12 targets afetados, Clippy estrito nos mesmos targets,
compilação das duas fixtures App e três doctests modificados passaram.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa as falhas iniciais de compilação, o replay de referências e a evidência
atual. Windows e macOS não foram executados.

Source: `crates/graphit-knowledge/src/published_lint.rs`. Fixtures públicas:
`crates/graphit-knowledge/tests/published_lint.rs`. A suíte
`native_index_publication.rs` integra o kernel após Auth/Data/EOF reais: corpus
a/z retorna cinco issues (um kind obrigatório, um par quebrado, duas páginas
curtas e um marker), enquanto PublishedEmpty retorna `NoDocuments`.

Qualificação focal Linux concluída: 10 testes públicos, 2 privados e 6 de
publicação nativa afetada passaram; Clippy com `-D warnings`, formatação focal
e diff também passaram. A revisão estática não encontrou falha material.
Cancelamento/deadline iniciais e word scan cancelado foram exercitados; não há
prova de cancelamento in-flight determinístico em SDK síncrono. Não implica
qualificação Windows/macOS, routing App ou conclusão da migração global.
O gate amplo e os manifests/lockfiles permanecem sob coordenação principal.

Comandos de regressão focal, a partir da raiz do projeto:

```text
cargo test --offline --locked -p graphit-knowledge --test published_lint --test native_index_publication -j1
cargo test --offline --locked -p graphit-knowledge --lib published_lint -j1
cargo clippy --offline --locked -p graphit-knowledge --lib --test published_lint --test native_index_publication -j1 -- -D warnings
```

Navegação: [metadata publicado](rust-knowledge-published-metadata-lint.md) ·
[documentos fonte e trechos](rust-knowledge-document-source.md) ·
[exportação Markdown](rust-knowledge-markdown-export.md).
