---
title: Rust AST ANTLR profile reconciliation
description: Native grammar mappings, delivered profile capabilities and historical defects.
doc_type: technical
references:
  - target: {type: task, id: tsk-2526}
    relation: implements
  - target: {type: task, id: tsk-3397}
    relation: specifies
  - target: {type: task, id: tsk-0b48}
    relation: implements
  - target: {type: task, id: tsk-82b9}
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
  - target: {type: task, id: tsk-20e3}
    relation: supports
  - target: {type: task, id: tsk-086a}
    relation: supports
  - target: {type: task, id: tsk-4eda}
    relation: supports
  - target: {type: task, id: tsk-4c23}
    relation: supports
  - target: {type: task, id: tsk-ab50}
    relation: relates_to
  - target: {type: task, id: tsk-0b99}
    relation: supports
  - target: {type: task, id: tsk-3316}
    relation: supports
  - target: {type: task, id: tsk-942f}
    relation: supports
  - target: {type: task, id: tsk-ac8b}
    relation: supports
  - target: {type: task, id: tsk-1787}
    relation: supports
  - target: {type: task, id: tsk-9724}
    relation: supports
  - target: {type: task, id: tsk-5cb2}
    relation: supports
  - target: {type: task, id: tsk-04a9}
    relation: supports
  - target: {type: task, id: tsk-6eae}
    relation: supports
  - target: {type: task, id: tsk-0eae}
    relation: supports
  - target: {type: task, id: tsk-bd58}
    relation: implements
  - target: {type: task, id: tsk-8121}
    relation: implements
  - target: {type: task, id: tsk-6de5}
    relation: implements
  - target: {type: task, id: tsk-b0ab}
    relation: implements
  - target: {type: task, id: tsk-8b3f}
    relation: implements
  - target: {type: task, id: tsk-dd3e}
    relation: implements
  - target: {type: task, id: tsk-73e3}
    relation: implements
  - target: {type: task, id: tsk-1d33}
    relation: supports
  - target: {type: task, id: tsk-3f5d}
    relation: uses
  - target: {type: task, id: tsk-08df}
    relation: uses
  - target: {type: task, id: tsk-0953}
    relation: uses
  - target: {type: task, id: tsk-0ce2}
    relation: supports
  - target: {type: task, id: tsk-a781}
    relation: supports
  - target: {type: task, id: tsk-16e5}
    relation: supports
  - target: {type: task, id: tsk-4192}
    relation: supports
  - target: {type: task, id: tsk-b91f}
    relation: supports
  - target: {type: task, id: tsk-b847}
    relation: specifies
  - target: {type: task, id: tsk-5260}
    relation: derived_from
---

# Rust AST ANTLR profile reconciliation

For maintainers of Graphit code indexing and the five ANTLR profiles. The
[semantic consumer](rust_ast_antlr_semantics.md) preserves configured queries
and reports invalid SDK vocabulary with its query origin. The current Linux S7
fixture compiles all configured fields and observes all five stock profiles,
including the real PostgreSQL body and COBOL preprocessor consumers. S7 and the
five concurrent consumers passed under `tsk-1d33`.
Stock descriptor counts are not a compatibility contract: equivalent invalid
descriptors may be consolidated with native capability and metadata proof.
User-supplied duplicate queries retain SDK behavior. The inventory below records
the original failures; the current consumer qualification is described separately.

## SDK configuration inventory

The existing full-profile fixture compiles every configured query
`pattern` field and each configured capture independently with the native SDK. It
also compiles the union of context selectors and every context name path,
using the dialect entry rule as a neutral capture anchor. A bad pattern or
context key therefore cannot hide an invalid capture. Errors retain the
original language, query index, data key, field and SDK cause; cancellation
and deadline errors abort the test instead of becoming configuration findings.
Valid selectors with zero matches remain valid.

The historical Linux run over 346 queries found 14 invalid query `pattern` fields and seven invalid context
selectors. Configured captures and context name paths reported no additional
compilation errors. These are the first native errors in each invalid field;
they do not prove the semantic correctness of valid paths.

| Profile | Historical query index and unknown SDK rule | Historical unknown context selectors |
|---|---|---|
| PL/SQL | 24 `tablespace_name`; 35 `record_type_definition`; 36 `table_type_definition`; 37 `varray_type_definition`; 38 `ref_cursor_type_definition`; 49 `function_call`; 50 `procedure_call` | `record_type_definition`, `table_type_definition`, `varray_type_definition`, `ref_cursor_type_definition` |
| PostgreSQL | 15 `compositetypestmt`; 16 `createenumstmt`; 17 `createrangestmt`; 22 `decl_statement` | `compositetypestmt`, `createenumstmt`, `createrangestmt` |
| TSQL | 40 `revoke_permission` | None |
| DB2 | 16 `parameter_declaration` | None |
| COBOL | 136 `copyStatement` | None |

These historical findings required real selectors and legitimate producers.
The subsequent semantic assertions remain in the same test and now pass with the
current corrected products. Task `tsk-dd3e` corrected the valid but ineffective
GRANT path independently. PostgreSQL body variables/complexity now use the
PL/pgSQL consumer, and Db2 comment metadata uses its actual native tokens. This
raw-observation result does not qualify graph publication.

## PL/SQL type definitions and bodies

The native grammar has one `create_type` declaration, containing either
`type_definition` or `type_body`; both contain `type_name`. The active
[PL/SQL profile](../../crates/graphit-ast/profiles/queries/plsql.yaml) retains these two entity queries with `data_key: types`, `graph_label: Type`
and the selected-node name capture:

| Query index | Native selector | Meaning |
|---|---|---|
| 8 | `//create_type/type_definition/type_name` | Type definition |
| 9 | `//create_type/type_body/type_name` | Type body |

`context_types` contains one `create_type: Type` entry, and `declaration_types`
contains `create_type` once. Both branches therefore retain their declaration
context. The nonexistent `create_type_body` descriptors were removed. A nested
`type_body` context would duplicate the same declaration and is not added.
This change does not assert that a native context range is a validated full
entity extent, resolve names, assign UIDs or publish graph relationships.

The active YAML and the catalog/schema-decoder copies are identical. The
[focused fixture](../../crates/graphit-ast/tests/antlr_semantic.rs) decodes the
actual shipped queries rather than constructing replacements:

```sql
CREATE OR REPLACE TYPE BODY pedido_t AS
  MEMBER PROCEDURE executar IS BEGIN NULL; END;
END; /

CREATE TYPE pedido_t AS OBJECT (id NUMBER);

CREATE TABLE pedido(id NUMBER);
```

The expected results are respectively query 9 only, query 8 only and neither
query. The positive captures are native Rule `type_name` nodes with raw name
`pedido_t`, beneath the corresponding branch and the single `create_type`
ancestor. All three cases passed on Linux with empty diagnostics. The
qualification uses the actual shipped queries and native Rule ranges.

Affected commands:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_semantic plsql_type_body_profile_follows_sdk_declaration_branches --jobs 1
cargo xtask cargo test --locked --offline -p graphit-ast --test catalog all_45_real_profiles_retain_inventory_and_declarative_routes --jobs 1
cargo xtask cargo test --locked --offline -p graphit-ast --test profiles all_45_profiles_preserve_scalar_values_sites_and_inventory --jobs 1
```

The latter two checks validate the actual changed corpus inventory and routes;
other profiles retain their contents. Windows/macOS execution is unrun.

## PL/SQL local types and tablespaces

Four equivalent local Type descriptors used nonexistent `*_type_definition`
rules. The native `type_declaration` rule puts `identifier` beside one of
`record_type_def`, `table_type_def`, `varray_type_def` or `ref_cursor_type_def`.
One `//type_declaration/identifier` entity query therefore covers all four
forms. Its `types`, `Type` and selected-node name metadata are preserved.
The `antlr_contexts` entry selects `//type_declaration`, labels it `Type` and
names it with `[{xpath: '/identifier'}]`;
other Type contexts and the two type-definition/body queries remain intact.

Tablespace names are grammar labels rather than a `tablespace_name` Rule.
The query selects `//create_tablespace` and names it with
`[{nearest_descendant: id_expression}]`, using breadth-first nearest traversal.
Permanent, temporary and undo branches each have
their creation name at that location. Table creation and ALTER TABLESPACE do
not become tablespace creation entities.

The three YAML copies now contain 86 queries after consolidation. This count
records the actual corpus, not a fixed requirement. The focused
`plsql_declarations_preserve_native_types_and_tablespaces` fixture decodes the
actual shipped subset without repairing its fields. It checks three tablespace
forms, table/ALTER negatives and four local types (`r_t`, `t_t`, `v_t`, `c_t`)
with four distinct native declaration contexts; the ordinary variable `comum`
is excluded. It verifies borrowed query metadata, native Rule names/ranges and
exact original-name projections. All six PL/SQL fixture inputs passed on Linux
with zero SDK diagnostics (one focal test, 0.16 seconds). The two affected corpus inventory checks and
current strict Clippy qualification passed; the earlier ENOSPC stopped
compilation before testing and was recovered by coordinated cache cleanup.
The SDK structural guards and raw function/procedure call-profile correction
are delivered by `tsk-1f8a` and `tsk-5cb2`. Dedicated native family contracts
are complete in `tsk-04a9`; raw dedicated production is qualified on Linux in
`tsk-b847` (four native tests, two affected inventory checks and scoped strict Clippy). The raw consumer `tsk-1d33` is qualified within its recorded scope;
semantic model and publication remain necessary in `tsk-a4cd`.

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_semantic plsql_declarations --jobs 1
```

## PostgreSQL Type declarations

The grammar places composite, enum and range declarations in `definestmt`,
along with shell/base types, operators and collations. The shipped Type query
uses `pattern: '//definestmt'`, `keyword: TYPE` and `select: '/any_name'`.
Its guard checks direct terminal text `TYPE`,
including lowercase spelling; `TYPE_P` is the grammar symbol rather than the
text being matched. One entity query replaces three equivalent invalid
selectors, retaining `types`, `Type` and selected-node name metadata. Its
`antlr_contexts` entry selects `//definestmt` with `keyword: TYPE`, label `Type`
and name `[{xpath: '/any_name'}]`.

The three PostgreSQL copies contain 37 queries. Domain and routine selectors,
other metadata and the still-unresolved body-variable descriptor remain
intact. The focal `postgresql_type_profile_captures_native_declaration_variants`
fixture checks composite, enum, range, shell and base forms, lowercase and a
mixed script. Operator, collation and domain declarations do not match this
Type selector. Names, borrowed metadata, native Rule ranges, exact original
projections and named `definestmt` contexts are asserted. All ten inputs passed on Linux with zero SDK diagnostics (one focal test,
0.06 seconds). Both affected corpus inventory checks and strict Clippy for
the library and semantic fixture passed. At that qualification, the corpus had 45 profiles,
994 queries, 198209 input bytes, 8244 wire nodes and 152385 wire text bytes;
scalar text was 152405 bytes, with the five Boolean records unchanged. These
are intrinsic fixture observations, not quotas. Windows/macOS execution is unrun.

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_semantic postgresql_type --jobs 1
```

The body producer in task `tsk-8121` is implemented and qualified on Linux:
SDK `Node.id()` identifies observations only within the same retained
Tree-sitter tree; original-context traversal and a private helper reuse the
already-parsed PostgreSQL host. That producer alone does not qualify body-variable extraction or composition.
The specific consumer in `tsk-82b9` below qualifies body variables and raw decision
routing; resolved graph owners and indexing publication remain separate.

## PostgreSQL routine selectors

The routine queries use direct-terminal guards for
Function and Procedure entities:

```yaml
queries:
  - data_key: functions
    graph_label: Function
    pattern: '//createfunctionstmt'
    keyword: FUNCTION
    select: '/func_name'
  - data_key: procedures
    graph_label: Procedure
    pattern: '//createfunctionstmt'
    keyword: PROCEDURE
    select: '/func_name'
```

The `antlr_contexts` entries use `pattern: '//createfunctionstmt'` and the
corresponding `keyword`, Function/Procedure labels and
`name: [{xpath: '/func_name'}]`. Both declarations retain the existing
`createfunctionstmt` declaration anchor; the declaration list is unchanged.
The semantic consumer reuses official XPath selectors bound to the grammar
during profile preparation. The SQL grammar remains unchanged; the configuration
uses the structured fields documented in [ANTLR profile queries](rust_ast_antlr_profile_queries.md).

Guards compare ASCII-insensitive text of native direct terminal children.
They do not scan the full SQL text or a routine body. A guard literal absent
from those children produces no match; invalid rule/token selector names
still return their SDK cause. Overlapping context selectors return a typed
configuration error identifying both selectors and the actual native node.

The focused fixture reads the shipped YAML with the existing serde-saphyr
parser and selects the two unchanged query objects and their context entries
for this bounded test. Remaining profile queries are retained in the product.
It covers a function, a procedure, both in one script and a lowercase function
whose body literal contains `PROCEDURE`. All four PostgreSQL cases passed
on Linux without syntax diagnostics. Context-selector SDK errors, terminal
rejection and typed overlap fixtures also passed. The updated corpus
decoder and catalog inventory checks passed. Strict Clippy passed for the
library and affected semantic tests.

## TSQL permission targets

`security_statement` contains the direct `GRANT` terminal and the optional
`table_name` target. The target is a sibling of `grant_permission`, so the
former `//grant_permission//table_name` selector was valid but ineffective.
The active [TSQL profile](../../crates/graphit-ast/profiles/queries/tsql.yaml) and both shipped
fixture copies now use `//security_statement[GRANT]/table_name`. All 41 queries
and their metadata remain intact. The GRANT query retains Relation/REFERENCES
and its selected-node name capture.

The focused fixture selects the actual shipped GRANT query with the mature
YAML parser. It checks a qualified `OBJECT::dbo.pedido` target, optional GRANT
without `ON`, unrelated `REVERT`, and a mixed script containing a GRANT word
literal. Expected observations are actual native Rule `table_name` ranges
and exact original projections; absent targets produce no occurrence. All
four GRANT cases passed on Linux with zero SDK diagnostics. The two affected
corpus inventory checks and strict Clippy also passed.
The `tsk-5b74` producer adds the complete `revoke_permission` statement under
`security_statement`, reusing the permission, class, principal, target and
column-list rules. The existing `//revoke_permission//table_name` stock query
now addresses a real SDK rule; its three YAML copies and Relation/REFERENCES
metadata remain unchanged. It selects only an actual `ON` target. Permission
columns, principals, GRANT statements and SQL string literals are not targets;
REVOKE without `ON` produces no target occurrence.

The grammar accepts permission lists, column lists, `GRANT OPTION FOR`, optional
class-qualified targets, `FROM` or `TO`, multiple principals, `CASCADE` and `AS`
in their documented order. The existing lexer already contains the required
tokens. SQL catalog legality, grants and execution success remain server concerns;
the simplified diagram and object-specific examples differ on the grant-option
condition for CASCADE, so no state-dependent predicate is added. See Microsoft's
[REVOKE syntax](https://learn.microsoft.com/en-us/sql/t-sql/statements/revoke-transact-sql?view=sql-server-ver17)
and [object permission syntax](https://learn.microsoft.com/en-us/sql/t-sql/statements/revoke-object-permissions-transact-sql?view=sql-server-ver17).

Only the canonical TSQL parser grammar and four official Parser outputs change.
The native fixture exercises the actual catalog and PreparedInput consumer,
borrowed query metadata, quoted UTF-8/CRLF targets, exact original projection,
strict/recovery diagnostics and original cancellation/deadline. The focused Linux
native test passed: its eleven positive and thirteen malformed inputs exercise
the actual producer and stock consumer. This producer does not qualify the complete
profile pipeline, PostgreSQL body or COBOL preprocessor composition. Windows and
macOS remain unrun.

## PostgreSQL host reuse and native body identity

The tsk-8121 producer adds a crate-private `parse_plpgsql_bodies` entry point for
an existing PostgreSQL host and its original operation. The facade parses that
host once. Tree-sitter nodes and captures expose native `node_id` for joins in
the same retained tree; IDs and local parents remain intact during body range/
point projection. Contextual walk/query/projection APIs preserve the original
I/O cancellation/deadline cause before optional controls. Linux qualification
passed: Tree-sitter 11/11, ANTLR engine 19/19, private host-reuse fixture 1/1 and
affected strict Clippy. Windows/macOS remain unrun.

The [host/body API](rust_ast_antlr_queries.md) documents its PostgreSQL/grammar/
operation preconditions and prepared-coordinate boundary. That producer alone does not extract body variables. The specific consumer in
`tsk-82b9` executes the retained body queries and joins scopes through native parent
links, with real original projection. Full names, symbol UIDs, resolved graph owners
and publication remain separate consumers.

## Refinement prerequisites

Task `tsk-6eae` remains an open DB2 contract refinement. It registered two
Source products: `tsk-1787` implements native DECFLOAT and
parameterized designator signatures, while `tsk-3316` corrects MODULE versus
modulo, delimited identifiers, schema-qualified routine names and variable
reference boundaries. Their canonical G4 and generated files overlap, so
`tsk-3316` follows the real `tsk-1787` product. Neither delivers aggregate
syntax or the nine Parameter queries.

The ARRAY index consumer retains its restricted `data_type_2`; widening it to
repair routine returns would change an unrelated syntax contract. A variable
value reference and a CREATE VARIABLE declaration already use different rules.
The proposed reference node permits one to three name parts; the proposed
schema declaration permits one or two. Local/state/target names stay separate. A
single-part default currently also matches `special_register: id_`, which
precedes the global-variable branch. Product `tsk-3316` must preserve the actual
raw text and node classification for that case; it cannot assert a resolved
Global_variable node. The complete correction packet was applied by normal task
revision in `tsk-3316` before Source. Its name/reference boundaries have native
G4 rules and focal fixtures qualified on Linux by `db2_module_names_native`.
The types producer retains restricted ARRAY indices and shared declaration rules;
its focal native tests inspect signature rules, parents, raw UTF-8/CRLF ranges,
explicit types and real diagnostics for invalid inferred attributes. The focal
and DB2 procedure/control-flow regression pass on Linux. A reserved `P` fixture
was corrected to `PROC_F`, preserving the parser's actual diagnostics and
identifier rules; Windows/macOS remain unrun. Integrated module designators with
inferred VARCHAR()/TIMESTAMP() passed in the name product `tsk-3316`.
Its three affected DB2 regressions and strict library/engine-fixture Clippy passed
on Linux. The first EOF fixture oracle failure remains in Task history; EOF is
asserted as a synthetic token at the empty source-end range rather than compared
to lexical source text. Windows/macOS remain unrun for this product.

The independent C language token correction `tsk-9724` passed its Linux focal
fixture, both affected DB2 regressions and strict Clippy. It preserves the symbolic `C_` token while
recognizing literal C, and retains ordinary C/c names through contextual `id_`.
Its generated DB2 files, shared identifier rule, fixtures and documentation
serialize before `tsk-3316`; the name producer preserves `ID | C_` while adding
contextual MODULE and delimited identifiers with doubled quotes. MODULE previously
lexed as ID; its keyword correction must retain ordinary MODULE/module names.
The [IBM reserved-word reference](https://www.ibm.com/docs/en/db2/11.5.x?topic=sql-reserved-schema-names-reserved-words)
places MODULE in the additional ISO/ANSI list, separate from enforced Db2 words.
Aggregate/default refinement is not a prerequisite for
those name/token corrections. Their raw one-part default observations remain
ambiguous rather than proving variable resolution.

Refinement `tsk-0b99` tracks complete aggregate, default, header and Parameter
coverage. Independent Source leaves may proceed once their own contracts and
completed producers are ready; unresolved X blanks or Method CAST targets do
not block parameter syntax that does not use them. Header refinement
`tsk-086a` created ready Method parameter leaf `tsk-4eda` and separate nominal
CAST refinement `tsk-4c23`. The complete nine-selector Parameter consumer still
requires every corresponding header producer. The
nine selectors are native routine-header paths; blanket `parameter_name`
would also include CALL arguments. The five other function branches cover six
families because sourced and template share a branch. Full module invocations,
expression/target names and entity publication remain explicit requirements.

The [official IBM aggregate diagrams](https://public.dhe.ibm.com/ps/products/db2/info/vr111/pdf/en_US/sqlbook.pdf), printed pages 1143–1145,
were rendered and visually read for this refinement. They confirm optional
parameters, optional state names, two clause orders and eight option families.
They show `(IN? parameter_name)?` before the type: the earlier hypothesis that
IN can appear without a parameter name was rejected and must be a negative
fixture. The diagram's aggregate type set cannot be assumed to equal the
canonical grammar's general built-in union. The [IBM statement](https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-create-function-aggregate-interface)
is reference evidence; the proposed aggregate fixtures remain unrun. Native
designator/type fixtures belong to the separate `tsk-1787` producer.

The same IBM 11.1 manual distinguishes five aggregate type positions. The
parameter and state descriptions exclude XML, LOBs and distinct types at
server validation; those restrictions do not remove XML or
`[SYSPROC.]DB2SECURITYLABEL` from the printed common syntax diagram.

| Position | Printed page | Contract evidence |
| --- | --- | --- |
| Parameter `data-type1` | 1147 | Built-in syntax; server restrictions remain separate |
| Simple return `data-type2` | 1148 | Parameter considerations and no distinct type |
| CAST target `data-type3` | 1148 | Empty inferred attributes permitted, except `FLOAT()` |
| CAST source `data-type4` | 1148 | Cast compatibility; no automatic inheritance of empty target attributes |
| State `data-type5` | 1150 | Parameter considerations and server restrictions |

The web page, versioned document and content API returned HTTP 403, but
[IBM Support](https://www.ibm.com/support/pages/sql-reference-cross-platform-development-0)
links the [official LUW 11.5 SQL Reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_reference_115.pdf).
Its edition dated 2023-02-07 was read visually: aggregate pages 1125–1127
(physical pages 1157–1159) connect all five type positions to the common
`built-in-type` diagram. That diagram includes XML,
`[SYSPROC.]DB2SECURITYLABEL` and DECFLOAT, but does not include BOOLEAN or CURSOR.
The general canonical built-in rule must retain those types for its other
consumers; aggregate needs its own published subset. Pages 1129–1130 allow
empty inferred attributes only at the CAST target, excluding `FLOAT()`; that
permission does not extend to the CAST source. `DECFLOAT()` as a CAST target is
derived from the general inference rule, rather than an executed IBM example.
Precision validation such as `DECFLOAT(17)` belongs to server semantics, rather
than an invented parser negative. These observations close the documentary
version question; the aggregate producer and its fixtures remain pending.

Routine defaults also require a separate contract. The aggregate and external
scalar IBM descriptions permit constants, special registers, global variables,
expressions and NULL. The earlier routine header baseline reused column
`default_clause`, whose optional value and `WITH` prefix do not establish those
routine forms. The scalar, PROCEDURE and remaining FUNCTION corrections below
now compose the dedicated helper; the aggregate consumer still needs integration.
Its shared column consumers and the restricted `param_decl` used by cursor
constructors must remain isolated from routine corrections. Existing
`special_register: id_` does not classify a one-part variable reference: the
raw SDK may select that alternative before `global_variable`. Dedicated
register syntax, signed literals and the expression boundary must be qualified
by their real producers before complete default or Parameter coverage is claimed.

The same 11.5 PDF closes the documentary default contract: page 1127 requires
`DEFAULT` followed by NULL, a constant, a permitted register, a global variable
or `(expression)`. Parentheses are required for the expression alternative;
`WITH DEFAULT`, an omitted value and an unparenthesized arbitrary expression
are column permissions that do not transfer to this routine diagram.
Pages 1358 and 1373 identify direct datetime/user registers, CURRENT SCHEMA and
CURRENT MEMBER. The general register diagram on page 89 and their descriptions
document underscore spellings and USER; timestamp aliases and precision forms
also need their native producer. The timestamp text explicitly permits
`CURRENT TIMESTAMP()` although the general diagram shows an integer inside
optional parentheses. Preserve both source observations when defining fixtures.

Pages 83–88 distinguish signed numeric constants, DECFLOAT special values,
Boolean values, character/graphic/binary/Unicode string families and typed
datetime literals. Their native token classes and raw ranges must remain
distinct. `DEFAULT "UNKNOWN"` or `DEFAULT CURRENT_MEMBER` may instead reference
a global variable. Bare `UNKNOWN` is an existing reserved token outside `id_`
and retains current native diagnostics; this is not a universal SQL restriction.
The source-backed default and header fixture map is preserved in `tsk-0b99`;
it remains proposed and unrun until the corresponding producers execute it.

The general register inventory for parenthesized default expressions is also
documentarily closed by the 11.5 diagram (printed page 89) and descriptions
91–107. It adds the four client attributes (`CURRENT CLIENT_ACCTNG`,
`CLIENT_APPLNAME`, `CLIENT_USERID`, `CLIENT_WRKSTNNAME`) and their `CLIENT ACCTNG`,
`CLIENT APPLNAME`, `CLIENT USERID`, `CLIENT WRKSTNNAME` forms; CURRENT DBPARTITIONNUM,
DECFLOAT ROUNDING MODE, DEFAULT TRANSFORM GROUP, DEGREE, EXPLAIN MODE/SNAPSHOT,
FEDERATED ASYNCHRONY, IMPLICIT XMLPARSE OPTION, ISOLATION, LOCALE LC_MESSAGES/LC_TIME,
LOCK TIMEOUT, MAINTAINED TABLE TYPES FOR OPTIMIZATION, MDC ROLLOUT MODE,
OPTIMIZATION PROFILE, PACKAGE PATH, PATH, QUERY OPTIMIZATION, REFRESH AGE, SERVER,
SQL_CCFLAGS, TEMPORAL BUSINESS_TIME/SYSTEM_TIME and TIMEZONE. In this list CURRENT
prefixes each multiword register; the CLIENT forms are the stated exceptions.
Documented additional aliases are CURRENT_PATH, CURRENT FUNCTION PATH,
CURRENT_SERVER and CURRENT_TIMEZONE. Do not invent underscore forms for the others.

The [IBM general reference](https://www.ibm.com/docs/en/db2/11.5.x?topic=elements-special-registers)
and [CURRENT PATH description](https://www.ibm.com/docs/es/db2/11.5.x?topic=registers-current-path)
support that inventory and FUNCTION PATH alias. Only timestamp has the described
precision argument. The original canonical and upstream ANTLR
`special_register: id_` have no equivalent enumeration. Product `tsk-942f` now
adds native productions, preserves that identifier alternative and keeps column
datetime/user-register rules separate. Its fourteen new alias/component lexer
symbols have contextual `id_` alternatives; DEGREE and locale forms reuse the
existing `DEGREE_`, `LC_MESSAGES_` and `LC_TIME_` symbols.

The Linux focal `db2_special_registers_native` passed with the full general
inventory and existing CREATE VARIABLE/expression consumers. A narrow
`timestamp_register_invocation` branch before function invocation prevents false
native function-name query matches for parenthesized aliases. Qualified names,
two arguments and expression arguments retain actual function observations.
Unparenthesized aliases can still select the existing identifier/column branch;
no resolved variable or register interpretation follows from spelling alone.
Four affected DB2 regressions (names, types, C language and procedure control
flow) and strict library/engine-fixture Clippy passed on Linux. Official Tool
4.13.2 regenerated eight outputs successfully. Windows/macOS are
unrun. Routine defaults, literals, aggregate, common headers and Parameter
consumers remain separate pending products in `tsk-0b99`.

Product `tsk-ac8b` now implements native nontext constants in `constant_`:
signed numbers, DECFLOAT special values with optional signs, Boolean values and
typed datetime strings. The signed helper includes existing L_ONE/L_ZERO tokens
because lexer priority gives them 0/1 instead of DECIMAL_LITERAL. Four new
DECFLOAT keyword symbols preserve ordinary names through contextual `id_`.
Shared numeric/integer/string helpers, column defaults and restricted ARRAY
indices remain separate. UNKNOWN is not Boolean: its old reserved token remains
outside `id_`, so its unquoted isolated default keeps SDK diagnostics, whereas
quoted `"UNKNOWN"` uses the identifier branch. This preserves the current grammar
boundary rather than declaring a universal SQL restriction. Calendar and
assignment validity are not parser checks. The later `tsk-6eae` adds L_ONE/L_ZERO
to `integer_constant`; signed 0/1 therefore use that wrapper with unchanged native
terminals. The `tsk-16e5` regression fixtures preserve exact sign, token, raw and
range checks through this current path.
Official regeneration succeeded, and the corrected Linux focal
`db2_nontext_constants_native` passed. Its first UNKNOWN fixture assumption failed
against the SDK; that failure remains in Task history. The fixture now asserts
actual unquoted diagnostics and quoted identifiers separately. Five affected
DB2 regressions and strict library/engine-fixture Clippy passed on Linux;
Source/documentation review found no remaining divergence. Windows/macOS remain
unrun. Remaining literal,
default and header products in `tsk-0b99` are not established by this change.

Product `tsk-0ce2` adds native G/N/GX/BX/UX strings under `constant_`,
with specific `prefixed_string_constant` family contexts and full raw tokens.
G/N preserve doubled SQL apostrophes and glyphs; BX requires hex pairs and GX/UX
groups of four, including primary `GX'FFFF'`. Empty forms are derived syntax
choices. Old string/numeric helpers, reserved G and isolated ID prefixes remain
separate. This is syntax recognition, with no codepage or UTF16 surrogate check.
The X implementation in `tsk-16e5` adds `HEX_CHARACTER_LITERAL` and
`hex_character_string_constant` below `prefixed_string_constant`. Its finite
`tsk-a781` contract uses hex pairs and SPACE U+0020, preserving case, raw blanks,
apostrophes and ranges. Zero pairs support derived empty/SPACE-only forms. No
server/codepage blank-set completeness, external whitespace transfer, double-byte
acceptance or decoded/stripped value is inferred. Lexer and Parser Tool 4.13.2
passed, with eight raw byte-identical outputs. The Linux X focal passed
(one test, 0.04 seconds; 57 ms wall), as did all six causal DB2 regressions. Four
regressions reran after reconciling their older 6eae wrapper expectations; the
two unchanged prefixed/Unicode PASS results were reused. The first X focal
failed on fixture parameter P (native P_ keyword); quoting the name fixed that
fixture. Odd X hex preserves one diagnostic and no following module in the
observed native recovery. Final strict library/engine-fixture Clippy passed
in 10.42 seconds after the assertion corrections. Windows/macOS remain unrun.
U&/UESCAPE is now delivered
and qualified separately by `tsk-4192`, using the contract from `tsk-b91f`.
These literal results alone do not establish complete default, aggregate,
header or Parameter consumer coverage. Odd `BX'ABC'` retains ID BX and STRING_LITERAL `'ABC'`, with a Regular
diagnostic on the string and strict rejection. The DB2 batch-boundary strategy
now resynchronizes using the official ANTLR API and preserves the following
module with its exact Source range. The earlier absence-of-next-module assertion
captured a recovery limitation; it is superseded while its Task history remains.
Lexer-invalid odd X recovery remains a separate, unchanged contract. Official Tool 4.13.2 regenerated
eight byte-equal outputs. Six affected DB2 regressions passed on Linux;
the corrected focal (one test, 0.06 seconds) and strict Clippy also passed after
the Data owner corrected an unrelated compilation failure. AST made no Data
change; initial recovery and foreign compile failures remain in Task history.
Final Source/documentation review found no remaining divergence. Windows/macOS are unrun.

Refinement `tsk-b91f` defines the implemented Unicode producer `tsk-4192`,
whose focal and affected DB2 regressions passed on Linux:
a full U& constant plus optional UESCAPE context, and per-literal official ANTLR
auxiliary lexer/parser validation. The escape argument is checked first; invalid
arguments retain diagnostics without selecting default backslash. Active four-
and plus-six-digit escapes, paired escapes and SQL quote pairs use declarative
rules with one-scalar error fallbacks after valid tokens. Strict UTF8 conversion
is limited to the one-character argument, with no SQL decoding or normalization.
The existing DB2 tree walk invokes the validator; it translates only new
auxiliary diagnostics with checked original offsets and regular local EOF
boundaries, reusing original operation control. New G4/generated files and two
build source entries are implemented. The Linux focal passed (one test, 0.09
seconds), including invalid escape arguments after UTF8/multiline comments,
original main and auxiliary diagnostic ranges, concurrent custom escape state
and original controls. Seven affected DB2 regressions passed. The initial build
failed before tests in unrelated Data remote_graph code, corrected by its owner
before retry; AST did not alter Data. Strict library/engine-fixture Clippy passed
on Linux; final Source/documentation review found no remaining divergence. Windows/macOS are unrun. This qualification does not complete
routine-default/header/Parameter coverage.

Task `tsk-0eae` closes the raw PL/SQL contract and registers two independent
outcomes. Product `tsk-5cb2` implements the raw correction: it preserves statement
`//call_statement/routine_name`, with selected raw name and ancestral
`call_statement`, and replaces the invalid expression query with
`//general_element_part[rule:function_argument]`, local `id_expression` and
ancestral `general_element`. These wrappers include arguments, chain and dblink;
they are raw structure, not decoded full names. The invalid `procedure_call`
entry was removed after real statement fixtures proved coverage. The
stock ASCII-only reject alternative was removed while retaining its keyword
blacklist and all custom rejects/duplicate query behavior. The three stock copies
are byte-identical; their entry counts are derived inventory, not quotas.

A parenthesized part is a **syntactic candidate**. The same G4 admits collection
indexing, constructors and `general_element` on an assignment LHS. `itens(1) :=
fn(2)` must retain both raw observations and native contexts, without declaring
both resolved calls. `fn()()` has one native part with multiple argument groups;
that does not settle semantic CallSite cardinality. The configured CALLS query
metadata is preserved raw; this stage does not publish an edge.

Completed refinement `tsk-04a9` preserves the family contracts and remaining semantic decisions:
string/numeric/JSON functions, analytic name wrappers, explicit alternatives of
`other_function`, keyword/collection methods, and tabular JSON_TABLE/XMLTABLE.
Cursor attributes and the G4 TIME literal are not calls; wrapper nodes and
OVER/KEEP clauses must not create duplicate callees. Methods such as collection
COUNT may omit parentheses. Use explicit SDK rule/token anchors for each branch,
never the first terminal or an indiscriminate `other_function` query. Resolution
of collection/function/constructor, full names and CallSite cardinality must be
closed before semantic publication. See the
[semantic contract](rust_ast_semantic_extraction.md) for those boundaries.

The structural guard producer `tsk-1f8a` is qualified on Linux: ten profile
tests, one focal contextual fixture and strict Clippy passed after recovery
from a disk-space build failure before tests ran. The new raw product fixtures
passed on Linux: eight cases, custom duplicate query/regex preservation and
original cancellation, with native classes, parents, raw qualifiers and exact
original ranges. The affected declarations/type-body regressions and both corpus
oracles passed. That historical qualification observed 45 profiles, 993 queries, 197759 input
bytes, 8238 wire nodes, 151977 wire text bytes and 151997 scalar text bytes;
these are historical observed inventory values. The `tsk-b847` snapshot had169 descriptors after its84 dedicated entries. The current PL/SQL stock has 177 queries: the 172 C8 entries plus five B2 member selectors. Their separate qualification scopes are recorded below. The earlier native
raw qualification passed on Linux (four native tests, two affected inventory
checks and scoped strict Clippy). Dedicated contracts are delivered by `tsk-04a9`. `tsk-1d33`
depends on the actual product `tsk-5cb2`; `tsk-a4cd` also needs `tsk-04a9` before
its full model can be ready. Analysis does not qualify fullCALLS, aggregate
parameters, decoded names, UIDs or graph publication.

Descriptors select explicit native Rule/Terminal paths, preserve the raw keyword and actual ancestor qualifier, Original projections and duplicate-query provenance. Cursor attributes, quantifiers and TIME literal do not become these candidates. JSON_TABLE/XMLTABLE remain raw tabular candidates; XMLTABLE is selected once through its own keyword, without an other_function duplicate. Configured CALLS metadata does not qualify decoded names, semantic CallSites, UIDs or graph publication.

## Delivered grammar and profile routing

The original contradictions now have these real producers and consumer evidence:

| Family | Observed mismatch | Required reconciliation |
|---|---|---|
| PostgreSQL | Routine guards/context selectors passed; the historical three invalid TYPE selectors have been replaced with the actual TYPE query/context, whose focal and corpus qualification passed; body declarations use the PL/pgSQL namespace | Product `tsk-82b9` passed; current S7/concurrency consumes real body variables and decisions with native host contexts. |
| DB2 | The nonexistent `parameter_declaration` descriptor is replaced with nine real header selectors and corrected contexts/comment metadata | Product `tsk-f627` native focal, corpus fixtures and strict Clippy passed; current S7 consumes the full profile. Graph publication remains separate. |
| TSQL | The formal `revoke_permission` producer and actual stock consumer passed their Linux native focal; GRANT regression also passed | Product `tsk-5b74` and current full-profile S7 passed; graph publication remains separate. |
| COBOL | `tsk-0b48` implements explicit preprocessor routing and native COPY observations with original origins and actual program parents; nine main/COPY fixtures plus actual JSON source and affected regressions/Clippy passed on Linux | The three stock copies tag COPY with `source: cobol-preprocessor`; current S7 verifies real COPY observations and native program contexts. Filesystem expansion remains separate. |

COPY observations and copybook filesystem expansion are separate producer
responsibilities. A word scan or an error node cannot stand in for the COPY
syntax tree. Missing rules remain errors; neither dropping capabilities nor treating
invalid selectors as an empty result preserves the capability. Task `tsk-8b3f`
tracks the correction products; `tsk-1d33` now qualifies their current full-profile
configuration and raw observations. Entry counts in the inventory run above are
historical and do not freeze the corrected stock corpus. Open semantic-model
requirements are not discharged by raw occurrences.

## Independent Method parameter correction

`tsk-4eda` implements dedicated Method parameter helpers qualified on Linux:
the focal, eight affected DB2 regressions and strict library/engine-fixture
Clippy passed. An initial unrelated Hub compilation failure before regression
tests remains recorded; its owner corrected the producer before the successful
retry. The IBM LUW 11.5 SQL Reference, edition 2023-02-07, physical
pages 1304–1305 and 1535/1540–1542, permits empty Method lists and optional
parameter names without direction or DEFAULT. LANGUAGE SQL requires names as
a contextual server condition. The signature builtin set includes
BOOLEAN, XML and `[SYSPROC.]DB2SECURITYLABEL`; Method specification parameters
exclude bare XML. Both forms retain nominal distinct/structured types and REF.
Catalog classification and assignment compatibility cannot be inferred from
identifier spelling. Dedicated type attributes accept the lexer's separate
zero/one tokens without widening shared integer or cursor productions.

The new Method specification parameter/list helpers replace only that consumer of `param_decl_list_paren`; `cursor_value_constructor` retains its existing helper. Native capture paths are recorded in the [query contract](rust_ast_antlr_queries.md#db2-method-parameter-paths). Return presence, ordinary types and defaults have independent deliveries below. The complete nine-selector raw metadata consumer is delivered by `tsk-f627`. The `tsk-6eae` MD delta adds an individual native signature parameter context by factoring the exact existing optional-name/type/locator production. It does not add a type alternative or classify a nominal identifier. Methodspec, zero-argument callers, locators, ordinary returns and CAST helpers are retained. A signature name now has this declaration as its direct parent; using the whole list as each parameter's extent would merge distinct slots. Model identity, owner and Original extent validation remain a separate consumer contract. At MD delivery, `tsk-4c23` retained the nominal/REF CAST syntax question. Its current practical REF contract below supersedes positive catalogue-class enumeration as a parser gate and keeps ordinary parameters independent. Its independent ordinary-return contract is now closed: IBM LUW 11.5 SQL Reference physical page 1541 explicitly gives the Method specification return type the same considerations as its parameter type; physical page 1305 requires the signature return to correspond to the specification. `tsk-ab50` implements reuse `method_spec_parameter_type` through a dedicated `method_ordinary_return_type` context in both ordinary positions. Catalog classification remains a server condition. The independent `tsk-e94e` target-attribute composition below has separate primary support. Nominal lexical alternatives remain preserved; the later REF constructor has its own actual product scope, without server type-class enumeration. Signature RETURNS remains optional; each Method specification requires exactly one typed return.

## Independent routine default correction

`tsk-20e3` implements `routine_default_clause` and its native fixture, qualified
on Linux, with a required value in the external scalar parameter consumer.
Aggregate and remaining header families reuse that producer through their own
leaves; PROCEDURE and remaining FUNCTION compositions are described below.
Shared column defaults, local declarations and cursor constructors retain their
existing behavior. The aggregate default consumer remains separate.

The value alternatives are NULL, an already-delivered `constant_`, one
of the eight direct register families with documented aliases, a raw global
variable, or a parenthesized expression. WITH DEFAULT and an empty DEFAULT
value are not routine forms. SYSDATE and LOCALTIMESTAMP permission is explicitly
derived from their timestamp-register identity; it is not server execution
evidence. Dedicated timestamp attributes accept zero/one tokens without
widening shared integer or register productions. The finite X lexical contract
is defined in `tsk-a781` and implemented separately in `tsk-16e5`, whose Linux
SDK focal and affected regressions passed. This does not establish complete
server literal coverage.

Only the main parser grammar and its four official outputs changed. The
single pinned Parser Tool call succeeded without diagnostics; published outputs
match its raw staging bytes. Lexer and Unicode auxiliary outputs remain unchanged.
The focal passed (one test, 0.36 seconds), including native value/name captures,
all direct register families, existing constants, raw global references,
parenthesized expressions, strict negatives, unaffected consumers, raw ranges,
diagnostics and original controls. Nine affected DB2 regressions and strict
library/engine-fixture Clippy passed. Windows/macOS remain unrun.
The full aggregate/default/header/nine-Parameter requirement is
not completed by this independent contract.

## Independent PROCEDURE list correction

`tsk-eb87` implements two external quantifiers and its native fixture, qualified
on Linux. The IBM LUW 11.5 diagrams at physical pages 1324–1326 and 1344–1349 allow
omitted or empty parameter lists and optional directions. External names are
optional; SQL names are required. The SQL prose instructs callers to encode
`()`; the proposed recognizer retains the diagram's omitted-list form.
The leaf changed only two external quantifiers: it permits an empty
`proc_ext_param_list` and makes `in_out_inout` optional in `proc_ext_param`.
SQL wrappers, body/options, types and defaults retain their current behavior.
This shape correction must not be presented as complete header qualification.
One official Parser Tool call succeeded without diagnostics; its four published
outputs match the raw staging bytes. Lexer and Unicode auxiliary outputs remain
unchanged. The focal passed (one test, 0.15 seconds), including both families,
named/unnamed and empty/omitted lists, native name captures, wrapper uniqueness,
raw ranges, strict negatives and original controls. Ten affected DB2 regressions
and strict library/engine-fixture Clippy passed. The first build failed before
tests in unrelated Auth code; its owner corrected it before retry. AST made no
Auth edit. Windows/macOS are unrun.

`tsk-929f` reconciles the actual type gap: external `XML AS CLOB(n[KMG])`.
The existing SQL builtin/alias, nominal and anchored type syntax remains in
`data_type`. `tsk-6eae` already qualified integer0/1 and anchored names with up
to three components, including the optional DATA TYPE, TO and ROW OF markers.
No strict-anchor rewrite or copied Method type enumeration is needed. Nominal
syntax does not classify ARRAY, CURSOR, distinct or ROW by identifier spelling.
Defaults (`tsk-4537`) and nine stock Parameter selectors (`tsk-f627`) are
completed producers with separate evidence. See the
[XML representation contract](rust_ast_antlr_queries.md#db2-external-procedure-xml-representation).

## Planned independent PROCEDURE default integration

`tsk-4537` implements its Source and native fixture, qualified on Linux after
readback and readiness review. It replaces only the two parameter references
to `default_clause` in `proc_ext_param` and `proc_parameter_list_item` with the
delivered `routine_default_clause` from `tsk-20e3`. The IBM LUW 11.5 diagrams at
physical pages 1326 and 1346 require the same five value alternatives: NULL,
constant, special register, global variable or parenthesized expression. The
prose at 1329/1349 permits column-default registers directly and other registers
through an expression. This completed composition is independent of the
external XML representation delivered in `tsk-929f`.

OUT/INOUT and ARRAY/ROW/CURSOR defaults are prohibited contextually by the primary
source. Replacing a value helper does not validate those conditions or nominal
catalog types; full PROCEDURE header qualification still requires that work.
The fixture uses IN or omitted direction and preserves this limit.
Existing lists, names, directions, types, body/options and other default consumers
remain unchanged. Its explicit fixture change adapts only the old header DEFAULT
oracle in `db2_procedure_parameter_lists_native`; the local DECLARE default
counterexample in `db2_routine_defaults_native` kept its then-current assertion.
The later `tsk-6eae` introduces `declare_variable_default_clause` for local
DECLARE. The `tsk-16e5` fixtures now require that helper and its statement parent,
raw `DEFAULT 1`, mandatory value and exact L_ONE token/range, while preserving
DDL `default_clause`, routine queries and the absence of routine-default helpers
inside the local declaration.

The leaf has completed producers `tsk-20e3`, `tsk-eb87` and `tsk-3316`. Its native
value paths are recorded in the
[native query contract](rust_ast_antlr_queries.md#planned-db2-procedure-default-paths).
The focal passed (one test, 0.67 seconds), all eleven affected DB2 regressions
and strict library/engine-fixture Clippy passed. The first compilation failed
before tests because the shared filesystem had no space. Its failure log is
retained; the coordinator recovered inactive duplicate executables and the
normal retry passed. This environment failure required no DB2 Source correction. One official Parser Tool call passed without diagnostics;
four raw Parser outputs match its stage, while Lexer and Unicode auxiliary outputs
remain unchanged. X, full types/headers and
shipped Parameter metadata remain separate required products.

## Independent ALTER TYPE Methodspec wrapper correction

`tsk-c23d` implements only the ADD consumer correction: `ADD METHOD
method_specification` becomes `ADD method_specification`, because the reused
Methodspec already includes METHOD with optional preceding OVERRIDING. The IBM
LUW 11.5 SQL Reference edition 2023-02-07 at physical pages 946–947 composes those
diagrams ambiguously; the ordinary example at 950/printed 918 uses one METHOD.
Official IBM dynamic-dispatch documentation explicitly permits override through
ALTER TYPE and places OVERRIDING before METHOD. ADD OVERRIDING METHOD is the
formal composition of that permission and the diagram, not a literal complete
ALTER-override example located in the reference or a server execution result.

The new fixture covers complete ordinary/override statements, empty/unnamed/raw
UTF8 names, reordered characteristics, scoped captures and direct Methodspec
parents. Strict duplicate/misplaced/missing keyword cases, unaffected ALTER
options and other header/cursor/CALL consumers, raw ranges, SDK recovery and
original controls passed in the final focal on Linux (1/1, 0.10 seconds).
Twelve affected DB2 regressions each passed 1/1; strict Clippy passed in 71.97
seconds. One official Parser Tool call passed in 1.79 seconds without diagnostics,
four raw Parser outputs match its stage, and all twelve Lexer/Unicode auxiliary
outputs remain unchanged. Rustfmt, authored diff checks, documentation whitespace
and 38 local links/anchors passed. Generated whitespace remains the official
Tool output. Windows/macOS remain unrun.

The follow-up `tsk-231a` implements exactly one typed RETURNS per
ordinary/OVERRIDING Methodspec, preserving free segment order. RETURNS NULL ON
NULL INPUT is a characteristic, not the typed return. Its new native
`method_specification_return_clause` Context replaces the old typed alternative
inside `method_specification_seq`, without an old-path alias. Return types and
locators now have that helper as their native parent. Parameter selectors and
the public cxx ABI stay intact. The pending packet was reconciled before Source
at revision 4: its replacement T1 includes the completed wrapper focal among
thirteen affected regressions; the old twelve-filter check is superseded.

## Independent Methodspec typed return

`tsk-231a` requires `seq*`, one typed-return clause, then `seq*`, where the sequence
contains options only. This delivery copied its original type/CAST/locator
content exactly; `tsk-ab50` subsequently composes dedicated ordinary return
types. Full CAST coverage and characteristic uniqueness/exclusivity remain required.
CREATE METHOD signature return remains optional. New native Context, rule name,
return parents and ordinals are exposed directly; no compatibility alias or
normalization hides the tree change. Source uses the mature ANTLR grammar/runtime,
with no keyword-counting parser, Go fallback or shared ABI edit.

The full CREATE TYPE/ALTER TYPE fixture exercises ordinary/OVERRIDING clauses,
option order, named/empty/C unnamed parameters, two Methodspecs each with its own
return, raw return/name/parameter captures and absence of the old return-parent
path. Causal missing/duplicate/dangling returns and characteristic-only RETURNS
NULL are strict negatives. Existing signatures, other ALTER options, scalar
and PROCEDURE defaults, CALL and cursor paths remain isolated. UTF8/CRLF,
nested hidden comments, native ranges/preorder/EOF, SDK recovery and strict
rejection, original cancel/deadline and concurrent operations are covered.
SELF placement is recognition only, with catalog/result-type compatibility
outside this leaf; override composition retains the explicit provenance above.

One official Parser Tool call passed in 1.99 seconds without diagnostics, with
four raw published outputs equal to its stage and twelve Lexer/Unicode outputs
unchanged. The final focal passed on Linux (1/1, 0.19 seconds), and thirteen
affected DB2 regressions each passed 1/1. Strict Clippy passed in 70.07 seconds.
The first focal failed on an incorrectly ordered CREATE METHOD counterexample;
its log is retained, and the fixture now reuses the previously qualified
signature form. Methodsignature Source was not changed to accept that example.
Rustfmt, authored diff checks, documentation whitespace and 39 local links/anchors
passed; generated whitespace remains the raw official output. Windows/macOS
remain unrun.
Full Method options/types/CAST, aggregate, X and all nine shipped Parameter
producers remain required.

## Independent FUNCTION default integration

`tsk-9b9a` changes exactly three default consumer references to the completed
`routine_default_clause` from `tsk-20e3`. External table functions,
OLE DB/sourced/TEMPLATE and SQL functions retain their existing header structure.
At that delivery, `param_decl_3` covered OLE and sourced/template. Current
`tsk-086a` separates the types below without omitting a family. IBM LUW 11.5 physical pages 1202/1205, 1222/1223,
1231/1232 and 1244/1248 share NULL/constant/special-register/global-variable/
parenthesized-expression values and the column-default register reference.

Three native value paths preserve direct value → clause → actual parameter
parents and raw names/order. The new fixture uses five complete templates for
four families, including an OLE character example, the completed value matrix,
external-table unnamed input, multiple defaults, no-default controls, causal
invalid values, isolated old consumers, UTF8/CRLF/comments/ranges/recovery/strict
and original cancellation/deadline/concurrency. The scalar fixture's table
counterexample and the PROC fixture's function counterexamples now expect the
new value context; PROC query matches remain empty, while column/local/cursor
and existing scalar/PROC header defaults retain their own parents.

No type/list/name/direction/locator/return/body/option production is changed.
The reference permits optional OLE/sourced/template names. The defaults leaf
left them required; `tsk-6375` qualified name omission using single-ID and quoted
nominal controls. At that historical delivery, qualified nominal syntax was still
missing. The later `tsk-6eae` producer supplies it; current callers reuse that
completed syntax without classifying catalog types by spelling.
DEFAULT restrictions for ARRAY/ROW/CURSOR and SQL OUT/INOUT, RETURNS ROW and
PREDICATES are contextual residuals, as are assignment and catalog effects.
No forbidden contextual default is used as a valid-SQL positive. Value matrix
recognition does not prove default-to-type assignment or server execution.
Alias and timestamp behavior inherits the explicit `tsk-20e3` provenance.

One official Parser Tool call passed in 17.80 seconds without diagnostics;
raw Parser4 matches its stage and all twelve Lexer/Unicode files are unchanged.
The final focal passed 1/1 in 1.62 seconds; all fourteen affected DB2 regressions
passed (one isolated filter plus thirteen exact libtest filters, 13/13 in 0.67
seconds). Strict Clippy passed in 153.93 seconds. The same affected filters were
grouped to avoid repeated shared-cache acquisition; no broader gate ran. Raw
outputs, format, authored diffs and forty local links/anchors passed; the three
maintained pages match the implemented selectors and their explicit limits.
Windows/macOS remain unrun. Full headers/types/context constraints, aggregate, X and nine shipped
Parameter producers remain required.

## Independent shared FUNCTION optional names

`tsk-6375` consumes the completed defaults composition and changes exactly one
optional marker in the then-shared `param_decl_3`. `tsk-086a` preserves optional
names in both current caller-specific rules;
SQL `param_decl_2` still requires names. Native `parameter_name` children exist
only for named slots. The existing type/default contexts, raw captures, order,
parents and SDK diagnostics are preserved; no fabricated name or normalized tree
is introduced. The three name/type/default selectors are documented in the
[shared FUNCTION contract](rust_ast_antlr_queries.md#db2-shared-function-parameter-names).

Primary IBM 11.5 physical pages 1220/1223 and 1229/1232 support optional names;
1241/1247 retain SQL required names. OLE fixtures use existing character/graphic
types; sourced/TEMPLATE nominal controls are single-ID or quoted, with catalog
classification and assignment outside parser proof. At this delivery,
zero-argument headers remained a gap; `tsk-a72c` subsequently supplies the
independent shared-header arity delta below. Qualified nominal types, full
context constraints, aggregate, X and nine shipped Parameter producers remain
required.

One official Parser Tool call passed in 2.28 seconds without diagnostics, raw
Parser4 matches its stage and Lexer/Unicode12 are unchanged. The focal passed
1/1 in 0.47 seconds, fifteen exact affected DB2 filters passed 15/15 in 1.67
seconds, and strict Clippy passed in 110.26 seconds. Raw outputs, format, authored
diffs and forty-three local links/anchors passed; the three maintained pages
match the optional native name/type/default selectors and explicit limits.
Linux qualification ran; Windows/macOS are unrun.

## Independent Method ordinary return types

`tsk-ab50` introduces `method_ordinary_return_type`, which consumes the mature
`method_spec_parameter_type` delivered by `tsk-4eda`. Exactly two ordinary
references change: optional Methodsignature RETURNS and the mandatory typed
Methodspec clause. At this delivery both CAST branches, their locators and
shared type rules were unchanged. The independent target composition in
`tsk-e94e`, described below, subsequently adds inferred attributes. The native Context/getters/ordinals expose this composition
directly; the public Rust/cxx ABI is unchanged.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1541 explicitly applies
Method parameter type considerations to ordinary Methodspec returns. Physical
page 1305 requires the signature return to correspond to Methodspec. This
explicit relationship supports sharing the non-XML builtin, nominal and REF
domain. Names may be unqualified or schema qualified, including quoted UTF8;
quoted "XML" is nominal syntax while bare XML is excluded. Mature builtin
attributes preserve numeric, length/unit, LOB size, BOOLEAN and security-label
forms. Empty ordinary attributes and malformed REF are strict syntax negatives.
Catalog classes, existence, correspondence, host mappings and conversions remain
server conditions; identifier spelling is not used to classify them.

Complete CREATE METHOD, CREATE TYPE and ALTER TYPE fixtures exercise ordered
raw type captures, named/empty parameters, Methodspec option ordering, locators,
overrides and two Methodspecs. CLOB AS LOCATOR uses Methodspec LANGUAGE C NOT
FENCED as the formal control. Signature omission remains valid, while absent
Methodspec returns, RETURNS without a type and duplicate typed clauses fail.
CAST controls preserve existing trees and do not qualify new CAST nominal/REF
forms. Parameter XML restrictions remain unchanged. Other headers, raw UTF8/CRLF,
nested hidden comments, token text/EOF, parents, SDK recovery versus strict
errors, original cancellation/deadline and concurrent parses are covered.

[Native return selectors](rust_ast_antlr_queries.md#db2-method-ordinary-return-paths)
remain separate from the shipped Parameter consumer. `tsk-4c23` still requires
primary positive type enumerations for CAST target/source; this ordinary leaf
cannot close it. Aggregate/options, X and stock metadata remain required.

Qualification passed on Linux: the new focal passed 1/1 in 1.45 seconds;
sixteen exact affected DB2 regressions passed 16/16 in 1.76 seconds; strict
Clippy passed in 83.60 seconds. Official Parser Tool 4.13.2 passed once in 2.01
seconds without diagnostics. Raw Parser4 matches its stage and twelve
Lexer/Unicode outputs remain unchanged. Rustfmt, authored checks, documentation
whitespace and 47 local links/anchors passed. Windows/macOS remain unrun.

## Independent Method CAST target attributes

`tsk-e94e` has a closed syntax delta separate from full CAST type enumeration.
IBM LUW 11.5 SQL Reference, edition 2023-02-07, physical page 1542 (printed 1510)
authorizes inferred attributes for parametrizable target types and explicitly
excludes FLOAT(). VARCHAR() is its literal example. All other empty-attribute
forms below derive from that quantified rule and the corresponding builtin
syntax; they are not executed IBM examples or server castability evidence.

| Existing inferred family | Primary physical pages defining parametrizable builtin |
| --- | --- |
| DEC/DECIMAL/NUMERIC/NUM | 1388, 1397 |
| DECFLOAT | 1388, 1398 |
| CHAR/CHARACTER/VARYING/VARCHAR, optional FOR BIT DATA | 1388, 1398 |
| CLOB/CHAR/CHARACTER LARGE OBJECT | 1388, 1399 |
| GRAPHIC/VARGRAPHIC/DBCLOB | 1388, 1399–1400 |
| NATIONAL/NCHAR/NVARCHAR/NCLOB variants | 1388, 1400 |
| BINARY/VARBINARY/BLOB/LARGE OBJECT | 1388, 1400–1401 |
| TIMESTAMP | 1388, 1401 |

Signature target and Methodspec target correspondence is supported by physical
pages 1304–1305 and 1542. The shared helper is mature `tsk-1787` Source; global
nonempty builtin forms prove preservation, not SQL authority. Only the two
target references change. Both sources, ordinary `tsk-ab50` returns, parameters,
cardinality and locator placement remain unchanged. `data_type_3` has an active
external scalar consumer and was preserved by this Method producer. Its later
inferred-attribute composition belongs to tsk-8187; no unused alias was introduced.

The existing target domain stays opaque. At this producer's delivery, nominal/REF
source syntax was still separate. General cast text on physical pages 80–81 is
not a per-slot catalogue enumeration; Root's current practical syntax scope does
not require that server proof. The later XML and REF units below have separate
product evidence. No identifier spelling guard or server classification is fabricated.

Thirty-seven inferred/nonempty pairs across three complete callers and two
parameter lists produce 444 cases. New inferred types exclude nonparametrizable
forms and FLOAT(); partially empty/repeated attributes and source inference
remain strict negatives. Raw targets and source parents, unchanged ordinary
selectors and real parameter names are checked with the
[native target paths](rust_ast_antlr_queries.md#db2-method-cast-target-paths).
Other qualified headers and original operation controls remain isolated.

Parser Tool passed once (2.01 seconds, no diagnostics); raw Parser4 and unchanged
Lexer/Unicode12 are confirmed. The new focal passed 1/1 in 2.05 seconds;
seventeen exact affected regressions passed 17/17 in 2.10 seconds and
strict Clippy passed in 97.19 seconds. Format, authored diffs,
documentation whitespace and local links/anchors passed. The
[recognizer receipt](rust_ast_antlr.md#db2-method-cast-target-inferred-attributes)
preserves the separate initial resource interruption and passing retry.
Windows/macOS are unrun.
Aggregate/options, X and all nine shipped Parameter producers remain required.

### Explicit XML correction and remaining nominal syntax

The XML implementation in tsk-4c23 specifies rejection of the XML builtin keyword on both sides of Method RETURNS CAST in all three callers. The [CREATE METHOD rule](https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-create-method) excludes XML from methods; the [CREATE TYPE statement](https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-create-type-structured) explicitly excludes XML in its CAST target and source. The global generic datatype helpers include XML; Method CAST now uses dedicated helpers that exclude it. The delta preserves other existing opaque shapes and treats a quoted identifier named XML as a nominal name rather than a builtin. The superseded packet tsk-3397 was folded into the existing refinement task under Root coordination. The initial native baseline confirmed incorrect XML acceptance in the four slots.

The complementary [IBM Db2 V11.5 Routines manual](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_dev_routines_115.pdf),
edition 2020-08-21, physical pages 147/156 (printed 135/144), connects the format
of a method result to CAST FROM. Page 155/143 permits structured results with
appropriate transforms. These passages were read as complete rendered pages.
They do not enumerate REF or qualified nominal syntax for each Method CAST
slot. The statement-specific restrictions still override general host-mapping
tables. Full nominal/REF refinement tsk-4c23 remains necessary; neither the
XML packet nor the delivered inference leaf closes it.

## Independent shared FUNCTION zero arguments

`tsk-a72c` composes mature ANTLR optional references into exactly two callers:
`create_function_old_db_external_function_statement` and
`create_function_sourced_or_template_statement`. Originally both used
`param_decl_list_3?`; `tsk-086a` keeps OLE there and uses
`sourced_function_param_decl_list?` for sourced/TEMPLATE. Both nonempty lists
require slots with optional names and existing defaults. OLE DB and
sourced/TEMPLATE are three complete statement families served by these callers.

Primary IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1223, permits zero
OLE DB parameters with mandatory parentheses despite the page-1220 diagram's
nonempty list. That divergence is explicit. Sourced/TEMPLATE page-1229 diagrams
have a bypass and page-1231 text allows zero while requiring parentheses. No
server example or provider/catalog classification is inferred from these rules.

Empty headers have no list or slot and produce zero captures for the existing
name/type/default selectors. Nonempty controls preserve raw values and native
parents, including unnamed slots. Hidden comments inside otherwise empty
parentheses remain native tokens with original UTF-8/CRLF ranges; they do not
become declarations. The complete fixture also covers malformed lists/defaults,
mandatory parentheses, other-header isolation and original operation controls.
The only older oracle adjustment removes `tsk-6375`'s empty-list negative and its
historical gap comment. All other old negatives remain unchanged.

The baseline comparison requires the whole grammar to match after reversing
only the two optional markers; twelve Lexer/Unicode outputs remain unchanged.
One official Parser Tool 4.13.2 invocation passed in 2.10 seconds without
diagnostics; raw Parser4 matches its stage. The focal passed 1/1 in 0.19 seconds;
eighteen exact affected DB2 regressions passed 18/18 in 2.37 seconds and strict
Clippy passed in 50.86 seconds. Format, authored differences,
documentation whitespace and local links/anchors passed. The
[recognizer receipt](rust_ast_antlr.md#db2-shared-function-zero-arguments)
preserves the initial fixture failure, correction and log provenance separately.
Windows/macOS are unrun. This delta does not close qualified nominal types,
contextual constraints, other FUNCTION arities, aggregate, X or shipped metadata.
See the [native query paths](rust_ast_antlr_queries.md#db2-shared-function-zero-argument-paths).

## Independent external-table FUNCTION zero arguments

`tsk-c72f` changes only the caller reference to `ext_table_param_decl_list?` in
`create_function_external_table_statement`. The list and its optional-name,
type/default/locator slots are literal existing Source. Empty headers have no
native list or declaration and zero anchored parameter captures; the unchanged
getter can be absent. Return-table columns and options remain native nodes.

Primary IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1199 (printed 1167),
contains the list bypass; physical page 1204 (printed 1172) allows zero parameters
and requires parentheses. Type/default pages 1200/1202 are separate contracts,
and this delta does not qualify all server/contextual restrictions. Complete
fixtures use the mature external-table C factory and optional-name/default
behavior from `tsk-9b9a`; no new nominal classification or locator rule is added.

The whole-grammar baseline must match after reversing the one optional marker.
A separate complete fixture checks empty/comment-only parentheses and preserved
named/unnamed/mixed nonempty captures, parents/order, mandatory parentheses,
malformed lists/defaults, other-header isolation, UTF-8/CRLF/raw tokens/EOF,
recovery/strict errors and original operation/concurrency controls. All previous
fixtures must match their pre-edit baseline; no older oracle changes are needed.

One official Parser Tool 4.13.2 invocation passed in 2.30 seconds without
diagnostics; raw Parser4 matches its stage and twelve Lexer/Unicode outputs are
unchanged. The focal passed 1/1 in 0.18 seconds; nineteen exact affected DB2
regressions passed 19/19 in 2.44 seconds and strict Clippy passed in
230.31 seconds. Format, authored differences, documentation
whitespace and local links/anchors passed. Windows/macOS are unrun. Full types,
returns/options, aggregate, X and nine shipped
Parameter producers remain necessary. See the
[native selector contract](rust_ast_antlr_queries.md#db2-external-table-zero-argument-paths).

The initial `tsk-c72f` focal ended 101 before tests (36.51 seconds) because
`native/icebug_query.cpp` could not include `json.hpp` in the separately owned
query/build consumer. Its result and full log are preserved under
`.build/ast-db2-external-table-function-zero-arguments-initial-build-fail-provenance.json`.
That failure is historical and exercised no parser assertion. The owned build
seam was corrected before the actual successful focal and affected regressions
reported above; parser grammar and Tool output were not regenerated.
The initial strict Clippy ended 101 in 88.52 seconds on `manual_is_multiple_of`
in the separately owned `icebug_query.rs:269`. Its original result and full log
remain under `.build/ast-db2-external-table-function-zero-arguments-clippy-result.json`.
The owner corrected that lint; the strict retry reported above passed at
separate `clippy-final` paths. The nineteen-filter Cargo duration includes an observed wait
on the shared build-directory lock.

## External scalar parameter type composition

`tsk-1c45` composes the two confirmed parameter constructor gaps in one
external scalar caller: `method_reference_type` for REF and the mature
`procedure_external_xml_type` for XML AS CLOB(n[KMG]). Both helpers remain
unchanged. The existing `data_type` branch, names/direction grouping, defaults
and locator positions remain direct. No new rule, enum, union or nominal
classifier is introduced.

IBM Db2 LUW11.5 SQL Reference physical pages1173/1179 describes these parameter
forms. The REF helper from `tsk-4c23` retains one- or two-component referents;
the XML helper from `tsk-929f` retains native `integer_value` and optional
`kmg`. That helper's historical PROCEDURE name does not restrict its syntax.
Existing builtins and opaque nominal names cover the other confirmed domains.
The preserved bare XML branch does not establish server representation legality.
This closes the concrete residual set in this slot, without a universal SQL audit.

Named slots declare Parameters; anonymous REF/XML slots and referenced types
do not create names or Type declarations. Locator fixtures use a separate CLOB
slot with the qualified NOT FENCED/NO SQL envelope. LANGUAGE, host assignment,
catalog classes, transforms and locator/default legality remain contextual.
Ordinary returns, CAST positions and other routine callers retain their contracts.

The [native type paths](rust_ast_antlr_queries.md#db2-external-scalar-parameter-type-paths) retain raw quotes/UTF8/CRLF/comments, native parents and exact Original ranges. The real stock consumer observes declared names and original operation causes. Windows/macOS and Db2 server execution are unrun.

## External scalar CAST target attribute inference

`tsk-8187` adds only `inferred_builtin_type` to the existing `data_type_3`
CAST target. The mature producer `tsk-1787` and its Method composition
`tsk-e94e` remain separate evidence; no enum or parallel helper is copied.
Explicit targets retain `data_type_3/data_type`. At that delivery, ordinary
returns and CAST sources used `data_type_2`; subsequent `tsk-2526` composes the
ordinary return separately below. CAST source, parameters, defaults and locator
positions remain unchanged. The manual's data-type4 is not a G4 `data_type_4` rule.

IBM Db2 LUW11.5 SQL Reference physical pages1173–1181, particularly1180,
permit empty attribute parentheses only in the CAST target and exclude
FLOAT(). The syntax retains original case, comments and byte ranges. It does
not calculate attributes, resolve nominal classes or validate server
castability. AS LOCATOR remains attached to the source, with its existing
contextual language/LOB/FENCED/NO SQL considerations.

## SQL FUNCTION parameter and scalar type composition

`tsk-80fb` changes only `param_decl_2` and the scalar RETURNS branch of
`create_function_sql_scalar_table_or_row_statement` to the existing
`data_type_1` helper. Parameters now have explicit REF syntax; scalar returns
use the general builtin, anchored and nominal family instead of the narrow
`data_type_2` helper. The latter remains unchanged for its other callers,
including ARRAY indexes. SQL ROW/TABLE column helpers from `tsk-1b14` remain
separate. Names, defaults, optional lists and the nine stock Parameter paths
keep their direct callers. No old `data_type`/`data_type_2` compatibility child
is synthesized for these two positions.

IBM Db2 LUW11.5 SQL Reference physical pages1241–1249 share the parameter
and scalar-return family; the RETURNS prose refers to the same considerations.
The existing helper was qualified for variables in `tsk-6eae`; that result
does not qualify these FUNCTION callers. REF uses the existing `type_name`
constructor; other nominal branches retain their existing qualified syntax.
Catalog classes, assignments and server body mode are not inferred from a name
or from an ANTLR compound-rule label. BOOLEAN/ARRAY/CURSOR/ROW outputs require
compound SQL (compiled) at the server; parser acceptance is not that execution.

The new Rust SDK fixture is `tests/db2_sql_function_types.rs`. The zero-argument
control retains all inputs and oracles except its two necessary type locators.
See the [native type paths](rust_ast_antlr_queries.md#db2-sql-function-parameter-and-scalar-types).
Root31zn passed the four new SDK cases and the stock header metadata case;
Root31zh passed the zero-argument and typed-return controls. These seven cases
are current FUNCTION evidence. Root31zs passed scoped AST Clippy after the
coalesced attempt stopped in another owner's Hub code; the seven SDK cases
were reused. Windows/macOS and Db2 server execution are unrun.

## Independent SQL FUNCTION zero arguments

`tsk-106e` changes only the caller reference to `param_decl_list_2?` in
`create_function_sql_scalar_table_or_row_statement`. The list remains nonempty
when present; its optional direction, mandatory name, type and default slot is
literal existing Source. The unchanged list getter can be absent. Empty headers
produce no list/declaration or anchored parameter captures; scalar return type
and SQL body remain native nodes. Required SQL names are not copied from the
separately qualified optional-name external-table/shared consumers.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1241 (printed 1209), shows the
list bypass; page 1246 (printed 1214) permits zero with mandatory parentheses and
page 1247 preserves names in nonempty slots. This is an independent arity delta,
not proof of SQL typed ROW/TABLE returns, full type domains, options or server
semantics. The scalar factory and shared default helpers reuse `tsk-9b9a`.

The whole-grammar baseline must match after reversing exactly one optional
marker. All older fixtures must match their full pre-edit prefix literally.
The new fixture covers empty/comment-only and named nonempty headers, optional
IN, default/raw captures/parents/order, mandatory parentheses, malformed lists
and defaults, unnamed SQL negatives, qualified unnamed external-table isolation,
other headers, UTF-8/CRLF/comments/raw tokens/EOF, recovery/strict errors and
original operation/concurrency controls. No older oracle needs alteration.

One official Parser Tool 4.13.2 invocation passed in 2.50 seconds without
diagnostics; raw Parser4 matches its stage and twelve Lexer/Unicode outputs are
unchanged. The focal passed 1/1 in 0.29 seconds; twenty exact existing affected
DB2 regressions passed 20/20 in 2.27 seconds and strict Clippy passed in
65.12 seconds. Format, authored differences, documentation
whitespace and local links/anchors passed.
Windows/macOS are unrun. Full scalar/parameter types and contextual SQL ROW/TABLE returns/options,
aggregate, X and nine shipped Parameter producers remain necessary. External
scalar arity is qualified by the independent delivery below. See the
[native selector contract](rust_ast_antlr_queries.md#db2-sql-zero-argument-paths).

## Independent external scalar FUNCTION zero arguments

`tsk-e3df` changes exactly the existing inline group to
`'(' (ext_scalar_param_decl (',' ext_scalar_param_decl)*)? ')'` in
`create_function_external_scalar_statement`. The slot remains literal, including
optional names/direction, mandatory type and optional default. The active scalar
`data_type_3` CAST return branch and all other headers are preserved. The unchanged
declaration-vector getter is empty, with no synthetic slot or list context.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1172 (printed 1140), has the
group bypass; page 1178 (printed 1146) permits zero with mandatory parentheses.
The qualified full scalar factory and mature default helpers reuse `tsk-20e3`.
This independent arity delta does not qualify full types, return CAST/locator
contexts, options, provider/catalog or server behavior.

The whole grammar equals its fresh previous baseline after reversing only this
group. All older fixtures preserve their full pre-edit prefix. The new fixture
checks empty/comment-only headers, named/unnamed and mixed nonempty declarations,
Q0 default/Q1 name/Q2 anchored type raw captures/parents/order, malformed lists
and defaults, qualified header isolation, original raw spans/comments/EOF,
recovery/strict errors and operation/concurrency controls. Nominal unnamed types
are not rejected as missing names. No older oracle requires alteration.

One official Parser Tool 4.13.2 invocation passed in 24.21 seconds
without diagnostics; raw Parser4 matches its stage and Lexer/Unicode12 remains
unchanged. The focal passed 1/1 in 0.18 seconds (Cargo 284.21 seconds);
twenty-one exact existing affected DB2 filters passed 21/21 in 2.66 seconds
(Cargo 54.56 seconds). Strict Clippy passed in 94.87 seconds.
Cargo durations include observed shared build-directory lock waits. Format,
authored differences, documentation whitespace and local links/anchors passed.
Windows/macOS are unrun. Full contextual types/returns/options, aggregate,
X and nine shipped Parameter producers remain necessary. See the
[selector contract](rust_ast_antlr_queries.md#db2-external-scalar-zero-argument-paths).

## Independent external scalar parameter locator

`tsk-ddbd` composes the mature `as_locator` helper only as the final optional
suffix of `ext_scalar_param_decl`, after the optional routine default. Whole-G4
equality after reversing that one suffix preserves the preceding name/direction,
type/default grammar, delivered empty outer header, active `data_type_3` CAST
return, other headers and existing locator consumers. All older fixtures retain
their full pre-edit prefix; no legacy alias or permissive fallback is added.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1173 (printed 1141), gives
the modifier position; page 1180 (printed 1148) gives LOB/distinct-over-LOB and
FENCED plus NO SQL context rules. Parser recognition is separate from contextual
semantic legality: no manual type guard or nominal-spelling classifier exists.
The complete positives use IN or implicit IN, LOB types, NOT FENCED and no
PREDICATES. Provider/catalog, server execution, default assignment and full
contextual type/options qualification remain necessary.

The new fixture exercises named/unnamed/defaulted and mixed locator slots,
quoted UTF-8 names, raw AS/comment/LOCATOR ranges and direct slot parents,
Q0/Q1/Q2 conservation and Q3 absence without the modifier or in an empty header.
Qualified Method return locators and other headers are isolated. Wrong order,
repetition and incomplete modifiers, original raw tokens/EOF and recovery/strict
operation/concurrency controls are checked without changing old oracles.

One official Parser Tool 4.13.2 invocation passed in 2.09 seconds
without diagnostics; four raw Parser outputs match their stage and twelve
Lexer/Unicode outputs are unchanged. The new local getter composes an existing
context; no new rule ordinal or public Rust/cxx ABI is needed. The focal passed 1/1 in 0.18 seconds (Cargo 449.34 seconds);
twenty-two exact existing affected DB2 filters passed 22/22 in 2.55 seconds
(Cargo 131.48 seconds). Strict Clippy passed in 63.82 seconds.
Cargo durations include any observed shared build-directory lock waits. Format,
authored differences, documentation whitespace and local links/anchors passed.
Windows/macOS are unrun. Full contextual
types/returns/options, aggregate, X and nine shipped Parameter producers remain
necessary. See the [selector contract](rust_ast_antlr_queries.md#db2-external-scalar-parameter-locator-path).

## Independent SQL typed return columns

`tsk-1b14` introduces six consumed SQL return helpers and changes only the
caller references for ROW and typed TABLE columns. TABLE has at least one
name/type column; ROW has at least two. Parentheses are mandatory. The existing
names-only `column_name_list` has no parentheses and remains unchanged for its
other consumers. Scalar and other TABLE returns, SQL header parameters/defaults,
empty arity, options and body grammar are preserved.

IBM LUW 11.5 SQL Reference (2023-02-07), physical pages 1243–1244, supplies the
complete syntactic binding: documented builtins, distinct/structured nominal
names and REF. Page 1249 supplies cardinalities; page 1248 covers schema/SQL path.
The completed `tsk-4eda` builtin/name/0–1 attribute producers are reused by local
SQL wrappers. XML and native CURSOR follow the SQL diagram, without transferring
Methodspec exclusions. No generic type union, inferred empty attributes or
CREATE TABLE helper is substituted. Nominal type classes and transform/server
restrictions are not resolved by identifier spelling or grammar alternative.

Source equality after removing exactly six helpers and reverting the two caller
references proves conservation of the fresh previous grammar. The new fixture
uses actual RETURN fullselect bodies, catalog preconditions for nominal/alias
controls and a structured-parameter/transform precondition for ROW. It checks
ordered name/type captures and header-parameter isolation, complete builtin
shapes from `tsk-4eda` plus XML/CURSOR, qualified nominal/REF names, raw comments,
cardinalities, causal syntax errors, recovery/strict behavior and operations.
All older fixtures retain their entire pre-edit prefix.

One official Parser Tool 4.13.2 invocation passed in 2.06 seconds
without diagnostics; raw Parser4 matches its stage and Lexer/Unicode12 remains
unchanged. Private native contexts/getters can be added and rule ordinals
renumbered by the official Tool; public Rust/cxx ABI and shared interfaces do not
change. The corrected focal passed 1/1 in 1.50 seconds (Cargo 5.0 seconds);
twenty-three exact existing affected DB2 filters passed 23/23 in 2.24 seconds
(Cargo 2.58 seconds). Strict Clippy passed in 97.03 seconds.
Format, authored differences, documentation whitespace and local links/anchors
passed. The initial qualified-table failure and its full log remain preserved.
Windows/macOS are unrun. Full scalar/parameter types, contextual return/transform/options,
aggregate, X and nine shipped Parameter producers remain necessary. See the
[selector contract](rust_ast_antlr_queries.md#db2-sql-typed-return-column-paths).

The initial focal failed at FROM SYSIBM.SYSDUMMY1 (101, 0.02 seconds; Cargo
92.56 seconds). The preserved initial full log records the qualified-table
limitation. Primary page 1255 uses unqualified EMPLOYEE and READS SQL DATA; the
corrected derived controls use SRC_DUMMY_F with one-row catalog precondition
and READS SQL DATA, including ROW controls that read a table. No table-name,
fullselect or direct-RETURN grammar was changed by that unit. Those controls
did not qualify QName or VALUES/RETURN. Ordinary FROM QName is addressed by
`tsk-6012` below; the independent `tsk-cf2d` contract is implemented by `tsk-7f03`.

## Ordinary FROM qualified reference binding

`tsk-3f37` and `tsk-6012` address the qualified FROM name that exposed the
initial `tsk-1b14` failure. IBM LUW 11.5
[SQL Reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_reference_115.pdf),
printed pages 4–10 and 649–650 (physical pages plus 32), distinguishes simple,
local schema-qualified and remote data-source/schema/object names. Remote
three-part ordinary FROM references are supported; this is unrelated to z/OS
location qualifiers or routine modules.

The caller-local binding is `(table_reference_schema '.')? table_name` or
`data_source_name '.' table_reference_schema '.' table_name`. Its schema component
reuses `schema_name` or admits the documented SESSION schema token. Shared name rules remain
single identifiers, preserving unrelated DDL/DML, ONLY/OUTER/ANALYZE and
CREATE NICKNAME/optimization-profile consumers. Existing FROM suffixes stay
in their caller. The official Tool adds private getters without a Rust/cxx ABI
change.

Raw QName/component selectors preserve bytes, case, quotes/doubled quotes,
dots inside identifiers, hidden comments and UTF-8 ranges. Actual Rust nodes
expose `span`, `status` and `parent`, with ordinary ranges represented by
`SpanStatus::Regular`. The shipped descendant SELECTS query remains a
final-name consumer; qualified relationship identity needs consumer integration.
Catalog existence, table/view/nickname/alias/CTE classification and implicit
schema resolution are not determined by spelling.

The complete typed TABLE SQL FUNCTION uses inlined
`BEGIN ATOMIC RETURN SELECT P_F FROM SYSIBM.SYSDUMMY1; END` through the existing
compound/RETURN/subselect route. It needs no direct-RETURN/VALUES dependency.
Malformed qualifiers must produce diagnostics with valid partial ranges.
The initial `tsk-1b14` failure remains failure evidence, and its unqualified
typed-return controls remain legitimate coverage. Necessary `tsk-cf2d` work is
independent. See [selectors](rust_ast_antlr_queries.md#db2-from-qname-selectors)
and [affected checks](rust_ast_antlr.md#db2-ordinary-from-table-qnames).

Qualification passed four affected ANTLR functions, scoped strict Clippy and
the single source-identity regression for the finite Original fixture. The
official Parser4 outputs equal the current generation stage, Lexer/Unicode12
and the old ANTLR test prefix are preserved, and reverting only the two local
helpers and caller returns the grammar to its pre-edit source. Historical
failed attempts remain failures; five distinct test functions passed.

## VALUES and direct RETURN binding

The `tsk-cf2d` contract is implemented by `tsk-7f03` after the actual
shared-output producer `tsk-6012`; `tsk-3f37` is QName analysis provenance.
That ordering is ownership coordination, not an extra functional dependency.
Only five narrow grammar changes are made: separate VALUES from grouped
fullselect in the initial alternatives, route the SQL body through existing
RETURN statement, add the missing ORDER BY prefix to its existing rule, and
require the CTE definition's fullselect with its optional nested WITH list, and
add the CREATE VIEW definition's mandatory AS token.

The current QName/SESSION and typed-return helpers, global RETURN/VALUES/
expression semantics, lexer, profiles and Rust/cxx ABI are preserved.
New raw captures use actual nodes and spans. Fullselect grouping, rows and set
operands remain tree/source ordered; inherited body/parameter/column parents
remain distinct. Existing SELECT and DELETE callers verify the shared order
prefix; no sort, pagination or isolation helper is broadened.

Syntax does not prove catalog existence, assignability, row/column cardinality,
structured observers/transforms or access-class legality. The ROW example
assumes a structured TYPE_F with compatible observer fields and transform;
the SELECT example assumes the referenced source table. Bare RETURN and
semantic-only restrictions are not fabricated parser negatives, and explicit
ROW constructors are not added.
The first executed focal exposed the old CTE helper's missing fullselect at
`AS (VALUES ...)`. The original case is retained. IBM LUW 11.5 physical page 748
confirms a mandatory fullselect and nested WITH for Mod Pack 2 and later; the
correction reuses the existing list and adds an empty-definition negative.
The next focal reached the required VALUES view and exposed the missing AS in
its existing caller. Physical page 1572 confirms the token. The VALUES fixture
is preserved with a SELECT control and a missing-AS negative.

Qualification uses only the new focal, affected existing function/procedure/
Original operations, current QName and general fullselect regressions, scoped
strict Clippy and local format/generation/document checks. No full-suite
campaign, compatibility, quota/resource wrapper or Go comparison is required.
Source inspection's finite Original fixture belongs to the previous delivery
and is not rewritten here. See
[queries](rust_ast_antlr_queries.md#db2-direct-return-and-values-selectors)
and [recognizer contract](rust_ast_antlr.md#db2-values-fullselect-and-direct-sql-return).

The final focal and eight affected regressions passed on Linux with all five
changes. Scoped strict Clippy and official Parser4 generation/equality passed;
format, authored whitespace and links are consistent with the current source.
The first two executed attempts failed on the CTE and view omissions described
above. Those failures remain history, and their fixtures passed after the minimal
corrections. Windows/macOS and SQL server semantics are unqualified.

## Db2 aggregate and shipped header consumers

Before product `tsk-2558`, the active aggregate rule contained TODO alternatives,
accepted only one of RETURNS or AGGREGATE, and required a nonempty parameter list
and state names beyond the primary syntax. Its Source now follows the closed
LUW 11.5 contract: both segments in either order, optional parameters and state
names, the aggregate builtin subset, dedicated routine defaults, and eight
option families without repetition. A recognition-local ANTLR field and
predicates handle option cardinality, with no global state or resource accounting.
The Linux focal passed, including resetting option cardinality between two
definitions. Nine affected native regressions, strict library/engine-fixture
Clippy, official Parser output equality and authorial formatting checks passed.
The initial dependency-resolver failure remains history. Windows/macOS and SQL
server semantics remain unqualified; this native product does not qualify S7.

Product `tsk-f627` then corrects the three shipped Db2 profile copies. It uses
nine actual routine-header Parameter paths, real SQL_COMMENT/LINE_COMMENT
metadata, aggregate and method contexts, and header-anchored Function/Procedure
declarations. SOURCE, FINALIZE and calls remain references rather than extra
declarations. These descriptors and contexts are implemented in all three stock
copies. The new Db2 fixture compiles every configured query/capture/context and
checks real header parameters, unnamed and empty lists, method/aggregate names,
call/reference isolation, cursor exclusions, raw bytes and the original request.
The Linux focal, both affected corpus fixtures and typed query-error regression
and strict Clippy passed. Earlier fixture, compile and dependency-warning
failures remain recorded. Native aggregate qualification remains separate. Corpus tests preserve query patterns
and source sites from their current
YAML, with inventory/routing checks retained, rather than frozen byte/node/query
totals. Structural input safeguards and custom-query multiplicity remain intact.

The generic consumer in `tsk-1d33` is already implemented. Its Db2 prerequisite
is now the actual profile product, which depends on the native aggregate and
completed header helpers, rather than analysis `tsk-0b99`. The current
`tsk-1d33` S7 and concurrent full-profile fixtures pass with TSQL REVOKE vocabulary,
preprocessor COPY and PL/pgSQL body composition from those real products.
Method CAST is delivered in `tsk-4c23`; the bounded external XML representation
is in `tsk-929f`. These results retain their own fixtures and evidence and do
not authorize guessed catalog type domains or server-validation claims.

See the [header selectors](rust_ast_antlr_queries.md#db2-header-parameter-selectors).

## PostgreSQL body profile composition

The specific consumer `tsk-82b9` uses the qualified host-reuse and prepared-profile
APIs. Its native fixture executes the full stock PostgreSQL host plus stock PL/pgSQL
queries over two homonymous Function/Procedure bodies. Variable captures and raw
PL/pgSQL decision observations retain separate body tree addresses and link to host
contexts by native parent chains. The invalid SQL-host variable descriptor and
PL/pgSQL complexity kinds were removed from all three PostgreSQL stock copies;
the PL/pgSQL profile continues to provide both capabilities.

See [composition contract](rust_ast_antlr_semantics.md#postgresql-host-and-plpgsql-body-composition)
and [prepared body queries](rust_tree_sitter_profile_matches.md#prepared-queries-over-retained-postgresql-bodies).
This unit does not qualify full S7 across every language, the graph assembler,
resolved graph owners, final metrics, UIDs or publication. Linux fixtures are real;
Windows/macOS execution remains unrun.

## COBOL explicit COPY consumer

The stock COPY descriptor retains `imports`, `REFERENCES`, the selected raw name
and its genuine `//copyStatement/copySource` pattern. It now explicitly selects
`source: cobol-preprocessor` in all three active copies. Other COBOL queries and
context maps continue against the main parser. The actual S7 compiler routes each
query to its declared tree and the COBOL semantic branch consumes both namespaces.
No rule error is retried against another grammar or treated as an empty match.

See the [consumer and scope contract](rust_ast_antlr_semantics.md#cobol-copy-observations-and-native-program-scopes)
and [SDK entry](rust_ast_antlr_queries.md#cobol-preprocessor-query-tree).
Source, C1–8 and a strict main directive fixture are implemented. The first build
failed on external Data compilation; later real scope failures exposed the missing
main producer and retained all required positive fixtures. The
[formal main directive producer](rust_ast_antlr.md#cobol-native-copy-directive-parents)
passed nine real Linux fixtures. Causal config/capture regressions and current
scoped Clippy also passed. COPY observation is not filesystem expansion or REPLACE
execution, and this qualification does not cover full S7.

## Current full-profile raw consumer qualification

`tsk-1d33` preserves every configured pattern/capture/context compilation check
and observes the five current full stock profiles without freezing entry counts.
The actual PostgreSQL host plus maintained PL/pgSQL fixture uses the mature SDK
loader; its Variable captures and IF decisions retain body node addresses and
actual host parent contexts. The actual COBOL COPY observation retains explicit
preprocessor query provenance and its own syntax ordinal, with a main-token link
to its actual program parent. Primary occurrences keep their effective Query
references, raw captures and context ordinals.

The Linux S7 fixture passed 1/1. Fourteen affected semantic fixtures passed,
including five concurrent dialect consumers using those same real namespace
assertions; two further Unicode/recovery and PL/SQL-call fixtures passed. Scoped strict Clippy and Source/documentation checks are recorded in
`.build/ast-semantic-s7/qualification-current.json`. Earlier failed inventory runs
remain historical; unchanged qualified provider/regression evidence is reused.
No grammar or stock profile changed in this consumer qualification. This does not
claim graph assembly/publication, resolved owners, UIDs, final metrics, COPY
filesystem expansion/REPLACE execution or Windows/macOS execution.

The current TSQL S7/concurrent fixture also executes the qualified ordinary
`REVOKE SELECT ON OBJECT::dbo.pedido FROM leitor;`. Its actual `revokes` occurrence
retains Relation/REFERENCES metadata, the empty graph label and the exact raw
`dbo.pedido` name on the native `table_name` beneath `revoke_permission`. Query
identity and prepared/Original captures use that TSQL syntax. The earlier S7 pass
compiled this descriptor but did not observe REVOKE; review found that coverage
gap and the actual fixture was completed without changing its producer.

## DB2 declared Type binding correction

The DT delta in `tsk-6eae` binds each concrete CREATE TYPE branch to its declared
name and one configured Type frame. Distinct uses `distinct_type_name`; array,
cursor, row and structured branches use direct `type_name`. A wrapper name
selector cannot reach these children and previously emitted no stock Type row.
The [native paths](rust_ast_antlr_queries.md#db2-type-declaration-paths-and-contexts)
retain metadata, exact captures and actual parents rather than deriving owners
from equal spelling or range containment.

[IBM LUW 11.5 CREATE TYPE cursor](https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-create-type-cursor)
requires AS before its optional row definition and CURSOR. The grammar correction
adds only that mandatory keyword and preserves the existing helpers. The weak,
named-row and anchored-row fixtures use this documented syntax. The
[row](https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-create-type-row),
[distinct](https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-create-type-distinct)
and [structured](https://www.ibm.com/docs/en/db2/11.5.x?topic=statements-create-type-structured)
fixtures preserve their native branch boundaries. Official SQL PL 11.5,
edition 2020-08-19, physical page36/printed26, was read as a complete rendered
page and confirms the bounded INTEGER ARRAY[100] fixture; the array HTML fetch
returned403. No 11.1 or z/OS example substitutes for LUW11.5 qualification.

At DT qualification, ARRAY[] and qualified declared Type names were still gaps.
The later CT and joint bindings below close those native cases. Current DB2
model, native QName/owner observations, NativeNN and ANTLR Graph have their own
qualified consumer receipts; the five DT branch fixtures alone did not deliver
those later products. Remaining active syntax belongs to the reconciled contracts,
without server datatype classification or castability requirements.

## DB2 optional array cardinality and CREATE TABLE delimiters

The CT delta in tsk-6eae corrects two native rules after a recorded before/after
focal. The [official SQL PL11.5 reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_apps_115.pdf),
edition2020-08-19, physical33/printed23 Table1 shows a simple array with ARRAY[].
The content is optional; its existing explicit cardinality or associative index
type alternatives retain their domains and both brackets remain mandatory.

The [LUW11.5 CREATE TABLE examples](https://www.ibm.com/docs/fr/db2/11.5.x?topic=statements-create-table)
show parenthesized column lists with zero or explicit final options, and LIKE
without final options. Source inspection found that create_table_statement
omitted the column-list parentheses while element_list is a naked list. The
initial required-tail hypothesis therefore did not explain the whole failure.
Only that branch now supplies mandatory '(' element_list ')', and final
create_table_opts becomes repeatable from zero. The helper and other alternatives
are retained; column types, catalog constraints and option combinations are not
reclassified by this change.

These native rules preserve the existing stock Table/Column/Type name nodes and
configured contexts. At CT delivery, QName/model/owner/NN/Graph consumers were
separate required products; their later qualification is linked below. Linux
actual; Windows/macOS and server execution remain unrun.

## DB2 Variable and Alias native subject correction

Actual VA baseline accepted Procedure-local DECLARE and seven Alias inputs with
zero stock subjects. DECLARE named id_ directly, while its selector required
variable_name; Alias names were under subtype branches, not the old wrapper path.
The named helpers and three declared-only Alias paths now correspond to native
nodes. Shared variable statement/type/default and referenced Alias targets stay
intact. [Query contract](rust_ast_antlr_queries.md#db2-variable-and-alias-declared-subject-paths).

Three standalone BEGIN ATOMIC inputs also failed before name selection. The
sql_statement dispatcher had no compound_sql_inlined caller, routing BEGIN into
embedded DECLARE SECTION instead. The additional caller was reconciled/read back
and reviewed before Source. The existing helper remains reused by routines.
The [LUW11.5 inlined language reference](https://www.ibm.com/docs/en/db2/11.5.x?topic=support-inline-sql-pl)
supports standalone blocks; the causal inputs retain required ATOMIC. This does
not qualify every helper form as dynamically executable, the opaque compiled
helper, server/catalog effects or full procedural language coverage.

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
anchors remain. Remaining active syntax is reconciled against actual consumer
contracts; catalog classes and server castability are not parser migration gates.

## Current Method CAST REF syntax scope

The later tsk-4c23 REF unit implements the missing constructor in all three
Method callers, using only `method_reference_type : REF '(' method_named_type ')'`
and the added CAST alternative. Ordinary, parameters, inferred targets and
explicit XML restrictions keep their mature productions. Earlier MC1 text that
requires exhaustive positive datatype-class enumeration, server castability or
indefinite Source prohibition is historical: Root's current requirement is
observable syntax with mature grammar, proportional primary evidence and genuine
SDK/consumer qualification.

The existing nominal lexical alternatives remain unchanged. They do not prove Row/Array/Distinct/Structured catalogue classification. The new REF nominal child uses the same current Method helper with up to two components; no lexer or schema-name expansion is part of this unit. The packet contains APIs, ownership, baselines and affected checks. Generation is official Parser4, with current Lexer/Unicode12 protected.

### Oracle typed declaration consumer

The [Oracle model](rust_ast_antlr_model.md) consumes stock Parameter and local Type observations using native owners and names. It retains native modes/defaults and five Type shapes from the four canonical type body families, including element/index/bound/definition statuses. Record and RefCursor are distinct from collections. This changes no profile query, grammar or input normalization. The C8/B1/B2 sections describe the native application and local-binding subsets.

## Native collection statements and assignment context

The C8 extension adds three explicit raw selectors for native `collection_method_call` terminals DELETE, EXTEND and TRIM, using the current generic SDK compiler. The three PL/SQL stock copies keep all preceding queries in order; their query count becomes 172. The previous seven-family/84-descriptor dedicated-call fixture remains its own qualified scope.

`AntlrOracleApplicationKind::CollectionMethodStatement` retains the actual statement, first direct expression as receiver, subsequent direct expressions as indices and direct native delimiters. It does not fabricate `function_argument` groups. Native syntax does not prove the receiver's type or a Function endpoint. `assignment` records the nearest assignment statement, its direct Target/Value branch and actual intervening expressions. An application inside a target index retains that path; the location is not classified as a semantic write.

The collector independently projects these nodes while PreparedInput is alive. Graph adds optional `collection_statement`/`assignment` within the existing observation JSON, keeps ObservedOnly/no source index/no edges, and omits literal_source for structural statement/branch/path nodes. Receiver/index expressions retain their own native facts. No Source authority, NN schema, SDK/G4 or DTO protocol is changed.

## Oracle collection member catalogue

B2 adds five patterns rooted in a genuine prefixed general_element and ending
at native COUNT/FIRST/LAST/LIMIT/NEXT terminals. The existing EXISTS pattern
already observes that native keyword. The catalogue is 177 queries in all three
byte-equal PL/SQL copies, preserving every prior 172 query and its order.

The real SDK focal exercises the five patterns, captures and duplicate grouped
provenance. B2 consumes those facts through the existing local Type5 binding;
it adds no grammar/SDK/whitelist or normalization. The previous 84-descriptor
dedicated-family test and 169/172-query snapshots remain historical scopes.

## External scalar ordinary return type composition

`tsk-2526` closes only the external scalar ordinary RETURNS domain and its
native/model consumers. The IBM Db2 LUW11.5
[SQL Reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_reference_115.pdf),
physical pages1173 and1179–1180 (printed1141 and1147–1148), draws builtin,
distinct/structured nominal and REF alternatives, then explicitly applies the
parameter considerations to the ordinary result. External XML uses
`XML AS CLOB(n)`, with the mature integer/K/M/G representation.

The implementation therefore composes a caller-local
`external_scalar_ordinary_return_type : data_type | method_reference_type | procedure_external_xml_type`.
No helper union is copied and global `data_type_2` is not broadened.
Nominal spellings do not establish catalog classes; anchors and bare XML
remain existing recognition controls, without new normative/server claims.
CAST source on physical1181 has its own exclusions; `tsk-086a` composes its
local domain below. Target inference and locator placement remain unchanged. The earlier aggregate IBM11.1
table above is a separate contract.

The ordinary wrapper is a direct statement child; its branch is a direct child
of that wrapper. The Function model records one Return from that complete
wrapper and keeps real Original projection. No Parameter or Type is created
from the type syntax. New native getters/ordinals expose the actual grammar,
without compatibility aliases or a public Rust/cxx ABI change.

## Db2 remaining FUNCTION domains

Task `tsk-086a` completes native type compositions for CAST source, external-table, OLE and sourced/TEMPLATE. The primary is IBM Db2 LUW11.5 [SQL Reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_reference_115.pdf), dated 2023-02-09: physical pages1173–1181,1199–1206,1220–1223,1229–1233 and1236 (printed1141–1149,1167–1174,1188–1191,1197–1201 and1204). Diagrams and prose were inspected separately. Manual type numbers are local to each family, not global G4 helpers.

| Position | Native composition | Context retained |
|---|---|---|
| CAST source | `external_scalar_cast_source_type`: existing type or REF | Explicit attributes; prose excludes distinct/ARRAY/structured catalog types; LOB locator |
| External-table parameter | Existing type, REF or external XML representation | Optional name, DEFAULT before locator; no direction |
| External-table column | `external_table_return_type`: same native forms | Direct name/type order; LOB locator; server excludes structured types |
| OLE parameter/column | `param_decl_3` / `oledb_return_type`: existing type or REF diagram branch | Prose restricts parameters to character/graphic or corresponding distinct; BINARY/VARBINARY/XML unsupported; no direction/locator |
| Sourced/TEMPLATE parameter | `sourced_function_parameter_type`: existing or inferred builtin | Optional name/default; empty attributes allowed in both; no new REF |
| Sourced return | `sourced_function_return_type`: existing or inferred builtin | RETURNS and SOURCE required; castability/catalog resolution contextual |
| TEMPLATE return | `template_function_return_type`: existing type | Empty attributes prohibited; explicit parameterized attributes required |

These compositions reuse mature helpers and preserve existing recognition controls. Opaque names cannot classify ARRAY, ROW, CURSOR, distinct or structured catalog types by spelling. Recognizing an OLE REF diagram branch does not prove provider legality; positive command fixtures use VARCHAR/GRAPHIC. XML AS CLOB is not inferred for CAST source from unrelated parameter rules. Language, transform and LOB restrictions remain contextual.

The sourced envelope requires exactly one RETURNS and SOURCE or AS TEMPLATE. SOURCE signatures use existing designator inference, accepting DECIMAL() and rejecting FLOAT(). References never declare extra symbols. Model consumers use direct native children and preserve column order before pairing names, Original projection, missing/nonexact statuses and operational causes. Ordinary results from `tsk-2526`, bindings from `tsk-044f`, target inference and unrelated ARRAY/cursor helpers remain completed independent products.

The three Db2 shipped copies remain byte-identical. The internal asset is a
productive Rust input: AstProfileLoader STOCK in
`crates/graphit-app/src/ast_profile_loader.rs:469–470` includes it, and new/load
decode and supply stock profiles. Only Db2 metadata and its dependent byte total
are reconciled in the two fixture corpus manifests; other languages stay intact.

Official ANTLR4.13.2 generation passed in1974ms without diagnostics. Only Db2Parser.cpp/h/interp changed; Parser tokens and twelve Lexer/Unicode outputs remain unchanged. No runtime/cxx or Cargo manifest/lockfile changed. The [native queries](rust_ast_antlr_queries.md#db2-remaining-function-type-domains) describe paths and actual consumers. Sixteen affected Linux tests passed: five new domain tests, four CAST tests,
one model/Original test, one ten-selector semantic test, two changed reference
consumers and three engine controls. Initial failures in fixture code (a
nonexistent ordinal field, query scope and an old query index) were corrected;
no API workaround or diagnostic normalization was introduced. Scoped Clippy
passed for the library and six affected bins (Cargo1m53s), with no Rust warnings.
Existing generated C++ warnings remain observable in the log. Windows/macOS and
Db2 server execution remain unrun.
