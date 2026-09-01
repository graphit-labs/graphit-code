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
Given an App.vue with a <script setup> containing `import { ref } from 'vue'` on line 8
When the file is indexed
Then an IMPORTS edge exists from the File App.vue to the Module "vue"
  And the statement's Import entity is on line 8, not on line 1
  And a function declared in the <script> is a Function entity with the correct absolute line
  And the Element "script" still exists, with its attributes
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
Given a <style lang="scss"> in a .vue
When the file is indexed
Then the block is skipped without error
  And no CSS entity is created from it
  And the Element "style" and its attributes still exist
  And the rest of the file is indexed normally
```

`TestVueStyleWithUnknownLangIsSkippedWithoutError`. Confirmed in the live graph:
`Typed.vue` has `Element style` and `AttributeValue scss` at line 18 and nothing
else from that block, while its `<script lang="ts">` produced `Interface Props`
and `Function useCounter`.

### UC-03 — The `lang` attribute selects the grammar

```gherkin
Given a <script setup lang="ts"> declaring `interface Props`
When the file is indexed
Then an Interface entity "Props" exists at the correct absolute line

Given a <script setup> with no lang attribute
When the file is indexed
Then the body is parsed as javascript

Given a <style> with no lang attribute
When the file is indexed
Then the body is parsed as css
```

`TestVueScriptLangAttributeSelectsTypeScript`, `TestVueStyleBodyIsParsedAsCSS`.
`interface` is TypeScript-only, so seeing it is proof that the ts grammar ran and
not the js one.

### UC-04 — The line offset is absolute, and tested where it can fail

```gherkin
Given an embedded block that does NOT start on line 1 of the file
When the file is indexed
Then every line reported by the inner parse is the file's absolute line
  And this holds for entities, CallSites and References
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
Given a Card.svelte with a <script> containing `import { onMount } from 'svelte'`
When the file is indexed
Then an IMPORTS edge exists to the Module "svelte"
  And `export let title` marks the entity as exported by the javascript rule
  And the <style> produces CSS entities
```

`TestSvelteScriptBodyContributesImportsAtAbsoluteLines`.

### UC-06 — HTML inline, and only when it is really code

```gherkin
Given an index.html with a <script type="module"> containing an import
When the file is indexed
Then an IMPORTS edge exists from the File index.html

Given a <script type="application/json"> in the same file
When the file is indexed
Then the body is NOT parsed as javascript
```

`TestHTMLInlineScriptAndStyleAreParsed`. The discriminator for HTML is `type`,
not `lang` — HTML has no `lang` on a script — and only the values that *are*
JavaScript are mapped, so a JSON payload, an import map and a
`type="text/x-template"` are skipped rather than parsed as code. This is the
`languages` mechanism doing exactly what it was designed for.

### UC-07 — Mapping a new language is just YAML

```gherkin
Given the script block of html.yaml, which maps only values that are JavaScript
When I add `application/json: json` to `languages:`
Then the body of <script type="application/json"> produces Pair and Value entities
  And they are at the file's absolute line
  And the element's markup stays intact
  And not one line of Go was changed
```

`TestMappingANewInnerLanguageIsYAMLOnly`. It exists because extensibility is a
promise that only holds if it is pinned by a test: `languages:` is an **allowlist**,
so adding and removing a mapping are the two operations a user is going to
want to perform, and both are one line of YAML.

The only condition is that the inner language exists — that it has a query file, which is where
the extension registration comes from. Every language we ship is eligible.
`TestEveryShippedEmbeddedBlockNamesRealGrammarNodes` fails if a `languages:`
points at a language that does not exist.

See "Extensibilidade: o que é YAML e o que é código" in the spec for the complete
boundary, including the two things that are **not** configurable
(`attribute_name`/`attribute_value` and `maxEmbedDepth`) and why.

### UC-08 — A dynamic selector, and only ONE mechanism

```gherkin
Given a project xml.yaml declaring
  pattern: '(element (STag (Name) @_tag) (content (CharData) @body) (#eq? @_tag "execute"))'
When a .xml with <note> and <execute> is indexed
Then only the body of <execute> goes to the inner grammar
  And the entity is at the absolute line
  And the XML markup stays intact

Given `#match? @_tag "^sql"` instead of `#eq?`
Then <sqlSetup> matches and <other> does not — several names without enumerating any

Given a <script setup lang="ts"> and a <style lang="scss"> in the same .vue
When the file is indexed
Then the script-specific block wins and TypeScript runs
  And the style-specific block claims the body and maps nothing
  And the generic block does NOT parse the scss as CSS
```

`TestEmbeddedPatternSelectsElementByName`, `TestEmbeddedPatternSelectsByRegex`,
`TestEmbeddedPatternReadsLanguageFromACapture`, `TestEmbeddedPatternKeepsAbsoluteLines`,
`TestFirstMatchingBlockClaimsTheBodyAndTheRestSkipIt`,
`TestGenericBlockFiresWhenTheAttributeIsAbsent`.

The selector was a node KIND, and that does not express the problem: in tree-sitter-xml
`<execute>` is an `element` like any other. Enumerating names would answer one
example and nothing beyond it. The engine already has a dynamic node selection language,
compiled and tested — the tree-sitter pattern — and now it is the **only** one:
`node`, `text` and `lang_attribute` were removed, along with the walk by kind,
`embeddedTextNode`, `embeddedAttrValue` and the constants
`attribute_name`/`attribute_value`.

The ORDER became part of the design: the first block whose pattern matches a body
**claims** it, and the claim happens at the match, before the language resolves.
That is what lets an optional attribute fit into two patterns with no special
case, and what stops `lang="scss"` from falling into the generic block's `default: css`.

Config consequence: an **explicit** `languages: {}` became a declaration with
meaning — "match, claim, map nothing" — distinct from an absent `languages`.
YAML separates the two and so does the validator.

### UC-09 — An ANTLR language can be the inner language

```gherkin
Given a project xml.yaml with `default: plsql` on an <execute> block
When a .xml with CREATE TABLE and CREATE PROCEDURE inside <execute> is indexed
Then Table, Column and Procedure exist at the ABSOLUTE line
  And the SELECTS edge exists from the procedure to the table it reads
  And both come out of the .xml file
  And the XML markup stays intact
```

`TestEmbeddedBlockCanBeParsedByTheANTLRBackend`,
`TestEmbeddedANTLRBlockProducesDMLEdges`, `TestEmbeddedLangResolvesAcrossBothBackends`.

Verified in the live graph: `Table pedido@5`, `Column id@6`, `Column status@7`,
`Procedure p_lista@11` and `p_lista -[:SELECTS]-> xpto @13`, all with
`source_file = src/changelog.xml`.

It came cheap because `AntlrParser.parseWithConfig` already received `src []byte` and
`shiftParsedLines`/`mergeParsedInto` were already backend-agnostic — the first cut's
decision to apply the offset in a single place paid off here.

The warning "an ANTLR language cannot be the inner one" was removed: it existed only while
the limitation existed. A language that exists in no backend is still skipped in
silence.

### UC-10 — `sql.yaml` extracted almost nothing, and the fallback hid it

```gherkin
Given `create table cliente (id integer, nome varchar(60));`
When the file is parsed BY TREE-SITTER, isolated from the ANTLR fallback
Then Table "cliente" exists and the Columns "id" and "nome" contained by it

Given `select nome from cliente join pedido on ...`
Then there are REFERENCES to "cliente" and to "pedido"
```

`sql_treesitter_test.go`. Only the `tables` pattern was broken — it required a
`name:` field on `create_table`, which assigns a field to no child. It compiled and matched
zero. `create_view`/`create_function` always worked, because their `name:` is
on the `object_reference`, which does have that field.

**Every test in there calls `parseSource` directly.** A test over `parseFixture` would pass
with the defect in place, because `CompositeParser` falls back to ANTLR on seeing zero — that is
exactly how it survived.

While in there, `sql.yaml` gained `columns` and the tables read by a `SELECT`, without
which `select * from xpto` in an embedded block would contribute nothing.

**Decided: tree-sitter is the DEFAULT parser for `.sql`**, and a dialect project
opts in through `ast.grammar` (`{"ast": {"grammar": ".sql=antlr-plsql"}}`), which is persistent
config and respected by the daemon, not just the `--grammar` flag.

That changed the scope of the fix: if tree-sitter parses most `.sql` files, it
has to cover what a `.sql` has. So besides `create_table` came `columns`,
`create_index` and the complete DML — `SELECTS`, `INSERTS`, `UPDATES`, `DELETES`,
`ALTERS` — with the SAME edge types as the ANTLR dialects, so that "who reads this
table" is a single question. Measured: ANSI DDL from 1 to 4 (parity with ANTLR),
PL/SQL from 1 to 3 against antlr-plsql's 4, missing only the `Procedure` — which is
exactly the opt-in case.

Two traps, both caught by tests and neither visible in production:

- **One `data_key` per edge type.** `buildRelationTypeMap` maps
  `data_key → relation_type`, so queries with the same `data_key` and different types
  collide and the last one wins, silently. With all of them under `dml`, every SELECT/INSERT/
  UPDATE became `ALTERS`.
- **The DELETE's `from` does not have the shape of the SELECT's `from`.** In both it is a sibling of the
  statement, but SELECT wraps the table in a `relation` and DELETE does not.

**Resolved — see UC-13.** This was a limitation for two days: the DML edge only
reached the graph when the statement had an entity around it. In a schema `.sql`, and
in the SQL of an embedded block, that was most of the DML.

### UC-11 — An escaped body is normalized before the sub-parse

```gherkin
Given a block whose body arrives escaped by the host — `qt &gt; 0`
When the block declares `normalize:` pointing at a `text_normalizers` of the language
Then the body reaches the sub-parse with the text the entities represent
  And the COUNT of line breaks does not change
  And a pair whose replacement contains a line break is dropped at load
  And `&#10;` / `&#xA;` are left as they are
```

`TestTextNormalizerPreservesNewlineCount`, `TestTextNormalizerKeepsLinesAligned`,
`TestEmbeddedXMLBlockDecodesAndParsesTheWholeBody`,
`TestTextNormalizerRejectsAReplacementThatAddsALine`,
`TestEmbeddedNormalizeIsOptInAndMustExist`,
`TestBlockNamingANormalizerInAnotherLanguageIsDropped`.

Why it became necessary: a block embedded in an XML **is not plain text**. `<` and `&`
are markup, so `WHERE qt > 0` reaches the file as `qt &gt; 0` and the host
grammar breaks the content into `CharData` / `EntityRef` / `CharData`. Capturing
`(CharData)` takes only the FIRST piece — statement truncated at the first comparison
operator, silently. The block captures the whole `content`, and the normalizer is what
gives back parseable text.

**The engine knows no escape scheme at all.** There is no entity table in Go.
How a language escapes its text is a fact about it, so it lives in the YAML — the same principle
as `context_types`. Declared by the language, chosen by the block, because they are two
different facts: the scheme belongs to the language, but NEEDING it belongs to the position — the
content of an XML element is escaped, the `raw_text` of an HTML `<script>` is not.

And the declaration goes where the need is: the **shipped** grammar only declares a
normalizer if the whole language needs it. A case specific to a project goes in that
project's override, which the `projeto > usuário > runtime` chain already supports.

### UC-12 — An embedded block pattern needs anchors, and the measurement is brutal

```gherkin
Given a large, deep XML document
When an embedded block matches a PAIR of children with no anchor
Then the cost is quadratic per `content` and repeats on every ancestor
```

Measured on an 888 KB XML (~500k lines):

| pattern | result |
|---|---|
| pair with no anchor, with an outer wrapper | **aborted at 60s**, 2 matches |
| anchor only between the two elements | 44 matches in **13.5s** |
| fully anchored | 44 matches in **327ms** |

With no anchor the `(element …)` matches EVERY element in the document and, for each one, looks for the
pair among the children of the `content` — a quadratic search that repeats on every ancestor of
a deep tree. The `.` anchors make the match positional.

Found the bad way: a test spent 10 minutes stuck in
`ts_query_cursor_next_match` and only the stack said where. **A slow pattern does not warn** —
the index simply never finishes. It is the same failure mode as a pattern that does not match, with
the difference that this one consumes the machine.

In a grammar where whitespace is a NAMED node — `CharData` in XML — it counts
towards adjacency and has to be written between the anchors.

### UC-13 — A reference with no enclosing entity belongs to the FILE

```gherkin
Given a statement at the top of a script, with no procedure around it
When the file is indexed
Then the DML edge exists with the File as its source

Given the same statement inside a named unit
Then the source is still the unit, and not the file
```

`TestReferenceWithNoEnclosingEntityIsSourcedAtTheFile`,
`TestFileSourcedDMLEdgeReachesTheGraph`, `TestEntitySourcedEdgeIsUnchanged`.

`ConvertToCache` left `SourceUID` empty when there was no `SourceName`, and the
writer discarded it — the same pattern as the Import entity, built and thrown away
with a `continue`. The file as the source is the shape `IMPORTS` already uses
(`File -[:IMPORTS]-> Module`): "what touches this table" is a question about the file
when there is nothing smaller that can be named.

**Correction of an estimate of mine.** I said it would require four points in the writer
plus the `File → Table` pair in the relationship tables group. Wrong about the schema:
`ladybug.go` **already emitted** `FROM File TO <alvo>` unconditionally. The schema always
supported it; what was missing was filling it in. It was three small changes — `ConvertToCache`,
`refSourceLabel` and a COPY step in `json_rebuild` mirroring what the DDL already had.

And `File` does **not** go into `dmlSourceLabels`: since the DDL already declares the pair, announcing it
there emits it twice and LadybugDB refuses the group with `Found duplicate FROM-TO
File-Table pairs`. Caught by the live graph, not by a unit test.

### UC-14 — Latent defect: `Table Function does not exist`

```gherkin
Given a file whose calls have no entity around them
When the graph is rebuilt
Then the Function table exists, and the rebuild does not abort
```

`stubFunctionJSON` creates a `Function` node for every unresolved call target, but
the table was only created when some call had a CALLER. A call at the top of a
script has no entity around it, so `callerSet` stayed empty, the stub rows
were emitted against a nonexistent table and the **whole rebuild aborted**.

Predating this work and valid for any language with top-level calls; the loose SQL
of an embedded block only made it reachable. It only showed up in the live graph — the
message goes to a logger that is a NOP when nobody passes one, so the index said
only "1 COPY operation(s) failed".

The missing symmetry was closed right after — see UC-15.

### UC-15 — A call with no enclosing entity is made by the FILE

```gherkin
Given a call at the top of a script, with no entity around it
When the file is indexed
Then the CALLS edge exists with the File as the caller

Given the same call inside a function or procedure
Then the caller is still it, and not the file

Given any caller
Then the `X → File` pair is NOT declared: a file is never a call target
```

`TestCallWithNoEnclosingEntityIsCalledByTheFile`, `TestFileCalledEdgeReachesTheGraph`,
`TestFileIsNeverACallTarget`.

Symmetric to UC-13, and valid for any language with top-level calls — not just for the
embedded SQL that exposed the case. Verified in the live graph, in a single index:

```
File app.js       -[:CALLS]-> init             (init() de topo em JavaScript)
Function boot     -[:CALLS]-> init             (contida, inalterada)
File carga.sql    -[:CALLS]-> p_carga_diaria   (bloco anônimo PL/SQL)
Procedure p_principal -[:CALLS]-> p_log        (contida, inalterada)
```

**Three gates, and each one hid the next.** Unlike DML, the `CALLS` DDL
has no fixed File step: it derives the pairs from `CallerLabels`. So it took

1. `cache_convert` passing the file as the caller,
2. `File` entering the `validTypes` allowlist of `rebuild_index`, which decides what becomes
   a `callerLabel`, and
3. the writer's loop no longer requiring `labelSet[cl]` — `File` is not in the `labelSet` on
   purpose, so that the node table is not emitted twice.

The third only showed up in the live graph: **the unit test called `callEdgeJSON` directly and
skipped the gate**, so it passed while the real path discarded everything. The gate became
`canWriteCallerLabel`, a function, precisely so it could be tested.

### UC-16 — A broken `embedded` config is rejected, not carried

```gherkin
Given an embedded block with no `node`, or no `text`
When the query file is loaded
Then the block is dropped with a WARN

Given an embedded block with no `default` and no `languages`
When the query file is loaded
Then the block is dropped — no language could resolve

Given a `languages` with no `lang_attribute`
When the query file is loaded
Then the map is dropped with a WARN, and the block survives through the `default`

Given a `node` the grammar does not have
When a file of that language is parsed
Then there is a WARN, once per language, not one per file
```

`TestEmbeddedConfigDropsMalformedEntries`,
`TestEmbeddedConfigDropsUnreachableLanguagesMap`,
`TestEmbeddedConfigLowercasesLanguageKeys`,
`TestEveryShippedEmbeddedBlockNamesRealGrammarNodes`.

### UC-17 — The element's Text node survives

```gherkin
Given a <script>const a = 1;</script> — a single-statement body
When the file is indexed
Then the Text node with "const a = 1;" still exists
  And the Variable "a" now exists too, at the absolute line
```

`TestSingleStatementScriptKeepsItsTextNodeAndGainsStructure`. The `element_text`
queries stay in all three grammars on purpose: this is addition, not replacement.

## Implementation Details

### Files created

- `internal/ast/treesitter_embedded.go` — the whole mechanism: the tree walk that
  finds blocks, the attribute read, the sub-parse, `shiftParsedLines` and
  `mergeParsedInto`.
- `internal/ast/embedded_parse_test.go`, `embedded_selector_test.go`, `embedded_antlr_test.go` and `sql_treesitter_test.go` — 35 tests.
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

**The offset applied in one place.** The design said "lineOffset é somado a
Line/EndLine de toda entidade produzida", which reads like a change at each of the
ten sites that compute a line from a node position. `shiftParsedLines` runs once
after every pass has finished, over `Entities`, `CallSites` and `References`. One
implementation, one unit test, and no site that can be forgotten. It also clears
`ParsedFile.mergeIdx`, because identity includes the line and every position that
index held is stale after a shift.

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
