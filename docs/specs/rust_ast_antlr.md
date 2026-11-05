---
title: "Rust ANTLR recognizers and preprocessing"
description: "The native recognizer APIs, source provenance and regeneration recipe for the maintained ANTLR families."
content-type: reference
audience: developers
references:
  - target: {type: task, id: tsk-2526}
    relation: implements
  - target: {type: task, id: tsk-6eae}
    relation: implements
  - target: {type: task, id: tsk-0b48}
    relation: implements
  - target: {type: task, id: tsk-2558}
    relation: implements
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
  - target: {type: task, id: tsk-20e3}
    relation: supports
  - target: {type: task, id: tsk-4eda}
    relation: supports
  - target: {type: task, id: tsk-113b}
    relation: derived_from
  - target: {type: task, id: tsk-0395}
    relation: derived_from
  - target: {type: task, id: tsk-13ae}
    relation: derived_from
  - target: {type: task, id: tsk-c0e2}
    relation: derived_from
  - target: {type: task, id: tsk-1479}
    relation: derived_from
  - target: {type: task, id: tsk-589d}
    relation: derived_from
  - target: {type: task, id: tsk-fb73}
    relation: derived_from
  - target: {type: task, id: tsk-b197}
    relation: derived_from
  - target: {type: task, id: tsk-1787}
    relation: supports
  - target: {type: task, id: tsk-9724}
    relation: supports
  - target: {type: task, id: tsk-3316}
    relation: supports
  - target: {type: task, id: tsk-942f}
    relation: supports
  - target: {type: task, id: tsk-ac8b}
    relation: supports
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
  - target: {type: task, id: tsk-4c23}
    relation: implements
---

# Rust ANTLR recognizers and preprocessing

The selected replacement uses generated C++ parsers, the official ANTLR 4.13.2
runtime, and concrete grammar actions/predicates in C++. Rust coordinates parsing,
projects observations, and indexes code. A small cxx interface returns the required
owned results and diagnostics.

The [engine Source](../../crates/graphit-ast/src/antlr_engine.rs) now uses the [C++/cxx boundary](../../crates/graphit-ast/src/antlr_engine/bridge.rs). Whole-indexer dispatch, symbol extraction and the final query pipeline remain separate [integration work](rust_ast_parsers.md).

The canonical inputs are the customized local product grammars from
`crates/graphit-ast/src/antlr_engine/grammars/`, preserved in
[the maintained inputs](../../crates/graphit-ast/src/antlr_engine/grammars).
Both COBOL grammars are byte-identical. TSQL adds ordinary `REVOKE` through
the formal `revoke_permission` statement and its permission-column helper,
using the existing lexer vocabulary and permission, class, target and principal
rules. The DB2 lexer corrects C and
MODULE keywords, separates the modulo token and supports doubled identifier
quotes. Its parser preserves contextual C/c and MODULE names, adds qualified name
boundaries, native DECFLOAT, separate inferred-attribute designator rules and
the general special-register inventory described below. PL/SQL lexer/parser
and PostgreSQL parser differ only in C++ action syntax/base includes and final
whitespace. PostgreSQL lexer also uses the Tool's `[_\p{L}]` Unicode property for
scalar identifier starts, replacing the old target's `unicode.IsLetter`/surrogate
callbacks, and removes an empty debug-assert callback. This preserves the authored
Letter predicate through the mature Unicode grammar compiler; exact equality of
Unicode-version tables across runtimes has not been established. Entries, options,
notices and local procedure/control-flow rules remain intact. Upstream grammars-v4
supplies references for base classes only; it does not replace these G4 inputs.

| Dialect | Required inputs | Parser entry |
|---|---|---|
| `Db2` | `Db2Lexer.g4`, `Db2Parser.g4`; per-literal `Db2UnicodeLexer.g4`, `Db2UnicodeParser.g4` | `db2_file` |
| `TSql` | `TSqlLexer.g4`, `TSqlParser.g4` | `tsql_file` |
| `PlSql` | `PlSqlLexer.g4`, `PlSqlParser.g4` | `sql_script` |
| `PostgreSql` | `PostgreSQLLexer.g4`, `PostgreSQLParser.g4` | `root` |
| `Cobol85` | `Cobol85.g4`, `Cobol85Preprocessor.g4` | `startRule` |

## Operation and result interface

The DB2 parser accepts `DECFLOAT` and `DECFLOAT(16)` through `built_in_type`.
Function, procedure, method and the two module designators use
`designator_data_type_list_paren`; its types include the existing `data_type`
and `inferred_builtin_type`. Empty attributes such as `DECIMAL()`,
`VARCHAR() FOR BIT DATA` and `TIMESTAMP()` are confined to these signatures.
`FLOAT()`, unparameterized types followed by `()`, user-defined type `()` and
partially empty attributes still produce SDK diagnostics. General declarations,
shared `data_type_list`, cursor parameters and restricted ARRAY index
`data_type_2` retain their existing rules. Precision/catalog validation remains
outside the parser.

Set `ANTLR_JAR` to your local ANTLR 4.13.2 complete JAR before running the regeneration commands.

```text
java -cp "$ANTLR_JAR" org.antlr.v4.Tool -Dlanguage=Cpp -encoding UTF-8 -Xexact-output-dir -no-listener -no-visitor -package graphit::antlr::db2 -o .build/ast-db2-types-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Lexer.g4
java -cp "$ANTLR_JAR" org.antlr.v4.Tool -Dlanguage=Cpp -encoding UTF-8 -Xexact-output-dir -no-listener -no-visitor -package graphit::antlr::db2 -lib .build/ast-db2-types-generated -o .build/ast-db2-types-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

Publish the complete eight `Db2{Lexer,Parser}.{cpp,h,interp,tokens}` files from
staging only after both commands succeed. The existing runtime and bridge are
reused. The two native module rules consume the signature helper; `tsk-3316`
qualifies their integrated MODULE fixtures.

On Linux, `db2_designator_types_native` passes the explicit and inferred type
cases, native parent/order/range assertions, malformed-attribute diagnostics,
declaration/ARRAY isolation and original-context cancellation. The existing
`db2_procedure_contains_control_flow_tree` regression also passes. The first
focal run rejected fixture name `P` because the canonical lexer emits reserved
`P_`; the corrected fixtures use `PROC_F` and `METHOD_F`, without widening
identifier rules. Windows/macOS remain unrun; module fixtures are qualified below.

The independent `tsk-9724` correction changes the DB2 `C_` token's literal from
`C_` to `C`, as required by native LANGUAGE C alternatives in external scalar
functions, external table functions, procedures and method specifications.
The symbolic token name remains `C_`. Its contextual `id_` alternative preserves
ordinary identifiers `C` and `c`; making C a global lexer keyword without that
alternative would reject names previously accepted as ID. String literals and
hidden comments retain their original text. LANGUAGE C_ receives normal native
diagnostics rather than a compatibility alias. The focal fixture covers all four
consumers, ordinary names, JAVA, mixed case, UTF-8/CRLF ranges and malformed
language values. The focal `db2_external_c_keyword_native` passed on Linux
(one test, 0.04 seconds). Both affected DB2 regressions passed:
`db2_designator_types_native` (0.05 seconds) and
`db2_procedure_contains_control_flow_tree` (0.02 seconds). Strict Clippy for the
library and engine fixture passed. Windows/macOS execution is unrun.

The DB2 name product `tsk-3316` changes the keyword to `MODULE: 'MODULE'`
and the percent operator to `MODULO: '%'`. Contextual `id_` accepts MODULE as
an ordinary name, preserving the prior ID capability, alongside `ID`, `C_` and
`DOUBLE_QUOTE_ID`. Delimited identifiers support doubled quotes and retain their
raw spelling. Common function, procedure, specific and module names have one or
two parts. ALTER MODULE object definitions and designators keep unqualified names.
CREATE VARIABLE uses its separate schema-qualified declaration helper (two
parts), while `qualified_global_variable_reference` accepts two or three parts.
One-part defaults still select the current `special_register: id_` branch; that
native classification does not prove variable resolution. Local/state/target
name rules and restricted ARRAY types retain their existing structure.

The same two Tool commands regenerate this product using staging
`.build/ast-db2-names-generated` in both `-o` and parser `-lib`. Both commands
exited successfully and published eight complete official files. The Linux focal
`db2_module_names_native` passed (one test, 0.06 seconds), including both inferred
module designators, unqualified module objects, EOF classification, recovery,
cancelled and expired original contexts. Its first run exposed an EOF fixture
oracle error: SDK `<EOF>` is synthetic and its span is empty at source end.
The corrected oracle asserts EOF separately and compares lexical spelling only
for non-EOF tokens. The three affected DB2 regressions passed (types, C language
and procedure/control flow); strict Clippy passed for the library and engine
fixture. Windows/macOS are
unrun. Aggregate syntax,
routine defaults, full module definitions and Parameter queries remain separate.

The DB2 register product `tsk-942f` adds `native_special_register` for the
[IBM LUW 11.5 general inventory](https://www.ibm.com/docs/en/db2/11.5.x?topic=elements-special-registers)
and documented aliases. It preserves `special_register: id_` as the first
alternative and retains ordinary identifier uses of all fourteen new lexer
symbols through contextual `id_`. Existing `DEGREE_`, `LC_MESSAGES_` and
`LC_TIME_` symbols are reused. Column datetime/user-register rules stay separate;
the general set is available to existing CREATE VARIABLE and expression
consumers. Routine defaults need their own restricted register helper.

`timestamp_register_invocation` precedes `function_invocation` in expression
prediction for parenthesized CURRENT_TIMESTAMP and LOCALTIMESTAMP aliases. The
native query `//function_invocation/function_name` returns no function names for
these register forms or multiword CURRENT TIMESTAMP. Qualified calls such as
`SCHEMA_F.LOCALTIMESTAMP(3)`, two-argument calls and arbitrary argument expressions
retain the existing function branch. Bare aliases remain subject to the existing
identifier/column ambiguity; their spelling does not establish resolved meaning.
CURRENT TIMESTAMP() is explicitly documented; accepting empty parentheses for
its aliases is derived from alias identity. Timestamp precision bounds remain
server validation, so the grammar accepts 13 without pretending to validate it.

The two official Tool commands use `.build/ast-db2-registers-generated` for
staging and parser `-lib`. They exited successfully and published eight complete
official files. The Linux focal `db2_special_registers_native` passed (one test,
0.30 seconds), covering the inventory, aliases, contextual names, raw UTF-8/CRLF
ranges, EOF, diagnostics/recovery and cancelled/expired original contexts.
Four affected DB2 regressions passed (names, types, C language and procedure
control flow), and strict library/engine-fixture Clippy passed on Linux.
Windows/macOS are unrun. This product does not establish
routine-default, aggregate, header, Parameter-query or semantic publication
coverage.

The DB2 constant product `tsk-ac8b` adds native signed numeric constants,
INF/INFINITY/NAN/SNAN and their signed forms, Boolean TRUE/FALSE, and typed
DATE/TIME/TIMESTAMP strings. These branches extend `constant_`, which existing
CREATE VARIABLE initializers and expressions consume. The shared
`numeric_constant`, `integer_value`, `positive_integer`, `string` and
`string_constant` rules retain their contracts. The existing L_ONE and L_ZERO
lexer symbols win over DECIMAL_LITERAL for 1 and 0; the signed helper therefore
admits those terminals explicitly, without changing shared integer helpers in
that delivery. The later `tsk-6eae` adds L_ONE/L_ZERO to `integer_constant`;
signed 0/1 now pass through that wrapper with the same native terminal. The
`tsk-16e5` regression fixtures require the wrapper, exact L_ONE/L_ZERO, sign,
raw source and ranges, replacing the earlier direct-child expectation.

The four new DECFLOAT special-value keywords retain ordinary identifier uses
through contextual `id_`. Raw token names change to the actual keywords; neither
spelling nor a constant node resolves an identifier's catalog meaning. UNKNOWN
is not a Boolean literal. Its existing reserved lexer token is outside `id_`, so
an isolated unquoted UNKNOWN default produces SDK diagnostics. Quoted
`"UNKNOWN"` retains the identifier branch; the product preserves this actual
boundary without declaring UNKNOWN universally invalid in DB2 SQL.
Typed datetime strings preserve spelling and their existing `string` child;
calendar formats, precision and assignment compatibility remain server validation.
Numeric fragments retain their existing decimal-dot and exponent recognition.

Official Tool 4.13.2 regenerated eight complete DB2 outputs using staging
`.build/ast-db2-nontext-constants-generated`. The corrected Linux focal
`db2_nontext_constants_native` passed (one test, 0.17 seconds). Its initial run
exposed the unquoted UNKNOWN fixture assumption; the actual old keyword diagnostic
and quoted identifier branch are now asserted separately, with the failure kept
in Task history. Five affected DB2 regressions (names, types, C language,
procedure/control flow and registers) and strict library/engine-fixture Clippy
passed on Linux. The Source/documentation review found no remaining divergence.
Windows/macOS are unrun. Prefixed strings, Unicode escapes, routine
defaults, headers, aggregate syntax and Parameter queries have separate products.

The DB2 prefixed-string product `tsk-0ce2` adds full native tokens for
G/N graphic strings, GX hexadecimal graphic strings, BX binary strings and UX
UTF16BE graphic strings. `prefixed_string_constant` and its family contexts are
consumed only through `constant_` in existing CREATE VARIABLE initializers and
expressions. G/N retain SQL doubled apostrophes and literal UTF8/CRLF glyphs;
BX requires hex pairs, while GX/UX require groups of four. `GX'FFFF'` is a valid
primary example; no minimum of eight digits is inferred. Empty forms are a
syntax/cardinality decision, rather than primary examples. Raw case, bytes and
quotes remain intact; codepages, lengths and UTF16 surrogate validity belong to
server validation. Shared string/numeric helpers remain unchanged. G retains
its existing reserved token, N and the other isolated prefixes remain ID, and
quoted G retains its identifier use.

The X character-hex implementation in `tsk-16e5` adds native
`HEX_CHARACTER_LITERAL` and `hex_character_string_constant` under
`prefixed_string_constant`. The `tsk-a781` contract recognizes hex pairs and
SPACE U+0020 at boundaries or between nibbles, preserving the complete raw token,
case, blanks and ranges. Empty and SPACE-only forms derive from zero pairs.
This finite recognizer capability does not claim an exhaustive server/codepage
blank set; external SQL whitespace is not imported into quoted X contents.
There is no decoder or blank removal, and the five forms above remain separate.
The focal covers direct/expression and routine defaults, native helper queries,
full-consumption diagnostics, recovery, EOF, cancellation and expiry. The Linux
focal passed (one test, 0.04 seconds; 57 ms wall). Odd X hex retains one native
diagnostic and does not recover the following module in the observed case.
Official Lexer/Parser generation passed with ANTLR 4.13.2, and all eight outputs
match its raw bytes. Six causal DB2 regressions passed: four rerun after the
6eae fixture reconciliation and two unchanged prefixed/Unicode results reused.
Final strict Clippy for the library and `antlr_engine` fixture target passed
in 10.42 seconds after the fixture corrections. Rustfmt, authored diff checks,
108 local documentation links and eight generated-byte comparisons passed.
This qualification ran on Linux; Windows/macOS remain unrun.
Unicode U&/UESCAPE has its separate
refinement `tsk-b91f` and Source producer `tsk-4192`, qualified on Linux.
These literal deliveries alone do not qualify complete routine-default,
header or Parameter consumer coverage.

The Method parameter product `tsk-4eda` adds dedicated builtin, nominal and REF parameter contexts to signatures and isolated parameter/list contexts to Method specifications. Empty lists and unnamed slots are supported. The later `tsk-6eae` MD delta factors each signature slot into `method_signature_parameter_decl`; its direct children are the optional name, required `method_parameter_type` and optional locator. `method_param_list` now contains individual declarations. Specifications retain `method_spec_parameter_decl`. Cursor constructors retain `param_decl_list_paren` and their existing restricted type helper. Parameter type attributes accept zero/one lexical tokens through `method_integer_attribute`; shared integer productions remain unchanged.

The signature builtin set includes XML, BOOLEAN and
`[SYSPROC.]DB2SECURITYLABEL`; specifications exclude bare XML. Nominal types
retain one or two raw identifier parts and REF. A single identifier such as
`P_F` can be an unnamed nominal type and produces no Parameter capture. The
two new keyword tokens remain contextual `id_` alternatives, preserving prior
ordinary names. Directions, DEFAULT, empty VARCHAR dimensions and empty REF
types are syntax errors; catalog type classification, precisions, LANGUAGE SQL
name requirements and locator compatibility remain server conditions.
Returns, CAST and mandatory RETURNS in specifications remain pending products.

Only the main lexer/parser G4 files changed for this leaf. Its two pinned Tool
calls succeeded without diagnostics; eight published outputs match the raw
staging bytes, and all eight Unicode auxiliary outputs remain unchanged.
Regenerate the main recognizers after later DB2 edits with:

```text
cmake -E make_directory .build/ast-db2-method-parameters-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-method-parameters-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Lexer.g4
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-method-parameters-generated -lib .build/ast-db2-method-parameters-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

Publish each main recognizer's `.h`, `.cpp`, `.interp` and `.tokens` from that
stage with `cmake -E copy_if_different` into
`crates/graphit-ast/src/antlr_engine/cpp/generated/db2`. Require both calls to
succeed before copying; never normalize official generated whitespace. The
[query contract](rust_ast_antlr_queries.md#db2-method-parameter-paths) describes
the two Method paths; stock metadata and semantic publication remain pending.

The ALTER TYPE wrapper product `tsk-c23d` removes the extra METHOD from
`alter_type_opts`: ADD now consumes the existing `method_specification`, whose
own METHOD and optional preceding OVERRIDING remain native. No Methodspec
parameter, type, return or characteristic production changes. The official
IBM LUW 11.5 example at physical page 950 uses a single METHOD. IBM's
[dynamic-dispatch documentation](https://www.ibm.com/docs/en/db2/11.5.x?topic=types-dynamic-dispatch-methods)
permits overriding through ALTER TYPE and places OVERRIDING before METHOD.
ADD OVERRIDING METHOD is an explicit formal composition of those sources; the
complete statement is not a located literal IBM example or server qualification.

The fixture `db2_alter_type_method_wrapper_native` passed on Linux (1/1, 0.10
seconds), together with twelve affected DB2 regressions and strict Clippy
(71.97 seconds). Its scoped name/parameter captures follow the
[query contract](rust_ast_antlr_queries.md#db2-alter-type-method-wrapper-paths),
which preserves raw Methodspec parents and prevents captures from other ALTER options,
CREATE TYPE, signatures, PROCEDURE, CALL and cursors. The follow-up `tsk-231a`
requires a unique typed Methodspec return through a separate native helper;
its implementation is described below. Existing Methodsignature return behavior, full
options/types/CAST and shipped Parameter metadata remain separate requirements.

One official Parser Tool call passed without diagnostics. Four published Parser
outputs match its raw stage; Lexer and Unicode auxiliary outputs remain unchanged.
These exact commands passed on Linux; Windows/macOS remain unrun:

```text
cmake -E make_directory .build/ast-db2-alter-type-method-wrapper-generated
cmake -E copy_if_different crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.h crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.cpp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.interp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.tokens .build/ast-db2-alter-type-method-wrapper-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-alter-type-method-wrapper-generated -lib .build/ast-db2-alter-type-method-wrapper-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

The Methodspec return product `tsk-231a` places one mandatory native
`method_specification_return_clause` between two option sequences. The old typed
RETURNS alternative is removed from `method_specification_seq`; options retain
free order before and after the return. RETURNS NULL ON NULL INPUT remains an
external characteristic and cannot substitute for the typed clause. The IBM
LUW 11.5 SQL Reference at physical pages 1535 and 1541 requires typed RETURNS
for ordinary and OVERRIDING Methodspec; its reordering notation does not permit
repeating that segment.

The new Context is a direct child of Methodspec. At this delivery, return
`data_type` and `as_locator` nodes moved beneath it, changing native parents and
rule ordinals without an alias. Its original type/CAST/locator alternatives were
copied exactly; `tsk-ab50` subsequently gives ordinary returns a dedicated type
context while retaining the CAST alternatives. Public Rust/cxx types, Method parameter paths and the
optional return of CREATE METHOD signatures remain intact. The
[query contract](rust_ast_antlr_queries.md#db2-methodspec-typed-return-path)
records the new raw clause selector. Full option uniqueness/exclusivity, type and
CAST contracts, aggregate, X and shipped Parameter publication remain required.
SELF placement fixtures exercise recognition only; catalog/result-type
compatibility and override inheritance are not server validation.

The final fixture `db2_method_spec_returns_native` passed on Linux (1/1, 0.19
seconds), together with thirteen affected DB2 regressions and strict Clippy
(70.07 seconds). One official Parser Tool call passed
in 1.99 seconds without diagnostics. Four published Parser outputs match its raw
stage; twelve Lexer/Unicode outputs remain unchanged. These exact commands
passed on Linux; Windows/macOS remain unrun:

```text
cmake -E make_directory .build/ast-db2-method-spec-returns-generated
cmake -E copy_if_different crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.h crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.cpp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.interp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.tokens .build/ast-db2-method-spec-returns-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-method-spec-returns-generated -lib .build/ast-db2-method-spec-returns-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

The PROCEDURE list product `tsk-eb87` permits empty external parameter lists
and optional IN/OUT/INOUT directions, with IN as the documented default. External
names remain optional; SQL names remain required. SQL empty and omitted lists
retain the existing grammar. The IBM diagram permits omission while its SQL
prose instructs `()`; this syntax recognition does not assert server execution.
The unique `create_procedure_statement` wrapper and current body, options, types
retain the list product’s existing behavior. Header defaults are composed through
`tsk-4537` below. Native name selectors are documented in the
[PROC query contract](rust_ast_antlr_queries.md#planned-db2-procedure-parameter-paths).
The Linux focal `db2_procedure_parameter_lists_native` passed (one test, 0.15
seconds), including both families, native name captures and unique wrappers,
raw ranges, strict negatives and original controls. Ten affected DB2 regressions
and strict library/engine-fixture Clippy passed. The initial build failed before
tests in unrelated Auth code; its owner corrected it before the successful retry.
AST made no Auth edit. Full type/default contracts and shipped Parameter metadata
remain separate required products.

Only two external quantifiers and four raw Parser outputs change. The single
pinned Parser Tool call succeeded without diagnostics, reusing the published
lexer. Lexer and Unicode auxiliary outputs remain byte-identical. Regenerate:

```text
cmake -E make_directory .build/ast-db2-procedure-parameter-lists-generated
cmake -E copy_if_different crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.h crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.cpp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.interp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.tokens .build/ast-db2-procedure-parameter-lists-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-procedure-parameter-lists-generated -lib .build/ast-db2-procedure-parameter-lists-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

Windows/macOS qualification remains unrun.

The routine-default product `tsk-20e3` introduced `routine_default_clause` first
in external scalar parameters; `tsk-4537` composes it into PROCEDURE parameters. A value is required: NULL, an existing constant,
a dedicated direct register, a raw global-variable reference, or a parenthesized
expression. Column `WITH DEFAULT` and omitted values remain separate. Direct
timestamp forms accept empty parentheses and zero/one attributes through a local
helper. The shared integer and general-register rules retain their behavior.
SYSDATE and LOCALTIMESTAMP permission follows their documented timestamp identity;
no server execution or assignment compatibility is asserted. One-part nominal
references do not resolve a catalog variable; bare UNKNOWN retains its reserved
token diagnostics, while quoted `"UNKNOWN"` can select `global_variable`.

Only the parser G4 and four official Parser outputs change. Lexer and Unicode
auxiliary outputs remain byte-identical. The one Parser Tool call succeeded with
no output diagnostics. Regenerate this parser using the published lexer:

```text
cmake -E make_directory .build/ast-db2-routine-defaults-generated
cmake -E copy_if_different crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.h crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.cpp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.interp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.tokens .build/ast-db2-routine-defaults-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-routine-defaults-generated -lib .build/ast-db2-routine-defaults-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

The Linux fixture `db2_routine_defaults_native` passed (one test, 0.36 seconds),
including native value/name captures, all direct register families, existing
constants, raw global references, strict negatives, unaffected consumers and
original controls. Nine affected DB2 regressions and strict library/engine-fixture
Clippy passed. Windows/macOS are unrun. Other headers,
aggregate syntax, complete X constants and shipped Parameter metadata remain
required separate products.

The FUNCTION-default composition `tsk-9b9a` reuses the same completed helper
in three consumers: `ext_table_param_decl`, `param_decl_3` and `param_decl_2`.
The shared `param_decl_3` covers OLE DB as well as sourced and TEMPLATE functions;
SQL scalar/table/row functions use `param_decl_2`. No helper, list, name, direction,
type, locator, return, option or body production changes. Native values now have
routine clause/value parents in those headers; no alias retains the old column
DEFAULT path. The
[function query contract](rust_ast_antlr_queries.md#db2-function-default-paths)
records the three value paths and shared OLE/sourced-template selector.

IBM LUW 11.5 diagrams/text at physical pages 1202/1205 (external table),
1222/1223 (OLE DB), 1231/1232 (sourced/TEMPLATE) and 1244/1248 (SQL) share the
required-value contract and column-default register reference. Timestamp and
alias recognition reuses `tsk-20e3`, including its explicit alias inference;
this is syntax recognition, without assignment/existence/server proof.
DEFAULT is contextually prohibited for ARRAY/ROW/CURSOR in these families and
also for SQL OUT/INOUT, RETURNS ROW and PREDICATES (physical 1249). Those
constraints remain required outside this value-only leaf. OLE/sourced/template
names are optional in the primary reference. The defaults leaf originally left
names required; the separate `tsk-6375` implementation now permits their omission
and is qualified. Its corrected contract uses single-ID or quoted
nominal controls; current `data_type` does not support a qualified nominal type.
That separate type-producer gap remains necessary. See the
[shared parameter-name contract](rust_ast_antlr_queries.md#db2-shared-function-parameter-names).

The new `db2_function_defaults_native` fixture covers four families using five
complete templates, including OLE DB, raw value/name captures and parent/range
checks. Existing scalar and PROC fixtures adapt only the newly obsolete function
value oracles; their isolation queries and column/local/cursor assertions stay
intact. The final focal passed 1/1 in 1.62 seconds. All fourteen affected DB2
regressions passed: one isolated filter and thirteen exact filters in one mature
libtest invocation (13/13 in 0.67 seconds). Strict Clippy passed in 153.93 seconds.
Grouping the remaining filters preserved coverage and avoided repeated shared
cache acquisition; no broader gate ran. One official Parser Tool call passed in
17.80 seconds without
diagnostics; four raw Parser outputs match the stage and all twelve Lexer/Unicode
outputs remain unchanged. These commands passed on Linux; Windows/macOS are unrun:

```text
cmake -E make_directory .build/ast-db2-function-defaults-generated
cmake -E copy_if_different crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.h crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.cpp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.interp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.tokens .build/ast-db2-function-defaults-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-function-defaults-generated -lib .build/ast-db2-function-defaults-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

The PROCEDURE-default composition `tsk-4537` replaces only the two value
consumers in `proc_ext_param` and `proc_parameter_list_item` with the delivered
`routine_default_clause`. `db2_procedure_defaults_native` exercises both complete
external and SQL statements, the five value branches, exact raw name/value
captures, required values, strict negatives, raw ranges and original controls.
The old PROCEDURE-list fixture changes only its header DEFAULT oracle; its
other list/name/direction assertions and local DECLARE SQL behavior stay intact.
The later `tsk-6eae` factors local defaults through
`declare_variable_default_clause`. The `tsk-16e5` fixtures require its
`declare_variable_statement` parent, raw `DEFAULT 1`, mandatory `default_values`
value and exact L_ONE token/range; DDL `default_clause` remains separate.
The Linux focal passed (one test, 0.67 seconds), all eleven affected DB2
regressions passed, and strict library/engine-fixture Clippy passed. The initial
compilation stopped before tests because the shared filesystem had no space;
the coordinator recovered inactive duplicate executables before the normal retry.
The failure log is retained; no DB2 syntax defect or Source fix is inferred from
that environment failure. Native value paths are documented in the
[PROC default query contract](rust_ast_antlr_queries.md#planned-db2-procedure-default-paths).

IBM LUW 11.5 prohibits defaults for OUT/INOUT and ARRAY/ROW/CURSOR contextually.
This value composition does not validate those conditions, assignment or catalog
identity; full header/type qualification remains required. X constants and shipped
Parameter metadata also remain separate products. Fixtures use IN or omitted
direction and make no server-execution assertion.

The single official Parser Tool call succeeded without diagnostics; its four
outputs match the raw staging bytes. Lexer and Unicode auxiliary outputs remain
byte-identical. The exact commands passed on Linux; Windows/macOS remain unrun:

```text
cmake -E make_directory .build/ast-db2-procedure-defaults-generated
cmake -E copy_if_different crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.h crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.cpp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.interp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.tokens .build/ast-db2-procedure-defaults-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-procedure-defaults-generated -lib .build/ast-db2-procedure-defaults-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

The Unicode Source product `tsk-4192` implements the closed contract from
`tsk-b91f`; its focal and affected DB2 regressions passed on Linux. A full U&
token and optional UESCAPE clause are consumed through `constant_`. The clause must first
validate exactly one permitted Unicode escape character; an invalid clause must
retain its error and cannot select the default backslash. Per-literal auxiliary
ANTLR lexer modes and parser productions validate active escapes of four
hex digits, plus-six hex digits, paired escapes and SQL doubled apostrophes.
Their error tokens consume one scalar after valid rules, avoiding a greedy
longest-match catch-all. The official UTF8 strict converter is used only for
the single escape-character token, with no SQL value decoding or normalization.

This design preserves raw tokens and hidden comments between the literal and
UESCAPE. The validator runs in the existing DB2 tree walk, with operation-local
objects and the original cancellation/deadline control. Only newly produced
auxiliary diagnostics receive checked offsets into the original source;
a local EOF diagnostic becomes a regular empty literal boundary, rather than
whole-source EOF. The two auxiliary G4 files, their official outputs and two
build source entries are implemented within the approved ownership. Existing
cxx ABI and other-language routing stay outside this product.

The auxiliary Unicode recognizers use the same pinned Tool/runtime 4.13.2.
The main generation script remains unchanged; supplementary commands are:

```text
cmake -E make_directory .build/ast-db2-unicode-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-unicode-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Lexer.g4
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-unicode-generated -lib .build/ast-db2-unicode-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-unicode-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2UnicodeLexer.g4
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-unicode-generated -lib .build/ast-db2-unicode-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2UnicodeParser.g4
```

All four calls must succeed before publication. For each recognizer, copy the
four `.h`, `.cpp`, `.interp` and `.tokens` files with `cmake -E copy_if_different`
from the stage into `crates/graphit-ast/src/antlr_engine/cpp/generated/db2`;
compare all sixteen raw files byte for byte. Generated SDK whitespace is retained.
These commands use Java/CMake on each platform; execution on Windows/macOS is
unrun. Only the two new Unicode `.cpp` files join the existing build source list.

The Linux focal `db2_unicode_string_constants_native` passed (one test, 0.09
seconds), with full raw contexts, UTF8/CRLF, hidden comments, four independent
custom escape instances, original cancellation/deadline and SDK diagnostics in
original ranges. It also covers invalid UESCAPE after multiline comments,
unchanged earlier main-lexer diagnostics, and multiple auxiliary errors without
reapplying offsets. Seven affected DB2 regressions passed: names, designator
types, C language, procedure control flow, registers, nontext constants and
prefixed strings. The first build failed before tests in unrelated Data
`remote_graph` code; the Data owner corrected it before the successful retry.
AST made no Data change. Strict library/engine-fixture Clippy passed on Linux;
final Source/documentation review found no remaining divergence. Windows/macOS are unrun; full AST/default/header/Parameter coverage
still has separate pending producers.

Odd `BX'ABC'` does not form a binary token: it retains ID BX and ordinary
string `'ABC'`. The parser reports the unconsumed string and strict parsing
rejects the input. DB2 batch-boundary recovery uses the official ANTLR error
strategy to resynchronize at the separator and retain the following valid
module with its exact Source range. Earlier Task evidence recorded loss of that
suffix under the former strategy; that limitation is superseded, not a current
acceptance requirement. This does not promise recovery for every malformed SQL
input; lexer-invalid odd X literals retain their separately tested behavior.

Official Tool 4.13.2 generated eight byte-equal DB2 outputs in
`.build/ast-db2-prefixed-strings-generated`. Six affected DB2 regressions passed
on Linux (names, types, C language, control flow, registers and nontext constants).
The corrected Linux focal `db2_prefixed_strings_native` passed (one test,
0.06 seconds), and strict library/engine-fixture Clippy passed. A retry was needed
after the Data owner corrected an unrelated compile error; AST did not change
Data. The initial recovery failure remains in Task history. Final
Source/documentation review found no remaining divergence. Windows/macOS are unrun.

`parse(dialect, source, &SnapshotContext)` uses LL prediction and real SDK
recovery. `parse_strict` rejects lexer/parser diagnostics, recovered error nodes
and missing required syntax. Native failures retain their cause and never become
successful partial output or select another engine.

The bridge returns rule/token names, buffered tokens with channels and spelling,
flat nodes with parent indexes, lexer/parser diagnostics, and recognized body
spans. Scalar metadata explicitly distinguishes regular half-open spans, EOF and
unindexed positions with reasons. EOF is classified before SDK sentinel indexes;
inclusive token stops require checked addition before width conversion. Inserted,
absent or reversed positions never become an invented zero range. Parent and
vocabulary indexes are validated.

Input, lexer, token stream, parser and tree live in one synchronous C++ operation.
The tree belongs to its parser. `ParsedSyntax` borrows the prepared source and owns necessary result records, without
C++ trees or caches per project. The official shared DFA remains the default.
The optional thread-local cache is not enabled. See the
[official C++ target](https://github.com/antlr/antlr4/blob/4.13.2/doc/cpp-target.md).

A Rust `OperationControl<'a>` borrows the original `SnapshotContext` and preserves
the first real control error. C++ may poll it synchronously and must release the
reference before returning. Checks surround SDK phases and collection; they do
not promise hard interruption inside every SDK conversion/prediction step. There
is no context clone or universal ledger. Cxx supports
[borrowed opaque Rust types](https://cxx.rs/extern-rust.html#opaque-rust-types).

Extern C++ recognition returns `Result<NativeSyntax>`. Diagnostics and stopped
operations are explicit states; unexpected standard exceptions become native
errors. Vendor text is preserved, not parsed into invented codes. Rust callbacks
must not panic. See [cxx exception translation](https://cxx.rs/binding/result.html).

`prepare_plsql(source, &SnapshotContext)` and
`prepare_cobol(source, CobolFormat, &SnapshotContext)` return
`Result<PreparedSource, AntlrEngineError>`. PL/SQL uses a separate native `lex`
operation with real lexer tokens and diagnostics; it does not run a parser and
discard parser errors. Preparation owns text and mappings, without retaining
native objects or the context.

The callback retains the first actual control error. C++ catches its private
stop exception and returns `Stopped` with empty arrays. Rust returns that exact
control error; `Stopped` without one is a native contract violation. A CXX
exception preserves both the native cause and any retained control error.
`Success` with a retained error is rejected, and a final context check precedes
publication. No stopped or failed operation publishes partial syntax.

For embedded PL/pgSQL, `parse_postgres_with_plpgsql(source, language, control,
&SnapshotContext)` uses the observed host-parent index. `GrammarLanguage::parse_with_context`
checks the original Core context in the Tree-sitter progress callback alongside
`ParseControl`; it preserves actual Core I/O errors in `EngineError::Operation`.
Neither SDK trees nor a cloned operation context cross the native ANTLR boundary.
The portable PL/pgSQL fixture builds its maintained C parser/scanner through CMake;
Linux execution does not establish Windows/macOS qualification.

## UTF-8 and original coordinates

The official SDK converts UTF-8 into UTF-32 internally. Rust supplies the prepared
string directly and does not create another UTF-32 copy. The SDK removes exactly
one initial BOM, so scalar-to-prepared-byte mapping accounts for that one scalar.
Second/interior BOMs remain input. Empty and BOM-only EOF are explicit cases.

The delivered [range producer](../../crates/graphit-ast/src/source_ranges.rs)
provides `SourceText::scalar_byte` and validated `range`. Bulk conversion resolves requested scalar endpoints in one pass over UTF-8
character boundaries,
without copying codepoints or materializing discarded byte points per record.
SDK line/column or token text length is not an origin.

The current [prepared producer](../../crates/graphit-ast/src/prepared_input.rs)
uses `PreparedInput::new(original_bytes, prepared_bytes, plan, &SnapshotContext)`
and borrowed `prepared_text`, `prepared_span`, `project_prepared`. Obsolete
`MappingLimits`, `AntlrSpan` and `project_antlr` are not current APIs.

Syntax records borrow prepared source for input binding while owning
nodes, token text and diagnostics. Projection returns existing borrowed
`OriginalProjection<'o>`: `Exact`, `Compound`, `Synthetic`, `Unmappable` or
`Ambiguous`. Keep all pieces/alternatives and their actual original bytes;
never substitute an enclosing hull, interpolation or truncated list.

## Required grammar semantics and embedding

PL/SQL preserves the maintained Oracle 10–12 union, newline lookahead and
join-start exclusion using actual token types. PostgreSQL preserves case-sensitive
dollar tags/reset, numeric rollback, operator retokenization, acceptable-operator
predicates, and grammar-defined Unicode letter sets beyond the BMP. Tool Unicode
support does not normalize source. DB2 retains procedure-body IF/WHILE control
flow; TSQL and both COBOL grammars remain active.

The C++ `ParseRoutineBody` action inspects real rule context/token state and
collects observed `AS` dollar-quoted `LANGUAGE plpgsql` bodies with their real host
parent. Empty or unindexed observations remain classified; the splice consumes
only reliable nonempty ranges. Single/E'/U&' literals, LANGUAGE sql/other languages,
and dollar literals in SET options retain host syntax without PL/pgSQL splice.

The [embedded facade](../../crates/graphit-ast/src/antlr_engine/embedded.rs)
preserves real Tree-sitter PL/pgSQL trees, nodes and compiled queries. Each body
links to `createfunc_opt_list` through `host_parent`, using actual `func_as`
selection. Local parents, named/error/missing flags, pattern indexes and properties
remain available. Node/capture ranges and points project into the original host. Body base points
are resolved in one UTF-8 scan; node/capture projection does not rescan host prefixes.
Trees/queries retain their grammar owners; another generation's query
fails explicitly. The caller supplies the trusted PL/pgSQL grammar and real
controls. No manual quoted-literal decoder is added.

Fixtures acquire `builtin("tree-sitter-plpgsql")`, using the linked maintained
C parser/scanner/headers and full BSD-3-Clause
[license](../../crates/graphit-ast/grammars/tree_sitter/plpgsql/LICENSE).
Their functional host/body assertions exercise the same linked language used by
the application. Historical native runs were qualified on Linux; Windows/macOS
execution remains unverified.

## Prepared text

[Preprocessing](../../crates/graphit-ast/src/antlr_engine/preprocessing.rs)
retains `PreparedSource`, `prepare_plsql`, `prepare_cobol`, `CobolFormat` and
`detect_cobol_format`. `text` borrows prepared bytes; `with_origin_plan` keeps plan
storage alive. Direct pieces contain identical bytes; inserted spaces/comments
are synthetic, omitted bytes are removed, and boundaries/EOF have explicit origins.

PL/SQL uses the generated lexer for CREATE MATERIALIZED VIEW metadata
`) USING (...)` followed by REFRESH, AS or BUILD, including nested parentheses and
quoted strings. Replacement preserves byte length and CR/LF but remains synthetic.
Unbalanced metadata remains input for the parser. Lexical faults retain original
input and real diagnostics rather than hiding errors.

COBOL supports Fixed (sequence 1–6, indicator 7, code through 72), Variable (same
prefix without cutoff) and Tandem (indicator 1). Stages 1–4 preserve debug code,
escape comments, join continuations and preserve spacing. Fragments remain
distinct origins; joined literals can be Compound. Unknown indicators/incomplete
prefixes are errors. COPY/REPLACE stage 5 and invented FREE format are excluded.
Detection examines up to 50 lines: Fixed if more than 30% have numeric sequence
fields or no eligible lines exist, otherwise Variable. Tandem is explicit.
The selected format determines the parser.

## Generation, build and publication

`tsk-1479` owns official Tool generation, C++ actions and generated five-family
Source plus COBOL preprocessor, preserving complete notices and Tool diagnostics.
The maintained [generation script](../../crates/graphit-ast/src/antlr_engine/scripts/generate.cmake)
uses the official 4.13.2 Tool, Java 11 or newer, UTF-8 and the C++ target. It runs
lexers before parsers, with namespaces `graphit::antlr::<family>` and without
unused listeners/visitors. The portable invocation is:

```text
cmake -DANTLR_TOOL_JAR=<official-4.13.2-jar> -DOUTPUT_DIRECTORY=<new-output> -P crates/graphit-ast/src/antlr_engine/scripts/generate.cmake
```

The output directory must be new. Generation happens in staging and publishes
only after all ten inputs succeed. Real Tool diagnostics are printed and retained
in `tool-diagnostics.log`; failure reports the exit/diagnostics and removes partial
staging. Complete [input notices](../../crates/graphit-ast/src/antlr_engine/grammars/GRAMMAR_NOTICES.md)
accompany generated outputs. The first Linux generation produced twelve C++ files
and twelve headers, including the COBOL preprocessor.

The two PostgreSQL warning 146 messages concern empty rules that leave the two
escape-string continuation modes. Those transitions are maintained grammar
behavior; their existing nullable-mode lexical cases remain functional fixtures.
Warnings are retained, not suppressed or reported as zero warnings.
Captured official artifacts and useful PostgreSQL lexer evidence are reused.
Historical custody/hash matrices and universal quotas are
not new requirements.

| Input family | Private C++ semantics | Functional consumers |
|---|---|---|
| PL/SQL | Full SDK scalar newline/EOF lookbehind; Oracle 10/11/12 union; join exclusion by actual token types | SQL/METADATA and preprocessing fixtures |
| PostgreSQL lexer | Case-sensitive tag stack and reset; two-scalar input/column rollback; operator reassignment; full-scalar lookahead | Numeric/operator/tag/Unicode and nullable-mode fixtures |
| PostgreSQL parser | Actual `createfunc_opt_list` with `AS/func_as` dollar delimiters and `LANGUAGE plpgsql`; maintained acceptable-operator predicate | AS-only routing, host-parent, recovery and PL/pgSQL splice fixtures |
| DB2 | Complete maintained grammar, including procedure IF/WHILE | DB2 procedure/control-flow fixture |
| TSQL | Complete maintained grammar | SQL/comments/case/recovery fixtures |
| COBOL85/preprocessor | Complete grammars and real `startRule` entries | Fixed/Variable/Tandem stages and origin fixtures |

The [PostgreSQL base](../../crates/graphit-ast/src/antlr_engine/cpp/actions/PostgreSQLParserBase.h)
returns operation-local `RoutineBodyObservation` records with the actual host
context and opening/closing SDK tokens. The bridge derives checked scalar spans
and host indexes before parser/token-stream destruction. It preserves absent or
inserted recovery positions as unindexed observations. `SET` dollar strings and
single/E/U& AS strings remain host-only. C++ does not parse PL/pgSQL bodies; the
delivered Rust Tree-sitter producer handles their parse/query/splice.

Generated/action Source is compiled with C++17 against real official SDK headers.
Compilation establishes signatures and closure. The bridge's eighteen functional
fixtures passed in MAIN on Linux under `tsk-b197`, including all five families,
recovery, preprocessing and the real PL/pgSQL splice. Whole-indexer/tool integration
and native Windows/macOS execution remain separate work. No extra replay/hash or
generation-check harness is required.
The Linux Tool 4.13.2/JDK 17.0.20 generation and compilation of all twelve
generated plus three action C++ files passed. Evidence is in
`.build/antlr-runtime-source/generated-qualification.json`,
`generation-qualified.log` and `generated-compile-qualified.log`. The first
compiler failure from a missing PostgreSQL superclass include was corrected in
the G4 and regenerated by the official Tool; generated code was not rewritten.
Generated comments retain relative grammar names.

`tsk-589d` delivers the [official runtime Source](../../crates/graphit-ast/src/antlr_engine/cpp/vendor/antlr4/GRAPHIT-PATCHES.md)
and private [build_support](../../crates/graphit-ast/src/antlr_engine/build_support.rs).
Approved `build_runtime()` returns `RuntimeInstall` with `include_dir`,
`library_dir`, `static_lib_name`, and the C++ definitions required by consumers,
or a build error with its cause. It uses mature
`cmake`. Configuration selects C++17/static and disables shared/upstream tests/demo.
MSVC CRT follows Cargo's target `crt-static`. Real target: `antlr4_static`;
installed headers: `include/antlr4-runtime`; Windows archive naming differs.

Official `Version.h` defines `ANTLRCPP_VERSION_*` but combines incorrectly named
`ANTLR4CPP_VERSION_*`. The owned Source patch corrects those six references explicitly,
preserving provenance/notices, without a fake header or silent string normalization.
The source-only vendor tree excludes upstream tests/demo, build caches, GoogleTest
downloads and prebuilt binaries.

The helper builds the official install target using `cmake 0.1`; it returns the
installed include/library paths instead of guessing generator-specific build paths.
Cross compilation requires `CMAKE_TOOLCHAIN_FILE`. Environment/I/O errors retain
their causes. Cmake-rs exposes build failures as panics: the helper returns
`NativeBuild` with the original payload, while native diagnostics remain in the
Cargo build log. It does not fabricate an exit status or retry another SDK.
Root must link the static runtime and platform C++ library, Unix threads, and
Apple CoreFoundation; Windows bridge compilation requires `ANTLR4CPP_STATIC`.
On macOS the build gives the AST runtime private C++ names to isolate it from
Ladybug's bundled ANTLR. The runtime, recognizers and bridge share the same
compiler definitions for `antlr4`, `antlrcpp`, and `XPathLexer`.

The [linked runtime test](../../crates/graphit-ast/src/antlr_engine/cpp/runtime_test/CMakeLists.txt)
uses the installed SDK, compiled version macros and `RuntimeMetaData::VERSION`,
then exercises real SDK UTF-8 conversion, initial BOM stripping, supplementary
scalars and EOF. Portable CMake/CTest invocations are:

```text
cmake -S crates/graphit-ast/src/antlr_engine/cpp/runtime_test -B .build/antlr-runtime-test -DCMAKE_PREFIX_PATH=<RuntimeInstall>
cmake --build .build/antlr-runtime-test --config Release --parallel 1
ctest --test-dir .build/antlr-runtime-test -C Release --output-on-failure
```

Use the same target toolchain and CRT for this test and the runtime. When
testing a macOS SDK installed by the Cargo build, append
`-DCMAKE_CXX_FLAGS="-Dantlr4=graphit_ast_antlr4 -Dantlrcpp=graphit_ast_antlrcpp -DXPathLexer=GraphitAstXPathLexer"`
to the CMake configure command.
The Linux Source overlay `.build/antlr-runtime-source` compiled the exact helper with
`cmake 0.1.58`; its linked runtime test passed. Evidence is in
`.build/antlr-runtime-source/qualification.json`, `build-qualified.log` and
`runtime-test.log`. This qualifies H Source/runtime only; it does not activate
ANTLR in MAIN or qualify the five generated parsers.
See [runtime CMake](https://github.com/antlr/antlr4/blob/4.13.2/runtime/Cpp/runtime/CMakeLists.txt)
and [version header](https://github.com/antlr/antlr4/blob/4.13.2/runtime/Cpp/runtime/src/Version.h).

`tsk-113b` owns bridge/Rust conversion/preprocess/embedding and qualifies the same
Source in `.build/antlr-bridge-source`. It compiles the actual maintained Source,
G/H artifacts and [build fragment](../../crates/graphit-ast/src/antlr_engine/build_bridge.rs),
without changing MAIN manifests or exports. Resolved libraries are cxx/cxx-build
1.0.205 and cmake 0.1.58. Source functional qualification passed all18ANTLR fixtures on Linux: the13
maintained cases plus EOF/BOM/Unicode diagnostics, real Core cancellation/deadline,
compound projection/input binding, Core control in the PL/pgSQL SDK callback and
representative PostgreSQL Unicode letters/tags/nonletters. The run is recorded in
`.build/antlr-bridge-source/functional-qualified.log`; it establishes Source behavior,
not MAIN activation or other-platform execution. Affected strict Clippy/formatting passed; the read-only impact review found and
verified the removal of discarded per-record BytePoint prefix scans.
Root owns `tsk-fb73`: shared Cargo/Lock/build.rs/exports and scoped lint composition.
The Main crate now exports `antlr_engine` and its build script calls
`build_runtime()` followed by `build_recognizers(&runtime)`. It pins `cxx` and
`cxx-build` to 1.0.205 and `cmake` to 0.1.58. The build links the installed static
SDK, Unix pthread and Apple CoreFoundation; cxx-build supplies the C++ standard
library. For a Windows/GNU target with a compiler identified as GNU by cc, the recognizer builder adds -Wa,-mbig-obj so generated parsers can use the PE/COFF big-object format beyond the ordinary section limit. Other target/compiler combinations, grammars, generated source, actions and pinned versions keep their existing configuration. The first Windows cross-check compiled the recognizers after this correction, then failed in the separate native icebug query consumer because SDK headers marked move definitions dllimport. After that consumer composed the SDK static link-mode metadata, the affected graphit-ai/graphit-ast library cross-check passed; this does not qualify application compilation or native Windows execution. The existing crate unsafe denial remains, with the narrow bridge allowance.
Actual bridge/generated/actions files and private SDK headers are composed through
[cxx-build](https://cxx.rs/build/cargo.html). Main library checking and ANTLR fixture compilation/linking passed on Linux;
these compile checks do not establish runtime test success. SDK
headers are not a public cross-crate interface.

`tsk-b197` executed the MAIN fixtures after publication. G/H proceed independently after
planning; bridge follows G/H; publication follows bridge Source; MAIN follows
publication. No bridge→publication→bridge cycle exists.

## Verification scope

Reusable [functional tests](../../crates/graphit-ast/tests/antlr_engine.rs)
cover five accept/reject pairs, recovery/strict, comments/case/UTF-8/CRLF, DB2
control flow, METADATA, PG numeric/operator/tag/body semantics, BOM, metadata
preprocessing, three COBOL formats/compound origins, real PL/pgSQL nodes/queries,
routing, owner lifetimes and controls. Add a focused regression only for an actual
gap in the C++ replacement.

After publication:

```text
cargo xtask cargo test -p graphit-ast --test antlr_engine --jobs 1
cargo xtask cargo clippy -p graphit-ast --lib --test antlr_engine --jobs 1 -- -D warnings
```

The private official C++ runtime check and18C++/cxx functional Source fixtures
passed on Linux. Affected strict Clippy and formatting passed. Planning, object
compilation, runtime-only qualification, functional Source and MAIN execution
remain distinct. MAIN publication passed library checking and fixture compilation/
linking on Linux under `tsk-fb73`; MAIN functional execution passed all eighteen
fixtures under `tsk-b197`. The affected MAIN rustfmt and strict Clippy checks
passed after the focused assertion for SDK-inserted recovery tokens was added.
Windows/macOS and cross compilation remain unrun. These leaves introduce no broad workspace gate,
format conversion.

Main publication qualification uses the actual workspace sources and resolved lock:

```text
cargo xtask cargo check --locked --offline -p graphit-ast --lib --jobs 1
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_engine --no-run --jobs 1
```

Both commands passed on Linux. Evidence is in `.build/antlr-main-publication/`
(`check-lib.log`, `antlr-no-run.log`, `qualification.json`). The initial offline
metadata attempt lacked the `cxxbridge-cmd` cache; normal Cargo resolution fetched
the missing dependency and succeeded. Official SDK overloaded-virtual warnings
remain in the native build log. No crate-wide unsafe allowance was introduced.

MAIN functional evidence is retained in `.build/antlr-main-qualification/`
(`functional-qualified.log`, `environment.json`, `qualification.json`,
`rustfmt-qualified.log`).
The run used the actual workspace sources and existing Main build cache:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_engine --jobs 1
```

The eighteen passing cases cover the five recognizers, strict/recovery behavior,
actual token and diagnostic boundaries, Unicode letters and dollar tags, initial
BOM and EOF, PL/SQL metadata, all three COBOL formats, compound origins and input
binding, original Core cancellation/deadlines, real PL/pgSQL nodes/queries and SDK
progress, and host/body routing. The existing recovery case was strengthened
with a missing COBOL `DIVISION`: the inserted SDK node retains
`Unindexed(InsertedRecoveryToken)` and no source span. All eighteen cases passed
again after that assertion; the initial log and fixture-development failures are
retained separately. This qualification
does not establish whole-indexer/query/symbol dispatch or MCP/CLI behavior.

## DB2 shared FUNCTION parameter names

`tsk-6375` changes only `parameter_name` to `parameter_name?` in `param_decl_3`.
The same native context serves OLE DB and sourced/TEMPLATE functions. It reuses
existing `data_type` and `routine_default_clause`; no type, direction, list,
return, option, body or public Rust/C++ interface changes. An unnamed slot has
no `Parameter_nameContext`, so name queries capture only actual names. Types
and default values retain their native parents and raw ranges.

IBM LUW 11.5 SQL Reference (2023-02-07), physical pages 1220/1223 and 1229/1232,
permits name omission in these families. SQL function pages 1241/1247 require
names; `param_decl_2` remains unchanged. OLE controls use CHAR, VARCHAR and
GRAPHIC contexts. Sourced/TEMPLATE controls include single-ID and quoted nominal
types, solely as parser shape; no catalog classification, provider/SOURCE
existence or default-to-type assignment is proved. Qualified nominal types
remain necessary. At this delivery zero-argument headers were a separate gap;
`tsk-a72c` subsequently makes the two caller references optional, while the
shared list itself still requires at least one slot when present.

The new `db2_shared_function_parameter_names_native` fixture covers three complete
families with named, unnamed and mixed slots, raw names/types/defaults, causal
invalid statements, other-header isolation, UTF-8/CRLF/comments/EOF and original
operation controls. The focal passed 1/1 in 0.47 seconds; fifteen exact affected
DB2 filters passed 15/15 in 1.67 seconds. Strict Clippy passed in 110.26 seconds.
Raw outputs, format, authored diffs and forty-three local links/anchors passed.
One official Parser Tool call passed in 2.28 seconds
without diagnostics; Parser4 matches its raw stage and all twelve Lexer/Unicode
outputs are unchanged. These invocations passed on Linux; Windows/macOS are unrun:

```text
cmake -E make_directory .build/ast-db2-shared-function-parameter-names-generated
cmake -E copy_if_different crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.h crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.cpp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.interp crates/graphit-ast/src/antlr_engine/cpp/generated/db2/Db2Lexer.tokens .build/ast-db2-shared-function-parameter-names-generated
java -jar "$ANTLR_JAR" -Dlanguage=Cpp -encoding UTF-8 -package graphit::antlr::db2 -Xexact-output-dir -no-listener -no-visitor -o .build/ast-db2-shared-function-parameter-names-generated -lib .build/ast-db2-shared-function-parameter-names-generated crates/graphit-ast/src/antlr_engine/grammars/db2/Db2Parser.g4
```

## DB2 Method ordinary return types

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

The initial focal failed on a basic CREATE TABLE counterexample that the
existing grammar already rejects. The ab50 artifact
`.build/ast-db2-method-ordinary-return-types-focal-initial-fail-result.json`
and FAIL history are retained;
only the new fixture's counterexamples were replaced with previously qualified
ALTER TABLE column, local DECLARE, sourced FUNCTION, CALL and cursor consumers.
No grammar change or second Tool run was used to bypass that failure.

These bounded commands passed; they are portable invocations, with actual
execution on Linux only:

```text
cargo xtask cargo test --locked --offline -p graphit-ast --test antlr_engine db2_method_ordinary_return_types_native
cargo xtask cargo clippy --locked --offline -p graphit-ast --lib --test antlr_engine -- -D warnings
```

## DB2 Method CAST target inferred attributes

`tsk-e94e` composes the mature `inferred_builtin_type` from `tsk-1787` into
exactly two Method CAST targets through `method_cast_target_type`. At that delivery, its other
alternative was the previous generic data_type. The subsequent XML delta below
changes its non-inferred child and source contexts. Ordinary returns, parameters
and locator placement remain unchanged. `data_type_3`
remains active in external scalar functions; only its Methodsignature getter
disappears. No global rule, public Rust/cxx ABI or shipped metadata changes.

The [primary derivation map](rust_ast_antlr_profile_reconciliation.md#independent-method-cast-target-attributes)
explains why each empty-attribute family belongs here. VARCHAR() is IBM's literal
example; the other parametrizable alternatives are formal derivations. At this
producer's delivery, nominal/REF CAST was separate work. The current REF syntax
unit below supersedes catalog enumeration/convertibility as parser requirements;
this target-attribute delta did not implement the later constructor.

Complete CREATE METHOD, CREATE TYPE and ALTER TYPE statements exercise 37
inferred/nonempty pairs, two parameter-list forms and their native raw
[selectors](rust_ast_antlr_queries.md#db2-method-cast-target-paths). Nonempty
controls use existing generic attribute tokens: LOB(2K), numeric(12,2), lengths
40, DECFLOAT(16) and TIMESTAMP(6). This does not qualify the separate zero/one
attribute gap in generic CAST types. Bare XML's existing generic behavior is
preserved; XML() and FLOAT() are outside the new inferred domain.

Fixtures include nested hidden comments inside empty parentheses, UTF8/CRLF,
raw token text and EOF, source/target parents, option order, locators, overrides,
two ordered Methods and causal negatives. SDK recovery is checked separately
from strict syntax errors; original cancellation/deadline and concurrent parses
remain exercised. Two ab50 CAST oracle blocks change only for the native target
context; ordinary queries still have zero CAST matches.

One official Parser Tool 4.13.2 call passed in 2.01 seconds without diagnostics.
Four published Parser outputs match the raw
`.build/ast-db2-method-cast-target-inferred-generated` stage; twelve Lexer/Unicode
outputs are unchanged. The new fixture passed 1/1 in 2.05 seconds; its
Cargo retry, including native compilation, took 879.01 seconds.
Seventeen exact affected DB2 filters passed 17/17 in 2.10 seconds
(Cargo 216.20 seconds), and strict Clippy passed in
97.19 seconds. The initial Cargo run
ended 101 before tests after an explicitly coordinated interruption of its own
native SDK compiler leaves. Its 2544.11-second result is preserved in
`.build/ast-db2-method-cast-target-inferred-focal-initial-resource-interruption-result.json`;
this is not a parser assertion failure. The passing retry used four jobs and
retained compiled objects. Both results remain distinct; the interruption is
not overwritten by the later pass. A later `tsk-a72c` runner clone accidentally
overwrote this successful focal's full log; its original structured result,
Task audit and review remain, while that full log is unavailable. The precise
loss is recorded in
`.build/ast-db2-shared-function-zero-arguments-log-provenance.json`; no full-log
reconstruction is claimed. Windows/macOS are unrun.

```text
cargo xtask cargo test --locked --offline -j4 -p graphit-ast --test antlr_engine db2_method_cast_target_inferred_attributes_native
cargo xtask cargo clippy --locked --offline -j4 -p graphit-ast --lib --test antlr_engine -- -D warnings
```
## DB2 Method CAST explicit XML exclusion

The XML delta in tsk-4c23 implements the statement-specific prohibition for
Method signature and Methodspec target/source. method_cast_non_xml_builtin_type
copies the mature global builtin alternatives except XML;
method_cast_non_xml_type preserves the other current generic alternatives.
method_cast_target_type consumes this type or the delivered inference helper.
Both CAST source positions consume method_cast_source_type; the unused
data_type_4 alias is removed. Global datatype rules, active external-scalar
data_type_3, ordinary returns, parameters and locator placement stay unchanged.

[Native capture paths](rust_ast_antlr_queries.md#db2-method-cast-target-paths)
reflect actual generated getters and parents. The
[primary contract](rust_ast_antlr_profile_reconciliation.md#explicit-xml-correction-and-remaining-nominal-syntax)
keeps nominal/REF syntax and catalog classification separate from this XML
correction. No new nominal form or server validity is inferred.

## DB2 shared FUNCTION zero arguments

`tsk-a72c` permits `()` in complete OLE DB, sourced and TEMPLATE FUNCTION headers
by making exactly two `param_decl_list_3` caller references optional. The mature
list still has one or more declarations, and its optional-name/type/default
slot from `tsk-6375` is unchanged. Empty headers have no native list, declaration,
parameter name or default clause; the three existing name/type/default queries
return zero matches. No empty node or Parameter is fabricated. Existing getters
and rule ordinals remain; the list getter can now be absent on these callers.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1223, explicitly allows
zero OLE DB parameters and requires parentheses. Its physical-page-1220 diagram
shows at least one declaration; this text/diagram divergence is retained rather
than hidden. Sourced/TEMPLATE physical-page-1229 diagrams bypass declarations,
and physical-page-1231 text likewise allows zero with mandatory parentheses.
These are syntax contracts, not executed provider/catalog/SOURCE evidence.

The new fixture `db2_shared_function_zero_arguments_native` covers the three
complete headers with empty or hidden-comment-only parentheses, UTF-8/CRLF,
raw tokens/EOF and no list/slot/captures. Named and unnamed nonempty slots retain
raw ordered names, types, defaults and parents; existing OLE character/graphic
and sourced/TEMPLATE single-ID/quoted nominal domains are preserved. The old
`tsk-6375` empty-list negative is replaced only for this causal delta. Missing
parentheses, malformed lists/defaults/directions and SQL unnamed slots remain
negatives. Other qualified headers, SDK recovery versus strict errors, original
cancellation/deadline and four concurrent parses remain exercised.

Only the two caller references change; global types, both Method CAST targets,
other routine lists and the public Rust/cxx ABI remain unchanged. Qualified
nominal support, contextual type/default restrictions, full options/returns,
aggregate, X and shipped Parameter metadata remain necessary work.

One official Parser Tool 4.13.2 invocation passed in 2.10 seconds without
diagnostics; four published Parser outputs match its raw
`.build/ast-db2-shared-function-zero-arguments-generated` stage and twelve
Lexer/Unicode outputs are unchanged. The focal passed 1/1 in 0.19 seconds
(Cargo 37.88 seconds); eighteen exact affected DB2 filters passed
18/18 in 2.37 seconds (Cargo 95.20 seconds), and strict Clippy
passed in 50.86 seconds. Format, authored differences,
documentation whitespace and local links/anchors passed. Windows/macOS are unrun.
The initial focal failed on a newly authored cursor control without its required
HOLD clause; its 62.12-second Cargo result and full log remain in the separate
`.build/ast-db2-shared-function-zero-arguments-focal-initial-fail-result.json`
`receipt`. Only that new control changed to a previously qualified cursor
constructor; grammar and Tool output were not regenerated. Successful logs use
exclusive paths. The earlier focal-log collision is recorded explicitly in
`.build/ast-db2-shared-function-zero-arguments-log-provenance.json`. Native selectors and list absence are detailed
in the [query contract](rust_ast_antlr_queries.md#db2-shared-function-zero-argument-paths).

```text
cargo xtask cargo test --locked --offline -j4 -p graphit-ast --test antlr_engine db2_shared_function_zero_arguments_native
cargo xtask cargo clippy --locked --offline -j4 -p graphit-ast --lib --test antlr_engine -- -D warnings
```

## DB2 external-table FUNCTION zero arguments

`tsk-c72f` makes exactly one `ext_table_param_decl_list` caller reference optional
in `create_function_external_table_statement`. Complete external-table FUNCTION
headers now admit `()`; the existing list still has one or more slots when
present. Optional parameter names, native types/defaults/locators, returns,
options and all other headers remain unchanged. No context, rule ordinal or
public Rust/cxx ABI is added; the existing list getter can be absent.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1199 (printed 1167), has a
list bypass; physical page 1204 (printed 1172) explicitly permits zero arguments
with mandatory parentheses. This establishes syntax, without provider/catalog,
default assignment or server execution proof. This independent arity delta does
not qualify full types, returns/options or shipped Parameter metadata.

The new fixture `db2_external_table_function_zero_arguments_native` uses the
complete C/PARAMETER STYLE SQL/EXTERNAL NAME/NO SQL factory already qualified by
`tsk-9b9a`. Empty and hidden-comment-only parentheses produce no native list or
slot and no anchored name/type/default captures; return-table columns remain
real nodes. Nonempty named, unnamed and mixed parameters preserve ordered raw
values and parents. Missing parentheses, malformed commas/defaults and SQL
unnamed slots remain negatives. Other qualified headers, original UTF-8/CRLF,
comments/raw tokens/EOF, recovery versus strict errors, cancellation/deadline and
four concurrent parses are checked. Existing fixtures are preserved literally.

One official Parser Tool 4.13.2 invocation passed in 2.30 seconds without
diagnostics; four published Parser outputs match the raw
`.build/ast-db2-external-table-function-zero-arguments-generated` stage and twelve
Lexer/Unicode outputs remain unchanged. The focal passed 1/1 in 0.18 seconds
(Cargo 69.33 seconds); nineteen exact affected DB2 filters passed
19/19 in 2.44 seconds (Cargo 158.05 seconds), and strict Clippy
passed in 230.31 seconds. Format, authored differences,
documentation whitespace and local links/anchors passed. Windows/macOS are unrun. See the
[native query contract](rust_ast_antlr_queries.md#db2-external-table-zero-argument-paths).

```text
cargo xtask cargo test --locked --offline -j4 -p graphit-ast --test antlr_engine db2_external_table_function_zero_arguments_native
cargo xtask cargo clippy --locked --offline -j4 -p graphit-ast --lib --test antlr_engine -- -D warnings
```

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

## DB2 SQL FUNCTION zero arguments

`tsk-106e` makes exactly one `param_decl_list_2` caller reference optional in
`create_function_sql_scalar_table_or_row_statement`. Complete SQL FUNCTION headers
now admit `()`. The list still contains one or more declarations when present;
every nonempty SQL slot still requires its parameter name. Direction, type,
default, returns, options, body and other FUNCTION headers remain unchanged.
The existing list getter can be absent; no context, rule ordinal or public
Rust/cxx ABI is added.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1241 (printed 1209), has the
list bypass; physical page 1246 (printed 1214) permits zero parameters with
mandatory parentheses. Physical page 1247 preserves required SQL parameter
names. These establish syntax, without server, provider/catalog, default
assignment or complete contextual type/return/options qualification.

The new `db2_sql_function_zero_arguments_native` fixture reuses the qualified
`tsk-9b9a` scalar INTEGER/LANGUAGE SQL/BEGIN ATOMIC factory. Empty and hidden-comment
headers have no native parameter list, declaration or anchored name/type/default
captures; the real return type and SQL body remain nodes. Nonempty required names,
optional IN, raw types/defaults, parents and order are preserved. Unnamed SQL
slots remain negatives, while the already qualified unnamed external-table
control remains positive without matching these selectors. Mandatory parentheses,
malformed commas/defaults, other-header isolation, original UTF-8/CRLF/comments,
raw tokens/EOF, recovery versus strict errors, cancellation/deadline and four
concurrent parses are checked by the new fixture. All older fixtures are literal.

One official Parser Tool 4.13.2 invocation passed in 2.50 seconds without
diagnostics. Four published Parser outputs match the raw
`.build/ast-db2-sql-function-zero-arguments-generated` stage; twelve Lexer/Unicode
outputs remain unchanged. The focal passed 1/1 in 0.29 seconds
(Cargo 262.50 seconds); twenty exact existing affected DB2 filters
passed 20/20 in 2.27 seconds (Cargo 271.96 seconds). Strict Clippy
passed in 65.12 seconds. Cargo durations include any observed
shared build-directory lock waits. Format, authored differences, documentation
whitespace and local links/anchors passed. Windows/macOS are unrun. See the
[native query contract](rust_ast_antlr_queries.md#db2-sql-zero-argument-paths).

```text
cargo xtask cargo test --locked --offline -j4 -p graphit-ast --test antlr_engine db2_sql_function_zero_arguments_native
cargo xtask cargo clippy --locked --offline -j4 -p graphit-ast --lib --test antlr_engine -- -D warnings
```

## DB2 external scalar FUNCTION zero arguments

`tsk-e3df` makes only the existing inline parameter group optional in
`create_function_external_scalar_statement`. Complete external scalar FUNCTION
headers admit `()` with mandatory parentheses. The declaration rule, optional
names/direction, types/defaults, returns/options and active `data_type_3` CAST
branch remain literal existing Source. The existing declaration-vector getter
is empty; no new list context, helper, rule ordinal or public Rust/cxx ABI exists.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1172 (printed 1140), shows
the parameter-group bypass; page 1178 (printed 1146) permits zero parameters and
requires parentheses. This establishes syntax, without provider/catalog, server,
conversion/default assignment or full contextual type/return/options validation.

The new `db2_external_scalar_function_zero_arguments_native` fixture reuses the
qualified `tsk-20e3` INTEGER/LANGUAGE C/PARAMETER STYLE SQL/EXTERNAL NAME/NO SQL
factory. Empty and hidden-comment headers contain no parameter declaration,
name/type/default or parameter captures; return type and options remain nodes.
Named and unnamed nonempty declarations preserve raw captures/order and direct
caller parents. Mandatory parentheses, malformed slots/defaults, qualified other
headers, raw UTF-8/CRLF/comments/tokens/EOF, recovery versus strict errors,
cancellation/deadline and four concurrent empty/typed parses are checked. A
nominal type without a parameter name is not treated as a missing-type error.
All older fixtures remain literal.

One official Parser Tool 4.13.2 invocation passed in 24.21 seconds
without diagnostics. Four published Parser outputs match the raw
`.build/ast-db2-external-scalar-function-zero-arguments-generated` stage;
twelve Lexer/Unicode outputs remain unchanged. The focal passed 1/1 in 0.18 seconds (Cargo 284.21 seconds);
twenty-one exact existing affected DB2 filters passed 21/21 in 2.66 seconds
(Cargo 54.56 seconds). Strict Clippy passed in 94.87 seconds.
Cargo durations include observed shared build-directory lock waits. Format,
authored differences, documentation whitespace and local links/anchors passed.
Windows/macOS are unrun. See the
[native query contract](rust_ast_antlr_queries.md#db2-external-scalar-zero-argument-paths).

```text
cargo xtask cargo test --locked --offline -j4 -p graphit-ast --test antlr_engine db2_external_scalar_function_zero_arguments_native
cargo xtask cargo clippy --locked --offline -j4 -p graphit-ast --lib --test antlr_engine -- -D warnings
```

## DB2 external scalar parameter AS LOCATOR

`tsk-ddbd` appends only `as_locator?` after `routine_default_clause?` in
`ext_scalar_param_decl`. The existing ANTLR `as_locator: AS LOCATOR` helper is
reused. Native `as_locator` is a direct child of the parameter declaration;
the official Tool adds its getter to that local context. No new rule ordinal,
helper, public Rust/cxx ABI or shared query/build interface is introduced.
Names/direction, types/defaults, optional outer arity, RETURNS/CAST/options and
all other headers preserve their existing grammar. Without the modifier, no
locator node or capture is synthesized.

IBM LUW 11.5 SQL Reference (2023-02-07), physical page 1173 (printed 1141), places
AS LOCATOR after the optional default. Page 1180 (printed 1148) describes LOB or
distinct-over-LOB applicability, the FENCED plus NO SQL restriction and unchanged
signature/promotion. Those type/catalog/context restrictions require semantic
validation; this parser composition does not classify types by spelling. The
new positives use LOB types with explicit or implicit IN, NOT FENCED and no
PREDICATES. Server execution, distinct-type catalogs, OUT/INOUT default legality
and full contextual types/options are not qualified by this leaf.

The new `db2_external_scalar_parameter_locator_native` fixture checks complete
scalar C headers for named CLOB/default NULL, unnamed BLOB, quoted UTF-8 DBCLOB
with raw intervening comments and mixed locator/ordinary slots. Q0 default,
Q1 name and Q2 type preserve raw captures/order/parents; Q3 selects the exact
`AS ... LOCATOR` span under its slot, including hidden comments. No-locator and
empty-header controls yield zero Q3. Wrong position, repetition and incomplete
modifiers are strict syntax errors; qualified Method return locators and other
headers do not become scalar parameter locators. Raw comments/tokens/EOF,
recovery/strict errors, original cancellation/deadline and four concurrent
empty/locator parses are checked. All older fixtures remain literal.

One official Parser Tool 4.13.2 invocation passed in 2.09 seconds
without diagnostics. Four published Parser outputs match the raw
`.build/ast-db2-external-scalar-parameter-locator-generated` stage; twelve
Lexer/Unicode outputs remain unchanged. The focal passed 1/1 in 0.18 seconds (Cargo 449.34 seconds);
twenty-two exact existing affected DB2 filters passed 22/22 in 2.55 seconds
(Cargo 131.48 seconds). Strict Clippy passed in 63.82 seconds.
Cargo durations include any observed shared build-directory lock waits. Format,
authored differences, documentation whitespace and local links/anchors passed.
Windows/macOS are unrun. See the
[native query contract](rust_ast_antlr_queries.md#db2-external-scalar-parameter-locator-path).

```text
cargo xtask cargo test --locked --offline -j4 -p graphit-ast --test antlr_engine db2_external_scalar_parameter_locator_native
cargo xtask cargo clippy --locked --offline -j4 -p graphit-ast --lib --test antlr_engine -- -D warnings
```

## DB2 SQL FUNCTION typed ROW and TABLE returns

`tsk-1b14` replaces only the SQL FUNCTION caller's two names-only return-list
references with typed lists. TABLE requires one or more columns; ROW requires
two or more. Parentheses and each column's name and type are required. The
global `column_name_list`, scalar return, parameter/default/empty-header grammar,
options/body and other TABLE alternatives remain unchanged.

Six local, consumed ANTLR helpers expose the SQL return lists, column, declared
type, builtin and neutral nominal type. Their binding comes from IBM LUW 11.5
SQL Reference (2023-02-07), physical pages 1243–1244 (printed 1211–1212).
The builtin wrapper reuses `tsk-4eda`'s complete `method_builtin_type`, including
XML, DB2SECURITYLABEL and the local 0/1 attribute helper, plus native CURSOR.
The nominal wrapper reuses `method_named_type` for one or two raw components;
REF has its own explicit shape. Methodspec XML exclusions are not copied to SQL.
Global `data_type`, `data_type_1`, `data_type_3`, CREATE TABLE definitions and
inferred empty attributes do not establish this domain.

Physical pages 1248–1249 retain schema/SQL-path and catalog restrictions;
page 1249 establishes TABLE/ROW cardinalities. Distinct and structured names
are accepted through a neutral nominal path, without classifying a catalog by
spelling. Structured types are included by the diagram. Builtin CURSOR differs
from cursor-type-name; recognition does not claim every contextual use is valid.
Catalog type classes, assignability, numeric limits, SQL path, transform usage,
provider/server execution and full scalar/parameter/options validation remain
separate requirements.

The new `db2_sql_function_typed_returns_native` fixture uses complete SQL bodies
with RETURN fullselect. Basic TABLE and ROW statements use SRC_DUMMY_F (one row);
all table-reading controls use READS SQL DATA;
nominal/REF and alias controls declare SRC_TYPES_F catalog preconditions, and
ROW controls declare a structured parameter and transform-use precondition.
These preconditions are not proved by the parser. It checks single/two/three
column cardinalities, the qualified `tsk-4eda` type array plus XML/CURSOR and
quoted nominal/REF names, ordered raw captures and direct column/list parents.
Malformed lists/types, other-header isolation, UTF-8/CRLF/hidden comments/raw
tokens/EOF, recovery/strict errors and original operations/concurrency are
covered without changing older fixtures.

One official Parser Tool 4.13.2 invocation passed in 2.06 seconds
without diagnostics. Four published Parser outputs match the raw
`.build/ast-db2-sql-function-typed-returns-generated` stage; twelve Lexer/Unicode
outputs remain unchanged. The corrected focal passed 1/1 in 1.50 seconds (Cargo 5.0 seconds);
twenty-three exact existing affected DB2 filters passed 23/23 in 2.24 seconds
(Cargo 2.58 seconds). Strict Clippy passed in 97.03 seconds.
Format, authored differences, documentation whitespace and local links/anchors
passed. The initial qualified-table failure and its full log remain preserved.
Windows/macOS are unrun. The
[native query contract](rust_ast_antlr_queries.md#db2-sql-typed-return-column-paths)
describes the new local getters and parents.

```text
cargo xtask cargo test --locked --offline -j4 -p graphit-ast --test antlr_engine db2_sql_function_typed_returns_native
cargo xtask cargo clippy --locked --offline -j4 -p graphit-ast --lib --test antlr_engine -- -D warnings
```

The initial focal failed at FROM SYSIBM.SYSDUMMY1 (101, 0.02 seconds; Cargo
92.56 seconds). The preserved initial full log records the qualified-table
limitation. Primary page 1255 uses unqualified EMPLOYEE and READS SQL DATA; the
corrected derived controls use SRC_DUMMY_F with one-row catalog precondition
and READS SQL DATA, including ROW controls that read a table. No table-name,
fullselect or direct-RETURN grammar was changed by that unit. Those controls
did not qualify QName or VALUES/RETURN. Ordinary FROM QName is addressed by
`tsk-6012` below; the independent `tsk-cf2d` contract is implemented by `tsk-7f03`.

## DB2 ordinary FROM table QNames

Ordinary FROM accepts one identifier, a local schema-qualified name, or a
three-component remote object name. IBM LUW 11.5 describes identifiers and
qualification on printed pages 4–10 and local/remote references on pages
649–650 of the [SQL Reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_reference_115.pdf)
(physical pages are printed numbers plus 32). A dot inside a quoted identifier
belongs to that component; doubled quotes preserve the original spelling.

`tsk-6012` adds the caller-local `table_reference_name` below ordinary
`singles_table_reference`, reusing existing identifier/schema/data-source/table
helpers. The local `table_reference_schema` component also admits the documented
SESSION schema token without broadening shared identifier rules. Existing
temporal, correlation and sampling suffixes stay in their
caller. The official C++ Tool 4.13.2 generates only Parser4; global
`table_name`, `remote_object_name`, lexer rules and public Rust/cxx ABI remain
unchanged.

`db2_fullselect_table_qnames_native` exercises eleven complete SELECTs, seven
malformed qualifiers and a complete typed TABLE SQL FUNCTION with
`BEGIN ATOMIC RETURN SELECT P_F FROM SYSIBM.SYSDUMMY1; END`. It checks raw
QName/component spans, quotes/comments, UTF-8/CRLF, selected and ancestor
captures, parents, shipped final-name selection and recovery bounds. This
inlined compound/RETURN/subselect route already exists and does not depend on
the separate direct-RETURN/VALUES work.

Full raw QName is available through the wrapper. The existing shipped SELECTS
query still captures the final component; schema-aware relationship identity
is a separate consumer requirement. Catalog existence and table/view/nickname/
alias/CTE resolution are not classified by spelling. Qualified names in DDL/DML,
ONLY, OUTER or ANALYZE are outside this binding. See
[selectors](rust_ast_antlr_queries.md#db2-from-qname-selectors) and
[scope reconciliation](rust_ast_antlr_profile_reconciliation.md#ordinary-from-qualified-reference-binding).

Affected checks:

```text
cargo xtask cargo test --locked --offline -j1 -p graphit-ast --test antlr_engine db2_fullselect_table_qnames_native
cargo xtask cargo test --locked --offline -j1 -p graphit-ast --test antlr_engine db2_sql_function_typed_returns_native
cargo xtask cargo test --locked --offline -j1 -p graphit-ast --test antlr_engine five_real_recognizers_accept_statements_and_reject_broken_syntax
cargo xtask cargo test --locked --offline -j1 -p graphit-ast --test antlr_engine one_initial_bom_preserves_original_sdk_byte_positions
cargo xtask cargo clippy --locked --offline -j1 -p graphit-ast --lib --test antlr_engine -- -D warnings
```

All four affected ANTLR functions and scoped strict Clippy passed. The first
QName run exposed the reserved SESSION schema token; its failure log is kept,
and the corrected focal retains that positive case. A broad all-test Clippy
attempt found an unrelated stale finite-deadline fixture in source inspection.
That fixture now uses the captured Original's `required_deadline()`; only
`identical_parameters_do_not_restore_original_request_identity` was run for
that correction and passed. No deadline or cancellation value was synthesized.
Five distinct native test functions passed in this delivery; failed attempts
and reruns are not added to that count. Windows/macOS were not run.

## DB2 VALUES fullselect and direct SQL RETURN

`tsk-7f03` connects the initial VALUES and grouped-fullselect alternatives and
routes a direct SQL FUNCTION body through the existing `return_statement`.
The shared ORDER BY rule now consumes its required `ORDER BY` prefix for both
sort-list and INPUT SEQUENCE alternatives. Existing expressions, VALUES rows,
RETURN payloads, set operators and pagination/isolation helpers are reused.
The existing CTE helper now requires a fullselect inside `AS (...)`, instead of
accepting an empty definition. Nested WITH uses the existing CTE list.
CREATE VIEW now requires its existing definition's missing `AS` token. The
fullselect, optional WITH list and view options retain their existing helpers.

IBM LUW 11.5 [SQL Reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_reference_115.pdf)
describes fullselect and VALUES at physical pages 742–744, SQL bodies at 1245,
ROW/TABLE examples at 1255 and RETURN at 1801–1802. These bindings validate
syntax; catalog objects, types, assignments, row/column cardinality, transforms
and SQL access classes still require contextual/server validation.
Physical page 748 (printed 716) requires the CTE fullselect and documents nested
WITH for Db2 11.5 Mod Pack 2 and later. Multiple definitions follow one WITH;
recognition does not validate recursive references or catalog dependencies.
Physical page 1572 (printed 1540) confirms the mandatory CREATE VIEW `AS`.

The native focal includes scalar expression/NULL, ROW/TABLE VALUES, SELECT,
inlined compound, CTE and scalar subquery function bodies. It also covers
standalone/grouped VALUES, UNION/EXCEPT/INTERSECT, CREATE VIEW, raw UTF-8/CRLF
comments and quoted columns, ORDER BY/OFFSET/FETCH/WITH UR, malformed syntax
and the original cancellation/deadline. Shared SELECT and DELETE order callers
are included because the prefix corrects their existing helper too.
The CTE cases cover direct RETURN, nested definitions, an ordinary SELECT caller
and rejection of empty `AS ()`.
View controls include VALUES and SELECT definitions, and reject a missing AS.

The current QName/SESSION binding is preserved. The shared RETURN helper keeps
its optional payload; recognizing bare RETURN does not establish its legality
as a FUNCTION body. Explicit ROW constructors and semantic-only
invalidations are outside this binding. No profile or Rust/cxx ABI change is
needed. See [selectors](rust_ast_antlr_queries.md#db2-direct-return-and-values-selectors)
and [reconciliation](rust_ast_antlr_profile_reconciliation.md#values-and-direct-return-binding).

Linux qualification passed the new focal (1/1, 0.36 seconds), eight exact affected
regressions and strict Clippy for the library and `antlr_engine` target. Official
ANTLR 4.13.2 regeneration completed without diagnostics; the four published Parser
outputs equal the final stage. Format, authored whitespace and section links
passed. The earlier CTE and view failures remain recorded; their required cases
were preserved and corrected. Windows/macOS and SQL server execution are unrun.

## DB2 aggregate interface recognition

Product `tsk-2558` implements the native aggregate interface through its existing
direct statement alternative. The parameter list may be empty; parameter names
are optional, and IN requires a name. State declarations have at least one
builtin type, optional names and no DEFAULT. Dedicated routine defaults reuse
the already delivered helper.

Both RETURNS and AGGREGATE WITH/USING are mandatory and may exchange order. The
implementation steps remain INITIALIZE/ACCUMULATE/MERGE procedures followed by
the FINALIZE function, using the existing native designators and module name.
Eight option families may appear in any order without repetition. Their eight
flags live only on the ANTLR option-rule context; duplicate or contradictory
variants produce ordinary SDK parser diagnostics. No global state is retained.

The dedicated aggregate builtin subset preserves integer, numeric, character,
graphic, binary, datetime, XML and security-label syntax. It excludes BOOLEAN,
CURSOR and nominal/anchor/ARRAY/REF alternatives without changing general or
Method type rules. Empty attributes use the mature inferred-builtin helper only
at a CAST target; FLOAT() and empty source/ordinary attributes remain invalid.
Precision, catalog, conversion and host restrictions remain server conditions.

The closed [LUW 11.5 primary reference](https://public.dhe.ibm.com/ps/products/db2/info/vr115/pdf/en_US/db2_sql_reference_115.pdf),
physical pages 1157–1159 and 1161–1162, supports these bindings. The Linux focal
passed, including two definitions in one input to verify recognition-local option
state. Nine affected native regressions and strict library/engine-fixture Clippy
passed. Official ANTLR 4.13.2 generated the four Parser outputs without diagnostics;
they match its output unchanged. The earlier dependency-resolver failure remains
recorded. Windows/macOS and SQL server behavior remain unqualified.
See the [native selectors](rust_ast_antlr_queries.md#db2-aggregate-interface-selectors).

## COBOL native COPY directive parents

The maintained main `Cobol85.g4` now recognizes `copyDirective` in data entries,
between data sections, in FILE section lists and in procedure/paragraph bodies.
The shape adapts the existing canonical preprocessor COPY/library/directory/
family/SUPPRESS/REPLACING productions to the main token vocabulary. Main newlines
are hidden and its directive terminator is `DOT_FS`. Existing `EQUALCHAR` pairs
form pseudo-text delimiters only when their actual token bounds are adjacent;
a single interior equal sign remains text. Numeric signs and non-COPY quoted
literal prefixes remain rejected in those directive fields. This adds no lexer
tokens or filesystem/replacement execution.

The directive's COPY terminal has real native parents through `programUnit`.
The [specific consumer](rust_ast_antlr_semantics.md#cobol-copy-observations-and-native-program-scopes)
still obtains COPY query data from the official preprocessor tree and uses this
canonical token only to link the actual main program/context chain. Empty,
malformed or unresolved observations do not acquire guessed parents.

Only Main `Cobol85Parser.cpp/.h` and `Cobol85.interp/.tokens` were regenerated
with the captured official ANTLR 4.13.2 Tool, standard C++/UTF-8/package options
and listeners/visitors disabled. Lexer outputs remain byte-identical and were
not replaced. The first absolute-path invocation altered only generated comment
prefixes and published nothing; the corrected relative grammar invocation
preserves the existing generation recipe. Preprocessor and other dialect outputs
remain unchanged.

`tests/antlr_cobol_copy.rs` retains the failing stock/physical/multiple/nested
parent fixtures and adds strict data/procedure/paragraph/FILE COPY controls with
quoted/library/SUPPRESS/REPLACING/pseudo-text/filename observations and malformed
source/library/BY/replacement/terminator/delimiter negatives. Nine main/COPY tests,
causal source/TS/COB regressions and scoped strict Clippy passed on Linux. Earlier
real failures are retained in the [consumer qualification](rust_ast_antlr_semantics.md#cobol-copy-observations-and-native-program-scopes).
Full S7, graph publication and Windows/macOS remain separate.

## DB2 cursor Type AS and declared Type bindings

The DT delta in `tsk-6eae` adds the mandatory AS keyword only to `create_type_cursor_statement`, preserving the optional existing row helpers. The stock consumer selects declared names separately from each of the five CREATE TYPE branches and configures one named branch Type frame per declaration. The generic wrapper remains native syntax. [Capture paths](rust_ast_antlr_queries.md#db2-type-declaration-paths-and-contexts) and [primary scope](rust_ast_antlr_profile_reconciliation.md#db2-declared-type-binding-correction) separate this correction from full Type grammar and the semantic model. Linux actual; Windows/macOS and SQL server execution remain unrun.

## DB2 optional array cardinality and CREATE TABLE delimiters

The CT delta in tsk-6eae makes only the existing ARRAY bracket contents optional
and requires parentheses around the element-list branch of CREATE TABLE. Its
final table options allow zero occurrences. The brackets, table alternatives,
column helper, content types, name nodes and native parent chains remain intact.
A bare column list without parentheses is rejected; no invalid legacy alternative
or source normalization is retained.

## DB2 Variable subjects, declared Alias names and inlined entrypoint

The VA delta in tsk-6eae factors DECLARE through existing variable_name:id_ and
wraps only the declared first Alias name in each subtype. It additionally exposes
the existing compound_sql_inlined helper at sql_statement top-level after actual
BEGIN ATOMIC baseline diagnostics showed the missing caller. Helpers, bodies,
compiled/embedded SQL routes and function/procedure callers are preserved.
Native ANTLR prediction distinguishes BEGIN DECLARE SECTION; no keyword dispatch
or SQL normalization substitutes for the parser.

Official ANTLR4.13.2 generated the raw Parser outputs once; twelve Lexer/Unicode artifacts remain unchanged. Subject captures and shared declaration extents are specified in [query paths](rust_ast_antlr_queries.md#db2-variable-and-alias-declared-subject-paths). This native correction does not deliver the Db2 semantic model, QName or Index consumer domain.

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

## Method CAST reference constructor

The DB2 grammar adds the genuine `method_reference_type` context
(`REF '(' method_named_type ')'`) to `method_cast_non_xml_type`. Both
`method_cast_target_type` and `method_cast_source_type` consume it in CREATE
METHOD and CREATE/ALTER TYPE Methodspec. The nominal helper retains its current
optional schema and unqualified name; existing ordinary returns and parameter
constructors are unchanged. Lexer and Rust/cxx ABI do not change.

The parser retains syntax, native parentage, raw ranges and diagnostics. Nominal catalog classes, existence and castability are server concerns, with no parser classification by spelling.

## DB2 MODULE object definitions

ALTER MODULE ADD/PUBLISH uses native module_type_definition and
module_variable_definition bodies. Type names are unqualified native
unqualified_type_name; variables use the single variable_name child.
ARRAY, CURSOR, DISTINCT and ROW retain the mature CREATE body domains;
structured Types and OR REPLACE DISTINCT reject. Variables retain written
data_type_1 and optional DEFAULT/CONSTANT initialization. Written initialization
requires a value; omitted initialization creates no NULL node. The global CREATE
VARIABLE caller has the same optional initialization boundary. Cursor sensitivity
and holdability are optional real branches; top-level SET reaches the constructor,
while compound-body assignment has a separate expression path.

The four anchored datatype helpers retain original 1–3-component names, reusing
the existing qualified reference rule and schema/table/column primitives. ROW
anchors accept optional OF as in the IBM diagram. These names remain lexical
observations: schema/server/module qualification does not classify an anchor or
resolve its catalog object. Incomplete or four-component anchors reject.
CHECK reuses search_condition and the existing expression branch; VALUE enters
the mature contextual identifier route without Lexer token replacement.
Datatype-dependent constraint validity remains a server concern.
The existing size-unit K/M/G tokens also work as contextual identifiers without
changing their numeric unit positions. Datatype/array integer atoms accept the
existing L_ONE/L_ZERO tokens alongside DECIMAL_LITERAL; source1/0, native getters
and original ranges remain explicit. No decimal parser or token rewrite is used.

## Db2 external scalar ordinary return types

`tsk-2526` composes the ordinary result of external scalar FUNCTION through
`external_scalar_ordinary_return_type`. It reuses `data_type`,
`method_reference_type` and `procedure_external_xml_type`; the shared
`data_type_2` definition and the CAST branch are unchanged.

    CREATE FUNCTION APP.F_RET(P_F INTEGER) RETURNS XML AS CLOB(2M)
    LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;

Builtins, opaque nominal names and REF retain their existing grammar. XML's
CLOB representation has a native wrapper and length/unit children. Existing
anchor and bare XML recognition is retained without proving server legality.
An ordinary AS LOCATOR is a direct statement child after the complete type;
LOB/catalog/language conditions remain server rules.

The new wrapper is exposed directly, without an old ordinary `data_type_2` alias. CAST target inference and its separate source remain unchanged. [Native selectors](rust_ast_antlr_queries.md#db2-external-scalar-ordinary-return-type-paths), [model observations](rust_ast_antlr_semantics.md#db2-native-ordinary-external-return-observations) and the [primary contract](rust_ast_antlr_profile_reconciliation.md#external-scalar-ordinary-return-type-composition) describe the boundary.
