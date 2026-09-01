---
title: Captured name is now trimmed in the tree-sitter adapter, and the Svelte binding gains parent_capture
status: done
created: 2026-08-05
updated: 2026-08-05
tags: [ast, treesitter, queries, references, svelte, html]
---

# Captured name is now trimmed in the tree-sitter adapter, and the Svelte binding gains parent_capture

## Objective

Settle the debt recorded in [add-vue-treesitter-grammar.md](add-vue-treesitter-grammar.md):
`svelte.yaml` produced a `REFERENCES` edge with the source's spacing, so a
binding written `{ title }` — syntax just as valid as `{title}` — pointed at a
name no declaration has, reporting nothing.

The investigation showed it was not a Svelte defect. It is a class, and Svelte was only
the visible case because the Vue grammar forced the subject: `{{ title }}` cannot
be written without spaces.

## Implementation Details

### What the survey showed, and how it changed the decision

The debt recommended two routes: (A) fix only `svelte.yaml`, and (B) evaluate
normalizing in the engine, with the caveat that (B) had a large and unknown blast
radius — "nobody knows today what it is".

The survey was done with the project's own loader (`parseQueryFile`), not by
text matching, listing every `type: relation` query that declares neither
`value_capture` nor `parent_capture`. There are **361**. Grouped by the type of node they
capture:

| no. | captured node |
|---|---|
| 212 | ANTLR path (`//callStatement/literal` etc.) |
| 66 | `identifier` |
| 17 | `type_identifier` |
| 10 | `field_identifier` |
| 9 | `property_identifier` |
| 9 | `name` |
| 8 | `attribute_value` |
| ... | `simple_identifier`, `word`, `sym_lit`, `constant`, ... |
| 1 | `svelte_raw_text` |

That inverts the reading of the risk. The 212 ANTLR paths **are already normalized** — the
ANTLR adapter passes everything through `dataText` unconditionally
(`antlr_adapter.go`, `dataText(unquoteIdentifier(target.FullText()))`). And the ~140
remaining ones capture **identifiers**, where surrounding whitespace is impossible: trimming
is a no-op. The set actually affected is small and nameable:

- `svelte.yaml` — `svelte_raw_text` in the binding (the debt's case)
- `html.yaml` — `attribute_value` in the 8 queries for `id` / `class` / `href` / `src` /
  `action` / `name` / `for` / `role`
- `css.yaml` — `plain_value` in `var(...)` and `string_content` in `url(...)`

That is: (B) is not the wide-radius change the debt feared, **as long as it is a
trim and not the whole `dataText`**. `dataText` also removes pairs of quotes and rejects a
value with a newline or above `maxDataValueLen` — applying that to 361 queries would be the
risky change. `strings.TrimSpace` on its own is a no-op on every identifier node.

### The two changes, independent in purpose

**1. Engine (`treesitter_adapter.go`).** The trim was already being computed and thrown away:

```go
if strings.TrimSpace(name) == "" {   // antes: só testava
```

It is now assigned. That fixes the whole class, including `html.yaml` and `css.yaml`,
and — this is what motivates putting it in the engine instead of in each query — it holds
for a grammar that does not exist yet: a query file can be added by a project or by a
Hub artifact, and it cannot be expected to know about this.

**2. `svelte.yaml`.** The binding gained `parent_capture`, in parity with `vue.yaml`.

> **CORRECTED on 2026-08-05, the same day.** This second change was wrong and was
> reverted — see [Correction](#correction-the-parent_capture-was-wrong) at the end of this
> document. The justification recorded here ("names the element that renders the
> binding, which before was the `File`") is **false**: the ancestor walk via
> `context_types` already resolved the element. What `parent_capture` actually did was
> opt the name into `dataText`, which removes pairs of quotes, turning
> `{ "foo" }` into a reference to the identifier `foo`.

The engine fix (1) is the one that settles the debt, and it is pinned by
`TestPaddedAttributeReferenceIsNormalised`, which fails if the `TrimSpace` is removed.

### The tree was checked, not assumed

The debt warned against assuming symmetry with Vue. Dumping the real tree:

```
expression → "{ title }"
  svelte_raw_text → "title "
```

The grammar **eats the space on the left and keeps the one on the right** — the target was `"title "`,
not `" title "` as the debt supposed. Same defect, different detail.

### shardCacheVersion 2 → 3

Mandatory and nearly forgotten. The cache is keyed by the content hash, so changing what
the parser produces does not move the key: without the bump, the fix would reach only files
edited afterwards, and every already-indexed project would keep the dangling edges while
running the new binary. It is exactly the scenario the comment in `shard_cache.go` describes.

## Use Cases

### UC-01: A captured name is normalized, regardless of what the query declares
- **Actor**: the tree-sitter query executor
- **Preconditions**: a pattern that captures `@name` on a node whose text may have whitespace
- **Main Flow**:
  1. `capture.Node.Utf8Text(src)` returns the raw text
  2. if the query declares `value_capture` or `parent_capture`, `dataText` runs (trim + removal of pairs of quotes)
  3. `strings.TrimSpace` runs **always**, covering the case where (2) did not run
  4. a name that is empty after the trim discards the match
- **Alternative Flows**:
  - identifier node: the trim is a no-op, behaviour identical to before
  - ANTLR queries: already normalized by `dataText` in the ANTLR adapter, they do not come through here
- **Error Scenarios**:
  - a name that was only whitespace is now discarded by the same `continue` as before
- **Postconditions**: no entity or relation target carries whitespace
- **Affected Files**: `internal/ast/treesitter_adapter.go`

### UC-02: The Svelte binding is attributed to the element that renders it
- **Actor**: the query executor
- **Preconditions**: `(expression (svelte_raw_text))` inside an `element` with a `start_tag`
- **Main Flow**: `{ title }` in a `<div>` generates `REFERENCES` to `title`, with the `div` as context
- **Alternative Flows**: `{title}` without spaces, behaviour unchanged as to the target, and it gains the same context
- **Error Scenarios**: a multiline expression or one above the value limit is discarded by `dataText`
- **Postconditions**: the reference is attributed to the `Element`, not to the `File`
- **Affected Files**: `internal/ast/queries/svelte.yaml`

## Test Cases & Acceptance Criteria

### Feature: Normalization of the captured name
Ref: UC-01, UC-02 — `TestSvelteBindingIsNormalised` and `TestPaddedAttributeReferenceIsNormalised` in `internal/ast/css_test.go`

#### Scenario: A binding with spaces resolves to the declaration
```gherkin
Given a Card.svelte with "{ title }" and "{subtitle}" inside a <div class="card">
  And a <script> that declares export let title and export let subtitle
When the file is parsed
Then there is a REFERENCES edge to "title"
  And there is a REFERENCES edge to "subtitle"
  And no reference target differs from its own trimmed form
  And the binding is contained by the Element "div"
```

#### Scenario: An attribute value with spaces resolves, in another grammar
```gherkin
Given a padded.html with id=" main ", class=" card " and href=" /pedidos "
When the file is parsed
Then there are REFERENCES edges to "main", "card" and "/pedidos"
  And no reference target differs from its own trimmed form
```

#### Scenario: The html guard fails without the engine fix
```gherkin
Given the TrimSpace assignment removed from treesitter_adapter.go
When TestPaddedAttributeReferenceIsNormalised runs
Then it fails reporting " main ", " card " and " /pedidos "
  But TestSvelteBindingIsNormalised keeps passing, because the parent_capture already normalizes
```

#### Scenario: A manifest from an earlier version is discarded
```gherkin
Given a shard manifest written with shardCacheVersion 2
When the binary with shardCacheVersion 3 opens the cache
Then the manifest is discarded and every file is reparsed
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/treesitter_adapter.go` | Modified | `strings.TrimSpace` on the captured name, always — fixes the class |
| `internal/ast/queries/svelte.yaml` | Modified | `parent_capture` on the binding: names the element and turns on the normalization |
| `internal/ast/shard_cache.go` | Modified | `shardCacheVersion` 2 → 3, otherwise the fix does not reach an already-indexed project |
| `internal/ast/css_test.go` | Modified | `TestSvelteBindingIsNormalised` and `TestPaddedAttributeReferenceIsNormalised` |

## Trade-offs & Decisions

- **`TrimSpace` in the engine, not `dataText` in the engine.** `dataText` does three more
  things (removes pairs of quotes, rejects newlines, rejects anything above
  `maxDataValueLen`) which across 361 queries would have a real blast radius. The trim is a
  no-op on an identifier, which is what ~140 of them capture.
- **Fix in the engine AND in `svelte.yaml`.** The engine alone would resolve the
  reference's target, but would leave the binding contained by the `File`. The YAML alone
  would resolve Svelte and leave `html.yaml` and `css.yaml` behind. The two assert
  different things.
- **Not touching the 8 `html.yaml` queries nor the 2 in `css.yaml`.** The engine already
  covers them; adding `parent_capture` there would be a behaviour change (the value would
  become a node of its own via `value_label`) with no request and no test to justify it.
- **Bumping `shardCacheVersion` accepting the cost of a reparse.** The alternative was not
  bumping and letting the fix arrive through file edits, which means the wrong graph
  running the right binary — the failure mode the field's own comment describes.

## Correction: the `parent_capture` was wrong

Recorded the same day, while closing the debts below. The two justifications for
adding `parent_capture` to the Svelte binding (and to the Vue interpolation, in the
previous task) were measured and **neither one held up**:

**Containment.** Measured with and without `parent_capture`, on the same fixture: `SourceName` of
`ReferenceInfo` is `"div"` in both cases. `context_types` + `context_name_paths` already
resolved the element through the ancestor walk — which is the normal mechanism, used by
every other grammar. In Vue, the same: `div` for the interpolation inside the element and
`template` for the one in the `template_element`.

**Normalization.** Covered by the unconditional trim in the engine, which is change (1). The
`parent_capture` reached the same result by a wider path.

And it charged a price I had not seen: `parent_capture` turns on `isData`, and `dataText`
does more than trim — **it removes pairs of quotes**. So `{ "foo" }` stopped producing
the inert target `"foo"` and started producing a reference to the identifier `foo`. That is
not noise: an edge to `foo` is a **false dependency**, and `foo` may perfectly well
exist, declared somewhere else, for no related reason at all. Confirmed in both
languages before reverting.

The three queries went back to the form without `parent_capture`, now with a comment saying why
they do **not** declare one — which is the information that was missing and the reason this was
added by mistake. Pinned by `TestQuotedBindingIsNotAnIdentifierReference`,
verified that it fails if the `parent_capture` is reintroduced.

The lesson, and it is the one worth keeping: `parent_capture` in a `type: relation` query is not a way
to "declare containment" — the containment already comes from `context_types`. It is a
selector of data semantics, and on a reference target that semantics is wrong.

## Technical Debt

- [x] **`svelte.yaml` with a non-normalized name** — resolved by the trim in the engine.
- [x] **`css.yaml` `var( --brand )` and `url( x )` without a test of their own** —
  `TestCSSPaddedFunctionReferencesAreNormalised` covers both, with whitespace inside
  the parentheses and inside the quotes of `url()`.
- [x] **Nothing stops a new query from capturing free text without declaring context** —
  closed, and the answer turned out to be that the premise was inverted. The original risk
  (a non-normalized target) is closed **in the engine**, so a new query that captures
  free text already comes out trimmed without having to declare anything. And the lint I had
  proposed — failing when a relation query captures a text node without `parent_capture` — would
  have pushed in exactly the wrong direction: declaring `parent_capture` there is the defect, not
  the fix. An exact guard in place of the heuristic:
  `TestQuotedBindingIsNotAnIdentifierReference`.

## System Knowledge

- **The ANTLR adapter normalizes unconditionally; tree-sitter did not normalize.**
  `antlr_adapter.go` passes every target through `dataText(unquoteIdentifier(...))`. It was the only
  asymmetry between the two adapters at that point, and the reason the 212 ANTLR paths
  in the survey were false positives.
- **`{ title }` in tree-sitter-svelte captures `"title "`** — the space on the left eaten
  by the grammar, the one on the right kept. Do not assume whitespace symmetry at both
  ends.
- **Audit a query file through the loader, not through grep.** `parseQueryFile` returns
  `[]ExternalQueryDef` with `Type`, `ValueCapture` and `ParentCapture` resolved; a grep
  over YAML does not distinguish a relation query from an entity query nor see the default of
  `Type`. The survey of the 361 came out of a throwaway test using the loader.
- **A change in what the parser produces requires bumping `shardCacheVersion`**, because the cache is
  keyed by the content hash and not by the conversion logic. Second case in a row
  (the first was imports becoming entities, version 2).

## Progress Log

### 2026-08-05
- Memory and the debt recorded in the Vue log consulted before touching code.
- Survey of the 361 relation queries without normalization, with the project's loader,
  grouped by the type of node captured — which reduced the risk set from "361
  unknowns" to 11 nameable ones and showed that the ANTLR adapter was already covered.
- The Svelte tree dumped before writing the pattern: the target was `"title "`, not
  `" title "`.
- Test written first and confirmed failing; then the two fixes; then
  confirmed that the `html` guard fails with the engine reverted and the Svelte one does not.
- All of `internal/ast` green with `-tags fts5`, plus `internal/hub`,
  `internal/daemon` and `cmd/graphit/commands`; build and vet clean.
- `shardCacheVersion` to 3 while reviewing the impact on an already-indexed project.
- Verified on the live graph: a scratch project with `.svelte` and `.html` indexed
  (`tree-sitter:svelte — 1`, `tree-sitter:html — 1`), targets returned `[title]`,
  `[subtitle]`, `[main]`, `[card]`, none with a space, the binding contained by the `div`.
