---
title: Add the Vue tree-sitter grammar and refresh the language list in docs and site
status: done
created: 2026-08-05
updated: 2026-08-05
tags: [ast, tree-sitter, grammar, vue, documentation]
---

# Add the Vue tree-sitter grammar and refresh the language list in docs and site

## Objective

Index Vue single-file components (`.vue`) in the AST graph, and bring the
published language list back in line with what the code actually ships.

The second half was not cosmetic. `docs/specs/ast_module.md` listed 42 languages
and `docs/site/index.html` advertised 23, while `internal/ast/queries/` held 44
query files: **CSS and Svelte had been added and never listed anywhere.** Adding
Vue on top of a list that was already two languages behind would have shipped a
third wrong number, so all three were reconciled against the code.

## Implementation Details

### Grammar choice

Two Vue grammars exist. `ikatyang/tree-sitter-vue` is the older, better-known
one (last pushed 2024-02). `tree-sitter-grammars/tree-sitter-vue` is actively
maintained (last pushed 2026-01) and its source layout is **identical to
tree-sitter-svelte's** — `parser.c`, `scanner.c`, `tag.h`, `tree_sitter/*.h`,
both descended from tree-sitter-html. The maintained one was taken, pinned at
commit `ce8011a414fdf8091f4e4071752efc376f4afb08` (grammar version 0.1.0,
tree-sitter ABI 15, MIT / Amaan Qureshi).

It ships `bindings/{c,python,rust}` but **no Go bindings**, so it could not be a
Go module dependency like hcl/lua/toml/xml/yaml/zig. It was vendored, which is
the route svelte, dart, kotlin and eleven others already take.

### Files created

- `internal/ast/treesitter/vue/` — `parser.c.inc`, `scanner.c.inc`, `tag.h`,
  `tree_sitter/{alloc,array,parser}.h`, `LICENSE`, and `binding.go`. The `.inc`
  suffix on the two C files is the repo's convention: it keeps them out of the Go
  build's own C compilation and they are `#include`d from `binding.go`'s cgo
  preamble instead. `.gitattributes` and `.astignore` already match
  `internal/ast/treesitter/**/*.c.inc` by wildcard, so neither needed an edit.
- `internal/ast/queries/vue.yaml` — 45 patterns. See Use Cases below.

### Files modified

- `internal/ast/treesitter_native.go` — `"vue": tsVue.Language` in
  `nativeGrammars`. No symbol collision with svelte/html despite the shared
  tree-sitter-html ancestry.
- `Makefile` — `vue` appended to `GRAMMAR_VENDORED`, which is what builds
  `tree-sitter-vue.so` and what `bundle_ast` copies into the launcher.

### The one thing Vue's grammar does that HTML's does not

`directive_attribute`. A directive has a long form and a shorthand, and the
grammar represents them differently:

```
v-bind:label="x"   directive_name . ":" . directive_value . "=" . quoted_attribute_value
:label="x"                         ":" . directive_value . "=" . quoted_attribute_value
```

In the shorthand there is **no `directive_name` node at all** — the sigil is an
anonymous token, and it is the only thing distinguishing `:label` (a prop) from
`@click` (an event) from `#footer` (a slot). Two consequences drove the query
design:

1. **Anonymous tokens must be matched literally**: `(directive_attribute "@" …)`.
   This works, and is how the three shorthand kinds are told apart.
2. **`:` is overloaded.** It is the shorthand bind sigil *and* the long form's
   argument separator, so an unanchored `(directive_attribute ":" (directive_value) @name)`
   matches `v-on:click` and reports an event handler as a prop. Verified against
   a real parse: it also caught `v-slot:footer`. The fix is the start anchor —
   `(directive_attribute . ":" . (directive_value) @name)` — which requires the
   sigil to be the first child, true only of the shorthand. The long forms are
   then recovered precisely with a predicate on `directive_name`
   (`(#eq? @_dir "v-on")`), which the engine honours; html.yaml, hcl.yaml,
   graphql.yaml and ruby.yaml already rely on `#eq?`.

Every pattern in the file was validated by parsing fixtures and dumping matches
before being written, not inferred from the grammar source.

## Use Cases

### UC-01: Index a Vue single-file component
- **Actor**: the indexer (`graphit ast index`, or the daemon's file watcher)
- **Preconditions**: `vue.yaml` is resolvable, so `.vue` is in the extension
  table; `tree-sitter-vue` resolves via `NativeLanguage("vue")`
- **Main Flow**:
  1. `rebuildExtTables` reads `extensions: [".vue"]` from `vue.yaml` and registers `.vue`
  2. `TreeSitterParser.Parse` resolves the grammar through `resolveTreeSitterLang("vue", "tree-sitter-vue")`
  3. The 45 patterns run; entities are merged via `AddOrMergeEntity`
  4. `ConvertToCache` writes nodes and CONTAINS/REFERENCES edges
- **Alternative Flows**:
  - A project-local `.graphit/ast/queries/vue.yaml` overrides the shipped file
  - `--grammar .vue=<other>` overrides the grammar for the extension
- **Error Scenarios**:
  - Query file absent → **no extension is registered and `.vue` files are not
    discovered at all**. Not "extracts little" — zero. Pinned by
    `TestEveryNativeGrammarHasQueries`.
  - A pattern that fails to compile is dropped with a WARN and nothing else.
    Pinned by `TestEveryShippedQueryPatternCompiles`.
- **Postconditions**: one `File` node plus its markup, directive and text entities
- **Affected Files**: `internal/ast/queries/vue.yaml`, `internal/ast/treesitter_native.go`, `internal/ast/treesitter/vue/binding.go`

### UC-02: Resolve which kind a shorthand directive is
- **Actor**: the query executor
- **Preconditions**: a `directive_attribute` node
- **Main Flow**:
  1. `. ":" .` → `Prop`; `. "@" .` → `EventHandler`; `. "#" .` → `Slot`
  2. Long forms go through `(#eq? @_dir "v-bind"|"v-on"|"v-slot")` and land on the same three labels
  3. Everything else keyed by `directive_name` becomes `Directive` (`v-if`, `v-model`, and bare `v-bind="obj"` / `v-on="handlers"`, which have no argument and are seen by no other pattern)
- **Alternative Flows**:
  - `:[expr]="v"` → `Prop` named by the `dynamic_directive_inner_value`
  - `v-if` / `v-else-if` / `v-show` values also become `Condition`, and `v-for` a `Loop`, matching the label svelte.yaml gives `{#if}`
- **Error Scenarios**:
  - A valueless shorthand directly followed by another shorthand
    (`#footer :class="cls"`) is **one** `directive_attribute` in this grammar —
    whitespace is extra, so the rule's `REPEAT1` swallows the next segment. The
    sibling anchors keep each sigil bound to its own argument, so both are still
    read correctly; only the start-anchored pattern misses the second one.
  - `.stop` / `.prevent` modifiers are parsed but deliberately not indexed
- **Postconditions**: the directive is contained by its `Element`
- **Affected Files**: `internal/ast/queries/vue.yaml`

### UC-03: Read what a template renders
- **Actor**: the query executor
- **Preconditions**: an `interpolation` node under `element` or `template_element`
- **Main Flow**: `{{ title }}` emits a `REFERENCES` edge to `title`
- **Error Scenarios**: a multi-line or over-long interpolation is dropped by `dataText`
- **Postconditions**: the reference is attributed to the element that renders it
- **Affected Files**: `internal/ast/queries/vue.yaml`

### UC-04 [KNOWN LIMITATION]: `<script>` and `<style>` bodies are not parsed
- **Actor**: the query executor
- **Main Flow**: tree-sitter-vue hands each body over as a single `raw_text`, recorded as the element's `Text`
- **Postconditions**: a single-statement body becomes a `Text` node; a real multi-statement
  one becomes no node at all, because `dataText` rejects an interior newline (the 256-char
  cap almost never decides it). The body text remains findable through the file's indexed
  source — what is lost is structure, not searchability. Measured, see the spec.
- **Consequence**: **a `.vue` file declares no imports** — `import { ref } from 'vue'` lives inside that `raw_text`. `exports.strategy` is `none` for the same reason: `defineProps` / `defineExpose` are not visible either.
- **Affected Files**: `internal/ast/queries/vue.yaml`

## Test Cases & Acceptance Criteria

### Feature: Vue SFC extraction
Ref: UC-01, UC-02, UC-03, UC-04 — `TestVueIsExtracted` in `internal/ast/css_test.go`

#### Scenario: A component's markup, props and events are indexed
```gherkin
Given a TodoList.vue with <div class="list" @click="focus"> containing <TodoItem :label="item.text" @remove="onRemove(item.id)" />
When the file is parsed by the composite parser
Then an Element "div" is contained by Element "template"
  And an Element "TodoItem" is contained by Element "div"
  And a Prop "label" is contained by Element "TodoItem"
  And a Value "item.text" is contained by Prop "label"
  And an EventHandler "click" is contained by Element "div"
  And an EventHandler "remove" is contained by Element "TodoItem"
```

#### Scenario: A long-form directive is not mistaken for a prop
```gherkin
Given a component with v-bind:done="item.done", v-on:edit="onEdit" and v-slot:footer
When the file is parsed
Then a Prop "done" exists
  And an EventHandler "edit" exists
  And a Slot "footer" exists
  But no Prop named "edit" exists
  And no Prop named "footer" exists
```

#### Scenario: Template logic is indexed against the element it governs
```gherkin
Given a <TodoItem v-for="item in items" v-if="visible" /> and an <input v-model="draft">
When the file is parsed
Then a Condition "visible" is contained by Element "TodoItem"
  And a Loop "item in items" is contained by Element "TodoItem"
  And a Directive "v-model" is contained by Element "input"
  And a Value "draft" is contained by Directive "v-model"
```

#### Scenario: An interpolation is a reference, with its padding removed
```gherkin
Given a template containing "{{ title }}" and a <script setup> declaring title
When the file is parsed
Then a REFERENCES edge targets "title"
  And the target is not " title "
```

#### Scenario: Embedded bodies are recorded as element text
```gherkin
Given a <script setup> body of `const title = "Pedidos";` and a <style scoped> body of `.list{color:red}`
When the file is parsed
Then a Text `const title = "Pedidos";` is contained by Element "script"
  And a Text ".list{color:red}" is contained by Element "style"
  And an Attribute "setup" is contained by Element "script"
```

### Feature: Grammar registration guard rails
Ref: UC-01

#### Scenario: A registered grammar always has a query file
```gherkin
Given tree-sitter-vue is registered in nativeGrammars
When TestEveryNativeGrammarHasQueries runs
Then vue.yaml is found, matched on either its language or its grammar field
```

#### Scenario: Every shipped pattern compiles
```gherkin
Given the 45 patterns in vue.yaml
When TestEveryShippedQueryPatternCompiles runs
Then all 617 shipped patterns across all grammars compile
```

#### Scenario: Containment is declared
```gherkin
Given vue.yaml declares context_types for element/template_element/script_element/style_element
  And context_name_paths of start_tag/tag_name for each
When TestEveryNonCallableContainerIsDeclaredAsAContext runs
Then vue:start_tag and vue:self_closing_tag are on record in nonCallableExemptions with a reason
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/treesitter/vue/parser.c.inc` | Created | Vendored generated parser |
| `internal/ast/treesitter/vue/scanner.c.inc` | Created | Vendored external scanner |
| `internal/ast/treesitter/vue/tag.h` | Created | Scanner dependency |
| `internal/ast/treesitter/vue/tree_sitter/{alloc,array,parser}.h` | Created | Headers the vendored C needs |
| `internal/ast/treesitter/vue/LICENSE` | Created | MIT, kept beside the code |
| `internal/ast/treesitter/vue/binding.go` | Created | cgo preamble exposing `tree_sitter_vue()` |
| `internal/ast/queries/vue.yaml` | Created | 45 extraction patterns and language config |
| `internal/ast/treesitter_native.go` | Modified | Register `"vue"` in `nativeGrammars` |
| `Makefile` | Modified | `vue` in `GRAMMAR_VENDORED` so the `.so` builds and ships |
| `internal/ast/css_test.go` | Modified | `TestVueIsExtracted` |
| `internal/ast/containment_coverage_test.go` | Modified | `vue:start_tag` / `vue:self_closing_tag` exemptions |
| `docs/specs/ast_module.md` | Modified | Rows 43–45 (CSS, Svelte, Vue); counts 42→45, 37→40 |
| `docs/guides/user_manual.md` | Modified | Counts 42→45, 37→40 |
| `docs/guides/troubleshooting.md` | Modified | Supported-language list: added CSS, Svelte, Vue |
| `docs/site/index.html` | Modified | "23 Supported Languages" → 45, full tag list |
| `README.md` | Modified | 42 → 45 default grammars |
| `THIRD_PARTY_LICENSES.md` | Modified | tree-sitter-vue MIT attribution |

## Trade-offs & Decisions

- **`tree-sitter-grammars/tree-sitter-vue` over `ikatyang/tree-sitter-vue`.**
  Maintained, ABI 15, and structurally identical to the svelte grammar already
  vendored here, so it needed no new build machinery.
- **Vendored rather than added as a Go module.** Not a choice — upstream ships no
  Go bindings.
- **Start anchor over a `#not-eq?` chain for the shorthand sigils.** The anchor
  states the actual grammatical fact (the sigil is the first child only in the
  shorthand) instead of enumerating directive names to exclude, which would rot
  as Vue adds directives.
- **Long forms recorded as their precise kind AND as a `Directive`.** Slight
  redundancy accepted: it is the only way `v-bind="obj"` (no argument) stays
  visible, and the two nodes state different facts — the directive as written,
  and its argument.
- **`Condition` and `Loop` reuse svelte.yaml's labels** rather than inventing
  Vue-specific ones, so "what gates this markup" is one query across both.
- **`parent_capture` on the `Condition` / `Loop` patterns.** It names the element the
  expression governs. This one is load-bearing and was proven so: without it the
  ancestor walk attributed `v-if` on a self-closing `<TodoItem />` to the enclosing
  `div`, because `self_closing_tag` is not a context type and the `element` wrapping
  it has no `start_tag` to be named from.

   > **FIXED on 2026-08-05.** This item originally included **interpolation** and
   > claimed that `parent_capture` was what enabled trimming. Wrong on both counts: the
   > interpolation containment already came from `context_types` (measured: `SourceName` is `div`
   > with and without it), and trimming is now unconditional in the adapter. Worse, it opted the target into
   > `dataText`, which strips quote pairs and made `{{ "foo" }}` become a reference to the
   > identifier `foo` — a false dependency. Removed from both interpolation
   > queries; see
   > [trim-captured-names-in-treesitter-adapter.md](trim-captured-names-in-treesitter-adapter.md).
- **Directive modifiers (`.stop`, `.prevent`) not indexed.** They are parsed and
  reachable, but a node per modifier is noise against its value.

## Technical Debt

- [x] **svelte.yaml has the untrimmed-name defect this task found in Vue.** Resolved
  in [trim-captured-names-in-treesitter-adapter.md](trim-captured-names-in-treesitter-adapter.md),
  and it turned out to be a class rather than a Svelte quirk: `html.yaml` (8 queries
  on `attribute_value`) and `css.yaml` (`var(...)`, `url(...)`) had it too. Fixed on
  in the engine: `strings.TrimSpace` on every captured name in the tree-sitter adapter,
  which is a no-op on the ~140 patterns capturing an identifier node. The 212
  ANTLR-path queries were never affected — that adapter already normalises
  unconditionally. The captured text was `"title "`, not `" title "`: the grammar eats
  the left space and keeps the right one. A second change to svelte.yaml went in with
  it and was reverted the same day, along with this task's own interpolation patterns —
see the Correction section of that log.
- [x] **`<script>` / `<style>` bodies are one opaque `raw_text`** (UC-04), so a
  `.vue` file contributed no IMPORTS edge and no export flags. Fixing it meant
  re-parsing the body with the `javascript` / `typescript` / `css` grammar — a composite
  parse this engine did not do for any language: `CompositeParser`
  dispatches between tree-sitter and ANTLR **by file extension**, and
  `parseWithConfig` read the file from disk on its first line and derived the
  context resolver, the docstring matchers and every entity position from that one
  buffer.

   **Resolved on 2026-08-05** — the design was followed through: `parseSource(src, lineOffset,
   embedDepth)` extracted from `parseWithConfig`, blocks declared in the language's YAML `embedded:` section,
   merge shifting lines by `shiftParsedLines`, `shardCacheVersion` 3 → 4.
   Applies to Vue, Svelte **and HTML** — HTML was included because enabling it required no existing assertion
   changes. `lang="scss"` and similar are silently skipped. Log:
   [embedded-language-parsing.md](embedded-language-parsing.md); spec with the six decisions
   recorded in [embedded_language_parsing.md](../specs/embedded_language_parsing.md).

- [x] **A valueless shorthand adjacent to another shorthand merges into one
  `directive_attribute`** (UC-02). Handled by the anchors, so there is no defect on our
  side. **Decision on 2026-08-05: do not open an upstream issue** — recorded here and
  nothing goes to `docs/upstream/`. If it is ever reported, the minimal repro is
  `<Panel #footer :class="cls" />`, which produces a single `directive_attribute` with
  `"#"`, `directive_value(footer)`, `":"`, `directive_value(class)`: whitespace is extra
  in the grammar, so the rule's `REPEAT1` swallows the following segment.

## System Knowledge

- **A grammar in `nativeGrammars` with no query file is inert, silently.** The
  extension table is built from the `extensions:` field of the *query files*, so a
  grammar without one registers no extension and its files are never discovered.
  This is why CSS and Svelte were invisible in every indexed project until their
  query files were written, and why the first verification step for a new grammar
  is an actual index, not a unit test.
- **The shipped query YAMLs are not `go:embed`ed in `internal/ast`.** `bundle_ast`
  in the Makefile copies `internal/ast/queries/*.yaml` into
  `cmd/launcher/runtime/ast/queries/`, the launcher embeds *that*, and it is
  extracted to `~/.graphit/runtime/<version>/ast/queries/`. So a locally built
  binary reads the **previously extracted** files: adding `vue.yaml` to the source
  tree is not enough to see it work, and the first end-to-end index of this task
  reported `tree-sitter:markdown — 1 file(s)` with the `.vue` file silently
  skipped until the file was placed in the runtime directory.
- **`dataText` only runs on a capture when the query declares `value_capture` or
  `parent_capture`** (`isData` in `treesitter_adapter.go`). It was true at the time
  that a pattern with neither kept the raw source text, whitespace included; the
  adapter now trims every captured name unconditionally, so only `dataText`'s *other*
  behaviours — stripping matched quote pairs, rejecting newlines and over-long values —
  remain gated on `isData`. Which is the part that matters when choosing whether to
  declare a capture: on a reference target, quote-stripping is wrong.
- **The engine honours `#eq?` predicates**, because `QueryCursor.Matches` is
  called with the source bytes, which is what lets go-tree-sitter filter text
  predicates.
- **`go test ./internal/ast/` without `-tags fts5` fails ~15 search tests** with
  `no such module: fts5`. `BUILD_TAGS := fts5` in the Makefile; those failures are
  a missing tag, not a regression.
- **Anonymous tokens are matchable in patterns** (`"@"`, `":"`, `"#"`), and
  sibling anchors (`.`) work between an anonymous and a named node. A start anchor
  before an anonymous node does *not* behave as "must be first child" on its own —
  it only worked here combined with a following anchored sibling.

## Progress Log

### 2026-08-05
- Searched memory and the wiki first; found the CSS/Svelte precedent, which named
  the exact trap (grammar registered, no query file → extension never registered).
- Compared the two Vue grammars, picked the maintained one, vendored it at a
  pinned commit, confirmed it compiles under cgo with no symbol collision.
- Dumped real parse trees before writing any pattern; discovered the anonymous
  sigil, the `:` overload, and the merged-node quirk, and validated all 45
  patterns against fixtures.
- `TestVueIsExtracted` written and passing; guard rails pass (617 patterns
  compile, 42 grammars checked for containment); full `internal/ast` suite green
  with `-tags fts5`; `make grammars-treesitter` builds 40/40 including vue (48K).
- End-to-end: built the binary, indexed a scratch project, confirmed
  `tree-sitter:vue — 1 file(s)` and queried the resulting Element / Prop /
  EventHandler / Slot / Condition / Loop nodes and the REFERENCES edge.
- Reconciled the language list in the spec, user manual, troubleshooting guide,
  README and site — which required adding the long-missing CSS and Svelte rows,
  not just Vue.
