---
title: Corpus desejado capturado Knowledge em Rust
type: spec
references:
  - target: {type: task, id: tsk-3f7e}
    relation: implements
  - target: {type: task, id: tsk-990e}
    relation: specified_by
  - target: {type: task, id: tsk-cd410}
    relation: uses
  - target: {type: task, id: tsk-1138}
    relation: uses
  - target: {type: task, id: tsk-f814}
    relation: uses
  - target: {type: task, id: tsk-9983}
    relation: consumer_boundary
  - target: {type: task, id: tsk-93e8}
    relation: consumer_boundary
  - target: {type: task, id: tsk-6dec}
    relation: refined_by
  - target: {type: task, id: tsk-6ded}
    relation: refined_by
---

# Corpus desejado capturado Knowledge em Rust

Contrato da biblioteca Rust para selecionar e compilar integralmente as fontes documentais de um PROJECT. O target focal executou oito testes reais e o Clippy scoped passou com warnings negados. Esta entrega é read-only: oferece captura, identidade e metadata compilada; a reconciliação com o corpus Data e a integração Root são entregas separadas. As fixtures de filesystem/Auth desta folha executam em Linux; Windows e macOS não foram qualificados.

## Jornada e fronteiras

O consumidor conserva o `Arc<SnapshotContext>` original e obtém uma `ProjectReadAdmission` Knowledge válida para ele. Fornece um `ConfigSnapshot` já produzido por Core e os limites de cada produtor. Esse snapshot é configuração comum: não concede acesso nem comprova uma origem AuthConfig por si só.

1. `capture_knowledge_sources(original, read, config, index_path, limits)` resolve KnowledgeSettings e a policy de ignore e chama `CapturedSourceReader::capture_selected_project` sobre o scope real da read. Captura a seleção completa com bytes retidos. `capture_knowledge_source_metadata` compartilha a mesma resolução, critérios, binding e guards, usando `capture_selected_project_metadata` para conservar somente metadata da seleção; hashing físico e revalidações continuam reais.
2. `prepare_desired_knowledge_corpus(original, read, &sources, kind_on_absent, limits)` compila todos os FILEs selecionados. Um erro tardio invalida o produto inteiro.
3. `corpus.documents(original, read)` e `corpus.authored_inputs(original, read, updated_at, stale_after)` revalidam os grants e as fontes antes de devolver views.
4. O consumidor downstream projeta referências, cria H e usa o lifecycle Data com guarda física. Este coletor não publica, retira, materializa branches ou executa inference.

O Reader pertence a `CapturedKnowledgeSources`; o corpus empresta esse Reader externo. Cada documento conserva o path nativo observado, a identidade canônica e um `CompiledDocument` com corpo emprestado. Não há objeto com referência para sua própria alocação, clone de corpo, flag de completude ou restauração por DTO.

## Seleção e configuração

`KnowledgeSourceCaptureLimits` contém `settings: KnowledgeSettingsLimits`, `selection: ProjectSelectionLimits` e `read: SourceReadLimits`. A policy de ignore recebe somente contexto/config; defaults compilados não exigem quotas ou stats. Os limites reais de config, Brand, critérios, controles e FILEs continuam com os produtores existentes.

A seleção usa `settings.docs_dir()` como subtree, extras vazios, `include_readme()`, extensões configuradas e os defaults protegidos. Controles custom/Git são `.wikiignore`/`.gitignore`; Core aplica precedência, pruning, observações de ausência/presença e deduplicação antes de abrir payloads selecionados. Os controles não viram documentos do corpus.

`index_path`, quando fornecido, é o override explícito já validado por KnowledgeSettings. A configuração fica congelada na captura e pode ser consultada por `sources.settings()`. Não há procura de configfile, ambiente ou fallback interno neste consumidor.

A ausência de docs_dir com README selecionado permite um corpus README-only. Uma seleção completa realmente vazia permite corpus vazio. Erro de leitura, limite concreto de entrada/payload, parser, metadata ou autorização nunca vira lista vazia nem omissão de um FILE.

## Identidade e metadata

O frontmatter é analisado pelo parser existente. Se `id` estiver presente, precisa ser uma string literal: `null`, booleanos, números, collections e string vazia são erros. `'123'` e `'true'` são strings válidas; espaços, caixa e Unicode permanecem intactos. O type probe usa o campo original completo, incluindo o separador YAML, num contexto de mapping com continuação indentada e perfil semântico de serde-saphyr. Não usa regex de coerção nem scalar isolado; o texto literal continua sendo o da arena original. A validação estrita de `id` não reclassifica campos editoriais/desconhecidos, como `custom: .nan` e `description: '.nan'`, conforme admissão do compiler metadata existente. O `.nan` plain no ID é rejeitado; tags YAML, inclusive `!!str`, são rejeitadas pelo parser existente antes do type probe. `id: ---` é uma string válida no mapping e um ID permitido. `id: ...` também é string nesse contexto, mas o codec rejeita seu prefixo `..` com `Identity(InvalidAuthoredId)`. Nenhum deles é reinterpretado como marcador de um documento isolado. Duplicatas, aliases e anchors inválidos continuam rejeitados pelo parser.

Cada FILE usa o [codec de identidade](rust_knowledge_source_identity.md). Com ID explícito, rename conserva slug e muda origem; sem ID, a identidade deriva reversivelmente dos componentes relativos e ambos mudam. Colisão de slug ou origem falha antes do retorno, sem sufixo automático ou adoção de rows antigas.

O fallback editorial é o file_stem Unicode. Nome físico sem representação Unicode fiel é rejeitado na captura; não há conversão com perda nem um slug alternativo para contornar essa entrada. O fallback e `kind_on_absent` são policy explícita no `CompiledDocument`, não prova da origem física. BOM, frontmatter e line endings permanecem no corpo integral; o digest físico é o digest da captura real.

Exemplo: `docs/guide.md` com `id: ' Manual Ω '` retorna slug ` Manual Ω ` e origem `knowledge-source/t/646f6373/67756964652e6d64`. As identidades `knowledge-source/t/...` e `source-t-*` existentes para paths Unicode permanecem iguais.

## Views, guards e erros

Todos os getters que consultam liveness/fonte retornam `Result`:

```text
sources.reader(original, read) -> Result<&CapturedSourceReader, DesiredCorpusError>
corpus.documents(original, read) -> Result<&[DesiredKnowledgeDocument<'source>], DesiredCorpusError>
corpus.authored_inputs(original, read, updated_at, stale_after)
    -> Result<Vec<AuthoredDocumentInput<'view, 'source>>, DesiredCorpusError>
```

A lifetime `'view` está ligada ao `&self` do corpus. Os getters de cada documento (`relative`, `slug`, `source_path`, `compiled`) são views comuns já obtidas, sem autoridade própria. `sources.revalidate` e `corpus.revalidate` oferecem a guarda explícita para o consumidor. O Reader não pode ser reconstruído de hashes, paths ou headers.

As guards verificam ReadKnowledge, mesma Arc original, scope record e VersionContext congelados, reader/request e revogação. Revalidação física completa ocorre após captura, antes e após preparação integral e nas views. Entre FILEs há checks baratos; o coletor não recaptura o projeto por arquivo. Mutação de body, novo FILE, remoção ou alteração de controle invalida o produto antes do próximo retorno verificado.

`DesiredCorpusError` preserva causas de Auth, Request, Source, Settings, Ignore, Parser, Compilation, TypedId, Identity e Allocation. WrongAdmission, DifferentRequest, ScopeChanged, InvalidId, MissingRetainedFile, DuplicateSlug/DuplicateOrigin, LimitExceeded e Overflow explicitam demais condições. Request mantém `Interrupted`/`TimedOut`; erro nunca devolve prefixo ou produto parcial. Checks são cooperativos, sem promessa de preempção durante trabalho interno dos SDKs.

`Source { operation, relative, cause }` mantém o objeto `io::Error` original como
`source()`. A operação descreve captura, abertura, leitura ou revalidação; o caminho relativo
nativo é opcional e só identifica o FILE em processamento quando disponível.
Uma falha agregada não inventa um caminho. O diagnóstico não inclui corpos,
não recaptura a fonte e não altera autoridade ou semântica de recuperação.

`updated_at` e `stale_after` são dados opcionais fornecidos pelo caller. As views sempre usam `stale_since: None`. A API não consulta relógio, resolve staleness global ou limpa flags persistidas automaticamente.

## Limites e referências downstream

`KnowledgeDesiredPreparationLimits` contém `parser: DocumentParseLimits`, `metadata: PublicationMetadataLimits` e `mapping: NativeAuthoredMappingLimits`. Core limita cada payload selecionado, cada controle, cada caminho e os critérios/profundidade. O coletor não impõe quantidade de FILEs nem soma de bytes do corpus.

O limite individual `mapping.max_field_bytes` admite separadamente nove campos reais: slug, origem, corpo integral, title, kind, summary, description, fallback_title e kind_on_absent. Prefixos e separadores canônicos pertencem ao campo que os contém. Igualdade passa; exceder um byte em qualquer campo rejeita o produto inteiro. Parser e metadata mantêm suas admissões por documento. URI e target pertencem à preparação downstream do mapping.

O número real de FILEs serve somente às reservas dos owners; não é uma quota. Buffers próprios usam reservas fallíveis e aritmética necessária aos tamanhos reais. Duplicatas, seleção até EOF, grants e Source continuam verificados independentemente; esses checks não dependem de contabilização agregada. Não há promessa de RSS ou processamento lazy.

A factory retentiva conserva todos os corpos selecionados; o compiled corpus e suas views mantêm esses owners. Ela continua ativa para o modo existente S4 `Some(expected)`. A factory de metadata não retém payloads dos FILEs. S4 `None` usa essa factory e a [primeira publicação em stream](rust_knowledge_source_stream.md), materializando e compilando um FILE por vez. Isso não qualifica RSS total do SDK ou uma única leitura física.

Este coletor não declara WikiTargets ou tuplas authored semanticamente validados. `93e8/native_document_mapping` valida WikiTargets/ReferenceGraph; `9983/document_authored_references` admite tuplas authored, consumida por primeira publicação `6101`, republicação `f02f` e pela [composição da seleção completa](rust_knowledge_desired_reconciliation.md). As validações semânticas de cada FILE precisam terminar antes de sua emissão. A seleção inteira e seu EOF validado antecedem commit; prepare SDK pode produzir staging durante o stream. Não confundir parser sintático de YAML com esses produtores de referências.

## Extensão corrente

O consumidor stream usa a factory de metadata para capturar a seleção por FILE.

## Verificação e navegação

[Source](../../crates/graphit-knowledge/src/desired_corpus.rs) e [fixtures Auth/Core reais](../../crates/graphit-knowledge/tests/desired_corpus.rs) protegem seleção configurada e deduplicada, corpo/digest emprestados, empty/README-only versus erro tardio, IDs tipados, rename/colisão, filename nativo, limites exatos/+1, grants/requests/versões, revogação/cancelamento/prazo e fontes alteradas antes/depois do preparo. Nenhuma dessas operações cria índice ou embeddings Knowledge.

```text
cargo test --offline --locked -p graphit-knowledge --test desired_corpus -j4
cargo clippy --offline --locked -p graphit-knowledge --lib --test desired_corpus --no-deps -j4 -- -D warnings
```

Ver também a [policy de ignore](../architecture/rust-knowledge-ignore-policy.md), [metadata de publicação](../architecture/rust-knowledge-publication-metadata.md) e [fronteiras Knowledge/Data/Records](../architecture/rust-knowledge-data-boundaries.md). A existência desta API não conclui a migração global ou suas entregas lifecycle/domínio pendentes.
