---
title: Rust AST Symbols and Identity
description: "Rust root/exact identities, original ranges, ownership and bounded serialization."
content-type: reference
audience: developers
references:
  - target: {type: task, id: tsk-afac}
    relation: reuses
  - target: {type: task, id: tsk-b0cb}
    relation: derived_from
  - target: {type: task, id: tsk-0fee}
    relation: implements
  - target: {type: task, id: tsk-2cd3}
    relation: implements
  - target: {type: task, id: tsk-0006}
    relation: derived_from
  - target: {type: task, id: tsk-e056}
    relation: uses
  - target: {type: task, id: tsk-13ae}
    relation: uses
  - target: {type: task, id: tsk-0395}
    relation: uses
  - target: {type: task, id: tsk-e4a9}
    relation: uses
  - target: {type: task, id: tsk-0ac6}
    relation: consumes
  - target: {type: task, id: tsk-7391}
    relation: reuses
  - target: {type: task, id: tsk-6f08}
    relation: relates_to
  - target: {type: task, id: tsk-3f5d}
    relation: derived_from
  - target: {type: task, id: tsk-2706}
    relation: uses
  - target: {type: task, id: tsk-98e7}
    relation: uses
---

# Identidade sintática Rust

Projeto graphit-code; sessão ses-c98a; migração tsk-7858. A folha `tsk-3f5d`
removeu a contabilidade artificial desses produtores. Os testes afetados em MAIN, Clippy e formatação passaram; os sete checks duráveis
passaram após a revisão final da documentação. Receipts históricos não
qualificam a API alterada. Extração nativa e E01–E10 pertencem a `tsk-b0cb`.

## Resultado, ownership e produtores

A identidade root/exact contém documento lógico, linguagem, grammar, label,
anchor do sujeito e owners declarados. Extents/propriedades/captura nativa,
resolver de owners, merger/relações, SCIP, storage e publicação têm produtores
próprios. Keys iguais permitem agrupar ocorrências, sem resolver conflito de
propriedades. Reutiliza ranges, contexto original, decoder, catálogo e
PreparedInput já existentes. O wire usa serde_json maduro.

## API pública e lifetimes

IdentityLimitsInput possui seis campos usize: source_bytes, path_components,
path_component_bytes, identity_text_bytes, owner_depth e uid_bytes.
IdentityLimits::new(input) valida valores positivos; path_components e
owner_depth devem caber em u32. Retorna InvalidLimits com o campo correspondente,
na ordem acima. IdentityLimits é Copy, opaco e fornece getters dos seis campos.

IdentityDocumentInput<'o> empresta relative_components: &'o [&'o str],
language: &'o str, grammar: &'o str e original_bytes: &'o [u8].

```rust
pub fn new(
    input: IdentityDocumentInput<'o>,
    limits: IdentityLimits,
    original: &'request SnapshotContext,
) -> Result<IdentityDocument<'o, 'request>, IdentityError>;

pub fn symbol<'doc>(
    &'doc self,
    label: &str,
    subject: SourceRange<'o>,
    owners: &[&SyntaxIdentity<'doc, 'o, 'request>],
) -> Result<SyntaxIdentity<'doc, 'o, 'request>, IdentityError>;
```

O documento retém metadados, bytes e SnapshotContext originais. `symbol` usa o
contexto retido. new/symbol verificam cancelamento e deadline durante a operação.
Interrupted mapeia Cancelled; TimedOut mapeia DeadlineExceeded; demais falhas
mapeiam ContextDenied. Falhas não retornam identidade parcial.
SyntaxIdentity expõe documento, label, subject, context e UID por getters.
Seu UID possui as_str(); frames de contexto possuem label/start/end imutáveis.
Owners são emprestados durante a chamada; frames são copiados para a filha,
que não retém referências às próprias ownerkeys. Documento, metadados, bytes e
contexto permanecem vivos enquanto a identidade os empresta.

## Valores, precedência e binding
Components: array não vazio, <=path_components; cada componente não vazio, sem NUL, sem "/" interno e diferente "."/"..", <=path_component_bytes. Backslash, colon, aspas, Unicode e espaços permanecem exatos, sem OS conversion/trim/casefold/repair. É identidade lógica relativa, não caminho físico portável; o produtor físico deve governar admissibilidade OS antes integração.
language, grammar e label: texto UTF-8 não vazio, sem NUL e sem byte ASCII whitespace no primeiro/último byte; cada texto <=identity_text_bytes. Byte interno e Unicode não são normalizados. Predicate rejeita, jamais corrige. language é InvalidDocument@language; grammar InvalidGrammar; label InvalidLabel. Os owners já possuem labels admitidos neste mesmo documento/limites.
new verifica o contexto retido antes de limites e conteúdo; operações futuras usam esse mesmo contexto original.
Depois do contextcheck, new valida counts/len, caps e predicados dos componentes/language/grammar; source.len é limitado antes de SourceText::new(original_bytes, limits.source_bytes()). new não cria buffers próprios. Empty originalsource é admissível, mas não suporta symbol não vazio.
symbol verifica o contexto primeiro e limita owners antes da iteração. Ordem semântica: label len/cap/predicate; subject binding; ownerchain binding/prefix; wire preflight/cap; reservas/cópias/output. Falha não retorna key parcial.
SourceRange só vem de actual SourceText::range ou actual PreparedInput Exact. Invalid/reversed/UTF8split range não pode ser fabricado via DTO.
Subject exige start<end. Calcular original_bytes.get(start..end) usando slice de bytes, não &str, após bounds/constants e checked end-start. Comparar pointer e len desse subslice com subject.text().as_bytes(): mismatch -> InvalidAnchor (field/index None; o enum fechado não possui Subject), SEM chamar SourceText::range ou escanear UTF8 do original. Nenhum pointer arithmetic unsafe, comparação de conteúdo ou DTO.
Assim original "α" e foreign ASCII.range(0,1) retorna InvalidAnchor, mesmo que 1 corte o UTF8 original. Equal bytes em duas alocações distintas também rejeita; zero-length é rejeitado antes pointer, eliminando ambiguidade de empty buffers.
Somente após binding, carregar upperbound do range e invocar SourceText.range(start,end); checar offsets, text pointer/len e BytePoints exatos contra subject. Mismatch de points vindos de SourceText sobre subslice com offsets relativos -> InvalidAnchor. Unexpected actual RangeError neste passo -> InvariantViolation com range_error tipado. Não inferir captura/extent/sourcegeneration a partir de ponteiro.
Owners: outer→inner. Para cada owner_i, ptr::eq(owner.document(), self); owner_i.context corresponde exatamente aos frames de owners[0..i]. Conferir label/start/end de cada frame, com comparação de texto e contexto verificado. Rejeitar cadeia truncada/trocada, owner estrangeiro, frame repetido ou owner cujo label/start/end é igual ao novo subject: InvalidContext@owners com índice i. Não exigir que parentkey continue vivo após call. O factory não prova containment; native owner resolver deve qualificar fullentityextent strictcontainment e syntaxkind. Não chamar keys manuais de resolver entregue.
Fonte física, geração, sourceversion e namespace project/logicalversion/Data exactview são companions externos reais. Mesma key em geração posterior tem mesmo UID lógico, mas não demonstra autorização/freshness. Domínios Syntax e SCIP permanecem separados.

## Wire UID root/exact
Tupla JSON compacta fixa, serde_json::to_writer com CompactFormatter normal:
["ast-syntax-root-1",[components...],language,grammar,label,[start_u64,end_u64],[[owner_label,start_u64,end_u64],...]]
Tudo em ordem e inteiro u64; cada usize→u64 checked. Sem maps arbitrários, joins com delimiters, hashes, digest de source, nome, extent, vendorDisplay, recursive ownerUID ou JSON Value owned. Uma única borrowed WireOwners adapter implementa serde::Serialize por serialize_seq(Some(owners.len())) e serialize_element da tupla (label,start_u64,end_u64); é glue fechado para o SDK maduro, sem formatter/escape/parser próprio ou callbacks de usuário. Outer/path/subject tuples usam serde normal. Prevalidar cada conversão antes cada passe; nenhuma Vec de tuples temporária é alocada para preflight. Não serializar as structs públicas com todos os campos nem guardar UID recursivo de owner.
O tag define domínio/version root/exact, não uma enum de um único caso para futuras stubs. Origin Compound/Synthetic/Ambiguous/Unmappable nunca são coerced para span. Exact projection é consumível nesta folha somente quando o teste declara root/mesmo buffer do mesmo buffer original; isso não qualifica embedding ou transformertruth. E07/E08 exigem contratos de origin/unidade próprios.
Primeiro passe counting Write sem ownedoutput determina comprimento exato L; verifica uid_bytes DURANTE cada chunk e no final. Segundo passe idêntico para Vec<u8> reservado fallibly em L exato, sem crescimento além da reserva validada; final output.len == L, depois String::from_utf8 movendo o Vec. Não há clone/cópia final da string. UID vazio seria InvariantViolation, não sucesso.

## Serializer limitado e erros

Offsets de subject e owners são convertidos de usize para u64 com verificação
antes de serializar; overflow retorna ArithmeticOverflow.
O UID usa a tupla fechada acima e serde_json. BoundedWriter conta o comprimento
exato e verifica uid_bytes em cada chunk. O segundo passe escreve no Vec
reservado com try_reserve_exact; count, cap, contexto e tamanho final permanecem
verificados. String::from_utf8 move o Vec. Labels/frames usam reservas falíveis.

BoundedWriter conserva first_error. A primeira falha tipada fica retida;
chamadas posteriores não modificam output. A sentinela IO é Other, pois
Write::write_all pode repetir Interrupted. O latch precede o resultado SDK;
erro SDK sem latch mapeia Serialization. Empty write retorna zero com latch
vazio; flush verifica latch/contexto. Diagnóstico vendor não é formatado.

IdentityError é Copy e possui code/field/index/range_error privados, getters
e Display estático. Codes: InvalidLimits, InvalidDocument, InvalidGrammar,
InvalidLabel, InvalidAnchor, InvalidContext, InvalidUtf8, LimitExceeded,
ArithmeticOverflow, Cancelled, DeadlineExceeded, ContextDenied, Allocation,
Serialization e InvariantViolation. Fields: SourceBytes, PathComponents,
PathComponentBytes, IdentityTextBytes, OwnerDepth, UidBytes, Language, Grammar,
Label, Owners e Uid. Limites estruturais retornam LimitExceeded; falha real de
try_reserve_exact retorna Allocation; conversões usam ArithmeticOverflow.
SourceText InvalidUtf8 conserva RangeError tipado; falha inesperada após
validação retorna InvariantViolation. owner_depth positivo permite owners[].

## Fixtures F01–F06

expected-identity.json conserva 15 tuplas/UIDs literais independentes. Os testes
comparam tupla, wire e pontos originais sem gerar expected pelo produtor.
F01: Go27, Run18..21; paths a.go/b.go distinguem documentos e replay é estável.
F02: JavaScript36, anchors9..12/27..30; BOM+α CRLF tem len26, Run16..19,
row1 cols9..12. F03: [a::b,c.go] versus [a,b::c.go], quote/backslash/Unicode/case
permanecem distintos e exatos. F04: JSON14, Pair2..3/8..9 e Value5..6/11..12,
com owners correspondentes; repeatedkey conserva UID lógico.

| F05 ação | Resultado |
|---|---|
| Cada limit zero; counts além u32 quando representáveis | InvalidLimits com campo exato |
| Cada cap exato ou um abaixo da entrada | Sucesso ou LimitExceeded com campo |
| Components vazios/dot/dotdot/slash/NUL; language/grammar/label inválidos | Erro de domínio, sem normalização |
| Reversed/out-of-bounds/UTF8 boundary | RangeError real antes de symbol |
| Empty range, foreign buffer igual, foreign ASCII contra Unicode, subslice points | InvalidAnchor antes de reprojeção indevida |
| Owner de outro documento, prefixo errado, repetido ou self-owner | InvalidContext com índice |
| Contexto cancelado/expirado, inclusive depois de document.new | Cancelled/DeadlineExceeded pelo contexto retido |
| UID literal F01 | 74 bytes, wire exato |
| Writer cap/first-error/write_all | Erro tipado retido, sem efeito posterior ou retry loop |
| reserve Vec<u8>/ContextFrame com usize::MAX | Allocation e Vec vazio por capacityoverflow real |

A reserva negativa usa o helper privado de produção; não tenta um heap gigante
nem alega OOM físico. A antiga fixture serializer-work-oracles.json foi retirada
com a contabilidade. Os 15 expected-identity casos continuam verificando count
contra comprimento do UID literal, output exato e cap count-1.

F06 é a composição pública usada em tests/symbols.rs:
1. Criar Cancellation/SnapshotContext e limites estruturais finitos reais.
2. Decodificar Go/JavaScript/JSON com decode(input,&profile_limits,original) e
   construir CatalogDraft::build(layers,&catalog_limits,original).
3. Emprestar language/grammar e graph_label/value_label dos getters efetivos.
4. Construir SourceText e SourceRange reais dos bytes originais.
5. Criar IdentityDocument::new(input,limits,original) e symbol(label,range,owners).
6. Comparar tupla/wire independentes e indexar referências por UID em BTreeMap
   TEST ONLY, mantendo keys/documents/profiles vivos e sem mover storage emprestado.
7. Lookup testa UID ausente, vazio, desconhecido e conhecido sem trim/casefold ou
   busca por nome/localização. DuplicateUID só aceita a mesma tupla verificada.
8. Doctests rejeitam document/key/UID escapando de seus donos, mutação dos bytes
   originais e descarte prematuro do contexto. O caso positivo mantém lifetimes.
9. Compor a projeção Exact de PreparedInput com o mesmo buffer original:

```rust
let prepared = PreparedInput::new(original_bytes, prepared_bytes, plan, original_context)?;
let projection = prepared.project_prepared(18, 21)?;
```

O consumidor conserva a classificação tipada de projection. Somente
OriginalProjection::Exact(range) chama document.symbol("Function", range, &[]).
project_prepared usa &self e SourceRange empresta bytes originais.
Compound/Synthetic/Ambiguous/Unmappable não viram bounding box. O SourceText
interno de PreparedInput usa len.max(1), permitindo entrada vazia.
A composição Pure não entrega Data/UI USED nem native extractor.
O Core usa `SnapshotContext::new(deadline, cancellation) -> Self`; o contexto
retido continua verificando cancelamento e validade temporal. A composição
atual usa essa API de dois argumentos, sem contabilização artificial de I/O.
A folha B tsk-afac foi concluída r21: 29 testes públicos, quatro unitários,
Clippy estrito e formatação passaram; essa evidência pertence a B.

## Fontes primárias das fronteiras SDK

[Write::write_all](https://doc.rust-lang.org/std/io/trait.Write.html#method.write_all)
explica o tratamento de Interrupted.
[Vec::try_reserve_exact](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.try_reserve_exact)
explica reserva falível e capacityoverflow.
O wire usa [serde_json::to_writer](https://docs.rs/serde_json/latest/serde_json/fn.to_writer.html).

## Parent capabilities remain required

E01 homonyms/paths/labels/languages; E02 qualified owners/packages/table columns; E03 repeated captures/properties/relations; E04 parameters/fields/decorators/exports; E05 CSS/data formats/decoded values/errors; E06 comments/docstrings/complexity excluding nested declarations; E07 all embedded languages/host ordering/recursive limits; E08 mature transforms/full origin/diagnostics; E09 calls/imports/inheritance/implements/access/DML/custom relations; E10 actual SCIP handoff owned separately. Pure keys do not close these capabilities.

## Affected validation

Leaf A `tsk-3f5d` completed the accounting removal in MAIN. On Linux the affected
public suites passed: profiles 10/10, catalog 14/14 and symbols 9/9.
The affected private units passed 3/3, 5/5 and 5/5 respectively. The six
symbols doctests passed, including five compile-fail lifetime cases.
Strict Clippy for the library and three affected targets passed with -D warnings.
Scoped rustfmt of the six owned files and all seven durable checks passed.
These historical Linux results belong to that cleanup; they do not qualify another platform.

```text
cargo xtask cargo test -p graphit-ast --test profiles --test catalog --test symbols --jobs 1
cargo xtask cargo test -p graphit-ast --lib profiles::tests --jobs 1
cargo xtask cargo test -p graphit-ast --lib catalog::tests --jobs 1
cargo xtask cargo test -p graphit-ast --lib symbols::tests --jobs 1
cargo xtask cargo test -p graphit-ast --doc symbols --jobs 1
cargo xtask cargo clippy -p graphit-ast --lib --test profiles --test catalog --test symbols --jobs 1 -- -D warnings
```

Receipts are `.build/rust-migration/ast-cleanup-a-tests-03.log`,
`ast-cleanup-a-{profiles,catalog,symbols}-unit.log` and
`ast-cleanup-a-symbols-doc.log` and `ast-cleanup-a-clippy.log` in the same directory. Initial compile failures
are retained in tests-01/tests-02; stale Display/import and a byte literal in
mechanical call adaptation were corrected before the passing run.
The six owned Rust files use scoped `rustfmt --edition 2024 --check`.
Cache reuse and one Cargo job were used; no broader suite qualifies this leaf.

After the Core composition, `tsk-2cd3` checked the current symbols producer:
nine public tests, five private tests and six symbols doctests passed on Linux.
The doctests include five compile-fail ownership cases and one positive example.
Scoped Clippy for the library and symbols target passed with `--no-deps -D warnings`,
as did formatting of the two Rust files. These results preserve actual input/UID
bounds, allocation errors, ranges, owners, cancellation and lifetimes. They do
not restore the removed accounting tests or qualify the full AST pipeline.
