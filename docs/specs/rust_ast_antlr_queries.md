---
title: Rust AST ANTLR queries
description: Native XPath selection over the live ANTLR parse tree.
doc_type: technical
references:
  - target: {type: task, id: tsk-2526}
    relation: implements
  - target: {type: task, id: tsk-6eae}
    relation: implements
  - target: {type: task, id: tsk-0b48}
    relation: implements
  - target: {type: task, id: tsk-f627}
    relation: specifies
  - target: {type: task, id: tsk-2558}
    relation: specifies
  - target: {type: task, id: tsk-7f03}
    relation: supports
  - target: {type: task, id: tsk-6012}
    relation: supports
  - target: {type: task, id: tsk-3f37}
    relation: relates_to
  - target: {type: task, id: tsk-cf2d}
    relation: relates_to
  - target: {type: task, id: tsk-1b14}
    relation: supports
  - target: {type: task, id: tsk-ddbd}
    relation: supports
  - target: {type: task, id: tsk-e3df}
    relation: supports
  - target: {type: task, id: tsk-106e}
    relation: supports
  - target: {type: task, id: tsk-c72f}
    relation: supports
  - target: {type: task, id: tsk-a72c}
    relation: supports
  - target: {type: task, id: tsk-e94e}
    relation: supports
  - target: {type: task, id: tsk-ab50}
    relation: supports
  - target: {type: task, id: tsk-6375}
    relation: supports
  - target: {type: task, id: tsk-9b9a}
    relation: supports
  - target: {type: task, id: tsk-c23d}
    relation: supports
  - target: {type: task, id: tsk-231a}
    relation: supports
  - target: {type: task, id: tsk-4537}
    relation: supports
  - target: {type: task, id: tsk-eb87}
    relation: supports
  - target: {type: task, id: tsk-929f}
    relation: supports
  - target: {type: task, id: tsk-80fb}
    relation: supports
  - target: {type: task, id: tsk-8187}
    relation: supports
  - target: {type: task, id: tsk-1c45}
    relation: supports
  - target: {type: task, id: tsk-086a}
    relation: supports
  - target: {type: task, id: tsk-4eda}
    relation: supports
  - target: {type: task, id: tsk-4c23}
    relation: supports
  - target: {type: task, id: tsk-8121}
    relation: implements
  - target: {type: task, id: tsk-08df}
    relation: implements
  - target: {type: task, id: tsk-b0cb}
    relation: derived_from
  - target: {type: task, id: tsk-a4cd}
    relation: relates_to
  - target: {type: task, id: tsk-b197}
    relation: reuses
  - target: {type: task, id: tsk-0395}
    relation: reuses
---

# Rust AST ANTLR queries

For AST adapter maintainers in graphit-code. Native queries are implemented and
qualified on Linux. This API reuses the five official
ANTLR C++ 4.13.2 recognizers generated from the project's maintained G4 files.
The [semantic extraction contract](rust_ast_semantic_extraction.md) tracks the
remaining profile and publication integrations.

## API and returned nodes

The [ANTLR engine](../../crates/graphit-ast/src/antlr_engine.rs) exports:

```rust
pub fn parse_queries<'s>(
    dialect: Dialect,
    source: &'s str,
    paths: &[String],
    context: &SnapshotContext,
) -> Result<ParsedQueries<'s>, AntlrEngineError>;

pub struct ParsedQueries<'s> {
    pub syntax: ParsedSyntax<'s>,
    pub matches: Vec<QueryMatch>,
}

pub struct QueryMatch {
    pub query_index: usize,
    pub node_index: usize,
}
```

Query indices address the supplied paths; node indices address real nodes in
the same result's `syntax.nodes`. Results are ordered by query, then native node
preorder, and unique per query/node pair. Distinct nodes with equal spelling
remain distinct. Duplicate query strings retain their separate indices.
These ordinals are neither symbol identities nor source authority.

An empty paths slice still parses and returns an empty matches vector. Existing
`parse`, `parse_strict` and `ParsedSyntax` retain their contracts. For example,
TSQL `SELECT 1;` with `//SELECT` selects the actual SELECT terminal;
`/tsql_file/SELECT` succeeds empty because SELECT is not a direct root child.

## SDK execution and errors

The adapter uses official `XPath::split` and `XPathElement::evaluate` while the
parser/tree are alive. A local synthetic root replaces the SDK's mutable static
root in `XPath::evaluate`; that method and `findAll` are not called. Matched
pointers become existing native traversal ordinals before owners are destroyed.
No native pointer escapes; wildcard selection excludes the synthetic root.
See the official [XPath interface](https://github.com/antlr/antlr4/blob/4.13.2/runtime/Cpp/runtime/src/tree/xpath/XPath.h)
and [implementation](https://github.com/antlr/antlr4/blob/4.13.2/runtime/Cpp/runtime/src/tree/xpath/XPath.cpp).

The SDK interprets rule/token names, `/`, `//`, wildcard, token literals and
inversion. Empty paths and paths containing `**` are explicitly rejected:
the profile nearest-capture operator must not become adjacent SDK wildcards.
The actual SDK XPath lexer runs with a strict diagnostic observer before
splitting, because its default splitter listener discards lexical errors.
Invalid characters preserve the SDK diagnostic as a native exception;
syntax and vocabulary errors retain the SDK exception. Paths are not
trimmed, repaired or case-folded. No source parser or query DSL parser is added.

Query failures return `AntlrEngineError::QueryFailure { query_index, cause,
control }`. The typed index comes from the operation's native begin-query
callback; it is not extracted from an error message. `cause` retains the actual
cxx exception and is exposed by `Error::source`. Recognition failures before
query execution retain `NativeFailure`; original cancellation/deadline failures
retain `Operation`. A failure returns no partial result.

## Coordinates, operation and concurrent calls

Matches reference actual half-open UTF-8 spans, parents, diagnostics and
recovery observations. Inserted recovery tokens retain `span: None`,
`missing/error` and `Unindexed(InsertedRecoveryToken)`. EOF retains its actual
zero-width EOF span. No bound is fabricated or widened into a hull.

Use `result.syntax.project(actual_prepared_input)` for original coordinates.
The actual prepared buffer binding and Exact, Compound, Synthetic, Ambiguous
and Unmappable outcomes are preserved. Equal text in another allocation does
not substitute for that binding. Source bytes remain unchanged.

Original operation checks surround recognition, splitting and element
evaluation, and precede delivery. SDK calls are indivisible between cooperative
polls; this API makes no hard-deadline guarantee. Concurrent calls retain their
own native trees, roots and query work, with no shared query state or global
query mutex/cache.

## Reusing a PostgreSQL host for PL/pgSQL bodies

The public `parse_postgres_with_plpgsql` facade parses the PostgreSQL host once,
then calls the crate-private helper in
[embedded.rs](../../crates/graphit-ast/src/antlr_engine/embedded.rs):

```rust
pub(crate) fn parse_plpgsql_bodies<'s>(
    host: &ParsedSyntax<'s>,
    language: &GrammarLanguage,
    control: ParseControl<'_>,
    context: &SnapshotContext,
) -> Result<Vec<PlPgSqlBody<'s>>, EmbeddedError>;
```

An AST consumer that already holds the host uses the helper without a second
host parse or copy. The caller must supply a PostgreSQL host parsed under that
same original operation and the actual PL/pgSQL grammar. `ParsedSyntax` does
not retain dialect/context identity, so these are caller preconditions rather
than fabricated validation. Body records borrow the host's source lifetime,
retain their own SDK tree/grammar owner, and retain `host_parent` as the actual
ANTLR option-list node ordinal. Missing or empty body observations stay in the
host; they do not create a substitute body tree. Host nodes, tokens, diagnostic
and recovery observations remain intact.

`PlPgSqlBody::nodes_with_context(control, context)` and
`matches_with_context(query, control, context)` preserve native Tree-sitter IDs
from the same retained body tree. The body node parent indexes remain local;
a root's `None` connects to `host_parent`. Ranges and byte points are projected
by the body offset and one UTF-8 pass for host bases, preserving CRLF and Unicode.
These are coordinates in the **actual host buffer**. When the host is prepared
text, the consumer must still use its `PreparedInput` origin plan for original
coordinates; the splice does not imply an Exact original projection.

Context checks precede optional controls and run throughout preparation, SDK
work and projection, including empty query results. Host preparation errors
retain `EmbeddedError::Host(AntlrEngineError::Operation)`; body SDK/projection
errors retain `EmbeddedError::TreeSitter(EngineError::Operation)`, with the
original I/O cause. Optional cancellation/deadline retain their Tree-sitter
errors. No partial body/projection result is delivered on failure.

The tsk-8121 qualification passed on Linux: the affected ANTLR engine suite
19/19 (1.21 s), Tree-sitter suite 11/11 (0.23 s), private host-reuse fixture 1/1
(0.42 s) and strict Clippy. Real dollar-body, Unicode/CRLF, same-tree ID,
negative routing, recovery and causal-control fixtures passed. Windows/macOS
remain unrun.
The private helper fixture uses a real, already parsed host with an empty body
and a recovered diagnostic, and checks retained observations and source binding.
The [Tree-sitter engine](rust_ast_tree_sitter.md) lists the affected commands.

## Qualification and integration boundary

Nine new functional tests in [antlr_queries.rs](../../crates/graphit-ast/tests/antlr_queries.rs)
cover roots and concrete profile name paths in all five grammars, terminals,
ordering/deduplication, inversion, errors with typed indices, recovery/EOF,
Exact/Compound original projections, real operation cancellation and four
synchronized concurrent callers. The existing engine suite is the affected
recognition regression. The DB2 name fixture uses
`CREATE TABLE pedido LIKE modelo COMPRESS NO;`, the current grammar's LIKE
alternative with its required table option.

The earlier column-list fixtures were rejected by that grammar; the query API
does not repair the input or change the grammar.

For the historical tsk-08df qualification on Linux, the affected suites passed: engine 18/18 (1.38 s) and queries
9/9 (0.23 s), including the corrected DB2 fixture. Affected strict Clippy also
passed against the current Main dependencies. The earlier dependency compilation
failure remains recorded in Task history.
Windows/macOS execution remains unrun. The affected commands are:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_queries --test antlr_engine --jobs 1
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test antlr_queries --jobs 1 -- -D warnings
rustfmt --edition 2024 --config skip_children=true --check crates/graphit-ast/src/antlr_engine.rs crates/graphit-ast/src/antlr_engine/bridge.rs crates/graphit-ast/src/antlr_engine/xpath.rs crates/graphit-ast/tests/antlr_queries.rs
```

Profile guards and nearest `**` captures are delivered separately by the
[profile query engine](rust_ast_antlr_profile_queries.md). The host/body producer
provides native syntax and projection; profile composition, owner/model/merge/
relation interpretation, complete embedded semantic extraction, SCIP handoff
and Data publication remain required integration work in tsk-a4cd. These native
queries do not qualify every active profile or complete semantic extraction.

## DB2 Method parameter paths

Source leaf `tsk-4eda` delivered the type helpers and isolated Method
specification parameters. The later `tsk-6eae` MD delta gives each signature
slot its own `method_signature_parameter_decl` parent. Its name remains an
exact selected `parameter_name`. The historical Linux focal verified the
previous paths; eight affected DB2
regressions and strict library/engine-fixture Clippy also passed after the Hub
owner corrected an unrelated compilation failure. The qualified selectors are:

```text
//method_signature/method_param_list/method_signature_parameter_decl/parameter_name
//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name
```

With an empty `CapturePath` operation list, each named slot must select its exact raw name
once; unnamed slots select none. A single nominal identifier such as `P_F`
can therefore be an unnamed type, rather than a missing-type error. Schema/type
names below REF are not Parameter names. Cursor constructors retain `param_decl_list_paren` and must not select
through either Method path. The nine-selector raw metadata consumer is delivered
by `tsk-f627`; the MD delta updates only its signature path in all three stock
copies. The later [Method CAST REF syntax scope](#method-cast-ref-capture-boundary)
is delivered separately by `tsk-4c23`; catalog enumeration is not a parser gate.
These raw paths and declaration delimiters do not qualify semantic entities,
resolved owners, UID or publication.

### Individual Method signature parameter nodes

The MD delta in `tsk-6eae` publishes native individual signature slots using
ANTLR Tool 4.13.2. The list getter returns
`Method_signature_parameter_declContext` children in native order; each exposes
`parameter_name()`, `method_parameter_type()` and `as_locator()`. Name can be absent,
but the declaration and type remain real nodes. No compatibility getters or
fabricated unnamed Parameter are retained. Decl spans exclude commas and list
parentheses, while retaining internal hidden comments. Methodspec uses its
existing individual declaration context.

## Planned DB2 PROCEDURE parameter paths

`tsk-eb87` implements list/direction correction with the existing native
`proc_ext_param` and `proc_parameter_list_item` contexts; its two selectors are
qualified on Linux in `db2_procedure_parameter_lists_native`:

```text
//proc_ext_param/parameter_name
//proc_parameter_list_item/parameter_name
```

Each named slot selects its exact raw name once with
an empty `CapturePath` operation list; external unnamed slots select none. Empty and
omitted lists produce no Parameter captures. The existing
`create_procedure_statement` wrapper remains unique per declaration. CALL
arguments, local declarations, cursors and Methods do not select through
these paths. Existing type contexts remain direct; the bounded external XML
representation is described below. Routine defaults were composed in
`tsk-4537`, reusing `tsk-20e3`. The nine stock Parameter selectors have their
own completed qualification in `tsk-f627`; list/direction tests alone do not
qualify semantic entities or publication.

The focal passed (one test, 0.15 seconds), ten affected DB2 regressions and strict
library/engine-fixture Clippy passed. The first compilation failed in unrelated
Auth code before tests; its owner fixed it before retry. Windows/macOS are unrun.

## Db2 external scalar parameter type paths

`tsk-1c45` composes REF and XML representation in `ext_scalar_param_decl`
using the existing helpers. Select the native branches and their children with:

    //ext_scalar_param_decl/data_type
    //ext_scalar_param_decl/method_reference_type
    //ext_scalar_param_decl/method_reference_type/method_named_type/type_name
    //ext_scalar_param_decl/procedure_external_xml_type
    //ext_scalar_param_decl/procedure_external_xml_type/integer_value
    //ext_scalar_param_decl/procedure_external_xml_type/kmg

an empty `CapturePath` operation list selects exact native ranges. REF keeps its
one- or two-component referent. The XML wrapper spans XML through its closing
parenthesis, including hidden comments; its integer and optional multiplier are
separate native children. An omitted K/M/G produces no multiplier node.

    CREATE FUNCTION APP.F_TYPES(IN P_REF REF(APP.TYPE_F),
        IN P_XML XML AS CLOB(2M), XML AS CLOB(2048))
    RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL NOT FENCED
    EXTERNAL NAME 'pkg.fn' NO SQL;

Existing `data_type`, parameter name, default and `as_locator` selectors
retain direct paths. The bare XML recognition control stays in `data_type`;
it does not prove server representation legality. Anonymous REF/XML slots have
no `parameter_name`, and referents/representations are not Type declarations.
A direction still requires a name. Malformed REF/XML constructors preserve native
recovery, token/diagnostic spans and strict Syntax errors.

The one SDK bin retains four REF functions unchanged and adds two XML/mixed-domain functions. It uses the shipped effective profile, borrowed query metadata, real Function contexts and Direct Original projections, preserving original operation causes. Windows/macOS and Db2 server/catalog/host execution are unrun.

## Db2 external scalar CAST attribute inference

`tsk-8187` retains the `data_type_3` wrapper and adds a native child for
inferred attributes. Query the complete target or its inferred branch with:

    //create_function_external_scalar_statement/data_type_3
    //create_function_external_scalar_statement/data_type_3/inferred_builtin_type

an empty `CapturePath` operation list selects the exact native type. Explicit attributes
continue through `data_type_3/data_type`, with no inferred child. For example:

    CREATE FUNCTION APP.F_INFERRED() RETURNS VARCHAR() CAST FROM VARCHAR(40)
    LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.fn';

The inferred target and explicit source keep separate ranges. The source is
still the caller's direct `data_type_2` child. The later `tsk-2526` ordinary
return wrapper is described below; parameters, names/defaults and locator paths
remain unchanged. The local manual labels
data-type3/data-type4 do not define a global G4 family. IBM LUW11.5 SQL
Reference page1180 permits inferred target attributes and excludes FLOAT().
Empty attributes in a source, ordinary return or parameter are not admitted
by this change. Native recovery and strict Syntax errors remain observable.

The Rust SDK fixture checks raw UTF8/CRLF/hidden comments, native parents, Exact Original projections, real stock query metadata and Function/Parameter contexts. It neither derives attribute values nor creates a Type entity or server/catalog binding. Windows/macOS and Db2 server execution are unrun.

## Db2 SQL FUNCTION parameter and scalar types

`tsk-80fb` binds the two general type positions to the mature `data_type_1`
helper. Select each complete native type with:

```text
//param_decl_2/data_type_1
//create_function_sql_scalar_table_or_row_statement/data_type_1
```

an empty `CapturePath` operation list retains the exact type range. For REF, the direct
`type_name` capture retains its existing one- or two-part constructor name;
builtins do not synthesize that child. Other nominal alternatives retain the
existing qualified-name syntax and remain unresolved catalog names. For example:

```sql
CREATE FUNCTION APP.F_REF(P_REF REF(APP.TYPE_F))
RETURNS REF(APP.TYPE_F) LANGUAGE SQL RETURN P_REF;
```

`parameter_name` and `routine_default_clause` stay direct under `param_decl_2`.
The stock Parameter and function-context paths remain unchanged. There is no
compatibility `data_type` or `data_type_2` child at these two type positions.
Typed ROW/TABLE returns continue through `sql_function_return_column` and its
local type helpers; ARRAY index and other `data_type_2` callers stay bounded.
The manual names data-type1/data-type2 do not identify the shared G4 rule
`data_type_2`. See IBM LUW11.5 SQL Reference physical pages1241–1249, where
the scalar RETURNS prose uses the same considerations as the parameters.

The Rust SDK fixture `tests/db2_sql_function_types.rs` covers REF, builtin and
national aliases, anchors, unresolved nominals, native hidden comments, exact
Original projections, malformed constructors and strict diagnostics. It also
uses the actual stock effective profile and direct PreparedInput to check
borrowed query metadata, Parameter captures and the real Function context.
A nominal spelling does not classify ARRAY/CURSOR/ROW or establish server
assignment or body mode. BEGIN ATOMIC fixtures exercise the existing recognizer;
no new COMPILED keyword or server execution claim is introduced.

## Db2 external PROCEDURE XML representation

`tsk-929f` adds `procedure_external_xml_type` as a direct alternative to
`data_type` inside `proc_ext_param`. Its native syntax is:

```text
procedure_external_xml_type : XML AS CLOB '(' integer_value kmg? ')' ;
```

The complete XML type can be selected with:

```text
//proc_ext_param/procedure_external_xml_type
```

Use an empty `CapturePath` operation list for the type, and direct capture paths
`integer_value` and `kmg` for its native children. An absent size unit is a
missing optional capture; the caller does not synthesize it. The existing
`//proc_ext_param/parameter_name` and routine-default paths remain direct.
Unnamed external slots still have no Parameter name. Other types retain the
existing direct `data_type` child, and SQL PROCEDURE is unchanged.

For example, the external declaration below retains XML, AS, CLOB, the size
and its unit as native syntax, including original case, comments and ranges:

```sql
CREATE PROCEDURE SCHEMA_F.PROC_XML(IN P_XML XML AS CLOB(2M))
LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';
```

The [IBM11.5 external XML routine contract](https://www.ibm.com/docs/en/db2/11.5.x?topic=routines-xml-data-type-support)
and SQL Reference physical page1329 require this CLOB representation. The
grammar reuses `integer_value` and `kmg`; no XML decoder, text normalizer or
second parser is introduced. Invalid representation keywords, empty or missing
sizes and unbalanced delimiters retain native diagnostics; strict parsing fails.
The new alternative does not admit XML AS CLOB in SQL PROCEDURE.

The existing recognizer also accepts bare XML through `data_type`; retaining
that syntax does not make it a valid external representation at the Db2 server.
Language and catalog rules are separate from syntax: the
[Java routine mapping](https://www.ibm.com/docs/en/db2/11.5.x?topic=jsri-data-types-that-map-database-data-types-in-java-applications)
confirms BOOLEAN for external Java stored procedures, although the external
diagram omits it. DECFLOAT language restrictions do not become a new lexical
rejection. [National types](https://www.ibm.com/docs/en/db2/11.5.x?topic=list-national-character-strings)
are aliases governed by Unicode/nchar_mapping; their original spelling is kept.
Nominal names remain unresolved catalog names. No server execution, assignment,
precision validation or language inference is performed by this change.

The Rust fixture `tests/db2_procedure_types.rs` uses the official C++ recognizer
through `parse_profile_queries`, not a hand-built tree. It covers native type,
name/default bindings, UTF8/CRLF/hidden comments, direct contexts, malformed
annotations, Source allocation/projections and original operation/query errors.

## Planned DB2 PROCEDURE default paths

`tsk-4537` implements the two PROC parameter default consumers. Its native fixture
qualifies these paths on Linux:

```text
//proc_ext_param/routine_default_clause/routine_default_value
//proc_parameter_list_item/routine_default_clause/routine_default_value
```

an empty `CapturePath` operation list selects the exact raw value once per header
default, preserving value → clause → parameter parents and ordered names from
the two existing name selectors. External unnamed slots retain value captures
and no Parameter-name captures. The helper already exists in `tsk-20e3`; these
PROC consumer captures passed in the focal (one test, 0.67 seconds), with all
eleven affected DB2 regressions and strict library/engine-fixture Clippy. The
initial build failed before tests because the shared filesystem had no space;
the coordinator recovered inactive duplicate executables before retry. The
failure log is preserved. Windows/macOS remain unrun. Shared column/local defaults, direction
and type validation, complete constants/X, stock metadata and publication are
separate requirements. This composition does not qualify OUT/INOUT or ARRAY/ROW/CURSOR
defaults by accepting the value syntax alone.

## DB2 ALTER TYPE Method wrapper paths

`tsk-c23d` implements the single-METHOD ADD wrapper. Its native fixture passed
on Linux (1/1, 0.10 seconds), including these scoped selectors:

```text
//alter_type_opts/method_specification/method_name
//alter_type_opts/method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name
//alter_type_opts/method_specification
```

an empty `CapturePath` operation list selects the exact raw method name, named parameters
and whole Methodspec. Unnamed and empty slots produce no Parameter-name capture.
The Methodspec remains a direct child of `alter_type_opts`; its raw span begins
with METHOD or OVERRIDING METHOD. Other ALTER options, CREATE TYPE, signatures,
PROCEDURE, named CALL arguments and cursors do not select through these paths.
Existing Method parameter selectors remain unchanged. Twelve affected DB2
regressions and strict Clippy also passed. These paths do not qualify stock
metadata or semantic publication; Windows/macOS remain unrun.

The return producer `tsk-231a` adds a dedicated mandatory typed-return Context
directly under Methodspec, replacing the old typed alternative inside
`method_specification_seq`. The public cxx ABI and existing parameter selectors
remain intact; no alias preserves the old return-parent path.

## DB2 Methodspec typed return path

`tsk-231a` implements this native selector, qualified on Linux:

```text
//method_specification/method_specification_return_clause
```

an empty `CapturePath` operation list selects the exact raw RETURNS clause once for each
ordinary or OVERRIDING Methodspec. The node is a direct child of
`method_specification`; return `data_type`/`as_locator` descend from the clause.
Option sequences contain no typed return. RETURNS NULL ON NULL INPUT is an
external characteristic and cannot substitute for this node. Options may precede
or follow the typed clause. Two Methodspecs must each own one return and cannot
borrow it from another declaration.

The old `//method_specification/method_specification_seq/data_type` path has no
return capture; ordinals and parents change naturally. No old-path alias or
normalization is provided. Existing Method parameter paths remain intact.
CREATE METHOD signatures retain their optional return and do not select here;
PROCEDURE/scalar/CALL/cursor/other ALTER paths remain outside this selector.
The final focal passed 1/1 (0.19 seconds), thirteen affected DB2 regressions
each passed 1/1, and strict Clippy passed in 70.07 seconds. Windows/macOS remain
unrun. Full type/CAST and option contracts, stock metadata
and semantic publication remain separate necessary products.

## DB2 FUNCTION default paths

`tsk-9b9a` implements these qualified native value selectors:

```text
//ext_table_param_decl/routine_default_clause/routine_default_value
//param_decl_3/routine_default_clause/routine_default_value
//sourced_function_param_decl/routine_default_clause/routine_default_value
//param_decl_2/routine_default_clause/routine_default_value
```

an empty `CapturePath` operation list selects the exact raw value in external table,
OLE DB, sourced/TEMPLATE and SQL function headers respectively. Each value is a
child of `routine_default_clause`, which is a child of its actual parameter
rule. `param_decl_3` serves OLE DB; `tsk-086a` separates sourced/TEMPLATE into
`sourced_function_param_decl` for its independent type attributes.
Named slots retain their existing `//<parameter-rule>/parameter_name` paths;
external-table unnamed slots produce no name capture. The defaults leaf left
`param_decl_3` names required; `tsk-6375` qualified omission, preserved in both
current rules. Qualified nominal syntax uses the completed name producers;
catalog classification remains unresolved. See the
[shared name/type/default selectors](#db2-shared-function-parameter-names).

The old column `default_clause` path disappears from these three consumers;
there is no alias or tree normalization. Existing scalar and PROC value paths,
local DECLARE, column defaults, cursor, Method and CALL contexts remain isolated.
Five complete templates verify family wrappers, raw ordered values/names and
causal invalid values. The final focal passed 1/1 in 1.62 seconds, all fourteen
affected DB2 regressions passed, and strict Clippy passed in 153.93 seconds.
The remaining thirteen exact filters ran together through mature libtest (13/13
in 0.67 seconds), preserving the bounded set. Windows/macOS are unrun; contextual
default restrictions, type/assignment,
full headers, stock Parameter metadata and semantic publication remain required.

## DB2 shared FUNCTION parameter names

`tsk-6375` implements these qualified native selectors:

```text
//param_decl_3/parameter_name
//param_decl_3/data_type
//param_decl_3/method_reference_type
//param_decl_3/routine_default_clause/routine_default_value
//sourced_function_param_decl/parameter_name
//sourced_function_param_decl/sourced_function_parameter_type
//sourced_function_param_decl/routine_default_clause/routine_default_value
```

an empty `CapturePath` operation list selects raw names only when present, actual types
through their listed native branches, and supplied default values. A mixed three-slot list with two named
slots yields two ordered name captures, three types and three default values;
an unnamed type is never fabricated as a Parameter. Name/type parents are
`param_decl_3` for OLE and `sourced_function_param_decl` for sourced/TEMPLATE.
Default values remain under `routine_default_clause` in their actual parameter.
Other routine/cursor/CALL contexts stay isolated.

Optional-name recognition uses mature ANTLR adaptive prediction and the existing
data-type rule. OLE controls use character/graphic types. Single-ID and quoted
nominal controls occur only in sourced/TEMPLATE; qualified nominal syntax
is supplied by completed name producers and catalog classification stays external. Lists still require at
least one slot when present. `tsk-a72c` subsequently makes their two caller
references optional for zero-argument headers, as described below. SQL names
remain required. No helper widening, aliases, spelling guards or normalization
are used. Stock Parameter metadata/publication is a separate required delivery.

The new focal passed 1/1 in 0.47 seconds; fifteen exact affected DB2 regressions
passed 15/15 in 1.67 seconds, and strict Clippy passed in 110.26 seconds.
Official Parser Tool 4.13.2 passed once in 2.28 seconds; raw
Parser4 matches its stage and all twelve Lexer/Unicode outputs are unchanged.
Windows/macOS are unrun; full header/type/context, aggregate and X requirements
remain open.

## DB2 Method ordinary return paths

`tsk-ab50` adds these native selectors with an empty `CapturePath` operation list:

```text
//method_signature/method_ordinary_return_type
//method_specification/method_specification_return_clause/method_ordinary_return_type
//method_ordinary_return_type/method_spec_parameter_type
```

The first two select the complete raw ordinary type, excluding AS LOCATOR.
Their parents are `method_signature` and `method_specification_return_clause`;
the third selects the mature type child under `method_ordinary_return_type`.
Method parameter selectors retain their explicit list/declaration ancestors,
so the reused type helper does not fabricate Parameter captures from returns.
Both CAST branches yield zero hits through these ordinary paths. At ab50
delivery, signatures used `data_type_3`/`data_type_4`, and Methodspec had two direct
`data_type` children. At tsk-e94e delivery, its composition changed the target context while
retaining each source. The XML delta below adds dedicated source contexts and
preserves these ordinary selectors.
There is no old ordinary getter alias or normalized tree. The typed clause
selector delivered by `tsk-231a` remains valid. Stock metadata is still pending.

See the [ordinary return domain](rust_ast_antlr.md#db2-method-ordinary-return-types)
and [primary boundary](rust_ast_antlr_profile_reconciliation.md#independent-method-ordinary-return-types).
The new focal passed 1/1 in 1.45 seconds; all sixteen exact affected DB2
regressions passed 16/16 in 1.76 seconds. Strict Clippy passed in 83.60 seconds.
Raw Parser4, twelve unchanged Lexer/Unicode outputs, rustfmt, authored checks
and all 47 maintained local links/anchors passed. Windows/macOS are unrun;
Source recognition is not server catalog validation.

## DB2 Method CAST target paths

At tsk-e94e delivery, the target selected a generic data_type or inferred
attributes. The XML correction in tsk-4c23 keeps both target selectors and the
inferred selector, and adds typed source selectors:

```text
//method_signature/method_cast_target_type
//method_specification/method_specification_return_clause/method_cast_target_type
//method_cast_target_type/inferred_builtin_type
//method_signature/method_cast_source_type
//method_specification/method_specification_return_clause/method_cast_source_type
//method_cast_non_xml_type/method_cast_non_xml_builtin_type
```

Targets and sources select their exact raw types, excluding RETURNS, CAST FROM
and source AS LOCATOR. A non-inferred target and each source have a
method_cast_non_xml_type child. Its builtin child retains the former native
alternatives except XML; existing opaque anchored/row/array name alternatives
keep their native children. Inferred targets retain inferred_builtin_type.
Quoted nominal names are not classified as builtins by spelling.

Signature's former data_type_4 source getter and the unconsumed rule are removed.
Methodspec no longer has a direct generic source data_type child. Both expose
method_cast_source_type. That Method delta left external-scalar data_type_3,
ordinary return paths and Method parameter selectors unchanged. The later
external target composition belongs to tsk-8187. No compatibility
getter or normalized tree is supplied.

The XML delta's native focal and seven affected cases passed. Scoped strict
Clippy passed in 82.784 seconds after the imported-consumer owner correction;
final bounded tracker review reports NOFAULT for this XML delta. The later REF
syntax contract supersedes original MC1 catalog/positive-class enumeration gates;
its own affected product evidence is separate. See the
[primary boundary](rust_ast_antlr_profile_reconciliation.md#explicit-xml-correction-and-remaining-nominal-syntax)
and [recognizer contract](rust_ast_antlr.md#db2-method-cast-explicit-xml-exclusion).

## DB2 shared FUNCTION zero-argument paths

`tsk-a72c` retains the three existing an empty `CapturePath` operation list selectors:

```text
//param_decl_3/parameter_name
//param_decl_3/data_type
//param_decl_3/method_reference_type
//param_decl_3/routine_default_clause/routine_default_value
//sourced_function_param_decl/parameter_name
//sourced_function_param_decl/sourced_function_parameter_type
//sourced_function_param_decl/routine_default_clause/routine_default_value
```

In empty OLE DB/sourced/TEMPLATE headers each returns zero matches. Neither
`param_decl_list_3`/`param_decl_3` nor the sourced
`sourced_function_param_decl_list`/`sourced_function_param_decl` exists under its
corresponding empty header; the list getter can be absent. Return-table columns, function names
and SOURCE names are not parameter captures. Hidden comments inside `()` remain
tokens, without a fabricated list/slot/default or compatibility node.

Nonempty lists preserve the same raw captures, declaration parents, order and
optional-name behavior. SQL required names and other routine/header/Method
selectors remain unchanged. Parentheses are still mandatory. The
[primary arity boundary](rust_ast_antlr_profile_reconciliation.md#independent-shared-function-zero-arguments)
and [recognizer contract](rust_ast_antlr.md#db2-shared-function-zero-arguments)
explain the OLE DB text/diagram divergence and necessary type/context gaps.
Native fixture qualification passed 1/1 in 0.19 seconds; eighteen exact affected
DB2 filters passed 18/18 and strict Clippy passed. Windows/macOS are unrun. Shipped Parameter metadata/publication remains a separate delivery.

## DB2 external-table zero-argument paths

`tsk-c72f` uses these anchored native an empty `CapturePath` operation list selectors:

```text
//create_function_external_table_statement/ext_table_param_decl_list/ext_table_param_decl/parameter_name
//create_function_external_table_statement/ext_table_param_decl_list/ext_table_param_decl/data_type
//create_function_external_table_statement/ext_table_param_decl_list/ext_table_param_decl/routine_default_clause/routine_default_value
```

Each selects zero nodes in empty external-table FUNCTION headers. Neither
`ext_table_param_decl_list` nor `ext_table_param_decl` exists; the unchanged list
getter can be absent. Return-table columns and other routine/local names do not
become header parameters. Comments inside otherwise empty parentheses remain
raw hidden tokens, without synthesized nodes or compatibility getters.

Named/unnamed/mixed nonempty slots keep exact raw capture text, order and actual
declaration parents. A default value remains under `routine_default_clause`
under `ext_table_param_decl`. Existing slot locators, all other header selectors
and SQL required names are unchanged. The
[primary arity boundary](rust_ast_antlr_profile_reconciliation.md#independent-external-table-function-zero-arguments)
and [recognizer contract](rust_ast_antlr.md#db2-external-table-function-zero-arguments)
retain necessary contextual type/return/options and shipped metadata work.
Native fixture qualification passed 1/1 in 0.18 seconds; nineteen exact affected
DB2 filters passed 19/19 and strict Clippy passed. Windows/macOS are unrun.

## DB2 SQL zero-argument paths

`tsk-106e` uses these anchored native an empty `CapturePath` operation list selectors:

```text
//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/parameter_name
//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/data_type_1
//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/routine_default_clause/routine_default_value
```

Each selects zero nodes in empty SQL FUNCTION headers. No native
`param_decl_list_2` or `param_decl_2` exists; the unchanged list getter can be
absent. Return types, body/local declarations and parameters of other headers do
not become SQL header parameters. Hidden comments remain raw tokens, without
synthesized lists, slots or getters.

Every nonempty SQL slot still requires its name. Ordered raw captures and actual
`param_decl_2` parents remain unchanged; default values stay under
`routine_default_clause` under that declaration. Other FUNCTION consumers keep
their current optional-name behavior. The
[primary arity boundary](rust_ast_antlr_profile_reconciliation.md#independent-sql-function-zero-arguments)
and [recognizer contract](rust_ast_antlr.md#db2-sql-function-zero-arguments) retain
necessary contextual type/return/options and shipped metadata work.
Native fixture qualification passed 1/1 in 0.29 seconds; twenty exact existing
affected DB2 filters passed 20/20 and strict Clippy passed. Windows/macOS are unrun.

## DB2 external scalar zero-argument paths

`tsk-e3df` preserves the existing `tsk-20e3` an empty `CapturePath` operation list selector
order and adds an anchored parameter-type selector:

```text
//ext_scalar_param_decl/routine_default_clause/routine_default_value
//ext_scalar_param_decl/parameter_name
//create_function_external_scalar_statement/ext_scalar_param_decl/data_type
```

Q0 default value, Q1 parameter name and Q2 parameter type each select zero nodes
for an empty external scalar header. No `ext_scalar_param_decl` exists; the
unchanged native vector getter is empty. Real return type/options and parameters
from other headers do not become external scalar parameters.

Nonempty names and types retain their direct declaration parents; default values
remain under `routine_default_clause` under that declaration. Ordered raw spans
and `SelectedNode` captures are preserved, including unnamed and mixed slots.
Hidden comments remain raw tokens, without synthesized contexts. See the
[primary arity boundary](rust_ast_antlr_profile_reconciliation.md#independent-external-scalar-function-zero-arguments)
and [recognizer contract](rust_ast_antlr.md#db2-external-scalar-function-zero-arguments).
Native qualification passed 1/1 in 0.18 seconds; twenty-one exact existing
affected DB2 filters passed 21/21 and strict Clippy passed. Windows/macOS are
unrun. Full contextual types/returns/options and shipped
metadata qualification remain necessary.

## DB2 external scalar parameter locator path

`tsk-ddbd` preserves existing Q0 default/Q1 name and anchored Q2 parameter type,
and adds Q3 using an empty `CapturePath` operation list:

```text
//ext_scalar_param_decl/routine_default_clause/routine_default_value
//ext_scalar_param_decl/parameter_name
//create_function_external_scalar_statement/ext_scalar_param_decl/data_type
//ext_scalar_param_decl/as_locator
```

Q3 selects the direct `as_locator` child of each scalar parameter declaration,
in source order. Its original span covers AS through LOCATOR, including hidden
intervening comments. The parameter span includes that suffix. The official
local context getter is `As_locatorContext *as_locator()`, referring to the
existing helper. Without the modifier the getter/child/capture is absent; mixed
slots expose locators only where written. Empty headers yield zero Q0–Q3 and
no declaration. A Method return locator is not an external scalar parameter.

The original Q2 selector observes the direct data_type branch. The later
[external parameter type paths](#db2-external-scalar-parameter-type-paths) select the
separate reference branch; the existing four-query fixture retains its scope.

Names and types retain declaration parents; default values remain under their
routine-default clause under that declaration. No synthetic flag, alias or range
normalization is introduced. The
[primary modifier and semantic boundaries](rust_ast_antlr_profile_reconciliation.md#independent-external-scalar-parameter-locator)
and [recognizer contract](rust_ast_antlr.md#db2-external-scalar-parameter-as-locator)
distinguish syntax from LOB/catalog/FENCED/default legality.
Native qualification passed 1/1 in 0.18 seconds; twenty-two exact existing
affected DB2 filters passed 22/22 and strict Clippy passed. Windows/macOS are
unrun. Full contextual types/options and shipped metadata
qualification remain necessary.

## DB2 SQL typed return column paths

`tsk-1b14` uses these native an empty `CapturePath` operation list selectors:

```text
//sql_function_return_column/column_name
//sql_function_return_column/sql_function_return_data_type
//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/parameter_name
```

Q0 name and Q1 declared type select exactly one raw node per column in source
order. Their direct parent is `sql_function_return_column`; its parent is the
table or row list, under the SQL FUNCTION caller. Q2 preserves actual header
parameters: return columns and nominal/REF identifiers do not become parameters.
Hidden comments, quotes/case and UTF-8 byte spans remain original.

The official caller exposes `Sql_function_return_table_columnsContext` and
`Sql_function_return_row_columnsContext`. Both list contexts expose a vector
and indexed getter for `Sql_function_return_columnContext`; columns expose
`column_name()` and `sql_function_return_data_type()`. Types expose local
builtin/nominal contexts or the REF shape. The nominal path remains neutral
about distinct versus structured catalog classes. Global old list contexts are
not synthesized under the corrected caller, and no compatibility alias is added.

Scalar and other headers contain no local typed-return column nodes. Existing
parameter names/defaults, return alternatives and unrelated consumers retain
their contracts. See the [primary binding and boundaries](rust_ast_antlr_profile_reconciliation.md#independent-sql-typed-return-columns)
and [recognizer contract](rust_ast_antlr.md#db2-sql-function-typed-row-and-table-returns).
Corrected native qualification passed 1/1 in 1.50 seconds; twenty-three exact
existing affected DB2 filters passed 23/23 and strict Clippy passed. Windows/macOS
are unrun. Full contextual types/options and shipped metadata
qualification remain necessary.

The initial focal failed at FROM SYSIBM.SYSDUMMY1 (101, 0.02 seconds; Cargo
92.56 seconds). The preserved initial full log records the qualified-table
limitation. Primary page 1255 uses unqualified EMPLOYEE and READS SQL DATA; the
corrected derived controls use SRC_DUMMY_F with one-row catalog precondition
and READS SQL DATA, including ROW controls that read a table. No table-name,
fullselect or direct-RETURN grammar was changed by that unit. Those controls
did not qualify QName or VALUES/RETURN. Ordinary FROM QName is addressed by
`tsk-6012` below; the independent `tsk-cf2d` contract is implemented by `tsk-7f03`.

## DB2 FROM QName selectors

Ordinary FROM exposes the original complete QName and its components:

```text
//from_clause//singles_table_reference/table_reference_name
//from_clause//singles_table_reference/table_reference_name/table_name
//from_clause//singles_table_reference/table_reference_name/table_reference_schema
//from_clause//singles_table_reference/table_reference_name/data_source_name
//from_clause//table_reference//table_name
```

The first selector matches one QName per reference, excluding correlation and
sampling suffixes. an empty `CapturePath` operation list captures that same node and
`CaptureStep::NearestAncestor("singles_table_reference".into())` captures its actual parent.
The next selectors expose the final component, optional schema and remote-only
data source. Local two-part names have schema and no data source; remote
three-part names have both. A quoted name containing dots is one component.

`SyntaxNode.status == SpanStatus::Regular` and
`source.get(node.span.clone().expect("regular selected span"))` expose the
original bytes, including quotes/doubled quotes and inter-component comments.
Do not fold case, remove quotes or substitute resolved names for source.

`Singles_table_referenceContext` exposes `table_reference_name()` and unchanged
suffix getters. The new context exposes `table_name()`, optional `table_reference_schema()`
and remote-only `data_source_name()`. No Rust/cxx ABI addition or compatibility
getter is added. The schema context holds `schema_name()` or the documented
SESSION token, without changing the global schema-name rule. The final selector
is the existing shipped SELECTS query and
continues to capture only the last component; schema-aware relationship
identity is not supplied by that profile.

The focal also checks actual typed-return columns and parameters in a complete
SQL FUNCTION using SYSIBM.SYSDUMMY1, and diagnostics with bounded recovery
spans for missing components, repeated dots, four parts and unclosed quotes.
Remote three-component references are positives. See
[primary binding](rust_ast_antlr_profile_reconciliation.md#ordinary-from-qualified-reference-binding)
and [recognizer checks](rust_ast_antlr.md#db2-ordinary-from-table-qnames).

Qualification passed the four affected ANTLR functions and scoped strict Clippy.
The finite Original fixture correction passed its one source-identity test;
see the recognizer contract for the preserved failed attempts and scope.

## DB2 direct RETURN and VALUES selectors

The native focal uses current tree selectors:

```text
//sql_function_body/return_statement
//sql_function_body/return_statement/fullselect/values_clause
//fullselect/values_clause/values_row
//sql_function_body/return_statement/expression
//create_function_sql_scalar_table_or_row_statement/param_decl_list_2/param_decl_2/parameter_name
//sql_function_return_column/column_name
//sql_function_return_column/sql_function_return_data_type
```

Direct RETURN has parent `sql_function_body`; direct VALUES sits below
`return_statement/fullselect`. Values rows have parent `values_clause` and
remain in source order. Grouped fullselect nests another `fullselect` inside
`fullselect_in_parentheses`, so scalar `RETURN(VALUES ...)` is an expression
with descendant VALUES rows, not a direct VALUES match. Compound RETURN is
not an immediate child of the body and does not match the direct selector.
CTE definitions retain `common_table_expression` parents and contain their actual
`fullselect`. Nested WITH has a `common_table_expression_list`; Q2 sees the nested
VALUES rows in source order without treating CTE column names as parameters.
CREATE VIEW definitions contain the existing fullselect below their actual view
statement. Q2 captures a VALUES definition's rows; a SELECT definition has none.

an empty `CapturePath` operation list, `SyntaxNode.span/status/parent` and
`SpanStatus::Regular` preserve original bytes and native parent identity.
Header parameters and typed-return columns retain their existing direct
parents; body method/column identifiers do not become parameters. Tuple
internal expression alternatives are chosen by the native parser, not a
catalog classifier.

The generated SQL body exposes `return_statement()` and both unchanged
compound getters, with RETURN now reached through its statement context.
`FullselectContext` retains its vector/index getters. `Order_by_clauseContext`
exposes ORDER/BY terminals and its existing sort/options or INPUT SEQUENCE.
No terminal RETURN compatibility getter or public Rust/cxx ABI is added.
See [recognizers](rust_ast_antlr.md#db2-values-fullselect-and-direct-sql-return)
and [scope](rust_ast_antlr_profile_reconciliation.md#values-and-direct-return-binding).

These selectors passed the current native focal on Linux, including nested CTEs
and VALUES/SELECT views. Eight affected regressions and scoped strict Clippy also
passed. This establishes parser captures and syntax, without catalog resolution
or server execution; Windows/macOS remain unrun.

## Db2 header Parameter selectors

Product `tsk-f627` replaced the nonexistent `parameter_declaration` descriptor.
The current paths include the sourced/TEMPLATE separation from `tsk-086a`:

```text
//agg_fn_param_decl/parameter_name
//ext_scalar_param_decl/parameter_name
//ext_table_param_decl/parameter_name
//param_decl_3/parameter_name
//sourced_function_param_decl/parameter_name
//param_decl_2/parameter_name
//proc_ext_param/parameter_name
//proc_parameter_list_item/parameter_name
//method_signature/method_param_list/method_signature_parameter_decl/parameter_name
//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name
```

The original nine selectors become ten in `tsk-086a`: `param_decl_3` serves
OLE DB and `sourced_function_param_decl` serves sourced/TEMPLATE. Named
slots produce one Parameter occurrence; unnamed slots produce none. Cursor
parameters, CALL arguments, local variables and referenced SOURCE/FINALIZE
names are outside these declarations. Metadata and provenance remain borrowed
from the effective query, with raw ranges and the original request retained.

The aggregate and method contexts use the existing Function label and
actual name paths. Function and procedure declarations are anchored to their
header rules, avoiding descendant names in references and bodies. Comment
metadata uses the real SQL_COMMENT and LINE_COMMENT tokens. All three shipped
copies implement these declarations and contexts. The internal asset is compiled
by Rust `AstProfileLoader::STOCK` (ast_profile_loader.rs:469–470); its new/load paths
decode and supply stock profiles, so this is a productive Rust input. The Linux focal, both affected
catalog/profile corpus fixtures, typed query-error regression and strict Clippy
passed. The initial fixture, compile and dependency-warning failures remain
recorded. This raw consumer does not
publish entities or infer method owners. See
the [producer scope](rust_ast_antlr_profile_reconciliation.md#db2-aggregate-and-shipped-header-consumers).

## DB2 aggregate interface selectors

The native focal for `tsk-2558` uses actual generated contexts:

```text
//create_function_aggregate_interface_statement/function_name
//agg_fn_param_decl/parameter_name
//state_variable_declaration/variable_name
//aggregate_return_clause/aggregate_builtin_type
//aggregate_return_clause/aggregate_cast_target_type
```

The first path captures only the declared function, excluding the referenced
FINALIZE designator. Parameter and state names have separate native parents;
unnamed slots have no name capture. A simple return has a direct builtin child.
For CAST, the target has its own context and the source is the direct builtin
child of the return clause. Captures preserve raw source bytes and SDK ordinals,
without type inference, decoded names or graph publication. The Linux focal,
nine affected native regressions and strict library/engine-fixture Clippy passed.
Windows/macOS and server behavior remain unqualified. See the [recognizer](rust_ast_antlr.md#db2-aggregate-interface-recognition).

## TSQL REVOKE target observations

The stock `revokes` selector `//revoke_permission//table_name` uses the formal
TSQL REVOKE statement rule. It preserves Relation/REFERENCES metadata and the
selected-node name capture. The matched target has a native `table_name` rule
parented by `revoke_permission`; column lists and principals are separate rules.
A statement without an `ON` target produces no occurrence. The consumer uses the
original PreparedInput context and preserves the raw quoted identifier range and
exact original projection; it does not inspect words in literals or reinterpret
GRANT as REVOKE.

The parser accepts the syntax supported by the existing permission/class/identifier
rules. Catalog existence, allowed permission combinations, grant-option effects
and principal authorization remain outside parsing. The focused implementation
and execution evidence belong to `tsk-5b74`; full indexing/publication and native
execution on other platforms remain separate qualification work.

## COBOL preprocessor query tree

The effective profile owns a separate `PreparedProfileQueries` plan for
`QuerySource::CobolPreprocessor`. The COBOL semantic consumer parses against
the official `Cobol85PreprocessorParser` startRule. It reuses the native
profile selectors, captures, diagnostics and checked result conversion, with no query
fallback or additional public dialect enum. Matches and captures address this
result's syntax tree. `//copyStatement/copySource` observes the raw target,
including an optional `OF`/`IN` library clause. A literal capture selects the
quoted target; `copyLibrary` selects the library when written. A valid absent
capture remains missing, while invalid syntax or vocabulary retains its actual
query index and native exception.

The [COBOL consumer](rust_ast_antlr_semantics.md#cobol-copy-observations-and-native-program-scopes)
uses an explicit query source and separate tree container to preserve main node
ordinals and associate scopes by native COPY keyword identity and parent links.
COPY observation does not expand files or execute replacements. The focused main/COPY qualification passed nine Linux tests. Initial build/runtime
failures are retained in the
[main directive contract](rust_ast_antlr.md#cobol-native-copy-directive-parents).

## DB2 Type declaration paths and contexts

The DT delta in `tsk-6eae` replaces the old zero-match wrapper name selector
with five anchored declaration paths. Each retains `types / Type / entity`
and selects its real raw name:

```text
//create_type_statement/create_type_array_statement/type_name
//create_type_statement/create_type_cursor_statement/type_name
//create_type_statement/create_type_distinct_statement/distinct_type_name
//create_type_statement/create_type_row_statement/type_name
//create_type_statement/create_type_structured_statement/type_name
```

Only the declared name is selected; cursor row references, source types,
supertypes and attribute names do not create another Type occurrence. Each
concrete branch is one configured Type context and declaration selector. Its
name path is direct `type_name`, except `distinct_type_name`. The syntax still
contains `create_type_statement`, but it is no longer a second configured frame.
Methodspec retains a nearest Function context and a named structured Type
ancestor. A context alone does not fabricate an emitted vertex or resolved owner.

Cursor Type now requires the native `AS` keyword, including weak, named-row and anchored-row forms. Quoted structured names and equal Type/Method/Parameter names retain distinct actual node ordinals and spans. The prior baseline found zero stock Type rows and unnamed wrapper frames for valid non-cursor branches, and native syntax errors for legitimate cursor forms with AS. This delta does not qualify ARRAY[] grammar, declared QName, a full Type domain or the DB2 model.

## DB2 Variable and Alias declared subject paths

The VA delta in tsk-6eae introduces native variable_name children for each local
DECLARE name while retaining its shared statement/type/default. The stock
//declare_variable_statement/variable_name path stays intact. Subject ranges are
individual names; the full declaration range belongs to the common statement,
not invented per-name delimiters.

Alias descriptors now use only these declared positions:

~~~text
//create_alias_statement/table_alias/declared_alias_name/alias_name
//create_alias_statement/module_alias/declared_alias_name/alias_name
//create_alias_statement/sequence_alias/declared_alias_name/alias_name
~~~

Only the first Alias name is wrapped; targets and referenced aliases remain raw references. All three stock copies retain aliases/Alias/entity metadata, native name captures and byte agreement. Local-name groups, quoted names, chains and equal declared/target spelling do not collapse native anchors or create target entities. Model identity/owners remain separate consumer work; raw selectors do not resolve catalog existence or target class.

## DB2 consolidated native prerequisites and context names

The joint tsk-6eae correction closes the actual local DEFAULT value caller,
Index basic keys/CLUSTER, Trigger target wrapper, five context names and contextual
Type QName shapes. Local DECLARE uses declare_variable_default_clause with mandatory
value; DDL default_clause keeps its separate empty value/ WITH forms. The raw
helper name/parent change is exposed directly, without compatibility aliases.

Index key expressions reuse the mature expression rule; direction and native
CLUSTER are optional. CLUSTER is a real main-Lexer token and contextual id_ name.
Direct key Column subjects are specifications with Index ancestry; expression
references and ON targets are not physical Table definitions or semantic owners.
Trigger REFERENCES captures table_or_view_name without catalog classification.
The five explicit stock context paths are index_name/table_name/view_name/
trigger_name/schema_name under their actual statements. An unnamed authorization
Schema stays Missing and does not fabricate a named owner.

Type/Distinct names support optional schema through unqualified_type_name; Method
named-type has no duplicate schema prefix. Module-object TYPE conserves max1.
Reference-only ARRAY/ROW/CURSOR helpers retain1–3 native components, including
written module qualifiers. These lexical shapes do not infer datatype classes.
Getters/tree parents change honestly; existing Type stock rule names and full
anchors remain. Unrelated supertype and full catalog domains remain necessary. MODULE object
definitions are covered by the current MODULE contract below.

## Method CAST REF capture boundary

A native `//method_reference_type` query observes the complete REF constructor;
`//method_reference_type/method_named_type` observes its mature nominal child.
The parent is `method_cast_non_xml_type`, beneath the actual target or source
wrapper. Native ranges retain quotes, comments and original byte/point evidence.

Stock Method name/Parameter selectors and query policies stay unchanged. Typed consumers retain the existing CastTarget/CastSource wrapper ranges; the referent name does not create a Type declaration or guessed relationship.

## DB2 MODULE subjects and namespace

The shipped DB2 selectors now observe actual object-definition names:

~~~text
//module_type_definition/unqualified_type_name
//module_variable_definition/variable_name
~~~

They retain types/Type/entity and variables/Variable/entity metadata. The
configured alter_module_statement Module context captures module_name, including
its written schema qualifier. The native registry associates that actual name
with the context; it creates no selected Module vertex. The internal owner-only
subject remains available to namespace lookup but is rejected in both direct
name and whole-statement custom Entity selection paths, independently of labels.
UnsupportedDeclaration or NameKindMismatch diagnoses the invalid canonical
declaration position through the existing model API. Each object keeps the
exact helper declaration/full extent, independently of the ancestor ALTER
statement extent. Referenced types, fields, defaults and anchor objects do not
become declarations. Quoted dots remain part of one identifier.

The real collect/assemble consumer preserves original QName components, namespaces, typed slots, diagnostics and tuple UID/data_key. Stock embedding still selects Type and excludes Variable; Graph emits both. Native cursor SELECT relations remain ObservedOnly. Original/Source/path/hash/version association uses the existing producers. The MODULE packet contains affected SDK/model/NN/Graph fixtures and execution boundaries.

## Db2 external scalar ordinary return type paths

`tsk-2526` exposes the ordinary result through these direct native paths:

    //create_function_external_scalar_statement/external_scalar_ordinary_return_type
    //create_function_external_scalar_statement/external_scalar_ordinary_return_type/data_type
    //create_function_external_scalar_statement/external_scalar_ordinary_return_type/method_reference_type
    //create_function_external_scalar_statement/external_scalar_ordinary_return_type/procedure_external_xml_type

REF retains `method_reference_type/method_named_type/type_name`. XML retains
`procedure_external_xml_type/integer_value` and optional `kmg`. Use
an empty `CapturePath` operation list for the complete raw wrapper or branch, including
case, quoted UTF8 and hidden comments. The wrapper covers the type, while an
ordinary locator remains its statement sibling:

    //create_function_external_scalar_statement/as_locator

An omitted locator/unit has no node. The old direct statement
`data_type_2` query selects zero ordinary return nodes. `tsk-086a` also replaces
CAST source with its own wrapper below. CAST targets and external scalar
parameter names/defaults retain their contracts. No alias or normalized tree masks
the new parent/ordinal.

The native focal projects these ranges against a distinct Original allocation and verifies both text and pointer origin. Malformed REF/XML and empty ordinary inference retain native diagnostics and strict Syntax errors. Nominal classes, XML/locator legality, host mappings and catalog lookup remain outside parsing. The [model contract](rust_ast_antlr_semantics.md#db2-native-ordinary-external-return-observations) uses the complete wrapper as Return.

## Db2 remaining FUNCTION type domains

Task `tsk-086a` composes caller-local types using the ANTLR C++ backend. Complete native ranges use an empty `CapturePath` operation list:

```text
//create_function_external_scalar_statement/external_scalar_cast_source_type
//ext_table_param_decl/data_type
//ext_table_param_decl/method_reference_type
//ext_table_param_decl/procedure_external_xml_type
//create_function_external_table_statement/external_table_return_type
//param_decl_3/method_reference_type
//create_function_old_db_external_function_statement/oledb_return_type
//sourced_function_param_decl/sourced_function_parameter_type
//fn_return_opts_item/sourced_function_return_type
//fn_return_opts_item/template_function_return_type
//fn_return_opts_item/designator_data_type_list/designator_data_type
```

CAST source has explicit attributes; target `data_type_3` retains independent inference. A source locator remains a statement sibling, outside the type range. External-table parameters accept REF and XML AS CLOB through existing helpers; DEFAULT precedes the locator. TABLE and OLE return-column wrappers pair with direct column-name siblings in native order. GENERIC TABLE has no return-column observations.

Sourced and TEMPLATE parameters both accept empty parameterized attributes. Sourced returns accept them too; TEMPLATE returns require explicit attributes. For example:

```sql
CREATE FUNCTION APP.F_SOURCE(P_CHAR CHAR(),DECIMAL() DEFAULT 1)
RETURNS VARCHAR() SOURCE APP.F_TARGET(CHAR(),DECIMAL());
CREATE FUNCTION APP.F_TEMPLATE(P_CHAR CHAR(),DECIMAL())
RETURNS VARCHAR(40) AS TEMPLATE;
```

RETURNS and one SOURCE or AS TEMPLATE branch are required. SOURCE supports a name, SPECIFIC name or signature; existing `designator_data_type_list` permits inferred attributes except FLOAT(). SOURCE names/types are references and create no extra Function, Parameter or Return. The repeated `fn_return_opts_item` envelope is removed, with no compatibility children. Malformed forms preserve native recovery, diagnostics and strict Syntax errors.

The model uses complete wrappers for Return/CastSource/ReturnColumn and native parameter types for Declared. The [model focal](../../crates/graphit-ast/tests/antlr_db2_model.rs) checks column order, named/anonymous slots, SOURCE isolation and distinct prepared/Original allocations. The [native focal](../../crates/graphit-ast/tests/db2_remaining_function_domains.rs) checks the four domains, causal negatives, UTF8/CRLF/comments, parents/ordinals and EOF. Official ANTLR4.13.2 generation and all sixteen affected Linux tests passed.
Scoped Clippy passed for the library and six affected bins (Cargo1m53s), with
no Rust warnings; existing generated C++ warnings remain in its log.
Windows/macOS and Db2 server execution remain unrun. See the [primary contract](rust_ast_antlr_profile_reconciliation.md#db2-remaining-function-domains).
