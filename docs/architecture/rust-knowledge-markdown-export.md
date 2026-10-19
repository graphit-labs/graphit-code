---
title: Exportação Markdown nativa de Knowledge em Rust
doc_type: architecture
references:
  - target: {type: task, id: tsk-9d92}
    relation: implements
  - target: {type: task, id: tsk-b324}
    relation: relates_to
  - target: {type: task, id: tsk-cd46}
    relation: implemented_by
  - target: {type: task, id: tsk-23c5}
    relation: specified_by
  - target: {type: task, id: tsk-5de4}
    relation: uses
  - target: {type: task, id: tsk-55e8}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: memory, id: 01M4MT1HZN2BNAKXPG6C81X5ZE}
    relation: supports
  - target: {type: task, id: tsk-c9e2}
    relation: refined_by
  - target: {type: task, id: tsk-a99d}
    relation: implements
---

# Exportação Markdown nativa de Knowledge em Rust

Integradores preparam arquivos Markdown a partir de uma captura Source Knowledge
publicada com `graphit_knowledge::markdown_export::MarkdownExportStage`.
O estágio renderiza cada documento owned em arquivo privado e, após EOF, entrega
um `SourceSnapshot` file-backed do Core, com páginas, índice, referências,
backlinks e comunidades. O [caller App autenticado](rust-knowledge-markdown-publication.md)
compõe autorização e publicação filesystem. A tool existente
`graphit_knowledge_export` seleciona `format: native|markdown`; `native` é o padrão.

## Entrada e autoridade

O chamador abre uma única captura Source-only usando
`capture_native_knowledge_search` com sua admissão real `ReadKnowledge`.
`KnowledgeSearchCapture::next_documents` retorna lotes owned FULL18 da mesma
geração e conserva Original, guards e validação de schema/identificadores.
`Some([])` pode conter rows Removed validadas e não significa EOF: somente
`None` confirma o fim válido. Não se abre NN nem outra captura para exportar. Para Knowledge importado,
`CapturedImportedKnowledge::capture_source` conserva o mesmo ModuleSnapshot por
Arc; seu cursor entrega documentos owned e revalida o registry e a admissão real
da instalação até EOF. O estágio e a árvore Markdown são os mesmos.

```rust,ignore
let mut stage = MarkdownExportStage::new_in(
    original, output_parent, modes, limits, KnowledgeTextExportFormat::Markdown,
)?;
while let Some(documents) = capture.next_documents(original, read).await? {
    for document in documents {
        stage.push(original, document)?;
    }
}
let plan = stage.finish(original)?;
// Manter plan vivo até revalidação, rename exclusivo e cleanup explícito.
```

`NoCorpus` permanece ausência; uma captura publicada que chega a EOF sem
documentos ativos permanece `PublishedEmpty(generation)` no App. Nenhum desses
resultados cria staging ou publica/remove o destino. No SDK, `finish` de um
estágio vazio retorna `NoDocuments`.

`MarkdownExportStage::new_in(original, output_parent, modes, limits, format)` é o único
construtor. O formato é explícito; o estágio conserva o Original e cria
um `TempDir` privado sob o parent fornecido. O parent deve existir; o App usa
o parent real do destino para conservar o mesmo filesystem durante a renomeação. `push(original, KnowledgeDocument)` consome um documento,
valida os identificadores e o corpo e grava metadata, H1 e corpo em spool por
arquivo. Depois da chamada, só slug, título, targets resolvidos e offset do corpo
permanecem para navegação. Body, recipe e summary não são retidos no corpus.
`finish(original)` completa índice e navegação,
copia prefixo e corpo com IO bounded e captura o diretório como `SourceSnapshot`.

`MarkdownExportPlan` contém `payload: SourceSnapshot`,
`communities: DocumentCommunities` e custódia privada do `TempDir`.
`directory()` permite inspecionar o caminho, sem conceder autoridade filesystem.
O owner mantém o plano inteiro vivo durante a revalidação física Core e a
publicação. `close(self)` devolve comunidades e o resultado IO real da remoção do
spool privado. Descartar o plano sem `close` usa a limpeza best effort do TempDir.
Depois de renomear `directory()` para fora do staging, `close` remove apenas o
spool remanescente e não o diretório publicado. A geração observada é owned pelo App e
não integra o plano nem os arquivos. Seu `corpus_uri` não é serializado.
Veja a [publicação nativa](rust-knowledge-native-publication.md).

A classificação usa o único ReferenceGraph construído com os metadados
retidos após EOF e ocorre antes da preparação do plano de publicação.
O report é owned e permanece separado dos arquivos exportados. Essa composição
não acrescenta campos a MarkdownExportLimits nem altera bodies, metadata,
filenames, navegação ou fingerprint do payload. O
[kernel de comunidades](../specs/rust_knowledge_document_domains.md) define
membros, label, coesão e ausência de grupo para singletons.

`MarkdownExportModes` recebe os modos de diretório e arquivo explicitamente.
O Core captura o diretório gerado sem limites agregados de corpus ou retenção
de corpos. Valida nomes, modos, identidades, digests e rejeita links; a
revalidação conserva o Original e detecta alterações antes da publicação.
O produtor não cria contexto nem concede acesso.

## Arquivos e navegação

A árvore contém a raiz relativa vazia, `pages/`, `index.md` e uma página por slug.
Os documentos são ordenados pelo slug UTF-8 para produzir o mesmo fingerprint
quando a ordem de entrada muda. O nome de página é
`pages/p-<SHA256 dos bytes UTF-8 do slug em hexadecimal minúsculo>.md`.
O basename tem 69 caracteres ASCII. Assim, slugs como `index`, `CON`, `A`, `a` e
nomes Unicode não se tornam nomes reservados ou aliases de caminho.

Os identificadores continuam sujeitos ao contrato de
[referências documentais](rust-knowledge-wiki.md#produtor-rust-de-referências-resolvidas): vazio, prefixo `..`,
slash e backslash são inválidos. Slugs duplicados são rejeitados. O namespace
não promete ausência matemática de colisões SHA256. Spool e páginas são abertos
com `create_new`: um caminho já existente retorna `Payload` com a causa IO
`AlreadyExists`, antes da captura física pelo Core, sem sobrescrever conteúdo.
O índice tem namespace próprio e não substitui uma página cujo slug é `index`.

Cada página tem metadata YAML nativa, H1 do título publicado, referências,
backlinks e uma seção Content com o corpo autoral. O título vazio ou só whitespace
usa o slug para apresentação; o metadata conserva o título original.
O índice informa a contagem de páginas e links para os arquivos em `pages/`.
Não há timestamp criado durante a exportação, agrupamento por heurística de tipo,
log de revisões ou histórico sincronizado fabricado.

A navegação usa a adjacência resolvida do `ReferenceGraph` existente por
`direct_neighbors`, outbound e inbound de profundidade 1, em ordem lexical.
O iterador revalida o Original por item e não cria vetor de resultados ou cap de
navegação paralelo. Links repetidos são deduplicados e autorreferências são
ignoradas. Destinos
conhecidos usam arquivos do namespace; backlinks apontam para a página de origem.
Um destino ausente aparece como texto seguido de `(missing)`, sem href para um
arquivo inexistente. Os targets originais, incluindo ordem, repetições e
referências ausentes, continuam no metadata nativo.

Somente H1, navegação e índice passam pelo serializer Markdown
`pulldown-cmark-to-cmark` 23.0.0, compatível com `pulldown-cmark` 0.13.4.
Os eventos são estruturados e usam parser options vazias e formatação padrão.
Labels com pontuação, entidades, HTML e Unicode permanecem texto. Cada CR, LF ou
TAB vira um espaço na apresentação em linha; esse ajuste não altera o metadata.
NUL em um label de apresentação retorna `InvalidDisplayText` porque CommonMark
não o representa literalmente.

Este é o perfil Markdown nativo definido em `tsk-23c5`. Os
[perfis OKF e Obsidian](rust-knowledge-okf-export.md) têm cabeçalho próprio e
metadata nativa na extensão graphit. O enum público
`graphit_knowledge::okf_export::KnowledgeTextExportFormat` seleciona
`Markdown`, `Okf { brand }` ou `Obsidian { brand }`, pelo quinto argumento
do único construtor `MarkdownExportStage::new_in`.

Markdown e OKF conservam o corpo autoral literal. Obsidian usa eventos e offsets
P1 e o mesmo normalizador canônico do produtor de referências. Só mapeia alvos
declarados naquele FILE e presentes no roster real, preservando fragmentos e
texto visível. Labels explícitas ficam byte a byte; labels implícitas são
materializadas minimamente após trocar o destino. Elegibilidade, erros causais,
exclusões, análise por FILE e qualificação estão no contrato do profile.
Qualificações Markdown anteriores não qualificam automaticamente os demais perfis.

## Metadata, corpo e proveniência

O cabeçalho reutiliza o serializer privado da
[fonte virtual nativa](rust-knowledge-document-source.md), com os mesmos 15 campos:
id, título, tipo, summary, description, summary_truncated, source_path,
physical_digest, compilation_digest, recipe, recipe_parameters, updated_at,
stale_since, stale_after_ns e resolved_wiki_targets.

`description` integral opcional não é substituída pelo preview `summary`.
`None` e `Some("")` conservam sua distinção; digests físico e de compilação
conservam papéis separados. Timestamps publicados são formatados em UTC com
nanosegundos e os mesmos limites de representação da fonte virtual. Não são
inventados ator `generated.by`, confiança, revisão, stale_reason ou sync_log.
A observação de staleness publicada não é recomputada como diagnóstico de
staleness durante a exportação.

No perfil Markdown, `parse_document` recebe `document.body` e
`document.recipe_parameters.fallback_title` no contexto original. A exportação
usa somente `parsed.source[parsed.envelope.body]`: título e summary recalculados
pelo parser não substituem valores publicados. BOM e frontmatter ficam fora da
janela segundo o contrato P1. Dentro da janela, espaços, CRLF, trailing newline,
links, código e HTML permanecem byte a byte, sem trim nem reflow.

A navegação precede o corpo, que é a última seção; nada é anexado depois dele.
Isso impede que um fence ou HTML autoral aberto engula a navegação gerada.
O parser permanece estrito: YAML malformado e quotas P1 não acionam fallback de
strip-frontmatter ou interpretação Go.

## Limites e falhas

Todos os limites próprios são `NonZeroUsize`. Não existem defaults ocultos.

| Limite | O que conta |
| --- | --- |
| max_file_bytes | Cada arquivo completo, incluindo YAML, escapes, navegação, separadores e corpo. |
| document | Limites por documento `DocumentParseLimits`, inclusive YAML, eventos, scalar, links e projeções. |
| references | `ReferenceLimits.max_identifier_bytes` para cada identificador. |

Não há limites Markdown cumulativos de páginas, bytes do corpus, bytes de saída
ou links de navegação. Os limites individuais de parser e arquivo continuam aplicáveis;
nenhum é substituído por um máximo artificial ou contador equivalente.
A contagem por arquivo usa aritmética checada. O contexto original é verificado
antes de criar o estágio, por página, target, evento, escrita, cópia bounded e
finalização. Outro Original é rejeitado antes de continuar o estágio.
Cancelamento e deadline já vencidos retornam `Request` com causa IO
`Interrupted` ou `TimedOut` antes de erros de conteúdo.

Os adapters `fmt::Write` e `io::Write` preservam a primeira causa de falha de
cancelamento, quota, overflow ou alocação. O writer YAML não repete
`Interrupted`. O erro SDK não mascara uma causa anterior no buffer.
Não é retornado plano parcial. Falhas de `finish` descartam o estágio privado;
o App também o descarta quando propaga falha de `push` ou do cursor. O SDK não
publica o destino. A publicação e seus efeitos pertencem ao Core/App.
Os limites individuais não são garantias de memória; SDKs podem reter blocos/eventos,
usam suas próprias alocações e não são preemptíveis. Não há garantia de RSS ou
prazo rígido para uma chamada síncrona opaca.

`MarkdownExportError` distingue `Request`, `Source`, `Parse`, `References`,
`Communities`, `Serialization`, `Payload`, `NoDocuments`,
`InvalidDisplayText`, `Limit`, `Overflow` e `Allocation`.
Communities conserva DocumentCommunityError real na cadeia Error::source.
Sua variante Library mantém o erro tipado e o Display do SDK, mas source retorna
None porque graphrs::Error não implementa std::error::Error. Limites e erros
de domínio sem causa não ganham uma causa artificial.

## Publicação e lifecycle

Consumir o plano não autoriza uma escrita. O
[caller App autenticado](rust-knowledge-markdown-publication.md) exige ReadKnowledge e
`Permission::External(ExternalEffect::PublishedFilesystem)` reais, destino sob a raiz
do projeto e fora do runtime, e as APIs do Core
`writer_with_context`, `materialize_payload` e `publish` com captura de escopo.
Root/ancestor e destinos dentro do runtime não são destinations PROJECT válidos.
Pending, recuperação, efeitos parciais e reconciliação usam o produtor existente
Core/FS; a exportação não cria um segundo writer ou rollback genérico.

Esta folha descreve preparação por arquivo e composição Source EOF → plano Core.
Publicação em destino e lifecycle do caller têm contrato e evidência próprios em
[publicação autenticada](rust-knowledge-markdown-publication.md). A integração
CLI/MCP usa o Main e tem sua evidência própria. Falhas FS usam os recibos estruturados reais do Core, conforme esse
caller, sem promessa de cadeia IO que a API do produtor não oferece.

A composição App mantém o plano inteiro vivo até terminar materialização,
publicação e reconciliação. Só então move as comunidades owned para
`KnowledgeMarkdownPublished`. O resultado contém a geração observada e as
comunidades, inclusive ArtifactChange::Unchanged. Não há clone de body ou do
corpus para produzir esse resultado.

As mesmas rows FULL EOF alimentam payload e classificação. O report não
promete uma pin SDK viva durante publicação filesystem nem que a geração
observada ainda seja o head atual. NoCorpus e PublishedEmpty mantêm suas
variantes sem report; documentos somente isolados produzem report vazio.
Recovery conserva sua API e não refaz leitura Data ou classificação.
Erros e efeitos depois da preparação/publicação permanecem causais.

## Verificação

A mudança para `MarkdownExportStage` substitui o builder slice e as verificações
exclusivas de quotas agregadas. Os testes atuais exercitam o plano file-backed,
namespace/fingerprint determinísticos, metadata e bytes independentes, navegação
resolvida, comunidades, limites individuais por arquivo e cancelamento. A regressão
`staged_body_is_final_suffix_and_private_files_live_with_the_plan` usa o estágio
real com fences/HTML abertos e confirma a limpeza do diretório ao descartar o
plano. A evidência de execução desta mudança pertence a `tsk-9d92`;
os recibos abaixo são históricos e não qualificam automaticamente a nova API.

A integração `tsk-a99d` passou nas suítes focais em Linux: sete funções
de export Knowledge, sete regressões do kernel e 16 funções App de exportação
autenticada. Três funções são novas nesta integração; replays e casos dentro
de funções não aumentam essa contagem. As asserções preservam bytes, digests,
modes, pins e fingerprint e verificam report em Created/Updated/Unchanged.

O teste puro Knowledge usa uma geração ordinary diferente e prova payload
idêntico/report estável. O App usa republicação Data real, prova avanço de
geração e label atualizado com Updated, seguido por Unchanged nessa geração.
Não demonstra nova geração SDK com os mesmos bytes filesystem.
O recibo `.build/rust-knowledge-community-export/validation-current.json`
separa essa evidência do kernel e dos receipts anteriores, mantidos intactos.
Clippy estrito nos targets focais passou. A qualificação documental final
consta no receipt desta integração. Windows e macOS não foram executados.

Os resultados e receipts anteriores são históricos e permanecem intactos.
A mudança de limites em `tsk-c9e2` foi qualificada em Linux: 157 funções
da biblioteca e dos 12 targets afetados, Clippy estrito nos mesmos targets,
compilação das duas fixtures App e três doctests modificados passaram.
O recibo `.build/rust-knowledge-reference-limits-cleanup/validation-current.json`
separa as falhas iniciais de compilação, o replay de referências e a evidência
atual. Windows e macOS não foram executados.

Os testes protegem namespace, fingerprint/geração, corpo literal, roundtrip de
metadata, escaping reparsado, referências reais, limites exatos e um abaixo,
erros/cancelamento e Data nativo com admissão real. Testes anteriores de export
foram revisados pelo propósito páginas/índice/nav/backlinks/YAML hostil; helpers,
aliases e trim de Go não foram portados. A fonte virtual mantém suas regressões.

Comandos portáveis, executados com o target/cache combinado com Main:

```text
cargo test --offline --locked -p graphit-knowledge --test markdown_export --test document_source --test native_index_publication --lib -j1
cargo clippy --offline --locked -p graphit-knowledge --lib --test markdown_export --test document_source --test native_index_publication -j1 -- -D warnings
```

A evidência executada e seus resultados ficam em `tsk-cd46`. Inspeção de contratos
não é execução; sucesso Linux não qualifica Windows/macOS nem a migração global.

Qualificação desta folha em Linux: 26 testes afetados passaram — seis privados
do exportador, seis públicos, seis da fonte virtual, dois privados da fonte e
seis de publicação nativa. As suítes da fonte passaram após a extração do helper
privado; exportação e publicação foram executadas novamente após a revisão final
de navegação e do oracle independente de bytes, SHA por arquivo e fingerprint
Core. Clippy com `-D warnings`, rustfmt dos arquivos próprios e `git diff --check`
passaram. A revisão de impacto confirmou a correção do fallback de títulos
whitespace e a cobertura do oracle. Recibos e logs estão registrados em
`tsk-cd46`; o runner compartilhado e o gate global permanecem com Main.
