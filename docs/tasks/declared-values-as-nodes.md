# The declared value becomes a node in every grammar, not only in data formats

Continuation of [data-format-key-value-nodes](data-format-key-value-nodes.md), which
made keys and values become nodes in XML, JSON, YAML, TOML, HTML and HCL. Here the same
rule starts to hold for programming languages and for what was left of the
data formats.

## Problem

A constant said it existed and nothing more. `const Endpoint =
"https://api.acme.com/v2"` produced a `Constant` node called `Endpoint`, and the URL
was nowhere — not as a node, not as a property, not in the search
index. "Who holds this endpoint", "which flag is on", "which column is born with
this status" had no answer.

The audit of the 42 grammars found, in addition, four defects.

### 1. No language captured a value

Before this change, `value_capture` appeared in 6 query files — all of them
data formats. In the remaining 36 grammars the declared value was discarded without
exception: `variables`, `constants`, `fields`, `properties` and `enum_values` only
kept the name.

### 2. Enum members did not exist

TypeScript, TSX, C#, Kotlin and C/C++ indexed the enum and none of its members.
`enum Status { Active = "active" }` produced an `Enum` node called `Status`; neither
`Active` nor `"active"` existed.

### 3. The ANTLR adapter ignored `value_capture` entirely

`plsql`, `tsql`, `db2`, `postgresql` and `cobol85` read the YAML field and did
nothing with it. A `DEFAULT 'ABERTO'` on a column and a `VALUE 'ABERTO'` clause in
COBOL — which is how COBOL declares a constant — were parsed and thrown away.

### 4. Broken patterns were silent no-ops

A pattern that does not compile is discarded with a log warning and nothing more: the
entities it should have produced simply do not appear, and no test
fails. Seven of the patterns written in this task got in that way before the
compilation test existed — `nullptr` in C++, `null_literal` in Kotlin, `symbol` in Ruby,
`nil` in Swift, `true` in Groovy. All of them would have gone unnoticed.

## Solution

### Engine: `value_capture` in ANTLR

On the tree-sitter side `value_capture` is a capture name. On the ANTLR side a match is
a subtree, not a list of captures, so `value_capture` is a **rule path**
separated by `/`, resolved from the matched node:

```yaml
- data_key: variables
  graph_label: Variable
  pattern: "//variable_declaration"
  name_capture: identifier
  value_capture: "default_value_part/expression"
  value_label: Value
```

Each segment is a **direct child**, on purpose. Oracle's `column_definition` is

```
column_name (datatype)? ... (DEFAULT expression | identity_clause)? inline_constraint+
```

so the `expression` of a DEFAULT is a child of the column, whereas the one of a CHECK is
buried under `inline_constraint`. A descendant search finds both, and the
first version of this change reported `VL NUMBER(12,2) CHECK (VL > 0)` as a
column whose default is `VL > 0`. Where the descendant search really is what you
want, there is the `**` segment.

### Engine: delimiters come off in pairs

`dataText` now removes matched pairs in a loop, and accepts backticks. That way Go's
`` `raw` ``, JavaScript's `` `tpl` `` and Python's `"""abc"""` arrive
clean, while `"say 'hi'"` keeps the quote that is its own — a trim by character
set would eat the inner quote.

### Grammars: 23 languages now have a value node

Go, Python, TypeScript, TSX, JavaScript, Java, C#, Rust, Ruby, PHP, Bash, C, C++,
Kotlin, Swift, Scala, Dart, Lua, Zig, R, Julia, Groovy, Dockerfile, Protobuf,
PL/SQL, T-SQL, COBOL 85.

Only **literals** are captured. An arbitrary expression is not the name of anything, and
indexing it costs more than it yields. The label is `Value`, the same as the data
formats: making the graph uniform is worth more than the precision of calling `Literal` what
in one grammar is a literal and in another is a scalar.

Gains that were not about variables:

| Grammar | What came to exist |
|---|---|
| TypeScript, TSX, C#, Kotlin, C, C++ | enum members, with their values |
| Dockerfile | value of `ENV`, default of `ARG`, value of `LABEL` |
| Protobuf | field number and enum value — the wire contract |
| Scala | top-level `val` (before, only `class_parameter`) |
| Dart | top-level `static_final_declaration` |
| COBOL 85 | `VALUE` clause |
| XML | CDATA |
| JSON | arrays of number and boolean |
| YAML | flow sequences (`[a, b]`) |
| TOML | inline tables and non-string arrays |
| HCL | heredoc, list items, object elements |

### Test: no pattern can fail silently

`TestEveryShippedQueryPatternCompiles` compiles the **540** shipped tree-sitter
patterns against each one's own grammar, and checks that every capture
named in the YAML (`name_capture`, `value_capture`, `parent_capture`) exists in the
pattern. A typo becomes a test failure instead of a silently
smaller graph.

## Gaps: resolved later, and the one that was left

The first cut of this task left four open items. Three were closed — see
[grammar-gaps-css-svelte-antlr-guards](grammar-gaps-css-svelte-antlr-guards.md) —
and one correction needs to be recorded here:

**The claim that `name_capture` in ANTLR XPath was decorative was
wrong.** It came from the example in `docs/specs/ast_module.md`, not from the query
files. In practice there are 325 `name_capture: name` (which is the default, and does not even get
to consult the child search) and four simple rule names, all added
in this task and all working. No query uses XPath there. The code path
was unused, not broken — it was the documentation that described something that does not exist.

**DB2 columns are still not extracted**, and now with the root cause:
`create_table_statement` in `Db2Parser.g4` does not have the parentheses of the column
list anywhere —

```
create_table_statement
    : CREATE TABLE if_not_exists? table_name ( element_list | OF … ) … create_table_opts+ ;
element_list
    : element_list_item (',' element_list_item)* ;
```

The parentheses in the first are ANTLR grouping, not tokens. Added to
`create_table_opts+`, which requires at least one option after the list, an ordinary
`CREATE TABLE T (C INT)` does not match and the parser falls into error recovery. The
fix is two lines in the `.g4`, but the generated Go parser is versioned and the
repository has no regeneration target — I am not doing that blindly.

**Nesting is still out.** `Pair → Pair` (JSON), `Mapping → Mapping` (YAML) and
the pairs of an inline table (TOML) do not hang from the key that opens them, for the same
reason as in the previous task: `entityUID` is name + context, so the same member
would exist as two nodes, `f::host` and `f::inline.host`.

## Two latent defects fixed along the way

Two queries can legitimately describe the same node — one matches the declaration by
itself, the other matches it again to reach the value, because no pattern
alone says "the name, and the value if there is one". That exposed:

**Duplicated entity.** Three Oracle columns arrived as six `Column` entities.
`ParsedFile.AddOrMergeEntity` now completes an already-registered entity instead
of appending another. The identity is label + name + context + line: name and line
alone would fuse the two `1`s of `{"a": 1, "b": 1}`, which are values of different
keys.

**Duplicated edge.** Two `Table→Column` edges arrived as four. The graph
writer emits one row per edge, so repeating was not a no-op: it was a second
identical edge in the database. `ConvertToCache` now deduplicates the file's CONTAINS
edges.

Both were already present in the previous commit — XML matches an element in two
queries — and they passed because that commit's tests checked existence, not
count.

## Verification

- `TestEveryShippedQueryPatternCompiles` — 540 patterns, 0 failures
- `TestDeclaredValuesBecomeNodes` — 23 sub-tests, one per language, checking the
  value node and the `value` property on the key
- `TestAntlrDeclaredDefaultsBecomeNodes` — plsql, tsql, cobol85, including the
  rejection of the CHECK read as a default
- `TestEnumMembersAreNodes` — typescript, kotlin, csharp
- `TestDataFormatCollectionsAndCDATA` — arrays, inline tables, flow sequences, CDATA
- `TestValueDelimitersAreStrippedPerPair` — the eight cases of `dataText`

Suite `./internal/...` with `-tags fts5` passes.

## Files modified

- `internal/ast/antlr_adapter.go` — `extractValueFromMatch`, `ruleByPath`,
  `nearestDescendantByRule`, emission of the value node
- `internal/ast/treesitter_adapter.go` — `dataText` removes pairs in a loop, accepts backticks
- `internal/ast/parser.go` — `ParsedFile.AddOrMergeEntity`
- `internal/ast/cache_convert.go` — dedupe of the CONTAINS edges
- `internal/ast/ladybug.go` — `EnumMember` label in the escape list
- 33 files in `internal/ast/queries/` (`db2.yaml` only got the note about the
  grammar gap)
- `internal/ast/declared_value_test.go`, `internal/ast/antlr_value_test.go` — new
- `docs/specs/ast_module.md` — `value_capture` in ANTLR


