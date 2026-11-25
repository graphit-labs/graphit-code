---
title: Exportação Knowledge em OKF e Obsidian
doc_type: architecture
description: Perfis textuais com proveniência observada, metadata nativa e navegação gerada.
references:
  - target: {type: task, id: tsk-69f2}
    relation: implements
  - target: {type: task, id: tsk-9d92}
    relation: consumer_boundary
  - target: {type: task, id: tsk-45ca}
    relation: derived_from
---

# Exportação Knowledge em OKF e Obsidian

Os perfis OKF e Obsidian preparam uma árvore de documentos Markdown com
frontmatter OKF 0.2. A extensão `graphit` conserva a metadata nativa observada.
O [perfil Markdown](rust-knowledge-markdown-export.md) mantém seu contrato próprio.

## Pré-requisitos e API

O caller conserva a captura Source-only real, geração, Original e admissões
até EOF, revalidação e publicação. Imported usa seu reader/pin e guards próprios.
O estágio recebe documentos ordinary, sem conceder acesso ou atestar EOF.
A Brand vem da Installation efetivamente capturada pelo caller, sem input de
actor/brand na tool ou CLI.

O módulo `graphit_knowledge::okf_export` define:

```rust
#[derive(Clone, Debug)]
pub enum KnowledgeTextExportFormat {
    Markdown,
    Okf { brand: graphit_core::brand::Brand },
    Obsidian { brand: graphit_core::brand::Brand },
}
```

Existe um único construtor:

```rust,ignore
MarkdownExportStage::new_in(original, output_parent, modes, limits, format)?;
```

`push(original, KnowledgeDocument)`, `finish(original)`, `MarkdownExportPlan` e `close` mantêm os contratos do estágio.
O caller drena Source até `None`; `Some([])` não é EOF. Qualquer erro de cursor
ou `push` interrompe a operação. A seleção pública `native|markdown|okf|obsidian`
está sob integração App/Main e tem check produtivo próprio.

## Jornada e custódia

1. Validar o destino efetivo com os guards existentes antes de staging.
2. Criar o estágio sob o parent existente, com formato e Brand capturada.
3. Entregar cada documento sob o mesmo Original; somente após EOF chamar `finish`.
4. Manter o plano durante revalidação e publicação; observar o resultado de `close`.

O TempDir é privado ao parent. O `Value` transitório contém metadata de um FILE,
sem corpo. Markdown e OKF copiam o corpo autoral como sufixo literal. No perfil
Obsidian, `push` admite e spoola o corpo raw sob `document.max_document_bytes`,
após o prefixo de cabeçalho. Depois do EOF válido e do roster completo, `finish`
faz uma única análise P1 por FILE no exportador para produzir o corpo final.
Input e eventos são descartados após aquele FILE; o mapa global conserva só
título, targets, prefixo e fallback_title. Isso não promete uma única leitura
ou análise na jornada inteira.

Leitura em chunks, verificações do Original e reservas falíveis mantêm os
limites reais. A árvore tem `index.md` e `pages/p-<SHA256 do slug literal>.md`;
nenhum slug admitido vira pathname direto.

A [publicação autenticada](rust-knowledge-markdown-publication.md) reutiliza
revalidação Core, rename exclusivo, NOREPLACE, efeitos conhecidos e cleanup.
O SDK não publica o destino nem reconstrói latest, pins ou admissões.

## Metadata e proveniência

Somente o índice raiz tem frontmatter `okf_version: "0.2"`. Páginas recebem:

| Campo | Valor observado |
| --- | --- |
| `type` | kind literal não vazio e representável |
| `id`, `title` | slug e título publicados |
| `description` | descrição integral quando presente, distinta de summary |
| `generated.by` | `process:<brand>-knowledge-wiki`, o exportador real |
| `generated.at` | updated_at pelo codec temporal nativo; ausente quando None |
| `sources` | mappings `{resource: source_path}` quando a string não é vazia |
| `graphit` | metadata nativa integral de 15 campos |

`source_path` é literal, incluindo whitespace; o exporter não resolve esse valor
como pathname de saída. String vazia omite `sources`, preservando-a em `graphit`.
`generated.by` não afirma autoria humana, verificação ou identidade anterior.
Nenhum relógio preenche datas ausentes.

A extensão conserva id/title/type (campo Rust kind), summary/description/summary_truncated,
source_path, physical_digest/compilation_digest, recipe/recipe_parameters,
updated_at/stale_since/stale_after_ns e resolved_wiki_targets. None e vazio,
precisão temporal e digests mantêm suas distinções nativas. A duração
`stale_after_ns` não ocupa a key OKF `stale_after`, cujo significado é instante
absoluto. Status, verified, revision e history não são fabricados.

A [especificação primária OKF 0.2](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md)
permite extensões e campos opcionais de proveniência; o profile segue seus
contratos de conceito, raiz e referências. O id lógico Graphit é uma extensão;
a identidade de conceito OKF continua sendo o caminho físico no bundle.

## Navegação e corpo Obsidian

Índice, referências e backlinks gerados apontam a arquivos físicos reais pelo
ReferenceGraph/C8. Destinos ausentes aparecem como texto missing, sem href.
Obsidian suporta [links internos](https://help.obsidian.md/links).

Markdown e OKF conservam literalmente a janela de corpo definida pelo parser P1.
Obsidian usa somente eventos `WikiLink` e offsets reais desse parser maduro.
O destino raw deve coincidir com o token reconhecido. Sua identidade canônica
vem da mesma `local_target` usada pelo produtor de `resolved_wiki_targets`:
o exportador compartilha essa função intra-crate sem mudar sua semântica.
O produtor exclui caminhos/esquemas e normaliza sufixos `?/#` e percent-encoding
UTF-8. O canônico precisa constar nas referências daquele FILE e no roster real;
só então vira `p-<SHA256 do canônico>.md`. Fragmento e sufixos raw são conservados.
Por exemplo, `name%20space` pode apontar ao arquivo seguro do ID observado
`name space`, sem inferência de alias ou nova regra de resolução.

O texto visível original também é conservado. Com `has_pothole=false`, o
exportador materializa a label original: `[[log]]` torna-se
`[[p-<hash>.md|log]]`; `[[log#Heading]]` torna-se
`[[p-<hash>.md#Heading|log#Heading]]`. Com `has_pothole=true`, a label explícita
fica byte a byte. Os demais bytes, delimitadores e Unicode/CRLF ficam intactos,
exceto pelo destino e pela materialização necessária do display implícito.

Não se inferem aliases físicos, namespaces ou equivalência entre ID e filename.
`None` do normalizador, alvo não declarado ou ausente no roster preservam o token.
Código inline/fenced e HTML ficam literais. Links Markdown comuns e embeds
ficam literais; não há lexer Markdown paralelo ou regex de descoberta de links.
Falha real do normalizador, como percent-encoding UTF-8 inválido, é erro causal
e não ausência legítima de referência.

## Erros, limites e verificação

Kind vazio/whitespace retorna `InvalidOkfMetadata("type")`. O kind exato
`Attested Computation` também é recusado: FULL18 não fornece a família condicional
de runtime/execução exigida pelo formato. Native e Markdown continuam aptos a
representar esse conteúdo. A recusa precede o spool daquela página; o caller
propaga o erro e descarta qualquer spool anterior.

`Metadata(serde_json::Error)` conserva a causa concreta de serialização;
`InvalidOkfMetadata(&'static str)` identifica metadata indisponível sem forjar
causa interna. `Target(Box<NativeAuthoredMappingError>)` preserva a causa do normalizador existente,
com slug e ordinal real do FILE exportado. Request/parser/referências/comunidades/Payload e limites mantêm
seus erros existentes. Original distinto, cancelamento, deadline e falhas
terminais não certificam uma exportação parcial saudável. Drop é best effort;
`close` devolve o IO de cleanup real.

`max_file_bytes` conta a saída final: YAML, escapes, navegação, separadores e
corpo renderizado, inclusive expansão do filename e da label. No Obsidian,
frontmatter raw descartado não conta como saída, mas permanece sujeito aos
limites de input P1. Limites YAML/eventos/links são verificados no `finish`;
qualquer falha não entrega plano parcial. Não há quota global nova ou promessa
sobre RSS.

As fixtures usam arquivos, YAML e parser reais: metadata/ausência/datas,
nomes seguros e determinismo, bodylinks físicos, labels e fragmentos,
percent-encoding de espaço/Unicode, destinos não declarados/ausentes/não locais,
UTF-8 inválido causal, código/HTML literais, envelope descartado e outputcap
exato/um abaixo, Original/cancel/deadline e cleanup.

Comandos focais na fila compartilhada configurada:

```text
cargo test --offline --locked -p graphit-knowledge --test markdown_export --test okf_export -j4
cargo clippy --offline --locked -p graphit-knowledge --lib --test markdown_export --test okf_export -j4 -- -D warnings
```

SDK, strict e integração produtiva dos perfis têm recibos separados. Deltas
usam somente o alvo okf_export afetado; a prova Markdown SDK76 é preservada.
Checks OKF/Root e requisitos amplos de Knowledge permanecem abertos até sua
evidência correspondente; SDK não substitui ingresso CLI/tool real.
