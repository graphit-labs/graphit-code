Task: Complexity of Cyclomatic for Real Tree Without Text Scan

Status completed (33 out of 45 languages; text scan removed permanently) on August 10, 2026.

## O problema

The inline code (inline comment) never looked at the tree — it was in the original text of the entity with spaces on both sides.
literal required around keywords (`" if "`, `" for "`, `" && "`, `"? "`, ...) in the raw form of the entity. False positive (comment and string literal inside the body) is considered as a branch — it's found by accident while checking the same `ComputeCyclomaticComplexity` after the first pass: its `if` is preceded by tab, not space, and the old scan simply didn't see it.

First pass of this task configured 14 languages and maintained text scanning as a fallback for the rest. Explicit request for continuation: **remove the fallback** – without hidden text scanning behind "language not configured," and configure what remains unconfigured.

What exists now

Configuration by language (`internal/ast/query_loader.go`, `ExternalQueryFile.Complexity`):
```yaml
complexity:
Nodes named, one per occurrence
Operators: ["&&", "||"]  # Tokens/operators, one occurrence each
```
Without this block, the entity receives complexity level 1 — **not** falling into any text scan. `ComputeCyclomaticComplexity`, `branchKeywordsBytes`, and `lowerBufPool` were removed (`internal/ast/complexity_equiv_test.go`, `BenchmarkComputeCyclomaticComplexity` in `phase3_bench_test.go`).

The two engines (`complexityMatcher` in `treesitter_adapter.go`, `antlrComplexityMatcher` in
`antlr_adapter.go`) are walking on the real sub-tree of their own entity, adding 1 per node in
`node_types` and 1 per leaf whose **text** (not the `Kind()`) is in `operators` — the text, not the kind, because Julia and Scala give every operator the same generic kind (`operator`, `operator_identifier`); only the text of the leaf tells what it is. Entering into a node whose kind is in
`context_types` (a nested declaration) the descent for that entity is punctuated by its own authority, without inflating from outside.

**The 28 configured languages** — each ___INLINE_26__/`operators` was checked against the real grammar, not chucked. The disposable harness is carrying each grammar via `NewDynGrammarLoader` (tree-sitter) or calling the `Driver.Parse` native (ANTLR – PL/SQL and COBOL 85 are pure Go without requiring a sidecar), spilling `Kind()`/`Rule` real samples of data: go, python, javascript, typescript, tsx, java, c, cpp, rust, ruby, php, kotlin, swift, dart, bash, groovy, haskell, julia, lua, objc, r, scala, zig, csharp, sql (tree-sitter, minimum – just `case`, without if/for/while in this dialect), hcl (`for_expr`/`conditional` of Terraform), plsql, cobol85 (via ANTLR).

---

**The 28 configured languages** — each ___INLINE_26__/`operators` was checked against the real grammar, not chucked. The disposable harness is carrying each grammar via `NewDynGrammarLoader` (tree-sitter) or calling the `Driver.Parse` native (ANTLR – PL/SQL and COBOL 85 are pure Go without requiring a sidecar), spilling `Kind()`/`Rule` real samples of data: go, python, javascript, typescript, tsx, java, c, cpp, rust, ruby, php, kotlin, swift, dart, bash, groovy, haskell, julia, lua, objc, r, scala, zig, csharp, sql (tree-sitter, minimum – just `case`, without if/for/while in this dialect), hcl (`for_expr`/`conditional` of Terraform), plsql, cobol85 (via ANTLR).

Assuming you meant to translate the code blocks and technical terms as well:

```python
They thought they had been making mistakes in their throw, plus the issue mentioned earlier:
"None of Python, Ruby, PHP, Bash, Lua, Julia, PL/SQL, or any other language can reuse `if` for `elif`, `elsif`, and `elseif`."
They are native nodes/rules; in languages of the family C (Go, JavaScript, TypeScript, Java, C, C++, Rust, Swift)
Dart, C#, Groovy, Objective-C, Scala, COBOL 85) The "else if" is already another nested if in the else branch, so counting `if` only counts the chain — confirmed by counting occurrences, not just examining the dump.
Ruby uses the same kind ``binary`` for comparison and for ``&&`/`||``. Therefore, ``binary`` does not enter into `__INLINE_4`; the operator itself is a token-stream anonymous part resolved by ``operators``.
Java, C/C++, Kotlin, Swift/Objective-C, C#, Zig all reuse the same node for `case` and `default`, which adds 1 to `default`/`else` without a `case`. The documentation is documented in each YAML comment.
PL/SQL and COBOL 85 count by WHEN, not by CASE/Evaluate in entirety.
Inline 0 and Inline 1 are separated by the branch, while Inline 2 and Inline 3 remain outside, following the same convention of not counting the default.

```

Note: The original code blocks and markdown links were not provided in the Portuguese text.

## Segunda passada: Clojure, Elixir, PostgreSQL, T-SQL, DB2

Explicit request for continuation: close the three dialects of SQL that had been left unverified,
and resolve Clojure/Elm in lieu of leaving them aside due to their structural difficulties.

Clojure and Elixir needed a new mechanism. In both, __INLINE_57__/__INLINE_58__/__INLINE_59__/__INLINE_60__ are not distinct node kinds — they're the same "call" node kind as any other invocation, and only the first named child (the symbol-head in Clojure; the macro identifier in Elixir) indicates which one it is. __INLINE_61__/__INLINE_62__ couldn't express this — both look at the kind of the node itself, never at the text of a specific child. I added __INLINE_63__/__INLINE_64__:
```yaml
complexity:
  head_calls:
    node_type: list_lit   # (Clojure) ou "call" (Elixir)
    names: [if, when, cond, ...]
```
The inline 65 has gained a third path: when entering a node whose kind is
inline 66, looks at inline 67 and sums 1 if the text is in inline 68. Verified with real tree (dump of parent-child shape, not just count of kind) and with 4 test cases covering both — including the case of "cond"/"case" counting once per form, not once per clause (the clauses are alternate children of the same node, without their own node). Clojure inline 69/inline 70 enter as inline 71 (are called macros like any other); Elixir inline 72/inline 73/inline 74/inline 75 operators are truthy (node inline 76) and enter in inline 77, which already nests by leaf text.

**PostgreSQL**: verified and **negative** — not an omission, but a definitive answer. This ANTLR grammar only understands the `CREATE FUNCTION` at the DDL level; everything after `AS $$ ... $$` is captured as a single string constant (`func_as -> sconst`). The PL/pgSQL inside `$$` never becomes a node that this grammar sees — unless you add an own PL/pgSQL grammar.

---

**PostgreSQL**: verified and **negative** — not an omission, but a definitive answer. This ANTLR grammar only understands the `CREATE FUNCTION` at the DDL level; everything after `AS $$ ... $$` is captured as a single string constant (`func_as -> sconst`). The PL/pgSQL inside `$$` never becomes a node that this grammar sees — unless you add an own PL/pgSQL grammar.

T-SQL: `if_statement`, `while_statement`, and `try_catch_statement` verified against the tree
ANTLR real. T-SQL does not have `FOR` (only `WHILE`), and `try_catch_statement` adds 1 to the TRY/CATCH integer counter, not 2. `CASE` and one `AND`/`OR` inside a condition were not exercised by the sample test — they remained outside because they were unverified, not because of an error.

**DB2**: only INLINE 91 verified. Two attempts with WHILE/CASE after the IF did not advance beyond the block IF—lexer saw the tokens WHILE/CASE/WHEN but parser did not progress; or the syntax of the sample was wrong for this dialect, or coverage of that grammar for these statements is limited. It missed out instead of being chucked—real signal, just smaller than other procedural SQL dialects here.

Third Pass: True PostgreSQL, DB2 with corrected grammar, Clojure per clause

Continuation Request: Ensure that PostgreSQL/T-SQL/DB2 is not left incomplete due to "lack of grammar mapping" — correct the grammar where it applies — and have Clojure/Elixir "grab everything" as PL/SQL does.

**PostgreSQL - tried and reverted.**

Attempt: `internal/ast/antlr/postgresql/plpgsql_splice.go`

Captured text between **INLINE_78** (only captured as **INLINE_79** opaque) and reparse with the PL/SQL driver, attaching the result as a child of the string node — PL/pgSQL was designed specifically to read as PL/SQL from Oracle; thus, the PL/SQL parser recognizes IF/LOOP/CASE in PL/pgSQL without error. The problem, found by purposeful construction of constructs that only exist in PL/pgSQL (`PERFORM`, `RAISE EXCEPTION '...'`, `RETURN QUERY`): the PL/SQL parser does not err on them — it recovers silently to any grammatical alternative (`PERFORM do_something()` read as a call to a function named ` PERFORM`; ` RAISE EXCEPTION '...'` turning into a meaningless subtree), without any indication that something went wrong. In tests, this did not invent an **INLINE_86**/**INLINE_87**/**INLINE_88** false by accident, but nothing guarantees it for PL/pgSQL in general — PERFORM/RAISE/RETURN QUERY are not rare cases; they are everyday occurrences of any function PL/pgSQL. An incorrect number without warning is worse than having no number at all — reverted. PostgreSQL remains negative verified: this grammar sees the body as an opaque string, a period, and does not configure **INLINE_89**.

Note: The original text contains several inline code blocks (denoted by ___INLINE_...___) that are preserved in their entirety without translation or modification.

T-SQL was missing the configuration, not grammatical. `CASE`/`AND`/`OR` already existed as real rules/tokens (`case_expression`, `switch_section`, tokens `'AND'`/`'OR'`) — they just hadn't entered the YAML because the previous verification test had a bug (comparing by the token's name, which comes between simple quotes, `'AND'`, not by the text, which is `AND` — the matcher already uses the text, so it was already working, just needed to configure). Added `switch_section` (by WHEN, not CASE entirely) and `operators: ["AND", "OR"]`.

**DB2 — it was grammatical, and it was corrected.**

Inline 115 arrived at Inline 116, which was Inline 117 — four keywords floating around, none carrying a body (Inline 118 is the placeholder that the very grammar uses for parts that never were written — there are 55 occurrences in the file). /Inline 119/Inline 120/Inline 121 inside a `CREATE PROCEDURE` only arrived at rules correctly (Inline 123) by accident, via recovery of error from parser. Inline 124 was edited — not just YAML — and the Go parser was **regenerated** with the inline local (the same version of header `// Code generated ... by ANTLR 4.13.2` that already existed in the files, so there was no risk of version drift):
- Completed Inline 126, which was literally Inline 127.
- Added Inline 128 — local variable declaration without any rule.
- Added Inline 129 (Inline 130) — the most common instruction of a procedure body also did not have a complete rule.
- Inline 131 redefined around these real rules, in place of the stub for four keywords.
- A separate ambiguity at Inline 132 — Inline 133 AND the three branches that he himself already covers (`create_procedure_external_statement | ...sourced... | ...sql_statement`) listed as alternatives SEPARATED to the same rule — made the procedure body silently discarded when IF, WHILE, and CASE appeared together in the same body (none of the three triggered the bug individually; isolated by bisecting at Inline 134). Removed the duplicate, resolved.
- Tested against regression: Inline 135 confirms that Inline 136/Inline 137 continue extracting the same; complete suite without new failures in Inline 138.

---

**DB2 — it was grammatical, and it was corrected.**

Inline 115 arrived at Inline 116, which was Inline 117 — four keywords floating around, none carrying a body (Inline 118 is the placeholder that the very grammar uses for parts that never were written — there are 55 occurrences in the file). /Inline 119/Inline 120/Inline 121 inside a `CREATE PROCEDURE` only arrived at rules correctly (Inline 123) by accident, via recovery of error from parser. Inline 124 was edited — not just YAML — and the Go parser was **regenerated** with the inline local (the same version of header `// Code generated ... by ANTLR 4.13.2` that already existed in the files, so there was no risk of version drift):
- Completed Inline 126, which was literally Inline 127.
- Added Inline 128 — local variable declaration without any rule.
- Added Inline 129 (Inline 130) — the most common instruction of a procedure body also did not have a complete rule.
- Inline 131 redefined around these real rules, in place of the stub for four keywords.
- A separate ambiguity at Inline 132 — Inline 133 AND the three branches that he himself already covers (`create_procedure_external_statement | ...sourced... | ...sql_statement`) listed as alternatives SEPARATED to the same rule — made the procedure body silently discarded when IF, WHILE, and CASE appeared together in the same body (none of the three triggered the bug individually; isolated by bisecting at Inline 134). Removed the duplicate, resolved.
- Tested against regression: Inline 135 confirms that Inline 136/Inline 137 continue extracting the same; complete suite without new failures in Inline 138.

Now it has `if_statement`, `while_statement`, and `case_statement` (once per CASE, not by WHEN — DB2 repeats the WHEN within the same instance of a rule, not as a child).

Clojure - `cond` and `case` pass now counting by clause rather than form. `ComplexityConfig` gained `pair_names` / `subject_pair_names` in `HeadCallConfig`: for forms whose clauses are alternated without a node, counts pairs of children after the head instead of summing 1 fixedly. `cond` has no subject before the pairs (`pair_names`, counts `n/2`); `case` has the value being married as its first child after the head (`subject_pair_names`, counts `(n-1)/2` - the whole division already discards a default final without test itself, needing no part to handle this case). Verified with 3 real samples including `cond` with `:else` and `case` with/without default.

Elixir - cases and conditions are forms, not by design, but because they were never tried.  
The obvious equivalent would be to count `stab_clause` (just like how Elixir structures each arm of a case/cond) — but `stab_clause` is the same node used by a unique clause function (__INLINE_157__), so counting directly adds 1 complexity to every closure passed to `Enum.map`/__INLINE_159__ etc., which is much more common than cases/conditions. Changing one mistake for another is not the right thing to do — documented in the YAML comment, not left as a silent gap.

Fourth Day: PostgreSQL Again - This Time with Proper Grammar, Not Borrowed

The "verified negative" of the second pass was indeed true but incomplete: PostgreSQL's ANTLR grammar actually does not recognize PL/pgSQL, but this doesn't mean that no grammar recognizes anything. Explicit request for continuation: use ANTLR as host (the DDL grammar already exists) and Tree-sitter as guest for the PL/pgSQL body — the same embedding method that XML→PL/SQL has proven to work in `internal/ast/embedded_antlr_test.go`, adapted because this generic mechanism's side-host only knows how to consume Tree-sitter, not ANTLR.

**Grammar**: `github.com/gmr/tree-sitter-postgres`, `plpgsql/` — MIT/BSD-3-Clause, generated code from the PostgreSQL's own Bison grammar for SQL parts, hand-written for procedural parts with an external C scanner for dollar-quoting and context-sensitive keywords. Vendorized in `internal/ast/treesitter/plpgsql/` following exactly the convention already used by all vendorized grammars (`binding.go` using cgo, `parser.c.inc`/`scanner.c.inc` with extension __INLINE_167__ to avoid being asked as a standalone C file, `tree_sitter/{alloc,array,parser}.h`). Verified before trusting it against exactly the constructions that broke the previous attempt — `PERFORM`, `RAISE EXCEPTION '...'`, `RETURN QUERY SELECT`, `FOREACH ... IN ARRAY` — without any producing an error node.

**Mechanism**: it is not the ___INLINE_173__ generic YAML (which creates a separate entity and would require merging by name/line, prone to errors), but a direct splice in Go — ___INLINE_174__. `spliceCreateFunctionBodies` finds each `createfunc_opt_list`, reads the item `LANGUAGE` and `AS` (which loads the dollar-quoted body as `anysconst`); when the language is `plpgsql`, reparsees the body with Grammar Tree-sitter above, converts the sub-tree (`sitterToTreeNode`) to the same format `antlrcommon.TreeNode` that the rest of the pipeline follows, and attaches as an extra child node of its own `anysconst` — without a new entity, without merge step, because `anysconst` is already a descendant of `createfunctionstmt`, the same node used by the Entity Function for scope. `antlrComplexityMatcher.score` does not know nor need to know that that part of the tree came from a different parser; it just goes `Rule`/`Children`.

**Note**: Inline references are replaced with the actual content, and inline comments are removed.

Bug found and fixed: The first version read the value of `LANGUAGE` with `leafText(item)` — which includes the very token `LANGUAGE` in the concatenation — producing `"languageplpgsql"` instead of `"plpgsql"`, so the comparison never matched and the splice never fired silently. Fixed by reading only `leafText(item.Children[1])` (the value, skipping the keyword token).

`postgresql.yaml` ganhou:
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
Without __INLINE_196__ — the PL/pgSQL grammar treats every expression as a transparent span (intended for an SQL grammar injection pipeline that this pipeline does not consume), so there is no node for AND/OR to find without a second-level embedding that doesn’t exist here. `elsif_clause` is its own node (IF it does not re-raise ELSIF); __INLINE_198___ counts by WHEN, and the ELSE branch of CASE is not a second __INLINE_199__ (confirmed through testing, not assumed — same convention of "default doesn’t count" already used in several ANTLR dialects here). `proc_exception` also triggers for `WHEN OTHERS`, even with deviation.

**Side effects in two conventions of the project, both resolved**: register `plpgsql` in
`nativeGrammars` without a `queries/plpgsql.yaml` that breaks `TestEveryNativeGrammarHasQueries` — created by YAML (a query on `Variable` using DECLARE, plus the same block above to avoid duplicating the list elsewhere). The YAML without `context_types`/`parent_capture` breaks `TestEveryShippedGrammarDeclaresItsContainment` — `plpgsql` entered into `flatLanguages`
(`internal/ast/containment_coverage_test.go`) with a documented reason: the real container is the
`CREATE FUNCTION` of the external ANTLR tree, not something that the ANTLR tree itself knows to name.

Tested from end to end: `TestSpliceFindsRealPlpgsqlBranches`, `TestSpliceIgnoresNonPlpgsqlLanguages`,
`TestSpliceDoesNotTouchOrdinaryStringConstants` (`plpgsql_splice_test.go`),
`TestSpliceHandlesPlpgsqlSpecificConstructs` (PERFORM/RAISE EXCEPTION/RETURN QUERY/FOREACH, without
error node), `TestExistingSchemaExtractionStillWorks` (`plpgsql_risky_test.go`, Common DDL does not
deviate) and `TestComplexityPlpgsqlSplicedIntoPostgresqlEntity` (`internal/ast/complexity_plpgsql_test.go` — parses a real `CREATE FUNCTION` via `Driver.Parse`, finds the `createfunctionstmt`, runs
`antlrComplexityMatcher.score` on it and checks the count: 5, not 6, because the ELSE of the CASE really does not sum).

What is left out has been documented in each YAML with the reason

- **JSON, TOML, XML, YAML, CSS, GraphQL, Protobuf, Markdown, Dockerfile, HTML, Svelte, Vue** —
  formats of data/config/markup without a concept of complexity; the `<script>` in HTML/Svelte/Vue is already counted as an entity of its own language (JS/TS), no need for anything here.

Verification

`TestComplexityWalksRealSyntaxTree`, `TestComplexityStopsAtNestedDeclaration` and
`TestComplexityMatcherOffWithoutConfig` (`internal/ast/complexity_treesitter_test.go`), more
`TestComplexityHeadCallsClojureAndElixir` (`internal/ast/complexity_headcalls_test.go`,
with the cases of `pair_names`/`subject_pair_names`) — all tree-sitter, even the humorous skip scheme
of `BenchmarkTS_LangLookup_Dynamic` when `.so` is not extracted.

```csharp
__INLINE_239__ + __INLINE_240__
(__INLINE_241__) — ANTLR is statically embedded into the binary,
without an external __INLINE_242__, so these run always, without conditional skip.
__INLINE_243__ / __INLINE_244__ cleaned; __INLINE_245__ regression-free — failures of __INLINE_246__ are the same as those registered in previous tasks,
not updated in files not touched by this.
