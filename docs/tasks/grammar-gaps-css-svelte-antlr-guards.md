# CSS and Svelte come into existence, and the ANTLR loose ends close

Closes the loose ends from [declared-values-as-nodes](declared-values-as-nodes.md) and
covers two grammars that were registered in the engine and never used.

## Problem

### 1. CSS and Svelte had a registered grammar and no queries

`treesitter_native.go` registers `"css"` and `"svelte"`, and neither `css.yaml` nor
`svelte.yaml` existed. The consequence is worse than "extracts little": the extension table is
built from the `extensions` field of the query files, so **no extension was registered**
for those two languages. `.css` and `.svelte` files were not even discovered. Every stylesheet
and every Svelte component of every indexed project was invisible to the graph and to search.

Nothing reported the omission. It was found by comparing the keys of `nativeGrammars`
against the `language:` of the query files.

### 2. HTML did not have the same depth as the others

`<!DOCTYPE>` and the body of an inline `<script>` or `<style>` were parsed and
discarded. A `<script>window.APP_ENV="prod"</script>` — real configuration, the
kind people look for — did not exist in the index.

### 3. PL/SQL constants were indexed as variables

Already recorded in the previous task: there is no `constant_declaration` rule in
`PlSqlParser.g4`. A constant is a `variable_declaration` with the `CONSTANT` terminal,
so the query written against `//constant_declaration` matched nothing and
every constant fell into `Variable`.

### 4. PostgreSQL did not extract a column DEFAULT

It had not been checked in the previous cut, precisely because the DB2 test
showed the cost of writing a query on an assumption.

## Solution

### Engine: keyword guard in the ANTLR matcher

The ANTLR pattern language (`//regra`, `/regra`) gained a guard suffix:

```
//variable_declaration[CONSTANT]     casa só quando o nó tem CONSTANT como terminal direto
//variable_declaration[!CONSTANT]    casa só quando não tem
```

It exists because a grammar can write two different things with a single rule.
The comparison ignores case — an SQL keyword shows up both ways in the real
world. `plsql.yaml` now separates `Constant` from `Variable` by it.

### Engine: ANTLR `name_capture` accepts a path

`extractNameFromMatch` used `ChildByRule`, a single direct child. It now uses the
same `ruleByPath` as `value_capture`, so `identifier`, `a/b` and `**/literal`
mean there what they mean over there. No existing query changes behavior:
of the 329 occurrences of `name_capture` in the ANTLR files, 325 are the default `name`
and the other four are plain rule names.

### New grammars

**`css.yaml`** — 18 patterns. Class, id, element, pseudo-class,
pseudo-element and attribute selectors; declarations as key/value pairs
(`CssProperty "color" → Value "#ff6600"`); custom properties with a label of their own,
which is what CSS has by way of variables; `@keyframes`, `@media` with the breakpoint,
`@font-face`; `@import` routed as a real dependency (Module node + IMPORTS
edge, the same path as javascript.yaml); `var(--x)` and `url(...)` as
REFERENCES.

Two details the tree imposes:

- `:root` is `(pseudo_class_selector (class_name …))` — the **same** `class_name` node
  that `.card` uses. A pattern not anchored on the selector reports a pseudo-class as a
  class. The patterns are anchored, and the test proves that `:root` and `:hover` do not
  come in as `CssClass`.
- A custom property also matches the generic declaration pattern, and then
  `--brand-color` would sit in both tables with the value hanging off whichever came
  first. Solved with `(#not-match? @name "^--")` on the generic one.

Labels carry the `Css` prefix where the plain word already means something else in the
graph: a CSS class is not a Java class, a CSS property is not a C# property.
`GraphQLField` had already set that precedent.

**`svelte.yaml`** — 14 patterns. Labels identical to `html.yaml`'s, so that a
question about markup does not need to know which of the two produced the node. Attributes with
a quoted value and with an expression value (`on:click={handle}` → `Attribute "on:click"
→ Value "handleClick"`), which makes "which component listens to this event"
answerable; `{binding}` as REFERENCES; the condition of `{#if}` and `{#each}` as a node.
The body of `<script>`/`<style>` arrives as a single `raw_text` — it is not parsed as JS
or CSS —, so it is recorded as the element's text.

### HTML

`doctype`, and the `raw_text` of `<script>` and `<style>` as the element's text. A
large body exceeds the engine's value limit and is discarded; a one-line one, the
common case of inline configuration, becomes a node. The test also proves what used to be an
assumption: an attribute on a script/style tag is captured, even though the `start_tag` sits
under `script_element` and not under `element`.

### PostgreSQL

`colconstraintelem` writes DEFAULT with `b_expr` and CHECK with `a_expr`, so naming
`b_expr` already tells a default apart from a constraint:

```yaml
value_capture: "colquallist/**/colconstraintelem/b_expr"
```

## What remains open

**DB2 columns.** Root cause now known: `create_table_statement` in
`Db2Parser.g4` does not have the parentheses of the column list — the parentheses there are
ANTLR grouping, not tokens — and it also requires `create_table_opts+`, at least one
option after the list. `CREATE TABLE T (C INT)` does not match and the parser falls into
error recovery, delivering the list as loose terminals. A two-line fix in the
`.g4`, but the generated Go parser is versioned and the repository has no regeneration
target. I am not touching it blind.

**Nesting** (`Pair → Pair`, `Mapping → Mapping`, TOML inline table) stays
out, for the same reason as always: `entityUID` is name + context.

## Verification

- `TestEveryNativeGrammarHasQueries` — a new safety net: a grammar registered in
  `treesitter_native.go` without a query file is now a test failure. It is the
  defect that left CSS and Svelte idle, with nobody complaining.
- `TestEveryShippedQueryPatternCompiles` — **576 patterns**, 0 failures
- `TestCSSIsExtracted` — 14 node types, the key/value pairs, the `@import` down to the
  IMPORTS edge in the cache, the REFERENCES from `var()` and `url()`, and the rejection of a
  pseudo-class as a class
- `TestSvelteIsExtracted` — element, quoted attribute, expression attribute,
  condition, binding
- `TestHTMLDetailIsExtracted` — doctype, attribute on a script tag, attribute without a
  value, unquoted value, inline script and style body
- `TestAntlrDeclaredDefaultsBecomeNodes` — now with postgresql, and with
  `wantLabel` proving `C_MAX` as `Constant` and `V_URL` as `Variable`

The `./internal/...` and `./cmd/...` suite passes with `-tags fts5`.

The two new query files enter the distribution with no build change:
`bundle_ast` in the Makefile copies `internal/ast/queries/*.yaml` by glob.

## Files modified

- `internal/ast/antlr/common/matcher.go` — `[KW]` / `[!KW]` guard, `segment.matches`
- `internal/ast/antlr_adapter.go` — `extractNameFromMatch` uses `ruleByPath`
- `internal/ast/ladybug.go` — CSS labels and `Doctype` in the Cypher escaping
- `internal/ast/queries/css.yaml`, `internal/ast/queries/svelte.yaml` — new
- `internal/ast/queries/html.yaml` — doctype and inline bodies
- `internal/ast/queries/plsql.yaml` — `[CONSTANT]` / `[!CONSTANT]` guard
- `internal/ast/queries/postgresql.yaml` — column DEFAULT
- `internal/ast/css_test.go` — new
- `internal/ast/antlr_value_test.go` — postgresql and `wantLabel`
- `docs/specs/ast_module.md` — keyword guard and ANTLR `name_capture`
