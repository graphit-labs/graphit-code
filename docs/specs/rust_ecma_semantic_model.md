---
title: Rust JavaScript, TypeScript and TSX semantic model contract
doc_type: technical
description: Retained native ECMAScript model, canonical name recipe and consumer boundaries.
references:
  - target: {type: task, id: tsk-d284}
    relation: implements
  - target: {type: task, id: tsk-75af}
    relation: implements
  - target: {type: task, id: tsk-7890}
    relation: uses
  - target: {type: task, id: tsk-e76e}
    relation: uses
  - target: {type: task, id: tsk-934b}
    relation: uses
  - target: {type: task, id: tsk-50e1}
    relation: uses
  - target: {type: task, id: tsk-77c6}
    relation: uses
---

# JavaScript, TypeScript and TSX model

Audience: AST maintainers and App consumers in Graphit,
session ses-c98a. This is the executable contract for tsk-75af. The native
backends and profile/context producers already exist; the family-specific
semantic model now consumes those producers. Its executed qualification and
remaining checks are recorded below; input files alone are not test evidence.

## Input and ownership

| Producing language | Actual grammar | Active extensions |
| --- | --- | --- |
| javascript | tree-sitter-javascript | .js, .jsx, .mjs |
| typescript | tree-sitter-typescript | .ts |
| tsx | tree-sitter-tsx | .tsx |

The three existing profiles in crates/graphit-ast/profiles/queries define active captures,
labels, data keys, contexts, policies and relations. Builtin grammar availability
does not imply semantic-model availability. Admit coherent pairs explicitly.
Wrong pairs, invalid queries/configuration and parser failures retain typed
causes. Recovery and nonexact Origin mappings remain observable.

AST owns the retained model, name recipe, range/identity assembly, NN codec and
owned Graph adapters. App owns parser-resource acquisition, foreground dispatch,
M inference, publication and HTTP/CLI/MCP journeys. Data, SCIP, Core, Auth, FS,
AI, Knowledge and Runtime remain separately owned. The existing Source and
SnapshotContext contracts apply; this model never creates a grant or reconstructs
Source authority from metadata.

## One retained native collection

The existing ParsedTreeSitterDocument::collect to assemble to code_projections
journey is extended for these pairs. The actual PreparedInput, Original,
relative components and IdentityLimits bind one context collection. Native
nodes, fields, ancestry, captures, context indices, policies and Origin mappings
remain retained under that owner. Neither model assembly nor consumers parse the
whole file again. Code remains an exact borrowed Original extent, while owned
canonical names may differ from their raw captured spelling.

Configured families are Function, Method, Class, Variable, Import, Export, Pair,
Value, Parameter and Field. TypeScript and TSX add Interface, Type, Enum and
EnumMember. Assigned arrows and function expressions retain both callable and
variable contributions when both queries match. All tuples, custom labels,
SDK properties, capture slots and multiplicity remain represented. JSX stays in
native code and extents; active profiles do not declare a JSXTag query.

Native declaration/initializer/body/type fields select complete extents. A name
anchor and full extent have different purposes and remain separate. Configured
span and parent captures must prove their actual owner, with conflicts exposed.
Multiple declarators, parameters, fields and enum members cannot borrow a
neighbor's owner or initializer. Unassigned enum members receive no invented
ordinal or value. Values and types retain native kind, Original range and
literal_source/code provenance; no JavaScript evaluation or constant folding
occurs. String fragments and expression parts retain explicit partial binding.

## Canonical IdentifierName recipe

Normal native identifier, type_identifier and property_identifier names use
SWC's mature public Lexer, StringInput, Buffer and token APIs on only the actual
captured lexeme. The coordinated exact dependencies are swc_ecma_parser 46.1.0,
swc_common 26.0.0 and swc_ecma_ast 29.0.2. Default features are disabled; parser
features typescript and unstable provide the typed fatal-error and EOF token
discriminants. Stacker, flow, visitors and a second file parser are unnecessary.

A proof succeeds only when one word token occupies the complete lexeme, followed
by EOF, with no fatal, recoverable or module lexical error. Its cooked name also
passes public SWC Ident start/continue categories; a Word token alone can accept
an escaped initial digit. Invalid decoded-name proof is distinct from library
lexer diagnostics. No repaired verify_symbol result is consumed. Retain real error
kinds and library spans. SWC supplies the cooked name, so a and escaped-u0061
have the same canonical name; raw_name and each distinct Original anchor/UID
remain untouched. Equivalent names may still produce multiple target candidates.
No manual unescape, regular-expression identifier parser or dummy code is used.

IdentifierName proof permits legitimate property keywords. Actual native parser
shape/recovery establishes the declaration role; this lexical proof alone does
not grant binding validity or parser authority. String/literal, computed,
private, JSX and other names retain explicit native kinds and observed or
unavailable status where a normal identifier recipe cannot be proved. They are
never silently coerced into Identifier names.

The NN encoder distinguishes genuinely model-proved ECMA identifier
representations when emitting embeddings. Its row decoder validates the same
captured-range lexical canonical recipe, producing language/grammar and Source
integrity. Serialized ranges and text cannot recreate a native node kind,
ParsedDocument, Original owner or Source grant. The preceding format-2 cutover
was coordinated with Root. The shared format-3 authority described below reuses
the Rust/Go/JSON/Oracle
name algorithms are reused; the ECMA domain uses its qualified lexical recipe.

## Policies and relations

Export statements, this. receivers, lexical-declaration name paths, anonymous
arrow/function boundaries, native declaration/comment types and configured
complexity branches/operators are retained. Metrics do not include nested
callables; disabled metrics are absent rather than zero. Comment attachment
uses genuine declaration ownership and Original gap/predecessor evidence.

Raw CALLS, INSTANTIATES, INHERITS, IMPLEMENTS, READS_FIELD, WRITES_FIELD,
DECORATOR and configured export relations preserve captures, qualifiers,
properties and effective global TargetRule or its explicit absence. LOCAL
resolution uses canonical names plus actual lexical declarations, scopes,
parameters, import aliases and class receivers. Target outcomes remain zero,
one, ambiguous, unresolved or observed-only. A resolved edge requires proven
source closure, declaration target kind and scope; spelling alone is insufficient.
Dynamic/computed/private receivers, external imports, cross-file targets,
unsupported scope shapes and ambiguous type contexts remain evidence with an
explicit unresolved/observed outcome. The resolver requires evidence for structural TypeScript types. App composes the
cross-file catalogue with SCIP.

## Consumers and failures

NativeNnTsInput/encode_native_nn_ts consume genuine code projections and caller
selection, vectors and model generation. Tests may use controlled vectors to
prove the codec; actual M inference/publication belongs to App. Invalid names,
nonavailable projections, duplicate tuples, wrong producing domain, selection
and vector mismatches fail typed before publication. Indexed Raw pointer,
relative path/components, full digest and actual version are checked; physical
Source uses its genuine capture and revalidation. Original checks cover entry,
loops and exit, including errors.

AstGraphTsInput/assemble_graph_ts retain every semantic row regardless of embed
policy and move owned DTOs to App. The separately owned FILE observation retains
key3 and is_file_observation marker; semantic tuples keep key2 and uid/data_key.
A custom semantic label AstFileObservation is legal and remains distinguishable.
Global diagnostics, capabilities and typed/raw resolution are preserved once per
FILE, including zero entities. The existing [Graph contract](rust_ast_semantic_graph.md)
explains unavailable-model FILE observations and their authority limits.

## Concrete fixtures and acceptance mapping

All files are in [tests/fixtures/ecma](../../crates/graphit-ast/tests/fixtures/ecma).
Expected observations are contracts until the affected native tests qualify them.

| Input | Required observations |
| --- | --- |
| component.tsx | Import, Props Interface, View Class, count Field/Value, render Method and its typed Parameter, App Function/Variable and separate Parameter; full JSX code, export policy, render complexity3 and no tag entity |
| component.jsx | Native JavaScript counterpart, real plain parameters and JSX extents; reuse under .mjs confirms the same producing pair |
| declarations.ts | Interface/Type/Enum/assigned and unassigned EnumMember, decorator/implements evidence, class field/value, method and optional parameters without fake initializer or structural type resolution |
| names.js | Canonical a/escaped-a with distinct anchors, dollar identifier, assigned arrow/function expression, object pairs/string fragments/array values/import alias and exact literal Source |
| local.js | Lexical shadow/TDZ evidence, duplicate F candidates, unknown target, anonymous boundary and class this read/write; computed write remains explicit unsupported evidence |
| recovery.tsx | Actual malformed native ERROR/recovery, retained raw Parameter/READS captures and diagnostic ranges; no invented declaration owner or semantic row |
| binding_scopes.js | Qualified native lexical/var/import alias/destructuring/rest/static/arrow/function/block boundaries before implementing LOCAL |

Additional affected cases cover custom labels/data keys/TargetRule absence,
nonexact mappings, empty and genuine physical NULL source, Original/allocation/
path/cancellation guards and supplied NN vectors with domain/row integrity
failures. Run only the causal model/name/NN/Graph fixtures and strict Clippy
for their targets. Broad all-language or publication success is not implied.

[Retained model](rust_tree_sitter_semantic_model.md) and [native Tree-sitter NN](rust_tree_sitter_native_nn.md) describe existing shared producer and consumer contracts.

## Qualified name and native-input prerequisites

Native evidence: JavaScript fields select property while TypeScript fields
select name; unassigned enum B is directly an enum_body child. Assigned
arrows/functions retain both anonymous Function and configured Variable frames.
The second name in a multi-declarator lexical declaration inherits the configured
first-name frame, so the semantic assembler must prove its own declaration
without attributing a sibling owner. Truly anonymous nested calls retain an
unresolved context and cannot bind to the outer caller. A decorator preceding
an exported class has no Class ancestor in its captured scope; any source binding
requires the actual export declaration field, not a guessed next class.

## Shared configured value-row prerequisite

The ECMA assembler consumes the necessary correction `tsk-77c6`: a configured
`value_label` requires an additional semantic row, not only declaration
metadata. Its retained proof uses the actual value capture Original anchor and
extent, the query `data_key`, and the corresponding real declaration owner.
The name is literal Source; it must not pass through the IdentifierName recipe.
Repeated captures use current identity/merge, while multi-subject ambiguity,
recovery and nonexact captures preserve explicit causes rather than guessing.

## Foreground NN selection and literal owner refinement

The stock JavaScript, TypeScript and TSX embed-label policies omit Value, Pair,
Import and Export. The model filters those labels before returning code
projections; Foreground currently selects every available projection, without
a literal-name filter. Therefore derived Value rows remain Graph entities and
do not enter stock NN selection. Under the previous codec, a custom policy embedding Value could reject the
whole stream. The explicit representation protocol distinguishes Identifier and LiteralSource explicitly and keeps
exact names/ranges, allowing mixed rows without a silent skip. The genuine
ECMA mixed-name batch also passed SDK publication/query with all columns
compared by UID/data_key. Family qualification requires parser and consumer tests.

The active Pair query with a string_fragment key and configured value_label
also requires a real literal declaration owner. The Go-scoped producer qualification in tsk-77c6 covered identifier owners.
The shared helper now supports explicit literal owner selections under NN75-P.
The ECMA assembler supplies actual literal-key ownership proof without
passing their names through IdentifierName, inventing configured context slots,
or claiming tsk-77c6 proved this ECMA case.

## Current NativeNN format 4 shape

Root approved the closed name_kind entity discriminator and namespace name_kind/
raw_name fields under tsk-75af. Format metadata is ast-native-nn-4; incomplete
or old schemas fail without migration/fallback. Identifier uses the mature
ECMAScript lexical recipe when the family integrates. LiteralSource equals the
actual captured lexeme, including string fragments and bare identifier values.
The namespace frame retains its real literal or identifier selection and raw
anchor lexeme. The Go and ECMA mixed codec/SDK proofs exist, including ECMA Pair literal-owner
assembly. App productive M and publication journeys remain downstream. See the
[NativeNN protocol](rust_tree_sitter_native_nn.md#explicit-name-representations-nn-format-3).

## Native model integration under qualification

The EC75-M implementation now routes the three exact producing pairs through
`tree_sitter_model/ecma.rs`. `TsSignature.ecma` carries actual native parameter,
type-parameter and return-type observations; `TsContribution.ecma_declaration`
carries native kind, export statement, attachment owner, comments and issues.
Existing Rust/Go contributions keep these ECMA fields absent. The owned Graph
serializer preserves these observations, and metadata participates in merging.

Own or sibling lexical context exclusions retain their native context indices
in each contribution instead of rewriting a captured owner name. Assigned
callable and configured variable frames remain distinct. Literal Pair owners
use their actual captured string fragment and explicit owner selection.
The codec now applies the same mature SWC name recipe to exact JS/TS/TSX
domains and preserves literal Source under the current shared format 4.

## Native lexical scope corrections

Catch parameters, including native destructuring patterns, shadow outer names in
the actual catch scope even when no configured entity row indexes them. A block
function retains its native block binding; possible Annex B effects are separately
retained as uncertainty at the real enclosing callable/program. Without proving
those conditions the lookup stays unsupported before an outer fallback; a closer
known lexical binding still wins. No binding or UID is invented for uncertainty.

Type-space references, including configured IMPLEMENTS to Interface, are independent
of runtime TDZ and can reference later native type declarations. Value-space
references retain TDZ checks. Function body var declarations use the native body
environment, so they do not shadow calls in default parameter initializers; calls
in the body still see the variable. These four cases passed their explicit actual Raw SDK
regressions; final qualification is recorded in the delivery receipt.

Format 3 introduced Generic NativeSource. The current shared format 4 also permits an absent remote model generation. ECMA
Identifier/LiteralSource and namespace validation are unchanged; NativeSource is
rejected for these reserved producing pairs. Historical ECMA receipts retain
their original scope. C's format-3 cutover has 15 passing Linux focal cases,
including real Main NN and focused specialized controls; existing ECMA name
algorithms are unchanged by source inspection. This does not claim a replay of
the ECMA campaign. See [current qualification](rust_tree_sitter_native_nn.md#generic-format-3-qualification)
for its exact scope and remaining platform/quality limits.
