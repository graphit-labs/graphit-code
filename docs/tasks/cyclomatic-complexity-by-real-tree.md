# Task: cyclomatic complexity by real tree, without text scanning

**Status: completed (33 of 45 languages; text scanning removed for good)** on 2026-08-10.

## The problem

`ComputeCyclomaticComplexity` (`internal/ast/parsers.go`) never looked at the tree — it counted
keywords (`" if "`, `" for "`, `" && "`, `"? "`, ...) in the entity's raw text, with a literal
space required on both sides. False positive (a comment and a string literal inside the body
count as a branch) and false negative (tab indentation instead of a space already breaks the match —
found by accident while checking `ComputeCyclomaticComplexity` itself after the first
pass: its `if` is preceded by a tab, not a space, and the old scan simply did not see it).

The first pass of this task configured 14 languages and kept the text scan as a fallback
for the rest. Explicit request to continue: **remove the fallback** — no text scan
hidden behind "language not configured" — and configure what had been left out.

## What exists now

**Per-language config** (`internal/ast/query_loader.go`, `ExternalQueryFile.Complexity`):
```yaml
complexity:
  node_types: [if_statement, for_statement, ...]   # nodes NOMEADOS, 1 por ocorrência
  operators: ["&&", "||"]                           # tokens/texto de operador, 1 por ocorrência
```
Without that block, the entity gets base complexity 1 — it **no** longer falls back to any text
scan. `ComputeCyclomaticComplexity`, `branchKeywordsBytes`, `lowerBufPool` and the benchmark that
existed only for them were deleted (`internal/ast/complexity_equiv_test.go`,
`BenchmarkComputeCyclomaticComplexity` in `phase3_bench_test.go`).

**The two engines** (`complexityMatcher` in `treesitter_adapter.go`, `antlrComplexityMatcher` in
`antlr_adapter.go`) walk the entity's own real subtree, adding 1 per node in
`node_types` and 1 per leaf whose **text** (not the `Kind()`) is in `operators` — the text, and not
the kind, because Julia and Scala give every operator the same generic kind (`operator`,
`operator_identifier`); only the leaf's text says which one it is. On entering a node whose kind is in
`context_types` (a nested declaration) the descent stops — that entity is scored on its own
account, without inflating the outer one.

**The 28 configured languages** — every `node_types`/`operators` was checked against the
real grammar, not guessed. A throwaway harness loading each grammar via
`NewDynGrammarLoader` (tree-sitter) or calling the native `Driver.Parse` (ANTLR — PL/SQL and
COBOL 85 are pure Go, no sidecar needed), dumping real `Kind()`/`Rule` values from a sample
with if/elif/for/while/switch-case/try-catch/ternary/`&&`/`||`, deleted after extracting the
data: go, python, javascript, typescript, tsx, java, c, cpp, rust, ruby, php, kotlin, swift,
dart, bash, groovy, haskell, julia, lua, objc, r, scala, zig, csharp, sql (tree-sitter, minimal —
only `case`, no if/for/while in that dialect), hcl (Terraform's `for_expr`/`conditional`), plsql,
cobol85 (via ANTLR).

Findings that would have been mistakes had they been guessed, besides the Julia/Scala problem already mentioned:
- **Python/Ruby/PHP/Bash/Lua/Julia/PL/SQL do not reuse `if` for `elif`/`elsif`/`elseif`** —
  they are their own nodes/rules; in the C-family languages (Go, JS, TS, Java, C, C++, Rust, Swift,
  Dart, C#, Groovy, Objective-C, Scala, COBOL 85) the "else if" is already another if nested in the else branch,
  so counting the `if` alone already counts the chain — confirmed by counting real occurrences, not just
  by looking at the dump.
- **Ruby uses the same `binary` kind for comparison and for `&&`/`||`** — which is why `binary` does not go
  into `node_types`; the operator itself is a separate anonymous leaf token, handled by `operators`.
- **Java/C/C++/Kotlin/Swift/Objective-C/C#/Zig reuse the same node for `case` and
  `default`** — a switch/when with no `case` at all still adds 1 for the `default`/`else`. The deviation is
  documented in each YAML's comment.
- **PL/SQL and COBOL 85 count per WHEN, not per whole CASE/EVALUATE** —
  `case_when_part_statement`/`evaluateWhen` per branch, and the `OTHER`/`ELSE` stays out, following the
  same convention of not counting the default.

## Second pass: Clojure, Elixir, PostgreSQL, T-SQL, DB2

Explicit request to continue: close the three SQL dialects that had been left unverified, and
solve Clojure/Elixir instead of setting them aside for being structurally hard.

**Clojure and Elixir needed a new mechanism.** In both, `if`/`when`/`cond`/`case` are not
distinct node kinds — they are the same "call" node as any other invocation, and only the
**first named child** (the head symbol in Clojure; the macro identifier in Elixir)
says which one it is. `node_types`/`operators` could not express that — both look at the node's own
kind, never at the text of a specific child. I added `head_calls` to `ComplexityConfig`:
```yaml
complexity:
  head_calls:
    node_type: list_lit   # (Clojure) ou "call" (Elixir)
    names: [if, when, cond, ...]
```
`complexityMatcher.score` gained a third path: on entering a node whose kind is
`node_type`, it looks at `NamedChild(0)` and adds 1 if the text is in `names`. Verified with a
real tree (a parent-child shape dump, not just a kind count) and with 4 test cases covering both —
including the case of "cond"/"case" counting once per form, not once per clause (the
clauses are alternating children of the same node, with no node of their own). Clojure's `and`/`or` come in
as `head_calls` too (they are macro calls like any other); Elixir's `&&`/`||`/`and`/`or`
are real operators (a `binary_operator` node) and go into `operators`, which already
matches by leaf text.

**PostgreSQL**: checked and **negative** — this is not a gap, it is a definitive answer. That
ANTLR grammar only understands `CREATE FUNCTION` at the DDL level; the whole body after
`AS $$ ... $$` is captured as a single string constant (`func_as -> sconst`). The PL/pgSQL
inside the `$$` never becomes a node that grammar sees — there is nothing to configure here
unless a PL/pgSQL grammar of its own is added.

**T-SQL**: `if_statement`, `while_statement`, `try_catch_statement` checked against the real
ANTLR tree. T-SQL has no `FOR` (only `WHILE`), and `try_catch_statement` adds 1 for the whole
TRY/CATCH pair, not 2. `CASE` and an `AND`/`OR` inside a condition were not exercised by the
sample tested — they were left out for not having been verified, not out of guesswork.

**DB2**: only `if_statement` verified. Two sample attempts with WHILE/CASE after the IF did not
advance the tree beyond the IF block — the lexer saw the WHILE/CASE/WHEN tokens, but the parser did not
progress; either the sample's syntax was wrong for that dialect, or that grammar's coverage
for those statements is limited. Left out instead of guessed — a real signal, only smaller than the
other procedural SQL dialects here.

## Third pass: PostgreSQL for real, DB2 with the grammar fixed, Clojure per clause

Request to continue: do not leave PostgreSQL/T-SQL/DB2 incomplete for "lack of grammar
mapping" — fix the grammar where that is the case — and make Clojure/Elixir "catch everything" the way
PL/SQL does.

**PostgreSQL — attempted and reverted.** The attempt: `internal/ast/antlr/postgresql/plpgsql_splice.go`
took the text between `$$...$$` (captured only as an opaque `sconst`) and reparsed it with the
PL/SQL driver, attaching the result as a child of the string node — PL/pgSQL was designed on
purpose to read like Oracle PL/SQL, so the PL/SQL parser recognizes PL/pgSQL's IF/LOOP/CASE
without error. The problem, found by deliberately testing constructs that only exist in
PL/pgSQL (`PERFORM`, `RAISE EXCEPTION '...'`, `RETURN QUERY`): the PL/SQL parser does not error on them
— it recovers silently into whatever grammatical alternative is left (`PERFORM
do_something()` read as a call to a function named `PERFORM`; `RAISE EXCEPTION '...'`
turning into a subtree with no meaning at all), with no signal that anything went wrong. In the samples
tested it did not go so far as to invent a false `if`/`loop`/`case` by accident, but nothing guarantees
that for real PL/pgSQL in general — PERFORM/RAISE/RETURN QUERY are not rare cases, they are the daily
bread of any PL/pgSQL function. A wrong number with no warning is worse than no number — reverted.
PostgreSQL stays in the verified negative: that grammar sees the body as an opaque string,
full stop, and has no `complexity:` configured.

**T-SQL — it was missing config, not grammar.** `CASE`/`AND`/`OR` already existed as real
rules/tokens (`case_expression`, `switch_section`, tokens `'AND'`/`'OR'`) — they just had not made it into the
YAML because the previous verification test had a bug (it compared by the token's name, which comes
in single quotes, `'AND'`, not by the text, which is `AND` — the matcher already uses the text, so it already
worked, it just needed configuring). Added `switch_section` (per WHEN, not per whole CASE)
and `operators: ["AND", "OR"]`.

**DB2 — it really was the grammar, and it was fixed.** `sql_procedure_body` reached
`sql_procedure_statement`, which was `CALL | FOR | IF | todo` — four loose keywords, none
carrying a body (`todo` is the placeholder the grammar itself uses for the parts that were never
written — it has 55 occurrences in the file). `IF`/`WHILE`/`CASE` inside a `CREATE PROCEDURE`
only reached the correct rules (`sql_constrol_statement`) by accident, via the parser's error
recovery. `Db2Parser.g4` was edited — not just the YAML — and the Go parser was **regenerated** with the
local `antlr-4.13.2-complete.jar` (the same version as the `// Code generated ... by ANTLR
4.13.2` header that was already in the files, so no risk of version drift):
- Completed `compound_sql_inlined`, which was literally `BEGIN todo END`.
- Added `declare_variable_statement` — a local variable declaration had no rule
  at all.
- Added `assignment_statement` (`SET var = expr`) — the most common statement in a procedure
  body also had no complete rule.
- `sql_procedure_statement` redefined around those real rules, in place of the four-keyword
  stub.
- A separate ambiguity in `sql_schema_statement` — `create_procedure_statement` AND the three
  branches it already covers itself (`create_procedure_external_statement | ...sourced... |
  ...sql_statement`) listed as SEPARATE alternatives of the same rule — made the procedure's
  body be silently discarded under full LL(*) when IF, WHILE and CASE appeared
  together in the same body (no pair of the three triggered the bug on its own; isolated by bisection
  in `TestDb2FullProcedure`). Duplicate removed, resolved.
- Tested against regression: `TestDb2ExistingExtractionStillWorks` confirms that
  `CREATE TABLE`/`CREATE VIEW` still extract the same; the full suite with nothing new in the
  `fts5` failures.

`db2.yaml` now has `if_statement`, `while_statement`, `case_statement` (once per CASE, not
per WHEN — DB2 repeats the WHEN inside the same rule instance, not as a child of its own) and
`operators: ["AND", "OR"]`.

**Clojure — cond/case now count per clause, not per form.** `ComplexityConfig`
gained `pair_names`/`subject_pair_names` in `HeadCallConfig`: for forms whose clauses
are alternating children with no node of their own, it counts pairs of children after the head instead of adding a
fixed 1. `cond` has no subject before the pairs (`pair_names`, counts `n/2`); `case` has the value
being matched as the first child after the head (`subject_pair_names`, counts `(n-1)/2` — integer
division already discards a trailing default with no test of its own, without needing to handle that case
separately). Verified with 3 real samples, including cond with `:else` and case with/without a default.

**Elixir — case/cond stay per form, on purpose, not for lack of trying.** The
obvious equivalent would be to count `stab_clause` (it is how Elixir structures each arm of case/cond) —
but `stab_clause` is the SAME node used by a single-clause function (`fn x -> x end`), so
counting it directly would add 1 of complexity to every closure passed to `Enum.map`/`reduce`/etc,
a pattern far more common than case/cond. Trading one inaccuracy for a worse one is not the fix
worth making — documented in the YAML's comment, not left as a silent gap.

## Fourth pass: PostgreSQL, again — this time with the right grammar, not a borrowed one

The second pass's "verified negative" was true but incomplete: PostgreSQL's ANTLR grammar
really does not see PL/pgSQL, but that does not mean no grammar sees it. Explicit
request to continue: use ANTLR as the host (the DDL grammar that already exists) and a Tree-sitter
as the guest for the PL/pgSQL body — the same form of embedding that XML→PL/SQL already proves works in
`internal/ast/embedded_antlr_test.go`, adapted because the HOST side of that generic mechanism only
knows how to consume Tree-sitter, not ANTLR.

**The grammar**: `github.com/gmr/tree-sitter-postgres`, `plpgsql/` — MIT/BSD-3-Clause, code-generated
from PostgreSQL's own Bison grammar for the SQL part, handwritten for the
procedural part, with an external C scanner for dollar-quoting and context-sensitive keywords. Vendored
in `internal/ast/treesitter/plpgsql/` following exactly the convention already used by every
vendored Tree-sitter grammar here (`binding.go` with cgo, `parser.c.inc`/`scanner.c.inc` with the
`.inc` extension so it is not picked up as a standalone C file, `tree_sitter/{alloc,array,parser}.h`).
Verified, before trusting it, against exactly the constructs that broke the previous
attempt — `PERFORM`, `RAISE EXCEPTION '...'`, `RETURN QUERY SELECT`, `FOREACH ... IN ARRAY` — with
none of them producing an error node.

**The mechanism**: it is not the YAML's generic `embedded:` (which creates a separate entity and would need
a merge by name/line, error-prone), it is a direct splice in Go —
`internal/ast/antlr/postgresql/plpgsql_splice.go`. `spliceCreateFunctionBodies` finds each
`createfunc_opt_list`, reads the `LANGUAGE` item and the `AS` item (which carries the dollar-quoted body as
`anysconst`); when the language is `plpgsql`, it reparses the body with the Tree-sitter grammar above,
converts the subtree (`sitterToTreeNode`) into the same `antlrcommon.TreeNode` format the rest of the
pipeline walks, and attaches it as an extra child of the `anysconst` node itself — no new entity, no merge
step, because `anysconst` is already a descendant of `createfunctionstmt`, the same node the
Function entity uses as its scope. `antlrComplexityMatcher.score` neither knows nor needs to know that that
stretch of the tree came from a different parser; it just walks `Rule`/`Children`.

Bug found and fixed: the first version read the `LANGUAGE` value with `leafText(item)` — which
includes the `LANGUAGE` token itself in the concatenation — producing `"languageplpgsql"` instead of
`"plpgsql"`, so the comparison never matched and the splice never fired, silently. Fixed by
reading only `leafText(item.Children[1])` (the value, skipping the keyword token).

`postgresql.yaml` gained:
```yaml
complexity:
  node_types:
    - stmt_if
    - elsif_clause
    - stmt_for
    - stmt_while
    - stmt_foreach_a
    - case_when
    - proc_exception
```
No `operators:` — the PL/pgSQL grammar treats every expression as an opaque span (designed for a
separate SQL grammar injection that this pipeline does not consume), so there is no AND/OR node
to find without a second level of embedding that does not exist here. `elsif_clause` is a node of its own (IF
does not re-nest ELSIF); `case_when` counts per WHEN, and the CASE's ELSE branch is not a second `case_when`
(confirmed by testing, not assumed — the same "the default does not count" convention already used in several
ANTLR dialects here); `proc_exception` also fires for `WHEN OTHERS`, the same deviation.

**Side effects on two project conventions, both resolved**: registering `plpgsql` in
`nativeGrammars` without a `queries/plpgsql.yaml` breaks `TestEveryNativeGrammarHasQueries` — the
YAML was created (a `Variable` query for the DECLARE, plus the same `complexity:` block above, so as not to
duplicate the list elsewhere). The YAML without `context_types`/`parent_capture` breaks
`TestEveryShippedGrammarDeclaresItsContainment` — `plpgsql` went into `flatLanguages`
(`internal/ast/containment_coverage_test.go`) with the reason documented: the real container is the
`CREATE FUNCTION` of the external ANTLR tree, not something plpgsql's own tree could name.

Tested end to end: `TestSpliceFindsRealPlpgsqlBranches`, `TestSpliceIgnoresNonPlpgsqlLanguages`,
`TestSpliceDoesNotTouchOrdinaryStringConstants` (`plpgsql_splice_test.go`),
`TestSpliceHandlesPlpgsqlSpecificConstructs` (PERFORM/RAISE EXCEPTION/RETURN QUERY/FOREACH, with no
error node), `TestExistingSchemaExtractionStillWorks` (`plpgsql_risky_test.go`, ordinary DDL does not
regress) and `TestComplexityPlpgsqlSplicedIntoPostgresqlEntity` (`internal/ast/complexity_plpgsql_test.go`
— parses a real `CREATE FUNCTION` via `Driver.Parse`, finds the `createfunctionstmt`, runs
`antlrComplexityMatcher.score` on it and checks the count: 5, not 6, because the CASE's ELSE
really does not add).

## What is still left out, documented in each YAML with the reason

- **JSON, TOML, XML, YAML, CSS, GraphQL, Protobuf, Markdown, Dockerfile, HTML, Svelte, Vue** —
  data/config/markup formats with no concept of complexity; HTML/Svelte/Vue's `<script>` is already
  counted as an entity of its own language (JS/TS), nothing is needed here.

## Verification

`TestComplexityWalksRealSyntaxTree`, `TestComplexityStopsAtNestedDeclaration` and
`TestComplexityMatcherOffWithoutConfig` (`internal/ast/complexity_treesitter_test.go`), plus
`TestComplexityHeadCallsClojureAndElixir` (`internal/ast/complexity_headcalls_test.go`,
with the `pair_names`/`subject_pair_names` cases) — all tree-sitter, the same scheme of graceful
skipping as `BenchmarkTS_LangLookup_Dynamic` when the `.so` is not extracted.

`TestCreateProcedureBodyParsesControlFlow` + `TestExistingSchemaExtractionStillWorks`
(`internal/ast/antlr/db2/db2_procedure_test.go`) — ANTLR is statically embedded in the binary,
with no external `.so`, so those always run, with no conditional skip.
`go build ./...`/`go vet ./...` clean; `go test ./internal/ast/... ./internal/hub/...` with no
new regression — the `fts5` failures are the same environment ones already recorded in earlier
tasks, in search files this one did not touch.
