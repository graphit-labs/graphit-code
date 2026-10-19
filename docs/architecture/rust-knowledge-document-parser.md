---
type: architecture
title: Parser documental Knowledge em Rust
description: Sintaxe YAML e Markdown com fonte original, limites explícitos e contexto Core.
references:
  - target: {type: task, id: tsk-87f7}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-cdb6}
    relation: describes
  - target: {type: task, id: tsk-b210}
    relation: derived_from
  - target: {type: task, id: tsk-31f2}
    relation: uses
  - target: {type: task, id: tsk-d83c}
    relation: uses
  - target: {type: knowledge, id: Wiki_Module_Specification}
    relation: implements
  - target: {type: knowledge, id: Persisted_record_relationships}
    relation: supports
---

# Parser documental Knowledge em Rust

`tsk-cdb6` entrega a projeção sintática necessária antes da compilação documental:
bytes UTF-8 → envelope, metadados YAML em arena plana, eventos Markdown tipados,
título e resumo. A implementação candidata está em
`crates/graphit-knowledge/src/document.rs`. Os testes estão no próprio módulo e
as fixtures em `crates/graphit-knowledge/tests/fixtures/document/`.

Estado: biblioteca pura ativada e qualificada funcionalmente no Linux. Run4
aprovou **16 unitários, três integrações públicas e Clippy estrito**, sem testes
falhos ou ignorados. A Source atual e os outros catorze arquivos monitorados
permaneceram íntegros durante os três comandos, todos os processos terminaram
e a CPU foi devolvida ao principal. O guard Git Integration específico foi
qualificado com bloqueio e restauração reais; essa entrega não encerra a
migração Knowledge/Wiki.

qualification-run-4.json (`../evidence/knowledge-cdb6/qualification-run-4.json`; historical local artifact unavailable)
registra comandos, ambiente real, contagens, logs, tempo/RSS e quinze hashes
antes/depois. A execução Linux não comprova Windows/macOS. Nenhum doctest do
parser foi executado ou inferido; as integrações públicas comprovaram tipos,
lifetimes e uso da API.

Os recibos anteriores são históricos preservados:
Run1 (`../evidence/knowledge-cdb6/qualification-run-1.json`; historical local artifact unavailable) contém a falha de
compilação; Run2 (`../evidence/knowledge-cdb6/qualification-run-2.json`; historical local artifact unavailable), duas
expectativas incorretas do SDK; Run3 (`../evidence/knowledge-cdb6/qualification-run-3.json`; historical local artifact unavailable),
os 19 testes aprovados e dois lints antes da correção idiomática.
Os oráculos atuais verificam o item sintético após `-` com span vazio e o
fechamento incompatível localizado na abertura `[` (coluna Unicode três,
offset YAML em bytes quatro), mantendo nó, span e causa exatos.
run-4-candidate.json (`../evidence/knowledge-cdb6/run-4-candidate.json`; historical local artifact unavailable)
registra a Source posteriormente executada em Run4. `rustfmt` não substituiu
nenhuma verificação funcional ou de lint.

## Entrada e resultado

```rust,ignore
pub fn parse_document<'a>(
    context: &SnapshotContext,
    input: &'a [u8],
    fallback_title: &str,
    limits: DocumentParseLimits,
) -> Result<ParsedDocument<'a>, DocumentParseError>
```

O chamador captura os bytes e fornece o contexto da requisição. O parser verifica
cancelamento e o prazo absoluto antes de admitir os dados, entre linhas/eventos,
ao construir nós e textos e antes de retornar. Usa o contexto original, sem
renovar prazo ou cancelamento, e não realiza I/O. Os limites documentais admitem
bytes, nós, eventos e textos. Bytes e árvore sintática não demonstram autoridade
de acesso nem identidade de captura.

`ParsedDocument` empresta `source`, a fonte integral original, sem normalizar
BOM, line endings ou corpo. Contém `envelope`, `metadata`, `markdown`, `title`,
`summary`, `summary_truncated` e `stats`. Todos os ranges são bytes da fonte
integral, em boundaries UTF-8. Ranges ausentes, fora dos limites ou inválidos
produzem `InvalidSpan`, sem ajustes silenciosos.

`DocumentEnvelope` identifica o BOM opcional, o frontmatter completo com
delimitadores/newline, apenas o payload YAML e o corpo. Um único BOM inicial é
reconhecido. Uma segunda BOM fica no corpo e impede o opener exato. A primeira
linha exatamente `---` abre frontmatter; admite LF, CRLF ou EOF. Opener em EOF
retorna `UnclosedFrontmatter`. O closer é uma linha exatamente `---` ou `...`,
inclusive em EOF. Espaços extras não são delimitadores. O corpo começa após o
newline do closer ou em EOF. O cap de frontmatter conta também os delimitadores,
excluindo a BOM, e é verificado antes da construção do parser YAML.

## YAML sem resolução de schema

`Metadata` guarda uma arena privada e um `root: Option<NodeId>`. `nodes()` expõe
uma slice e `node(id)` consulta um índice; `NodeId` tem construtor privado e
`index()` de leitura. Nós são `Scalar { value, style }`,
`Sequence { items }` e `Mapping { entries }`, preservando a ordem da fonte.
Os filhos são índices, portanto descartar a arena não envolve um Drop recursivo.

O escalar conserva texto decodificado, estilo e span bruto do SDK. O texto não
precisa coincidir com o source slice: escapes, scalars multiline e valores
omitidos são exemplos. Granit pode sintetizar `~` para um valor omitido com span
vazio ou do delimitador `:`/`-`. Não há detector `implicit`, coerção numérica,
booleana ou nula no resultado. Coleções usam o intervalo da abertura até o fim
do fechamento correspondente; closings block com range vazio são legítimos.

São aceitos um root mapping ou payload vazio/comments-only, cujo root é `None`.
`{}` produz um mapping vazio presente. Chaves devem ser escalares textuais.
Duplicatas são comparadas pelo texto decodificado, case-sensitive, em cada
mapping. Plain empty, `~`, `null`, `Null` e `NULL` são chaves inválidas; quoted
`null` e quoted empty são chaves textuais permitidas. Spellings numéricos de
chaves não são convertidos nem tratados como identidades de schema.

Anchors, aliases emitidos, tags explícitas — inclusive core tags — e a chave
decodificada `<<` são recusados. Símbolos em strings comuns são preservados.
Um alias desconhecido pode falhar no SDK antes da emissão; nesse caso permanece
um erro YAML nativo. O segundo `DocumentStart` retorna `MultipleDocuments`.
Um separator YAML `--- # second` dentro do payload não é um closer do envelope
e exercita essa regra. YAML malformed falha integralmente, com ou sem references.

Unknown fields e `references` conservam seus tipos sintáticos. Ausência,
`references: []`, lista preenchida, null e mapping permanecem distinguíveis.
O resultado não é o tipo canônico Records nem executa Preserve/Replace,
validação de IDs/relation/scope ou publicação. O futuro consumidor deve validar
as referências authored antes de modificar o corpus, conforme
`Persisted_record_relationships`.

## Markdown e seleção editorial

O parser habilita apenas tables e wikilinks. Retorna os eventos nativos de
`pulldown-cmark` com os ranges deslocados pelo começo do corpo. Não reescreve
markup nem renderiza HTML. Code blocks, inline code, HTML, links, imagens e
destinos permanecem na sintaxe ou fonte. Cada `Start(Link)` debita um link bruto,
incluindo autolink, email, wiki, repetido e self; não há deduplicação nem resolver.
Imagens contam nos eventos/textos, não em links. Unresolved reference links sem
definition não recebem callback que os transforme em destinos resolvidos.

O título usa `title` scalar não vazio/non-null do mapping raiz, depois o primeiro
H1 com texto não vazio, depois o fallback aparado. O resumo usa `description`
scalar não vazio/non-null, depois o primeiro Paragraph elegível, depois `""`.
Heading, code block, table e HTML block não são prosa elegível. O primeiro
parágrafo elegível é selecionado mesmo quando seu texto é vazio. Inline HTML
é ignorado na seleção de texto; Text/Code são concatenados e breaks viram espaço,
conservando texto de links e alt text. Somente as saídas editoriais são aparadas.

`title`/`description` list ou mapping são `InvalidFieldType`. Plain null/`~` e
texto vazio usam o próximo candidato; quoted `null` é texto literal. Título não
é truncado. Resumo corta um prefixo em boundary UTF-8 sob `max_summary_bytes`,
inclusive para uma description explícita; `summary_truncated` indica descarte.

## Limites e erros

Todos os dez limites são `NonZeroUsize` obrigatórios, sem defaults de produto.

| Limite | Trabalho contado |
|---|---|
| `max_document_bytes` | Bytes integrais do input, antes de UTF-8/parsing. |
| `max_input_bytes` | Soma checked de input e fallback bruto. |
| `max_frontmatter_bytes` | Envelope inteiro, excluindo BOM. |
| `max_yaml_events` | Cada evento emitido, inclusive stream/document e Start/End. |
| `max_markdown_events` | Cada evento emitido, inclusive markup protegido. |
| `max_yaml_depth` | Peak combinado de mappings/sequences antes do frame push. |
| `max_scalar_bytes` | Texto decodificado de cada scalar YAML e fallback bruto. |
| `max_links` | Cada `Start(Link)`, antes de armazenar o evento. |
| `max_projection_bytes` | Ledger cumulativo de textos retidos e cópias. |
| `max_summary_bytes` | Prefixo editorial UTF-8 do resumo. |

O ledger de projeção debita texto de todos os nós e campos textuais de eventos,
mesmo emprestados; cópias de chaves usadas para detectar duplicatas; buffers de
seleção editorial; título e resumo finais. Cada cópia é debitada novamente antes
de clone/append/store, sem refunds. Fonte emprestada e ranges não debitam o
documento integral outra vez. Nodes/edges/frame stack são limitados por eventos.
`ParseStats` expõe eventos, peak de depth, links e ledger final; não mede RSS.

Somas e ranges usam operações checked. As reservas próprias de Vec/String e
HashSet são fallible; falhas retornam `Request(OutOfMemory)`. Isso não promete
recuperar toda falha de alocação interna do SDK. A emissão já pode ter alocado
texto decodificado e o primeiro passe Markdown é síncrono. Admissão dos bytes,
checks de contexto e limites nativos ajudam a limitar o trabalho, sem hard RSS
ou preempção de cada chamada individual.

Granit recebe opções explícitas: comentários sem emissão/buffer, simple-key
lookahead de 1024 caracteres da gramática YAML, flow/block nesting iguais ao
cap de depth, directive bytes iguais ao cap frontmatter e parâmetros de reserved
directive iguais ao cap de eventos. Native scanner caps continuam erros YAML
nativos, distinguíveis do limite lógico de depth combinado.

Erros são `LimitExceeded`, `InvalidUtf8`, `UnclosedFrontmatter`,
`Yaml { source, location }`, `InvalidFieldType`, `DuplicateKey`,
`UnsupportedFeature`, `MultipleDocuments`, `InvalidSpan` e `Request`.
O display não inclui snippets do documento. `source()` conserva Utf8Error,
ScanError ou io::Error. YAML locations usam linha um-based, coluna Unicode
zero-based e linha original com deslocamento dos LF anteriores ao payload.
Nenhum erro retorna projeção parcial ou tenta parsing permissivo alternativo.

## SDKs e verificação

SDKs exatos: `serde-saphyr = 1.3.0`, sem defaults e com `deserialize`, apenas
para seu reexport `granit_parser`; `granit-parser = 1.3.0` transitive já locked;
`pulldown-cmark = 0.13.4`, sem defaults/html/getopts/simd/serde. A união de features
de outros consumidores no workspace não é subtraída pela edge local.

Os archives oficiais inspecionados e seus SHA-256 são:

| Source primária | SHA-256 | Licença / MSRV |
|---|---|---|
| [Granit 1.3.0](https://crates.io/api/v1/crates/granit-parser/1.3.0/download) | `e20f99e46474f56bd905c56e817ebddcf377a611f94c53ac4649e4d3fa3c0cd0` | MIT ou Apache-2.0 / 1.81 |
| [pulldown-cmark 0.13.4](https://crates.io/api/v1/crates/pulldown-cmark/0.13.4/download) | `e9f068eba8e7071c5f9511831b44f32c740d5adf574e990f946ddb53db2f314e` | MIT / 1.71.1 |

As fixtures protegem envelope/BOM/CRLF/EOF, scalars/styles/spans de coleções,
omissões e references sintáticas, YAML estrito, diagnósticos Unicode, Markdown
rico/protegido, precedência editorial, todos os caps, ledger de cópias, prefixos
UTF-8, arena profunda, cancelamento/prazo e zero I/O, overflow e ranges inválidos.
Cancelamento entre unidades é inspecionado; testes não alegam comprovar uma
interrupção dinâmica dentro de chamadas síncronas do SDK.

Na ativação vigente, executar somente na janela combinada com Root:

```text
cargo test --locked -p graphit-knowledge --lib document::tests -- --test-threads=2
cargo test --locked -p graphit-knowledge --test document -- --test-threads=2
cargo clippy --locked -p graphit-knowledge --all-targets -- -D warnings
```

A fonte contém 16 unitários e nenhum doctest. Três integrações da API pública
estão publicadas em `tests/document.rs` e descritas no
pacote de ativação (`../evidence/knowledge-cdb6/README.md`; historical local artifact unavailable). O exemplo `rust,ignore` acima não é prova de compilação.
A qualificação focada evita repetir References, Staleness e Settings.

O principal qualificou o guard Integration `graphit-knowledge/document` no
catálogo rápido de 37 fases. A prova observou três testes públicos PASS e
exatamente uma falha `broken_fast_contract`, Cargo EXIT101 e
`git hook run pre-commit` EXIT1, seguida de restauração positiva das 37 fases.
Os 929 arquivos copiados e 146 diretórios ficaram invariantes; houve isolamento
da cópia compilada, todos os filhos diretos foram aguardados, scratch residual
vazio e remoção do TMP privado. A Source atual mantém os hashes do Run4:
`document.rs` `7a81ba22d3775fe7a5a19f50c36af2bff31f359df87dcf42ec6e510ffe324d0a`
e `tests/document.rs`
`1cd6c6e7fc61ef96f04a1c94c828cce08beb014fe5be0d537b1be90b3cc8d67d`.

Receipt do guard:
`.build/rust-core-birth-anchor-20261009/root-qualification/two-new-current-guards-1791626157505598000/receipt.json`,
SHA-256 `1b1a0bf7ecfb90df338c192d97263b4f6ea4c6d177a5e613bcdcf4185cee3074`.
Seção matching Document:
`823cf8f8dae2647c8ae9e5bd7a4c4d1e488c5e35231f2fd787789573316338c6`;
raw log `90de918b1e7aee82caa449dc9ad33376c8a4bce9d1ead6cfe1bce277070f5902`;
trace Git `5ec3fd4c75ac1ef1025c6deac7587037f12c36b69e48ef3fafb7bcd49ae70fdf`.
O driver conservou SHA-256
`87d9397792c1a26d06f303d88a3d0412bff48ae16a892c048690df3b06ca14b4`.
O primeiro grupo de dois guards terminou em `FAULT` por ordem de execução de
outro alvo; seu recibo SHA-256
`d819a919e3f9ceda7d54430cdee7f643c0e68eef7136590b09e7f0e03f808efb`
permanece histórico e não foi convertido em PASS. Os antigos guards de
`project_document`, Settings ou staleness não eram a contraprova deste parser.

Os 16 unitários, três integrações e Clippy do Run4 continuam provas funcionais
separadas; o teste negativo público não é uma nova execução dos 16 unitários.
Nenhum novo Cargo ou alteração de Source foi feito nesta atualização documental.
A revisão de consistência confronta os mesmos contratos e fixtures nos dois
sentidos. A prova Linux não qualifica Windows/macOS, preempção de SDKs,
quiescência interna, desempenho de produto ou a migração global.

Para manutenção do guard, na janela combinada com o principal:

```text
cargo xtask verify-fast-guards graphit-knowledge/document
```

As demais fronteiras continuam em [Knowledge/Wiki](rust-knowledge-wiki.md),
[Data/Records](rust-knowledge-data-boundaries.md) e
[AI/Live](rust-knowledge-ai-live.md). Discovery, identidade/domínio, corpus,
geração, busca, pacotes e lifecycle não são implementados por este parser.

## Adaptação privada Core65

A qualificação privada Core65 de tsk-87f7 passou no Linux: 151 testes
funcionais/unitários, sete doctests, Clippy estrito nos targets afetados e
formatação dos 18 arquivos Rust. O patch aguarda composição serial pelo principal
com o produtor Core. As qualificações Main anteriores permanecem históricas.
