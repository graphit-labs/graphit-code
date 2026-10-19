---
title: Rust AST Oracle semantic model
description: Genuine Oracle declaration identities, owners, values and original code projections.
doc_type: technical
references:
  - target: {type: task, id: tsk-6eae}
    relation: implements
  - target: {type: task, id: tsk-cafe}
    relation: implements
  - target: {type: task, id: tsk-fcc0}
    relation: implements
  - target: {type: task, id: tsk-a4cd}
    relation: supports
  - target: {type: task, id: tsk-1d33}
    relation: consumes
  - target: {type: task, id: tsk-3f2a}
    relation: consumes
  - target: {type: task, id: tsk-f13b}
    relation: relates_to
  - target: {type: task, id: tsk-4c23}
    relation: implements
---

# Rust AST Oracle semantic model

For AST and NativeNN integrators. The [Oracle model](../../crates/graphit-ast/src/antlr_engine/model.rs)
assembles Table, Column, Variable, Constant, Function, Procedure, Package,
Parameter and local Type declarations from the genuine [raw ANTLR consumer](rust_ast_antlr_semantics.md).
Other declaration families and
relations remain explicit observations. The broader [semantic model and publication](rust_ast_semantic_extraction.md)
requirements remain separate.

## Collect, assemble and consume

```rust
let parsed = ParsedAntlrDocument::collect(profile, input, relative_components, limits)?;
let model = parsed.assemble()?;
let projections = model.code_projections()?;
```

The Oracle path uses the effective language=plsql, grammar=antlr-plsql,
parser=Antlr4 configuration. The same entrypoint also consumes db2/antlr-db2;
see [DB2 observations](#db2-observations-and-nativenn). Other profile pairs fail
with UnsupportedProfile before SDK execution. input is the actual PreparedInput,
not a reconstructed raw-match DTO. relative_components and the existing
IdentityLimits feed the qualified identity factory.

Collection parses once through `parse_semantic_occurrences`. The private owner
retains its raw syntax, occurrences and contexts, the same original allocation,
borrowed original request, an `IdentityDocument`, and selected declaration/name/type
projections. It retains no copy of the source or syntax and no persistent cache.
Assembly borrows this owner; returned entities cannot outlive it. No constructor
accepts arbitrary public raw structs plus another input to establish authority.

`observations()` gives the retained raw document. The assembled model exposes
`entities()`, `diagnostics()`, `owner_links()`, `unresolved_relations()` and
`code_projections()`. Relation indices address the complete retained raw inventory,
including occurrences with local Oracle resolution, and preserve query metadata,
captures, qualifiers and contexts. `oracle_calls()` and
`oracle_call(occurrence_index)` expose the local callable consumer described below;
global SQL targets remain outside this model.

Each entity has its genuine `SyntaxIdentity`, effective `data_key`, decoded name,
original `name_raw`, native declaration ordinal, namespace, contributions,
conflicts and diagnostic indices. Namespace identities come from the same
identity factory and actual native ancestors. Namespaces expose `identity()`,
`name()`, `anchor()`, `node_index()` and `extent()`.

## Native declarations and owners

| Observed native branch | Full declaration extent |
|---|---|
| `create_table` or its direct `table_name` | Actual `create_table`, excluding the outer statement semicolon |
| `column_definition` or its direct `column_name` | Actual column definition, including type/default/constraints and excluding comma |
| `variable_declaration` | Actual declaration, including its semicolon |
| `create_function_body/function_name` and `create_procedure_body/procedure_name` | Actual standalone declaration; qualified header components remain separate |
| `function_body/identifier`, `function_spec/identifier` | Actual local function body or specification, including its native delimiters |
| `procedure_body/identifier`, `procedure_spec/identifier` | Actual local procedure body or specification, including its native delimiters |
| `create_package/package_name`, `create_package_body/package_name` | Actual package specification or body; only its mandatory header declares the Package |
| `parameter/parameter_name` | Actual individual parameter, without widening to its callable |
| `type_declaration/identifier` | Actual local Type declaration, including its semicolon |

Identity subjects are exact original name spans, distinct from those complete
extents. The [extent primitive](rust_ast_entity_extent.md) validates the supplied
original allocation/range. It does not select an arbitrary ancestor, widen over
trivia or create a hull. Explicit configured span captures remain authoritative:
a missing, nonexact, recovered or incomplete fragment makes code unavailable.
Raw type observations retain the actual `datatype`, `type_name` or `type_spec`
projection; they are not evaluated type-system facts.

For the stock Oracle `create_table` context, an omitted name path selects the
whole declaration in raw observations. The model obtains the actual canonical
`table_name` child, rather than decoding the declaration as a name. An explicit
nonempty context path must select that native subject; an invalid/missing owner
is diagnosed instead of silently replaced. Self exclusion compares actual
declaration ordinals. Equal names or ranges do not establish self or ownership.

```sql
CREATE TABLE "ACME"."PEDIDO"
  ("ID_PEDIDO" NUMBER,"DT_PEDIDO" DATE,"VL_TOTAL" NUMBER);
```

This produces Table `PEDIDO` and three Columns with a genuine `PEDIDO` Table
namespace. `ACME` is not a Table owner. If an emitted owner tuple exists, the
owner link records its model index; a context-only namespace does not fabricate
an emitted entity or a graph edge. Supported native named owners are Table,
Function, Procedure, Package and registered local Type. Other configured owner families are diagnosed
as unsupported. Anonymous DECLARE variables legitimately
have no configured named owner.

The optional package name following `END` produces a typed `DeclarationReference`
observation rather than another Package entity. It retains its actual source and
header declaration ordinal without attaching an error to the valid header.
Package specification and body remain separate declarations and identities.
Function return `type_spec` is a native Return observation; it does not infer a
resolved type or classify an invocation as a function rather than collection access.

## Oracle native Parameter and Type observations

The [declaration helper](../../crates/graphit-ast/src/antlr_engine/model/oracle_declarations.rs)
adds borrowed intrinsic facts through AntlrEntity::oracle_declaration(). Parameter
contains every native IN/OUT/INOUT/NOCOPY terminal in order and an optional default
expression. G4 permits omitted types and repeated modes; those facts remain
observable without pretending Oracle server validity. A default is separate from
the query's declared_value: NotConfigured is preserved when no value capture exists.

Type(Box<AntlrOracleTypeDeclaration>) retains kind, complete definition, optional
element/index type observations, optional bound expression and native modifiers.
NestedTable and AssociativeArray are distinguished by table_indexed_by_part;
Varray retains VARRAY/VARYING ARRAY/ARRAY and its bound expression. Record and
RefCursor remain distinct non-collection facts. Direct native parents prevent a
RECORD field's NOT NULL from becoming an outer Type modifier. Nominal and
%TYPE/%ROWTYPE syntax remains actual type evidence, without a resolved binding.

Keywords and expressions borrow the actual SyntaxNode and OriginalProjection.
An absent optional field differs from a required expression lost during recovery:
the latter is Some(Missing). A present Regular node can itself have an empty native
span, as observed for malformed VARRAY(); it stays present with its exact zero-width
projection and a MissingNativeField diagnostic. Required empty fields cannot grant
code availability. No parser input, node status or Origin is repaired.

The existing Graph adapter preserves these facts in optional oracle_declaration
metadata only for the new subjects. Other entities, DB2 QName semantics and the
NN schema/domain stay unchanged. Parameter embedding follows effective embed_labels;
the genuine NN fixture enables Parameter explicitly. Fixtures use controlled input
vectors to qualify the codec, not model inference. All facts and full Graph properties
remain stable across independent parses and owned transfer after parser owners drop.

## Values, contributions and conflicts

The mature locked `sqlparser` tokenizer decodes one native identifier word,
including Unicode and doubled double quotes, and preserves its original lexical
anchor. Extra tokens or other quote styles are not repaired, trimmed or case
folded. Original and prepared identifier decodes must agree. Callable and package
qualified names use actual native `id_expression` components: each has its own
Original range, decoded word, quote style and node ordinal. The final component
is the entity/namespace identity anchor; the selected wrapper remains in the raw
contribution, while `AntlrQualifiedName` retains the complete ordered components.
A package schema component does not create a Schema frame or a combined span.
Introducer syntax outside this declaration contract is rejected explicitly.
Other Oracle call-chain and database-link families still require their own
semantic contracts; DB2 QName anchors follow the separate contract below.

Scalar values retain raw ranges plus actual tokenizer Number, string/national
string or TRUE/FALSE/NULL tokens. Numbers stay textual, without `f64` conversion.
Empty, escaped, long and multiline strings retain their full bytes. Other
expressions, including `1+2` or unary expressions, remain raw `Expression` values;
the model does not evaluate them. Missing, NotConfigured and NonExact values
remain distinct.

Merge uses the genuine `(uid, data_key)` tuple. Stock name-only and value queries
for the same declaration merge concordant full extents and preserve both original
contributions and query provenance. NotConfigured does not assert absence.
Disagreeing configured values/extents remain conflicts; a bad owner observation
also makes the matching projection unavailable independently of query order.
The same UID under different data keys remains two entities. Homonyms at different
native anchors, labels, contexts or paths remain distinct identities.

[Common semantic fields](../../crates/graphit-ast/src/semantic_fields.rs)
provide `ExtentObservation`, `ContributionConflictField` and
`ContributionConflict` to both this model and the active
[JSON model](../../crates/graphit-ast/src/semantic_model.rs). There are no JSON-named
aliases, generic assembly trait or new UID format. Value/parser/contribution and
aggregate result types remain specific to each model.

## Available code and failures

For configured `embed_labels`, `code_projections()` returns Available only with
an exact complete declaration extent, exact required owner extents, no relevant
diagnostics and no contribution conflicts. Its text is the complete original
declaration bytes, without an embedding prefix or name-only snippet. Otherwise
Unavailable retains the entity index and diagnostic/conflict references. An
exact name with a compound extent may have a genuine identity while code remains
unavailable.

Systemic failures return `AntlrModelError` with the actual operation, raw parser,
origin, identity, extent, range or regex policy cause; no partial model escapes.
Missing/recovery/nonexact/unsupported declaration or owner observations remain
indexed diagnostics. A name intentionally excluded by policy also retains an
explicit diagnostic and its raw occurrence. The original request is checked
before and after collection, assembly and code delivery, including error exits.
Actual cancellation and expiry after collection prevent further delivery.

## NativeNN Oracle codec

[NativeNnAntlrInput and encode_native_nn_antlr](../../crates/graphit-ast/src/native_nn.rs)
consume the genuine assembled model and indices into its own entities, with caller-observed
vectors in selection order and a model generation. The [private Oracle codec](../../crates/graphit-ast/src/native_nn_antlr.rs)
calls `code_projections()` once. Unknown, duplicate, non-embedding or unavailable selections,
missing generations, wrong vector width and nonfinite values fail the entire batch.
No projection DTO or reconstructed owner establishes provenance.

The same Original request and full source allocation must belong to the model and the
observed Raw snapshot. Path components and full-source hash are checked. Rows preserve
the observed Raw version, physical branch and inherited target; these are storage observations,
not permissions or freshness grants. `TextNotStored` accepts an actual PROJECT physical
capture for the same path and content, revalidated on delivery and error exits. `FileAbsent`
and an indexed allocation paired with a different physical allocation fail explicitly.

Rows use the existing twenty-column Arrow schema and `ast-parser-entity-original-1` recipe.
CodeDocument is the full original native declaration, without a prefix, manual reconstruction
or another parser pass. The immutable `(uid, data_key)` tuple allows the same genuine UID
in different configured data keys and rejects the exact duplicate tuple. The model ID,
revision, config ID and dimensions remain part of schema identity even for empty batches.

The persisted name domain is explicitly `plsql`/`antlr-plsql`. A mature SQL tokenizer validates
one actual identifier with no quoting or double quoting, including doubled quotes and Unicode;
decoded names differ from the retained raw anchors. The encoder checks namespace names
against actual source anchors. The persisted validator checks name anchors inside the projection
and structural namespace ranges; it cannot recover source bytes outside the projection or
restore native provenance from rows. Rust and JSON retain their own name rules. No schema
expansion or custom identifier decoder is introduced.

The [Oracle NN fixtures](../../crates/graphit-ast/tests/native_nn_antlr.rs) exercise the genuine
model through actual Raw named-branch capture, codec, private Main creation, native SDK
publication and nearest-vector query, preserving both data keys. They also exercise actual
NULL physical capture and changed-file rejection. The vectors in these codec fixtures are
caller observations; they do not prove an inference runtime or indexing callback.

Graph publication, SCIP, resolved targets, metrics, all-language models and the production
Raw/indexing callback remain separate deliveries.

## Qualification

The [Oracle fixtures](../../crates/graphit-ast/tests/antlr_model.rs) exercise the
stock Table/three Columns, declaration/value merge, quoted/Unicode identifiers,
exact numeric tokens, long strings/raw expressions, data keys/homonyms, actual
bad captures/parent/span and conflicts in both orders, unresolved relations,
compound origins, native recovery and original cancellation/expiry. The
[JSON semantic fixtures](../../crates/graphit-ast/tests/semantic_model.rs) and
[JSON NativeNN fixtures](../../crates/graphit-ast/tests/native_nn_json.rs) exercise
the actual second consumers of the neutral fields.

```text
cargo xtask cargo test -p graphit-ast --locked --offline --jobs 4 --test antlr_model --test semantic_model --test native_nn_json
cargo xtask cargo clippy -p graphit-ast --locked --offline --jobs 4 --lib --test antlr_model --test semantic_model --test native_nn_json -- -D warnings
```

The current Linux runtime qualification passed nine Oracle, seven JSON semantic
and three JSON NativeNN fixtures. Initial fixture schema/origin-plan mistakes
and a Rust temporary-lifetime failure remain recorded in
`.build/ast-oracle-model-fcc0/qualification-current.json`. The first Clippy run
found an own collapsible conditional, corrected without changing its behavior.
Final strict/fmt/links/Source-to-Knowledge evidence is recorded in that receipt.
Windows/macOS are unrun. Existing S7, parser, identity, extent and other NN
producer suites were reused unchanged rather than replayed.

### NativeNN Oracle qualification

The current Linux AM9 fixture passes actual PEDIDO Table rows in two data keys and
three Column rows through Main creation, publication and SDK query, checking genuine
UID/data keys, decoded names, full projections and namespaces after query. The source
comes from an actual named Raw branch. Codec negatives cover invalid parent/extent
conflicts, native recovery, unavailable/non-embedding selections, empty schema,
foreign Original/allocation/path, model schema/width/config/revision, vector cardinality
and finite values, exact duplicate tuples, NULL physical source, FileAbsent and changed files.

```text
cargo xtask cargo test -p graphit-ast --locked --offline --jobs 4 --test native_nn_antlr --test native_nn_json --test native_nn
cargo xtask cargo clippy -p graphit-ast --locked --offline --jobs 4 --lib --test native_nn_antlr --test native_nn_json --test native_nn -- -D warnings
```

All thirty existing NN and three JSON NN regression fixtures passed in the affected run.
The final three Oracle fixtures passed after completing the five-row SDK case. Source
was unchanged during that final fixture correction. The receipt
`.build/ast-oracle-nn-cafe/qualification-current.json` retains the helper's initial Arc
signature compile error and the fixture's initial Arrow Arc comparison compile error,
with their causal fixes and current results. Scoped Clippy passed for all affected NN targets.
After the parallel Catalog input cleanup, the fixture retained only `policy_entries` and
`selector_bytes`; the causal lib/Oracle-fixture Clippy passed again without replaying
unchanged NN runtime tests. This qualifies SDK publication/query with
caller-observed vectors; production indexing callbacks and inference remain separate.
Windows/macOS are unrun. Parser, S7, identity, model, M and authentication providers
were reused without replay.

## DB2 observations and NativeNN

The DB2 consumer is implemented in the same collector/assembler and a private DB2 registry.
It retains actual current stock subjects, configured labels/data keys, declaration
extents and native ancestry. name()/namespace name() are local decoded display
components; identity subjects remain complete native QName anchors.
qualified_name().components() exposes native component ordinals, exact Original
ranges, decoded values and quote styles. context_node_index() retains the raw
context provenance separately from the canonical native owner declaration.

AntlrContribution::type_roles retains explicit Declared, Return, CastSource,
CastTarget and ReturnColumn observations in native preorder. Index key columns
expose IndexKeySpecification roles; they are not physical TableColumns. Named
Schema ancestry is genuine, while an unnamed AUTHORIZATION Schema produces no
invented entity/namespace. Grouped Variables retain separate subjects and shared
statement extents. Subject diagnostics block only the affected native subject;
a malformed capture of the shared declaration with no determinable subject
blocks all matching declaration subjects instead of selecting the first.

The [ANTLR NativeNN encoder](../../crates/graphit-ast/src/native_nn_antlr.rs)
selects the name domain from the actual projection pair and shares the DB2
mature sqlparser QName decoder with the model. Both entity and owner lexical
anchors match their local decoded display. Existing same-allocation/path/Original
Source association and projection availability guards remain unchanged.

[Current affected tests](../../crates/graphit-ast/tests/antlr_db2_model.rs) consume
the real collect/assemble/projection/encoder APIs. Their receipts distinguish
native syntax from server catalog classes, Graph resolution and inference.
The Oracle qualification below/above remains historical Oracle evidence; DB2
qualification is recorded in its task packet and current model receipt.

## Method CAST REF typed observations

The actual DB2 model consumes the existing target/source native wrappers when
their new child is `method_reference_type`. CastTarget and CastSource retain the
complete REF literal and Original range; scalar slots have no paired return-column
name. Namespace/identity belong to the genuine Method and its actual declaration
ancestry. The nominal referent creates no synthetic Type entity or owner.

This consumer uses the existing collector/assembler and typed-role APIs; no
catalogue classification or castability check is introduced.

## Oracle native application groups

The Oracle model exposes borrowed `oracle_applications()` and
`oracle_application(occurrence_index)` observations for native
`general_element_part`, `keyword_as_method` and `routine_name` CALLS
occurrences. Each retained occurrence keeps its own ordered groups and direct
arguments, optional native argument name, expression and KEEP clause. Repeated
groups remain distinct; nested argument lists belong to their own heads.
Origins are projected independently while PreparedInput is alive. Native
status/error/missing and nonexact origins remain observable; node presence
does not establish validity.

Boundaries are native ancestors in nearest-to-outer order, including bodies,
blocks, anonymous blocks, callable declarations, parameters and defaults.
A variable initializer can contain a `default_value_part`; the model retains
that actual ancestry. These observations do not assign a caller or classify
Function versus collection indexing. Other dedicated/dynamic/raw relations stay
retained without fabricated generic argument groups.

The C7 syntax producer adds optional `oracle_application` to the existing
`observation_json` and FILE outcomes without granting a source or endpoint.
It preserves ObservedOnly, no source index and no edges. The separate local
callable consumer can supplement eligible occurrences without changing the
NN format, shared DTO, protocol, profile or grammar.

Boundary metadata contains only native status, ordinals and explicit Original ranges; it omits literal_source so callable/body text is not duplicated per outcome. Borrowed native Origins remain unchanged.

## Native collection statements and assignment context

The C8 extension adds three explicit raw selectors for native `collection_method_call` terminals DELETE, EXTEND and TRIM, using the current generic SDK compiler. The three PL/SQL stock copies keep all preceding queries in order; their query count becomes 172. The previous seven-family/84-descriptor dedicated-call fixture remains its own qualified scope.

`AntlrOracleApplicationKind::CollectionMethodStatement` retains the actual statement, first direct expression as receiver, subsequent direct expressions as indices and direct native delimiters. It does not fabricate `function_argument` groups. Native syntax does not prove the receiver's type or a Function endpoint. `assignment` records the nearest assignment statement, its direct Target/Value branch and actual intervening expressions. An application inside a target index retains that path; the location is not classified as a semantic write.

The collector independently projects these nodes while PreparedInput is alive. Graph adds optional `collection_statement`/`assignment` within the existing observation JSON. The collection/LHS producer grants no callable endpoint and omits literal_source for structural statement/branch/path nodes. Separate callable expressions within an index or assignment value can receive local CALLS evidence from the consumer below. Receiver/index expressions retain their own native facts. No Source authority, NN schema, SDK/G4 or DTO protocol is changed.

## Oracle local binding

The B1 consumer adds `oracle_bindings()` and `oracle_binding(occurrence_index)`.
Each `AntlrOracleApplicationBinding` retains receiver/head lookup, optional
declared-type lookup, classification, collection type, native type evidence and
typed issues. Lookups distinguish Resolved, Ambiguous, Missing and Indeterminate;
candidates retain declaration/name/scope nodes, decoded name/quote style and real
entity indices, which can be empty or plural.

Lookup uses the complete native registry, including declarations excluded by
profile queries or rejection policies. It follows lexical scope and declaration
visibility, resolves a value/parameter type at its declaration, and never chooses
the first of several same-scope candidates. Cursor, exception, subtype and
conditional declarations can prevent an outer collection match.

A collection classification requires an actual local Type5 declaration and its
required fields, checked by the shared ef54 predicate independently of emitted
entities. Empty/recovered/nonexact element/index/bound fields remain explicit
issues. One argument group can become CollectionIndex or CollectionConstructor
(NestedTable/Varray); the dedicated statement becomes CollectionMethod. A Type
in a direct assignment Target is InvalidContext. A constructor evaluated inside
a target index retains its expression context. Repeated result application,
external/type-alias/function resolution remain necessary. The following B2
section records the supported native member subset.

## Native collection member binding

B2 adds optional `AntlrOracleApplication.member`: the actual general_element,
a complete receiver node when one exists, name, PERIOD separator and own group
delimiters. Bare accessors keep zero groups; EXISTS keeps its native sibling
groups. An incomplete receiver is None rather than a range assembled from parts.

`collection_method` distinguishes Count/First/Last/Limit/Next/Exists and the
existing Delete/Extend/Trim statements. The six accessors require B1's unique
local Value/Parameter and complete Collection Type5 proof. COUNT/FIRST/LAST/LIMIT
have no groups; NEXT/EXISTS require one positional, nonempty, usable expression.
UnsupportedInvocation retains invalid forms. Record/scalar/type-head, quoted
method names, external/alias types and unproven receivers do not become methods.

The native prefix lookup stops at the proven receiver; ordinary reference
lookup keeps its chain rejection. A continuation in general_element ancestors
also prevents partial classification, including `(xs.COUNT).NEXT(1)`. The search
stops at an expression boundary, preserving `xs(xs.COUNT):=1`,
`n:=(xs.COUNT)` and a member inside an argument expression.

## Local Oracle callable references

The document's borrowed oracle_calls vector keeps a call_index ordinal, original
occurrence_index, source/target states, actual native caller and body, evidence,
causal issues, optional supplementary call_lookup and one outcome per direct
argument group. The first group can refer to the head symbol; later result
applications keep their own native group with UnsupportedInvocation. Bare
procedural calls have zero groups. The retained raw relation inventory, B1 binding
and B2 collection classification remain independently available.

A source must be the nearest actual callable whose own native body contains the
head. Defaults, declaration initializers, specifications and anonymous roots grant
no caller; no outer callable is substituted. Targets reuse native lexical
visibility, shadowing, sqlparser identifier decoding, origin and scope continuity.
A scalar/type/collection reference remains with its existing producer. Package,
external, qualified, database-link and dynamic-result references receive no
fabricated Function endpoint. Self-recursion needs its own name/body/continuity
proof and cannot clear a B1 issue globally. Resolution denotes a local symbol
reference, not Oracle compiler validation of arguments or overloads.

Graph source_index addresses oracle_calls[call_index] when source is Bound;
entity_index is a distinct ordinal. capture_index remains absent without a real
capture ordinal. Only Bound source plus Resolved target can emit an edge between
their actual node keys. UID uses the existing relation/source/target/data-key/exact
Original capture tuple. Structured oracle_call metadata retains causes and native
ranges without structural literal_source or whole-owner debug output. B1 metadata
is preserved even when the supplementary procedural lookup succeeds.

The qualified OC producer emits its supported Function/Procedure labels with
canonical native name captures; arbitrary graph labels are not an added feature
of this consumer. Configured data_key and actual identities remain intact.
The reviewed implementation now checks verified contiguous Compound structural
diagnostics, TargetRule labels for resolved and ambiguous candidates, and direct
assignment heads separately from invocations inside indices. Graph target causes
retain the target issue even when a source issue has the same reason. The
four correction regressions passed in those affected targets. Two additional
consumer focals were appended after App13 to close F3/F5 assertion gaps; their
runtime passed in the current complete AST17 targets; scoped strict also passed. The parent and session
remain open; this consumer does not by itself complete C7–C9 or publication.

The post-App16 eligibility correction retains Indeterminate lookup causes even
when all possible candidates are noncallables. Only Resolved/Ambiguous proven
noncallable references stay outside the CALLS carrier; an uncertain charset/name
comparison cannot silently discard the occurrence. Both complete affected AST17
targets qualified this correction without weakening the F3 expectation.
