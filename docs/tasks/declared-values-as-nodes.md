The declared value becomes a node in every grammar, not just data formats.

Continuation of [data-format-key-value-nodes](data-format-key-value-nodes.md), which made keys and values turn into nodes in XML, JSON, YAML, TOML, HTML, and HCL. The same rule applies to programming languages as well as the remaining data formats.

## Problema

A constant stated that there was only existence and nothing else. `const Endpoint =
"https://api.acme.com/v2"` — INLINE_0 _Constant _INLINE_1 _Endpoint`, and the URL was nowhere to be found—neither as a node, nor as a property, nor in any index of search. "Who holds this endpoint," "what flag is linked," "which column comes with this status" had no answer.

The audit of the 42 grammars also found four defects.

### 1. Nenhuma linguagem capturava valor

Before this change, `value_capture` appeared in 6 query files — all of them in data format. The declared value was discarded without exception for the remaining 36 grammars: `variables`, `constants`, `fields`, `properties`, and `enum_values` only stored the name.

2. Members of the enum did not exist

TypeScript, TSX, C#, Kotlin, and C/C++ indexed the enum and none of its members.
____INLINE_8__ produced a node named ____INLINE_9__; neither __INLINE_10__ nor __INLINE_11__ existed.

The adapter ANTLR ignored `value_capture` completely

Inline 14, Inline 15, Inline 16, and Inline 17 read the YAML field but did nothing with it. An inline in a column and a clause Inline 20 in COBOL— which is how COBOL declares a constant— were parsed and thrown away.

Broken patterns were silent no-operation ops.

A pattern that does not compile is discarded with a log warning and nothing more: the entities it should have produced simply do not appear, and none of the tests fail. Seven of these patterns written in this task entered without being noticed before the compilation test existed — `nullptr` in C++, `null_literal` in Kotlin, `symbol` in Ruby, `nil` in Swift, `true` in Groovy. All of them would have passed unnoticed.

Solution

### Motor: `value_capture` no ANTLR

On the side of tree-sitter `value_capture` is a capture name. On the side of ANTLR, a match is
a subtree, not a list of captures, so `value_capture` is a **rule path** separated by `/`,
resolved from the matching node:

```yaml
- data_key: variables
  graph_label: Variable
  pattern: "//variable_declaration"
  name_capture: identifier
  value_capture: "default_value_part/expression"
  value_label: Value
```

Each segment is a direct **child** of purpose. `column_definition` in Oracle is

```
column_name (datatype)? ... (DEFAULT expression | identity_clause)? inline_constraint+
```

Then the DEFAULT's _INLINE_31_ is a child of the column, while the CHECK's _INLINE_32_ is buried underneath it. A descendant search finds both, and the first version of this change reports _INLINE_33_ as a column whose default is _INLINE_34_. Where the descendant search really matters, there is segment _INLINE_35___.

Motor: Pairs of delimiters exit.

The inline code passed the removal of married pairs in loops and accepted backticks. Thus,
`__INLINE_37_raw__INLINE_38___` do Go, o `` `tpl` `` do JavaScript e o `"""abc"""` from Python arrives clean, while __INLINE_43__ retains the quote that is its — a trim by character set would eat the inner quote.

Grammars: Twenty-three languages now have value nodes

Go, Python, TypeScript, TSX, JavaScript, Java, C#, Rust, Ruby, PHP, Bash, C, C++,
Kotlin, Swift, Scala, Dart, Lua, Zig, R, Julia, Groovy, Dockerfile, Protobuf,
PL/SQL, T-SQL, COBOL 85.

Only **literal** values are captured. Any expression is not the name of anything, and indexing it costs more than it yields. The label is __INLINE_44__, the same as data formats: uniforming the graph is worth more than precision in calling it `Literal` which in grammar is literal and elsewhere is scalar.

Note: Inline labels are typically used for quick reference or to group related items together, but they should be kept minimal to avoid clutter.

Gains that were not about variables:

Translation:

| Grammar | What has existed |
|---|---|
| TypeScript, TSX, C#, Kotlin, C, C++ | members of enum with their values |
| Dockerfile | value of `ENV`, default of `ARG`, value of `LABEL` |
| Protobuf | field number and enum value — the wire contract |
| Scala | top-level `val` (previously only `class_parameter`) |
| Dart | top-level `static_final_declaration` |
| COBOL 85 | clause `VALUE` |
| XML | CDATA |
| JSON | arrays of number and boolean |
| YAML | flow sequences (`[a, b]`) |
| TOML | inline tables and non-string arrays |
| HCL | heredoc, list items, object elements |

Test: No pattern can fail in silence

Inline 54 compiles the **540 patterns** distributed against its own grammar for each one, ensuring that every named capture in YAML (___INLINE_55__, ___INLINE_56__, ___INLINE_57__) exists in the pattern. A typo becomes a test failure instead of an unremarked smaller graph.

Gaps: resolved later, and the one left over

The first cut of this task left four issues. Three were closed – see [grammar-gaps-css-svelte-antlr-guards](grammar-gaps-css-svelte-antlr-guards.md) – and a precise correction must be registered here:

The statement that INLINE_58 in XPath of ANTLR Inline was decorative was incorrect. It came from example `docs/specs/ast_module.md`, not the query files. In practice there are 325 INLINE_60 (which is the default and does not even consult for child search) and four simple rule names, all added to this task and all working. No query uses XPath there. The code path was unused, not broken — the documentation described something that doesn't exist.

Columns in the DB2 continue to not be extracted, and now with the root cause:
`create_table_statement` does not have any parentheses around the list of columns in place.

```
create_table_statement
    : CREATE TABLE if_not_exists? table_name ( element_list | OF … ) … create_table_opts+ ;
element_list
    : element_list_item (',' element_list_item)* ;
```

The parentheses in the first are not grouping of ANTLR, but tokens. Added to
`create_table_opts+`, which requires at least one option after the list, a common
`CREATE TABLE T (C INT)` does not match and the parser falls into error recovery. The fix is two lines in `.g4`, but the generated Go parser is versioned and the repository has no regeneration target — I don't do this blindly.

The nesting continues outside. `Pair → Pair` (JSON), `Mapping → Mapping` (YAML), and the pairs of an inline table (TOML) do not nest around the key that opens them, for the same reason as before: `entityUID` is name + context, so the same member would exist as two nodes, `f::host` and `f::inline.host`.

## Dois defeitos latentes corrigidos no caminho

Two queries can legitimately describe the same node—either by declaring it itself or by referencing another to obtain its value because no single standard says "the name and the value if there is one." This exposed:

Duplicate entity. Three Oracle columns arrived as six entities `Column`.
`ParsedFile.AddOrMergeEntity` completes an existing registered entity instead of appending another. Identity is a label + name + context + line: the name and line alone would fuse the two `1` of `{"a": 1, "b": 1}`, which are different key values.

---

**Note:** The code block format has been preserved as requested.

Duplicate Edge. Two edges __INLINE_75__ came in as four. The graph writer emits one line per edge, so repeating was not an NO-OP (no-op): it was a second identical edge in the database. `ConvertToCache` now deduplicates the edges CONTAINS from the file.

The two had already been in the previous commit — where an element was being used in two queries — and they passed because that commit's tests checked for existence, not count.

Verification

- `TestEveryShippedQueryPatternCompiles` — 540 patterns, 0 failures
- `TestDeclaredValuesBecomeNodes` — 23 subtests, one per language, verifying the value node and property `value` in the key
- `TestAntlrDeclaredDefaultsBecomeNodes` — PL/SQL, T-SQL, COBOL85, including the rejection of CHECK as default
- `TestEnumMembersAreNodes` — TypeScript, Kotlin, C#
- `TestDataFormatCollectionsAndCDATA` — arrays, inline tables, flow sequences, CDATA
- `TestValueDelimitersAreStrippedPerPair` — the eight cases of `dataText`

Suite `./internal/...` with `-tags fts5` passes.

## Arquivos modificados

- `internal/ast/antlr_adapter.go` — `extractValueFromMatch`, `ruleByPath`,
  `nearestDescendantByRule`, issuance of the value node
- `internal/ast/treesitter_adapter.go` — `dataText` removes pairs in loop, accepts backticks
- `internal/ast/parser.go` — `ParsedFile.AddOrMergeEntity`
- `internal/ast/cache_convert.go` — deduplicates edges CONTAINS
- `internal/ast/ladybug.go` — label `EnumMember` in the escape list
- 33 files in `internal/ast/queries/` (only file `db2.yaml` received a grammar error note)
- `internal/ast/declared_value_test.go`, `internal/ast/antlr_value_test.go` — new
- `docs/specs/ast_module.md` — `value_capture` in ANTLR


