---
title: Fonte virtual nativa de documentos Knowledge
doc_type: architecture
references:
  - target: {type: task, id: tsk-5de4}
    relation: implemented_by
  - target: {type: task, id: tsk-b210}
    relation: specified_by
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8218}
    relation: uses
  - target: {type: task, id: tsk-8b24}
    relation: integrates
  - target: {type: task, id: tsk-7d1e}
    relation: uses
  - target: {type: task, id: tsk-cab0}
    relation: implements
---

# Fonte virtual nativa de documentos Knowledge

Integradores de leitura documental podem usar
`graphit_knowledge::document_source::render_document_source` para inspecionar
os metadados e o corpo de um `KnowledgeDocument` completo já lido pelo Data.
O resultado é uma fonte virtual: cabeçalho YAML nativo, uma linha vazia e o
corpo integral armazenado. A seleção de linhas e padrões usa o Core existente.
Esta página descreve a API da biblioteca; a ligação aos comandos CLI/MCP é uma
entrega de seus respectivos chamadores.

```rust,ignore
pub fn render_document_source(
    original: &SnapshotContext,
    document: &KnowledgeDocument,
    selection: &graphit_core::text_slice::Request,
    limits: DocumentSourceLimits,
) -> Result<graphit_core::text_slice::Result, DocumentSourceError>
```

## Leitura, identidade e dados

Para obter uma página, o chamador usa `read_native_knowledge_page` com o slug
lógico exato. O Data filtra os registros desse slug no pin autorizado e valida
seu stream até EOF, incluindo duplicidade, namespace, estado e witness. O
resultado `KnowledgeSnapshotPage::Found { document, focused_source }` traz o
corpo integral e a proveniência do mesmo registro; não consulta todo o grafo de
referências para compor metadata. Um documento sem projeção de referências
conserva `focused_source: None`, sem namespace fabricado.

O get não valida nem retém corpos alheios: um erro num documento não selecionado
não invalida a página solicitada. Essa regra substitui a exigência anterior de
integridade global num get. A leitura completa por `read_native_knowledge_index`
continua servindo aos consumidores que precisam do corpus integral, como
exportação. O renderer recebe o documento já selecionado, sem IO adicional.
Veja a [publicação nativa](rust-knowledge-native-publication.md) e o
[mapping documental](rust-knowledge-native-document-mapping.md).

`NoCorpus`, corpus publicado vazio e slug ausente são estados da leitura e do
lookup no chamador. Documento encontrado com corpo vazio continua encontrado:
sua fonte contém o cabeçalho. Uma seleção por padrão sem matches retorna sucesso
com `source` e `matches` vazios, preservando `total_lines`. Falhas de leitura,
permissão, cancelamento, prazo, formato ou limites continuam erros.

O renderer empresta o documento e o contexto durante a chamada. Ele não faz IO,
admissão Auth, pin de armazenamento, busca de arquivo físico ou revalidação de
origem. A geração é observação do Data e permanece com o chamador. Não há aliases,
remoção de extensão, fallback de título, relógio ou metadados de Memory/Task
fabricados a partir de documentos PROJECT.

## Cabeçalho e corpo

O cabeçalho projeta os quinze campos de metadata do FULL16 atual. O corpo fica
fora do YAML novo.

| Campo público | Valor |
|---|---|
| `id`, `title`, `type` | Slug, título e kind literais; título/type vazios permanecem vazios |
| `summary`, `description`, `summary_truncated` | Valores armazenados; description ausente é `null`, string vazia é `''` |
| `source_path` | Origem lógica fornecida pelo produtor |
| `physical_digest` | Digest físico de 32 bytes em hex minúsculo ou `null` |
| `compilation_digest` | Digest de compilação de 32 bytes em hex minúsculo, separado do físico |
| `recipe` | Identificador da receita armazenada |
| `recipe_parameters` | `fallback_title`, `kind_on_absent`, `max_summary_bytes` armazenados |
| `updated_at`, `stale_since` | RFC3339 UTC com nove casas de nanos ou `null` |
| `stale_after_ns` | Intervalo em nanos unsigned ou `null` |
| `resolved_wiki_targets` | Array de targets armazenados, conservando ordem, duplicatas, self e dangling |

Os timestamps usam `chrono`; `-17ns` em relação ao epoch é
`1969-12-31T23:59:59.999999983Z`. Timestamps fora do intervalo de nanos `i64`,
intervalos fora de nanos `u64` e parâmetros que não cabem na representação
retornam `OutOfRange`, sem data ou valor substituto. O renderer exibe as
observações de staleness armazenadas; não avalia a atualidade dos arquivos.

O serializer é `serde-saphyr = 1.3.0`, com `serialize` ativado explicitamente.
Todas as strings são quoted, block scalars e diretivas YAML estão desativados.
Títulos com dois pontos, aspas, quebras de linha e texto parecido com delimitador
continuam strings válidas ao roundtrip.

O framing é `---\n` + YAML terminado em LF + `---\n\n` + body. O corpo conserva
seus bytes UTF-8, inclusive BOM, CRLF, Markdown e um eventual frontmatter
original. Esse envelope original é parte do corpo e não um segundo cabeçalho
gerado. Para ler os metadados, faça parse somente do primeiro header novo.

## Seleção textual

`Request::default()` retorna toda a fonte virtual. Os parâmetros opcionais
seguem `graphit_core::text_slice::apply`: range primeiro, depois head, tail e
pattern. Por exemplo, `Request { head: 20, ..Default::default() }` seleciona as
primeiras vinte linhas da fonte virtual, incluindo o cabeçalho. Para procurar
texto com contexto, use `Request { pattern: "installation".into(), before: 2,
after: 2, ..Default::default() }`.

O padrão literal é case-insensitive; com `is_regex: true`, vale a expressão
Rust regex fornecida. Range inválido, parâmetros negativos e regex inválida
retornam `Selection` com o erro do Core. Contextos sobrepostos são mesclados,
`Match.line_number` é absoluto na fonte virtual e `line_numbers` usa a formatação
do Core. LF final conta uma linha final vazia. O resultado é o próprio
`text_slice::Result`, com `source`, `total_lines`, range e `matches`.

O preview editorial é a API separada de
[compilação e leitura documental](rust-knowledge-wiki.md); quando necessário,
o chamador passa `document.body` e `document.summary` para `preview_markdown`.
Fonte virtual não reescreve o Markdown para exportação e não implementa ranking.

## Limites, falhas e recuperação

Todos os limites de `DocumentSourceLimits` são positivos (`NonZeroUsize`).

| Limite | O que conta e quando é verificado |
|---|---|
| `max_input_bytes` | Soma body, metadata textual, parâmetros, targets e pattern, antes do serializer |
| `max_targets` | Número de targets armazenados antes de iterar ou serializar |
| `max_virtual_bytes` | Fonte completa, incluindo escapes YAML e framing, antes de cada cópia no buffer privado |
| `max_virtual_lines` | Linhas da fonte completa antes da seleção Core |
| `max_output_bytes` | Bytes de `Result.source` mais todas as cópias `Match.line`, depois do Core |
| `max_matches` | Todas as linhas de `Result.matches`, inclusive contexto, depois do Core |

O uso exato é aceito; um byte, linha ou entrada acima do limite falha. Aritmética
de tamanho usa checked operations. O buffer permanece privado até o resultado
completo; um erro não retorna o prefixo acumulado.

`SnapshotContext::check` conserva cancelamento e deadline do request original.
As escritas, mesmo vazias, verificam esse contexto; `write_all` é single-attempt
e não repete `Interrupted`. Uma falha terminal do writer conserva a primeira
causa. Há verificações antes do trabalho e depois de serializer e Core. Esses
limites são lógicos: SDK e Core executam trechos síncronos, e a verificação do
resultado após Core não impede suas alocações nem garante preempção ou RSS.

`Request` conserva o `io::Error` causal, inclusive `Interrupted`/`TimedOut`;
`Serialization` conserva `serde_saphyr::SerializeError`. `Limit`, `Overflow`,
`OutOfRange`, `Allocation`, `Encoding` e `Selection` identificam as demais
falhas. Erro já ocorrido no SDK/Core não é substituído por cancelamento posterior.
Cancelamento ou deadline pede um novo request pelo chamador; um limite exige
reduzir a seleção/entrada ou ajustar sua política explícita. Erro de metadata
exige conferir o produtor; o renderer não corrige silenciosamente o documento.

## Evidência e validação focal

Fontes: `crates/graphit-knowledge/src/document_source.rs`,
`crates/graphit-data/src/knowledge_index.rs` e
`crates/graphit-core/src/text_slice.rs`. Os testes de
`crates/graphit-knowledge/tests/document_source.rs` cobrem roundtrip, framing,
seleção, limites exatos/um abaixo, erros, cancelamento e deadline. Os testes
privados do módulo verificam overflow e a primeira falha do writer com o SDK
real. O fixture existente `native_index_publication` exercita
captura FILE → compilação → publicação Knowledge/Data → FULL16 → fonte virtual.

Comandos focais, sem repetir o gate amplo:

```text
cargo test --offline --locked -p graphit-knowledge --lib document_source -j1
cargo test --offline --locked -p graphit-knowledge --test document_source --test native_index_publication -j1
cargo clippy --offline --locked -p graphit-knowledge --lib --test document_source --test native_index_publication -j1 -- -D warnings
```

Os dois targets de integração foram executados em Linux: seis casos do renderer
puro e seis da publicação nativa real passaram. O filtro de lib executou dois
casos privados de overflow e writer com SDK; também passaram. A qualificação
registrada em Task identifica os comandos efetivos. O Clippy estrito focal,
com `-D warnings`, a formatação dos arquivos próprios e o diffcheck também
passaram. Outras plataformas não foram executadas nesta qualificação.
A API de leitura Source não encerra geração por domínio, busca, lint, exportação
ou lifecycle e não declara a validação ponta a ponta concluída.

## Exportação Markdown nativa

A [exportação Markdown](rust-knowledge-markdown-export.md) reutiliza o mesmo
serializer privado de metadata15 e entrega um plano Core com índice e navegação.
Sua seleção de corpo é a janela P1 sem BOM/frontmatter; esta API de fonte virtual
continua retornando o corpo integral armazenado, incluindo esses bytes.
