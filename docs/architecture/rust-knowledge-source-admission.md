---
title: Leitura autenticada de fonte Knowledge no App
doc_type: architecture
references:
  - target: {type: task, id: tsk-3032}
    relation: implements
  - target: {type: task, id: tsk-21cd}
    relation: uses
  - target: {type: task, id: tsk-322f}
    relation: implements
  - target: {type: task, id: tsk-5de4}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: uses
  - target: {type: task, id: tsk-8218}
    relation: uses
  - target: {type: task, id: tsk-7d1e}
    relation: uses
  - target: {type: task, id: tsk-1f4a}
    relation: uses
---

# Leitura autenticada de fonte Knowledge no App

Integradores App podem usar `read_knowledge_source_authenticated` para ler a
fonte virtual de um documento PROJECT por seu slug canônico exato. A biblioteca
admite o caller real, lê o stream selecionado por slug até EOF e delega renderização e seleção
textual aos produtores Knowledge/Core existentes.

A folha inicial 322f qualificou sete testes e a API autenticada em Linux. A
correção 3032 consome o [reader local selecionado](../specs/knowledge_index_page.md)
qualificado em 21cd, preservando a API e os estados do App. A suíte atual foi qualificada em Linux: oito PASS, Clippy estrito exit 0 e
rustfmt/diff aprovados. A prova anterior permanece histórica.

Esta folha não realiza cutover do ingresso CLI/MCP nem aposenta contratos ativos
de aliases e hints sob responsabilidade de seu integrador.

## Contrato público

A API está em [knowledge_source.rs](../../crates/graphit-app/src/knowledge_source.rs):

```rust
pub async fn read_knowledge_source_authenticated<'a>(
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    scope: ScopeInput<'a>,
    request: KnowledgeSourceRequest<'_>,
    policy: KnowledgeSourcePolicy,
    original: &'a RequestContext<'a>,
) -> Result<KnowledgeSourceRead, KnowledgeSourceError>
```

O caller fornece a autoridade selecionada, caller Runtime ou Local autenticado,
scope PROJECT capturado e `RequestContext` original. A admissão opaca é criada
dentro da operação; grants ou identidades reconstruídos não substituem autoridade.

```rust
pub struct KnowledgeSourceRequest<'a> {
    pub slug: &'a str,
    pub selection: &'a graphit_core::text_slice::Request,
}
pub struct KnowledgeSourcePolicy {
    pub index: KnowledgeIndexReadLimits,
    pub source: DocumentSourceLimits,
}
```

Ambos são `Clone + Copy`. A request empresta os argumentos durante a chamada;
o resultado é owned e não conserva esses empréstimos.

## Autoridade e sequência

A operação verifica Original e usa a factory existente
`ProjectRecordAuthority::admit_authenticated_read(caller, original, scope,
ProjectReadModule::Knowledge)`. A factory verifica a linhagem real de issuer,
caller, request e PROJECT capturado, com `Read(Knowledge)` efetivo. Runtime e
Local podem usar somente leitura, sem Configure ou permissão de publicação.
Veja [Admissão PROJECT](../specs/project_record_admission.md).

1. Verificar Original e obter a admissão autenticada Knowledge.
2. Validar slug não vazio e seu limite de bytes.
3. Chamar `read_native_knowledge_page` com admissão, snapshot e slug originais.
4. Aguardar todos os registros do slug selecionado até EOF, validando duplicidade e integridade.
5. Rechecar Original/Auth e mapear o estado selecionado owned de Data.
6. Quando encontrado, chamar `render_document_source` uma vez.
7. Rechecar Original/Auth antes de devolver qualquer estado de sucesso.

Data filtra o slug no scanner do pin admitido e valida somente seus registros.
Corpo e focused_source são extraídos juntos, sem uma consulta global de metadata.
Um registro inicial não permite ignorar duplicatas ou corrupção posteriores do
mesmo slug. Um documento alheio não é carregado/validado por esse get; a antiga
exigência de falha por corrupção não selecionada foi supersedida.
Data/Source errors retornam imediatamente
com sua primeira causa; rechecagens posteriores não substituem essas falhas.

## Identidade e ingresso

O slug usa igualdade UTF-8 exata: sem trim, remoção de `.md`, alias, casefold,
normalização Unicode, fallback de título ou heading. Se o corpus contém somente
`zz-guide`, `ZZ-GUIDE`, `zz-guide.md` e ` zz-guide ` não selecionam essa página.
Esses bytes poderiam ser outra identidade armazenada; não são aliases dela.

Slug vazio produz `InvalidSlug`. Ultrapassar
`policy.index.content.max_identifier_bytes` produz `SlugLimit`, após admissão
legítima e antes de Data. Essa validação reutiliza o limite existente.

Após passar os guards App de vazio/bytecap, separadores `/` e `\`,
prefixo `..` e controles produzem erro Data causal
`InvalidField("slug")`, após Auth e antes da aquisição nativa, inclusive se o
corpus estiver ausente. Esses inputs não produzem `PageAbsent`. Vazio e bytecap
continuam sendo os erros App acima.

Trim, extensão, equivalência Unicode, colisões e hints do ingresso MCP continuam
sob responsabilidade de seu integrador. Esta biblioteca canônica não os
implementa nem declara aposentados os testes funcionais daquele ingresso.

## Fonte virtual e seleção

App passa o documento FULL16 ao
[renderer nativo Knowledge](rust-knowledge-document-source.md), sem abrir um
Markdown local ou reconstruir título/body. O renderer compõe `---\n`, YAML
terminado em LF, `---\n\n` e body literal. Description `None` e string vazia são
distintos; digests, datas/nanos, receita/parâmetros e targets vêm do documento
publicado. BOM, CRLF e eventual envelope do body permanecem literais. O renderer
não avalia staleness atual nem fabrica metadados de Memory/Task.

Core aplica `range → head → tail → pattern` dentro do renderer, uma única vez.
Coordenadas e `total_lines` pertencem à fonte virtual completa, incluindo header
e a linha vazia após LF final. Um padrão sem matches é `Found` com `source` e
`matches` vazios e contagem real. Body vazio também é `Found`, com header.
Veja [Publicação FULL16](rust-knowledge-native-publication.md).

## Estados owned e geração

| Estado | Significado |
| --- | --- |
| `NoCorpus` | Ausência observada e autorizada por Data. |
| `PublishedEmpty { generation }` | Geração publicada sem documentos. |
| `PageAbsent { generation }` | Corpus não vazio, sem slug exato. |
| `Found { generation, slug, selection, focused_source }` | Documento e seleção Core concluídos. |

`Found.slug` é o `String` movido do documento Data; `selection` é
`graphit_core::text_slice::Result`. Nenhum scanner, pin, admissão, handle ou
referência ao corpus escapa. A geração é a observação Data, sem autoridade,
promessa de head atual ou CAS. B ainda não materializado pode herdar default,
com `physical_branch = None` e `inherited_target` correspondente.

A leitura não materializa B nem usa writer, cria, repara ou publica corpus.
Markdown local pode estar ausente.

## Limites e falhas

O índice selecionado não usa quotas textuais agregadas do corpus.
`max_identifier_bytes` limita cada identificador selecionado e
`batch_rows` os batches SDK. Não há quota agregada de páginas ou corpos alheios.
Um stream selecionado vazio usa somente um probe de metadata de uma identidade
para distinguir corpus vazio de página ausente. Exportação e listagem integral
usam seus readers completos; um get exige EOF somente de sua seleção.

`index` reutiliza `KnowledgeIndexReadLimits`; `source` reutiliza os seis limites
positivos de `DocumentSourceLimits`: input agregado, targets, bytes e linhas
virtuais, bytes de saída e matches. Escaping e framing contam nos bytes virtuais;
a saída inclui `source` e cópias de `Match.line`. Esse limite é verificado após
Core, sem impedir todas as suas alocações internas. Não há nova quota, promessa
de RSS ou preempção nativa.

```rust
pub enum KnowledgeSourceError {
    Request(io::Error),
    Auth(AuthError),
    Data(Box<KnowledgeIndexError>),
    Source(Box<DocumentSourceError>),
    InvalidSlug,
    SlugLimit,
}
```

`Error::source()` conserva causas Request/Auth/Data/Source; os erros App de vazio/bytecap não
têm causa interna, enquanto a sintaxe inválida conserva a causa Data. Regex inválida e falhas de seleção são Source errors;
corpus corrupto ou índice `Other` são Data errors. Não viram ausência ou vazio.
Uma falha não retorna prefixo, faz retry, reparo ou fallback físico.

`Display`/`Debug` do wrapper de erro mostram categorias fixas sem paths, fonte
ou credenciais. O sucesso deriva `Debug` e pode expor fonte, slug e geração;
esta biblioteca não define uma política global de logging/wire. Uma nova
tentativa deve usar autoridade/contexto adequados, sem renovar o Original.

## Cobertura e qualificação

Os oito testes em [knowledge_source.rs](../../crates/graphit-app/tests/knowledge_source.rs)
usam Auth Runtime/Local, scopes Core, publicação H/Data e SDK reais. A autorização
de seed é separada da política ReadKnowledge do reader.

| Caso | Fronteira protegida |
| --- | --- |
| Ausência/publicação vazia | Estados distintos, nenhuma criação por leitura. |
| FULL16/lookup/Core | Body/metadata literal, alvo tardio com batch de uma linha, seleção e ownership. |
| Named/default/inherited | Geração correta e B sem materialização. |
| Denial/linhagem/slug | Permissão, Original, issuer e scope de outra instalação; vazio/bytecap e sintaxe inválida causal após Auth. |
| Seleção/renderer | Limite do registro selecionado, regex inválida e limite Source sem prefixo. |
| `Other`/corrupção | Corpus real substituído somente no índice; I/O nativo e SDK causais. |
| Cancelamento/deadline | Original causal, inclusive cancelamento em voo. |
| Retenção selecionada | Três bodies válidos com total acima do limite por documento, Source owned e sem mutação. |

Imagens de arquivos antes/depois verificam ausência de mutação. A qualificação
3032 teve oito PASS (3,66 s, zero falhas/ignored/filtered), Clippy estrito
exit 0, rustfmt/diff exit 0 e seis links relativos válidos. Source, fixtures, Doc
e Wiki foram revisados nos dois sentidos. Receipts estão em
`.build/rust-knowledge-app-selected`. O
receipt anterior `.build/knowledge-app-source/qualification.json` conserva a
prova histórica de 322f (sete PASS, 3,51 s, Clippy/fmt).

Fixtures históricas de linha alheia inválida/duplicada pertencem ao contrato
anterior de 21cd e não qualificam o get selecionado atual. A exigência global foi
supersedida: os casos atuais validam independência de documentos alheios e
integridade/duplicidade do slug selecionado. O App mantém limites de
identificadores/targets selecionados e do renderer sem saída parcial. Não
atribuir o aceite atual às provas históricas de 322f/3032.

Comandos focais para os targets afetados, usando o perfil do projeto e o target
compartilhado. A qualificação histórica usou overrides globais de debug; a
execução atual não os reaplica. Use `CARGO_INCREMENTAL=0` e o SDK configurado
na janela warm compartilhada autorizada:

```text
cargo xtask cargo test --offline --locked -p graphit-app --test integration -j1 --target-dir .build/rust-records-data/knowledge-index-source/target -- knowledge_source::
cargo xtask cargo clippy --offline --locked -p graphit-app --lib --test integration -j1 --no-deps --target-dir .build/rust-records-data/knowledge-index-source/target -- -D warnings
rustfmt --edition 2024 --check crates/graphit-app/src/knowledge_source.rs crates/graphit-app/tests/knowledge_source.rs
```

Histórico 322f: `.build/knowledge-app-source/native-first.log`, `native-second.log`,
`native-third.log` e `clippy-first.log`. Resultados correntes usam o diretório 3032 acima. A primeira compilação 3032
parou em um arquivo Records ainda não entregue; a segunda executou os oito testes.
O primeiro Clippy parou na incoerência do lock compartilhado; o segundo passou
após reconciliação pelo owner, preservando `--locked`. Essas tentativas permanecem
nos receipts e não são apresentadas como testes aprovados.
Não repetir matrizes Core/Data/renderer como novos resultados App. Esta página
não certifica Windows/macOS, cutover CLI/MCP, wire global ou migração concluída.
