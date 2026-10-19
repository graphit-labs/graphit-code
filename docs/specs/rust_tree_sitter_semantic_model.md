---
title: Rust Tree-sitter Struct and Field semantic model
doc_type: technical
description: Genuine Rust and stock Go declarations, native literal Source and local evidence resolution from one context collection.
references:
  - target: {type: task, id: tsk-9fa2}
    relation: implements
  - target: {type: task, id: tsk-75af}
    relation: implements
  - target: {type: task, id: tsk-50e1}
    relation: implements
  - target: {type: task, id: tsk-d29d}
    relation: implements
  - target: {type: task, id: tsk-6d5b}
    relation: implements
  - target: {type: task, id: tsk-a82f}
    relation: implements
  - target: {type: task, id: tsk-e76e}
    relation: uses
  - target: {type: task, id: tsk-3f5d}
    relation: uses
  - target: {type: task, id: tsk-072f}
    relation: uses
  - target: {type: task, id: tsk-fcc0}
    relation: uses
  - target: {type: task, id: tsk-5e3a}
    relation: uses
  - target: {type: task, id: tsk-107e}
    relation: relates_to
  - target: {type: task, id: tsk-77c6}
    relation: implements
---

# Rust Tree-sitter semantic model

Audience: AST indexing, query and NativeNN consumer maintainers. This library model produces Rust `Function`, method syntax, `Struct` and named struct `Field` entities through retained native collection and assembly. The Go model uses the same owner and assembler for stock declarations and local resolution. JavaScript/TypeScript/TSX have a dedicated native assembler documented in the [ECMA contract](rust_ecma_semantic_model.md). The implementation is [tree_sitter_model.rs](../../crates/graphit-ast/src/tree_sitter_model.rs), with [executable examples](../../crates/graphit-ast/tests/tree_sitter_model.rs). It consumes [native contexts](rust_tree_sitter_contexts.md) and the [identity/extent contracts](rust_ast_semantic_extraction.md).

The qualified grammar fixture is `tree-sitter-rust 0.24.2`, using the real SDK.
Syn `2.0.119`, with only `full` and `parsing` and defaults disabled, decodes
identifiers, declared field types, signatures, visibility and documentation attributes. No manually written Rust parser is used.
The Rust model never executes text normalizers or infers types. Python identifier canonicalization follows its normative NFKC rule, as documented below.

## Caller flow and lifetime

The caller supplies its actual effective Rust profile, loaded `GrammarLanguage`,
the legitimate `PreparedInput` and its original request, literal relative path
components, and validated `IdentityLimits`. The model does not load startup
profiles, acquire a snapshot, read a file or choose project extensions.

1. Prepare `PreparedTreeSitterContexts` using the actual effective profile and
   grammar, then call `ParsedTreeSitterDocument::collect(&factory, &input,
   relative_components, limits)`.
2. The factory collects contexts exactly once. It retains the native tree,
   queries, matches and scopes, and builds `IdentityDocument` from the unchanged
   `input.original_text()` allocation and the same `SnapshotContext`.
3. Call `parsed.assemble()` to get `TreeSitterSemanticDocument` borrowing the
   parsed owner. All node and capture association uses the retained tree. There
   is no second parse, query execution or SDK node walk.
4. Read `entities()`, `diagnostics()` and `capabilities()`, then consume
   `code_projections()` to distinguish complete snippets from unavailable ones.

The parsed document owns projections computed while the original `OriginPlan`
was alive. The input/plan and prepared factory may be dropped after collection;
the original/prepared text, profile, path metadata and request must remain alive.
The assembled model borrows this owner. There is no public factory accepting
foreign matches with another source, and no public entity/model restoration DTO.
The retained boxed `syn::Type` is syntax, with no promise of transfer across
threads; a synchronous encoder can borrow the model and return an owned batch.

`collect`, `assemble`, parsed/model getters and projections check the original
request before and after work. Rust loops poll it, and Syn parsing is checked
before and after its call; Syn has no internal cancellation callback. Errors
preserve the operation, context, origin, identity, range or extent cause.
`SnapshotContext::check` currently controls request cancellation and deadline.
These checks are not a Raw/storage lease or snapshot-authority check. The caller
and the NativeNN/Raw publisher retain that responsibility.

## Native declarations and identity

The supported predicate is explicit:

| Entity | Name node and native field | Declaration extent |
|---|---|---|
| `Function` / explicit `Method` | `function_item.name: identifier`, with direct native impl/trait classification | Complete native `function_item` |
| Explicit signature query | `function_signature_item.name: identifier` in a trait/foreign declaration list | Complete native declaration including semicolon |
| `Struct` | `struct_item.name: type_identifier` | Complete native `struct_item` |
| `Field` | `field_declaration.name: field_identifier`, under `field_declaration_list` whose immediate parent is `struct_item` | Complete native `field_declaration` |

Union and enum-variant field lists also contain `field_declaration`, so they are
reported as unsupported declaration families in this adapter. Tuple fields are
types in `ordered_field_declaration_list`; no ordinal names are manufactured.
Tuple and unit **struct entities** are supported by their real `struct_item`.

Capture node IDs, indexes, prepared byte offsets, points and text must agree
with the retained SDK tree. These IDs remain document-local evidence, not
persistent identifiers. The name's nonempty `OriginalProjection::Exact` is the
literal UID anchor. `IdentityDocument::symbol` creates the genuine identity from
that anchor, label and genuine outer-to-inner owner identities. The `data_key`
is a separate literal merge/publication field.

Raw identifiers retain their original token, for example `r#type`. Syn's
`IdentExt::parse_any` and `unraw` produce display name `type`. The display name's
length can differ from the raw anchor; neither the UID nor its range is rebuilt
from display text. Unicode, CRLF and literal path components are preserved.
Syn spans never replace original byte ranges or SDK points.

Struct extents include the native visibility, generics, body/closing delimiter,
or semicolon when present in the item. Field extents include visibility, name,
colon and type. A field's comma and preceding attributes belong to the field
list, so they are outside its extent. Preceding struct documentation/attributes
are also separate. No sibling concatenation or enclosing hull is used.

An explicitly configured `span_capture` must select the candidate's own native
declaration. Spans are associated with each name candidate before cardinality:
each span belongs to the most specific candidate declaration containing its
native node, determined by walking the retained parent chain. Nested fragments
cannot be hidden by an outer candidate. Repeated captures of the same declaration
preserve their addresses. Missing, fragment, body and
ancestor spans are diagnosed, never replaced with the default declaration.

## Namespace and declared type

Scopes and their self-exclusion, unresolved contexts and path attempts come
from the context provider. Owner chains are checked against real native ancestry
and original containment where an exact owner extent is available. Namespace
frames are not additional semantic entities.

`ContextName::Missing` remains missing. Rust `impl_item` has a `type` field
and no `name` field. The stock profile now explicitly configures
`context_name_queries: {impl_item: '(impl_item type: (_) @name) @context'}`; the provider selects that actual native
field and reports `Found`. Profiles without the path remain unresolved and
cannot silently receive a shorter UID. The whole raw self type is validated by
Syn and preserved as both namespace name and anchor in
`ConfiguredRustImplType`, for example `crate::S<T>`. This does not resolve `Self`,
trait implementations or another declaration. Other identifier owner names use
Syn's identifier codec. Configured labels remain part of their genuine frames.

For a named field, `TsDeclaredType::Present(TsTypeSyntax)` holds the actual native
`type` node, prepared and original projections, its raw original range, and
`Box<syn::Type>`. References, lifetimes, generic, tuple, array and function type
syntax are retained without printing or type resolution. Struct declared type
is `NotApplicable`; its generics are not converted into a fictitious field type.
Missing, recovered, nonexact, decoder-error and Syn `Verbatim` types remain
unavailable with diagnostics. Literal values are not a field/struct decoder stub:
their stock constant/static family still requires its own migration.

## Functions and method syntax

`function_item.name: identifier` creates the effective query label. Stock Rust
uses `Function` for free functions, impl methods, associated functions without a
receiver, and trait methods with a body. `TsCallableKind` distinguishes these by
the immediate `declaration_list` parent and its impl/trait container. A nested
function in a block stays `Function` even inside a method. An explicit `Method`
query is accepted only for a native method; it does not relabel stock entities.
Metavariable captures are unsupported instead of being decoded as names.

A query explicitly capturing `function_signature_item` can produce
`TraitSignature` or `ForeignFunction` under the actual trait/foreign container.
Stock's `function_item` query does not claim abstract signatures. Their full
extent includes the native semicolon; bodyless signatures have no invented body.

`TsContribution::callable` and `TsEntity::callable()` expose immutable
`TsFunctionDetails`, shared with `Rc` per native declaration during one assembly.
Repeated captures reuse the actual details. The model and projections remain
borrowed from the genuine collected owner; `Rc` does not create another authority
or promise that Syn syntax can move across threads.

`TsSignature` preserves the prepared range, original projection, exact raw range
when available, and boxed mature `syn::Signature`. SDK token/node bounds exclude
visibility, body and the signature item's semicolon, with no trim or text split.
The signature retains generics, where clause, inputs/patterns/receiver, variadic
arguments, return syntax, ABI and modifiers. Implicit return is Syn's actual
`ReturnType::Default`, without an invented `()` range. Callable declared field
type is `NotApplicable`.

Native impl `default fn` selects `syn::ImplItemFn` directly over the complete
Original Exact item. Its defaultness marker, raw signature, visibility and parsed
signature remain separate. The model does not remove `default`, retry a failed
signature through another parser or implement a Rust parser. This validates
syntax without resolving specialization permissions or compilation semantics.

`TsVisibility` keeps the actual raw visibility and mature `Public`, `Restricted`
or `Inherited` syntax. `pub(crate)` differs from public external visibility.
Signature/visibility errors, recovery and nonexact origins remain indexed in
`TsFunctionDetails::issues`. `TsIssue::Callable { issue_index }` addresses those
same details; no participating cause is cleared by another contribution.

## Callable attachments, metrics and export

Configured native comments and attributes attach through real sibling/parent
chains. Contiguous preceding outer comments/attributes belong to the next
function in that parent; another item breaks the chain. Whitespace-only gaps,
including blank lines, must agree in prepared and original ranges. Leading inner
docs/attributes in the body belong to the containing function. Outer docs in the
body belong to the following declaration and are not attached to the container.
Attachments retain their original projections and raw bytes outside the function
extent. Ordinary comments retain raw text.

Doc fields `outer`, `inner` and `doc` come from the SDK. Syn
`Attribute::parse_outer`/`parse_inner` decodes actual attributes and doc comments;
`Meta::NameValue → Expr::Lit → Lit::Str.value()` preserves leading spaces and
handles escaped or raw strings. A documentation expression such as `include_str!`
remains a typed attribute and an explicit unsupported expression cause. It is
never executed or turned into empty documentation. Invalid origins and parser
errors are observable and make the participating snippet unavailable.

`TsComplexity::Score` stores base one plus configured named decision nodes and
anonymous operator leaf tokens in the native body. It preserves decision node
indexes and the separate body index. Nested callable/declaration/anonymous
function subtrees are excluded; their scores remain independent. Text in strings
or comments is not counted. Empty rules yield `Disabled`; configured metrics on
a bodyless signature yield `NoBody`. Unsupported head-call policy and recovery
produce `Unavailable` with a cause. These counts describe the retained prepared
syntax; all origins and unavailable projections remain explicit.

`TsExport::Modifier(bool)` implements the stock `modifier` policy with keyword
`pub` from the native visibility token, including restricted `pub(crate/self/
super/in path)`. It does not equate this policy with external public visibility
or search the body/name/docs for text. `None` yields `NotConfigured`; other export
strategies/configuration remain explicitly unavailable in this Rust family.

## Relation source observations

`relations()` returns each real relation query/match, effective provenance,
capture bindings/properties/original projections, target rule, and native source
bindings. Calls and field reads bind to the nearest callable container; nested
calls belong to the nested function. A preceding decorator attribute uses the
same native attachment owner. Source bindings contain the model's actual entity
indexes, or `UnavailableContainer` when the native callable has no emitted
identity, or `MissingContainer` when no supported callable container exists.
`Bound` proves the identity binding; inspect the entity's projection separately
for source availability. It does not turn a malformed body into an available
snippet.

These are relation observations and caller bindings, not resolved edges.
`ObservedOnlyRelation` remains explicit. Target resolution with zero/one/ambiguous
candidates, other source-container families and graph publication retain their
separate full-migration requirements. No target or cross-language edge is
manufactured from a name.

## Contributions, diagnostics and snippets

Every name capture contributes independently, including repeated captures in a
match. Merge uses only `(genuine UID, literal data_key)`. Contributions retain
occurrence/capture addresses, structural container, extent, declared type, callable details and
their own namespace evidence. Different data keys stay separate entities.

Extent agreement requires the same original source, ranges and points (or the
same nonexact projection). Differences use the common `ContributionConflict`
with `Extent` and the two contribution indexes. Declared type agreement uses raw
original bytes, without Syn equality/printing features; divergent present types
use `TsModelConflictField::DeclaredType`. Callable disagreements use
`TsModelConflictField::Callable`; a callable conflict does not produce a fake
field declared-type conflict. Name/namespace disagreement is an explicit binding
diagnostic. No later contribution clears a preceding failure. Causes are linked
to an entity even when the bad query appeared before its valid contribution.

`TsEntity::extent()` and `declared_type()` return aggregate views with `Conflict`
as well as the normal exact/present, missing/nonexact or unavailable states.
`namespaces()` is the canonical first contribution's display view; use all
`contributions()` for evidence and the diagnostics/projection state for validity.
Conflicting namespace evidence cannot produce an available snippet.

Recovery is local to participating native nodes and their subtrees. A broken
struct or owner yields diagnostics/unavailable projections; a separate intact
struct can still be available. Native scopes and nonexact origins are retained.
Only an exact nonempty original declaration creates `EntityExtent`; partial or
synthetic origins never produce a fabricated complete extent.

`code_projections()` respects the profile's `embed_labels`. Its output is:

- `TsProjection::Available`: entity index and genuine entity reference, exact
  `EntityExtent`, original `text`, language and grammar. No participating failure,
  merge conflict or incomplete namespace is concealed.
- `TsProjection::Unavailable`: entity index plus diagnostic and conflict indexes
  explaining why a complete snippet cannot be used. A capture that cannot create
  an identity is reported in diagnostics instead of a fake entity.

## Capabilities and integration boundary

`TsQueryCapability` retains the effective query, including its configuration
provenance, and reports supported callables/structs/named struct fields, observed-only
families/relations, or unsupported configuration. Other groups, matches,
captures, properties and recovery remain available in `context_document()`.
Functions and methods use this same assembler and its `SupportedCallable`
capability. The historical `tsk-9969` extractor stays a Root-owned consumer cutover
obligation: replace its callers with this genuine model, then remove its separate
preparation/query/document/extractor without an alias or wrapper. This model
never invokes that extractor or parses the SDK tree a second time.

Nonempty `name_reject`, parent/qualifier/target/value metadata, `name_is_data` and
target-label configuration on these entity queries are explicitly unsupported.
A nonempty profile `text_normalizers` map reports
`UnsupportedProfileNormalization`, retains the native observations and promotes
no candidates, including when the profile has no queries. Diagnostics expose
`TsDiagnosticLocation::Profile`, `Query { query_index }`, or
`Occurrence(TsOccurrence)` with actual SDK query/match/capture indices. Unsupported
query configuration without matches uses `Query`; it never fabricates match zero.
No normalizer is applied or assumed already applied by a caller.

The remaining Rust families, other active languages, resolved relationships,
additional syntax/metric/export/attachment policies, embedded handling and replacement of
the existing function extraction path remain necessary in the
[full semantic migration](rust_ast_semantic_extraction.md). This adapter does not
establish those capabilities or full-model completion.

The [native NN codec](rust_tree_sitter_native_nn.md) consumes this genuine model
through `NativeNnTsInput` and `encode_native_nn_ts`. It preserves Struct/Field,
configured data keys/namespaces, raw identifier display names and native type
namespaces in the existing Arrow20 recipe. It verifies actual Source/Raw,
same allocation/request, digests, dimensions and generation; persisted rows do
not restore native provenance. Its storage fixtures and qualification remain
distinct from model projection tests and daemon/model inference integration.

## Qualification

Run the existing portable invocations from the repository root:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test tree_sitter_model
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test tree_sitter_model -- -D warnings
```

The target exercises actual stock/derived Rust SDK queries and source mappings,
raw identifiers/Unicode/CRLF, named/tuple/unit structures, enum/union exclusions,
real owner chains/Missing paths, type codecs, local recovery, spans, merge
conflicts, request cancellation/deadline and plan/factory drop. Qualification
results and environment are recorded with the delivery Task. One-platform runs
do not qualify unexecuted Windows/macOS targets or all active grammars.

The initial Struct/Field delivery's Linux qualification completed 14 native tests in 1.70 seconds
and the exact focal Clippy invocation with `-D warnings`. The full test command
took 10.24 seconds including the shared build queue/build; Clippy took 11.63
seconds. These runs used the coordinated shared Cargo target, four build jobs,
disabled debug information and incremental compilation, with the locked offline
dependency graph. Windows and macOS were not executed. This evidence qualifies
the model and local projections, while the NN codec and
the remaining semantic families retain their separate qualification scopes.

The Function/Method extension qualifies 21 native tests, including mature
async/const/unsafe/ABI/variadic and pattern syntax, typed impl namespaces and
default methods, outer/inner attachments, native metrics/export, original gap
validation and nested decorator/caller bindings. Its final Linux test command
completed in 43.47 seconds including the shared build queue, with 1.51 seconds
in the test target. The final strict Clippy invocation passed in 139.65 seconds including the shared
queue. Scoped quality evidence is recorded in `tsk-6d5b`; Windows/macOS were not run. Root owns the separate old Function
consumer cutover and remaining full AST integration.

## Go native syntax, values and local relationships

The exact profile pair is go/tree-sitter-go with the qualified builtin Go0.25
grammar. One collect retains the genuine input, Original, native tree, queries
and IdentityDocument. The [Go branches](../../crates/graphit-ast/src/tree_sitter_model/go.rs)
use that native syntax without another parse, foreign Syn payload or rebuilt
identity. Unsupported profile pairs remain a typed error.

Stock Function, Method, Struct, Field, Interface, Type, Constant, Variable,
Import and named Parameter queries assemble entities. Native type_alias and
variadic_parameter_declaration are accepted when an actual query captures them;
the assembler does not manufacture queries, anonymous names or embedded fields.
Each name keeps its original subject/UID and configured data_key. Type/Interface/
Struct on one type_spec have their own labels and identities. Complete extents
are the individual specification or parameter/field declaration, excluding
sibling specifications and grouping wrappers. Anonymous struct fields keep their
real declaration and configured ancestors without a fabricated named Struct.

Go0.25 puts const_spec's name field around the comma-separated sequence, while
var_spec repeats the name field for each identifier. The two stock Constant
queries therefore select direct identifier children of const_spec, retaining
real captures for every declared name. Identifiers inside the initializer are
not direct children and do not become subjects. This follows the
[versioned grammar](https://github.com/tree-sitter/tree-sitter-go/blob/v0.25.0/grammar.js),
not text splitting or subject expansion.

TsNameRepresentation distinguishes Identifier from LiteralSource{native_kind}.
Import name/name_raw are the actual quoted path lexeme with authored escapes;
they are not a decoded path or identifier. Its optional native alias is separate
metadata. No JSON, shell unquote, basename-derived package or receiver owner is
invented. Stock Import and Parameter do not embed.

TsDeclaredType::Native exposes actual node index/kind, prepared range, retained
Original projection and optional exact raw range. Type/Interface and named
parameters require their native type; const/var may omit it legitimately.
Go signatures retain native receiver/type_parameters/parameters/result and the
original complete header; absent Syn syntax and genuine bodyless declarations
are legitimate. MethodGo receiver references are not Struct declarations.

TsContribution.declared_value distinguishes NotConfigured, configured Missing
and Native initializer/capture observations. The initializer is boxed to avoid
a large enum variant; its ranges and Original remain borrowed. Binding uses the
actual native subject ordinal and direct expression-list children, independent
of capture/query order. Subject, OtherSubject, UnprovenArity, Initializer, ExpressionPart and
OutsideInitializer are explicit states. Whole-initializer and nested expression
captures retain native evidence without claiming a scalar subject value. A capture belonging to another declared
subject is retained without being assigned to this subject. Name-only queries
do not assert a missing value. Actual contribution disagreements stay indexed
conflicts rather than first/last-value selection.

The value product is literal_source: native kind, authored bytes/ranges, Original
and binding/status. Large numbers, base prefixes, hex/decimal floats, rune and
string escapes, raw CRLF and long values are preserved. It is not decoded_value;
the current NN consumer uses original code text. The model performs no constant
evaluation, tuple-return inference or iota folding. A real future decoded-value
consumer needs an independently qualified mature codec.

Namespaces retain native ordinals, anchors, extents and configured labels.
A configured type_spec:Struct frame remains that frame even for Interface/Type;
it does not fabricate an emitted Struct vertex. Self exclusion uses declaration
ordinal. context_index is Some for a real context. It is None for a genuine
explicit parent capture (with explicit_parent_capture_index) or for the real
declaration frame of a derived value row (DerivedDeclarationIdentifier, without
a fabricated parent-capture slot).
Concordant native context/capture owners coalesce. Invalid ancestry, ambiguity,
conflicting label/anchor, recovery and nonexact origin remain diagnostics.

Non-callable Go declaration metadata exposes native kind, optional alias, export,
attachment boundary, comments and issues. Exports use Unicode Lu and actual
package/group scope or field/method; local declarations, imports and parameters
are not exported. Preceding comments require native sibling boundaries and
whitespace with one LF. Original gaps and the retained predecessor are checked,
so a Prepared synthetic newline cannot reclassify an originally trailing comment.
Grouping wrappers are explicit attachment boundaries. Comment in embed_labels
alone creates no query/entity/data_key. Complexity still excludes nested
func_literal; no Rust attribute/doc decoder is applied.

local_relations() returns occurrence/source indices, native evidence and
Resolved(actual entity index), NotFound in this document, Ambiguous(actual
candidates), or Unresolved(typed reason). Existing raw relation query/capture/
TargetRule/source observations remain available. Local lookup uses actual
declarations/scopes and source positions. Parameters become visible in the
body; local variable declarations do not shadow their own initializer. Native
switch statements delimit their initializer bindings. Type-switch aliases are
visible only in each native type/default clause after its colon, as specified by
[Go type switches](https://go.dev/ref/spec#Type_switches); header and subsequent
calls use the enclosing bindings. Explicit
receiver types and real local type/member declarations can establish method/
field targets. Target rules restrict labels; data_key-distinct candidates are
not arbitrarily chosen. Assignment '=' LHS is not read; compound assignments
can read and write. Calls inside func_literal have that boundary and cannot
silently belong to an outer named function.

Interface receivers (including local aliases and inline interface types) yield
Unresolved/DynamicReceiver with their actual native type node. Whole declaration
parent captures with several names are InvalidExplicitParent; no first-name
owner is selected. A captured parent name can identify that real native subject.
Direct struct members remain resolvable when another member is embedded.

This is local evidence resolution. Unknown/dynamic receivers, imports, promoted
members, missing policies, unsupported shapes, recovery and nonexact source
remain observable. Cross-file/catalogue resolution and productive Graph writer/
publication are separate contracts. Serialized rows or public index DTOs cannot
restore the model or Source authority.

Examples are in [tree_sitter_go_model.rs](../../crates/graphit-ast/tests/tree_sitter_go_model.rs).
tsk-d29d qualifies the original four-family producer; tsk-50e1 tracks the
remaining model/local resolution and its affected checks. Qualification state
and actual failures are in their receipts; this contract does not claim that
every pending check or Graph/App integration has passed. Actual shared M
inference belongs to the Root-owned consumer.

Graph consumers use the [native AST Graph producer](rust_ast_semantic_graph.md),
which retains the genuine Source association and emits owned per-file observations.
It consumes all available supported entity families independently of NN embed labels;
LOCAL targets create edges, while raw unresolved/observed-only outcomes remain visible.
The adapter is separate from Root writer/publication and cross-file integration.

### Configured additional value rows

The `value_label` metadata requests an additional semantic row. The base declaration keeps `TsDeclaredValue::Native`; this metadata alone does not represent the additional row. A derived row uses the configured label and the same query `data_key`, with the native value capture as its exact Original name anchor and extent. Its name is `LiteralSource`: authored quotes, escapes and an empty quoted string remain intact. There is no scalar decoding, trimming, constant complexity score or invented ownership edge.

`TsContribution::derived_value` contains `TsDerivedValueSubject`: the actual
declaration UID/label and native declaration/name indices, the value capture
slot and its proven binding. The contribution name-capture slot is the actual
value slot; owner_name_capture_index separately retains the declaration name
slot in the same native query/match. Its namespace chain retains the existing owners
and adds the declaration itself as `DerivedDeclarationIdentifier`, with
genuine anchor and full extent, and no fabricated context/capture index.
Identity and merge use the existing symbol formula and `(UID, data_key)`;
equal literals of different owners remain distinct and repeated queries retain
their contributions.

A direct `Subject` binding must match the subject ordinal. `Initializer`
requires one subject. `ExpressionPart` requires proven arity and the matching
ordinal. Other-subject captures remain metadata for the corresponding owner.
Unproven arity, out-of-initializer captures, unavailable owners, recovery and
nonexact Origin cannot invent a derived identity; their metadata/diagnostics
remain explicit. Such value ambiguity does not poison a valid declaration's
projection. A derived literal containing a function cannot become a callable
relation source: the native anonymous boundary stays unavailable/unresolved.

This shared assembler consumes retained native value proofs; it does not parse another language through Go. Rust families outside the current model remain scoped by concrete active consumers.

## Python configured baseline

The exact producing pair is `python/tree-sitter-python`. The retained model now implements the configured stock Function, Class, Import, Variable and Parameter families and derived Value rows. This status does not qualify every Python syntax form or another language.

[python.rs](../../crates/graphit-ast/src/tree_sitter_model/python.rs) consumes retained native fields, captures and Original ranges. Each identifier uses the mature unicode-ident XID checks, with an explicit initial underscore, and unicode-normalization NFKC; authored anchors and identity locations remain unchanged. Dotted imports and value names preserve their native literal Source, including quotes and escapes. There is no scalar decoder or evaluator.

`TsPythonSignature` retains native parameters, optional type parameters and return type. `TsPythonDeclarationDetails` retains the native declaration kind, `TsExport::NamePrefix`, wrapper attachment owner, comments, an optional first native string expression and issues. The first string is an opaque syntax observation, with no decoded text or semantic docstring claim. Decorated definitions use their actual definition child and wrapper extent. The export policy uses each entity's own name capture. Nested functions/classes/lambdas do not contribute to the enclosing callable's complexity; disabled metrics remain Disabled.

LOCAL direct CALLS require a genuine bound source, configured TargetRule and proved lexical binding. Duplicate canonical names remain ambiguous. Parameters, later assignments, imports and opaque scopes block an unproved outer fallback. Class namespaces are skipped as method closures; private-name mangling and implicit `__class__`, decorators, headers, comprehensions and dynamic receivers remain explicitly unresolved where their environments are not proved. No global homonym edge is invented. Other configured relations remain native observations without a missing target rule being synthesized.

The [Python model fixtures](../../crates/graphit-ast/tests/tree_sitter_python_model.rs) exercise native owners, canonical/raw names, custom keys/labels/parents/spans/properties, values, policy metadata, causal LOCAL outcomes, Graph and Source guards. [The native contract](../../crates/graphit-ast/tests/python_native_contract.rs) checks the installed grammar's fields before those semantic algorithms.
