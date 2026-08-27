---
title: "Embedded Language Parsing"
description: "How the body of <script> and <style> in single-file components is parsed with the grammar of the language it is written in. Implemented for Vue, Svelte and HTML."
content-type: reference
audience: developers
keywords:
  - AST
  - Tree-sitter
  - Vue
  - Svelte
  - embedded
  - composite parse
prerequisites:
  - "docs/specs/ast_module.md"
related:
  - "docs/tasks/add-vue-treesitter-grammar.md"
  - "docs/tasks/embedded-language-parsing.md"
---

# Embedded Language Parsing

> **Status: implemented on 2026-08-05**, for `<script>` and `<style>` in Vue, Svelte and
> HTML. The design below was followed; the six open decisions are recorded with the
> chosen option and rationale. Implementation log:
> [embedded-language-parsing.md](../tasks/embedded-language-parsing.md).

## The problem

A single-file component mixes languages in one file:

```vue
<template>
  <h1>{{ title }}</h1>
</template>

<script setup>
import { ref } from 'vue'
const title = ref('Pedidos')
</script>

<style scoped>
h1 { color: red; }
</style>
```

`tree-sitter-vue` and `tree-sitter-svelte` deliver the body of `<script>` and `<style>` as
**a single `raw_text` node**. The grammars do not look inside it — this is not a limitation of our
query, it is what the grammar produces.

### What is missing is structure, not textual search

This distinction is the point, and it is easy to get wrong. Measured on 2026-08-05:

**As text, the body IS searchable.** The `files` table (`internal/ast/search_lance.go`)
stores the entire file text in `source`, and `file_fts` indexes it. So,
with `ast.index_source` enabled — the default:

- `ast_search` matches the file. Verified with a literal that exists only inside a
  function body: the hit comes back as `type: file`, `line: 0` — it tells *which file*, not where.
- `ast_source` with `pattern` finds the line inside it.

With `--no-source` / `ast.index_source: false` that layer disappears and the body is
nowhere.

**As structure, nothing existed** — and that was the debt. What was broken, and how
it is now:

| before | now |
|---|---|
| A `.vue` or `.svelte` contributed no `IMPORTS` edge. In a Vue project the dependency graph simply lacked the components, and no textual search replaces an edge, because "what depends on this module" is a traversal. | The `import` in the block produces the `Import` in the external file and the edge `File -[:IMPORTS]-> Module`, with the **absolute line** of the statement. |
| No entity from the script existed: `const title` was not a `Variable`, a function declared there was not a `Function`, nothing showed up as `CALLS`. | All entities of the inner language exist, at the absolute line, contained by the `File`. `CALLS` too. |
| `exports.strategy: none` in both grammars, and it had to be: `defineProps` / `export let` lived in `raw_text`. | Still `none` — but now it governs only the **markup**. The block's `is_exported` comes from `typescript.yaml` / `javascript.yaml`, which is what knows what an `export` is. |

### The `Text` node, and why it almost never appears

The body is stored as the element's text (`Text` contained by the `Element` "script"), but
`dataText` does `TrimSpace` first and then **rejects any internal newline**. So:

| body | result |
|---|---|
| `<script>const a = 1;</script>` | `Text` node with the code as its name |
| `<script setup>` + one indented statement on its own line | `Text` node — the surrounding newlines are trimmed |
| two or more statements | **no node** |

In other words the real-world case — a real component script — produces zero. The
`maxDataValueLen = 256` cap is almost never what decides: the newline decides first, and by far.

This behavior **has not changed**, and the `element_text` queries remain in all three
grammars on purpose: the body of a single statement becomes a `Text` node whose name is the statement, so the
element's content remains searchable *as content*, alongside the structure now extracted
from it. The embedded parse is an addition, not a replacement.

Markdown code fences are excluded: the body of a fence is documentation text, and an
example inside a code block is not a project declaration — indexing it as an entity would create
`Function` nodes nobody can call.

## Why not just a query

`CompositeParser` (`internal/ast/composite_parser.go`) dispatches between tree-sitter and ANTLR
**by file extension**. No language has parsing of a region of the
file with another grammar. The name "composite" refers to the two backends, not to
embedded languages.

And `parseWithConfig` (`internal/ast/treesitter_adapter.go`) read the file from disk on its
first line and assumed a single `src` for everything else: the context resolver
(`newContextResolver`), the docstring matchers and the positions of all entities were
derived from that buffer. There was no way to pass a sub-buffer without touching that function — hence
`parseSource` below.

## Design

Four pieces, in the order they were built.

### 1. `parseSource`, extracted from `parseWithConfig`

```go
func (t *TreeSitterParser) parseWithConfig(path, ext string, cfg *tsLangConfig, isDepend bool, opts ParseOptions) (*ParsedFile, error) {
    src, err := ReadFileBytes(path)
    if err != nil {
        return nil, err
    }
    return t.parseSource(path, ext, cfg, src, 0, 0, isDepend, opts)
}

// lineOffset is added to the lines of everything the parse produces. embedDepth limits
// the sub-parse: a language whose block names itself would recurse indefinitely.
func (t *TreeSitterParser) parseSource(path, ext string, cfg *tsLangConfig, src []byte,
    lineOffset, embedDepth int, isDepend bool, opts ParseOptions) (*ParsedFile, error)
```

`embedDepth` was not in the original design and was added: the config is declarative and
nothing prevents a `default: vue` inside `vue.yaml`. `maxEmbedDepth = 1`, which is all that
any real component needs — TypeScript does not declare embedded blocks.

### 2. Blocks declared in the language YAML

The configuration belongs to the language, not to the engine — same principle as `context_types` and
`exports`.

The selector is a **tree-sitter pattern**, and the full form is in
"[The shape, as shipped](#the-shape-as-shipped)" — the first cut used `node` + `text`, which
was replaced. In `vue.yaml`:

```yaml
embedded:
  # `lang` present: the pattern captures the value, lang_capture names it
  - pattern: '(script_element (start_tag (attribute (attribute_name) @_a (quoted_attribute_value (attribute_value) @lang))) (raw_text) @body (#eq? @_a "lang"))'
    text_capture: body
    lang_capture: lang
    languages: { js: javascript, ts: typescript, tsx: tsx }
  # `lang` absent: the fallback, behind the specific one
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    default: javascript
```

`svelte.yaml` gets the same, without `tsx`. `html.yaml` gets a variant where the
discriminator is **`type`**, not `lang` — HTML has no `lang` on a script, and `type` is what
separates code from payload. Only values that *are* JavaScript are mapped, so
`type="application/json"`, `type="importmap"` and `type="text/x-template"` are skipped instead
of being parsed as code.

### 3. The sub-parse and the merge

For each body claimed by a block:

1. Resolve the language via `lang_capture`, falling back to `default`. Unmapped value,
   or language with no registered grammar → **skip the body**, no error.
2. `parseSource(path, innerExt, innerCfg, body, textNode.StartPosition().Row, depth+1, …)`.
3. `mergeParsedInto`: `Entities` by `data_key` via `AddOrMergeEntity`, plus `CallSites` and
   `References`.

The `path` of the inner parse is the outer file's path, intentionally: the `IMPORTS` edge must
leave the `.vue`, not a synthetic path.

The offset is applied by `shiftParsedLines`, **once, at the end of the parse**, instead of a
`+ lineOffset` in each of the ten places that compute a line from a node position.
It is the part that fails silently — a wrong line still looks like a line — so it is worth having a
single implementation, with its own test.

`textNode.StartPosition().Row` is the offset because `raw_text` starts immediately after the
`>`, on the same line as the tag: line 1 of the inner parse *is* that line.

### 3b. Who the block's statement belongs to — the host entity

A statement inside an embedded block has no surrounding entity **in its own language**
— a bare `INSERT INTO t` in a configuration value is not inside any procedure —
so it would fall back to the file, and the graph could only say "this FILE writes to t". But
the block does not float in the file: it sits inside something the HOST grammar named, and
that name is the answer to "which unit of this configuration touches that table".

`attributeToHostEntity` runs **before the merge**, while the block's position is still in
hand, and fills `SourceName` only for those with no origin of their own — a block that actually
declares a procedure keeps the procedure.

`hostEntityAt` picks the innermost entity that **contains the entire block, and contains it
strictly** — starting before or ending after it. Three rules in one:

- **The innermost**, because documents nest: the element carrying a value is inside the
  one describing a step, which is inside the one describing the flow. The outermost would make the
  root element the origin of everything, which is the "file" answer with extra steps.
- **That contains**, because an entity ending INSIDE the block does not host it. In a
  data grammar the span of an entity goes from the name to the end of the name's PARENT — the start tag, for
  `(STag (Name) @name)` — so the element that literally carries the text has a one-line span
  and is indistinguishable from the sibling above.
- **Strictly**, because an entity whose span COINCIDES with the block is the thing carrying the
  block, not a surrounding unit: `<value>select …</value>` on a single line puts tag and
  statement on the same line, and simple containment would pick the `<value>`.

Content labels (`Value`, `AttributeValue`, `Text`, `Comment` — those whose `name` equals
the content itself) are skipped before any of this: the block's text IS one of those nodes, and
attributing the statement to the statement's text says nothing.

What remains is the entity the grammar declared with a wide enough span, and that is where
`span_capture` comes in (see `docs/specs/ast_module.md`): without it, a data grammar has no
host to offer and the origin stays as the file — an honest answer, instead of a
neighboring node.

**`host_labels`: when the block lives in an ATTRIBUTE of the tag that names the unit.** The rule
above applies to a block that is CONTENT of something. There is the other form: the PL/SQL for an exported screen in XML lives in `<Trigger Name="POST-QUERY" TriggerText="…"/>`, with line breaks
encoded — so unit and block occupy the SAME physical line, "strictly contains"
excludes exactly who should answer, and everything containing the block is coarser than
it (the item, the data block, the form). The block then declares which labels are units:

```yaml
embedded:
  - pattern: '(Attribute (Name) @_a (AttValue) @body (#eq? @_a "TriggerText"))'
    text_capture: body
    normalize: attr_text
    host_labels: [FormTrigger]
    default: plsql
```

When declared, containment is still required but no longer needs to be strict, and only those
labels are considered — the choice stops depending on a span being wider by accident.
When omitted, the default rule applies.

**THE PREDICATE MUST BE INSIDE THE PATTERN'S PARENTHESES.** `(node) @cap (#eq? @cap "x")`
compiles as TWO patterns, and the predicate then constrains the second one — which captures
nothing. Result: it filters nothing, silently, and the embedded block sends EVERY candidate to
the inner parser. Measured on an XML tag: all three attribute values reached the
PL/SQL parser, including one planted as a decoy (`Name="select x from dual_decoy"` became a
SELECTS edge). Written inside — `(node @cap (#eq? @cap "x"))` — the same predicate keeps only the
intended match. This is not an engine bug: it is query syntax that misleads, and that is why it is
written here.

**The bug this fixed, and worth keeping as a testing warning:** the caller passed `innerOffset`
as "block line". Offset is how much to add to a 1-based line from the sub-parse, i.e.
`firstLine - 1` — so the lookup ran on the line BEFORE the block, and in an
indented XML the answer is the sibling above. Measured on a real corpus: every DML edge from every
embedded block of a flow came from an `Element` called `key`. The unit test for
`attributeToHostEntity` passed, because it always received an absolute line; the one that was wrong was the
caller, which no test exercised. The fix came with a test through the real
parse path — `TestEmbeddedBlockIsAttributedToTheContainingUnitNotTheSiblingAbove`.

### 3c. Fragment: the body that is not a compilation unit

A block does not need to contain a full program. An XML-exported screen stores its
program unit as `PROCEDURE x(…) IS … END;`, which in PL/SQL is a **declaration** — valid
only inside a declarative section. On its own it parses as nothing: MEASURED on a real corpus,
those bodies yielded zero entities, zero calls and zero DML, and the only thing they
produced was the word `PROCEDURE` as a call target (on the live graph there were 19045
calls, ALL keywords). Wrapped in `DECLARE … BEGIN NULL; END;`, the same body
delivers the procedure, its cursors and everything it calls — measured on the same file:
34 → 50 calls, 11 → 17 SELECTS, 101 → 129 PL/SQL entities.

```yaml
embedded:
  - pattern: '(Attribute (Name) @_a (AttValue) @body (#eq? @_a "ProgramUnitText"))'
    text_capture: body
    normalize: forms_attr
    wrap_prefix: 'DECLARE '
    wrap_suffix: ' BEGIN NULL; END;'
    host_labels: [FormProgramUnit]
    default: plsql
```

Why it lives on the BLOCK and not on the language: which wrapper a fragment needs is a fact of
POSITION, not of the language — the same PL/SQL in a `.sql` file arrives with `CREATE OR REPLACE`
in front and needs nothing. Who knows what that attribute carries is the grammar that
declares the block.

**Neither side may contain a line break**, and a declaration containing one is
discarded at load — the same invariant as `text_normalizers`, for the same reason: every line
the sub-parse reports is shifted by the block's start line. A prefix on the first line
and a suffix after the last cost COLUMNS, which nothing records, not lines.

### 4. Bump of `shardCacheVersion`

3 → 4. The cache is keyed by content hash, so changing what the parser produces does not move
the key: without the bump, the change would reach only files edited afterwards, and every already
indexed project would keep running the new binary with the old graph.

## Extensibility: what is YAML and what is code

The question that matters after all this is "can I change this without recompiling?". The
answer is yes for everything that decides **which grammar parses which block**, and the boundary
is here intentionally.

### YAML only — no Go line

| What you want | How |
|---|---|
| **Map a new language onto a block** — `<script type="application/json">` becoming real JSON | One line in `languages:`. Covered by `TestMappingANewInnerLanguageIsYAMLOnly`, which adds `application/json: json` to `html.yaml` and expects `Pair`/`Value` at the absolute line. |
| Change which attribute selects the language | The `pattern` captures the value and `lang_capture` names it. That is why Vue/Svelte read `lang` and HTML reads `type` with no special case in the engine. |
| Change a block's default language | `default`. |
| **Stop mapping** a language — stop parsing a kind of block | Remove the key from `languages:`. An unmapped value is skipped silently, so `languages:` is an **allowlist**, not a denylist. |
| Declare a new block in a file that already has a grammar | Another entry in `embedded:`. As many as needed, and ORDER decides who claims a body first. |
| Select by node name, by regex, or by context | The `pattern` is a tree-sitter query: `#eq?`, `#match?`, sibling anchors, nesting. That is how `<execute>` in XML became project config. |
| Enable it in a language that currently has no blocks — Astro, Markdown, Handlebars | An `embedded:` in its YAML. Nothing else. |

The single requirement is that the inner language **exists**, i.e., has a query file: that is where
the extension registration comes from, and `tsLangConfigByName` resolves by name from the same
tables. Every language we ship is eligible — `json`, `css`, `typescript`, `tsx`,
`javascript`, `yaml`, `toml`, `xml`, `bash`, `python`, …

A language we **do not** ship needs its grammar first, which is the same requirement as
parsing a file of that language — nothing specific to embedded blocks: native
grammar (`treesitter_native.go` + Makefile) or `.so` dynamically loaded via Hub.
`TestEveryShippedEmbeddedBlockNamesRealGrammarNodes` fails if a `languages:` points to a
language that does not exist, and `TestEveryShippedEmbeddedPatternCompiles` if a pattern does not
compile — neither passes silently.

### It is code

One thing, intentionally:

- **`maxEmbedDepth = 1`.** A block inside a block inside a block is not parsed. There
  is no real case: `<script>` holds TypeScript, and TypeScript does not declare blocks.

There was a second — the `attribute_name` / `attribute_value` kinds by which the attribute was
read. It **no longer exists**: the selector became a pattern, and the pattern locates the language value in any
grammar. The first grammar to disagree with those constants (XML,
`Attribute` → `Name` / `AttValue`) was resolved by deleting the problem, instead of turning it into two
config fields nobody else would need.

## Decisions

The six questions that were open, with the chosen option. All followed the recommendation.

| Question | Chosen | Why |
|---|---|---|
| Entity from the script contained by the `Element` "script" or by the `File`? | **`File`** | A top-level function in a `<script setup>` is top-level of the module; hanging it on the element invents a level the author did not write. It comes for free: the inner parse's `contextResolver` only sees the block, so a top-level declaration resolves to an empty context and falls to the `File`. |
| `File.lang` of a `.vue` stays `vue`? | **Yes** | The file is an SFC, and "which files are Vue" must keep working. Verified end-to-end: `.vue` → `vue`, `.svelte` → `svelte`. The `Import` generated by the block also gets `lang` from the outer file, for the same reason. |
| Column offset on the first line | **Ignore** | `raw_text` starts after the `>`, so the first line has shifted columns — but only lines are stored (`line_number`, `end_line`), so there is nothing to fix. |
| `lang="scss"`, `lang="jsx"`, no grammar | **Skip silently** | As already done for extensions without a grammar. A WARN per block would be one log line per file in a project full of them. What **does** warn is what cannot be right: a `node` the grammar does not have, once per language via `embedWarnOnce`. |
| `is_exported` of the block | **From the inner language** | Who decides TypeScript exports is `typescript.yaml`. Verified: `export function useCounter` → `is_exported: true`; `function reload` in the same file → `false`. |
| Applies to HTML inline and Markdown code fences? | **HTML yes, Markdown no** | HTML was included because enabling it required changing **no** existing test assertions — the condition the scope set. Markdown stays out: a fence body is documentation example, and indexing it would create `Function` nodes nobody can call. |

## Dynamic block selector, and ANTLR backend

> **Status: implemented.** Cut 1 (dynamic selector) and cut 2 (ANTLR backend) on
> 2026-08-05/06. Request: be able to say,
> via project config, that in XML the content of `<execute>select * from xpto</execute>` is
> SQL — that the same holds for multiple node names or for an attribute `type = sql`, and that the
> selector be **dynamic**, able to express possibilities nobody has enumerated yet.
>
> ✅ **The literal example works.** Verified on the live graph: an `<execute>` in XML, with
> `default: plsql` declared only in the project's `xml.yaml`, produces
> `Table pedido@5`, `Column id@6`, `Column status@7`, `Procedure p_lista@11` and the edge
> `p_lista -[:SELECTS]-> xpto @13` — all at absolute lines, leaving the `.xml`.
>
> **Unified.** There are not two forms: the selector is a pattern, and it is the same path for
> the `<script>` of a Vue and the `<execute>` of a project XML. `node`, `text` and
> `lang_attribute` were removed, along with the kind walk and the
> `attribute_name` / `attribute_value` constants of the engine.
>
> **The config names the LANGUAGE; which backend parses it is the engine's problem.** `plsql`
> (ANTLR) and `sql` (tree-sitter) are both valid answers for the same block, and whoever
> writes the YAML should not need to know the difference. This splits the work in two, and the
> first already solves the whole example:
>
> | | what it delivers | depends on |
> |---|---|---|
> | **Cut 1** — dynamic selector | `<execute>` parsed as `sql`, which is tree-sitter and **already works today** | nothing beyond itself |
> | **Cut 2** — ANTLR backend in the block | the same configs now accept `plsql`, `postgresql`, `tsql`, `db2`, `cobol85` | cut 1 to be useful |

### What already works today, without code

**The override is the same as for queries**: project > user > runtime chain. A project puts
`xml.yaml` in `.graphit/ast/queries/` with an `embedded:` and it is done — `hasLangConfig` already counts
`Embedded`, so the file is valid.

A note worth writing down: **override is per language, not merged.** If the project's `xml.yaml`
has only the `embedded:`, `resolveQueriesForLang` returns only it and the XML queries from the
runtime **disappear**. The correct flow is the one already documented in the manual: copy the
entire file and edit it.

### The two holes, both closed

**1. The selector is a node KIND, and that is too static.** In tree-sitter-xml `<execute>` is
an `element` like any other — the name lives in `STag/Name`:

```
(element (STag (Name) "execute") (content (CharData) "select * from xpto") (ETag ...))
```

`node: element` would match **every** element in the file. And a slightly richer selector —
`names: [execute, statement]` — would solve the example and nothing beyond it: it does not express "element
whose name matches `^sql[A-Z]`", nor "only when inside `<mapper>`", nor "when it has
the `type` attribute, whatever the tag name is". Enumerating possibilities is the wrong shape
for the problem.

The engine **already has** a dynamic, declarative, tested and
compiled node-selection language: the tree-sitter pattern. Every `queries[].pattern` is one. Using the same thing here does not
invent new vocabulary and brings for free predicates (`#eq?`, `#match?` with regex), sibling
anchors, alternation, nesting and the validation path that already exists (`sitter.NewQuery`, and
the net of `TestEveryShippedQueryPatternCompiles`).

**2. Only tree-sitter languages were accepted as inner languages.** `tsLangConfigByName("plsql")`
returned `false`, and the five ANTLR languages — `plsql`, `postgresql`, `tsql`, `db2`,
`cobol85` — could not be a block's language. Naming one of them made the block
skipped silently, the worst possible outcome because it looks like it worked.

**Closed by cut 2.** `embeddedLangConfig` resolves by name across both
backends, and `parseEmbeddedBody` dispatches to what the language requires. The config names the
LANGUAGE; which backend parses it is the engine's problem.

### The shape, as shipped

A block is **always** a pattern. There is no simple form and general form — there is one, because "which
node is this block" is the same question in an SFC and in XML, and only a query answers it in general.

```yaml
# vue.yaml — the `lang` attribute is OPTIONAL, so two blocks
embedded:
  - pattern: '(script_element (start_tag (attribute (attribute_name) @_a (quoted_attribute_value (attribute_value) @lang))) (raw_text) @body (#eq? @_a "lang"))'
    text_capture: body
    lang_capture: lang
    languages: { ts: typescript, js: javascript, tsx: tsx }
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    default: javascript
```

```yaml
# project xml.yaml — same mechanism, different grammar
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#eq? @_tag "execute"))'
    text_capture: body
    default: sql
```

**ORDER is part of the design, not an accident.** The first block whose pattern matches a given body
node **claims** that node; the following ones ignore it. This is how two patterns
express an optional attribute — the specific one first, the generic one as fallback — with no special case
in the engine.

**The claim happens on MATCH, before the language is resolved.** This is the difference
between correct and almost correct: a `<style lang="scss">` matches the specific block, which maps
`languages: {}` and resolves no language — and the claim is what prevents the
generic block behind it from taking the same body and parsing SCSS as CSS.

Therefore `languages: {}` **explicit** is a meaningful declaration, distinct from
`languages` absent: "match, claim, and map nothing". YAML distinguishes the two, and the
validator does too.

What this removed from the engine, and it is not trivial: the kind walk, `embeddedTextNode`,
`embeddedAttrValue`, and the `attribute_name` / `attribute_value` constants plus the depth
limit of the attribute search. The pattern locates the language value in any
grammar, so the first one to disagree with those constants — XML, `Attribute` → `Name` /
`AttValue` — stopped being a problem instead of becoming two config fields.
### Normalization: the escaped body, and why the engine knows no escaping

An embedded block in XML **is not pure text**, and this is not a detail: `<` and `&` are
markup, so a `WHERE qt > 0` arrives in the file as `qt &gt; 0` and the host grammar
splits the content into `CharData` / `EntityRef` / `CharData`.

Consequence for the selector: capturing `(CharData)` grabs **only the first piece**, and the
sub-parse receives a statement truncated at the first comparison operator — silently,
which is the worst outcome. The block must capture the entire `content`, and then the body arrives with
entities still encoded.

```yaml
# in the language YAML — or in the project override, where a specific case should
# live: in `ast.queries_dir` (default `.graphit/ast/queries/`), and with `merge: true` at
# the root, otherwise the project file REPLACES the runtime XML and takes its queries
# away. With merge, `text_normalizers` is merged key by key and the `embedded`
# blocks from the project come BEFORE those the language already had.
text_normalizers:
  xml_entities:
    replace:
      "&lt;": "<"
      "&gt;": ">"
      "&amp;": "&"
      "&quot;": '"'
      "&apos;": "'"
    numeric_char_refs: true      # &#62; and &#x3E;

embedded:
  - pattern: '…(content) @body…'
    text_capture: body
    normalize: xml_entities
    default: plsql
```

**The engine knows no escaping scheme.** There is no entity table in Go, no
"XML mode". How a language escapes its text is a fact of that language, so it lives in its YAML
— same principle as `context_types` and `embedded` itself. A grammar that escapes
differently, or one nobody has encountered yet, declares its own and a block names it.

**Named in the language, chosen by the block**, and both halves are necessary: an
escaping scheme belongs to the language, but NEEDING it belongs to the position. The content of an
XML element is escaped; the `raw_text` of an HTML `<script>` is not, although HTML also has
entities. That is why `normalize:` is opt-in per block, and why several blocks can
share a normalizer without repeating the table.

#### The invariant the engine enforces

**A normalizer must not change the COUNT of line breaks.** Every entity's line
from the sub-parse is shifted by the block's start line in the host file, so a
replacement that produced `\n` would move everything after it inside the block — swapping
a visible syntax error for a wrong line number, which is the failure mode this
entire module exists to avoid.

Two defenses, both passive:

- At **load time**, a pair whose replacement contains a line break is discarded with WARN. It is the
  only place where the check is total and cheap; at parse time it would be a scan per
  body.
- At **parse time**, `&#10;` and `&#xA;` are left as-is. Decoding them would add
  a line.

Also left as-is: anything not declared (a `&nbsp;` is not ours to
guess) and a bare `&`, which is far more likely an operator than a broken
entity. And the search for `;` is limited to 12 bytes — without it, a `&` used as an operator
would scan to the next semicolon anywhere in the statement.

Longer keys match first, so a scheme declaring `&amp;` and `&amp;amp;`
resolves the specific one: map order is not order.

#### Where the declaration should live

A **shipped** grammar should only declare a normalizer if the entire language needs
it. The case that motivated this — SQL inside a flow-tool XML export — is
specific to one project, and the declaration stayed in that project's `xml.yaml`, not in the
shipped one: the `project > user > runtime` chain already allows it, and the generalized grammar should not
carry what only one case uses.

Remember that override is **per language, not merged**: a project `xml.yaml` containing only
the new sections erases the runtime's XML queries. Copy the entire file and edit it.

### Cut 2 — the ANTLR backend, and why it was cheap

`AntlrParser.parseWithConfig(path, ext, cfg, src []byte, …)` **already received the bytes**, and
`driver.Parse(src)` operates on them — there was never any disk read in there. And
`shiftParsedLines` / `mergeParsedInto` are backend-agnostic by construction: they operate
on `*ParsedFile`, not on the tree. It was the decision to apply the offset **in one place, at the
end**, made in the first cut for a different reason, that made this one almost free.

What was added:

1. `embeddedLangConfig(projectDir, name)` resolves by name across both backends, and
   `antlrLangConfigByName` is the counterpart of `tsLangConfigByName` — `antlrExtMap` answers "what
   parses .sql" and `antlrGrammarMap` "what is antlr-plsql", and neither answers "what is the
   language called plsql". Tree-sitter wins ties; nothing ships under both names today.
2. `parseEmbeddedBody` dispatches on what the language requires. The tree-sitter path passes the
   offset **into** `parseSource`, because that is where a nested block is resolved; the
   ANTLR path calls `shiftParsedLines` **afterwards**, because there is nothing inside it to
   resolve.
3. **Where the dispatch lives**: `TreeSitterParser` builds an `AntlrParser` on demand
   (`&AntlrParser{projectDir: t.projectDir}`, a one-field struct). Moving it up to
   `CompositeParser` would be purer by name, but would require the outer parse to return the
   tree, which it does not — a larger refactor for no observable gain. Measured, not
   assumed.

The "ANTLR language cannot be inner" warning was **removed**: it existed only while the
limitation existed. A language that does not exist in any backend keeps being skipped in
silence, which is the right policy for `lang="scss"`.

Verified on the live graph, with `default: plsql` declared only in a project `xml.yaml`:

```
src/changelog.xml
  Table pedido@5, Column id@6, Column status@7, Procedure p_lista@11
  p_lista -[:SELECTS]-> xpto @13
```

Absolute lines, DML edge leaving the `.xml`, and the sibling `<note>` producing nothing.

### `sql.yaml` matched almost nothing — fixed

Found by accident while writing tests for cut 1, and predating all of this.

**Only the `tables` pattern was broken**, not all three — `object_reference` assigns `name`
to its identifier, but `create_table` assigns no field to any child:

```
create_table child 0 kind=keyword_create     field=""
create_table child 1 kind=keyword_table      field=""
create_table child 2 kind=object_reference   field=""
create_table child 3 kind=column_definitions field=""
```

A pattern requiring a non-existent field **compiles** — the name exists somewhere in the
grammar, so `TestEveryShippedQueryPatternCompiles` passes — and matches **zero** times. It is the
failure mode of "broken query pattern is a silent no-op", with a second layer on
top: `CompositeParser` tries tree-sitter, sees zero and falls back to ANTLR, so the `.sql` seemed
to work.

Fixed by removing the `name:` keys. And, since the grammar was being fixed, `sql.yaml`
gained what it was missing to be useful as an **inner** language — which is where the defect came from:

- `columns`, with `parent_capture` for the table: `column_definitions` is not a context type, and
  without it a column would have no owner. A table without columns is half a table, and the ANTLR side
  always had them.
- `table_refs`: the tables a `SELECT` reads, as `REFERENCES` and not as a `Table` entity
  — the table is *used* there, not declared, and creating a `Table` node would make it look like a
  declaration. Without it `select * from xpto` contributed absolutely nothing.

Tests live in `sql_treesitter_test.go` and **all call `parseSource` directly**, which is
pure tree-sitter. A test written on `parseFixture`/`CompositeParser` would pass with the
defect in place — exactly how it survived.

#### Decided: tree-sitter is the DEFAULT parser for `.sql`

The question stayed open for a day and was answered: **the tree-sitter grammar is the default**,
and a dialect project opts in.

The opt-in is persistent and has existed since before this — `ast.grammar`, a map
`.ext=grammar` resolved from project config (`config.ResolveGrammarOverrides`) and
respected by the daemon, not just by the `--grammar` flag:

```json
{ "ast": { "grammar": ".sql=antlr-plsql" } }
```

With this `CompositeParser` uses the dialect directly, without fallback.

The decision has a consequence that guided the rest of the work: if tree-sitter is what
parses most `.sql` in a project, it must **cover what a `.sql` contains** —
fixing `create_table` alone was not enough. Hence `columns`, `create_index` and the full DML
(`SELECTS`, `INSERTS`, `UPDATES`, `DELETES`, `ALTERS`), with the **same edge types** that
the ANTLR dialects produce, so "who reads this table" is a single question,
regardless of which backend parsed the file.

Measured on the same file, before and after:

| | before | after |
|---|---|---|
| ANSI DDL — tree-sitter | 1 (View only) | **4** |
| Oracle PL/SQL — tree-sitter | 1 | **3** |
| Oracle PL/SQL — antlr-plsql | 4 | 4 |

What tree-sitter still does not see in an Oracle file is the `Procedure`, which is PL/SQL and not
ANSI — exactly the case `ast.grammar` exists to solve.

##### DML edges arrive, even without a surrounding entity

This was a limitation for two days and is now fixed. Worth recording because the
fix ended up being much smaller than estimated, and because discovering it required
the live graph.

**The problem.** A DML edge only arrived when the statement had a surrounding entity.
`p_lista -[:SELECTS]-> xpto` arrived, coming from a block with a `PROCEDURE` inside; a
`insert into auditoria …` at the top of a script did not. `ConvertToCache` left
`SourceUID` empty when there was no `SourceName`, and the writer discarded it — the same
pattern as the Import entity, which was built and thrown away with a `continue`.

In a schema `.sql`, and in SQL from an embedded block, this is the MAJORITY of DML: a
`<value>` configuration contains bare statements by nature.

**The fix, and what I estimated wrong.** I said it would require "four writer sites
plus the `File → Table` pair in the relation-tables group". I was wrong about
the schema: `ladybug.go` **already emitted** `FROM File TO <target>` unconditionally for each
DML type. The schema always supported the file as origin — what was missing was someone
to fill it. There were three small changes:

- `ConvertToCache`: `sourceUID = relPath` when there is no surrounding entity. The file
  as origin is the form `IMPORTS` already uses (`File -[:IMPORTS]-> Module`).
- `refSourceLabel`: resolves the origin's label, `File` when the UID is a known file
  path.
- `json_rebuild`: a COPY step for edges with file origin, mirroring the
  step DDL already had.

`File` **does not** go into `dmlSourceLabels`, and that is the detail the live graph caught:
as DDL already declares the pair, announcing it there would emit it twice and LadybugDB rejects the
entire group with `Found duplicate FROM-TO File-Table pairs`.

**A latent defect that came along.** The first rebuild with embedded blocks aborted
with `Table Function does not exist`. `stubFunctionJSON` creates a `Function` node for every
unresolved call target, but the table was only created when some call had a
CALLER — and a call at the top of a script has no surrounding entity. The rows were
emitted against a non-existent table and the whole rebuild fell over. It predated this
work and applied to any language with a top-level call; the bare SQL of an
embedded block just made it reachable. Now the table exists whenever there is a call target.

**Symmetry was closed next:** a CALL without a surrounding entity is also made
by the file. Applies to any language with a top-level call, not just the embedded SQL
that exposed the case — verified on the same index with a top-level `init()` in
JavaScript and an anonymous PL/SQL block.

There the path differs from DML, and the difference cost two hidden gates: the DDL for
`CALLS` has **no** fixed File step, it derives pairs from `CallerLabels`. So
`File` had to join the `validTypes` allowlist that decides what becomes `callerLabel`, and
the writer loop had to stop requiring `labelSet[cl]` — `File` is not in the
`labelSet` on purpose.

The second gate only appeared on the live graph: **the unit test called `callEdgeJSON`
directly and skipped the loop condition**, so it passed while the real path discarded
everything. It became `canWriteCallerLabel`, a function, so it can be tested. This is the third time
in this series where "green test" and "edge in the graph" were different things.

**Measured on the live graph**, on a flow-tool XML export with SQL in
processors: `File -[:SELECTS]-> …` for tables read, plus `INSERTS` and
`UPDATES`, all leaving the `.xml`.

##### Two traps found while writing DML

**One `data_key` per edge type.** `buildRelationTypeMap` (`internal/ast/helper.go`) maps
`data_key → relation_type`, so two queries with the same `data_key` and different types
collide and the **last wins, silently**. With everything under `dml`, every `SELECT`, `INSERT` and
`UPDATE` became `ALTERS`. Caught by test; there would be no way to notice in production.

**The `from` for DELETE does not have the same shape as the `from` for SELECT.** In both it is a SIBLING of the
statement, not a child — but SELECT wraps the table in a `relation` and DELETE does not. One
pattern cannot serve both, and the DELETE one anchors on `statement` so it does not capture
any `from`.

### Cost of a pattern: anchor siblings

This is the risk the next section predicted in theory, measured in practice — and it is worse
than written. It is not "one sub-parse per element": it is the PATTERN that explodes.

Measured on an 888 KB XML, ~500k lines, with ~44 blocks to extract:

| pattern | result |
|---|---|
| pair of children without anchors, inside a wrapper | **aborted at 60s**, 2 matches |
| anchor only between the two elements | 44 matches in **13.5s** |
| fully anchored | 44 matches in **327ms** |

`(element …)` matches EVERY element in the document. For each, the engine searches for the
`(key, value)` pair among the children of `content` — a quadratic search over that
`content`'s size — and in a deep XML this repeats at every ancestor. The `.` anchors make
the match positional and the cost linear.

Two things to remember when writing one:

- **Whitespace can be a named node.** In XML the indentation `CharData` counts
  for adjacency, so it must appear between the anchors.
- **A slow pattern gives no warning.** The index simply never finishes; what showed up was
  a test stuck for 10 minutes in `ts_query_cursor_next_match`, and only the stack said where.
  It is the same failure mode as the non-matching pattern, except this one burns
  the machine instead of staying quiet.

### Risks this cut has and the previous one did not

- **`content/CharData` is not a single node.** An `<execute>` with a comment or CDATA in the middle has
  several sibling `CharData` nodes, and one capture resolves **one**. Either the body starts concatenating
  siblings — and then the line offset stops being a single number, which is precisely the part that
  fails silently —, or the cut declares it only supports contiguous bodies. Decide with a real fixture
  before writing code, not by reading the grammar.
- **`element` is recursive.** The walk does not descend into a matched node, which remains correct; but an
  `<execute>` inside another `<execute>` needs a test.
- **Cost.** Today it is one block per SFC. A loose pattern on a 600k-element XML would be a
  sub-parse per element. At minimum the index must report how many embedded blocks
  were parsed — silence here reads as "there was nothing", just like any other undeclared truncation.

## Cost and risk

- **One second parse per block.** `parserPool` amortizes it; cost is proportional to the
  script size, not the project. The inner parse's `IndexSource` is forced to `false` — the outer
  parse already stores the entire source, and a second copy of the block would be discarded immediately.
- **Hot path.** Extracting `parseSource` touched the most complex function of the adapter.
  It was done without introducing a second disk read or a second `parserPool.Get`: the block body
  is a slice of `src`, not a copy.
- **New config surface.** Non-existent `node`/`text` in the grammar fail **open**
  (non-matching pattern is a silent no-op), so there are three safety nets: `validEmbeddedBlocks` in the
  query loader rejects malformed blocks at load, `TestEveryShippedEmbeddedBlockNamesReal‑
  GrammarNodes` covers the files we ship, and the engine warns once per language when
  the kind does not resolve. Same lesson as `parsePathSegment`.

## Acceptance criteria

Both pass, on tests and on the live graph.

```gherkin
Given an App.vue with <script setup> containing `import { ref } from 'vue'` on line 8
When the file is indexed
Then there is an IMPORTS edge from File App.vue to Module "vue"
  And the import statement entity is on line 8, not on line 1
  And a function declared in <script> is a Function entity with the correct absolute line
  And the Element "script" still exists, with its attributes

Given a <style lang="scss"> in a .vue
When the file is indexed
Then the block is skipped without error
  And the rest of the file is indexed normally
```

The offset is what fails silently, so **every** fixture puts the `<script>` after the
`<template>`: with the script at the top, a zero offset passes.
