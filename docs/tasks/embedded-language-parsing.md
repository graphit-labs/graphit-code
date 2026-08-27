---
title: Parse the embedded <script> and <style> of single-file components
status: done
created: 2026-08-05
updated: 2026-08-05
tags: [ast, tree-sitter, vue, svelte, html, embedded, imports]
---

# Parse the embedded `<script>` and `<style>` of single-file components

## Objective

Close the last open debt from
[add-vue-treesitter-grammar.md](add-vue-treesitter-grammar.md): the body of
`<script>` and `<style>` in a `.vue` or `.svelte` was a single opaque `raw_text`
node, so a single-file component contributed **no `IMPORTS` edge, no entity from
its script, and no export flag**. In a Vue project the dependency graph simply
did not contain the components.

Scope taken: `<script>` and `<style>`, in Vue and Svelte, with language selection
by the `lang` attribute — plus HTML inline, which was conditional on the html
suite staying green with no assertion change. Markdown code fences are out.

The design was already written in
[embedded_language_parsing.md](../specs/embedded_language_parsing.md) and was
followed; the spec now records the chosen answer for each of the six open
decisions.

## What was NOT built, and why

- **Markdown code fences.** A fence body is a documentation example, not a
  declaration of this project. Indexing it would create `Function` nodes nobody
  can call, `Import` statements to modules the project does not depend on, and an
  `IMPORTS` edge out of a `.md`. The mechanism is declarative, so if this is ever
  wanted it is an addition to `markdown.yaml` and nothing else.
- **Configurable attribute node kinds.** The engine reads the language attribute
  through the constants `attribute_name` / `attribute_value`. Making those config
  would add two knobs with no user: all three HTML-shaped grammars agree on them,
  verified by dumping each tree. A grammar that disagreed would resolve no
  attribute and fall back to the block's `default` — a skip or a default, never a
  wrong language — so the failure mode is benign and the field can be added the
  day a grammar needs it.
- **A WARN per skipped block.** `lang="scss"` is ordinary Vue and has no grammar
  here. A warning per block is a log line per file in a project full of them, so
  the skip is silent. What *does* warn — once per language, via `embedWarnOnce` —
  is a config that cannot be right: a `node` kind the grammar does not have, two
  blocks declaring the same node, or an inner parse that failed outright.
- **`exports.strategy` on `vue.yaml` / `svelte.yaml` changed away from `none`.**
  It stays `none` because it now governs only the *markup*, which has no export of
  its own. The block's `is_exported` comes from the inner language, which is the
  decision the spec recommended and the one that is actually right: what an
  `export` is, is a fact about TypeScript.

## Use Cases

### UC-01 — A component's imports reach the dependency graph

```gherkin
Given um App.vue com <script setup> contendo `import { ref } from 'vue'` na linha 8
When the file is indexed
Then existe aresta IMPORTS do File App.vue para o Module "vue"
And the entity Import statement is on line 8, not line 1.
And an declared function in the <script> is an entity of type Function with the correct absolute line.
  And o Element "script" continua existindo, com seus atributos
```

Verified in `TestVueScriptBodyContributesImportsAndEntitiesAtAbsoluteLines` and
end-to-end on a scratch project:

```
MATCH (f:File)-[:IMPORTS]->(m:Module) WHERE f.path ENDS WITH '.vue' RETURN f.path, m.name
→ src/App.vue   | vue
→ src/App.vue   | src/OrderList.vue
→ src/Typed.vue | vue
→ src/Typed.vue | pinia
```

### UC-02 — A block whose language has no grammar is skipped, silently

```gherkin
Given um <style lang="scss"> num .vue
When the file is indexed
The block is jumped without error.
And no CSS entity is created from it.
  And o Element "style" e seus atributos continuam existindo
And the rest of the file is indexed normally.
```

`TestVueStyleWithUnknownLangIsSkippedWithoutError`. Confirmed in the live graph:
`Typed.vue` has `Element style` and `AttributeValue scss` at line 18 and nothing
else from that block, while its `<script lang="ts">` produced `Interface Props`
and `Function useCounter`.

### UC-03 — The `lang` attribute selects the grammar

```gherkin
Given um <script setup lang="ts"> declarando `interface Props`
When the file is indexed
Then existe entidade Interface "Props" na linha absoluta correta

Given um <script setup> sem atributo lang
When the file is indexed
The body is parsed like JavaScript.

Given um <style> sem atributo lang
When the file is indexed
The body is parsed as CSS.
```

`TestVueScriptLangAttributeSelectsTypeScript`, `TestVueStyleBodyIsParsedAsCSS`.
`interface` is TypeScript-only, so seeing it is proof that the ts grammar ran and
not the js one.

### UC-04 — The line offset is absolute, and tested where it can fail

```gherkin
Given an embedded block that does not start on line 1 of the file
When the file is indexed
Then every line reported by the internal parser is an absolute line of the file
  And isso vale para entidades, CallSites e References
```

`TestShiftParsedLinesMovesEveryLineBearingRecord`,
`TestShiftParsedLinesZeroIsANoOp`. Every fixture in the suite puts the `<script>`
**after** the `<template>` — with the script first, an offset of zero passes and
the bug ships.

Live check, `src/App.vue`: `Element script`@8, `Import vue`@9,
`Import ./OrderList.vue`@10, `Variable title`@12, `Variable orders`@13,
`Function fetchOrders`@15, `Function reload`@19, `CALLS reload→fetchOrders`@20,
`Element style`@25, `CssClass wrap`@26, `CssProperty color`@27.

### UC-05 — Svelte gets the same treatment

```gherkin
Given um Card.svelte com <script> contendo `import { onMount } from 'svelte'`
When the file is indexed
Then existe aresta IMPORTS para o Module "svelte"
  And `export let title` marca a entidade como exportada pela regra do javascript
  And o <style> produz entidades CSS
```

`TestSvelteScriptBodyContributesImportsAtAbsoluteLines`.

### UC-06 — HTML inline, and only when it is really code

```gherkin
Given um index.html com <script type="module"> contendo um import
When the file is indexed
Then existe aresta IMPORTS do File index.html

Given um <script type="application/json"> no mesmo arquivo
When the file is indexed
Then the body is not parsed as JavaScript.
```

`TestHTMLInlineScriptAndStyleAreParsed`. The discriminator for HTML is `type`,
not `lang` — HTML has no `lang` on a script — and only the values that *are*
JavaScript are mapped, so a JSON payload, an import map and a
`type="text/x-template"` are skipped rather than parsed as code. This is the
`languages` mechanism doing exactly what it was designed for.

UC-07 - Mapping a new language is just YAML

```gherkin
Given the HTML.yaml block that maps only values that are JavaScript
When eu acrescento `application/json: json` ao `languages:`
Then o corpo de <script type="application/json"> produz entidades Pair e Value
They are at the absolute line of the file.
  And o markup do elemento continua intacto
  And nenhuma linha de Go foi alterada
```

Yes, because extensibility is a promise that only holds if it's fixed by test: `languages:` is an **allowlist**, so adding and removing mappings are the two operations users will want to do, and both are lines of YAML.

The unique condition is that the internal language exists — there is a query file from which the extension registration comes. Every language we send is eligible.
`TestEveryShippedEmbeddedBlockNamesRealGrammarNodes` fails if an `languages:` points to a language that does not exist.

See "Extensibility: What is YAML and what is code" in the spec for the complete border,
including the two things that **are not** configurable
(`attribute_name`/`attribute_value` and `maxEmbedDepth`) and why.

UC-08 - A Dynamic Selector, AND A Mechanism Only

```gherkin
Given um xml.yaml de projeto declarando
  pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#eq? @_tag "execute"))'
When an .xml file with `<note>` and `<execute>` tags is indexed
Then only the body of [execute] goes into internal grammar
And the entity is on the absolute line.
  And o markup do XML fica intacto

Given `#match? @_tag "^sql"` em vez de `#eq?`
Then home and other things—many names without listing any

Given um <script setup lang="ts"> e um <style lang="scss"> no mesmo .vue
When the file is indexed
The specific block of code wins, and TypeScript runs.
And the specific block of the style claims the body and maps nothing.
And the generic block does not parse SCSS as CSS
```

`TestEmbeddedPatternSelectsElementByName`, `TestEmbeddedPatternSelectsByRegex`,
`TestEmbeddedPatternReadsLanguageFromACapture`, `TestEmbeddedPatternKeepsAbsoluteLines`,
`TestFirstMatchingBlockClaimsTheBodyAndTheRestSkipIt`,
`TestGenericBlockFiresWhenTheAttributeIsAbsent`.

The selector was a node KIND, and this does not express the problem: in tree-sitter-xml
`<execute>` is an `element` like any other. Enumerating names would provide only one example.
The engine already has a dynamic node selection language compiled and tested — the pattern tree-sitter — and now it is the **only**:
`node`, `text`, and `lang_attribute` were removed along with the walk by kind,
`embeddedTextNode`, `embeddedAttrValue`, and constants
`attribute_name`/`attribute_value`.

Translation: The selector was a node KIND, and this does not express the problem. In tree-sitter-xml, __INLINE_77__ is an __INLINE_78__, just like any other. Enumerating names would provide only one example. The engine already has a dynamic node selection language compiled and tested — the pattern tree-sitter — and now it is the **only**:
`node`, `text`, and `lang_attribute` were removed along with the walk by kind,
`embeddedTextNode`, `embeddedAttrValue`, and constants
`attribute_name`/`attribute_value`.

This translation maintains the technical terms and structure of the original Portuguese text while converting it into idiomatic English.

The ORDER has become part of the design: the first block whose pattern fits a body
the **reclaim**, and the reclaim occurs in marriage, before language resolves. This is what allows an optional attribute to fit into two patterns without special cases,
and prevents `lang="scss"` from falling into `default: css` within the generic block.

Consequence of config: **inline** became a statement with meaning — "case, request, do not map anything" — distinct from **inline** missing. YAML separates the two and the validator also does.

### UC-09 — Uma linguagem ANTLR pode ser a linguagem interna

```gherkin
Given um xml.yaml de projeto com `default: plsql` num bloco <execute>
When an `.xml` file containing `CREATE TABLE` and `CREATE PROCEDURE` within `<execute>` is indexed
Then existem Table, Column e Procedure na linha ABSOLUTA
  And existe a aresta SELECTS da procedure para a tabela lida
  And ambas saem do arquivo .xml
  And o markup do XML fica intacto
```

`TestEmbeddedBlockCanBeParsedByTheANTLRBackend`,
`TestEmbeddedANTLRBlockProducesDMLEdges`, `TestEmbeddedLangResolvesAcrossBothBackends`.

Verified in the live graph: `Table pedido@5`, `Column id@6`, `Column status@7`,
`Procedure p_lista@11` and `p_lista -[:SELECTS]-> xpto @13`, all with
`source_file = src/changelog.xml`.

It was cheap because `AntlrParser.parseWithConfig` had already received `src []byte` and `shiftParsedLines`/`mergeParsedInto`, which were agnostic to backend — the first cut applied the offset in a place that only cost here.

The warning "ANTLR language cannot be internal" has been removed: it only existed while the limitation was in place. Any language that does not exist on the backend continues to jump silently.

UC-10 - Inline 103 extracted almost nothing, and the fallback hid.

```gherkin
Given `create table cliente (id integer, nome varchar(60));`
When parsing the file using the Tree-Sitter parser, isolated from the fallback ANTLR.
Then existe Table "cliente" e as Columns "id" e "nome" contidas por ela

Given `select nome from cliente join pedido on ...`
Then there are references for "client" and "order."
```

The only pattern that was broken was the `sql_treesitter_test.go`. It required a field in ___INLINE_105__, which did not assign the field to any child. It compiled and married zero. ___INLINE_106__/___INLINE_107__ always worked because their ___INLINE_108__ is in ___INLINE_109__, which indeed has that field.

All tests in there call ___INLINE_112__ directly. A test about ___INLINE_113__ would pass with the defect, because `CompositeParser` falls into ANTLR when it sees zero — exactly how he survived.

Taking advantage of, **INLINE_115** gained **INLINE_116**, and the tables read by a **INLINE_117**, without which **INLINE_118** would contribute nothing to a block embedded.

Decided: the tree-sitter is the default parser for `.sql`, and an inline dialect project opts-in to `ast.grammar` (`{"ast": {"grammar": ".sql=antlr-plsql"}}`), which is persistent and respected by the daemon, not just the flag `--grammar`.

This translation maintains the technical terms and structure of the original Portuguese text while converting it into idiomatic English.

This changed the scope of the repair: if tree-sitter parses most of the `.sql`, it needs to cover what a `.sql` has. Therefore, in addition to `create_table`, `columns`, `create_index`, and the complete DML — `SELECTS`, `INSERTS`, `UPDATES`, `DELETES`, `ALTERS` — with the same edge types of the ANTLR dialects, "who reads this table" is a single question. Measured: ANSI DDL from 1 to 4 (parity with ANTLR), PL/SQL from 1 to 3 against 4 of ANTLR, missing only `Procedure` — exactly the case of opt-in.

---

Note: The code block and markdown are not provided in this translation.

Two traps, both caught in tests but none visible in production:

- **An inline view for the type of edge.** It maps
  __LINE__136__, then queries with the same __LINE__137__ and different types collide, and the last wins silently. With all under __LINE__138__,
  every SELECT/INSERT/UPDATE became __LINE__139__.
- **The DELETE inline view does not have the form of the SELECT inline view.** Both are siblings of the statement, but the SELECT wraps the table in a __LINE__142__, while the DELETE does not.

Resolved — see UC-13. This was a limitation for two days: the DML edge only reached the graph when the statement had surrounding entities. In an INLINE_143 schema and in SQL within a nested block, this was the majority of DML.

UC-11 - A body that escapes is normalized before the subparse

```gherkin
Given um bloco cujo corpo vem escapado pelo hospedeiro — `qt &gt; 0`
When o bloco declara `normalize:` apontando para um `text_normalizers` da linguagem
Then o corpo chega ao sub-parse com o texto que as entidades representam
And the count of line breaks does not change
And a pair whose replacement contains line break is discarded during the load.
And they remain as they are.
```

`TestTextNormalizerPreservesNewlineCount`, `TestTextNormalizerKeepsLinesAligned`,
`TestEmbeddedXMLBlockDecodesAndParsesTheWholeBody`,
`TestTextNormalizerRejectsAReplacementThatAddsALine`,
`TestEmbeddedNormalizeIsOptInAndMustExist`,
`TestBlockNamingANormalizerInAnotherLanguageIsDropped`.

Why did it become necessary: a block embedded in an XML **is not pure text**. `<` and `&` are markup, so `WHERE qt > 0` arrives at the file as `qt &gt; 0` and the grammar hostess parts the content into `CharData` / `EntityRef` / `CharData`. Capturing `(CharData)` captures only the first part — truncated statement in the first comparison operator, silently. The block captures the entire `content`, and the normalizer returns parseable text.

The engine does not know any escape scheme. There is no entity table in Go.
As a language escapes its text is the language's fact, so it lives in YAML — even principle
of `context_types`. Declared by the language, chosen by the block, because they are two different facts: the schema is of the language, but to require it is of position — the content of an XML element is escaped, the `raw_text` of a `<script>` HTML not.

And the declaration goes where there is need: the grammar **sent** only declares one normalizer if the entire language needs it. A specific case of a project goes in the override of that project, which the chain `project > user > runtime` already supports.

UC-12 - The block pattern requires anchoring, and measurement is brutal

```gherkin
Given um documento XML grande e profundo
When an embedded block houses a pair of children without anchors
The cost is quadratic for `content` and repeats in each ancestor.
```

Measured in an 888 KB XML document (~500K lines):

Portuguese:
| pattern | resultado |
|---|---|
| without anchor, with external wrapper | **aborted in 60 seconds**, 2 matches |
| only anchor between the two elements | 44 matches in **13.5 seconds** |
| fully anchored | 44 matches in **327 milliseconds** |

English:

Without anchors, the inline 143 house CADA element of the document and, for each one, searches for a pair between the children of inline 144 — a quadratic search that repeats in every ancestor of a deep tree. The anchors inline 145 make positional marriage possible.

Found the way it was supposed to go: a test stayed 10 minutes stuck in
`ts_query_cursor_next_match` and only the stack told where. **A slow pattern doesn't warn** —
the simple index just stops. It's the same failure mode as a non-matching pattern, with
the difference being that this one consumes the machine.

In a grammar where whitespace is named — INLINE_167 in XML — it counts for adjacency and must be written between anchors.

UC-13 - A reference without a surrounding entity belongs to the FILE

```gherkin
Given um statement no topo de um script, sem procedure em volta
When the file is indexed
Then existe a aresta DML com o File como origem

Given o mesmo statement dentro de uma unidade nomeada
Then the origin remains the unit, not the file.
```

`TestReferenceWithNoEnclosingEntityIsSourcedAtTheFile`,
`TestFileSourcedDMLEdgeReachesTheGraph`, `TestEntitySourcedEdgeIsUnchanged`.

The writer left the ___INLINE_172__ empty when there was no ___INLINE_173__, and he discarded — even with the entity of Import, built and thrown away with a `continue`. The source file is the form that `IMPORTS` already uses (`File -[:IMPORTS]-> Module`): "what this table touches" is a question about the file when there is nothing smaller than what can be named.

**Correction of an Estimate I Made.**
I stated that I would require four points in the writer and also include __INLINE_177__ in the group of tables of relationships. Incorrect about the schema:
`ladybug.go` **already emitted** `FROM File TO <alvo>` conditionally. The schema always supported; it was just missing filling. There were three small changes — `ConvertToCache`, `refSourceLabel`, and a COPY step in `json_rebuild` mirroring what the DDL already had.

And `File` does not enter `dmlSourceLabels`: as the DDL already declares the pair, announce it there emits twice and LadybugDB rejects the group with `Found duplicate FROM-TO File-Table pairs`. Picked up from the live graph, not from unit tests.

### UC-14 — Defeito latente: `Table Function does not exist`

```gherkin
Given an file whose calls do not have surrounding entities
When the graph is reconstructed
Then the table function exists, and the rebuild does not abort.
```

Inline 186 creates a node Inline 187 for every unresolved call target,
but the table was only created when some calls had CALLER. A call at the top of a script does not have surrounding entity, so Inline 188 was empty, stub lines were emitted against an nonexistent table and the **full rebuild aborted**.

Before this work and valid for any language with top-level call; the SQL freed from an embedded block only made it accessible. It appeared in the live graph — the message goes to a logger that is NOP when no one passes through, so the index said merely "1 COPY operation(s) failed".

The symmetry that was missing was closed right away—see UC-15.

UC-15 - A call without an entity around it is made by the FILE

```gherkin
Given uma chamada no topo de um script, sem entidade em volta
When the file is indexed
Then existe a aresta CALLS com o File como chamador

Given the same name inside a function or procedure
The caller remains her, not the file.

Given qualquer chamador
Then `inline 0` is not declared: an archive has never been a target of calling
```

`TestCallWithNoEnclosingEntityIsCalledByTheFile`, `TestFileCalledEdgeReachesTheGraph`,
`TestFileIsNeverACallTarget`.

Symmetric to UC-13, and valid for any language with a top-level call— not just SQL embedded that exposed the case. Verified in live graphs, in one index only:

```
File app.js       -[:CALLS]-> init             (init() de topo em JavaScript)
Function boot     -[:CALLS]-> init             (contida, inalterada)
Brazilian Portuguese to idiomatic English:

"Create file 'carga.sql' and establish a call relationship with 'p_carga_diaria' in the anonymous PL/SQL block."
Procedure p_principal -[:CALLS]-> p_log        (contida, inalterada)
```

Three doors, each concealing the next. Unlike DML, the DDL of __INLINE_192__
does not have a fixed step file; it derives pairs of __INLINE_193__. Therefore, it was necessary to

Note: The placeholders "__INLINE_192__" and "__INLINE_193__" are likely code snippets or identifiers that should be replaced with actual values.

1. Pass the file as caller,
2. Enter the allowlist of `validTypes`, which decides what becomes `callerLabel`,
3. The writer loop stops requiring `labelSet[cl]` — `File` is not in the `labelSet` for that purpose, to avoid emitting the table of nodes twice.

The third only appeared in the live graph: **the unit test called `callEdgeJSON` directly and skipped over the gate**, so it passed while the real path discarded everything. The gate turned into `canWriteCallerLabel`, a function, just to be able to be tested.

### UC-16 — A broken `embedded` config is rejected, not carried

```gherkin
Given um bloco embedded sem `node`, ou sem `text`
When the query file is loaded
The block is discarded with WARN.

Given um bloco embedded sem `default` e sem `languages`
When the query file is loaded
Then the bloc is discarded—no language could resolve

Given um `languages` sem `lang_attribute`
When the query file is loaded
Then the map is discarded with WARN, and the block survives by `default`

Given an inline 0 that the grammar doesn't have
When an instance of that language is parsed
Then there's a WARN once per language, not one per file.
```

`TestEmbeddedConfigDropsMalformedEntries`,
`TestEmbeddedConfigDropsUnreachableLanguagesMap`,
`TestEmbeddedConfigLowercasesLanguageKeys`,
`TestEveryShippedEmbeddedBlockNamesRealGrammarNodes`.

### UC-17 — The element's Text node survives

```gherkin
Given an `<script>` const `a = 1;` — body of an instruction only
When the file is indexed
Then the Text object with "const a = 1;" continues to exist.
And now the variable "a" also exists on the absolute line.
```

`TestSingleStatementScriptKeepsItsTextNodeAndGainsStructure`. The `element_text`
queries stay in all three grammars on purpose: this is addition, not replacement.

## Implementation Details

### Files created

- `internal/ast/treesitter_embedded.go` — the whole mechanism: the tree walk that
  finds blocks, the attribute read, the sub-parse, `shiftParsedLines` and
  `mergeParsedInto`.
- `internal/ast/embedded_parse_test.go`, `embedded_selector_test.go`, `embedded_antlr_test.go` e `sql_treesitter_test.go` — 35 tests.
- `docs/tasks/embedded-language-parsing.md` — this log.

### Files modified

- `internal/ast/treesitter_adapter.go` — `parseWithConfig` split into itself plus
  `parseSource(path, ext, cfg, src, lineOffset, embedDepth, isDepend, opts)`. Plus
  `tsLangNameMap` / `projectTsLangMap` / `tsLangConfigByName` / `primaryExtOf`:
  `tsExtMap` answers "what parses `.vue`" and `tsGrammarMap` "what is
  `tree-sitter-vue`", and neither answers "what is the language called
  `typescript`", which is the question a block asks.
- `internal/ast/query_loader.go` — the `EmbeddedBlock` type, the `embedded:` field,
  `validEmbeddedBlocks`, `hasLangConfig` extended, and `projectTsLangCache` added
  to `invalidateDerivedQueryCaches`.
- `internal/ast/queries/{vue,svelte,html}.yaml` — the `embedded:` blocks, and the
  comments above `element_text` rewritten: they used to explain what was missing.
- `internal/ast/shard_cache.go` — `shardCacheVersion` 3 → 4, with its reason line.
- `docs/specs/embedded_language_parsing.md` — status, the design as built, the six
  decisions with the chosen answer.
- `docs/specs/ast_module.md` — the HTML/Svelte/Vue rows of the language table, the
  `embedded` key in the YAML schema section and its row in the field table.
- `docs/tasks/add-vue-treesitter-grammar.md` — the debt checkbox.

### The parts that were not in the design

**`embedDepth`.** The config is declarative and nothing stops a `default: vue`
inside `vue.yaml`, which would recurse until the stack ends.
`maxEmbedDepth = 1` — one level is all any real component needs, since TypeScript
declares no embedded blocks. `TestEmbeddedParseDoesNotRecurseIntoItself` builds
exactly that pathological config and asserts the parse completes.

The offset applied in one place. The design stated "lineOffset is added to Line/EndLine of each entity produced," which reads like a change at each of the ten sites that compute a line from a node position. `shiftParsedLines` runs once after every pass has finished, over `Entities`, `CallSites`, and `References`. One implementation, one unit test, and no site that can be forgotten. It also clears `ParsedFile.mergeIdx` because identity includes the line and every position that index held is stale after a shift.

**Translation:**

**`IndexSource` forced off for the inner parse.** The outer parse already holds the
whole file's source; a second copy of the block's text would be stored nowhere.

**The block body is a slice, not a copy.** `src[textNode.StartByte():textNode.EndByte()]`
rather than `[]byte(textNode.Utf8Text(src))`, which the design sketched.

### The tree, which had to be dumped before writing any of this

All three grammars turned out to be structurally identical for this purpose:

```
script_element
  start_tag
    tag_name              "script"
    attribute
      attribute_name      "setup"
    attribute
      attribute_name      "lang"
      quoted_attribute_value
        attribute_value   "ts"
  raw_text                "\nimport { ref } from 'vue'\n…"
  end_tag
```

Two things came out of the dump that are not in any grammar.json:

1. **`raw_text` starts on the same row as the start tag**, because it begins
   immediately after the `>`. So the offset is `textNode.StartPosition().Row`
   exactly — the block's own line 1 *is* that row — and not the row after it.
2. **`attribute_value` sits under `quoted_attribute_value` for `lang="ts"` but
   directly under `attribute` for `lang=ts`.** Pairing each `attribute_value` with
   the `attribute_name` that most recently preceded it in document order handles
   both without caring about the wrapper.

## Testing

Written before the implementation and confirmed failing (build failure on
`shiftParsedLines`, `mergeParsedInto` and `qf.Embedded`), in this order:

1. `go test -tags fts5 ./internal/ast/` — **ok**, 35 new tests, no skips.
2. `internal/hub`, `internal/daemon`, `cmd/graphit/commands` — **ok**.
3. `go build -tags fts5 ./...` and `go vet -tags fts5 ./...` — clean.
4. `TestEveryShippedQueryPatternCompiles`, `TestEveryNativeGrammarHasQueries` —
   **PASS**.
5. End-to-end: binary built, the three changed YAMLs copied into
   `~/.graphit/runtime/dev/ast/queries/`, `graphit init` + `ast index --reindex` on
   a scratch project with `.vue`, `.svelte`, `.html` and `.js`, then the acceptance
   Cypher. A passing test proves neither extension registration nor cache
   behaviour; only the live graph does.

`-tags fts5` is not optional: without it ~15 search tests fail with
`no such module: fts5`, which is the missing tag and not a regression.

## System Knowledge

- **`TestHTMLDetailIsExtracted` passing with the embedding on is itself the
  evidence HTML was safe to enable.** That test stages only `html.yaml`, so its
  inner javascript/css configs resolve from the runtime install, and its
  assertions on `Text` nodes and attributes hold unchanged while the sub-parse
  runs. The scope made "no existing html assertion changes" the condition; it was
  met without editing a line of that test.
- **The `File`-level containment falls out of the design rather than being coded.**
  The inner parse builds its own `contextResolver` over the block's text only, so a
  top-level declaration in a `<script setup>` resolves an empty context and lands
  under the `File`. No special case was needed to get the answer the spec
  recommended.
- **The runtime YAML copy is a real step, not a formality.** `bundle_ast` in the
  Makefile is what puts `internal/ast/queries/*.yaml` where `initTsExtMap` reads
  them (`~/.graphit/runtime/<version>/ast/queries/`); a plain
  `go build ./cmd/graphit` skips it entirely and the new binary reads the **old**
  YAML. The symptom is the index reporting the wrong language, or nothing, with
  the file and the tests both correct.
