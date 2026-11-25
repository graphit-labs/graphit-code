---
title: Rust AST Symbols and Identity
description: "Compact logical identities, original ranges, ownership and declaration grouping."
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

## Primitivas compactas de identidade lógica Code

`graphit_data::CodeFileId`, `CodeUid` e `CodePrimaryKey` definem a nova receita
compacta consumida por `IdentityDocument::symbol`. A troca dos produtores e seus
consumidores deve ser qualificada conjuntamente. A receita JSON posicional antiga
não é um decoder de compatibilidade da API compacta.

| Valor | Conteúdo e representação |
|---|---|
| `CodeFileId` | SHA-256 de componentes lógicos relativos do FILE; não contém bytes do arquivo, branch, versão ou endereço físico |
| `CodeUid` | `u1:` e Base64 URL-safe sem padding de 48 bytes: hint FILE de 16 bytes e digest semântico SHA-256 completo de 32 bytes |
| `CodePrimaryKey` | `k1:` e a mesma forma de 48 bytes: hint FILE de 16 bytes e digest SHA-256 de UID completo e `data_key` literal |

UID e PK têm exatamente 67 bytes ASCII. `Display` e `FromStr` usam a biblioteca
`base64`; a leitura exige prefixo, comprimento e alfabeto canônicos. Não há JSON
aninhado, decoder da receita antiga ou restauração da origem por esses valores.
Cada campo do hash tem comprimento `u64` little-endian seguido dos bytes exatos;
essa moldura preserva fronteiras entre componentes e campos. Os domínios são
`graphit-code-file-1`, `graphit-code-declaration-1` e
`graphit-code-projection-1`.

`CodeDeclaration::Scip` recebe o conjunto não vazio, ordenado e sem duplicatas dos
símbolos globais originais realmente associados à mesma ocorrência, validados pelo
parser oficial SCIP, além de kind e ordinal de declaração. A primitiva rejeita
conjuntos fora dessa forma canônica. A ordenação/deduplicação aplica-se só ao seed;
os símbolos e fatos individuais continuam sendo observação e chave de associação/
exportação. Um símbolo pode corresponder a múltiplas declarações; vários símbolos
podem descrever uma ocorrência. Alterar esse conjunto altera sua identidade
semântica; o produtor não escolhe arbitrariamente o primeiro símbolo. Símbolos locais não têm promessa de estabilidade entre
indexações e usam o fundamento lógico de fallback. Fatos externos sem FILE
continuam no domínio SCIP original, sem inventar FILE ou UID desta receita.

`CodeDeclaration::Fallback` recebe linguagem, kind, owners lógicos tipados, nome
canônico (ou fingerprint observado do sujeito anônimo), tipo/assinatura quando
efetivamente disponível, indicador de anonimato e ordinal. Escopos podem ser
UIDs de declarações ou símbolos globais SCIP originais; são domínios distintos.
Tipos/assinaturas só participam depois do merge das contribuições e do consenso
entre as projeções da mesma declaração. Todos os textos precisam ser Exact e
iguais; Missing, NonExact ou conflito deixam esse componente desconhecido. Isso
não descarta observações nem divide um sujeito para esconder seus diagnósticos.
Scopes são refeitas pelo mesmo factory depois desse consenso. Os modelos
ANTLR/Tree-sitter genéricos não transportam tipo/assinatura comprovados; a
assinatura completa de callable também está ausente no carrier ANTLR especializado.

O ordinal pertence ao grupo de declarações logicamente indistinguíveis, nunca
a hits ou a projeções `data_key`. O produtor liga contribuições repetidas à mesma
declaração nativa antes de atribuí-lo. Ranges podem ligar/ordenar observações
transitórias, mas não entram no digest.

Inserir linhas ou comentários anteriores não altera os campos lógicos de uma
declaração intacta. Renomear ou mover o FILE, alterar nome/tipo/assinatura ou
scope pode alterar o UID. Inserir/reordenar duplicatas indistinguíveis pode
alterar seus ordinais; alterar o sujeito de um anônimo pode alterar sua identidade.
Não há promessa universal de estabilidade de símbolos entre versões de indexer.

`CodeRelationOccurrence` tem prefixo `o1:` e 67 bytes ASCII. O domínio
`graphit-code-reference-1` inclui o FILE lógico completo, source literal tipada,
tipo, `data_key` opcional da consulta, alvo lógico observado e ordinal atribuído no
produtor do fato. `CodeRelationUid` (`r1:`, também 67 bytes) inclui FILE, source e
target literais tipados, tipo e essa ocorrência. Os endpoints usam o
`GraphAnchorKey` existente, com domínio explícito STRING/INT64; a string `"1"` e o
inteiro `1` permanecem distintos. Não se decodifica ou reinterpreta uma PK opaca.
Os produtores Code continuam a obter suas PKs pela API tipada.

O hint pertence ao FILE real da associação de source, passado explicitamente ao
helper. Ranges, índices de captura e member/row físicos ficam fora do hash.
Ocorrências e aliases distintos são preservados, inclusive quando o destino só é
resolvido depois. UIDs repetidos em artefatos genéricos não autorizam deduplicar
arestas físicas; o leitor mantém seu próprio ordinal capturado.

`data_key` permanece propriedade independente: duas projeções podem compartilhar
UID e manter PKs e vetores distintos. A adaptação dos registros existentes deve
preservar cada projeção e seus vetores compatíveis, sem mudar modelo ou receita.

`file_hint()` devolve os 16 bytes tipados para auxiliar lookup na leitura capturada.
É dado, não autorização, prova de existência, integridade ou freshness. O consumidor
confere a identidade completa e os metadados do FILE dentro da seleção autorizada;
não extrai um prefixo textual Base64, pois a fronteira de 16 bytes cruza um grupo
de codificação. O hint não substitui identidade física de nó/aresta na captura.

A fonte da receita é `crates/graphit-data/src/code_identity.rs`. Os testes em
`crates/graphit-data/tests/code_identity.rs` cobrem vetores independentes SHA-256 /
Base64, escopos, multiplicidade SCIP, projeções distintas e rejeição de texto
malformado. A integração dos parsers, relações, Source e reuso de vetores exige
seus próprios testes; a qualificação da primitiva isolada não os aprova.

Projeto graphit-code; sessão ses-c98a; migração tsk-7858. A folha `tsk-3f5d`
removeu a contabilidade artificial desses produtores. Os testes afetados em MAIN, Clippy e formatação passaram; os sete checks duráveis
passaram após a revisão final da documentação. Receipts históricos não
qualificam a API alterada. Extração nativa e E01–E10 pertencem a `tsk-b0cb`.

## Resultado, ownership e produtores

`SyntaxIdentity` conserva documento lógico, linguagem, grammar, label, anchor do
sujeito e owners declarados como evidência. Seu UID é a identidade lógica compacta
definida acima; grammar e posições não entram no digest. Extents/propriedades/captura nativa,
resolver de owners, merger/relações, SCIP, storage e publicação têm produtores
próprios. Keys iguais permitem agrupar ocorrências, sem resolver conflito de
propriedades. Reutiliza ranges, contexto original, decoder, catálogo e
PreparedInput já existentes. A receita usa `sha2` e `base64` maduros; observações estruturadas mantêm os codecs de seus consumidores.

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
    declaration: LogicalDeclaration<'_>,
) -> Result<SyntaxIdentity<'doc, 'o, 'request>, IdentityError>;
```

O documento retém metadados, bytes e SnapshotContext originais. `symbol` usa o
contexto retido. new/symbol verificam cancelamento e deadline durante a operação.
Interrupted mapeia Cancelled; TimedOut mapeia DeadlineExceeded; demais falhas
mapeiam ContextDenied. Falhas não retornam identidade parcial.
SyntaxIdentity expõe documento, label, subject, context e UID por getters.
Seu UID possui `as_str()` e `code()` tipado; frames de contexto possuem
label/start/end imutáveis. `LogicalDeclaration` informa nome canônico, tipo/
assinatura opcional e indicador de anonimato observados pelo produtor.
Owners são emprestados durante a chamada; frames são copiados para a filha,
que não retém referências às próprias ownerkeys. Documento, metadados, bytes e
contexto permanecem vivos enquanto a identidade os empresta.

## Valores, precedência e binding
Components: array não vazio, <=path_components; cada componente não vazio, sem NUL, sem "/" interno e diferente "."/"..", <=path_component_bytes. Backslash, colon, aspas, Unicode e espaços permanecem exatos, sem OS conversion/trim/casefold/repair. É identidade lógica relativa, não caminho físico portável; o produtor físico deve governar admissibilidade OS antes integração.
language, grammar e label: texto UTF-8 não vazio, sem NUL e sem byte ASCII whitespace no primeiro/último byte; cada texto <=identity_text_bytes. Byte interno e Unicode não são normalizados. Predicate rejeita, jamais corrige. language é InvalidDocument@language; grammar InvalidGrammar; label InvalidLabel. Os owners já possuem labels admitidos neste mesmo documento/limites.
new verifica o contexto retido antes de limites e conteúdo; operações futuras usam esse mesmo contexto original.
Depois do contextcheck, new valida counts/len, caps e predicados dos componentes/language/grammar; source.len é limitado antes de SourceText::new(original_bytes, limits.source_bytes()). new inicia um agrupador de declarações vazio, sem copiar bytes da Source. Empty originalsource é admissível, mas não suporta symbol não vazio.
symbol verifica o contexto primeiro e limita owners antes da iteração. Ordem semântica: label len/cap/predicate; subject binding; ownerchain binding/prefix; cap do UID fixo e campos semânticos; reservas/cópias, admissão e output. Falha não retorna key parcial.
SourceRange só vem de actual SourceText::range ou actual PreparedInput Exact. Invalid/reversed/UTF8split range não pode ser fabricado via DTO.
Subject exige start<end. Calcular original_bytes.get(start..end) usando slice de bytes, não &str, após bounds/constants e checked end-start. Comparar pointer e len desse subslice com subject.text().as_bytes(): mismatch -> InvalidAnchor (field/index None; o enum fechado não possui Subject), SEM chamar SourceText::range ou escanear UTF8 do original. Nenhum pointer arithmetic unsafe, comparação de conteúdo ou DTO.
Assim original "α" e foreign ASCII.range(0,1) retorna InvalidAnchor, mesmo que 1 corte o UTF8 original. Equal bytes em duas alocações distintas também rejeita; zero-length é rejeitado antes pointer, eliminando ambiguidade de empty buffers.
Somente após binding, carregar upperbound do range e invocar SourceText.range(start,end); checar offsets, text pointer/len e BytePoints exatos contra subject. Mismatch de points vindos de SourceText sobre subslice com offsets relativos -> InvalidAnchor. Unexpected actual RangeError neste passo -> InvariantViolation com range_error tipado. Não inferir captura/extent/sourcegeneration a partir de ponteiro.
Owners: outer→inner. Para cada owner_i, ptr::eq(owner.document(), self); owner_i.context corresponde exatamente aos frames de owners[0..i]. Conferir label/start/end de cada frame, com comparação de texto e contexto verificado. Rejeitar cadeia truncada/trocada, owner estrangeiro, frame repetido ou owner cujo label/start/end é igual ao novo subject: InvalidContext@owners com índice i. Não exigir que parentkey continue vivo após call. O factory não prova containment; native owner resolver deve qualificar fullentityextent strictcontainment e syntaxkind. Não chamar keys manuais de resolver entregue.
Fonte física, geração, sourceversion e namespace project/logicalversion/Data exactview são companions externos reais. Mesma key em geração posterior tem mesmo UID lógico, mas não demonstra autorização/freshness. O fallback lógico e o fundamento SCIP usam tags distintas no digest.

## Admissão, representação e erros

O UID usa a receita compacta acima. A Source e os frames conservam seus ranges
exatos para binding, diagnóstico e Source; esses offsets não são identidade.
O produtor passa `LogicalDeclaration` antes do merge por UID/data_key. Nome vazio
é uma observação válida de chave JSON e não implica anonimato. Não são inventados
nomes ou entidades para sintaxe anônima que o modelo não admite.

Um agrupador privado do documento associa cada seed lógico de ordinal zero às
declarações nativas distintas admitidas. A mesma âncora nesse grupo reutiliza o
ordinal, inclusive em queries repetidas, projeções e reconstrução de owners. Uma
nova ocorrência recebe o próximo ordinal do grupo. São mapas transitórios locais
ao parse; sua ordem de iteração não entra na receita e não há cache ou índice
persistente. A enumeração de duplicatas segue as observações do produtor; inserir/
reordenar uma duplicata indistinguível pode alterar esse pareamento.

O kernel verifica `uid_bytes` contra 67 antes de reservar o output. `Display`
escreve pelo codec maduro em buffer de comprimento fixo; a String reserva sua
capacidade de modo falível. Os labels, scopes e frames também usam reservas
falíveis, com verificação do SnapshotContext original. O writer JSON de duas
passagens da receita posicional foi removido.

As validações existentes de limites, paths, UTF-8, subject/owner binding,
cancelamento, deadline e lifetimes permanecem. Falha não devolve key parcial.
Conversões verificadas retornam ArithmeticOverflow; reserva falível retorna
Allocation; erro de representação interna retorna InvariantViolation. Não há
normalização silenciosa de texto nem novos limites arbitrários.

## Fixtures F01–F06

expected-identity.json conserva 15 conjuntos de observações e UIDs literais
independentes, calculados com Python hashlib/struct/base64. `typed_tuple` descreve
as observações da fixture, não o wire nem os inputs serializados do digest.
`prior_subjects` admite declarações reais anteriores do mesmo grupo para qualificar
o ordinal. Os testes comparam UID e pontos originais sem gerar expected pelo
produtor.
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
| UID literal F01 | 67 bytes, receita compacta exata |
| reserve Vec<u8>/ContextFrame com usize::MAX | Allocation e Vec vazio por capacityoverflow real |

A reserva negativa usa o helper privado de produção; não tenta um heap gigante
nem alega OOM físico. A antiga fixture serializer-work-oracles.json foi retirada
com a contabilidade. Os 15 casos verificam UIDs literais e bindings; o teste de
capacidade verifica 67/66 para o output de comprimento fixo.

F06 é a composição pública usada em tests/symbols.rs:
1. Criar Cancellation/SnapshotContext e limites estruturais finitos reais.
2. Decodificar Go/JavaScript/JSON com decode(input,&profile_limits,original) e
   construir CatalogDraft::build(layers,&catalog_limits,original).
3. Emprestar language/grammar e graph_label/value_label dos getters efetivos.
4. Construir SourceText e SourceRange reais dos bytes originais.
5. Criar IdentityDocument::new(input,limits,original) e symbol(label,range,owners,seed).
6. Comparar observações/UIDs independentes e indexar referências por UID em BTreeMap
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
OriginalProjection::Exact(range) chama `document.symbol` com owners e seed
semântico observados.
project_prepared usa &self e SourceRange empresta bytes originais.
Compound/Synthetic/Ambiguous/Unmappable não viram bounding box. O SourceText
interno de PreparedInput usa len.max(1), permitindo entrada vazia.
A composição Pure não entrega Data/UI USED nem native extractor.
O Core usa `SnapshotContext::new(deadline, cancellation) -> Self`; o contexto
retido continua verificando cancelamento e validade temporal. A composição
atual usa essa API de dois argumentos, sem contabilização artificial de I/O.
A folha B tsk-afac foi concluída r21: 29 testes públicos, quatro unitários,
Clippy estrito e formatação passaram; essa é evidência histórica de B, não aprovação da nova receita compacta.

## Fontes primárias das fronteiras SDK

O kernel usa as APIs maduras `sha2::Sha256`, `base64::Engine`,
`std::fmt::Write` e reservas falíveis de String/Vec/HashMap. A implementação da
receita é `crates/graphit-data/src/code_identity.rs`; binding e contexto estão em
`crates/graphit-ast/src/symbols.rs`. Os vetores literais independentes pertencem aos
testes dessas fronteiras. Não há serializer, escape, decoder ou hash próprio.

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

After the Core composition, `tsk-2cd3` checked the current symbols producer:
nine public tests, five private tests and six symbols doctests passed on Linux.
The doctests include five compile-fail ownership cases and one positive example.
Scoped Clippy for the library and symbols target passed with `--no-deps -D warnings`,
as did formatting of the two Rust files. These results preserve actual input/UID
bounds, allocation errors, ranges, owners, cancellation and lifetimes. They do
not restore the removed accounting tests or qualify the full AST pipeline.
