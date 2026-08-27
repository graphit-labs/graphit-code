CSS and Svelte are here, and the ANTLR issues have been closed.

Closes outstanding issues in the `[declared-values-as-nodes]` section of `declared-values-as-nodes.md` and covers two grammars that were registered in the engine but never used.

## Problema

CSS and Svelte had grammar registered but no queries.

The inline code is registered, and there was no __INLINE_3__ nor __INLINE_4__. The consequence is worse than "extracts little": the extension table is built from the `extensions` field of query files, so **no extensions were registered** for these two languages. Files `.css` and `.svelte` were even not discovered. All style sheets and all Svelte components in every indexed project were invisible to the graph and search.

Nothing was reported of the omission. It was found by comparing the keys of `nativeGrammars` with those of `language:` in the query files.

2. HTML did not have the same depth as the other languages.

The _INLINE_10_ and the body of a _INLINE_11_ or _INLINE_12_ inline were parsed and discarded. A _INLINE_13_ — the actual configuration, of the type sought — did not exist in the index.

3. Constants in PL/SQL were indexed as variables.

Already registered in the previous task: there is no rule `constant_declaration` in `PlSqlParser.g4`. A constant is a `variable_declaration` with the terminal `CONSTANT`, so the query written against `//constant_declaration` does not match anything and all constants fall into `Variable`.

Note: The code blocks, markdown, file paths, and technical terms have been preserved as per your request.

4. PostgreSQL does not extract default from column

The test of DB2 did not have to be verified in the previous cut, precisely because it showed the cost of writing queries by assumption.

Solution

Motor: Keyword guard in ANTLR Matcher

The grammar of the ANTLR Patterns language gained a guard suffix:

```
The variable is only assigned when the node has CONSTANT as its direct terminal.
The variable is only assigned when it does not exist.
```

There exists because one grammar can write two different things with just one rule.
The comparison ignores case — the SQL keyword appears in both forms in the real world. __INLINE_22__ separates __INLINE_23__ from __INLINE_24__ by it.

### Motor: `name_capture` do ANTLR aceita caminho

The inline code is using the same `ruleByPath` that `value_capture` uses, so `identifier`, `a/b`, and `**/literal` mean the same as they do in the original text. There are no existing queries that change behavior: out of 329 occurrences of `name_capture` in the ANTLR files, 325 are set to the default `name`, and the other four are simple rule names.

New grammars

**`css.yaml`** — 18 patterns. Selectors for classes, IDs, elements, pseudo-classes,
pseudo-elements, and attributes; declarations as key-value pairs (`CssProperty "color" → Value "#ff6600"`); custom properties with their own labels, which is what CSS has of variables; `@keyframes`, `@media` with the breakpoint, `@font-face`; `@import` routed as a dependency of truth (module node + IMPORTS, the same path as javascript.yaml); `var(--x)` and `url(...)` as references.

Two details that the tree imposes:

- **INLINE_43** is the same node as **INLINE_44**, which uses **INLINE_45**.
  A non-bound pattern reports pseudo-class as a class. The patterns are bound, and the test proves that `:root` and `:hover` do not enter as `CssClass`.
- Custom property "casa" also follows the generic declaration pattern, and there it would appear in both tables with the value hanging on what came first. Resolved with **INLINE_51** in the generic.

Note: The underscores (__) are placeholders for actual inline code or content that should be replaced with the appropriate values when translating to English.

Labels include the prefix __INLINE_52__ where simple words already mean something else in the graph: CSS classes are not Java classes, CSS properties are not C# properties. __INLINE_53__ has already set this precedent.

**`svelte.yaml`** — 14 patterns. Labels identical to those of **`html.yaml`**, so that a question about markup does not need to know which of the two produced the node. Attributes with value cited and with expression value (`on:click={handle}` → `Attribute "on:click" → Value "handleClick"`), making "which component listens to this event" responsive; `{binding}` as REFERENCES; condition **`{#if}`** and **`{#each}` as node. The body of **`<script>`/`<style>` comes as a single **`raw_text` — not parsed as JS or CSS —, so it is registered as text of the element.

### HTML

The element's text is set to __INLINE_63__ and __INLINE_64__, as inline content. A large body passes the value limit of the engine and is discarded; a line item, which is the common configuration for inline elements, becomes a node. The test also proves what was previously an assumption: attributes in script/style tags are captured, although __INLINE_67__ remains within __INLINE_68__ but not within __INLINE_69__.

### PostgreSQL

```python
DEFAULT writes with `b_expr` and CHECK with `a_expr`, then `b_expr` already distinguishes DEFAULT from restriction:
```

```yaml
value_capture: "colquallist/**/colconstraintelem/b_expr"
```

What remains open

**DB2 Columns.** Root cause now known: `create_table_statement` of `Db2Parser.g4` in the list of columns does not have parentheses — the parentheses are groupingANTLR tokens, not tokens themselves — and still requires `create_table_opts+`, at least one option after the list. `CREATE TABLE T (C INT)` does not match, and the parser falls into error recovery mode, delivering the list as standalone terminals. Two lines in `.g4` need correction, but the generated Go parser is versioned and the repository has no regeneration target. I do not make changes blindly.

---

Note: The inline codes (`create_table_statement` to `.g4`) are placeholders for actual code snippets or identifiers that should be replaced with the appropriate content in the translation process.

Indentation (`Pair → Pair`, `Mapping → Mapping`, inline TOML table) follows
out of the same reason as always: `entityUID` is name + context.

Verification

- `TestEveryNativeGrammarHasQueries` — new network: a registered grammar is passed as an inline query without a file, causing the test failure. This defect left CSS and Svelte idle, with no one complaining.
- `TestEveryShippedQueryPatternCompiles` — 576 patterns, 0 failures
- `TestCSSIsExtracted` — 14 types of nodes, key-value pairs (the `@import` until the IMPORTS in the cache), references to `var()` and `url()`, and rejection of pseudo-class as a class
- `TestSvelteIsExtracted` — element, cited attribute, expression attribute, condition, binding
- `TestHTMLDetailIsExtracted` — doctype, attribute inside script tag, empty attribute value, value without quotes, inline script body and style
- `TestAntlrDeclaredDefaultsBecomeNodes` — now with postgresql, and with `wantLabel` proving `C_MAX` as `Constant` and `V_URL` as `Variable`

Suite `./internal/...` and `./cmd/...` proceed with `-tags fts5`.

The two new query files enter the distribution unchanged during build:
`bundle_ast` in the Makefile copies `internal/ast/queries/*.yaml` by glob.

## Arquivos modificados

- `internal/ast/antlr/common/matcher.go` stores `[KW]` / `[!KW]`, `segment.matches`
- `internal/ast/antlr_adapter.go` uses `extractNameFromMatch`
- `internal/ast/ladybug.go` CSS labels and `Doctype` in Cypher escape
- `internal/ast/queries/css.yaml`, `internal/ast/queries/svelte.yaml` — new
- `internal/ast/queries/html.yaml` doctype and inline bodies
- `internal/ast/queries/plsql.yaml` stores `[CONSTANT]` / `[!CONSTANT]`
- `internal/ast/queries/postgresql.yaml` DEFAULT column width
- `internal/ast/css_test.go` new
- `internal/ast/antlr_value_test.go` PostgreSQL and `wantLabel`
- `docs/specs/ast_module.md` stores key word and `name_capture` from ANTLR
