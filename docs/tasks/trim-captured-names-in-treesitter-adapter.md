---
title: Nome capturado passa a ser trimado no adaptador tree-sitter, e o binding do Svelte ganha parent_capture
status: done
created: 2026-08-05
updated: 2026-08-05
tags: [ast, treesitter, queries, references, svelte, html]
---

# Nome capturado passa a ser trimado no adaptador tree-sitter, e o binding do Svelte ganha parent_capture

## Objective

Resolve the registered debt in [add-vue-treesitter-grammar.md]:
The _INLINE_0_ produced an area with the source spacing, then one
The written binding `{ title }` — syntax so valid as `{title}` — pointed to a
Name that has none, and nothing reports.

The investigation revealed that it was not a defect of Svelte. It's a class, and Svelte only.
It was obvious because the Vue grammar forced the subject: `{{ title }}` cannot
written without spaces

## Implementation Details

What did the survey reveal, and how did it alter the decision?

The debit recommended two routes: (A) correct only Inline 0, and (B) evaluate
normalizar no engine, com a ressalva de que (B) tinha raio de alcance grande e
Unknown - "Nobody knows today."

The survey was conducted using the project's own __INLINE_0 loader, not by

Translation: The investigation was carried out with the project's own __INLINE_0 loader, not through.
text marriage listing all queries `type: relation` that do not declare
Here is the translation:

Neither of these. They are **361**. Grouped by the type of node that

This translation maintains the structure and meaning of the original Portuguese sentence while rendering it in idiomatic English.
capturam:

Number: Captured
|---|---|
| 212 | caminho ANTLR (`//callStatement/literal` etc.) |
| 66 | `identifier` |
| 17 | `type_identifier` |
| 10 | `field_identifier` |
| 9 | `property_identifier` |
| 9 | `name` |
| 8 | `attribute_value` |
| ... | `simple_identifier`, `word`, `sym_lit`, `constant`, ... |
| 1 | `svelte_raw_text` |

This reverses the reading of risk. The 212 ANTLR paths are already normalized—‌
adaptador ANTLR passa tudo por `dataText` incondicionalmente
(`antlr_adapter.go`, `dataText(unquoteIdentifier(target.FullText()))`). E os ~140
The remaining capture **tokens** where spacing around them is impossible: trimar
It is an NOP. The truly affected set is small and nameable:

"**INLINE_0** is **INLINE_1** in the binding (the case of debit)"
- `html.yaml` — `attribute_value` nas 8 queries de `id` / `class` / `href` / `src` /
  `action` / `name` / `for` / `role`
- `css.yaml` — `plain_value` em `var(...)` e `string_content` em `url(...)`

In other words: (B) is not the size of the change that debit feared, **as long as it's one.
Trim and not **INLINE_0**, entirely **INLINE_1** also removes pairs of quotes and rejects
valor com newline ou acima de `maxDataValueLen` — aplicar isso a 361 queries seria a
Risk-taking change. Inline 0 is an identity node operation in all identifiers alone.

The two changes, regardless of intent

Engine (INLINE_0). The trim was already calculated and thrown out:

```go
If `strings.TrimSpace(name)` is an empty string, then...
```

It has been assigned. Correct the entire class, including Inline 0 and Inline 1,
And—what motivates putting it in the engine instead of on each query—is applicable to grammar as well.
yet does not exist: an query file can be added by a project or by a user
Artifact of the Hub, and one cannot expect him to know about it.

**2. `svelte.yaml`.** O binding ganhou `parent_capture`, em paridade com `vue.yaml`.

Corrected on August 5, 2026, in the same day. This second change was wrong and was corrected.
Reversed - see [Correction](#correction-the-parent-capture-was-incorrect) at the end.
> documento. A justificativa registrada aqui ("nomeia o elemento que renderiza o
binding, which was previously the **INLINE_0**", is **false**: the ancestral migration route.
The `INLINE_0` already resolved the element. What did `INLINE_1` actually do?
> optar o nome para o `dataText`, que remove pares de aspas, transformando
On reference to identifier `foo`.

The correction in the engine (1) resolves the debit, and is plugged by
`TestPaddedAttributeReferenceIsNormalised`, que falha se o `TrimSpace` for removido.

The tree was appraised, not presumed.

The debit warned against assuming symmetry with Vue. Dumping the real tree:

```
expression → "{ title }"
  svelte_raw_text → "title "
```

Grammar eats from the left and holds onto the right—The target was INLINE_0__.
not as the debit had supposed. Even defect, different detail.

### shardCacheVersion 2 → 3

Required and almost forgotten. The cache is keyed by the content's hash, so changing it would require recalculating the entire contents.
The parser produces no change in the key: without the bump, correction would only affect files
Edited afterwards, and all projects would follow with hanging edges running.
The new binary is exactly as described in `shard_cache.go`.

## Use Cases

UC-01: The name captured is normalized regardless of what the query declares.
- **Actor**: o executor de queries tree-sitter
Preconditions: a pattern that captures INLINE_0 in a node whose text can have spacing
- **Main Flow**:
  1. `capture.Node.Utf8Text(src)` devolve o texto cru
Here's the translation:

"If the query declares `value_capture` or `parent_capture`, `dataText` runs (trim + removal of quote pairs)."
The code runs **always** and covers the case where (2) does not run.
  4. nome vazio depois do trim descarta o match
- **Alternative Flows**:
Identifier Tag: The trim method is an identity operation, behaving exactly as before.
Brazilian Portuguese:
- Queries ANTLR: already normalized by `dataText` in the ANTLR adapter, do not proceed here.
- **Error Scenarios**:
The name that used to be just space is now discarded by the same _INLINE_0_.
Postconditions: No entity or relationship target carries spacing.
- **Affected Files**: `internal/ast/treesitter_adapter.go`

The binding of Svelte is attributed to the element that renders it.
- **Actor**: o executor de queries
- **Preconditions**: `(expression (svelte_raw_text))` dentro de um `element` com `start_tag`
- **Main Flow**: `{ title }` num `<div>` gera `REFERENCES` para `title`, com o `div` como contexto
Alternative Flows: INLINE 0 without spaces, behavior unchanged as target, and retains the same context
Error Scenarios: The expression is discarded if it exceeds the value limit or spans multiple lines by default.
Postconditions: The reference is assigned to INLINE_0, not INLINE_1.
- **Affected Files**: `internal/ast/queries/svelte.yaml`

## Test Cases & Acceptance Criteria

Feature: Captured Name Normalization
Ref: UC-01, UC-02 — `TestSvelteBindingIsNormalised` e `TestPaddedAttributeReferenceIsNormalised` em `internal/ast/css_test.go`

Scenario: Spacing Binding Resolves Declaration
```gherkin
Given um Card.svelte com "{ title }" e "{subtitle}" dentro de um <div class="card">
  And um <script> que declara export let title e export let subtitle
When the file is parsed
Then existe aresta REFERENCES para "title"
  And existe aresta REFERENCES para "subtitle"
And none of its reference targets differs from itself trimmed
And it is contained within the Element "div"
```

Scenario: Attribute Value with Spaces Resolved, in Another Grammar
```gherkin
Given um padded.html com id=" main ", class=" card " e href=" /pedidos "
When the file is parsed
Then existem arestas REFERENCES para "main", "card" e "/pedidos"
And none of its reference targets differs from itself trimmed
```

The HTML guardian fails without correction in the engine.
```gherkin
Given that the TrimSpace attribute has been removed from treesitter_adapter.go
When TestPaddedAttributeReferenceIsNormalised roda
Then ele falha reportando " main ", " card " e " /pedidos "
But TestSvelteBindingIsNormalised continues passing because the parent_capture already normalizes.
```

Scenario: The previous version is discarded
```gherkin
Given um manifesto de shard escrito com shardCacheVersion 2
When the binary with shardCacheVersion 3 opens the cache
The manifesto is discarded, and all files are reparsed.
```

## Files Changed

| File | Change | Reason |
|---|---|---|
| `internal/ast/treesitter_adapter.go` | Modified | `strings.TrimSpace` no nome capturado, sempre — corrige a classe |
Here is the translation:

"_`internal/ast/queries/svelte.yaml`_ | Modified | _`parent_capture`_ not bound: names the element and links to normalization"

This translation maintains the structure of the original Portuguese text while rendering it in idiomatic English. The placeholders "`internal/ast/queries/svelte.yaml`" and "`parent_capture`" are kept as they are, likely referring to specific identifiers or variables within a code snippet.
Here is the translation:

"_`internal/ast/shard_cache.go`_ | Modified | _`shardCacheVersion`_ → 3, otherwise the correction does not reach already indexed project"

This translation maintains the structure and meaning of the original Portuguese text while making it more idiomatic in English.
| `internal/ast/css_test.go` | Modified | `TestSvelteBindingIsNormalised` e `TestPaddedAttributeReferenceIsNormalised` |

## Trade-offs & Decisions

In the engine, `TrimSpace` is not present, but `dataText` is in the engine. `dataText` does three more things.
  (remove pares de quotes, rejeita newline, rejeita acima de `maxDataValueLen`) que em
There would be 361 actual reach radius queries. The trim is an identity operation, which is what
  ~140 delas capturam.
Correct in Engine and Inline 0. Only the engine will hit the target.
Reference, but would leave the binding contained by `File`. Only YAML resolves Svelte
  e deixaria `html.yaml` e `css.yaml`. As duas afirmam coisas diferentes.
Do not touch the 8 queries for `html.yaml` and the two for `css.yaml`. The engine already handles them.
It covers; adding `parent_capture` there would be a behavior change (the value would pass through).
To untangle it on its own using `value_label`, without request or test that validates it.
The inline 0 bump is accepting the cost of reparsing. The alternative was not.
To correct mistakes automatically, which means correcting wrong graphs
Running binary correctly — the mode of failure that the very field comment describes.

Correction: The `parent_capture` was incorrect.

Signed on the same day as settling below the debits. The two justifications for
Add `parent_capture` to the binding in Svelte and to the interpolation in Vue for the task
anterior) foram medidas e **nenhuma se sustentou**:

Containment. Measured with and without `parent_capture`, in the same fixture: `SourceName` of
INLINE 0 is INLINE 1 in both cases. INLINE 2 + INLINE 3 already
They resolved the element by following ancestral footsteps -- which is the normal mechanism used by
All other grammars. In Vue, it's also: **`<slot>`** for interpolation within the element.
`template` para a do `template_element`.

Normalization. Covered by the unconditional trim on the engine, which is change (1).
`parent_capture` chegava ao mesmo resultado por um caminho mais largo.

And she charged a price I had never seen: _INLINE_0_ connects _INLINE_1_, and _INLINE_2_.
It does more than trim—**it removes quotes**. Then `{ "foo" }` stopped producing
The inert target `"foo"` started producing a reference to the identifier `foo`. This is not
It's noise: an "inline" for `foo` is a **false dependency**, and `foo` can perfectly
Existence declared in another place for no reason whatsoever is confirmed in both.
linguagens antes de reverter.

The three queries returned to their original form without INLINE_0, now with comments saying by
that **does not** declare one — that is the information missing and the reason for why it was
adicionado por engano. Pinado por `TestQuotedBindingIsNotAnIdentifierReference`,
verificado que ele falha se o `parent_capture` for reintroduzido.

Lesson, and that's the one you should keep: **inline** queries in a table are not a good idea.
From "declare containment" — the containment has already been defined in INLINE_0_. It is a selector for
Semantics of data, and in a reference point for this semantics is incorrect.

## Technical Debt

- Inline 0 with normalized name resolved by trim in the engine.
- [ ] Inline 0, 1, and 2 without their own test case.
The ``TestCSSPaddedFunctionReferencesAreNormalised`` covers both with spacing within.
Of the parentheses and within the quotes of `url()`.
- [x] **Nada impede uma query nova de capturar texto livre sem declarar contexto** —
  fechado, e a resposta acabou sendo que a premissa estava invertida. O risco original
The target is not normalized; it's closed **in the engine**, so a new query that captures
The text is already trimmed, and no declaration was necessary. And the lint I had proposed
— fail when a relation query captures no text without `parent_capture` - would
Pushed exactly in the wrong direction: declaring INLINE_0__ there is the flaw, not
Correction. Store exactly in place of heuristic:
  `TestQuotedBindingIsNotAnIdentifierReference`.

## System Knowledge

The adapter ANTLR normalizes conditionally, whereas Tree-Sitter did not normalize at all.
The `antlr_adapter.go` passes all targets through `dataText(unquoteIdentifier(...))`. It was the only one.
Asymmetry between the two adapters at this point, and the reason for the 212 paths
  ANTLR do levantamento eram falso positivo.
The left margin is eaten by `{ title }`.
For grammar, the right one is maintained. Don't presume spacing symmetry in both.
  pontas.
Audit the query file through the loader, not by using grep. INLINE_0 returns.
  `[]ExternalQueryDef` com `Type`, `ValueCapture` e `ParentCapture` resolvidos; um grep
In YAML, it does not differentiate between query and relation of query and entity nor sees the default of
The 361 was lifted from a test that was discarded using the loader.
The change in what the parser produces requires a bump of `shardCacheVersion`, because the cache is
Keyed by the content's hash and not by the conversion logic. Second case follows.
The first was imports turning into an entity, version 2.

## Progress Log

### 2026-08-05
Checked the registered memory and debit in the Vue log before touching code.
Data Extraction from 361 Non-Normalized Relationships Queries without Normalization Loader Project Output
grouped by captured node type — which reduced the risk of "361"
Unknowns were shown as part of 11 notable cases, and it was demonstrated that the ANTLR adapter is already covered.
The "Svelte" tree dumped before writing the standard: target was `"title "`, not
  `" title "`.
First, write and test the written part with errors; then correct both mistakes; finally, retest.
Confirmed that the guard for INLINE_0 fails with the engine reverted and the Svelte one.
- `internal/ast` inteiro verde com `-tags fts5`, mais `internal/hub`,
  `internal/daemon` e `cmd/graphit/commands`; build e vet limpos.
For project already indexed, inline revision impacts assessment for 3.
- Verificado no grafo vivo: projeto scratch com `.svelte` e `.html` indexado
  (`tree-sitter:svelte — 1`, `tree-sitter:html — 1`), alvos devolvidos `[title]`,
Inline 0, Inline 1, Inline 2, no space, contained by `div`.
