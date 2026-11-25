---
title: Rust AST ANTLR semantic occurrences
description: Effective profiles bound to native captures and original origins.
doc_type: technical
references:
  - target: {type: task, id: tsk-2526}
    relation: implements
  - target: {type: task, id: tsk-fb2c}
    relation: implements
  - target: {type: task, id: tsk-6eae}
    relation: implements
  - target: {type: task, id: tsk-0b48}
    relation: implements
  - target: {type: task, id: tsk-82b9}
    relation: implements
  - target: {type: task, id: tsk-f627}
    relation: implements
  - target: {type: task, id: tsk-2558}
    relation: consumes
  - target: {type: task, id: tsk-0b99}
    relation: derived_from
  - target: {type: task, id: tsk-0eae}
    relation: supports
  - target: {type: task, id: tsk-5cb2}
    relation: supports
  - target: {type: task, id: tsk-04a9}
    relation: supports
  - target: {type: task, id: tsk-1d33}
    relation: implements
  - target: {type: task, id: tsk-a4cd}
    relation: derived_from
  - target: {type: task, id: tsk-0953}
    relation: uses
  - target: {type: task, id: tsk-3f5d}
    relation: uses
  - target: {type: task, id: tsk-0395}
    relation: uses
  - target: {type: task, id: tsk-e056}
    relation: uses
  - target: {type: task, id: tsk-8b3f}
    relation: relates_to
  - target: {type: task, id: tsk-73e3}
    relation: implements
  - target: {type: task, id: tsk-1f8a}
    relation: consumes
  - target: {type: task, id: tsk-b847}
    relation: specifies
  - target: {type: task, id: tsk-929f}
    relation: implements
  - target: {type: task, id: tsk-80fb}
    relation: implements
  - target: {type: task, id: tsk-8187}
    relation: implements
  - target: {type: task, id: tsk-1c45}
    relation: implements
  - target: {type: task, id: tsk-044f}
    relation: implements
---

# Rust AST ANTLR semantic occurrences

For AST extractor and indexer maintainers in graphit-code. Task `tsk-1d33`
specifies this next consumer of the qualified [profile query API](rust_ast_antlr_profile_queries.md).
The [consumer](../../crates/graphit-ast/src/antlr_engine/semantic.rs) is implemented;
The current five shipped profiles and their PostgreSQL body/COBOL preprocessor
consumers pass the Linux S7 configuration and concurrent-consumer fixtures.
[Profile reconciliation](rust_ast_antlr_profile_reconciliation.md) records the
delivered producers and the historical failures.
The complete [semantic extraction contract](rust_ast_semantic_extraction.md)
remains tracked by `tsk-a4cd`.

## Consumer and borrowed data

`antlr_engine::semantic::parse_semantic_occurrences` accepts one
`EffectiveProfile` and the actual `PreparedInput`. It parses the prepared text
once using the official SDK, binds configured captures, and returns typed
entity/relation occurrences plus contexts from the same native syntax.

```rust
pub fn parse_semantic_occurrences<'profile, 'o, 'v>(
    profile: &'profile EffectiveProfile,
    input: &PreparedInput<'o, 'v, '_, '_>,
) -> Result<SemanticDocument<'profile, 'o, 'v>, SemanticError>;

pub struct SemanticDocument<'profile, 'o, 'v> {
    pub profile: &'profile EffectiveProfile,
    pub syntax: ParsedSyntax<'v>,
    pub occurrences: Vec<SemanticOccurrence<'profile, 'o, 'v>>,
    pub contexts: Vec<SemanticContext<'profile, 'o, 'v>>,
}

pub enum CaptureBinding<'o, 'v> {
    NotConfigured,
    Missing,
    Present(CapturedNode<'o, 'v>),
}

pub struct CapturedNode<'o, 'v> {
    pub node_index: usize,
    pub prepared: Option<SourceRange<'v>>,
    pub original: Option<OriginalProjection<'o>>,
}
```

Each occurrence retains its original profile `query_index` and a borrowed
`Query`. Its query kind distinguishes entity and relation occurrences. The
reference preserves data key, graph label, relation type, value label, target
labels/fallback, name rejection, name-as-data policy and effective provenance.
Occurrences preserve query/preorder ordering and duplicates across queries;
they do not merge or discard homonyms.

An occurrence has `matched`, `name`, `value`, `span`, `parent` and `qualifier`
bindings, plus context indices into `document.contexts`. The matched binding
always addresses its actual native node. All other present bindings address
nodes in that same `syntax`.

Captured text is raw prepared text. Original projections retain `Exact`,
`Compound`, `Synthetic`, `Unmappable` and `Ambiguous` outcomes. A recovered
unindexed node remains present with unavailable ranges; its class, status,
error and missing flags remain in `syntax`. No artificial zero range or hull
is created. Only a verified exact original declaration extent may later feed
the [extent primitive](rust_ast_entity_extent.md).

## Capture meanings

ANTLR `captures` maps semantic slots to ordered native operations. An omitted
name or empty operation list selects the matched node. Missing value/span/parent/
qualifier slots remain `NotConfigured`; a configured operation without a target
is `Missing`. `xpath` uses the official runtime's relative child/descendant
selection. `nearest_descendant` retains breadth-first nearest-depth semantics;
`nearest_ancestor` excludes the matched node and does not retry a farther anchor
when a later operation fails.

Configured missing names never become another node's name. Invalid selectors or
vocabulary are configuration errors, even when no tree node matches. Name
decoding, identities, exports, metrics, entity merging and graph publication
remain subsequent consumers of these observations.

## Native contexts

`antlr_contexts` contains official XPath owner definitions with optional direct-
child `keyword`/`child_rule` conditions and `negate`. Each definition supplies a
label and ordered name operations. For example:

```yaml
antlr_contexts:
  - pattern: '//createfunctionstmt'
    keyword: FUNCTION
    label: Function
    name: [{xpath: '/func_name'}]
```

Each `SemanticContext` retains its selector, optional label, explicit-name flag,
matched binding and name binding. An empty name operation list selects the
context node; missing explicit names remain missing. Contexts are unique by
native node and ordered by native preorder. Overlapping definitions still fail
with `OverlappingContextSelectors` instead of silently changing precedence.
Occurrence scopes follow actual parents nearest first and exclude the matched
node. Display-name equality does not merge distinct owners.

Prepared selectors belong to the effective profile, separately for primary and
COBOL preprocessor grammars. Plans retain bound immutable vocabulary IDs and
official runtime XPath elements, with no parser/tree/source pointer. Per-file
results and request callbacks remain independent. See the complete
[selection/capture contract](rust_ast_antlr_profile_queries.md).

## PL/SQL raw call candidates

Product `tsk-5cb2` implements the stock correction defined by `tsk-0eae`. Its
Linux native fixture passed eight cases plus custom multiplicity and original
cancellation. The delivered child-rule guard supports the
`//general_element_part[rule:function_argument]` selector. It captures local
`id_expression` and ancestral `general_element`; statements retain raw
`routine_name` and ancestral `call_statement`. Names, matched extents and raw
qualifiers remain distinct bindings with the original profile metadata.
Qualifiers may include arguments/dblink and are not decoded qualified names.

A `function_argument` child is syntax, not proof of a semantic invocation.
Collection indexing, constructors and assignment LHS can share this shape.
`itens(1) := fn(2)` retains both candidates; `fn()()` retains one part and its
multiple argument children. Future CallSite classification/cardinality must use
closed native/declaration contracts, not the presence of parentheses or spelling.
No CALLS edge is published by the raw occurrence API.

The stock correction removes only the ASCII-first-character reject
branch for quoted/Unicode IDs; custom rejects and duplicate queries retain their
configuration and multiplicity. The invalid `procedure_call` entry was removed
after the native statement fixture proved simple, CALL, qualified, no-argument,
dblink and chain coverage through `call_statement/routine_name`. Dedicated function families, collection
methods without arguments and tabular producers are mapped by completed contract `tsk-04a9`.
The cohesive producer `tsk-b847` adds 84 explicit native keyword descriptors
(169 queries in that qualified snapshot); raw runtime, affected inventories and scoped strict Clippy passed on Linux. The current stock has172 queries after three C8 native collection statement selectors. Receiver/type/name binding, bare COUNT/FIRST and constructor discrimination, and resolved CALLS/Graph edges remain necessary in `tsk-a4cd`.
See [profile reconciliation](rust_ast_antlr_profile_reconciliation.md) and
[full semantic boundaries](rust_ast_semantic_extraction.md). Invalid selectors
remain typed SDK errors; the other profile/family corrections are still pending.

## Request, routing and errors

The read-only `PreparedInput::original_context()` borrows exactly the
retained `SnapshotContext`. The consumer uses that request for native parsing,
capture materialization, projection and final delivery checks. There is no
alternate context argument or copied request.

Routing supports `ProfileParser::Antlr4` and the five exact grammar names
`antlr-plsql`, `antlr-postgresql`, `antlr-db2`, `antlr-tsql`, `antlr-cobol85`.
Omitted start rule uses the delivered SDK entry point; a supplied rule must
equal that entry point. Other parsers, grammars or supplied start rules return
typed unsupported errors. Profile language/grammar metadata remains unchanged.

`SemanticError` retains operation, configuration, native engine and projection
causes. Native query errors additionally identify either the original profile
query index or the context `selector`. Profile queries form the native request prefix;
context queries follow it. The inner SDK `QueryFailure` retains its actual
index, capture slot, cause and original control error. No partial document is
returned after an error. SDK lexical/syntax diagnostics remain in `syntax`.

## Fixtures and qualification

The current Linux qualification of `tsk-1d33` runs the S7 fixture separately and
only the affected semantic cases, then scoped Clippy:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_semantic all_shipped_antlr_profiles_compile_and_produce_occurrences -- --exact
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test antlr_semantic -- -D warnings
```

The [functional fixtures](../../crates/graphit-ast/tests/antlr_semantic.rs) preserve
effective query references/provenance, raw entity/relation metadata, duplicates,
missing captures, literal context names, native classes/parents, Original
projections and actual SDK errors. S7 compiles every current pattern, configured
capture, context selector and context name path independently against its declared
tree. Stock query counts are not frozen. A valid selector yielding no match stays
valid; an invalid selector stays an error.

S7 passed 1/1, and fourteen affected semantic fixtures passed, including the five
concurrent consumers and the three focused stock capabilities whose artificial
count checks were removed. PostgreSQL includes homonymous Function and Procedure
PL/pgSQL bodies with real `v` Variable captures and configured IF observations.
Capture slots resolve to retained body nodes, while host contexts follow actual
native parents. COBOL includes an actual `COPY BOOK.`: the preprocessor occurrence
addresses its own syntax and links through the main COPY token to the actual
`programUnit` and nearest-to-outer main contexts. No sidecar is discarded before
these assertions. Each primary SQL profile also produces its real Table
capability; COBOL produces its Program capability.

Two further S6/PL/SQL-call fixtures passed in this qualification. Unchanged
error-origin, request cancellation/deadline, ancestor, inserted-recovery and
Db2-header evidence is reused from qualified `tsk-0b48`/`tsk-f627`; the producer
suites were not replayed. The current
receipt is `.build/ast-semantic-s7/qualification-current.json`. No production
consumer, stock profile, bridge or generated grammar changed in this unit.

The current TSQL S7/concurrent fixture also executes the qualified ordinary
`REVOKE SELECT ON OBJECT::dbo.pedido FROM leitor;`. Its actual `revokes` occurrence
retains Relation/REFERENCES metadata, the empty graph label and the exact raw
`dbo.pedido` name on the native `table_name` beneath `revoke_permission`. Query
identity and prepared/Original captures use that TSQL syntax. The earlier S7 pass
compiled this descriptor but did not observe REVOKE; review found that coverage
gap and the actual fixture was completed without changing its producer.

### Historical qualification failures

The initial Linux runs exposed fixture schema assumptions and invalid stock
selectors, then passed fourteen focused fixtures while the two full-profile
fixtures still failed. The original 346-query inventory and its 21 invalid
pattern/context fields remain historical evidence. Their real corrected
producers, including Db2 headers, TSQL REVOKE, PostgreSQL body composition and
COBOL COPY, are now consumed by the current S7 tests; the earlier failures do not
represent the current status and were not erased or converted into passes.

This qualification covers raw semantic observations and configuration routing.
Resolved owners, decoded names, UIDs, final metrics, graph publication and copybook
filesystem expansion/REPLACE execution remain subsequent consumers. Windows/macOS
execution is unrun. No general benchmark or comparison campaign is required.

## Db2 header occurrences

Product `tsk-f627` binds the shipped Db2 profile to nine native Parameter header
paths. Named slots yield one raw occurrence; unnamed slots yield none. Cursor
parameters, local declarations and named CALL arguments stay outside this set.
SOURCE and FINALIZE routine names remain references, without extra Function or
StoredProcedure declarations. Eight Function selectors include the native
aggregate and the two Method headers; three Procedure selectors select direct
header names. Methods retain the existing Function label without assigning a
resolved owner, a new graph type or a UID.

Aggregate and Method contexts have explicit raw name paths. Ordinary Function
and Procedure wrapper contexts use the SDK's nearest name path. An occurrence
keeps actual native parent/context ordinals, the borrowed effective query and its
provenance, and raw prepared/original captures from the same request. No generic
consumer or parser ABI change is required. LINE_COMMENT and SQL_COMMENT are
valid metadata tokens; this metadata alone does not publish Comment entities.

The [Db2 header selectors](rust_ast_antlr_queries.md#db2-header-parameter-selectors)
and [reconciliation scope](rust_ast_antlr_profile_reconciliation.md#db2-aggregate-and-shipped-header-consumers)
match the three active stock copies. The Linux focal, both affected corpus
fixtures, typed query-error regression and strict Clippy passed. Initial fixture,
compile and dependency-warning failures remain recorded.
Full shipped-profile S7, the remaining nominal Method
CAST/procedure-type questions, graph publication, server execution and
Windows/macOS remain outside this bounded qualification.

## PostgreSQL host and PL/pgSQL body composition

[`parse_postgresql_semantic_occurrences`](../../crates/graphit-ast/src/antlr_engine/semantic/postgresql.rs)
parses the effective PostgreSQL host profile once and consumes an already prepared
PL/pgSQL Tree-sitter profile. It returns the unchanged host `SemanticDocument` and
specific body sidecars. Each sidecar retains the actual body tree, grammar owner,
nodes and effective query observations. Host `CapturedNode.node_index` continues
to address ANTLR nodes; body ordinals always address that sidecar's nodes.

`BodyMatchScope` identifies a query group and match, mapping its ordered capture
slots to body node ordinals by native `node_id`. Parent ordinals remain local to
that retained body tree. The body root connects through `body.host_parent` to the
actual host parent chain. `host_contexts` is stored once per body in nearest-to-outer
order and addresses the host's contexts. Homonymous functions and procedures remain
separate; names and containing byte ranges never establish this link.

`BodyDecision` records nodes selected by the PL/pgSQL profile's actual
`complexity.node_types`, with prepared and original projections. It is a raw
observation, not a final complexity score. PostgreSQL stock no longer asks its SQL
parser for `decl_statement` or PL/pgSQL decision node kinds. Variables and decisions
are obtained from the existing PL/pgSQL stock profile instead.

Body captures and decisions use the same `PreparedInput` origin plan as the host.
Exact offsets, compound pieces, synthetic or unmappable results and ambiguity are
preserved by the origin producer. Source is borrowed; native body trees own their
SDK storage and grammar/library lifetimes. SQL-language bodies, single/E/U& literals,
unrelated dollar strings and empty bodies do not become PL/pgSQL sidecars. Host
routine observations and recovery diagnostics remain available.

Original operation checks precede optional cancellation/deadline controls, run
through the native work and projection, and run before delivery. Errors retain the
host, embedded SDK, origin or native-contract cause and return no partial document.
The caller supplies the actual PL/pgSQL grammar from its trusted loader; language
metadata is not proof of a grammar's identity. The collector validates the host's
prepared buffer binding without copying or reparsing it.

The [native composition fixtures](../../crates/graphit-ast/tests/postgresql_semantic.rs)
cover stock PostgreSQL and PL/pgSQL, Function/Procedure homonyms, Unicode/CRLF,
body scopes, owners, controls, origins and negative routing. Qualification is
Linux-only; Windows/macOS, full graph assembly, UIDs, resolved graph owners and
publication remain outside this unit.

## COBOL COPY observations and native program scopes

[`parse_cobol_semantic_occurrences`](../../crates/graphit-ast/src/antlr_engine/cobol_copy.rs)
returns `CobolSemanticDocument { main, preprocessor }`. The main parser consumes
queries whose source is `primary`. The official `Cobol85PreprocessorParser`
consumes queries whose source is `cobol-preprocessor`, preserving original query
indices, effective metadata, configured capture slots, raw bytes and multiplicity.
Each preprocessor occurrence retains its own `SemanticOccurrence` and
`CopyProgramScope`; its matched/capture ordinals address only
`preprocessor.syntax`. `preprocessor.grammar()` identifies `Cobol85Preprocessor.g4`.
`main` node ordinals and context indices keep their existing meanings. The
main-only consumer rejects a secondary source instead of silently omitting it.

An internal SDK ancestor capture obtains the actual COPY terminal below
`copyStatement`. It links to a main COPY terminal or error node with the identical
checked prepared-byte span in the same borrowed source, then follows actual
main-tree parents to the nearest `programUnit`. `Linked.program_node_index` and
`context_indices` address `main`; context indices remain nearest-to-outer.
Equal names, source containment and line guesses do not establish scope.
`NoAnchor`, `UnindexedAnchor`, `OutsideProgram` and `AmbiguousAnchor` preserve
unresolved observations explicitly. COPY query syntax comes from the
preprocessor tree. The [main directive producer](rust_ast_antlr.md#cobol-native-copy-directive-parents)
keeps the real COPY terminal under programUnit; any main recovery remains explicit.
Both SDK diagnostic lists remain intact.

The caller can prepare physical Fixed, Variable or Tandem source with
[`prepare_cobol`](../../crates/graphit-ast/src/antlr_engine/preprocessing.rs)
and bind its real `OriginPlan` to `PreparedInput`. Every match/capture uses the
same original operation and origin projections, including continued compound
UTF-8 text. The consumer opens no copybooks and executes no REPLACING clause;
filesystem expansion and preprocessing stage 5 remain separate work.

`tests/antlr_cobol_copy.rs` exercises stock and custom COPY, raw quoted/library
names, SUPPRESS/REPLACING, native parent scopes, physical/original formats,
opaque strings/comments/SQL, invalid selectors/source configuration and original
cancellation/deadline. Qualification for `tsk-0b48` passed nine real main/COPY Linux tests, the actual
JSON query-source preflight, affected config/capture/Original regressions and
strict scoped Clippy. The JSON factory rejects secondary sources before any
query compilation; the nine unchanged cases were preserved after its isolated
preflight change. Earlier real build, fixture, lint and lock-resolution failures
remain recorded. Generated C++ warnings are preserved without rewriting output.
Full S7, graph publication, COPY filesystem expansion/REPLACE execution and
Windows/macOS remain outside this qualification.

### External PROCEDURE XML type observations

The [external XML representation](rust_ast_antlr_queries.md#db2-external-procedure-xml-representation)
adds a native `procedure_external_xml_type` child only for XML AS CLOB.
The Parameter name and routine-default children remain direct under
`proc_ext_param`, so the completed stock selector and default consumers retain
their paths. Other external types and SQL PROCEDURE retain `data_type`.
A query can select the complete XML type and native size/unit children without
deriving a range from text or classifying a nominal catalog name.

The new Rust fixture consumes the actual native tree and Source projections.
It does not introduce a Type entity, UID, semantic binding, Graph/NN publication
or Db2 server validation. Root31w passed the four new SDK cases; Root31u passed
the three affected PROCEDURE engine cases and the shipped nine-family header
metadata case. Root31z scoped Clippy exited successfully after disk recovery.
Existing model qualifications retain their separate scopes; Windows/macOS and
Db2 server execution are unrun.

### External scalar parameter type observations

The [external scalar type paths](rust_ast_antlr_queries.md#db2-external-scalar-parameter-type-paths)
observe three direct branches: existing `data_type`, `method_reference_type`
and `procedure_external_xml_type`. REF retains `method_named_type/type_name`;
XML representation retains `integer_value` and optional `kmg`. Names,
defaults and locators keep their direct caller paths.

The stock consumer observes only declared names in the actual Function context.
Anonymous REF/XML slots create no Parameter, and their type spellings do not
declare Type entities or establish bindings. Query metadata/query indexes remain
borrowed from the actual effective profile. Mixed XML/REF/builtin captures and
names retain exact Original ranges. Cancellation, deadline and invalid-query
causes remain original.

### External scalar inferred CAST observations

The [external CAST target paths](rust_ast_antlr_queries.md#db2-external-scalar-cast-attribute-inference)
retain `data_type_3` and observe `inferred_builtin_type` only in its new
alternative. Explicit target observations keep `data_type_3/data_type`;
source/ordinary return and parameter domains remain separate. The observed
empty attributes are native syntax, not calculated values or catalog types.

The focal uses the real stock profile and Direct Original projections. Query metadata remains borrowed, and the actual Function context and Parameter name are not inferred from target identifiers. Cancellation, deadline and invalid-query causes remain original. Windows/macOS and Db2 server execution are unrun.

### SQL FUNCTION parameter and scalar type observations

The [SQL FUNCTION native type paths](rust_ast_antlr_queries.md#db2-sql-function-parameter-and-scalar-types)
observe `data_type_1` directly under `param_decl_2` and the scalar-return caller.
The Parameter name and default children retain their existing direct paths.
Stock queries still borrow their effective-profile metadata; REF type names
and other nominal identifiers do not create Parameter or Type entities.
The focal uses the actual stock semantic consumer and Direct Original
projections, alongside native type queries, recovery and operation causes.

Typed ROW/TABLE columns remain in their local helpers; variable-helper and
stock-nine-header qualifications are reused within their original scopes.
The four new SDK cases and stock metadata case passed in Root31zn; the two
affected engine controls passed in Root31zh. Root31zs passed scoped AST
Clippy; the seven SDK cases were reused after the coalesced Hub quality
failure. No catalog binding, Graph/NN publication, server assignment or
compiled-body inference is added.
Windows/macOS and Db2 server execution are unrun.

## Db2 Type declaration frames

The DT delta in `tsk-6eae` observes declared Type names from the five concrete
CREATE TYPE branches. Name captures, node ordinals, effective query metadata
and exact Original projections remain native. Each declaration has one Type
context with a direct captured name. The generic wrapper remains in syntax and
is not an additional configured context. A Methodspec Function remains nearer
than its `create_type_structured_statement` Type ancestor. ALTER TYPE references
do not invent a declared Type or Type context.

The [Type selectors](rust_ast_antlr_queries.md#db2-type-declaration-paths-and-contexts) exclude referenced row, source and supertype names. These are raw observations; model identity, canonical QName, semantic owner links and Graph publication require their separate consumer contract. Linux actual; Windows/macOS and SQL server execution remain unrun.

## DB2 model and NativeNN consumer

[ParsedAntlrDocument](../../crates/graphit-ast/src/antlr_engine/model.rs) now collects actual db2/antlr-db2 and plsql/antlr-plsql profiles. The private [DB2 registry](../../crates/graphit-ast/src/antlr_engine/model/db2.rs) maps native name subjects to genuine declaration extents, typed children and owner ancestry.

Entity and namespace display names are the local final decoded DB2 component.
The identity anchor remains the complete native QName. qualified_name() retains
component node ordinals, exact Original ranges, decoded values and quote styles.
Mature sqlparser Tokenizer/Parser errors remain typed; no split, SQL fold, trim or
SDK location is substituted for an Original projection. Only current native name
syntax is admitted. A written external qualifier or CREATE METHOD FOR type
reference creates no Schema/Type owner.

Contribution type_roles retains Declared, Return, CastSource, CastTarget and
ReturnColumn observations in native order. declared_type remains the declaration's
own written slot. Grouped Variables have distinct subjects and shared declaration
extents; diagnostics associate with the subject, while genuine statement recovery
blocks all affected projections. Index columns expose IndexKeySpecification
roles with Index ancestry. An unnamed AUTHORIZATION Schema creates no subject or
namespace. Effective labels, keys, captures and policies remain consumer inputs.

The Db2 model binds written types to the current direct native children:

| Header slot | Native type child | Model observation |
|---|---|---|
| SQL FUNCTION parameter | data_type_1 under param_decl_2 | declared_type and Declared |
| External scalar parameter | data_type, method_reference_type or procedure_external_xml_type | declared_type and Declared |
| External PROCEDURE parameter | data_type or procedure_external_xml_type | declared_type and Declared |
| SQL FUNCTION scalar result | data_type_1 under the function statement | Return |
| External scalar ordinary result | external_scalar_ordinary_return_type under the function statement | Return |

Every eligible direct type child is collected before enforcing one written
slot. Multiple candidates preserve the native-contract error; descendant
search or first-success selection cannot hide an inconsistent declaration.
The existing required_origins and type_observation paths retain native node
identity, exact Original ranges and explicit NonExact/Missing observations.
An anonymous slot creates no Parameter, and a REF/XML type wrapper creates no
Type entity. The parameter/SQL correction in tsk-044f retains CAST source/target,
ROW/TABLE columns and other families, without grammar, parser, profile or public
DTO changes. The separate tsk-2526 ordinary external grammar and Return binding
are described below.

[encode_native_nn_antlr](../../crates/graphit-ast/src/native_nn_antlr.rs) uses the
real projection pair to select Oracle or DB2 name matching for both entity and
owner names. DB2 shares the model's mature QName decoder. The existing same-source
allocation, path and Original checks remain in force. The
[affected model/NN fixtures](../../crates/graphit-ast/tests/antlr_db2_model.rs)
exercise the actual collect/assemble/projection/encoder path. Graph resolution,
server datatype classes, inference and parent/session completion are outside
these model observations.

### Oracle callable consumer scope

The [Oracle semantic model](rust_ast_antlr_model.md) consumes the eight native Function/Procedure/Package declaration kinds and their actual owner ancestry. Qualified names retain separate native components with the final component as the Oracle identity/NN name anchor; the whole selected wrapper remains raw. Package END names are typed DeclarationReference observations rather than extra entities.

### Oracle typed declaration consumer

The [Oracle model](rust_ast_antlr_model.md) consumes stock Parameter and local Type observations using native owners and names. It retains native modes/defaults and five Type shapes from the four canonical type body families, including element/index/bound/definition statuses. Record and RefCursor are distinct from collections. This changes no profile query, grammar or input normalization.

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

The Graph adapter adds optional `oracle_application` to the existing `observation_json` and FILE outcomes. At the C7 observation stage, it preserves ObservedOnly, no source index and no resolved edges. The local callable consumer below adds resolution only where native source and target proof both succeed. No NN/DTO/protocol/profile/G4 change is involved. Bindings, complete C7–C9, constructors, collection/LHS/bare methods, chains/@link resolution and publication remain necessary outcomes.

Boundary metadata contains only native status, ordinals and explicit Original ranges; it omits literal_source so callable/body text is not duplicated per outcome. Borrowed native Origins remain unchanged.

## Native collection statements and assignment context

The C8 extension adds three explicit raw selectors for native `collection_method_call` terminals DELETE, EXTEND and TRIM, using the current generic SDK compiler. The three PL/SQL stock copies keep all preceding queries in order; their query count becomes 172. The previous seven-family/84-descriptor dedicated-call fixture remains its own qualified scope.

`AntlrOracleApplicationKind::CollectionMethodStatement` retains the actual statement, first direct expression as receiver, subsequent direct expressions as indices and direct native delimiters. It does not fabricate `function_argument` groups. Native syntax does not prove the receiver's type or a Function endpoint. `assignment` records the nearest assignment statement, its direct Target/Value branch and actual intervening expressions. An application inside a target index retains that path; the location is not classified as a semantic write.

The collector independently projects these nodes while PreparedInput is alive. Graph adds optional `collection_statement`/`assignment` within the existing observation JSON, keeps ObservedOnly/no source index/no edges, and omits literal_source for structural statement/branch/path nodes. Receiver/index expressions retain their own native facts. No Source authority, NN schema, SDK/G4 or DTO protocol is changed.

## Local binding preserves raw evidence

B1 consumes existing Oracle occurrences and declarations; it adds no stock
selector and does not replace raw multiplicity, query provenance or diagnostics.
Native declarations survive absent/rejected entity queries. Several emitted
data_key tuples for one declaration remain one lexical candidate.

The mature identifier tokenizer preserves decoded components and quote style.
Quoted names compare exactly; unquoted ASCII uses Oracle uppercase semantics.
Unquoted identical decoded forms are provably equal. Other comparisons involving
unquoted non-ASCII remain DatabaseCharacterSet indeterminacy; they never use a
generic Unicode casefold or fall through an uncertain local declaration.
This follows [Oracle identifier rules](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/Database-Object-Names-and-Qualifiers.html)
and [lexical visibility](https://docs.oracle.com/en/database/oracle/oracle-database/26/lnpls/scope-and-visibility-identifiers.html).

Typed lookup states describe local evidence. SQL name precedence, implicit loop scopes, conditional/default contexts, qualified/link/introducer receivers and unsupported type references remain explicit boundaries. A syntactic CALLS occurrence does not acquire a Function endpoint by excluding collections.

## Native collection member observations

B2 appends five stock patterns for native COUNT/FIRST/LAST/LIMIT/NEXT terminals
under a general_element with an actual prefix. The current catalogue is 177
queries; all 172 C8 entries remain in order. EXISTS reuses its existing pattern.
A terminal match retains its raw matched/name/provenance and promotes the
application head through the exact native id-expression chain.

A grouped NEXT or COUNT can match both the general and terminal patterns.
Both occurrences survive with their real query indices and data_keys.
They share lexical declaration evidence without becoming ambiguous candidates.

Syntactic capture does not validate an Oracle invocation. COUNT() and missing
NEXT/EXISTS indices remain observed and unresolved with typed causes. Quoted
methods, complex/indexed/link receivers and unsupported contexts retain evidence.
No new grammar or parser is introduced. PRIOR has no demonstrated active member
requirement in the inspected migration baseline; it is outside B2 and creates
no Grammar work or product gate. Public Oracle documentation alone does not
establish an active Graphit capability.

## Local Oracle CALLS consumer

The [local model contract](rust_ast_antlr_model.md#local-oracle-callable-references)
adds borrowed caller/body and lexical target outcomes without changing raw C7
occurrences, B1 lookups or B2 classifications. Only a Bound native callable source
and Resolved permitted local Function/Procedure entity produce CALLS between real
Graph node keys. Observed, ambiguous, unsupported and indeterminate cases retain
their actual causes; Package/global/qualified/database-link/compiler overload
resolution and full C7–C9 publication are outside this leaf. An uncertain charset
comparison cannot be discarded as a proven noncallable.

## Db2 native ordinary external return observations

`tsk-2526` binds the complete direct `external_scalar_ordinary_return_type`
to `AntlrTypeRole::Return` on external scalar Function contributions.
Its builtin/opaque, REF or XML representation branch does not create a
Parameter or Type entity. Ordinary AS LOCATOR lies outside the observed type.
CAST continues to use direct `data_type_3` as CastTarget and direct
`data_type_2` as CastSource.

The model counts all direct ordinary/source/target slots before producing
roles. More than one slot in a position, ordinary mixed with CAST, or source
without target is a structural NativeContract error. Zero slots and a partial
target remain available to native recovery. Slots are never discarded because
their span/origin is unavailable or their error/missing bit is set: the existing
`type_observation` and Original projection preserve Missing/NonExact evidence.

The return-role fixture includes DATE, REF, XML AS CLOB, CLOB with a locator and an opaque qualified name, requiring the actual wrapper/parent, native order and borrowed Original pointer. CAST/ReturnColumn cases remain independent. See the [native query paths](rust_ast_antlr_queries.md#db2-external-scalar-ordinary-return-type-paths).
