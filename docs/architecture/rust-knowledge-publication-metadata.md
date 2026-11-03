---
type: architecture
title: Metadados editoriais de captura nativa em Rust
description: Compilação editorial de FILE retido ou buffer externo validado pelo Core, com borrowing e requisição original.
references:
  - target: {type: task, id: tsk-87f7}
    relation: implements
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-db76}
    relation: implements
  - target: {type: task, id: tsk-41ba}
    relation: implements
  - target: {type: task, id: tsk-b210}
    relation: derived_from
  - target: {type: task, id: tsk-cdb6}
    relation: uses
  - target: {type: task, id: tsk-9df1}
    relation: uses
  - target: {type: task, id: tsk-6ded}
    relation: refined_by
---

A factory compila metadados editoriais imutáveis de um FILE capturado em
CapturedSourceReader. Pode consumir bytes retidos no reader ou um buffer externo
validado contra essa captura. CompiledDocument empresta o reader e os bytes
usados; o consumidor conserva ambos os owners. Publicação,
seleção completa, avaliação Current e geração por domínio têm produtores próprios.

## Jornada e API atual

Conservar o reader e o mesmo Arc<SnapshotContext> usado na captura. Fornecer o
Path exatamente como registrado, policy owned e limites positivos. Um arquivo
capturado como File tem Path::new("") na raiz interna do reader, sem nova seleção.
Arquivo vazio retido continua válido. O resultado só existe após sucesso integral:

~~~rust
pub fn compile_captured_document<'a>(
    reader: &'a CapturedSourceReader,
    relative: &Path,
    request: &Arc<SnapshotContext>,
    policy: PublicationMetadataPolicy,
    parse_limits: DocumentParseLimits,
    limits: PublicationMetadataLimits,
) -> Result<CompiledDocument<'a>, CompilationMetadataError>;
~~~

`compile_captured_document` usa os bytes retidos no reader e não acrescenta
validação física do arquivo. Continua válida após remoção do FILE físico,
desde que os bytes retidos e o Original permaneçam válidos.

Para um buffer externo pertencente ao caller:

~~~rust
pub fn compile_captured_file_bytes<'a>(
    reader: &'a CapturedSourceReader,
    relative: &Path,
    bytes: &'a [u8],
    request: &Arc<SnapshotContext>,
    policy: PublicationMetadataPolicy,
    parse_limits: DocumentParseLimits,
    limits: PublicationMetadataLimits,
) -> Result<CompiledDocument<'a>, CompilationMetadataError>;
~~~

Essa API exige o FILE registrado e chama Core `validate_file_bytes`, que valida
path, tamanho, SHA, binding e Original. O resultado empresta reader e buffer;
não exige retenção dos bytes no reader. As factories compartilham o kernel
privado de compilação editorial.

O contexto original conserva identidade, cancelamento e prazo.
`CompiledDocument::check_request` delega a `reader.check_request(request)`.
Uma request estrangeira é rejeitada, mesmo com prazo e configuração iguais.
Digest e produto compilado não concedem autoridade.

PublicationMetadataPolicy contém fallback_title e kind_on_absent String
explícitos. O produto conserva essa policy e metadados owned com RAII normal;
suas Strings de entrada já foram alocadas pelo caller. Não há NativePermit,
resource/work, quotas universais ou constructors alternativos de autoridade.
CompiledDocument tem campos privados e não oferece Clone, Deserialize, Default,
FromParsedDocument, construtor público ou getters mutáveis.

| Getter | Resultado |
| --- | --- |
| whole_source() | &str UTF-8 integral emprestada dos bytes retidos ou do buffer externo validado |
| title() / kind() | valores editoriais efetivos |
| description() | Option<&str> integral, None distinto de Some("") |
| summary() / summary_truncated() | projeção e flag literais do parser |
| physical_source_digest() | digest físico opaco de 32 bytes do File real |
| provenance() | spans e estados editoriais conhecidos |
| recipe_policy() / summary_limit() | policy exata e limite de summary em u64 |

## Regras editoriais e YAML

Title segue literalmente o [parser documental](rust-knowledge-document-parser.md):
title explícito não-null/nonblank, depois primeiro H1, depois fallback fornecido,
com trim editorial existente. Title vazio é válido. Title plain numérico,
.nan, .inf e 1e999 conserva o texto editorial aceito pelo parser; a admissão
estrita de description/type não reclassifica title.

Description é decodificada integralmente pelo serde-saphyr@1.3.0:

| Valor authored | Description efetiva |
| --- | --- |
| ausente ou null | None |
| `""` | Some("") |
| `'   '` | Some("   ") |
| `"  α\nβ  "` | Some("  α\nβ  "), nove bytes UTF-8 |
| block literal/folded | valor conforme folding/chomping do SDK |

Não aplicar trim ou prefixo adicional à description. O layout YAML original
permanece no whole_source. Summary continua a projeção editorial do parser;
com a description de nove bytes acima e max_summary_bytes=2, o resumo é α e a
flag de truncamento é true, enquanto description conserva os nove bytes.

Type authored textual vira kind integral: vazio e texto custom como
`domain/a 🍀` são preservados. Type ausente/null usa somente kind_on_absent
explícito. O valor de kind é explícito e independente do caminho. Data
consome o resultado; não preenche esse valor novamente.

A projeção privada usa no_schema=true e strict_booleans=true. Known
description/type plain 123, true, TRUE, .nan e .inf rejeitam a compilação;
textos quoted equivalentes são admitidos. SDK1.3.0 trata plain1e999 como String
literal; yes/on também são texto com strict_booleans. Essa fronteira é do SDK,
sem regex ou gramática numérica concorrente.

Parser primeiro rejeita duplicatas sintáticas decodificadas, inclusive keys
quoted conhecidas. Somente depois a projeção privada usa duplicate_keys=FirstWins
para tolerar equivalências adicionais do SDK entre keys desconhecidas, como
plain0xB/11. Isso preserva o corpo e os campos conhecidos. Merge keys, tags,
anchors, aliases e documentos múltiplos já são negados pelo parser. O decoder
usa merge_keys=Error, legacy_octal_numbers=false, reject_unsupported_tags=true,
emit_comments=false e with_snippet=false. reject_non_finite_typeless_float=false
permite ignorar title/unknown/nested .nan/.inf; known strings continuam estritas.

Sem frontmatter, frontmatter vazio ou só comentários tem metadata.root=None;
a compilação conserva title/summary e policy sem invocar um mapping sintético
do SDK. Root inválido conserva o erro do parser.

## Proveniência e integridade

PublicationProvenance fornece title(), description() e kind() como
FieldProvenance. Title é Unknown: igualdade do valor não prova sua autoria.
Description ausente é Absent; presente é AuthoredScalar com span original e
Text ou Null. Kind textual é AuthoredScalar/Text; policy fornecida é SuppliedKind,
com authored_null_span quando houve null. NodeId e arena parser não sobrevivem
à compilação; os spans são dados, sem reconstrução de SourceWitness.

Não há quota `max_captured_nodes` nem preflight que limite `nodes().len()`.
Cada fronteira controlada verifica o contexto, incluindo busca do FILE, parser,
entradas SDK e completion final. O Path exato precisa corresponder a File com
digest Some. MissingCapturedFile, NonFile e MissingPhysicalDigest conservam seus
erros próprios.

A factory retida também exige retained Some e distingue UnretainedFile. A factory
de buffer externo valida os bytes pelo Core e conserva falhas dessa validação em
PhysicalSource. Nenhuma factory copia o body, retém a arena parser ou aceita
ParsedDocument do caller como prova.

Na jornada por FILE, uma passagem significa materialização local, compilação
editorial e encode uma vez por FILE. Isso não promete uma única execução de
parser ou um único I/O físico: rechecks Core e reparses locais que protegem
inputs ordinary permanecem.

## Limites e erros

PublicationMetadataLimits mantém quatorze capacidades positivas:
input bytes, title, kind, description, effective summary, effective metadata,
owned metadata, root entries e budgets SDK de nodes/depth/scalar bytes/events/
lookahead characters/flow depth. As somas usam checked_add. Input inclui whole
raw body + fallback + kind policy. Effective soma title + kind efetivo + description
+ summary; owned soma policy + title + summary + description + kind authored,
sem duplicar kind fornecido pela policy. None/Someempty têm zero bytes e presença
semântica distinta. Os limites não contabilizam overhead/RSS do allocator.

O parser tem seus limites próprios. Seus title/summary são movidos, e os limites
posteriores não impedem alocações anteriores sob o limite parser. A decodificação
privada usa budgets maduros do SDK. Lookahead mede caracteres independentemente
de bytes UTF-8. Na fixture '# H\n\nbody\n', effective10 e owned11 passam;
9/10 respectivamente falham. Não há soma de reservas artificiais nem probes de
headroom. Política/body/SDK continuam limitados por seus contratos reais.

OriginalRequest conserva io::Error e kind reais dos checks de request, incluindo
Interrupted, TimedOut ou InvalidInput de contexto estrangeiro. PhysicalSource
conserva o io::Error real de Core validate_file_bytes e o expõe por Error::source.
Parser e TypedMetadata conservam suas causas. Limit/Overflow são tipados; MetadataMismatch rejeita divergência entre
valor decodificado e sintaxe. O bridge Serde conserva a primeira causa local
em slot privado; mensagens de vendor não classificam erro. Falha/cancelamento/
prazo ao final descarta o candidato inteiro. Os checks são cooperativos e não
prometem preempção síncrona do SDK nem recuperação de OOM.

## Verificação e continuidade

A corretiva tsk-db76 adapta somente Source, fixtures e este contrato ao Core MAIN
já promovido, mantendo a política dos produtores tsk-41ba/tsk-25a0. APIs futuras
de tsk-2706 ficam fora deste packet. Comandos da unidade:

~~~text
cargo test --offline --locked -p graphit-knowledge --test publication_metadata --test compilation_fingerprint
cargo test --offline --locked -p graphit-knowledge --lib publication_metadata::tests
cargo test --offline --locked -p graphit-knowledge --lib compilation_fingerprint::tests
cargo clippy --offline --locked -p graphit-knowledge --lib --test publication_metadata --test compilation_fingerprint -- -D warnings
~~~

A revisão conserva testes de valores/YAML/proveniência, limites exatos e abaixo,
whole-body/noIO, denials, duplicate keys, File vazio, cancelamento original,
foreign e completion real. Deadline é exercitado após captura válida: o contexto
original expira e a factory retorna TimedOut sem IO/resultado. Testes de permits,
reservas e work removidos não representam obrigações atuais.

A API está integrada ao Knowledge principal. Em Linux, passaram os 19 testes
públicos de fingerprint/metadados, suas 10 unidades, Clippy com `-D warnings`
e formatação. Estes resultados cobrem o contrato puro de compilação; publicação
e persistência têm suas entregas próprias. Windows e macOS não foram executados
nesta unidade. Ver [parser](rust-knowledge-document-parser.md),
[fingerprint](rust-knowledge-compilation-fingerprint.md) e
[avaliação de staleness](rust-knowledge-staleness-projection.md).

## Adaptação privada Core65

A folha tsk-87f7 adapta fixtures e exemplos ao contrato Core65 SourceReady.
SnapshotContext::new(deadline, Cancellation) é infalível; SourceReadLimits
recebe somente range_bytes, block_bytes e stream_bytes. O contexto original
conserva identidade, cancelamento e prazo.

A qualificação privada Core65 de tsk-87f7 passou no Linux: 151 testes
funcionais/unitários, sete doctests, Clippy estrito nos targets afetados e
formatação dos 18 arquivos Rust. O patch aguarda composição serial pelo principal
com o produtor Core. As qualificações Main anteriores permanecem históricas.

Em Unix, os casos existentes removem o caminho físico depois da captura
e antes de compilar. Bytes retidos, borrowing e digests normativos permanecem
verificados; o teste prova independência de reabrir o path, sem instrumentar
todos os syscalls. Os contratos de recipe e limites intrínsecos são preservados.
